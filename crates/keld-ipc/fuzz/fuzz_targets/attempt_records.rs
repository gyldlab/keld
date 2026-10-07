//! `keld-attempt` claim and health record fuzz target (KEL-53 §4 "Candidate
//! connect-back" *Messages* and *Health sequence*; §6 S4b; §7 "8
//! (keld-attempt codec)").
//!
//! Property set, at every read position: decoding arbitrary bytes terminates
//! without panic and either refuses with a classified `KELD-IPC-*` error or
//! admits a record whose canonical encoding is the whole input; the stream
//! reader agrees with the slice decoder, consumes exactly one encoded record
//! and never reads past a refused magic. The locator check is Windows-only and
//! is covered by the deterministic tests.

#![no_main]

use std::io::Cursor;

use keld_ipc::attempt::{AttemptReadPosition, AttemptRecord, AttemptRecordError};

const POSITIONS: [AttemptReadPosition; 7] = [
    AttemptReadPosition::OwnerClaim,
    AttemptReadPosition::OwnerAcknowledgement,
    AttemptReadPosition::OwnerBoot,
    AttemptReadPosition::OwnerReady,
    AttemptReadPosition::CandidateChallenge,
    AttemptReadPosition::CandidateReceipt,
    AttemptReadPosition::CandidateHealthResult,
];

fn encode(record: &AttemptRecord) -> Vec<u8> {
    let mut bytes = Vec::new();
    record.write_to(&mut bytes).expect("writing to a Vec");
    bytes
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    for position in POSITIONS {
        let decoded = AttemptRecord::decode(position, data);
        let mut stream = Cursor::new(data);
        let streamed = AttemptRecord::read_from(&mut stream, position);
        let consumed = usize::try_from(stream.position()).expect("cursor within input");
        match (decoded, streamed) {
            (Ok(record), Ok(read)) => {
                assert_eq!(read, record, "stream and slice decoders disagree");
                assert_eq!(encode(&record), data, "admitted bytes are not canonical");
                assert_eq!(consumed, data.len());
            }
            (Ok(record), Err(error)) => panic!("slice admitted {record:?}, stream refused: {error}"),
            (Err(error), Ok(read)) => {
                // Only a whole record followed by more bytes reads from a stream.
                assert!(
                    matches!(error, AttemptRecordError::TrailingBytes { .. }),
                    "stream admitted {read:?}, slice refused: {error}"
                );
                let encoded = encode(&read);
                assert_eq!(consumed, encoded.len());
                assert!(data.len() > encoded.len() && data.starts_with(&encoded));
            }
            (Err(decode_error), Err(read_error)) => {
                for error in [&decode_error, &read_error] {
                    let text = error.to_string();
                    assert!(text.starts_with("KELD-IPC-01"), "unclassified: {text}");
                }
                if matches!(read_error, AttemptRecordError::MagicNotAdmitted { .. }) {
                    assert_eq!(consumed, 8, "read past a refused magic");
                }
            }
        }
    }
});
