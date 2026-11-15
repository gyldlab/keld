//! Native owner-private stage, completion and case-alias publication contracts.
//!
//! An authenticated writer may stage and complete bytes without selecting a
//! version. The original process crash-cut, lease and sentinel assertions stay
//! intact; exact subprocess test entry points stay with the parent.

use super::{
    GOLDEN, higher_release, higher_release_version, load_windows_activation_write_snapshot,
    load_windows_baseline, run_version_publication_crash_child, seed_per_user_baseline, support,
};

#[test]
fn per_user_writer_snapshot_excludes_readers_and_competing_writers() {
    let fixture = tempfile::tempdir().expect("per-user writer fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");

    let reader = load_windows_baseline(&trust).expect("initial protected reader");
    let observation = reader.observation().clone();
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "live coherent reader retains the shared lease and excludes a writer"
    );
    drop(reader);
    let candidate = higher_release(&verifier, &observation);

    let writer = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect("per-user host acquires the exact exclusive writer snapshot");
    assert_eq!(writer.current().version, "1.0.0");
    assert_eq!(writer.last_known_good().version, "1.0.0");
    assert!(writer.previous_known_good().is_none());
    assert!(
        load_windows_baseline(&trust).is_err(),
        "exclusive writer snapshot excludes new readers"
    );
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "exclusive writer snapshot excludes a competing writer"
    );

    let keeper_lease = writer
        .duplicate_lifecycle_lease_retention()
        .expect("keeper receives reduced rights to the exact lease object");
    drop(writer);
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "keeper retention handle preserves the existing share-zero writer lease"
    );
    drop(keeper_lease);
    let writer = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect("writer can reacquire after keeper retention is released");

    let mut extraction = writer
        .open_extraction_root()
        .expect("writer capability opens its mode-bound staging root");
    let archive = fixture.path().join("baseline.tar");
    let staged = extraction
        .extract(&candidate, &archive)
        .expect("authenticated candidate stages under the held writer lease");
    assert_eq!(staged.identity().version, "2.0.0");
    let stage_name = staged.name().to_owned();
    let staged_path = trust
        .installation
        .update_root
        .join("versions")
        .join(&stage_name);
    assert_eq!(
        std::fs::read(staged_path.join("content.tar")).expect("staged archive bytes"),
        GOLDEN
    );
    assert!(
        !staged_path.join(".complete").exists(),
        "staging alone never publishes a runnable version"
    );
    assert!(
        load_windows_baseline(&trust).is_err(),
        "staged candidate keeps the exclusive lease for the transaction owner"
    );
    drop(staged);
    drop(extraction);
    let committed = load_windows_baseline(&trust).expect("dropping writer root releases readers");
    assert_eq!(committed.version_floor(), "1.0.0");
    assert_eq!(
        committed.identity().baseline.version,
        "1.0.0",
        "staging cannot change protected baseline selection"
    );
}

#[test]
fn per_user_writer_completes_a_stage_without_publishing_or_selecting_it() {
    let fixture = tempfile::tempdir().expect("per-user immutable-version fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted per-user verifier");
    let baseline = load_windows_baseline(&trust).expect("load exact baseline");
    let observation = baseline.observation().clone();
    drop(baseline);
    let candidate = higher_release(&verifier, &observation);
    let source = fixture.path().join("candidate.tar");
    std::fs::write(&source, GOLDEN).expect("write verified candidate source");

    let update = &trust.installation.update_root;
    let before = ["version-floor", "current", "last-known-good"]
        .map(|name| std::fs::read(update.join(name)).expect("read active record before publish"));
    assert!(!update.join("previous-known-good").exists());
    assert!(!update.join("activation-journal").exists());

    let snapshot = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect("acquire exact per-user activation writer");
    let mut root = snapshot
        .open_extraction_root()
        .expect("retain the exclusive writer lease through staging");
    let stage = root
        .extract(&candidate, &source)
        .expect("extract verified candidate as an incomplete stage");
    let diagnostic_name = stage.name().to_owned();
    let completed = stage
        .complete()
        .expect("complete the stage under the writer lease");
    assert_eq!(completed.identity(), candidate.identity());
    assert_eq!(completed.name(), diagnostic_name);

    let versions = trust.installation.update_root.join("versions");
    let staged = versions.join(&diagnostic_name);
    assert!(
        !versions.join(&candidate.identity().version).exists(),
        "completion never renames the stage to a version"
    );
    assert_eq!(
        std::fs::read(staged.join("content.tar")).expect("retained authenticated archive"),
        GOLDEN
    );
    let complete = crate::records::decode_complete(
        &std::fs::read(staged.join(".complete")).expect("read completion record"),
    )
    .expect("canonical completion record");
    assert_eq!(complete.artifact, *candidate.identity());
    assert_eq!(complete.content_size, GOLDEN.len() as u64);
    keld_guard::validate_windows_owner_private_directory(&support::directory(&staged))
        .expect("the completed stage retains the exact owner-private descriptor");

    let after = ["version-floor", "current", "last-known-good"]
        .map(|name| std::fs::read(update.join(name)).expect("read active record after completion"));
    assert_eq!(
        after, before,
        "completing a stage cannot select a candidate"
    );
    assert!(!update.join("previous-known-good").exists());
    assert!(!update.join("activation-journal").exists());
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "the exclusive writer lease remains held by the extraction root"
    );

    drop(root);
    drop(
        load_windows_activation_write_snapshot(&trust, &verifier)
            .expect("a completed stage is a tolerated leftover, never a selectable version"),
    );
}

#[test]
fn per_user_version_publication_refuses_windows_case_alias_without_writing_marker() {
    let fixture = tempfile::tempdir().expect("version-name collision fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted per-user verifier");
    let baseline = load_windows_baseline(&trust).expect("load exact baseline");
    let observation = baseline.observation().clone();
    drop(baseline);
    let candidate = higher_release_version(&verifier, &observation, "2.0.0-alpha");
    let source = fixture.path().join("candidate.tar");
    std::fs::write(&source, GOLDEN).expect("write verified candidate source");

    let snapshot = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect("acquire exact per-user activation writer");
    let mut root = snapshot
        .open_extraction_root()
        .expect("retain the exclusive writer lease");
    let alias = trust
        .installation
        .update_root
        .join("versions")
        .join("2.0.0-ALPHA");
    std::fs::create_dir(&alias).expect("plant a case alias under the writer lease");
    std::fs::write(alias.join("sentinel"), b"preserve me").expect("write collision sentinel");

    let stage = root
        .extract(&candidate, &source)
        .expect("extract candidate with a case-alias target");
    let stage_path = trust
        .installation
        .update_root
        .join("versions")
        .join(stage.name());
    let error = stage
        .complete()
        .expect_err("Windows ordinal case alias must refuse before the completion marker");
    assert_eq!(error.code(), "KELD-UPDATE-015");
    assert!(matches!(
        error,
        crate::UpdateError::VersionPublication {
            outcome: crate::VersionPublicationOutcome::StageRetained,
            ..
        }
    ));
    assert!(!stage_path.join(".complete").exists());
    assert_eq!(
        std::fs::read(alias.join("sentinel")).expect("collision sentinel preserved"),
        b"preserve me"
    );
}

/// The completion marker is the stage's last write: a crash after it leaves only a
/// tolerated stage. Every later cut belongs to the journaled activation transaction.
#[test]
fn a_crash_after_the_completion_marker_leaves_only_an_unselected_stage() {
    support::assert_user_principal_token();
    let fixture = tempfile::tempdir().expect("stage completion crash-cut fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted per-user verifier");
    let source = fixture.path().join("candidate.tar");
    std::fs::write(&source, GOLDEN).expect("write verified candidate source");
    let update = &trust.installation.update_root;
    let before = ["version-floor", "current", "last-known-good"]
        .map(|name| std::fs::read(update.join(name)).expect("read active record before cut"));

    let marker =
        run_version_publication_crash_child(fixture.path(), &trust.volume_guid, "complete-marker");
    let stage_name = marker
        .split_once(" stage=")
        .expect("child reports the exact diagnostic stage")
        .1
        .to_owned();
    let versions = update.join("versions");
    assert!(versions.join(&stage_name).join(".complete").is_file());
    assert!(
        !versions.join("2.0.0").exists(),
        "completion never publishes the final version name"
    );
    drop(
        load_windows_activation_write_snapshot(&trust, &verifier)
            .expect("a completed stage does not halt or select anything"),
    );
    let after = ["version-floor", "current", "last-known-good"]
        .map(|name| std::fs::read(update.join(name)).expect("read active record after cut"));
    assert_eq!(
        after, before,
        "the completion cut cannot mutate active records"
    );
    assert!(!update.join("previous-known-good").exists());
    assert!(!update.join("activation-journal").exists());
}
