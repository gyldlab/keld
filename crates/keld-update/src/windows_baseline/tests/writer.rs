//! Per-user production writer admission over real Windows handles.

#![allow(unsafe_code)] // Test-only adoption of the exact remote handle returned by DuplicateHandle.

use std::fs::OpenOptions;
use std::io::Cursor;
use std::io::{BufRead as _, BufReader, Write as _};
use std::ops::{Deref, DerefMut};
use std::os::windows::io::FromRawHandle as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::support::{self, GOLDEN};
use crate::records::{ActivationJournal, ActivationPhase};
use crate::tests::{digest_hex, manifest_json, release_json, sign};
use crate::windows_baseline::{
    WindowsBaselineTrust, load_windows_activation_write_snapshot, load_windows_baseline,
    load_windows_recovery_inspection,
};
use crate::windows_extraction::{StageProtection, open_source, populate_stage};
use crate::windows_fs::{
    create_directory_relative_with_profile, create_file_relative_exclusive_with_profile,
    create_file_relative_with_profile, qualified_volume_root,
};
use keld_ipc::{
    WindowsLifecycleBinding, WindowsLifecycleExpectation, WindowsLifecyclePurpose,
    WindowsLifecycleRendezvousListener, WindowsNamedPipeBootstrapStream,
    connect_windows_lifecycle_rendezvous_until,
};

const LIFECYCLE_HELPER_ENV: &str = "KELD_WINDOWS_LEASE_KEEPER_HELPER";
const LIFECYCLE_HELPER_TEST: &str =
    "windows_baseline::tests::writer::windows_lifecycle_process_helper";
const RECOVERY_COMPOSITION_HELPER_ENV: &str = "KELD_WINDOWS_RECOVERY_COMPOSITION_HELPER";
const RECOVERY_COMPOSITION_HELPER_TEST: &str =
    "windows_baseline::tests::writer::windows_recovery_composition_process_helper";
const VERSION_PUBLICATION_HELPER_ENV: &str = "KELD_VERSION_PUBLICATION_CRASH_CUT";
const VERSION_PUBLICATION_HELPER_TEST: &str =
    "windows_baseline::tests::writer::windows_version_publication_crash_helper";
const VERSION_PUBLICATION_VOLUME_ENV: &str = "KELD_VERSION_PUBLICATION_VOLUME";

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
fn per_user_writer_publishes_complete_version_without_selecting_it() {
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
    let published = stage
        .publish_version()
        .expect("publish complete immutable version under the writer lease");
    assert_eq!(&published, candidate.identity());

    let version = trust
        .installation
        .update_root
        .join("versions")
        .join(&published.version);
    assert!(version.is_dir());
    assert!(!version.join("incomplete").exists());
    assert!(
        !trust
            .installation
            .update_root
            .join("versions")
            .join(&diagnostic_name)
            .exists()
    );
    assert_eq!(
        std::fs::read(version.join("content.tar")).expect("retained authenticated archive"),
        GOLDEN
    );
    let complete = crate::records::decode_complete(
        &std::fs::read(version.join(".complete")).expect("read completion record"),
    )
    .expect("canonical completion record");
    assert_eq!(complete.artifact, *candidate.identity());
    assert_eq!(complete.content_size, GOLDEN.len() as u64);
    keld_guard::validate_windows_owner_private_directory(&support::directory(&version))
        .expect("renamed version retains the exact owner-private descriptor");

    let after = ["version-floor", "current", "last-known-good"]
        .map(|name| std::fs::read(update.join(name)).expect("read active record after publish"));
    assert_eq!(
        after, before,
        "version publication cannot select a candidate"
    );
    assert!(!update.join("previous-known-good").exists());
    assert!(!update.join("activation-journal").exists());
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "the exclusive writer lease remains held by the extraction root"
    );

    drop(root);
    let reopen_error = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect_err("an unjournaled complete version is never selected by directory presence");
    assert!(
        reopen_error
            .to_string()
            .contains("unreferenced version entry"),
        "unreferenced candidate must fail closed: {reopen_error}"
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
        .publish_version()
        .expect_err("Windows ordinal case alias must refuse before marker or rename");
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

#[test]
fn version_publication_reports_unconfirmed_effect_after_rename() {
    let fixture = tempfile::tempdir().expect("post-rename publication failure fixture");
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
        .map(|name| std::fs::read(update.join(name)).expect("read active record before cut"));

    let snapshot = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect("acquire exact per-user activation writer");
    let mut root = snapshot
        .open_extraction_root()
        .expect("retain the exclusive writer lease");
    let stage = root
        .extract(&candidate, &source)
        .expect("extract verified candidate");
    let mut destination_tampered = false;
    let error = stage
        .publish_version_with_observer(|boundary, _| {
            if boundary
                == crate::windows_extraction::VersionPublicationBoundary::VersionDirectoryPublished
            {
                std::fs::write(
                    update.join("versions").join("2.0.0").join("content.tar"),
                    b"tampered after rename",
                )?;
                destination_tampered = true;
            }
            Ok(())
        })
        .expect_err("final readback must reject content changed after publication");
    assert_eq!(error.code(), "KELD-UPDATE-015");
    assert!(
        destination_tampered,
        "the observer mutates only after rename"
    );
    assert!(
        error.to_string().contains("final version readback failed"),
        "the refusal comes from final version validation: {error}"
    );
    assert!(matches!(
        error,
        crate::UpdateError::VersionPublication {
            outcome: crate::VersionPublicationOutcome::DestinationUnconfirmed,
            ..
        }
    ));
    let version = update.join("versions").join("2.0.0");
    assert!(version.join(".complete").is_file());
    assert_eq!(
        std::fs::read(version.join("content.tar")).expect("read mutated destination"),
        b"tampered after rename",
        "the final readback test must actually corrupt the published archive"
    );
    let after = ["version-floor", "current", "last-known-good"]
        .map(|name| std::fs::read(update.join(name)).expect("read active record after cut"));
    assert_eq!(after, before, "publication cannot mutate active records");
    assert!(!update.join("previous-known-good").exists());
    assert!(!update.join("activation-journal").exists());
    drop(root);
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "unconfirmed unjournaled version must stay fail-closed"
    );
}

#[test]
fn per_user_version_publication_crash_cuts_leave_only_unselected_artifacts() {
    support::assert_ordinary_token();
    for cut in ["complete-marker", "version-directory"] {
        let fixture = tempfile::tempdir().expect("version publication crash-cut fixture");
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

        let marker = run_version_publication_crash_child(fixture.path(), &trust.volume_guid, cut);
        let stage_name = marker
            .split_once(" stage=")
            .expect("child reports the exact diagnostic stage")
            .1
            .to_owned();
        let versions = update.join("versions");
        let stage = versions.join(&stage_name);
        let candidate = versions.join("2.0.0");
        match cut {
            "complete-marker" => {
                assert!(stage.join(".complete").is_file());
                assert!(
                    !candidate.exists(),
                    "crash before rename cannot publish the final version name"
                );
                drop(
                    load_windows_activation_write_snapshot(&trust, &verifier)
                        .expect("incomplete diagnostic stages do not select a candidate"),
                );
            }
            "version-directory" => {
                assert!(!stage.exists(), "the rename consumes the diagnostic leaf");
                assert!(candidate.join(".complete").is_file());
                let error = load_windows_activation_write_snapshot(&trust, &verifier)
                    .expect_err("an unjournaled version refuses recovery and selection");
                assert!(
                    error.to_string().contains("unreferenced version entry"),
                    "the directory alone cannot authorize selection: {error}"
                );
            }
            _ => unreachable!("parent supplies a closed cut selector"),
        }
        let after = ["version-floor", "current", "last-known-good"]
            .map(|name| std::fs::read(update.join(name)).expect("read active record after cut"));
        assert_eq!(
            after, before,
            "the publication cut cannot mutate active records"
        );
        assert!(!update.join("previous-known-good").exists());
        assert!(!update.join("activation-journal").exists());
    }
}

#[test]
#[ignore = "private immutable-version publication crash-cut subprocess entry point"]
fn windows_version_publication_crash_helper() {
    support::assert_ordinary_token();
    let root =
        PathBuf::from(std::env::var_os("KELD_VERSION_PUBLICATION_ROOT").expect("fixture root"));
    let cut = std::env::var(VERSION_PUBLICATION_HELPER_ENV).expect("publication cut");
    assert!(matches!(
        cut.as_str(),
        "complete-marker" | "version-directory"
    ));
    let install = root.join("KeldPerUserFixture");
    let mut trust = support::trust_for(&install);
    trust.installation.install_mode = crate::DirectInstallMode::PerUserDirect;
    trust.volume_guid = std::env::var(VERSION_PUBLICATION_VOLUME_ENV).expect("fixture volume GUID");
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted per-user verifier");
    let baseline = load_windows_baseline(&trust).expect("load exact baseline");
    let observation = baseline.observation().clone();
    drop(baseline);
    let candidate = higher_release(&verifier, &observation);
    let snapshot = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect("acquire the writer lease in the child process");
    let mut extraction = snapshot
        .open_extraction_root()
        .expect("retain the transaction owner in the child process");
    let stage = extraction
        .extract(&candidate, &root.join("candidate.tar"))
        .expect("extract exact candidate before the requested crash cut");
    let _ = stage.publish_version_with_observer(|boundary, stage_name| {
        let requested = matches!(
            (cut.as_str(), boundary),
            (
                "complete-marker",
                crate::windows_extraction::VersionPublicationBoundary::CompleteMarkerPublished
            ) | (
                "version-directory",
                crate::windows_extraction::VersionPublicationBoundary::VersionDirectoryPublished
            )
        );
        if requested {
            println!("KELD_VERSION_PUBLICATION_CUT={cut} stage={stage_name}");
            std::io::stdout()
                .flush()
                .expect("flush exact crash-boundary witness");
            std::process::exit(91);
        }
        Ok(())
    });
    panic!("the requested immutable-version publication cut was not reached");
}

fn run_version_publication_crash_child(root: &Path, volume: &str, cut: &str) -> String {
    let mut child = ChildReaper::new(
        Command::new(std::env::current_exe().expect("current test executable"))
            .args([
                "--exact",
                VERSION_PUBLICATION_HELPER_TEST,
                "--ignored",
                "--nocapture",
            ])
            .env("KELD_VERSION_PUBLICATION_ROOT", root)
            .env(VERSION_PUBLICATION_VOLUME_ENV, volume)
            .env(VERSION_PUBLICATION_HELPER_ENV, cut)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn version publication crash-cut child"),
    );
    let stdout = child.stdout.take().expect("child stdout");
    let (line_tx, line_rx) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let mut lines = BufReader::new(stdout).lines();
        let marker = lines.find_map(|line| match line {
            Ok(line) if line.starts_with("KELD_VERSION_PUBLICATION_CUT=") => Some(Ok(line)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        });
        let _ = line_tx.send(marker);
        for line in lines {
            if line.is_err() {
                break;
            }
        }
    });
    let marker = line_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("child reaches the named publication boundary")
        .expect("read child stdout")
        .expect("child writes boundary marker");
    let status = child.wait().expect("reap crash-cut child");
    reader.join().expect("join crash-cut stdout reader");
    assert_eq!(
        status.code(),
        Some(91),
        "child must exit at the requested cut"
    );
    assert!(marker.contains(&format!("KELD_VERSION_PUBLICATION_CUT={cut}")));
    marker
}

#[test]
fn recovery_inspection_retains_writer_lease_and_preserves_pending_state() {
    let fixture = tempfile::tempdir().expect("per-user recovery inspection fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (attempt_id, lifecycle_channel_id) =
        seed_pending_activation_journal(&trust, ActivationPhase::PublishPending);
    let update_root = &trust.installation.update_root;
    let journal_path = update_root.join("activation-journal");
    let journal_before = std::fs::read(&journal_path).expect("pending journal bytes");
    let pointer_names = ["version-floor", "current", "last-known-good"];
    let pointers_before = pointer_names
        .map(|name| std::fs::read(update_root.join(name)).expect("protected pointer bytes"));

    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("read-only recovery inspection accepts consistent protected pending journal");
    assert_eq!(inspection.identity(), &trust.installation);
    assert_eq!(
        inspection.lifecycle_installation_id(),
        &trust
            .lifecycle_installation_id()
            .expect("trusted installation binding")
    );
    assert_eq!(inspection.attempt_id(), &attempt_id);
    assert_eq!(inspection.lifecycle_channel_id(), &lifecycle_channel_id);
    assert_eq!(inspection.version_floor(), "1.0.0");
    assert_eq!(inspection.current().version, "1.0.0");
    assert_eq!(inspection.last_known_good().version, "1.0.0");
    assert!(inspection.previous_known_good().is_none());
    assert!(
        OpenOptions::new()
            .read(true)
            .open(update_root.join("activation.lock"))
            .is_err(),
        "inspection retains the exclusive writer lease during witness comparison"
    );
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "the ordinary writer loader still refuses pending journals"
    );
    assert_eq!(
        std::fs::read(&journal_path).expect("journal while inspection lives"),
        journal_before,
        "inspection cannot change the activation journal"
    );
    for (name, before) in pointer_names.into_iter().zip(&pointers_before) {
        assert_eq!(
            std::fs::read(update_root.join(name)).expect("pointer while inspection lives"),
            *before,
            "inspection cannot change {name}"
        );
    }
    drop(inspection);
    assert!(
        OpenOptions::new()
            .read(true)
            .open(update_root.join("activation.lock"))
            .is_ok(),
        "dropping the inspection releases its exact writer lease"
    );
    assert_eq!(
        std::fs::read(&journal_path).expect("journal after inspection drop"),
        journal_before
    );
    for (name, before) in pointer_names.into_iter().zip(&pointers_before) {
        assert_eq!(
            std::fs::read(update_root.join(name)).expect("pointer after inspection drop"),
            *before
        );
    }
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "the ordinary writer loader must continue refusing pending recovery after inspection"
    );
    std::fs::write(&journal_path, b"malformed-journal")
        .expect("install malformed protected journal negative control");
    assert!(
        load_windows_recovery_inspection(&trust, &verifier).is_err(),
        "malformed journal bytes must not produce a recovery inspection"
    );
    let impossible_history = String::from_utf8(journal_before.clone())
        .expect("canonical journal UTF-8")
        .replace(r#""prior_floor":"1.0.0""#, r#""prior_floor":"0.5.0""#);
    assert_ne!(impossible_history.as_bytes(), journal_before);
    std::fs::write(&journal_path, impossible_history)
        .expect("install journal with an impossible historical floor");
    assert!(
        load_windows_recovery_inspection(&trust, &verifier).is_err(),
        "prior floor below the recorded rollback/LKG artifacts must refuse inspection"
    );
    std::fs::write(&journal_path, &journal_before).expect("restore exact pending journal");
}

#[test]
fn recovery_inspection_accepts_candidate_selected_while_health_is_pending() {
    let fixture = tempfile::tempdir().expect("awaiting-health recovery fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (attempt_id, lifecycle_channel_id) =
        seed_pending_activation_journal(&trust, ActivationPhase::AwaitingHealth);

    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("valid awaiting-health cut has candidate current and prior LKG");
    assert_eq!(inspection.attempt_id(), &attempt_id);
    assert_eq!(inspection.lifecycle_channel_id(), &lifecycle_channel_id);
    assert_eq!(inspection.version_floor(), "2.0.0");
    assert_eq!(inspection.current().version, "2.0.0");
    assert_eq!(inspection.last_known_good().version, "1.0.0");
    assert!(inspection.previous_known_good().is_none());
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "ordinary writer admission continues to refuse this pending transaction"
    );
}

#[test]
fn recovery_inspection_accepts_health_commit_cut_with_previous_published_first() {
    let fixture = tempfile::tempdir().expect("health commit-cut fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (attempt_id, lifecycle_channel_id) = seed_pending_activation_journal(
        &trust,
        ActivationPhase::HealthAccepted {
            health_receipt_digest: [0x99; 32],
        },
    );
    let baseline = trust.installation.baseline.clone();
    let update = support::directory(&trust.installation.update_root);
    write_profiled_record(
        &update,
        "previous-known-good",
        &crate::records::encode_pointer(crate::records::PointerKind::PreviousKnownGood, &baseline)
            .expect("previous known-good intermediate pointer"),
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    );

    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("health-accepted journal accepts its durable previous-pointer cut");
    assert_eq!(inspection.attempt_id(), &attempt_id);
    assert_eq!(inspection.lifecycle_channel_id(), &lifecycle_channel_id);
    assert_eq!(inspection.version_floor(), "2.0.0");
    assert_eq!(inspection.current().version, "2.0.0");
    assert_eq!(inspection.last_known_good().version, "1.0.0");
    assert_eq!(
        inspection.previous_known_good(),
        Some(&trust.installation.baseline)
    );
}

#[test]
fn recovery_inspection_accepts_health_commit_cut_after_lkg_publication() {
    let fixture = tempfile::tempdir().expect("health commit-final-cut fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (_, _) = seed_pending_activation_journal(
        &trust,
        ActivationPhase::HealthAccepted {
            health_receipt_digest: [0x9a; 32],
        },
    );
    let baseline = trust.installation.baseline.clone();
    let update = support::directory(&trust.installation.update_root);
    write_profiled_record(
        &update,
        "previous-known-good",
        &crate::records::encode_pointer(crate::records::PointerKind::PreviousKnownGood, &baseline)
            .expect("previous known-good pointer"),
        keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate,
    );
    let mut candidate = baseline.clone();
    candidate.version = "2.0.0".to_owned();
    candidate.content_blake3 = *blake3::hash(GOLDEN).as_bytes();
    std::fs::write(
        trust.installation.update_root.join("last-known-good"),
        crate::records::encode_pointer(crate::records::PointerKind::LastKnownGood, &candidate)
            .expect("candidate LKG pointer"),
    )
    .expect("publish candidate LKG pointer in fixture");

    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("health-accepted journal accepts the durable candidate-LKG commit cut");
    assert_eq!(inspection.version_floor(), "2.0.0");
    assert_eq!(inspection.current().version, "2.0.0");
    assert_eq!(inspection.last_known_good().version, "2.0.0");
    assert_eq!(inspection.previous_known_good(), Some(&baseline));
}

#[test]
fn recovery_inspection_accepts_rollback_pending_before_and_after_current_replacement() {
    for current_candidate in [true, false] {
        let fixture = tempfile::tempdir().expect("rollback crash-cut fixture");
        let trust = seed_per_user_baseline(fixture.path());
        let verifier = crate::UpdateVerifier::new(
            trust.installation.clone(),
            crate::tests::signing_key().verifying_key().to_bytes(),
        )
        .expect("trusted test verifier");
        seed_pending_activation_journal(
            &trust,
            ActivationPhase::RollbackPending {
                failure: crate::records::ActivationFailureClass::HealthRejected,
            },
        );
        let expected_current = if current_candidate {
            let mut candidate = trust.installation.baseline.clone();
            candidate.version = "2.0.0".to_owned();
            candidate.content_blake3 = *blake3::hash(GOLDEN).as_bytes();
            candidate
        } else {
            let baseline = trust.installation.baseline.clone();
            std::fs::write(
                trust.installation.update_root.join("current"),
                crate::records::encode_pointer(crate::records::PointerKind::Current, &baseline)
                    .expect("rollback-target current pointer"),
            )
            .expect("publish rollback target in fixture");
            baseline
        };

        let inspection = load_windows_recovery_inspection(&trust, &verifier)
            .expect("rollback-pending journal accepts each recorded current-pointer cut");
        assert_eq!(inspection.version_floor(), "2.0.0");
        assert_eq!(inspection.current(), &expected_current);
        assert_eq!(inspection.last_known_good(), &trust.installation.baseline);
        assert!(inspection.previous_known_good().is_none());
    }
}

#[test]
fn recovery_inspection_rejects_superseded_lkg_without_historical_previous() {
    let fixture = tempfile::tempdir().expect("missing historical previous fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    seed_pending_activation_journal(&trust, ActivationPhase::PublishPending);

    let journal_path = trust.installation.update_root.join("activation-journal");
    let mut journal = crate::records::decode_activation_journal(
        &std::fs::read(&journal_path).expect("seed pending journal"),
    )
    .expect("canonical seeded journal");
    let prior_lkg = journal.candidate.clone();
    journal.prior_floor = prior_lkg.version.clone();
    journal.prior_last_known_good = prior_lkg.clone();
    journal.rollback_target = prior_lkg.clone();
    journal.candidate.version = "3.0.0".to_owned();
    let journal_bytes = crate::records::encode_activation_journal(&journal)
        .expect("journal with valid floor but missing required history");
    std::fs::write(journal_path, journal_bytes).expect("write history-gap journal");

    let update_root = &trust.installation.update_root;
    std::fs::write(
        update_root.join("version-floor"),
        crate::records::encode_floor(&prior_lkg.version).expect("prior floor"),
    )
    .expect("write prior floor");
    std::fs::write(
        update_root.join("current"),
        crate::records::encode_pointer(crate::records::PointerKind::Current, &prior_lkg)
            .expect("current pointer"),
    )
    .expect("write current pointer");
    std::fs::write(
        update_root.join("last-known-good"),
        crate::records::encode_pointer(crate::records::PointerKind::LastKnownGood, &prior_lkg)
            .expect("LKG pointer"),
    )
    .expect("write LKG pointer");

    let error = load_windows_recovery_inspection(&trust, &verifier)
        .expect_err("journal cannot omit the older known-good artifact after baseline moved");
    assert!(
        matches!(
            error,
            crate::UpdateError::Baseline {
                step: "activation journal previous-known-good history",
                ..
            }
        ),
        "history gap has a direct refusal, got {error:?}"
    );
}

#[test]
fn qf1_witness_composes_with_recovery_inspection_under_the_reacquired_lease() {
    for substitution in [
        QfBindingSubstitution::None,
        QfBindingSubstitution::Attempt,
        QfBindingSubstitution::Channel,
    ] {
        run_qf1_recovery_composition_case(substitution);
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one Windows crash cut proves coordinator/keeper loss, family exit, writer reacquisition and fail-closed byte preservation"
)]
fn lost_coordinator_and_keeper_leave_pending_journal_refusing_ordinary_writer() {
    let fixture = tempfile::tempdir().expect("all-owners-lost fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (attempt_id, channel_id) =
        seed_pending_activation_journal(&trust, ActivationPhase::PublishPending);
    let installation_id = trust
        .lifecycle_installation_id()
        .expect("canonical trusted install binding");
    let binding = WindowsLifecycleBinding::new(
        installation_id,
        attempt_id,
        channel_id,
        WindowsLifecyclePurpose::CoordinatorToKeeper,
    )
    .expect("all-owners-lost test binding");
    let endpoint =
        WindowsNamedPipeBootstrapStream::endpoint_for_lifecycle_install(&installation_id);
    let journal_path = trust.installation.update_root.join("activation-journal");
    let journal_before = std::fs::read(&journal_path).expect("journal before owner loss");
    let pointer_names = ["version-floor", "current", "last-known-good"];
    let pointers_before = pointer_names.map(|name| {
        std::fs::read(trust.installation.update_root.join(name)).expect("pointer bytes")
    });

    let mut coordinator = ChildReaper::new(spawn_recovery_composition_process(
        "coordinator",
        &trust,
        binding,
        &endpoint,
        None,
    ));
    let mut coordinator_lines =
        BufReader::new(coordinator.stdout.take().expect("coordinator stdout")).lines();
    let ready = next_prefixed_line(&mut coordinator_lines, "QF1_COORDINATOR_READY");
    let member_pid = ready
        .strip_prefix("QF1_COORDINATOR_READY member=")
        .expect("coordinator exposes exact Job-member PID for the test oracle")
        .parse::<u32>()
        .expect("valid member PID");
    let mut coordinator_session = 0_u32;
    // SAFETY: coordinator PID comes from the live Child; the output is writable.
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                coordinator.id(),
                &raw mut coordinator_session,
            )
        },
        0
    );
    let member_peer =
        keld_runtime::windows_job::WindowsProcessPeer::open(member_pid, coordinator_session)
            .expect("pin exact attempt member before coordinator and keeper loss");
    assert!(
        !member_peer.has_exited().expect("query live attempt member"),
        "candidate family is live before both owners are killed"
    );
    let mut keeper = ChildReaper::new(spawn_recovery_composition_process(
        "keeper",
        &trust,
        binding,
        &endpoint,
        Some((coordinator.id(), coordinator_session)),
    ));
    let mut keeper_lines = BufReader::new(keeper.stdout.take().expect("keeper stdout")).lines();
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "QF1_KEEPER_READY"),
        "QF1_KEEPER_READY"
    );
    assert!(
        OpenOptions::new()
            .read(true)
            .open(trust.installation.update_root.join("activation.lock"))
            .is_err(),
        "keeper retains share-zero exclusion before its own death"
    );

    coordinator.kill().expect("kill coordinator before QF1");
    assert!(
        !coordinator.wait().expect("wait coordinator loss").success(),
        "the coordinator must be lost before the keeper"
    );
    keeper.kill().expect("kill keeper before successor/QF1");
    assert!(
        !keeper.wait().expect("wait keeper loss").success(),
        "no successor retirement receipt may exist after keeper loss"
    );
    assert!(
        member_peer
            .wait_until_exited(Duration::from_secs(10))
            .expect("wait on exact member after Job close"),
        "loss of the final Job handle reaps the KILL_ON_JOB_CLOSE family"
    );
    assert!(
        OpenOptions::new()
            .read(true)
            .open(trust.installation.update_root.join("activation.lock"))
            .is_ok(),
        "both owner processes are gone, so no live lease handle remains"
    );
    let refusal = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect_err("pending journal must refuse without authenticated QF1 witness");
    assert!(
        matches!(
            refusal,
            crate::UpdateError::Baseline {
                step: "activation recovery",
                ..
            }
        ),
        "refusal occurs after taking the free lease, not because another owner remains: {refusal:?}"
    );
    assert_eq!(
        std::fs::read(&journal_path).expect("journal after owner loss"),
        journal_before,
        "all-owners-lost recovery preserves the pending journal"
    );
    for (name, before) in pointer_names.into_iter().zip(&pointers_before) {
        assert_eq!(
            std::fs::read(trust.installation.update_root.join(name))
                .expect("pointer after owner loss"),
            *before,
            "all-owners-lost recovery preserves {name}"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QfBindingSubstitution {
    None,
    Attempt,
    Channel,
}

#[expect(
    clippy::too_many_lines,
    reason = "one native fixture proves QF1 and protected recovery-context equality across coordinator, keeper and successor"
)]
fn run_qf1_recovery_composition_case(substitution: QfBindingSubstitution) {
    let fixture = tempfile::tempdir().expect("QF1/recovery composition fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let (attempt_id, channel_id) =
        seed_pending_activation_journal(&trust, ActivationPhase::PublishPending);
    let installation_id = trust
        .lifecycle_installation_id()
        .expect("canonical trusted install binding");
    let bound_attempt = if substitution == QfBindingSubstitution::Attempt {
        [0xe1; 32]
    } else {
        attempt_id
    };
    let bound_channel = if substitution == QfBindingSubstitution::Channel {
        [0xe2; 32]
    } else {
        channel_id
    };
    let binding = WindowsLifecycleBinding::new(
        installation_id,
        bound_attempt,
        bound_channel,
        WindowsLifecyclePurpose::CoordinatorToKeeper,
    )
    .expect("exact coordinator-to-keeper test binding");
    let locator = installation_id;
    let endpoint = WindowsNamedPipeBootstrapStream::endpoint_for_lifecycle_install(&locator);
    let expected_image = std::env::current_exe()
        .expect("test executable")
        .canonicalize()
        .expect("canonical test image");
    let journal_path = trust.installation.update_root.join("activation-journal");
    let journal_before = std::fs::read(&journal_path).expect("protected journal bytes");
    let pointer_names = ["version-floor", "current", "last-known-good"];
    let pointers_before = pointer_names.map(|name| {
        std::fs::read(trust.installation.update_root.join(name)).expect("pointer bytes")
    });

    let mut coordinator = ChildReaper::new(spawn_recovery_composition_process(
        "coordinator",
        &trust,
        binding,
        &endpoint,
        None,
    ));
    let mut coordinator_lines =
        BufReader::new(coordinator.stdout.take().expect("coordinator stdout")).lines();
    let coordinator_ready = next_prefixed_line(&mut coordinator_lines, "QF1_COORDINATOR_READY");
    let member_pid = coordinator_ready
        .strip_prefix("QF1_COORDINATOR_READY member=")
        .expect("coordinator publishes the exact Job member PID")
        .parse::<u32>()
        .expect("valid member PID");
    let mut coordinator_session = 0_u32;
    // SAFETY: the coordinator PID comes from this live Child, and the output is writable.
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                coordinator.id(),
                &raw mut coordinator_session,
            )
        },
        0
    );
    let member_peer =
        keld_runtime::windows_job::WindowsProcessPeer::open(member_pid, coordinator_session)
            .expect("pin candidate-family member while both owners live");
    assert!(
        !member_peer.has_exited().expect("query live member"),
        "attempt member must still be alive before owner-loss controls"
    );
    let mut keeper = ChildReaper::new(spawn_recovery_composition_process(
        "keeper",
        &trust,
        binding,
        &endpoint,
        Some((coordinator.id(), coordinator_session)),
    ));
    let mut keeper_lines = BufReader::new(keeper.stdout.take().expect("keeper stdout")).lines();
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "QF1_KEEPER_READY"),
        "QF1_KEEPER_READY"
    );
    assert!(
        OpenOptions::new()
            .read(true)
            .open(trust.installation.update_root.join("activation.lock"))
            .is_err(),
        "keeper retains share-zero writer exclusion before coordinator death"
    );

    coordinator
        .kill()
        .expect("kill exact coordinator process after keeper adoption");
    assert!(
        !coordinator
            .wait()
            .expect("wait coordinator death")
            .success(),
        "coordinator helper must die before keeper retirement"
    );
    writeln!(keeper.stdin.as_mut().expect("keeper stdin"), "RETIRE")
        .expect("authorize keeper retirement phase");
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "QF1_KEEPER_LISTENER_READY"),
        "QF1_KEEPER_LISTENER_READY"
    );

    let mut keeper_session = 0_u32;
    // SAFETY: keeper is live and its PID is returned by the exact Child owner.
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                keeper.id(),
                &raw mut keeper_session,
            )
        },
        0
    );
    let connection = connect_windows_lifecycle_rendezvous_until(
        &endpoint,
        WindowsLifecycleExpectation::from_keeper(installation_id)
            .expect("cold successor independently expects install identity"),
        Instant::now() + Duration::from_secs(10),
        |pid, session| {
            if pid != keeper.id() || session != keeper_session {
                return None;
            }
            let peer = keld_runtime::windows_job::WindowsProcessPeer::open(pid, session).ok()?;
            if peer.image_path().canonicalize().ok()? != expected_image
                || peer.token_facts().session_id != session
            {
                return None;
            }
            Some(peer)
        },
    )
    .expect("authenticate keeper and receive the exact-attempt QF1 channel");
    let retirement = keld_runtime::windows_job::WindowsLifecycleRetirementWitness::receive(
        connection,
        Instant::now() + Duration::from_secs(10),
    )
    .expect("QF1 is required before returning the retirement witness");
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "QF1_KEEPER_RETIRED"),
        "QF1_KEEPER_RETIRED"
    );
    assert_eq!(retirement.active_processes().expect("exact Job census"), 0);
    let retired_binding = retirement.binding();
    assert_eq!(retired_binding.installation_id(), &installation_id);
    assert_eq!(
        retired_binding.purpose(),
        WindowsLifecyclePurpose::KeeperToSuccessor
    );

    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("successor reacquires exclusive lease and reads protected recovery context");
    assert_eq!(
        inspection.lifecycle_installation_id(),
        retired_binding.installation_id()
    );
    assert_eq!(
        retired_binding.attempt_id() == inspection.attempt_id(),
        substitution != QfBindingSubstitution::Attempt,
        "wrong-attempt QF1 must not match the protected journal"
    );
    assert_eq!(
        retired_binding.lifecycle_channel_id() == inspection.lifecycle_channel_id(),
        substitution != QfBindingSubstitution::Channel,
        "wrong-channel QF1 must not match the protected journal"
    );
    assert_eq!(inspection.attempt_id(), &attempt_id);
    assert_eq!(inspection.lifecycle_channel_id(), &channel_id);
    assert!(
        OpenOptions::new()
            .read(true)
            .open(trust.installation.update_root.join("activation.lock"))
            .is_err(),
        "the inspection owns the reacquired share-zero lease while witness IDs are compared"
    );
    assert_eq!(
        std::fs::read(&journal_path).expect("journal during comparison"),
        journal_before,
        "QF1 composition and inspection are read-only"
    );
    for (name, before) in pointer_names.into_iter().zip(&pointers_before) {
        assert_eq!(
            std::fs::read(trust.installation.update_root.join(name))
                .expect("pointer during comparison"),
            *before,
            "QF1 composition cannot modify {name}"
        );
    }
    drop(inspection);
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "matching or mismatched QF1 cannot bypass the ordinary pending-journal refusal"
    );
    writeln!(keeper.stdin.as_mut().expect("keeper stdin"), "EXIT")
        .expect("release keeper helper after QF1 evidence capture");
    drop(keeper.stdin.take());
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "QF1_KEEPER_EXIT"),
        "QF1_KEEPER_EXIT"
    );
    assert!(keeper.wait().expect("wait keeper").success());
    assert_eq!(
        std::fs::read(&journal_path).expect("journal after composition"),
        journal_before
    );
    for (name, before) in pointer_names.into_iter().zip(&pointers_before) {
        assert_eq!(
            std::fs::read(trust.installation.update_root.join(name))
                .expect("pointer after composition"),
            *before
        );
    }
}

#[test]
fn reduced_activation_lease_transfer_preserves_share_zero_after_coordinator_drop() {
    let fixture = tempfile::tempdir().expect("per-user keeper lease fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");
    let writer = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect("acquire exact per-user writer lease");

    let mut attempt =
        keld_runtime::windows_job::WindowsProcessJob::create().expect("create unnamed attempt Job");
    let mut member = spawn_lifecycle_process_helper("member");
    attempt
        .assign_child(&member)
        .expect("assign exact attempt member");

    let mut keeper = spawn_lifecycle_process_helper("keeper");
    let keeper_stdout = keeper.stdout.take().expect("keeper stdout");
    let mut keeper_lines = BufReader::new(keeper_stdout).lines();
    let mut keeper_session = 0_u32;
    // SAFETY: keeper is live and its PID comes from Child.
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                keeper.id(),
                &raw mut keeper_session,
            )
        },
        0
    );
    let keeper_peer =
        keld_runtime::windows_job::WindowsProcessPeer::open(keeper.id(), keeper_session)
            .expect("pin exact keeper process");
    let local_retention = writer
        .duplicate_lifecycle_lease_retention()
        .expect("create reduced handle to exact share-zero lease");
    let remote_lease = attempt
        .transfer_activation_lease_handle_to(&keeper_peer, &local_retention)
        .expect("transfer reduced share-zero retention to exact keeper process");
    drop(local_retention);

    drop(writer);
    assert!(
        load_windows_baseline(&trust).is_err(),
        "remote keeper handle must retain installation-wide writer exclusion"
    );

    writeln!(
        keeper.stdin.as_mut().expect("keeper stdin"),
        "LEASE {remote_lease}"
    )
    .expect("deliver remote lease handle to one-shot keeper");
    let readiness = keeper_lines
        .find_map(|line| match line {
            Ok(line) if line.starts_with("LEASE_KEEPER_READY") => Some(Ok(line)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .unwrap_or_else(|| {
            let status = keeper.wait().expect("wait failed lease keeper");
            let stderr = keeper
                .stderr
                .take()
                .map(std::io::read_to_string)
                .transpose()
                .expect("read keeper error");
            panic!("lease keeper exited before readiness: status={status}; stderr={stderr:?}");
        })
        .expect("read keeper readiness");
    assert_eq!(readiness, "LEASE_KEEPER_READY");
    assert!(
        load_windows_baseline(&trust).is_err(),
        "keeper retains the exclusive open after the original writer drops"
    );
    writeln!(keeper.stdin.as_mut().expect("keeper stdin"), "EXIT")
        .expect("release keeper lease handle");
    drop(keeper.stdin.take());
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "LEASE_KEEPER_EXIT"),
        "LEASE_KEEPER_EXIT"
    );
    assert!(keeper.wait().expect("wait keeper").success());
    assert!(
        load_windows_baseline(&trust).is_ok(),
        "closing the final remote duplicate releases the writer exclusion"
    );
    attempt
        .terminate_and_wait(&member, std::time::Duration::from_secs(10))
        .expect("retire attempt Job after lease retention closes");
    let _ = member
        .wait()
        .expect("wait attempt member reaped by Job close");
}

#[test]
#[ignore = "private Windows keeper/member subprocess entry point"]
fn windows_lifecycle_process_helper() {
    match std::env::var(LIFECYCLE_HELPER_ENV).as_deref() {
        Ok("member") => std::thread::park(),
        Ok("keeper") => run_lease_keeper_helper(),
        other => panic!("unexpected lifecycle helper role {other:?}"),
    }
}

#[test]
#[ignore = "private QF1/recovery composition subprocess entry point"]
fn windows_recovery_composition_process_helper() {
    match std::env::var(RECOVERY_COMPOSITION_HELPER_ENV).as_deref() {
        Ok("coordinator") => run_recovery_composition_coordinator(),
        Ok("keeper") => run_recovery_composition_keeper(),
        other => panic!("unexpected recovery composition helper role {other:?}"),
    }
}

struct ChildReaper {
    child: Child,
    reap_on_drop: bool,
}

#[test]
fn unassigned_lifecycle_member_is_reaped_when_job_assignment_fails() {
    let child = spawn_lifecycle_process_helper("member");
    let mut session = 0_u32;
    // SAFETY: child is live and the session output is writable.
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                child.id(),
                &raw mut session,
            )
        },
        0
    );
    let observer = keld_runtime::windows_job::WindowsProcessPeer::open(child.id(), session)
        .expect("pin exact unassigned member process");
    drop(ChildReaper::new(child));
    assert!(
        observer.has_exited().expect("query exact member process"),
        "the assignment guard must terminate and reap a child that never joined the Job"
    );
}

impl ChildReaper {
    fn new(child: Child) -> Self {
        Self {
            child,
            reap_on_drop: true,
        }
    }

    fn disarm(&mut self) {
        self.reap_on_drop = false;
    }
}

impl Deref for ChildReaper {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl DerefMut for ChildReaper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

impl Drop for ChildReaper {
    fn drop(&mut self) {
        if self.reap_on_drop && !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn spawn_recovery_composition_process(
    role: &str,
    trust: &WindowsBaselineTrust,
    binding: WindowsLifecycleBinding,
    endpoint: &str,
    server: Option<(u32, u32)>,
) -> Child {
    let fixture_root = trust
        .installation
        .install_root
        .parent()
        .expect("install root parent")
        .to_owned();
    let image = std::env::current_exe()
        .expect("test executable")
        .canonicalize()
        .expect("canonical test image");
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command
        .args([
            "--exact",
            RECOVERY_COMPOSITION_HELPER_TEST,
            "--ignored",
            "--nocapture",
        ])
        .env(RECOVERY_COMPOSITION_HELPER_ENV, role)
        .env("KELD_QF1_FIXTURE_ROOT", fixture_root)
        .env("KELD_QF1_VOLUME_GUID", &trust.volume_guid)
        .env("KELD_QF1_ENDPOINT", endpoint)
        .env(
            "KELD_QF1_INSTALLATION_ID",
            crate::error::hex_digest(binding.installation_id()),
        )
        .env(
            "KELD_QF1_ATTEMPT_ID",
            crate::error::hex_digest(binding.attempt_id()),
        )
        .env(
            "KELD_QF1_CHANNEL_ID",
            crate::error::hex_digest(binding.lifecycle_channel_id()),
        )
        .env("KELD_QF1_SERVER_IMAGE", image.as_os_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some((pid, session)) = server {
        command
            .env("KELD_QF1_SERVER_PID", pid.to_string())
            .env("KELD_QF1_SERVER_SESSION", session.to_string());
    }
    command
        .spawn()
        .unwrap_or_else(|error| panic!("spawn QF1 {role} process fixture: {error}"))
}

fn run_recovery_composition_coordinator() {
    let fixture_root =
        std::path::PathBuf::from(std::env::var_os("KELD_QF1_FIXTURE_ROOT").expect("fixture root"));
    let install_root = fixture_root.join("KeldPerUserFixture");
    let mut trust = support::trust_for(&install_root);
    trust.installation.install_mode = crate::DirectInstallMode::PerUserDirect;
    trust.volume_guid = std::env::var("KELD_QF1_VOLUME_GUID").expect("fixture volume GUID");
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted per-user verifier");
    let inspection = load_windows_recovery_inspection(&trust, &verifier)
        .expect("coordinator admits protected pending journal and share-zero lease");
    let installation_id = parse_lifecycle_id("KELD_QF1_INSTALLATION_ID");
    let expected_attempt = parse_lifecycle_id("KELD_QF1_ATTEMPT_ID");
    let expected_channel = parse_lifecycle_id("KELD_QF1_CHANNEL_ID");
    assert_eq!(inspection.lifecycle_installation_id(), &installation_id);
    assert_ne!(expected_channel, [0; 32]);
    assert_ne!(expected_attempt, [0; 32]);
    let binding = WindowsLifecycleBinding::new(
        installation_id,
        expected_attempt,
        expected_channel,
        WindowsLifecyclePurpose::CoordinatorToKeeper,
    )
    .expect("exact coordinator-to-keeper binding");
    let retention = inspection
        .duplicate_lifecycle_lease_retention()
        .expect("retain this inspection's exact share-zero lease");
    let mut attempt = keld_runtime::windows_job::WindowsProcessJob::create()
        .expect("create exact unnamed attempt Job");
    let mut member = ChildReaper::new(spawn_lifecycle_process_helper("member"));
    attempt
        .assign_child(&member)
        .expect("assign exact attempt-family member");
    member.disarm();
    let locator = *inspection.lifecycle_installation_id();
    let listener = WindowsLifecycleRendezvousListener::bind(locator, binding)
        .expect("bind one-shot coordinator-to-keeper endpoint");
    let mut coordinator_session = 0_u32;
    // SAFETY: the current coordinator PID is live and the output is writable.
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                std::process::id(),
                &raw mut coordinator_session,
            )
        },
        0
    );
    println!("QF1_COORDINATOR_READY member={}", member.id());
    std::io::stdout()
        .flush()
        .expect("flush coordinator readiness");
    let expected_image = std::env::current_exe()
        .expect("test executable")
        .canonicalize()
        .expect("canonical keeper image");
    let accepted = listener
        .accept_until(
            Instant::now() + Duration::from_secs(10),
            |pid, session, facts| {
                if session != coordinator_session || facts.session_id != session {
                    return None;
                }
                let peer =
                    keld_runtime::windows_job::WindowsProcessPeer::open(pid, session).ok()?;
                if peer.image_path().canonicalize().ok()? != expected_image
                    || peer.token_facts() != facts
                {
                    return None;
                }
                Some(peer)
            },
        )
        .expect("authenticate exact keeper and finish fresh transcript")
        .expect("keeper connects before handoff deadline");
    attempt
        .transfer_lifecycle_attempt_handoff(
            accepted,
            &retention,
            Instant::now() + Duration::from_secs(10),
        )
        .expect("transfer exact Job/lease and receive keeper-adoption receipt");
    drop(retention);
    drop(inspection);
    drop(attempt);
    drop(member);
    println!("QF1_COORDINATOR_HANDED_OFF");
    std::io::stdout()
        .flush()
        .expect("flush coordinator handoff");
    std::thread::park();
}

fn run_recovery_composition_keeper() {
    let (binding, mut handoff) = receive_recovery_composition_handoff();
    println!("QF1_KEEPER_READY");
    std::io::stdout().flush().expect("flush keeper readiness");
    assert_eq!(
        BufReader::new(std::io::stdin())
            .lines()
            .next()
            .expect("retirement command")
            .expect("read retirement command"),
        "RETIRE"
    );
    retire_recovery_composition_handoff(binding, &mut handoff);
    assert_eq!(
        BufReader::new(std::io::stdin())
            .lines()
            .next()
            .expect("keeper exit command")
            .expect("read keeper exit"),
        "EXIT"
    );
    drop(handoff);
    println!("QF1_KEEPER_EXIT");
    std::io::stdout().flush().expect("flush keeper exit");
}

fn receive_recovery_composition_handoff() -> (
    WindowsLifecycleBinding,
    keld_runtime::windows_job::WindowsLifecycleKeeperHandoff,
) {
    let endpoint = std::env::var("KELD_QF1_ENDPOINT").expect("lifecycle endpoint");
    let server_pid = std::env::var("KELD_QF1_SERVER_PID")
        .expect("coordinator PID")
        .parse::<u32>()
        .expect("valid coordinator PID");
    let server_session = std::env::var("KELD_QF1_SERVER_SESSION")
        .expect("coordinator session")
        .parse::<u32>()
        .expect("valid coordinator session");
    let expected_image = std::path::PathBuf::from(
        std::env::var_os("KELD_QF1_SERVER_IMAGE").expect("coordinator image"),
    )
    .canonicalize()
    .expect("canonical coordinator image");
    let binding = WindowsLifecycleBinding::new(
        parse_lifecycle_id("KELD_QF1_INSTALLATION_ID"),
        parse_lifecycle_id("KELD_QF1_ATTEMPT_ID"),
        parse_lifecycle_id("KELD_QF1_CHANNEL_ID"),
        WindowsLifecyclePurpose::CoordinatorToKeeper,
    )
    .expect("exact keeper binding");
    let connection = connect_windows_lifecycle_rendezvous_until(
        &endpoint,
        WindowsLifecycleExpectation::exact(binding),
        Instant::now() + Duration::from_secs(10),
        |pid, session| {
            if pid != server_pid || session != server_session {
                return None;
            }
            let peer = keld_runtime::windows_job::WindowsProcessPeer::open(pid, session).ok()?;
            if peer.image_path().canonicalize().ok()? != expected_image
                || peer.token_facts().session_id != session
            {
                return None;
            }
            Some(peer)
        },
    )
    .expect("authenticate exact coordinator and complete rendezvous receipts");
    let handoff = keld_runtime::windows_job::WindowsLifecycleKeeperHandoff::receive_attempt_bundle(
        connection,
        Instant::now() + Duration::from_secs(10),
    )
    .expect("adopt exact attempt Job and reduced writer lease");
    assert_eq!(handoff.active_processes().expect("keeper Job query"), 1);
    (binding, handoff)
}

fn retire_recovery_composition_handoff(
    binding: WindowsLifecycleBinding,
    handoff: &mut keld_runtime::windows_job::WindowsLifecycleKeeperHandoff,
) {
    let successor_binding = binding.with_purpose(WindowsLifecyclePurpose::KeeperToSuccessor);
    let listener =
        WindowsLifecycleRendezvousListener::bind(*binding.installation_id(), successor_binding)
            .expect("bind keeper-to-successor one-shot channel");
    println!("QF1_KEEPER_LISTENER_READY");
    std::io::stdout()
        .flush()
        .expect("flush successor listener readiness");
    let mut keeper_session = 0_u32;
    // SAFETY: current keeper PID is live and the output is writable.
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                std::process::id(),
                &raw mut keeper_session,
            )
        },
        0
    );
    let expected_image = std::env::current_exe()
        .expect("test executable")
        .canonicalize()
        .expect("canonical successor image");
    let successor = listener
        .accept_until(
            Instant::now() + Duration::from_secs(10),
            |pid, session, facts| {
                if session != keeper_session || facts.session_id != session {
                    return None;
                }
                let peer =
                    keld_runtime::windows_job::WindowsProcessPeer::open(pid, session).ok()?;
                if peer.image_path().canonicalize().ok()? != expected_image
                    || peer.token_facts() != facts
                {
                    return None;
                }
                Some(peer)
            },
        )
        .expect("authenticate exact successor and receive acknowledgement")
        .expect("successor connects before deadline");
    handoff
        .retire_to_successor(
            successor,
            Duration::from_secs(10),
            Instant::now() + Duration::from_secs(10),
        )
        .expect("release lease only after successor zero ACK, then send QF1");
    println!("QF1_KEEPER_RETIRED");
    std::io::stdout().flush().expect("flush keeper retirement");
}

fn parse_lifecycle_id(name: &str) -> [u8; 32] {
    let encoded = std::env::var(name).unwrap_or_else(|_| panic!("missing {name}"));
    assert_eq!(
        encoded.len(),
        64,
        "{name} must be a 32-byte lowercase hex digest"
    );
    let mut decoded = [0_u8; 32];
    for (index, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(pair).expect("hex pair is UTF-8");
        decoded[index] = u8::from_str_radix(pair, 16).expect("hex pair is valid");
    }
    decoded
}

fn run_lease_keeper_helper() {
    let mut lines = BufReader::new(std::io::stdin()).lines();
    let record = lines
        .next()
        .expect("remote lease record")
        .expect("read remote lease record");
    let mut fields = record.split_ascii_whitespace();
    assert_eq!(fields.next(), Some("LEASE"));
    let remote_lease = fields
        .next()
        .expect("remote lease handle value")
        .parse::<usize>()
        .expect("valid remote lease handle");
    assert_eq!(fields.next(), None, "lease record must be exact");
    // SAFETY: this remote handle value was just created in this exact helper's
    // process by the coordinator's DuplicateHandle call.
    let handle = unsafe {
        std::os::windows::io::OwnedHandle::from_raw_handle(
            (remote_lease as *mut std::ffi::c_void).cast(),
        )
    };
    let mut lease = std::fs::File::from(handle);
    assert!(
        lease.write_all(b"forbidden").is_err(),
        "keeper lease handle cannot write the activation lock"
    );
    println!("LEASE_KEEPER_READY");
    std::io::stdout().flush().expect("flush keeper readiness");
    assert_eq!(
        lines
            .next()
            .expect("keeper exit command")
            .expect("read exit"),
        "EXIT"
    );
    println!("LEASE_KEEPER_EXIT");
    std::io::stdout().flush().expect("flush keeper exit");
}

fn spawn_lifecycle_process_helper(role: &str) -> Child {
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .args(["--exact", LIFECYCLE_HELPER_TEST, "--ignored", "--nocapture"])
        .env(LIFECYCLE_HELPER_ENV, role)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
        .spawn()
        .unwrap_or_else(|error| panic!("spawn {role} process fixture: {error}"))
}

fn next_prefixed_line(
    lines: &mut impl Iterator<Item = std::io::Result<String>>,
    prefix: &str,
) -> String {
    lines
        .find_map(|line| match line {
            Ok(line) if line.starts_with(prefix) => Some(Ok(line)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .unwrap_or_else(|| panic!("line starting with {prefix:?} missing"))
        .unwrap_or_else(|error| panic!("read line starting with {prefix:?}: {error}"))
}

#[test]
fn machine_writer_modes_refuse_before_opening_any_installation_path() {
    let fixture = tempfile::tempdir().expect("mode-refusal fixture");
    for mode in [
        crate::DirectInstallMode::MachineUacDirect,
        crate::DirectInstallMode::MachineSeamlessDirect,
    ] {
        let mut trust = support::trust_for(&fixture.path().join("absent-install"));
        trust.installation.install_mode = mode;
        let verifier = crate::UpdateVerifier::new(
            trust.installation.clone(),
            crate::tests::signing_key().verifying_key().to_bytes(),
        )
        .expect("mode-bound verifier");
        let error = load_windows_activation_write_snapshot(&trust, &verifier)
            .expect_err("privileged writer mechanisms have no production admission yet");
        assert!(
            matches!(
                error,
                crate::UpdateError::Baseline {
                    step: "activation writer mechanism",
                    ..
                }
            ),
            "mode {mode:?} refuses before path admission, got {error:?}"
        );
    }
}

#[test]
fn managed_owner_refuses_baseline_and_writer_before_filesystem_admission() {
    let fixture = tempfile::tempdir().expect("managed refusal fixture");
    let install = fixture.path().join("absent-managed-install");
    let mut trust = support::trust_for(&install);
    trust.owner = crate::InstallOwner::Managed {
        mechanism: "Microsoft Store MSIX".to_owned(),
    };
    assert!(
        trust.lifecycle_installation_id().is_err(),
        "managed deployments do not mint Keld direct-update lifecycle bindings"
    );
    let verifier = crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted test verifier");

    let read_error = load_windows_baseline(&trust).expect_err("managed read refuses early");
    assert!(matches!(
        read_error,
        crate::UpdateError::ManagedInstall { ref mechanism }
            if mechanism == "Microsoft Store MSIX"
    ));
    let writer_error = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect_err("managed writer refuses early");
    assert!(matches!(
        writer_error,
        crate::UpdateError::ManagedInstall { ref mechanism }
            if mechanism == "Microsoft Store MSIX"
    ));
    let signed_baseline = support::baseline(&trust);
    let init_error = crate::initialize_windows_baseline(
        &signed_baseline,
        &fixture.path().join("absent.tar"),
        &trust,
    )
    .expect_err("managed installation cannot bootstrap direct updater state");
    assert!(matches!(
        init_error,
        crate::UpdateError::ManagedInstall { ref mechanism }
            if mechanism == "Microsoft Store MSIX"
    ));
    assert!(
        !install.exists(),
        "early owner refusal creates no install tree"
    );
}

fn higher_release(
    verifier: &crate::UpdateVerifier,
    observation: &crate::ProvenanceObservation,
) -> crate::VerifiedFull {
    higher_release_version(verifier, observation, "2.0.0")
}

fn higher_release_version(
    verifier: &crate::UpdateVerifier,
    observation: &crate::ProvenanceObservation,
    version: &str,
) -> crate::VerifiedFull {
    let admitted = verifier.admit(observation).expect("admit real provenance");
    let compressed = zstd::stream::encode_all(Cursor::new(GOLDEN), 0)
        .expect("compress authenticated full package");
    let release = release_json(
        version,
        &compressed.len().to_string(),
        &digest_hex(&compressed),
        &GOLDEN.len().to_string(),
        &digest_hex(GOLDEN),
        "",
    );
    let manifest = manifest_json(&release);
    let crate::ManifestDecision::Update(selected) = admitted
        .verify_manifest(&manifest, &sign(&manifest))
        .expect("verify higher release manifest")
    else {
        panic!("higher release must be selected");
    };
    selected
        .verify_full(&mut Cursor::new(compressed), &mut Vec::new())
        .expect("verify complete higher package")
}

fn seed_pending_activation_journal(
    trust: &WindowsBaselineTrust,
    phase: ActivationPhase,
) -> ([u8; 32], [u8; 32]) {
    let profile = keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate;
    let baseline = trust.installation.baseline.clone();
    let mut candidate = baseline.clone();
    candidate.version = "2.0.0".to_owned();
    candidate.content_blake3 = *blake3::hash(GOLDEN).as_bytes();

    let versions = support::directory(&trust.installation.update_root.join("versions"));
    let candidate_directory =
        create_directory_relative_with_profile(&versions, &candidate.version, profile)
            .expect("create staged journal candidate tree");
    let candidate_tree =
        create_directory_relative_with_profile(&candidate_directory, "tree", profile)
            .expect("create protected candidate content tree");
    copy_fixture_tree(
        &trust
            .installation
            .update_root
            .join("versions")
            .join(&trust.installation.baseline.version)
            .join("tree"),
        &candidate_tree,
        profile,
    );
    drop(candidate_tree);
    let mut content =
        create_file_relative_with_profile(&candidate_directory, "content.tar", profile)
            .expect("create staged candidate content");
    content
        .write_all(GOLDEN)
        .expect("write staged candidate bytes");
    content.sync_all().expect("flush staged candidate bytes");
    drop(content);
    let complete = crate::records::encode_complete(
        &candidate,
        u64::try_from(GOLDEN.len()).expect("fixture size fits u64"),
    )
    .expect("encode candidate completion record");
    let mut marker = create_file_relative_with_profile(&candidate_directory, ".complete", profile)
        .expect("create candidate completion marker");
    marker
        .write_all(&complete)
        .expect("write completion marker");
    marker.sync_all().expect("flush completion marker");
    drop(marker);
    drop(candidate_directory);

    let current = if phase == ActivationPhase::PublishPending {
        baseline.clone()
    } else {
        candidate.clone()
    };
    let floor = if phase == ActivationPhase::PublishPending {
        baseline.version.clone()
    } else {
        candidate.version.clone()
    };
    let update_root = &trust.installation.update_root;
    std::fs::write(
        update_root.join("version-floor"),
        crate::records::encode_floor(&floor).expect("recovery floor"),
    )
    .expect("seed recovery floor");
    std::fs::write(
        update_root.join("current"),
        crate::records::encode_pointer(crate::records::PointerKind::Current, &current)
            .expect("current pointer"),
    )
    .expect("seed recovery current pointer");

    let attempt_id = [0x88; 32];
    let lifecycle_channel_id = [0x77; 32];
    let journal = ActivationJournal {
        attempt_id,
        candidate,
        rollback_target: baseline.clone(),
        prior_floor: baseline.version.clone(),
        prior_last_known_good: baseline,
        prior_previous_known_good: None,
        helper_image_blake3: [0x55; 32],
        health_channel_id: [0x66; 32],
        lifecycle_channel_id,
        phase,
    };
    let journal_bytes =
        crate::records::encode_activation_journal(&journal).expect("encode exact pending journal");
    let update = support::directory(&trust.installation.update_root);
    write_profiled_record(&update, "activation-journal", &journal_bytes, profile);
    (attempt_id, lifecycle_channel_id)
}

fn copy_fixture_tree(
    source: &std::path::Path,
    destination: &std::fs::File,
    profile: keld_guard::WindowsInstallProtectionProfile,
) {
    for entry in std::fs::read_dir(source).expect("read trusted baseline fixture tree") {
        let entry = entry.expect("read baseline fixture entry");
        let source_path = entry.path();
        let kind = entry.file_type().expect("read baseline fixture entry kind");
        if kind.is_dir() {
            let name = entry
                .file_name()
                .into_string()
                .expect("UTF-8 baseline fixture directory name");
            let child = create_directory_relative_with_profile(destination, &name, profile)
                .expect("create protected copied fixture directory");
            copy_fixture_tree(&source_path, &child, profile);
        } else {
            assert!(
                kind.is_file(),
                "baseline fixture contains a non-file object"
            );
            let name = entry
                .file_name()
                .into_string()
                .expect("UTF-8 baseline fixture file name");
            let bytes = std::fs::read(source_path).expect("read baseline fixture file");
            let mut copy = create_file_relative_with_profile(destination, &name, profile)
                .expect("create protected copied fixture file");
            copy.write_all(&bytes).expect("write copied fixture file");
            copy.sync_all().expect("flush copied fixture file");
        }
    }
}

fn seed_per_user_baseline(root: &std::path::Path) -> WindowsBaselineTrust {
    let install_path = root.join("KeldPerUserFixture");
    let mut trust = support::trust_for(&install_path);
    trust.installation.install_mode = crate::DirectInstallMode::PerUserDirect;
    let parent = support::directory(root);
    let profile = keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate;
    let install = create_directory_relative_with_profile(&parent, "KeldPerUserFixture", profile)
        .expect("exact owner-private install root");
    let update = create_directory_relative_with_profile(&install, "updates", profile)
        .expect("exact owner-private update root");
    let versions = create_directory_relative_with_profile(&update, "versions", profile)
        .expect("exact owner-private versions root");
    trust.volume_guid = qualified_volume_root(&parent).expect("fixture volume GUID");

    let archive = root.join("baseline.tar");
    std::fs::write(&archive, GOLDEN).expect("canonical baseline source");
    let verified = support::baseline(&trust);
    let mut source = open_source(&archive).expect("admit baseline source handle");
    let validated = verified
        .validate_windows_archive(&mut source)
        .expect("validate signed baseline package");
    let stage = create_directory_relative_with_profile(&versions, "1.0.0", profile)
        .expect("create baseline version");
    let (directories, files) = populate_stage(
        stage,
        "1.0.0",
        &validated,
        &mut source,
        StageProtection::OwnerPrivate,
        &mut |_, _| Ok::<(), std::io::Error>(()),
    )
    .expect("populate the exact baseline tree");
    let complete = crate::records::encode_complete(verified.identity(), verified.content_size())
        .expect("canonical completion record");
    let mut marker = create_file_relative_with_profile(
        &directories[0]
            .try_clone()
            .expect("version handle")
            .into_std_file(),
        ".complete",
        profile,
    )
    .expect("create owner-private completion marker");
    marker
        .write_all(&complete)
        .expect("write completion marker");
    marker.sync_all().expect("flush completion marker");
    drop(marker);
    drop(files);
    drop(directories);

    write_profiled_record(
        &update,
        "version-floor",
        &crate::records::encode_floor(&verified.identity().version).expect("baseline floor"),
        profile,
    );
    write_profiled_record(
        &update,
        "current",
        &crate::records::encode_pointer(crate::records::PointerKind::Current, verified.identity())
            .expect("current pointer"),
        profile,
    );
    write_profiled_record(
        &update,
        "last-known-good",
        &crate::records::encode_pointer(
            crate::records::PointerKind::LastKnownGood,
            verified.identity(),
        )
        .expect("last-known-good pointer"),
        profile,
    );
    let provenance = crate::records::encode_provenance(
        &crate::InstallProvenance {
            identity: trust.installation.clone(),
            owner: crate::InstallOwner::Direct,
        },
        &trust.publisher_scope,
        &trust.volume_guid,
    )
    .expect("mode-bound per-user provenance");
    write_profiled_record(&install, "install-provenance", &provenance, profile);
    let lock = create_file_relative_exclusive_with_profile(&update, "activation.lock", profile)
        .expect("persistent exclusive lease object");
    lock.sync_all().expect("flush persistent lease object");
    drop(lock);
    trust
}

fn write_profiled_record(
    parent: &std::fs::File,
    name: &str,
    bytes: &[u8],
    profile: keld_guard::WindowsInstallProtectionProfile,
) {
    let mut record = create_file_relative_with_profile(parent, name, profile)
        .unwrap_or_else(|error| panic!("create protected {name}: {error}"));
    record
        .write_all(bytes)
        .unwrap_or_else(|error| panic!("write protected {name}: {error}"));
    record
        .sync_all()
        .unwrap_or_else(|error| panic!("flush protected {name}: {error}"));
}
