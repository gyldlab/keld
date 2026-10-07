//! `keld-attempt` claim and health record bytes (KEL-53 §4 "Candidate
//! connect-back": the *Messages* table, *Transcript* and *Health sequence*;
//! §7 "8 (keld-attempt codec)").
//!
//! Oracles: golden records assembled here from the spec table's literal
//! segments (8-byte magic, one fill byte per 32-byte field, little-endian PIDs
//! written byte by byte), the spec's admission table written out literally,
//! the spec's purpose-`1` locator vector, and a reader that records every read
//! past the magic. No expected byte or admission comes from the code under
//! test.

use std::io::{self, Cursor, Read};

use super::{
    AttemptBootAcknowledgement, AttemptChallenge, AttemptClaim, AttemptFailureClass,
    AttemptHealthResult, AttemptReadPosition, AttemptRecord, AttemptRecordError, AttemptRecordKind,
    AttemptTranscript,
};
use crate::token::SessionToken;

const INSTALLATION: [u8; 32] = [0x11; 32];
const ATTEMPT: [u8; 32] = [0x22; 32];
const CHANNEL: [u8; 32] = [0x33; 32];
const CLIENT_NONCE: [u8; 32] = [0x44; 32];
const SERVER_NONCE: [u8; 32] = [0x55; 32];
const DIGEST: [u8; 32] = [0x66; 32];
const CLIENT_PID: u32 = 0x0A0B_0C0D;
const CLIENT_PID_LE: [u8; 4] = [0x0D, 0x0C, 0x0B, 0x0A];
const SERVER_PID: u32 = 0x0102_0304;
const SERVER_PID_LE: [u8; 4] = [0x04, 0x03, 0x02, 0x01];

use AttemptReadPosition::{
    CandidateChallenge, CandidateHealthResult, CandidateReceipt, OwnerAcknowledgement, OwnerBoot,
    OwnerClaim, OwnerReady,
};

const POSITIONS: [AttemptReadPosition; 7] = [
    OwnerClaim,
    OwnerAcknowledgement,
    OwnerBoot,
    OwnerReady,
    CandidateChallenge,
    CandidateReceipt,
    CandidateHealthResult,
];

/// KEL-53 §4 *Health sequence*, written out: the magics each position admits.
const ADMITTED: [(AttemptReadPosition, &[&[u8; 8]]); 7] = [
    (OwnerClaim, &[b"KELD-AH1"]),
    (OwnerAcknowledgement, &[b"KELD-AA1"]),
    (OwnerBoot, &[b"KELD-AB1", b"KELD-AF1"]),
    (OwnerReady, &[b"KELD-AY1", b"KELD-AF1"]),
    (CandidateChallenge, &[b"KELD-AC1"]),
    (CandidateReceipt, &[b"KELD-AR1"]),
    (CandidateHealthResult, &[b"KELD-AK1"]),
];

/// Field byte ranges of `KELD-AA1`/`KELD-AR1` from the spec table.
const TRANSCRIPT_FIELDS: [(usize, usize); 7] = [
    (8, 40),
    (40, 72),
    (72, 104),
    (104, 136),
    (136, 168),
    (168, 172),
    (172, 176),
];

fn golden_claim() -> Vec<u8> {
    [
        b"KELD-AH1".as_slice(),
        &INSTALLATION,
        &CLIENT_NONCE,
        &CLIENT_PID_LE,
    ]
    .concat()
}

fn golden_challenge() -> Vec<u8> {
    [
        b"KELD-AC1".as_slice(),
        &ATTEMPT,
        &CHANNEL,
        &SERVER_NONCE,
        &SERVER_PID_LE,
    ]
    .concat()
}

fn golden_transcript(magic: [u8; 8]) -> Vec<u8> {
    [
        magic.as_slice(),
        &INSTALLATION,
        &ATTEMPT,
        &CHANNEL,
        &CLIENT_NONCE,
        &SERVER_NONCE,
        &CLIENT_PID_LE,
        &SERVER_PID_LE,
    ]
    .concat()
}

fn golden_boot() -> Vec<u8> {
    [b"KELD-AB1".as_slice(), &ATTEMPT, &CHANNEL, &DIGEST].concat()
}

fn golden_failure(class: u8) -> Vec<u8> {
    [b"KELD-AF1".as_slice(), &[class]].concat()
}

fn golden_health(result: u8) -> Vec<u8> {
    [b"KELD-AK1".as_slice(), &[result]].concat()
}

fn claim() -> AttemptClaim {
    AttemptClaim::new(
        INSTALLATION,
        SessionToken::from_bytes(CLIENT_NONCE),
        CLIENT_PID,
    )
}

fn challenge() -> AttemptChallenge {
    AttemptChallenge::new(
        ATTEMPT,
        CHANNEL,
        SessionToken::from_bytes(SERVER_NONCE),
        SERVER_PID,
    )
}

fn transcript() -> AttemptTranscript {
    AttemptTranscript::for_owner(&claim(), &INSTALLATION, CLIENT_PID, &challenge())
        .expect("the owner admits its own installation and the connected client")
}

fn boot() -> AttemptBootAcknowledgement {
    AttemptBootAcknowledgement::new(ATTEMPT, CHANNEL, DIGEST)
}

fn encode(record: &AttemptRecord) -> Vec<u8> {
    let mut bytes = Vec::new();
    record.write_to(&mut bytes).expect("writing to a Vec");
    bytes
}

/// Every record with its golden bytes, the spec table's size and one position
/// that admits it.
fn goldens() -> Vec<(AttemptRecord, Vec<u8>, usize, AttemptReadPosition)> {
    use AttemptFailureClass::{ApplicationExitBeforeReady, BootError, BootstrapReadRefused};
    use AttemptHealthResult::{Accepted, RolledBack};
    vec![
        (
            AttemptRecord::Claim(claim()),
            golden_claim(),
            76,
            OwnerClaim,
        ),
        (
            AttemptRecord::Challenge(challenge()),
            golden_challenge(),
            108,
            CandidateChallenge,
        ),
        (
            AttemptRecord::Acknowledgement(transcript()),
            golden_transcript(*b"KELD-AA1"),
            176,
            OwnerAcknowledgement,
        ),
        (
            AttemptRecord::Receipt(transcript()),
            golden_transcript(*b"KELD-AR1"),
            176,
            CandidateReceipt,
        ),
        (
            AttemptRecord::BootAcknowledgement(boot()),
            golden_boot(),
            104,
            OwnerBoot,
        ),
        (AttemptRecord::Ready, b"KELD-AY1".to_vec(), 8, OwnerReady),
        (
            AttemptRecord::Failure(BootstrapReadRefused),
            golden_failure(1),
            9,
            OwnerBoot,
        ),
        (
            AttemptRecord::Failure(ApplicationExitBeforeReady),
            golden_failure(2),
            9,
            OwnerReady,
        ),
        (
            AttemptRecord::Failure(BootError),
            golden_failure(3),
            9,
            OwnerBoot,
        ),
        (
            AttemptRecord::Failure(BootError),
            golden_failure(3),
            9,
            OwnerReady,
        ),
        (
            AttemptRecord::HealthResult(Accepted),
            golden_health(1),
            9,
            CandidateHealthResult,
        ),
        (
            AttemptRecord::HealthResult(RolledBack),
            golden_health(2),
            9,
            CandidateHealthResult,
        ),
    ]
}

/// A peer that sends `prefix` and then nothing: a further read records itself
/// and reports the silence as the deadline would.
struct ThenSilence {
    prefix: Vec<u8>,
    served: usize,
    reads_past_prefix: usize,
}

impl ThenSilence {
    fn new(prefix: &[u8]) -> Self {
        Self {
            prefix: prefix.to_vec(),
            served: 0,
            reads_past_prefix: 0,
        }
    }
}

impl Read for ThenSilence {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = &self.prefix[self.served..];
        if remaining.is_empty() {
            self.reads_past_prefix += 1;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "silent peer: deadline elapsed",
            ));
        }
        let count = remaining.len().min(buf.len());
        buf[..count].copy_from_slice(&remaining[..count]);
        self.served += count;
        Ok(count)
    }
}

#[test]
fn every_record_has_its_spec_magic_and_size() {
    for (kind, magic, size) in [
        (AttemptRecordKind::Claim, b"KELD-AH1", 76),
        (AttemptRecordKind::Challenge, b"KELD-AC1", 108),
        (AttemptRecordKind::Acknowledgement, b"KELD-AA1", 176),
        (AttemptRecordKind::Receipt, b"KELD-AR1", 176),
        (AttemptRecordKind::BootAcknowledgement, b"KELD-AB1", 104),
        (AttemptRecordKind::Ready, b"KELD-AY1", 8),
        (AttemptRecordKind::Failure, b"KELD-AF1", 9),
        (AttemptRecordKind::HealthResult, b"KELD-AK1", 9),
    ] {
        assert_eq!(&kind.magic(), magic, "{kind:?}");
        assert_eq!(kind.record_len(), size, "{kind:?}");
    }
}

#[test]
fn every_record_encodes_to_its_golden_bytes_and_round_trips() {
    for (record, golden, size, position) in goldens() {
        assert_eq!(golden.len(), size, "{record:?}");
        assert_eq!(encode(&record), golden, "{record:?}");
        assert_eq!(record.kind().record_len(), size, "{record:?}");
        assert_eq!(
            AttemptRecord::decode(position, &golden).expect("golden decodes"),
            record
        );
        let mut stream = Cursor::new(golden.as_slice());
        assert_eq!(
            AttemptRecord::read_from(&mut stream, position).expect("golden reads"),
            record
        );
        assert_eq!(stream.position(), u64::try_from(size).expect("small"));
    }
}

#[test]
fn each_magic_is_admitted_only_at_its_positions() {
    // Class 3 is admitted at both AF1 positions, so only the magic decides.
    let records = [
        golden_claim(),
        golden_challenge(),
        golden_transcript(*b"KELD-AA1"),
        golden_transcript(*b"KELD-AR1"),
        golden_boot(),
        b"KELD-AY1".to_vec(),
        golden_failure(3),
        golden_health(1),
    ];
    let mut refused = 0;
    for (position, admitted) in ADMITTED {
        for bytes in &records {
            let magic: [u8; 8] = bytes[..8].try_into().expect("8-byte magic");
            let decoded = AttemptRecord::decode(position, bytes);
            let streamed = AttemptRecord::read_from(&mut ThenSilence::new(bytes), position);
            if admitted.contains(&&magic) {
                assert!(decoded.is_ok(), "{position:?} {:?}", magic.escape_ascii());
                assert!(streamed.is_ok(), "{position:?} {:?}", magic.escape_ascii());
            } else {
                refused += 1;
                for result in [decoded, streamed] {
                    assert!(
                        matches!(
                            result,
                            Err(AttemptRecordError::MagicNotAdmitted { position: at, magic: got })
                                if at == position && got == magic
                        ),
                        "{position:?} {:?}: {result:?}",
                        magic.escape_ascii()
                    );
                }
            }
        }
    }
    // 8 records x 7 positions, of which 9 pairs are admitted.
    assert_eq!(refused, 8 * 7 - 9);
}

#[test]
fn magics_outside_the_closed_set_are_refused_at_every_position() {
    for magic in [
        *b"KELD-LC1",
        *b"KELD-LA1",
        *b"KELD-LR1",
        *b"KELD-BH1",
        *b"KELD-BQ1",
        *b"KELD-BA1",
        *b"KELD-BR1",
        *b"KELD-BO1",
        *b"keld-ah1",
        *b"KELD-AH2",
        *b"KELD-AH\0",
        [0; 8],
        [0xFF; 8],
    ] {
        for position in POSITIONS {
            let result = AttemptRecord::read_from(&mut ThenSilence::new(&magic), position);
            assert!(
                matches!(
                    result,
                    Err(AttemptRecordError::MagicNotAdmitted { magic: got, .. }) if got == magic
                ),
                "{position:?} {:?}: {result:?}",
                magic.escape_ascii()
            );
        }
    }
}

#[test]
fn a_non_admitted_magic_followed_by_silence_is_refused_without_another_read() {
    for (position, admitted) in ADMITTED {
        for kind_magic in [
            b"KELD-AH1",
            b"KELD-AC1",
            b"KELD-AA1",
            b"KELD-AR1",
            b"KELD-AB1",
            b"KELD-AY1",
            b"KELD-AF1",
            b"KELD-AK1",
        ] {
            let mut peer = ThenSilence::new(kind_magic);
            let result = AttemptRecord::read_from(&mut peer, position);
            if admitted.contains(&kind_magic) {
                if kind_magic == b"KELD-AY1" {
                    // AY1 is all magic: nothing further is read.
                    assert!(result.is_ok(), "{result:?}");
                    assert_eq!(peer.reads_past_prefix, 0);
                } else {
                    // Positive control: an admitted magic waits for the rest
                    // under the deadline, so the silence is observed.
                    assert!(
                        matches!(
                            &result,
                            Err(AttemptRecordError::Io { source })
                                if source.kind() == io::ErrorKind::TimedOut
                        ),
                        "{position:?}: {result:?}"
                    );
                    assert_eq!(peer.reads_past_prefix, 1);
                }
            } else {
                assert!(
                    matches!(result, Err(AttemptRecordError::MagicNotAdmitted { .. })),
                    "{position:?}: {result:?}"
                );
                assert_eq!(
                    peer.reads_past_prefix,
                    0,
                    "{position:?} {:?} read past a refused magic",
                    kind_magic.escape_ascii()
                );
            }
        }
    }
}

#[test]
fn a_truncated_record_and_one_extra_trailing_byte_are_refused() {
    for (record, golden, size, position) in goldens() {
        let short = &golden[..size - 1];
        // A short AY1 is a short magic.
        let needed = if size - 1 < 8 { 8 } else { size };
        let result = AttemptRecord::decode(position, short);
        assert!(
            matches!(
                result,
                Err(AttemptRecordError::Truncated { expected, actual })
                    if expected == needed && actual == size - 1
            ),
            "{record:?}: {result:?}"
        );
        assert!(
            matches!(
                AttemptRecord::read_from(&mut Cursor::new(short), position),
                Err(AttemptRecordError::Io { source })
                    if source.kind() == io::ErrorKind::UnexpectedEof
            ),
            "{record:?}"
        );

        let long = [golden.as_slice(), &[0]].concat();
        assert!(
            matches!(
                AttemptRecord::decode(position, &long),
                Err(AttemptRecordError::TrailingBytes { expected, actual })
                    if expected == size && actual == size + 1
            ),
            "{record:?}"
        );
        // A stream reader takes exactly one record and leaves the next byte.
        let mut stream = Cursor::new(long.as_slice());
        assert_eq!(
            AttemptRecord::read_from(&mut stream, position).expect("one whole record"),
            record
        );
        assert_eq!(stream.position(), u64::try_from(size).expect("small"));
    }
    for bytes in [&[][..], &b"KELD-AH"[..]] {
        assert!(matches!(
            AttemptRecord::decode(OwnerClaim, bytes),
            Err(AttemptRecordError::Truncated { expected: 8, actual }) if actual == bytes.len()
        ));
    }
}

#[test]
fn every_out_of_set_failure_class_is_refused_including_zero() {
    for value in (0..=u8::MAX).filter(|value| !(1..=3).contains(value)) {
        for position in [OwnerBoot, OwnerReady] {
            assert!(
                matches!(
                    AttemptRecord::decode(position, &golden_failure(value)),
                    Err(AttemptRecordError::ValueOutOfSet {
                        kind: AttemptRecordKind::Failure,
                        value: got,
                    }) if got == value
                ),
                "{position:?} class {value}"
            );
        }
    }
}

#[test]
fn every_out_of_set_health_result_is_refused_including_zero() {
    for value in (0..=u8::MAX).filter(|value| !(1..=2).contains(value)) {
        assert!(
            matches!(
                AttemptRecord::decode(CandidateHealthResult, &golden_health(value)),
                Err(AttemptRecordError::ValueOutOfSet {
                    kind: AttemptRecordKind::HealthResult,
                    value: got,
                }) if got == value
            ),
            "result {value}"
        );
    }
}

#[test]
fn a_failure_class_is_admitted_only_at_its_position() {
    use AttemptFailureClass::{ApplicationExitBeforeReady, BootError, BootstrapReadRefused};
    // KEL-53 §4 *Health sequence*: class 1 only before AB1, class 2 only
    // after it, class 3 on either side.
    for (position, value, admitted) in [
        (OwnerBoot, 1, Some(BootstrapReadRefused)),
        (OwnerBoot, 2, None),
        (OwnerBoot, 3, Some(BootError)),
        (OwnerReady, 1, None),
        (OwnerReady, 2, Some(ApplicationExitBeforeReady)),
        (OwnerReady, 3, Some(BootError)),
    ] {
        let result = AttemptRecord::decode(position, &golden_failure(value));
        match admitted {
            Some(class) => {
                assert_eq!(
                    result.expect("admitted class"),
                    AttemptRecord::Failure(class)
                );
            }
            None => assert!(
                matches!(
                    result,
                    Err(AttemptRecordError::FailureClassNotAdmitted { position: at, class })
                        if at == position && class as u8 == value
                ),
                "{position:?} class {value}: {result:?}"
            ),
        }
    }
}

/// Flips one byte inside each field of `golden`, decodes it at `position`
/// and hands the decoded record to `check`.
fn each_one_field_mutation(
    golden: &[u8],
    fields: &[(usize, usize)],
    position: AttemptReadPosition,
    check: impl Fn(AttemptRecord) -> Result<(), AttemptRecordError>,
) {
    check(AttemptRecord::decode(position, golden).expect("golden decodes"))
        .expect("the unmutated record is accepted");
    for &(start, end) in fields {
        for offset in [start, end - 1] {
            let mut mutated = golden.to_vec();
            mutated[offset] ^= 0xA5;
            let record = AttemptRecord::decode(position, &mutated).expect("still well-formed");
            let result = check(record);
            assert!(
                matches!(
                    result,
                    Err(AttemptRecordError::TranscriptMismatch | AttemptRecordError::BootMismatch)
                ),
                "byte {offset} of field {start}..{end}: {result:?}"
            );
        }
    }
}

#[test]
fn the_owner_refuses_a_one_field_mutation_of_the_acknowledgement() {
    let expected = transcript();
    each_one_field_mutation(
        &golden_transcript(*b"KELD-AA1"),
        &TRANSCRIPT_FIELDS,
        OwnerAcknowledgement,
        |record| match record {
            AttemptRecord::Acknowledgement(received) => expected.require_match(&received),
            other => panic!("OwnerAcknowledgement admitted {other:?}"),
        },
    );
}

#[test]
fn the_claimant_refuses_a_one_field_mutation_of_the_receipt() {
    let expected = transcript();
    each_one_field_mutation(
        &golden_transcript(*b"KELD-AR1"),
        &TRANSCRIPT_FIELDS,
        CandidateReceipt,
        |record| match record {
            AttemptRecord::Receipt(received) => expected.require_match(&received),
            other => panic!("CandidateReceipt admitted {other:?}"),
        },
    );
}

#[test]
fn the_owner_refuses_a_one_field_mutation_of_the_boot_acknowledgement() {
    let expected = boot();
    each_one_field_mutation(
        &golden_boot(),
        &[(8, 40), (40, 72), (72, 104)],
        OwnerBoot,
        |record| match record {
            AttemptRecord::BootAcknowledgement(received) => expected.require_match(&received),
            other => panic!("OwnerBoot admitted {other:?}"),
        },
    );
}

#[test]
fn the_owner_refuses_a_claim_with_a_foreign_installation_or_client_process() {
    let AttemptRecord::Claim(received) =
        AttemptRecord::decode(OwnerClaim, &golden_claim()).expect("golden claim")
    else {
        panic!("OwnerClaim admits only AH1");
    };
    let accepted = AttemptTranscript::for_owner(&received, &INSTALLATION, CLIENT_PID, &challenge())
        .expect("own installation and the connected client");
    assert_eq!(
        encode(&AttemptRecord::Acknowledgement(accepted)),
        golden_transcript(*b"KELD-AA1")
    );

    assert!(matches!(
        AttemptTranscript::for_owner(&received, &[0x77; 32], CLIENT_PID, &challenge()),
        Err(AttemptRecordError::ForeignInstallation)
    ));
    assert!(matches!(
        AttemptTranscript::for_owner(&received, &INSTALLATION, CLIENT_PID + 1, &challenge()),
        Err(AttemptRecordError::ClientProcessMismatch {
            claimed: CLIENT_PID,
            connected,
        }) if connected == CLIENT_PID + 1
    ));
}

#[test]
fn every_record_error_names_its_code_and_fix() {
    let cases = [
        (
            AttemptRecordError::MagicNotAdmitted {
                position: OwnerClaim,
                magic: *b"KELD-AK1",
            },
            "KELD-IPC-015: ",
        ),
        (
            AttemptRecordError::Truncated {
                expected: 76,
                actual: 75,
            },
            "KELD-IPC-015: ",
        ),
        (
            AttemptRecordError::TrailingBytes {
                expected: 76,
                actual: 77,
            },
            "KELD-IPC-015: ",
        ),
        (
            AttemptRecordError::ValueOutOfSet {
                kind: AttemptRecordKind::Failure,
                value: 0,
            },
            "KELD-IPC-015: ",
        ),
        (
            AttemptRecordError::FailureClassNotAdmitted {
                position: OwnerReady,
                class: AttemptFailureClass::BootstrapReadRefused,
            },
            "KELD-IPC-015: ",
        ),
        (AttemptRecordError::ForeignInstallation, "KELD-IPC-016: "),
        (
            AttemptRecordError::ClientProcessMismatch {
                claimed: 1,
                connected: 2,
            },
            "KELD-IPC-016: ",
        ),
        (
            AttemptRecordError::ServerProcessMismatch {
                challenged: 1,
                connected: 2,
            },
            "KELD-IPC-016: ",
        ),
        (AttemptRecordError::LocatorMismatch, "KELD-IPC-016: "),
        (AttemptRecordError::TranscriptMismatch, "KELD-IPC-016: "),
        (AttemptRecordError::BootMismatch, "KELD-IPC-016: "),
        (
            AttemptRecordError::Io {
                source: io::Error::from(io::ErrorKind::UnexpectedEof),
            },
            "KELD-IPC-017: ",
        ),
    ];
    for (error, code) in cases {
        let text = error.to_string();
        assert!(text.starts_with(code), "{text}");
        assert!(text.contains("End the exchange"), "{text}");
    }
}

#[cfg(windows)]
mod claimant {
    //! The claimant's `KELD-AC1` checks, which use the Windows-only locator.

    use std::io::{self, Write as _};
    use std::time::Duration;

    use super::{
        ATTEMPT, CHANNEL, CLIENT_PID, CandidateChallenge, INSTALLATION, OwnerClaim, SERVER_NONCE,
        SERVER_PID, challenge, claim, encode, golden_transcript,
    };
    use crate::attempt::{
        AttemptChallenge, AttemptClaim, AttemptRecord, AttemptRecordError, AttemptTranscript,
    };
    use crate::bootstrap::connected_named_pipe_pair;
    use crate::link::AppLinkDeadlines as _;
    use crate::token::SessionToken;

    /// KEL-53 §4 *Locator* golden vector for the IDs above.
    const RENDEZVOUS: &str =
        r"\\.\pipe\keld-attempt-a56a565b56c571bd19b06b8e62845fa5a14c28fecd5611c94e4c90e8a1641ba3";

    fn offered(attempt_id: [u8; 32], health_channel_id: [u8; 32]) -> AttemptChallenge {
        AttemptChallenge::new(
            attempt_id,
            health_channel_id,
            SessionToken::from_bytes(SERVER_NONCE),
            SERVER_PID,
        )
    }

    #[test]
    fn the_claimant_admits_ids_that_derive_its_rendezvous_name() {
        let accepted =
            AttemptTranscript::for_claimant(&claim(), &challenge(), RENDEZVOUS, SERVER_PID)
                .expect("the golden IDs derive the golden name");
        assert_eq!(
            encode(&AttemptRecord::Acknowledgement(accepted)),
            golden_transcript(*b"KELD-AA1")
        );
    }

    #[test]
    fn a_challenge_whose_ids_fail_the_locator_check_is_refused_before_aa1() {
        let other_installation =
            AttemptClaim::new([0x77; 32], SessionToken::from_bytes([0x44; 32]), CLIENT_PID);
        let cases = [
            ("another attempt", claim(), offered([0x77; 32], CHANNEL)),
            ("another channel", claim(), offered(ATTEMPT, [0x77; 32])),
            ("exchanged IDs", claim(), offered(CHANNEL, ATTEMPT)),
            ("zero attempt", claim(), offered([0; 32], CHANNEL)),
            ("zero channel", claim(), offered(ATTEMPT, [0; 32])),
            ("attempt equals channel", claim(), offered(ATTEMPT, ATTEMPT)),
            (
                "attempt equals installation",
                claim(),
                offered(INSTALLATION, CHANNEL),
            ),
            ("another installation", other_installation, challenge()),
        ];
        for (case, claimed, offered) in cases {
            // No transcript, so no AA1 can be built or sent.
            assert!(
                matches!(
                    AttemptTranscript::for_claimant(&claimed, &offered, RENDEZVOUS, SERVER_PID),
                    Err(AttemptRecordError::LocatorMismatch)
                ),
                "{case}"
            );
        }
        // The right IDs for another rendezvous name are refused too.
        let elsewhere = format!(r"\\.\pipe\keld-attempt-{}", "0".repeat(64));
        assert!(matches!(
            AttemptTranscript::for_claimant(&claim(), &challenge(), &elsewhere, SERVER_PID),
            Err(AttemptRecordError::LocatorMismatch)
        ));
    }

    #[test]
    fn a_challenge_from_another_server_process_is_refused() {
        assert!(matches!(
            AttemptTranscript::for_claimant(&claim(), &challenge(), RENDEZVOUS, SERVER_PID + 1),
            Err(AttemptRecordError::ServerProcessMismatch {
                challenged: SERVER_PID,
                connected,
            }) if connected == SERVER_PID + 1
        ));
    }

    /// On the shipped Windows transport: a peer that sends one non-admitted
    /// magic and then holds the connection open silently. The kill-switch
    /// deadline is far longer than the test; a reader that waited for more
    /// bytes would end with a timeout, not the refusal.
    #[test]
    fn a_non_admitted_magic_on_a_silent_pipe_is_refused_before_its_deadline() -> io::Result<()> {
        let (mut owner, mut candidate) = connected_named_pipe_pair()?;
        owner.set_app_link_read_deadline(Some(Duration::from_mins(1)))?;
        candidate.write_all(b"KELD-AC1")?;
        let result = AttemptRecord::read_from(&mut owner, OwnerClaim);
        assert!(
            matches!(
                result,
                Err(AttemptRecordError::MagicNotAdmitted {
                    position: OwnerClaim,
                    magic,
                }) if magic == *b"KELD-AC1"
            ),
            "{result:?}"
        );
        drop(candidate);
        Ok(())
    }

    /// Positive control for the test above on the same transport: an admitted
    /// magic followed by silence is read on until the deadline expires.
    #[test]
    fn an_admitted_magic_on_a_silent_pipe_waits_for_its_deadline() -> io::Result<()> {
        let (mut owner, mut candidate) = connected_named_pipe_pair()?;
        candidate.set_app_link_read_deadline(Some(Duration::from_millis(200)))?;
        owner.write_all(b"KELD-AC1")?;
        let result = AttemptRecord::read_from(&mut candidate, CandidateChallenge);
        assert!(
            matches!(
                &result,
                Err(AttemptRecordError::Io { source })
                    if source.kind() == io::ErrorKind::TimedOut
            ),
            "{result:?}"
        );
        drop(owner);
        Ok(())
    }
}
