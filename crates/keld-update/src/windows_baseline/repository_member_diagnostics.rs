//! Negative D-90 observation of unpointed D-64 admission members.
//!
//! This is deliberately an observation of ONE retained native admissions
//! directory, not a repository-wide completeness producer. It creates no
//! writer lock, registry, journal, reader whitelist, or eligibility capability.
//! KEL-53 T-82 must independently hold its single exclusive repository lease,
//! authenticate the retained parent, and cover every supported v1/v2 root,
//! pending owner, admitted permit, old Host/Guard/launcher and in-flight process.

use cap_std::fs::Dir;

use super::error;
use crate::UpdateError;
use crate::records::{PendingOperationKind, PendingRepositoryOperation, RepositoryAnchor};

/// Refuse a partly published protected record beside the fixed D-68 pointer.
///
/// `record_publication::prepare_record` creates `pending-<random>` in the
/// same retained parent as `pending-operation.json` before its absent-target
/// publish. A process exit at that cut leaves no D-68 pointer but does leave
/// durable native evidence. It must block new admission and ordinary metadata
/// selection. With a pointer present, a second pending sibling is also
/// unaccounted for and requires the original writer's recovery.
///
/// The exact canonical D-68 pointer name is the only `pending-` exception;
/// actual record identity and bytes are checked separately by the protected
/// reader. This is a local negative observation, not an authenticated KEL-53
/// repository-wide census or permission to delete anything.
pub(super) fn require_no_repository_pending_siblings(repository: &Dir) -> Result<(), UpdateError> {
    for entry in repository
        .entries()
        .map_err(|cause| error("repository pending namespace", cause))?
    {
        let entry = entry.map_err(|cause| error("repository pending namespace", cause))?;
        let name = entry.file_name().into_string().map_err(|_| {
            error(
                "repository pending namespace",
                "non-UTF-8 repository member prevents a complete pending-sibling observation",
            )
        })?;
        if name
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("pending-"))
            && name != "pending-operation.json"
        {
            return Err(error(
                "repository pending publication residue",
                "an unaccounted protected record sibling or noncanonical pending pointer blocks restart and admission",
            ));
        }
    }
    Ok(())
}

/// Refuse an unpointed D-64 journal, unknown leaf or incomplete publication
/// in the retained KEL-53 admissions directory.
///
/// Call this ONLY after an authenticated D-68 pending-pointer read returned
/// absent, with the original repository owner retaining its native admissions
/// directory. D-64 publishes its admitted record before the D-68 pointer; a
/// crash in that interval must never become an ordinary metadata restart.
///
/// Even terminal records require the exact protected D-64/D-68 owner to inspect
/// their identity and release; a matching filename is never cleanup authority.
/// The record's bytes and descriptor are not trusted here: ANY entry refuses,
/// including case variants, temporary publication siblings and non-UTF-8 names.
/// Opening an entry to infer its phase is unnecessary for this negative gate.
///
/// `Ok(())` means only that this one retained directory yielded no entry
/// during this observation. It is NOT an authoritative empty-store proof.
/// Without the repository-wide writer lease, another writer could publish
/// immediately after the observation; another supported root might already
/// hold a member. The unconstructible `RepositoryLedgerCensus` still gates
/// all v2 publication and launch authority.
///
/// # Errors
/// Refuses enumeration error, an orphan admission journal, unknown entry, or
/// unreadable filename, without modifying or deleting any object.
pub(super) fn require_no_unpointed_admissions(admissions: &Dir) -> Result<(), UpdateError> {
    let mut entries = admissions
        .entries()
        .map_err(|cause| error("repository admissions observation", cause))?;
    let Some(entry) = entries.next() else {
        return Ok(());
    };
    let entry = entry.map_err(|cause| error("repository admissions observation", cause))?;
    let Ok(name) = entry.file_name().into_string() else {
        return Err(error(
            "repository admissions namespace",
            "unreadable admission filename blocks ordinary metadata selection",
        ));
    };
    if name
        .strip_suffix(".json")
        .is_some_and(|prefix| prefix.len() == 64 && crate::error::is_lowercase_hex(prefix))
    {
        return Err(error(
            "repository unpointed admission",
            "a D-64 attempt exists without a D-68 pointer; the original owner must prove no effect or complete exact recovery before selection",
        ));
    }
    Err(error(
        "repository admissions namespace",
        "an unexpected admission object or partial publication blocks ordinary metadata selection",
    ))
}

/// Refuse unexplained D-64/D-65/D-68 operation members in all three retained
/// operation directories before accepting an exact pointed repository operation.
///
/// The caller must first authenticate the D-68 pointer and all three retained
/// directory handles as part of its existing KEL-53 repository owner. The
/// owner must retain its SINGLE exclusive metadata lease across this check,
/// record readback, and any decision. Directory traversal cannot substitute
/// for that lease or prove coverage of other v1/v2 installations.
///
/// With a pointed operation, exactly its derived journal leaf must occur in
/// the corresponding directory. A second admission (even same channel), an
/// unpointed recovery/transition journal, a different-case alias, a pending
/// temporary sibling or a missing pointed leaf refuses. The exact pointed
/// journal's type, DACL, bytes, phase and owner still require an independent
/// retained-handle read and the recorded recovery owner; a matching name is
/// not an authenticated journal.
///
/// Without a pointer, any observed operation member refuses. An empty local
/// operation set is ONLY a negative observation: this function neither mints
/// `RepositoryLedgerCensus` nor attests that every supported historic root,
/// retained permit, old reader, or in-flight process has been enumerated.
/// A writer racing after an unlocked observation requires a recheck under the
/// real repository lease; the local directory scan is never a lock.
///
/// # Errors
/// Refuses foreign pointer identity, absent pointed member, unexpected or
/// unreadable name, or enumeration error. It makes no changes to any record.
pub(super) fn require_only_pointed_repository_member(
    admissions: &Dir,
    recoveries: &Dir,
    transitions: &Dir,
    anchor: &RepositoryAnchor,
    pending: Option<&PendingRepositoryOperation>,
) -> Result<(), UpdateError> {
    if pending.is_some_and(|pointer| {
        pointer.repository_id != anchor.repository_id
            || pointer.app_id != anchor.app_id
            || pointer.owner_sid != anchor.owner_sid
    }) {
        return Err(error(
            "repository member identity",
            "pending operation differs from the authenticated repository, app or owner SID",
        ));
    }

    let expected = pending.map(|pointer| {
        (
            pointer.kind,
            format!("{}.json", crate::error::hex_digest(&pointer.attempt_id)),
        )
    });
    let mut found = false;
    for (kind, parent) in [
        (PendingOperationKind::InitialAdmission, admissions),
        (PendingOperationKind::ManualRecovery, recoveries),
        (PendingOperationKind::PlannedTransition, transitions),
    ] {
        for entry in parent
            .entries()
            .map_err(|cause| error("repository member observation", cause))?
        {
            let entry = entry.map_err(|cause| error("repository member observation", cause))?;
            let name = entry.file_name().into_string().map_err(|_| {
                error(
                    "repository member namespace",
                    "non-UTF-8 operation entry blocks repository admission",
                )
            })?;
            let is_pointed = expected
                .as_ref()
                .is_some_and(|(expected_kind, leaf)| *expected_kind == kind && leaf == &name);
            if !is_pointed || found {
                return Err(error(
                    "repository member namespace",
                    "unpointed or excess repository operation entry blocks the exact owner",
                ));
            }
            found = true;
        }
    }
    if expected.is_some() && !found {
        return Err(error(
            "repository pointed membership",
            "pending pointer names no corresponding journal in its retained directory",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod pointed_tests;

#[cfg(test)]
mod crash_tests;

#[cfg(test)]
mod tests {
    use std::fs;

    use cap_std::ambient_authority;
    use cap_std::fs::Dir;

    use super::require_no_unpointed_admissions;
    use crate::UpdateError;

    fn refused_step(directory: &Dir) -> &'static str {
        match require_no_unpointed_admissions(directory)
            .expect_err("a retained admission entry must block pointer-absent restart")
        {
            UpdateError::Baseline { step, .. } => step,
            other => panic!("unexpected D-90 refusal: {other}"),
        }
    }

    #[test]
    fn real_retained_directory_detects_orphan_between_admitted_and_pointer_publication() {
        let fixture = tempfile::tempdir().expect("isolated native admissions directory");
        let path = fixture.path().join("admissions");
        fs::create_dir(&path).expect("create empty admissions directory");
        let retained = Dir::open_ambient_dir(&path, ambient_authority())
            .expect("retain native admissions parent");

        // No local entry observed is a DIAGNOSTIC only: an unsupported v1
        // sibling, in-flight permit or a writer outside the absent T-82 lease
        // remains unknown.
        require_no_unpointed_admissions(&retained)
            .expect("the local directory is empty before simulated crash");
        let leaf = format!("{}.json", "2a".repeat(32));
        let admission = path.join(&leaf);
        fs::write(&admission, b"admitted-durable-before-pointer")
            .expect("simulate durable D-64 leaf before D-68 publication");
        let before = fs::read(&admission).expect("read admission bytes");

        assert_eq!(refused_step(&retained), "repository unpointed admission");
        assert_eq!(fs::read(&admission).expect("preserved orphan"), before);
        assert!(
            !fixture.path().join("pending-operation.json").exists(),
            "refusal must not synthesize or repair a missing D-68 pointer"
        );
    }

    #[test]
    fn unexpected_partial_publication_refuses_without_deletion_or_adoption() {
        let fixture = tempfile::tempdir().expect("isolated partial journal directory");
        let path = fixture.path().join("admissions");
        fs::create_dir(&path).expect("create admissions directory");
        let unexpected = path.join("pending-uncommitted");
        fs::write(&unexpected, b"partial").expect("durable incomplete sibling");
        let retained = Dir::open_ambient_dir(&path, ambient_authority())
            .expect("retain native admissions parent");

        assert_eq!(refused_step(&retained), "repository admissions namespace");
        assert_eq!(
            fs::read(&unexpected).expect("preserved incomplete bytes"),
            b"partial"
        );
    }

    #[test]
    fn reinspection_detects_new_admission_after_an_unlocked_empty_observation() {
        use std::sync::mpsc;
        use std::thread;

        let fixture = tempfile::tempdir().expect("isolated concurrent admission fixture");
        let path = fixture.path().join("admissions");
        fs::create_dir(&path).expect("create admissions directory");
        let retained = Dir::open_ambient_dir(&path, ambient_authority())
            .expect("retain native admissions parent");

        let leaf = path.join(format!("{}.json", "6b".repeat(32)));
        let other_leaf = leaf.clone();
        let (release_writer, start_writer) = mpsc::channel();
        let writer = thread::spawn(move || {
            start_writer.recv().expect("trigger simulated other writer");
            fs::write(&other_leaf, b"admitted").expect("write after unlocked local observation");
        });
        require_no_unpointed_admissions(&retained).expect("initial local observation");
        release_writer.send(()).expect("release concurrent writer");
        writer.join().expect("concurrent writer finished");
        assert_eq!(refused_step(&retained), "repository unpointed admission");
        assert!(
            leaf.is_file(),
            "the negative fence must preserve the journal"
        );
        // This observation detects a new local member on recheck; it cannot
        // serialize the race. T-82's existing single repository lease must do so.
    }
}
