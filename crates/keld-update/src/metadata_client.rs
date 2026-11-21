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
//! Test-layout review trigger (owner KEL-270/T-20): the four tests remain
//! adjacent to this private, closed production intake while T-82 is blocked.
//! Split/review at the qualified tough-client integration; no test identity
//! or runner change is authorized by this exception request.

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

#[cfg(test)]
mod tests {
    use super::{MetadataClientRefusal, SignedRoleBytes, TufRole, verify_ready_revision};

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
}
