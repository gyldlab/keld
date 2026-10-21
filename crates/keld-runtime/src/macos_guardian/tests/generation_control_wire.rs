//! KGC1 generation-control wire codec, split-reader and admission contract tests.
//! Test-specific input readers live beside their only contract consumers.

use std::io::{self, Read};
use std::time::{Duration, Instant};

use super::super::{
    CONTROL_PREPARED, CONTROL_REGISTERED, CONTROL_REVOKE, CONTROL_SPAWNED,
    GENERATION_CONTROL_RECORD_LEN, GUARDIAN_SHUTDOWN_DEADLINE, GenerationControlReader,
    GenerationControlRecord, RevocationCause, decode_revocation_cause, read_generation_control,
    validate_control_response, write_generation_control,
};

#[test]
fn generation_control_codec_is_fixed_bounded_and_rejects_nonzero_padding() {
    let record = GenerationControlRecord::new(CONTROL_PREPARED, 7)
        .with_pid(41)
        .with_payload(b"socket#token".to_vec());
    let mut bytes = Vec::new();
    write_generation_control(&mut bytes, &record).expect("encode KGC1 record");
    assert_eq!(bytes.len(), GENERATION_CONTROL_RECORD_LEN);
    assert_eq!(&bytes[..4], b"KGC1");
    assert_eq!(bytes[4], 1);
    assert_eq!(bytes[5], CONTROL_PREPARED);
    assert_eq!(&bytes[8..12], &7_u32.to_be_bytes());
    assert_eq!(&bytes[12..16], &41_u32.to_be_bytes());
    assert_eq!(&bytes[16..18], &12_u16.to_be_bytes());
    let decoded = read_generation_control(&mut bytes.as_slice()).expect("decode KGC1 record");
    assert_eq!(decoded.kind, CONTROL_PREPARED);
    assert_eq!(decoded.attempt, 7);
    assert_eq!(decoded.pid, 41);
    assert_eq!(decoded.payload, b"socket#token");

    bytes[GENERATION_CONTROL_RECORD_LEN - 1] = 1;
    let error = read_generation_control(&mut bytes.as_slice())
        .expect_err("nonzero private-record padding must fail");
    assert!(error.to_string().contains("padding"), "{error}");
}

#[test]
fn generation_control_reader_retains_split_bytes_across_would_block() {
    let record =
        GenerationControlRecord::new(CONTROL_REVOKE, 9).with_cause(RevocationCause::ChildExited);
    let mut encoded = Vec::new();
    write_generation_control(&mut encoded, &record).expect("encode split KGC1 record");
    let mut source = SplitControlReader {
        bytes: encoded,
        offset: 0,
        block_next: false,
    };
    let mut reader = GenerationControlReader::new();
    let decoded = loop {
        if let Some(decoded) = reader.poll(&mut source).expect("poll split control") {
            break decoded;
        }
    };
    assert_eq!(decoded.kind, CONTROL_REVOKE);
    assert_eq!(decoded.attempt, 9);
    assert_eq!(
        decode_revocation_cause(decoded.cause).expect("decode split cause"),
        RevocationCause::ChildExited
    );
}

#[test]
fn generation_control_partial_record_has_one_overall_deadline() {
    for source in [&mut AlwaysWouldBlock as &mut dyn Read, &mut OneByteAtATime] {
        let mut reader = GenerationControlReader::new();
        reader.filled = 1;
        reader.started = Instant::now().checked_sub(GUARDIAN_SHUTDOWN_DEADLINE);
        let error = reader
            .poll(source)
            .expect_err("expired partial record must fail");
        assert!(error.to_string().contains("read deadline"), "{error}");
    }
}

#[test]
fn subsequent_kgc1_record_cannot_complete_on_an_expired_final_byte() {
    // A previously started, incomplete KGC1 record has a single shared
    // deadline even if the final byte happens to be ready immediately.
    let record = GenerationControlRecord::new(CONTROL_REVOKE, 11);
    let mut bytes = Vec::new();
    write_generation_control(&mut bytes, &record).expect("exact full control record");
    let mut prefix = SplitControlReader {
        bytes: bytes[..GENERATION_CONTROL_RECORD_LEN - 1].to_vec(),
        offset: 0,
        block_next: false,
    };
    let mut decoder = GenerationControlReader::new();
    while prefix.offset < prefix.bytes.len() {
        assert!(
            decoder
                .poll(&mut prefix)
                .expect("incomplete native-sized KGC1 record")
                .is_none(),
            "partial KGC1 must not publish any transition"
        );
    }
    assert_eq!(decoder.filled, GENERATION_CONTROL_RECORD_LEN - 1);
    decoder.started = Instant::now().checked_sub(GUARDIAN_SHUTDOWN_DEADLINE);
    let mut final_byte = &bytes[GENERATION_CONTROL_RECORD_LEN - 1..];
    let refusal = decoder
        .poll(&mut final_byte)
        .expect_err("expired final KGC1 byte must not authorize a transition");
    assert!(refusal.to_string().contains("read deadline"), "{refusal}");
}

#[test]
fn first_kgc1_deadline_stops_continuous_successful_one_byte_reads() {
    // Unlike a scheduler-driven socket drip, this reader never yields
    // WouldBlock. Without an absolute deadline inside the reader's loop, it
    // would read every byte and attempt to decode a malformed 404-byte frame.
    let mut source = AlwaysReadySlowByte { reads: 0 };
    let mut reader = GenerationControlReader::new();
    let began = Instant::now();
    let error = reader
        .poll_until(&mut source, Some(began + Duration::from_millis(15)))
        .expect_err("constant-success byte drip must observe first-record deadline");
    assert!(error.to_string().contains("first KGC1 record"), "{error}");
    assert!(
        source.reads > 0,
        "the reader must actually consume hostile bytes"
    );
    assert!(
        source.reads < GENERATION_CONTROL_RECORD_LEN,
        "deadline enforcement cannot wait for the entire malformed frame"
    );
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "single poll was not promptly interrupted at the original deadline"
    );
}

#[test]
fn initial_kgc1_reader_rejects_complete_frame_arriving_after_deadline() {
    let mut encoded = Vec::new();
    write_generation_control(
        &mut encoded,
        &GenerationControlRecord::new(CONTROL_REVOKE, 1),
    )
    .expect("encode full private record");
    let mut source = DelayedCompleteFrame { encoded };
    let deadline = Instant::now() + Duration::from_millis(10);
    let error = GenerationControlReader::new()
        .poll_until(&mut source, Some(deadline))
        .expect_err("a fully buffered frame cannot become valid after its original deadline");
    assert!(error.to_string().contains("first KGC1 record"), "{error}");
}

#[test]
fn generation_control_response_fields_are_transition_exact() {
    let spawned = GenerationControlRecord::new(CONTROL_SPAWNED, 3).with_pid(91);
    let registered = GenerationControlRecord::new(CONTROL_REGISTERED, 3).with_pid(91);
    validate_control_response(&spawned, &registered).expect("exact registered response");

    for malformed in [
        GenerationControlRecord::new(CONTROL_REGISTERED, 3).with_pid(92),
        GenerationControlRecord::new(CONTROL_REGISTERED, 3)
            .with_pid(91)
            .with_payload(b"unexpected".to_vec()),
        GenerationControlRecord {
            cause: 1,
            ..GenerationControlRecord::new(CONTROL_REGISTERED, 3).with_pid(91)
        },
    ] {
        let error = validate_control_response(&spawned, &malformed)
            .expect_err("malformed authenticated response must fail");
        assert!(error.to_string().contains("response fields"), "{error}");
    }
}

struct AlwaysWouldBlock;

impl Read for AlwaysWouldBlock {
    fn read(&mut self, _output: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::WouldBlock))
    }
}

struct OneByteAtATime;

impl Read for OneByteAtATime {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        output[0] = 0;
        Ok(1)
    }
}

struct AlwaysReadySlowByte {
    reads: usize,
}

impl Read for AlwaysReadySlowByte {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(Duration::from_millis(1));
        output[0] = b'K';
        self.reads += 1;
        Ok(1)
    }
}

struct DelayedCompleteFrame {
    encoded: Vec<u8>,
}

impl Read for DelayedCompleteFrame {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(Duration::from_millis(25));
        output[..self.encoded.len()].copy_from_slice(&self.encoded);
        Ok(self.encoded.len())
    }
}

struct SplitControlReader {
    bytes: Vec<u8>,
    offset: usize,
    block_next: bool,
}

impl Read for SplitControlReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.block_next {
            self.block_next = false;
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        if self.offset == self.bytes.len() {
            return Ok(0);
        }
        self.block_next = true;
        let length = output.len().min(17).min(self.bytes.len() - self.offset);
        output[..length].copy_from_slice(&self.bytes[self.offset..self.offset + length]);
        self.offset += length;
        Ok(length)
    }
}
