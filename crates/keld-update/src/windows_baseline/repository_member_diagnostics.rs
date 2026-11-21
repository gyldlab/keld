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
