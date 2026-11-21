//! Closed production boundary for KEL-270 T-20 metadata admission.
//!
//! This module MUST NOT grant a `ReadyRevision` until the independently reviewed
//! `tough =0.24.0` patch, complete dependency graph and T-82 grants are integrated.
//! The four byte slices are untrusted. Bounded input is NOT authenticated input.
//! In particular, ordinary v0 manifest Ed25519 checks are not TUF root/role
//! threshold verification, and this module deliberately has no signature parser.
//!
//! The only pinned source is awslabs/tough commit
//! 98d8eb8b2ce63515d9b4981c938ef6453c5b5771, crate SHA-256
//! 35b378d98765c2ae9cdc3e9963ea7e670da8cdd9ee39611b8d722083c7f1ac11.
//! Until the owner reviews the corrected source, *every* role set refuses.
//!
//! Test-layout review trigger (owner KEL-270/T-20): the scoped tests remain
//! adjacent to this private, closed production intake while T-82 is blocked.
//! Split/review at the qualified tough-client integration; no test identity
//! or runner change is authorized by this exception request.

use std::io::Read;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TufRole {
    Root,
    Timestamp,
    Snapshot,
    Targets,
}

impl TufRole {
    const fn max_bytes(self) -> usize {
        match self {
            Self::Root => 65_536,
            Self::Timestamp => 16_384,
            Self::Snapshot => 262_144,
            Self::Targets => 1_048_576,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SignedRoleBytes<'a> {
    pub(crate) root: &'a [u8],
    pub(crate) timestamp: &'a [u8],
    pub(crate) snapshot: &'a [u8],
    pub(crate) targets: &'a [u8],
}

/// One real, caller-owned reader per untrusted top-level role. This intake
/// borrows the readers; it never opens a pathname, transfers a HANDLE, starts
/// a network request or creates a parallel metadata datastore.
pub(crate) struct SignedRoleReaders<'a> {
    pub(crate) root: &'a mut dyn Read,
    pub(crate) timestamp: &'a mut dyn Read,
    pub(crate) snapshot: &'a mut dyn Read,
    pub(crate) targets: &'a mut dyn Read,
}

impl SignedRoleBytes<'_> {
    fn roles(self) -> [(TufRole, usize); 4] {
        [
            (TufRole::Root, self.root.len()),
            (TufRole::Timestamp, self.timestamp.len()),
            (TufRole::Snapshot, self.snapshot.len()),
            (TufRole::Targets, self.targets.len()),
        ]
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum MetadataClientRefusal {
    MissingRole(TufRole),
    RoleExceedsCap(TufRole),
    RoleReadFailed(TufRole),
    TufClientNotQualified,
}

/// Private, non-cloneable admission authority. No constructor exists in this
/// unqualified branch, so a caller cannot mint one from role JSON or digests.
#[derive(Debug)]
pub(crate) struct ReadyRevision {
    _private: (),
}

/// Production intake rejects missing/oversized role blobs before any future
/// parser allocation. These checks are necessary, never sufficient. A complete
/// apparent role graph still refuses without qualified TUF signature, version,
/// expiry, historical-floor and consistent-snapshot verification.
///
/// This function owns no filesystem, lease, network request, installer route,
/// key choice or write authority.
pub(crate) fn verify_ready_revision(
    roles: SignedRoleBytes<'_>,
) -> Result<ReadyRevision, MetadataClientRefusal> {
    for (role, length) in roles.roles() {
        if length == 0 {
            return Err(MetadataClientRefusal::MissingRole(role));
        }
        if length > role.max_bytes() {
            return Err(MetadataClientRefusal::RoleExceedsCap(role));
        }
    }
    Err(MetadataClientRefusal::TufClientNotQualified)
}

/// Reads real role streams to their reported EOF within the D-21 individual
/// limits, then passes their complete untrusted bytes to the closed verifier.
///
/// A short successful `Read` is not an EOF. An error after the final signed
/// byte is still an error until the actual EOF observation succeeds. A
/// cap-plus-one body refuses without buffering the overflow byte.
///
/// This is NOT the TUF transport: the existing acquisition owner must enforce
/// HTTPS origin, identity content encoding, request/aggregate size and timeout,
/// and source-handle custody. EOF does not authenticate a signed length or
/// hash. No input, including perfectly bounded signed-looking JSON, mints
/// `ReadyRevision` before T-20/T-82 qualifies patched tough and its root,
/// threshold, expiry, rollback, historical floors and snapshot graph.
pub(crate) fn verify_ready_revision_from_readers(
    readers: SignedRoleReaders<'_>,
) -> Result<ReadyRevision, MetadataClientRefusal> {
    let SignedRoleReaders {
        root,
        timestamp,
        snapshot,
        targets,
    } = readers;
    let root = read_complete_role(TufRole::Root, root)?;
    let timestamp = read_complete_role(TufRole::Timestamp, timestamp)?;
    let snapshot = read_complete_role(TufRole::Snapshot, snapshot)?;
    let targets = read_complete_role(TufRole::Targets, targets)?;
    verify_ready_revision(SignedRoleBytes {
        root: &root,
        timestamp: &timestamp,
        snapshot: &snapshot,
        targets: &targets,
    })
}

// Keep both the I/O work and its allocated body bounded. A read asks for at
// most the remaining bytes plus one to distinguish exact-cap EOF from excess.
// All bytes stay private and are dropped on any refusal. Never parse a prefix.
fn read_complete_role(
    role: TufRole,
    reader: &mut dyn Read,
) -> Result<Vec<u8>, MetadataClientRefusal> {
    read_complete_role_observing_reservations(role, reader, |_| {})
}

// The no-op observer is compiled away for production. Tests use it to count
// actual successful reservation calls, not wall-clock time or allocator luck.
fn read_complete_role_observing_reservations(
    role: TufRole,
    reader: &mut dyn Read,
    mut observe_reservation: impl FnMut(usize),
) -> Result<Vec<u8>, MetadataClientRefusal> {
    let mut complete = Vec::new();
    let mut chunk = [0_u8; 8_192];
    loop {
        let remaining = role.max_bytes() - complete.len();
        let requested = chunk.len().min(remaining + 1);
        let read = reader
            .read(&mut chunk[..requested])
            .map_err(|_| MetadataClientRefusal::RoleReadFailed(role))?;
        if read == 0 {
            return if complete.is_empty() {
                Err(MetadataClientRefusal::MissingRole(role))
            } else {
                Ok(complete)
            };
        }
        if read > remaining {
            return Err(MetadataClientRefusal::RoleExceedsCap(role));
        }
        if complete.len() + read > complete.capacity() {
            // Grow geometrically, even if an adversarial Read returns one
            // byte at a time. Never *request* capacity above the role cap;
            // retained bytes and cap-plus-one refusal are unchanged.
            let desired_capacity = complete
                .capacity()
                .saturating_mul(2)
                .max(chunk.len())
                .min(role.max_bytes());
            complete
                .try_reserve_exact(desired_capacity - complete.len())
                .map_err(|_| MetadataClientRefusal::RoleReadFailed(role))?;
            observe_reservation(desired_capacity);
        }
        complete.extend_from_slice(&chunk[..read]);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MetadataClientRefusal, SignedRoleBytes, SignedRoleReaders, TufRole,
        read_complete_role_observing_reservations, verify_ready_revision,
        verify_ready_revision_from_readers,
    };
    use std::io::{self, Cursor, Read};

    // Independent D-21 oracles. Do not derive fixture lengths from the
    // production TufRole::max_bytes implementation under test: a changed cap
    // must make the exact-cap or cap-plus-one regression fail.
    const D21_ROLE_LIMITS: [(TufRole, usize); 4] = [
        (TufRole::Root, 65_536),
        (TufRole::Timestamp, 16_384),
        (TufRole::Snapshot, 262_144),
        (TufRole::Targets, 1_048_576),
    ];

    fn inputs<'a>(
        root: &'a [u8],
        timestamp: &'a [u8],
        snapshot: &'a [u8],
        targets: &'a [u8],
    ) -> SignedRoleBytes<'a> {
        SignedRoleBytes {
            root,
            timestamp,
            snapshot,
            targets,
        }
    }

    #[test]
    fn complete_signed_looking_roles_never_create_ready_revision() {
        let claimed = br#"{"signed":{"_type":"root","version":1},"signatures":[{"keyid":"bad","sig":"bad"}]}"#;
        assert_eq!(
            verify_ready_revision(inputs(claimed, claimed, claimed, claimed)).unwrap_err(),
            MetadataClientRefusal::TufClientNotQualified
        );
    }

    #[test]
    fn missing_each_role_refuses() {
        let candidate = b"{";
        for (role, root, timestamp, snapshot, targets) in [
            (
                TufRole::Root,
                &[][..],
                &candidate[..],
                &candidate[..],
                &candidate[..],
            ),
            (
                TufRole::Timestamp,
                &candidate[..],
                &[][..],
                &candidate[..],
                &candidate[..],
            ),
            (
                TufRole::Snapshot,
                &candidate[..],
                &candidate[..],
                &[][..],
                &candidate[..],
            ),
            (
                TufRole::Targets,
                &candidate[..],
                &candidate[..],
                &candidate[..],
                &[][..],
            ),
        ] {
            assert_eq!(
                verify_ready_revision(inputs(root, timestamp, snapshot, targets)).unwrap_err(),
                MetadataClientRefusal::MissingRole(role)
            );
        }
    }

    #[test]
    fn one_byte_over_each_independent_role_cap_refuses() {
        let candidate = b"{";
        for (role, max_bytes) in D21_ROLE_LIMITS {
            let over = vec![b'X'; max_bytes + 1];
            let mut root = &candidate[..];
            let mut timestamp = &candidate[..];
            let mut snapshot = &candidate[..];
            let mut targets = &candidate[..];
            match role {
                TufRole::Root => root = &over,
                TufRole::Timestamp => timestamp = &over,
                TufRole::Snapshot => snapshot = &over,
                TufRole::Targets => targets = &over,
            }
            assert_eq!(
                verify_ready_revision(inputs(root, timestamp, snapshot, targets)).unwrap_err(),
                MetadataClientRefusal::RoleExceedsCap(role)
            );
        }
    }

    #[test]
    fn at_each_exact_cap_still_refuses_without_tuf_authentication() {
        let root = vec![b'{'; 65_536];
        let timestamp = vec![b'{'; 16_384];
        let snapshot = vec![b'{'; 262_144];
        let targets = vec![b'{'; 1_048_576];
        assert_eq!(
            verify_ready_revision(inputs(&root, &timestamp, &snapshot, &targets)).unwrap_err(),
            MetadataClientRefusal::TufClientNotQualified
        );
    }

    struct ObservedReader<'a> {
        bytes: &'a [u8],
        offset: usize,
        eof_observations: usize,
        max_chunk: usize,
        fail_after: Option<usize>,
    }

    impl<'a> ObservedReader<'a> {
        fn new(bytes: &'a [u8]) -> Self {
            Self {
                bytes,
                offset: 0,
                eof_observations: 0,
                max_chunk: usize::MAX,
                fail_after: None,
            }
        }
    }

    impl Read for ObservedReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.fail_after.is_some_and(|end| self.offset >= end) {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "interrupted body",
                ));
            }
            let available = self.bytes.len() - self.offset;
            if available == 0 {
                self.eof_observations += 1;
                return Ok(0);
            }
            let count = available.min(buffer.len()).min(self.max_chunk);
            buffer[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
            self.offset += count;
            Ok(count)
        }
    }

    #[test]
    fn short_reads_require_actual_eof_for_all_four_roles() {
        let role_bytes = br#"{"signed":{"_type":"timestamp","version":100,"expires":"2099-01-01T00:00:00Z"},"signatures":[]}"#;
        let mut root = ObservedReader::new(role_bytes);
        let mut timestamp = ObservedReader::new(role_bytes);
        let mut snapshot = ObservedReader::new(role_bytes);
        let mut targets = ObservedReader::new(role_bytes);
        for reader in [&mut root, &mut timestamp, &mut snapshot, &mut targets] {
            reader.max_chunk = 3;
        }
        assert_eq!(
            verify_ready_revision_from_readers(SignedRoleReaders {
                root: &mut root,
                timestamp: &mut timestamp,
                snapshot: &mut snapshot,
                targets: &mut targets,
            })
            .unwrap_err(),
            MetadataClientRefusal::TufClientNotQualified
        );
        for reader in [&root, &timestamp, &snapshot, &targets] {
            assert_eq!(reader.offset, role_bytes.len());
            assert_eq!(reader.eof_observations, 1);
        }
    }

    #[test]
    fn one_byte_targets_fragments_require_only_logarithmic_reservations() {
        // Independent D-21 Targets cap. The old per-read reserve_exact(1)
        // attempts 1,048,576 reservations against this compliant reader.
        const TARGETS_LIMIT: usize = 1_048_576;
        const MAX_RESERVATIONS: usize = 8;
        let payload = vec![b'X'; TARGETS_LIMIT];
        let mut source = ObservedReader::new(&payload);
        source.max_chunk = 1;
        let mut reservations = Vec::new();
        let complete =
            read_complete_role_observing_reservations(TufRole::Targets, &mut source, |target| {
                reservations.push(target)
            })
            .expect("one-byte reads still produce the complete bounded role");

        assert_eq!(complete.len(), TARGETS_LIMIT);
        assert!(
            complete.iter().all(|&byte| byte == b'X'),
            "the complete role must retain every original byte"
        );
        assert_eq!(source.offset, TARGETS_LIMIT);
        assert_eq!(source.eof_observations, 1);
        assert!(
            !reservations.is_empty() && reservations.len() <= MAX_RESERVATIONS,
            "one-byte fragmentation made {} reservation requests, not logarithmic growth",
            reservations.len()
        );
        assert!(
            reservations.iter().all(|&target| target <= TARGETS_LIMIT),
            "a reservation requested capacity beyond the independent Targets cap"
        );
        assert!(
            reservations
                .windows(2)
                .all(|pair| pair[1] >= pair[0].saturating_mul(2)),
            "reservation targets must grow geometrically: {reservations:?}"
        );
    }

    #[test]
    fn exact_cap_streams_observe_eof_but_cannot_grant_trust() {
        let one = b"X";
        for (role, max_bytes) in D21_ROLE_LIMITS {
            let complete = vec![b'X'; max_bytes];
            let mut root = Cursor::new(if role == TufRole::Root {
                complete.as_slice()
            } else {
                one
            });
            let mut timestamp = Cursor::new(if role == TufRole::Timestamp {
                complete.as_slice()
            } else {
                one
            });
            let mut snapshot = Cursor::new(if role == TufRole::Snapshot {
                complete.as_slice()
            } else {
                one
            });
            let mut targets = Cursor::new(if role == TufRole::Targets {
                complete.as_slice()
            } else {
                one
            });
            assert_eq!(
                verify_ready_revision_from_readers(SignedRoleReaders {
                    root: &mut root,
                    timestamp: &mut timestamp,
                    snapshot: &mut snapshot,
                    targets: &mut targets,
                })
                .unwrap_err(),
                MetadataClientRefusal::TufClientNotQualified,
                "role {role:?}"
            );
        }
    }

    #[test]
    fn cap_plus_one_stream_refuses_without_reading_full_oversized_body() {
        let one = b"X";
        for (role, max_bytes) in D21_ROLE_LIMITS {
            let oversized = vec![b'X'; max_bytes + 4_096];
            let mut root = Cursor::new(if role == TufRole::Root {
                oversized.as_slice()
            } else {
                one
            });
            let mut timestamp = Cursor::new(if role == TufRole::Timestamp {
                oversized.as_slice()
            } else {
                one
            });
            let mut snapshot = Cursor::new(if role == TufRole::Snapshot {
                oversized.as_slice()
            } else {
                one
            });
            let mut targets = Cursor::new(if role == TufRole::Targets {
                oversized.as_slice()
            } else {
                one
            });
            assert_eq!(
                verify_ready_revision_from_readers(SignedRoleReaders {
                    root: &mut root,
                    timestamp: &mut timestamp,
                    snapshot: &mut snapshot,
                    targets: &mut targets,
                })
                .unwrap_err(),
                MetadataClientRefusal::RoleExceedsCap(role)
            );
            let consumed = match role {
                TufRole::Root => root.position(),
                TufRole::Timestamp => timestamp.position(),
                TufRole::Snapshot => snapshot.position(),
                TufRole::Targets => targets.position(),
            };
            assert_eq!(consumed, max_bytes as u64 + 1, "role {role:?}");
        }
    }

    #[test]
    fn stream_failure_at_or_after_last_byte_is_not_eof() {
        let one = b"X";
        for (role, eof_failure) in [
            (TufRole::Root, false),
            (TufRole::Timestamp, false),
            (TufRole::Snapshot, false),
            (TufRole::Targets, false),
            (TufRole::Root, true),
            (TufRole::Timestamp, true),
            (TufRole::Snapshot, true),
            (TufRole::Targets, true),
        ] {
            let mut root = ObservedReader::new(one);
            let mut timestamp = ObservedReader::new(one);
            let mut snapshot = ObservedReader::new(one);
            let mut targets = ObservedReader::new(one);
            let chosen = match role {
                TufRole::Root => &mut root,
                TufRole::Timestamp => &mut timestamp,
                TufRole::Snapshot => &mut snapshot,
                TufRole::Targets => &mut targets,
            };
            chosen.fail_after = Some(usize::from(eof_failure));
            assert_eq!(
                verify_ready_revision_from_readers(SignedRoleReaders {
                    root: &mut root,
                    timestamp: &mut timestamp,
                    snapshot: &mut snapshot,
                    targets: &mut targets,
                })
                .unwrap_err(),
                MetadataClientRefusal::RoleReadFailed(role),
                "role {role:?}, after-last-byte={eof_failure}"
            );
        }
    }
}
