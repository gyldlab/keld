//! Common journaled activation over real per-user Windows records and versions.
//!
//! Oracles are on-disk protected records decoded by the canonical codecs, the version
//! directory census, OS sharing behavior, and child-process exit at each persisted cut.

use std::collections::BTreeSet;
use std::io::Write as _;

use super::support::{self, CUT_ENV, GOLDEN, ROOT_ENV};
use super::writer::{higher_release_version, seed_per_user_baseline};
use crate::records::{ActivationJournal, ActivationPhase, PointerKind};
use crate::windows_baseline::{
    ActivationHealthReceipt, ProcessFamilyRetirement, WindowsActivationAttempt,
    WindowsActivationOutcome, WindowsBaselineTrust, WindowsRecoveryOutcome,
    load_windows_activation_write_snapshot, load_windows_recovery_inspection,
};
use crate::{ActivationEffect, ActivationFailureClass, UpdateError};

const COORDINATOR: [u8; 32] = [0x5a; 32];
const CRASH_HELPER: &str = "windows_baseline::tests::transaction::windows_activation_crash_helper";
const CRASH_EXIT: i32 = 93;

fn verifier(trust: &WindowsBaselineTrust) -> crate::UpdateVerifier {
    crate::UpdateVerifier::new(
        trust.installation.clone(),
        crate::tests::signing_key().verifying_key().to_bytes(),
    )
    .expect("trusted per-user verifier")
}

/// Extracts, publishes and starts activation of `version` under one writer lease.
fn begin(trust: &WindowsBaselineTrust, version: &str) -> WindowsActivationAttempt {
    let (root, published) = publish(trust, version);
    root.begin_activation(&published, COORDINATOR)
        .expect("journal and select the published candidate")
}

/// Publishes one complete immutable version and keeps the writer lease in its root.
fn publish(
    trust: &WindowsBaselineTrust,
    version: &str,
) -> (crate::WindowsExtractionRoot, crate::ArtifactIdentity) {
    let verifier = verifier(trust);
    let snapshot = load_windows_activation_write_snapshot(trust, &verifier)
        .expect("acquire the exclusive per-user writer lease");
    let observation = crate::ProvenanceObservation::Protected {
        record: crate::InstallProvenance {
            identity: trust.installation.clone(),
            owner: crate::InstallOwner::Direct,
        },
        version_floor: Some(snapshot.version_floor().to_owned()),
    };
    let candidate = higher_release_version(&verifier, &observation, version);
    let source = trust
        .installation
        .install_root
        .parent()
        .expect("fixture root")
        .join(format!("candidate-{version}.tar"));
    std::fs::write(&source, GOLDEN).expect("write verified candidate source");
    let mut root = snapshot
        .open_extraction_root()
        .expect("writer snapshot opens its staging root");
    let published = root
        .extract(&candidate, &source)
        .expect("extract the authenticated candidate")
        .publish_version()
        .expect("publish the complete immutable version");
    (root, published)
}

fn receipt(attempt: &WindowsActivationAttempt) -> ActivationHealthReceipt {
    ActivationHealthReceipt::new(
        *attempt.attempt_id(),
        *attempt.health_channel_id(),
        attempt.candidate().clone(),
    )
}

fn retirement(attempt: &WindowsActivationAttempt) -> ProcessFamilyRetirement {
    ProcessFamilyRetirement::from_exact_zero_observation(
        *attempt.lifecycle_installation_id(),
        *attempt.attempt_id(),
        *attempt.lifecycle_channel_id(),
    )
}

fn commit(trust: &WindowsBaselineTrust, version: &str) {
    let attempt = begin(trust, version);
    let health = receipt(&attempt);
    let resolution = attempt
        .accept_health(&health)
        .expect("exact health commits the candidate");
    assert_eq!(resolution.outcome(), WindowsActivationOutcome::Committed);
    assert_eq!(resolution.current().version, version);
    assert!(resolution.cleanup_error().is_none());
}

/// Decoded protected state: floor, current, LKG, previous, journal, version names.
#[derive(Debug, PartialEq, Eq)]
struct Observed {
    floor: String,
    current: String,
    last_known_good: String,
    previous_known_good: Option<String>,
    journal: Option<ActivationJournal>,
    versions: BTreeSet<String>,
    pending_records: usize,
}

fn observe(trust: &WindowsBaselineTrust) -> Observed {
    let update = &trust.installation.update_root;
    let pointer = |name: &str, kind| {
        crate::records::decode_pointer(
            kind,
            &std::fs::read(update.join(name)).expect("protected pointer bytes"),
        )
        .expect("canonical protected pointer")
        .version
    };
    let previous = update.join("previous-known-good");
    let journal = update.join("activation-journal");
    let mut versions = BTreeSet::new();
    for entry in std::fs::read_dir(update.join("versions")).expect("version census") {
        let name = entry.expect("version entry").file_name();
        let name = name.into_string().expect("UTF-8 version entry");
        versions.insert(if name.starts_with("retired-") {
            "retired-*".to_owned()
        } else if name.starts_with("incomplete-") {
            "incomplete-*".to_owned()
        } else {
            name
        });
    }
    let pending_records = std::fs::read_dir(update)
        .expect("update census")
        .filter(|entry| {
            entry
                .as_ref()
                .expect("update entry")
                .file_name()
                .to_string_lossy()
                .starts_with("pending-")
        })
        .count();
    Observed {
        floor: crate::records::decode_floor(
            &std::fs::read(update.join("version-floor")).expect("protected floor bytes"),
        )
        .expect("canonical floor"),
        current: pointer("current", PointerKind::Current),
        last_known_good: pointer("last-known-good", PointerKind::LastKnownGood),
        previous_known_good: previous
            .exists()
            .then(|| pointer("previous-known-good", PointerKind::PreviousKnownGood)),
        journal: journal.exists().then(|| {
            crate::records::decode_activation_journal(
                &std::fs::read(journal).expect("protected journal bytes"),
            )
            .expect("canonical protected journal")
        }),
        versions,
        pending_records,
    }
}

fn names(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn assert_resolved(
    trust: &WindowsBaselineTrust,
    current: &str,
    previous: Option<&str>,
    floor: &str,
    versions: &[&str],
) {
    let observed = observe(trust);
    assert_eq!(observed.floor, floor, "{observed:?}");
    assert_eq!(observed.current, current, "{observed:?}");
    assert_eq!(observed.last_known_good, current, "{observed:?}");
    assert_eq!(
        observed.previous_known_good.as_deref(),
        previous,
        "{observed:?}"
    );
    assert_eq!(observed.journal, None, "resolution removes the journal");
    assert_eq!(observed.versions, names(versions), "{observed:?}");
    assert_eq!(observed.pending_records, 0, "{observed:?}");
    drop(
        load_windows_activation_write_snapshot(trust, &verifier(trust))
            .expect("a resolved installation admits the next writer without orphans"),
    );
}

fn assert_refusal(
    result: Result<impl std::fmt::Debug, UpdateError>,
    step: &str,
    effect: ActivationEffect,
) {
    match result.expect_err("the transaction must refuse") {
        UpdateError::Activation {
            step: refused,
            effect: observed,
            ..
        } => assert_eq!((refused, observed), (step, effect)),
        other => panic!("expected a {step} refusal with {effect:?}, got {other:?}"),
    }
}

#[test]
fn record_replacement_requires_a_generated_sibling_and_a_file_destination() {
    use crate::windows_fs::{RecordSlot, replace_record_slot};
    let fixture = tempfile::tempdir().expect("record slot fixture");
    let path = fixture.path();
    let pending = format!("pending-{}", "a".repeat(64));
    std::fs::write(path.join("current"), b"previous").expect("existing slot");
    std::fs::write(path.join("last-known-good"), b"known-good").expect("another slot");
    std::fs::write(path.join("install-provenance"), b"protected").expect("provenance");
    std::fs::write(path.join(&pending), b"replacement").expect("prepared sibling");
    std::fs::create_dir(path.join("version-floor")).expect("directory at a slot name");
    let parent = support::directory(path);

    for source in ["last-known-good", "install-provenance", "pending-short"] {
        assert!(
            replace_record_slot(&parent, source, RecordSlot::Current).is_err(),
            "{source} is not a generated pending sibling"
        );
    }
    assert!(
        replace_record_slot(&parent, &pending, RecordSlot::Floor).is_err(),
        "MOVEFILE_REPLACE_EXISTING refuses a directory destination"
    );
    for (leaf, bytes) in [
        ("current", &b"previous"[..]),
        ("last-known-good", b"known-good"),
        ("install-provenance", b"protected"),
        (pending.as_str(), b"replacement"),
    ] {
        assert_eq!(
            std::fs::read(path.join(leaf)).expect("record after refusals"),
            bytes,
            "{leaf} is untouched by every refused replacement"
        );
    }
    assert!(path.join("version-floor").is_dir());

    replace_record_slot(&parent, &pending, RecordSlot::Current)
        .expect("a generated sibling replaces an existing fixed file slot");
    assert_eq!(
        std::fs::read(path.join("current")).expect("replaced slot"),
        b"replacement"
    );
    assert!(!path.join(&pending).exists());
}

#[test]
fn stale_record_preparations_are_admitted_and_deleted_through_one_handle() {
    let profile = keld_guard::WindowsInstallProtectionProfile::PerUserOwnerPrivate;
    for case in ["genuine", "linked", "directory"] {
        let fixture = tempfile::tempdir().expect("stale preparation fixture");
        let elsewhere = tempfile::tempdir().expect("same-volume link fixture");
        let trust = seed_per_user_baseline(fixture.path());
        let update = trust.installation.update_root.clone();
        let leaf = format!("pending-{}", "b".repeat(64));
        if case == "directory" {
            std::fs::create_dir(update.join(&leaf)).expect("directory at a pending name");
        } else {
            let parent = support::directory(&update);
            let mut file =
                crate::windows_fs::create_file_relative_with_profile(&parent, &leaf, profile)
                    .expect("profiled preparation");
            file.write_all(b"prepared").expect("prepared bytes");
            drop(file);
            if case == "linked" {
                std::fs::hard_link(update.join(&leaf), elsewhere.path().join("second-link"))
                    .expect("second link outside the update root");
            }
        }
        let result = crate::repair_windows_unjournaled_versions(&trust, &verifier(&trust));
        if case == "genuine" {
            result.expect("a genuine preparation is removed");
            assert!(!update.join(&leaf).exists());
        } else {
            let error = result.expect_err("a non-genuine preparation must refuse");
            assert!(
                format!("{error:?}").contains("stale record admission"),
                "{case} refuses at admission, not elsewhere: {error:?}"
            );
            assert!(
                update.join(&leaf).exists(),
                "{case} preparation stays for manual recovery"
            );
        }
    }
}

#[test]
fn per_user_updates_commit_through_the_common_trace_and_retire_superseded_versions() {
    support::assert_user_principal_token();
    let fixture = tempfile::tempdir().expect("per-user activation fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = verifier(&trust);

    let attempt = begin(&trust, "2.0.0");
    let awaiting = observe(&trust);
    assert_eq!(awaiting.floor, "2.0.0", "floor advances before selection");
    assert_eq!(awaiting.current, "2.0.0");
    assert_eq!(
        awaiting.last_known_good, "1.0.0",
        "the prior known-good stays until exact health"
    );
    let journal = awaiting.journal.expect("live attempt is journaled");
    assert_eq!(journal.phase, ActivationPhase::AwaitingHealth);
    assert_eq!(&journal.attempt_id, attempt.attempt_id());
    assert_eq!(&journal.health_channel_id, attempt.health_channel_id());
    assert_eq!(
        &journal.lifecycle_channel_id,
        attempt.lifecycle_channel_id()
    );
    assert_eq!(journal.helper_image_blake3, COORDINATOR);
    assert_eq!(journal.rollback_target.version, "1.0.0");
    assert_eq!(journal.prior_floor, "1.0.0");
    assert_eq!(
        attempt.lifecycle_installation_id(),
        &trust
            .lifecycle_installation_id()
            .expect("trusted lifecycle installation ID")
    );
    assert!(
        load_windows_activation_write_snapshot(&trust, &verifier).is_err(),
        "the live attempt keeps the share-zero writer lease through health"
    );
    assert!(
        load_windows_recovery_inspection(&trust, &verifier).is_err(),
        "recovery cannot race a live owner"
    );

    let health = receipt(&attempt);

    let resolution = attempt
        .accept_health(&health)
        .expect("exact health commits the first update");
    assert_eq!(resolution.outcome(), WindowsActivationOutcome::Committed);
    assert!(resolution.cleanup_error().is_none());
    assert_resolved(&trust, "2.0.0", Some("1.0.0"), "2.0.0", &["1.0.0", "2.0.0"]);

    commit(&trust, "3.0.0");
    assert_resolved(&trust, "3.0.0", Some("2.0.0"), "3.0.0", &["2.0.0", "3.0.0"]);
}

#[test]
fn per_user_rollback_restores_the_target_retires_the_candidate_and_keeps_the_floor() {
    support::assert_user_principal_token();
    let fixture = tempfile::tempdir().expect("per-user rollback fixture");
    let trust = seed_per_user_baseline(fixture.path());

    let attempt = begin(&trust, "2.0.0");
    let proof = retirement(&attempt);
    let resolution = attempt
        .roll_back(ActivationFailureClass::HealthRejected, &proof)
        .expect("bound failure rolls back");
    assert_eq!(resolution.outcome(), WindowsActivationOutcome::RolledBack);
    assert_eq!(resolution.current().version, "1.0.0");
    assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);

    let verifier = verifier(&trust);
    let admitted = verifier
        .admit(&crate::ProvenanceObservation::Protected {
            record: crate::InstallProvenance {
                identity: trust.installation.clone(),
                owner: crate::InstallOwner::Direct,
            },
            version_floor: Some(observe(&trust).floor),
        })
        .expect("admit the rolled-back installation");
    let manifest = crate::tests::manifest_json(&crate::tests::release_json(
        "2.0.0",
        "1",
        &crate::tests::digest_hex(b"x"),
        "1",
        &crate::tests::digest_hex(b"x"),
        "",
    ));
    assert!(
        matches!(
            admitted
                .verify_manifest(&manifest, &crate::tests::sign(&manifest))
                .expect("authentic replayed manifest"),
            crate::ManifestDecision::NoUpdate
        ),
        "the failed signed version is never automatically reselected"
    );

    commit(&trust, "3.0.0");
    assert_resolved(&trust, "3.0.0", Some("1.0.0"), "3.0.0", &["1.0.0", "3.0.0"]);
}

#[test]
fn substituted_health_receipts_refuse_before_any_write_and_leave_recovery_authoritative() {
    for substitution in ["attempt", "channel", "candidate"] {
        let fixture = tempfile::tempdir().expect("receipt falsifier fixture");
        let trust = seed_per_user_baseline(fixture.path());
        let attempt = begin(&trust, "2.0.0");
        let before = observe(&trust);
        let mut attempt_id = *attempt.attempt_id();
        let mut health_channel_id = *attempt.health_channel_id();
        let mut candidate = attempt.candidate().clone();
        match substitution {
            "attempt" => attempt_id[0] ^= 1,
            // A real identity from the same attempt, but not its health channel.
            "channel" => health_channel_id = *attempt.lifecycle_channel_id(),
            _ => candidate.content_blake3[0] ^= 1,
        }
        let substituted = ActivationHealthReceipt::new(attempt_id, health_channel_id, candidate);
        assert_refusal(
            attempt.accept_health(&substituted),
            "health receipt binding",
            ActivationEffect::JournalBoundRecoveryRequired,
        );
        assert_eq!(
            observe(&trust),
            before,
            "{substitution}: a substituted receipt writes nothing"
        );
        assert_eq!(
            recover_exact(&trust).expect("exact recovery of the unresolved attempt"),
            WindowsActivationOutcome::RolledBack,
            "{substitution}: health that never matched the attempt is never committed"
        );
        assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);
    }
}

#[test]
fn substituted_retirement_or_coordinator_refuses_live_and_recovery_writes() {
    let fixture = tempfile::tempdir().expect("retirement falsifier fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = verifier(&trust);
    let attempt = begin(&trust, "2.0.0");
    let before = observe(&trust);
    let installation = *attempt.lifecycle_installation_id();
    let attempt_id = *attempt.attempt_id();
    let channel = *attempt.lifecycle_channel_id();
    let flip = |mut value: [u8; 32]| {
        value[31] ^= 1;
        value
    };
    let substituted = [
        ProcessFamilyRetirement::from_exact_zero_observation(
            flip(installation),
            attempt_id,
            channel,
        ),
        ProcessFamilyRetirement::from_exact_zero_observation(
            installation,
            flip(attempt_id),
            channel,
        ),
        ProcessFamilyRetirement::from_exact_zero_observation(
            installation,
            attempt_id,
            flip(channel),
        ),
    ];
    assert_refusal(
        attempt.roll_back(ActivationFailureClass::ProcessCrash, &substituted[0]),
        "process-family retirement binding",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    assert_eq!(
        observe(&trust),
        before,
        "live rollback writes nothing without exact proof"
    );

    for wrong in &substituted {
        let inspection =
            load_windows_recovery_inspection(&trust, &verifier).expect("inspect the lost attempt");
        assert_refusal(
            inspection.recover(wrong, COORDINATOR),
            "process-family retirement binding",
            ActivationEffect::JournalBoundRecoveryRequired,
        );
        assert_eq!(observe(&trust), before);
    }
    let exact =
        ProcessFamilyRetirement::from_exact_zero_observation(installation, attempt_id, channel);
    let inspection =
        load_windows_recovery_inspection(&trust, &verifier).expect("inspect the lost attempt");
    assert_refusal(
        inspection.recover(&exact, flip(COORDINATOR)),
        "coordinator identity",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    assert_eq!(observe(&trust), before);
    let inspection =
        load_windows_recovery_inspection(&trust, &verifier).expect("inspect the lost attempt");
    assert_refusal(
        inspection.resume_unlaunched(COORDINATOR),
        "unlaunched resume",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    assert_eq!(
        observe(&trust),
        before,
        "a launched attempt is never resumed without a retirement binding"
    );

    assert_eq!(
        recover_exact(&trust).expect("exact retirement recovers the lost attempt"),
        WindowsActivationOutcome::RolledBack,
        "an attempt that lost its owner during health rolls back"
    );
    assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);
}

#[test]
fn a_refusal_before_the_journal_retires_only_the_published_candidate() {
    let fixture = tempfile::tempdir().expect("pre-journal refusal fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let diagnostic = trust
        .installation
        .update_root
        .join("versions")
        .join(format!("incomplete-{}", "b".repeat(64)));
    std::fs::create_dir(&diagnostic).expect("a live diagnostic stage beside the versions");
    let earlier_retired = trust
        .installation
        .update_root
        .join("versions")
        .join(format!("retired-{}", "c".repeat(64)));
    std::fs::create_dir(&earlier_retired).expect("an earlier retired tree");

    let (root, published) = publish(&trust, "3.0.0");
    assert_refusal(
        root.begin_activation(&published, [0; 32]),
        "start",
        ActivationEffect::ProtectedStateUnchanged,
    );
    let refused = observe(&trust);
    assert_eq!(refused.journal, None);
    assert_eq!(
        (refused.floor.as_str(), refused.current.as_str()),
        ("2.0.0", "2.0.0")
    );
    assert_eq!(
        refused.versions,
        names(&["1.0.0", "2.0.0", "incomplete-*", "retired-*"]),
        "only the refused candidate is retired; known-good and diagnostic entries stay"
    );

    let (root, published) = publish(&trust, "3.0.0");
    let mut unrelated = published;
    unrelated.content_blake3[0] ^= 1;
    assert_refusal(
        root.begin_activation(&unrelated, COORDINATOR),
        "start",
        ActivationEffect::ProtectedStateUnchanged,
    );
    assert_eq!(
        observe(&trust).versions,
        names(&["1.0.0", "2.0.0", "incomplete-*", "retired-*"]),
        "an identity that does not match the published version never becomes runnable"
    );

    assert!(
        earlier_retired.is_dir() && diagnostic.is_dir(),
        "generated entries are neither renamed nor deleted by a refused start"
    );
    std::fs::remove_dir(&diagnostic).expect("remove the diagnostic fixture");
    commit(&trust, "3.0.0");
    assert_resolved(&trust, "3.0.0", Some("2.0.0"), "3.0.0", &["2.0.0", "3.0.0"]);
}

#[test]
fn an_unretirable_refused_candidate_halts_until_the_explicit_repair() {
    let fixture = tempfile::tempdir().expect("retained candidate fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let verifier = verifier(&trust);
    let (root, published) = publish(&trust, "2.0.0");
    let candidate_tree = trust
        .installation
        .update_root
        .join("versions")
        .join("2.0.0")
        .join("tree");
    let holder = std::fs::File::open(nested_file(&candidate_tree))
        .expect("hold a file inside the refused candidate");
    assert_refusal(
        root.begin_activation(&published, [0; 32]),
        "start",
        ActivationEffect::UnjournaledVersionRetained,
    );
    let error = load_windows_activation_write_snapshot(&trust, &verifier)
        .expect_err("an unjournaled complete version halts every ordinary writer");
    assert!(
        error.to_string().contains("unreferenced version entry"),
        "{error}"
    );
    assert!(
        crate::repair_windows_unjournaled_versions(&trust, &verifier).is_err(),
        "the repair cannot retire a tree that is still open"
    );
    drop(holder);

    assert_eq!(
        crate::repair_windows_unjournaled_versions(&trust, &verifier)
            .expect("the explicit repair retires the unjournaled version"),
        1
    );
    let repaired = observe(&trust);
    assert_eq!(repaired.journal, None);
    assert_eq!(
        (repaired.floor.as_str(), repaired.current.as_str()),
        ("1.0.0", "1.0.0"),
        "the repair never selects or advances anything"
    );
    assert_eq!(repaired.versions, names(&["1.0.0", "retired-*"]));
    assert_eq!(
        crate::repair_windows_unjournaled_versions(&trust, &verifier)
            .expect("an orphan-free installation repairs nothing"),
        0
    );
    commit(&trust, "2.0.0");
    assert_resolved(&trust, "2.0.0", Some("1.0.0"), "2.0.0", &["1.0.0", "2.0.0"]);
}

#[test]
fn the_unjournaled_repair_refuses_a_pending_journal() {
    let fixture = tempfile::tempdir().expect("repair refusal fixture");
    let trust = seed_per_user_baseline(fixture.path());
    // A publish-pending journal references a candidate that no pointer selects yet: the
    // one state in which an unguarded repair would retire a journaled version.
    let stdout = support::child(
        CRASH_HELPER,
        fixture.path(),
        "first-commit",
        "publish-pending",
        CRASH_EXIT,
    );
    assert!(
        stdout.contains("KELD_ACTIVATION_CUT=publish-pending"),
        "{stdout}"
    );
    let before = observe(&trust);
    assert_eq!(
        before.journal.as_ref().map(|journal| &journal.phase),
        Some(&ActivationPhase::PublishPending)
    );
    assert!(before.versions.contains("2.0.0"));
    let error = crate::repair_windows_unjournaled_versions(&trust, &verifier(&trust))
        .expect_err("a journaled attempt belongs to journal-bound recovery");
    assert!(
        error
            .to_string()
            .contains("pending journal requires the process-family recovery owner"),
        "{error}"
    );
    assert_eq!(
        observe(&trust),
        before,
        "the refused repair renames nothing"
    );
    assert_eq!(
        recover_exact(&trust).expect("recovery resumes the journaled candidate"),
        WindowsActivationOutcome::Committed
    );
    assert_resolved(&trust, "2.0.0", Some("1.0.0"), "2.0.0", &["1.0.0", "2.0.0"]);
}

#[test]
fn the_unjournaled_repair_refuses_unknown_or_damaged_entries_before_any_rename() {
    for stray in [
        "unprotected-empty",
        "not-a-version",
        "renamed-version",
        "foreign-scope",
        "generated-file",
    ] {
        let fixture = tempfile::tempdir().expect("stray entry fixture");
        let trust = seed_per_user_baseline(fixture.path());
        let versions = trust.installation.update_root.join("versions");
        // Beside a valid unjournaled orphan, an unknown entry must stop the repair before
        // its first rename; a damaged published tree is itself the only orphan.
        let damaged_tree = matches!(stray, "renamed-version" | "foreign-scope");
        if !damaged_tree {
            let (root, published) = publish(&trust, "2.0.0");
            drop(root);
            drop(published);
        }
        match stray {
            "unprotected-empty" => {
                std::fs::create_dir(versions.join("9.9.9")).expect("an empty directory");
            }
            "not-a-version" => {
                std::fs::create_dir(versions.join("not-a-version")).expect("a stray name");
            }
            "renamed-version" => {
                // A genuine protected tree whose completion record names another version.
                let (root, published) = publish(&trust, "3.0.0");
                drop(root);
                drop(published);
                std::fs::rename(versions.join("3.0.0"), versions.join("9.9.9"))
                    .expect("store a complete tree under another version name");
            }
            "foreign-scope" => {
                let (root, published) = publish(&trust, "3.0.0");
                drop(root);
                let mut foreign = published;
                foreign.app_id = "com.example.other".to_owned();
                let content_size = std::fs::metadata(versions.join("3.0.0").join("content.tar"))
                    .expect("published archive")
                    .len();
                std::fs::write(
                    versions.join("3.0.0").join(".complete"),
                    crate::records::encode_complete(&foreign, content_size)
                        .expect("canonical foreign completion record"),
                )
                .expect("rewrite the protected completion record in place");
            }
            _ => {
                std::fs::write(
                    versions.join(format!("retired-{}", "d".repeat(64))),
                    b"not a tree",
                )
                .expect("a file under a generated directory name");
            }
        }
        let before = observe(&trust);
        let error = crate::repair_windows_unjournaled_versions(&trust, &verifier(&trust))
            .expect_err("an unknown or damaged entry refuses the whole repair");
        assert!(
            !matches!(
                error,
                UpdateError::Activation {
                    effect: ActivationEffect::UnjournaledVersionRetained,
                    ..
                }
            ),
            "{stray}: a damaged entry is not a retryable rename failure: {error:?}"
        );
        assert_eq!(
            observe(&trust),
            before,
            "{stray}: nothing is renamed, not even the valid unjournaled 2.0.0"
        );
    }
}

#[test]
fn the_unjournaled_repair_never_retires_a_case_variant_of_a_referenced_tree() {
    let fixture = tempfile::tempdir().expect("case variant fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0-rc.1");
    let versions = trust.installation.update_root.join("versions");
    std::fs::rename(versions.join("2.0.0-rc.1"), versions.join("2.0.0-RC.1"))
        .expect("store the current tree under a case-variant name");
    let error = load_windows_activation_write_snapshot(&trust, &verifier(&trust))
        .expect_err("the exact-name census halts on the case variant");
    assert!(
        error.to_string().contains("unreferenced version entry"),
        "{error}"
    );
    assert!(
        crate::repair_windows_unjournaled_versions(&trust, &verifier(&trust)).is_err(),
        "the repair still halts on the census instead of retiring the current tree"
    );
    let observed = observe(&trust);
    assert!(
        observed.versions.contains("2.0.0-RC.1") && !observed.versions.contains("retired-*"),
        "the referenced tree is never renamed: {observed:?}"
    );
    assert_eq!(observed.current, "2.0.0-rc.1");
}

#[test]
fn an_unlaunched_attempt_resumes_under_the_lease_with_fresh_channels() {
    let fixture = tempfile::tempdir().expect("unlaunched resume fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let stdout = support::child(
        CRASH_HELPER,
        fixture.path(),
        "first-commit",
        "floor-advanced",
        CRASH_EXIT,
    );
    assert!(
        stdout.contains("KELD_ACTIVATION_CUT=floor-advanced"),
        "{stdout}"
    );
    let lost = observe(&trust)
        .journal
        .expect("the lost attempt is journaled");
    assert_eq!(lost.phase, ActivationPhase::PublishPending);

    let inspection = load_windows_recovery_inspection(&trust, &verifier(&trust))
        .expect("inspect the unlaunched attempt");
    let attempt = inspection
        .resume_unlaunched(COORDINATOR)
        .expect("a never-launched attempt resumes under the writer lease alone");
    assert_eq!(attempt.attempt_id(), &lost.attempt_id);
    assert_ne!(attempt.health_channel_id(), &lost.health_channel_id);
    assert_ne!(attempt.lifecycle_channel_id(), &lost.lifecycle_channel_id);
    let resumed = observe(&trust)
        .journal
        .expect("the resumed attempt stays journaled");
    assert_eq!(resumed.phase, ActivationPhase::AwaitingHealth);
    assert_eq!(
        &resumed.lifecycle_channel_id,
        attempt.lifecycle_channel_id()
    );

    let stale = ProcessFamilyRetirement::from_exact_zero_observation(
        *attempt.lifecycle_installation_id(),
        lost.attempt_id,
        lost.lifecycle_channel_id,
    );
    assert_refusal(
        attempt.roll_back(ActivationFailureClass::ProcessCrash, &stale),
        "process-family retirement binding",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    assert_eq!(
        recover_exact(&trust).expect("the re-minted channel binds recovery"),
        WindowsActivationOutcome::RolledBack
    );
    assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);
}

/// First regular file below `tree/` that sits inside at least one subdirectory.
fn nested_file(tree: &std::path::Path) -> std::path::PathBuf {
    let mut directories = vec![tree.to_path_buf()];
    let mut fallback = None;
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).expect("read retiring tree") {
            let path = entry.expect("retiring tree entry").path();
            if path.is_dir() {
                directories.push(path);
            } else if directory != tree {
                return path;
            } else {
                fallback.get_or_insert(path);
            }
        }
    }
    fallback.expect("the retiring tree contains a file")
}

#[test]
fn an_open_nested_handle_in_the_retiring_tree_preserves_the_journal() {
    let fixture = tempfile::tempdir().expect("blocked retirement fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let attempt = begin(&trust, "3.0.0");
    let retiring_tree = trust
        .installation
        .update_root
        .join("versions")
        .join("1.0.0")
        .join("tree");
    // std opens with read, write and delete sharing; NTFS still refuses the directory
    // rename while any descendant handle is open.
    let holder = std::fs::File::open(nested_file(&retiring_tree))
        .expect("hold a nested file inside the version being retired");
    let health = receipt(&attempt);
    assert_refusal(
        attempt.accept_health(&health),
        "version retirement",
        ActivationEffect::JournalBoundRecoveryRequired,
    );
    let blocked = observe(&trust);
    assert!(matches!(
        blocked.journal.as_ref().map(|journal| &journal.phase),
        Some(ActivationPhase::HealthAccepted { .. })
    ));
    assert_eq!(blocked.last_known_good, "3.0.0");
    assert_eq!(blocked.previous_known_good.as_deref(), Some("2.0.0"));
    assert!(blocked.versions.contains("1.0.0"));
    drop(holder);

    let resolution = recover_exact(&trust).expect("released retirement finishes the commit");
    assert_eq!(resolution, WindowsActivationOutcome::Committed);
    assert_resolved(&trust, "3.0.0", Some("2.0.0"), "3.0.0", &["2.0.0", "3.0.0"]);
}

#[test]
fn a_process_running_from_a_retired_tree_defers_only_its_deletion() {
    let fixture = tempfile::tempdir().expect("running retiree fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let attempt = begin(&trust, "3.0.0");
    let image = trust
        .installation
        .update_root
        .join("versions")
        .join("1.0.0")
        .join("tree")
        .join("retiree-host.exe");
    let system = std::env::var_os("SystemRoot").expect("Windows system root");
    std::fs::copy(
        std::path::Path::new(&system)
            .join("System32")
            .join("cmd.exe"),
        &image,
    )
    .expect("place an executable image inside the version being retired");
    let mut running = std::process::Command::new(&image)
        .args(["/c", "pause"])
        .current_dir(fixture.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("run an image from the retiring tree");

    let health = receipt(&attempt);
    let resolution = attempt
        .accept_health(&health)
        .expect("a mapped image does not block the directory rename");
    assert_eq!(resolution.outcome(), WindowsActivationOutcome::Committed);
    match resolution.cleanup_error() {
        Some(UpdateError::Activation {
            effect: ActivationEffect::ResolvedWithLeftovers,
            ..
        }) => {}
        other => panic!("deleting a running image must be reported as a leftover: {other:?}"),
    }
    let resolved = observe(&trust);
    assert_eq!(resolved.journal, None);
    let mut expected = names(&["2.0.0", "3.0.0"]);
    expected.insert("retired-*".to_owned());
    assert_eq!(resolved.versions, expected);

    running.kill().expect("stop the image");
    running.wait().expect("reap the image");
    commit(&trust, "4.0.0");
    assert_resolved(&trust, "4.0.0", Some("3.0.0"), "4.0.0", &["3.0.0", "4.0.0"]);
}

/// Recovers the protected journal with a retirement binding read back from it.
///
/// The caller has already observed the prior owner's exit; that observation is the
/// family-retirement evidence this binding names.
fn recover_exact(trust: &WindowsBaselineTrust) -> Result<WindowsActivationOutcome, UpdateError> {
    let inspection = load_windows_recovery_inspection(trust, &verifier(trust))?;
    let lost_channel = *inspection.lifecycle_channel_id();
    let retirement = ProcessFamilyRetirement::from_exact_zero_observation(
        *inspection.lifecycle_installation_id(),
        *inspection.attempt_id(),
        lost_channel,
    );
    match inspection.recover(&retirement, COORDINATOR)? {
        WindowsRecoveryOutcome::Resolved(resolution) => {
            assert!(resolution.cleanup_error().is_none());
            Ok(resolution.outcome())
        }
        WindowsRecoveryOutcome::AwaitingHealth(attempt) => {
            let journal = observe(trust)
                .journal
                .expect("resumed attempt is journaled");
            assert_eq!(journal.phase, ActivationPhase::AwaitingHealth);
            assert_ne!(
                journal.lifecycle_channel_id, lost_channel,
                "a resumed owner never reuses the lost owner's lifecycle channel"
            );
            let health = receipt(&attempt);
            let resolution = attempt.accept_health(&health)?;
            Ok(resolution.outcome())
        }
    }
}

/// Expected recovery after one persisted cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterCut {
    /// No journal references the published candidate; startup halts on the orphan.
    OrphanHalts,
    /// Publish-pending resumes to a live attempt, which then commits on exact health.
    ResumesThenCommits,
    /// Health was never accepted; recovery rolls back.
    RollsBack,
    /// Health was accepted; recovery finishes the commit.
    FinishesCommit,
    /// The journal is already removed; the next writer is admitted directly.
    AlreadyCommitted,
    AlreadyRolledBack,
}

/// One crash scenario: which owner crashes, on top of which installed history.
#[derive(Debug, Clone, Copy)]
struct Scenario {
    /// Child case: a live `commit`/`rollback` owner (prefixed `first-` for the first
    /// update), or a `recover`ing owner after `first_crash`.
    case: &'static str,
    /// Whether one update already committed, so previous-known-good exists.
    prior_commit: bool,
    /// For recovering-owner cuts: the live-owner crash that leaves the journal.
    first_crash: Option<&'static str>,
    cuts: &'static [(&'static str, AfterCut)],
}

const COMMIT_PATH: &[(&str, AfterCut)] = &[
    ("prepared:publish-pending", AfterCut::OrphanHalts),
    ("publish-pending", AfterCut::ResumesThenCommits),
    ("prepared:floor-advanced", AfterCut::ResumesThenCommits),
    ("floor-advanced", AfterCut::ResumesThenCommits),
    ("prepared:candidate-selected", AfterCut::ResumesThenCommits),
    ("candidate-selected", AfterCut::ResumesThenCommits),
    ("prepared:awaiting-health", AfterCut::ResumesThenCommits),
    ("awaiting-health", AfterCut::RollsBack),
    ("prepared:health-accepted", AfterCut::RollsBack),
    ("health-accepted", AfterCut::FinishesCommit),
    (
        "prepared:prior-known-good-preserved",
        AfterCut::FinishesCommit,
    ),
    ("prior-known-good-preserved", AfterCut::FinishesCommit),
    ("prepared:candidate-committed", AfterCut::FinishesCommit),
    ("candidate-committed", AfterCut::FinishesCommit),
    ("version-retired", AfterCut::FinishesCommit),
    ("journal-removed", AfterCut::AlreadyCommitted),
];

const ROLLBACK_PATH: &[(&str, AfterCut)] = &[
    ("prepared:rollback-pending", AfterCut::RollsBack),
    ("rollback-pending", AfterCut::RollsBack),
    ("prepared:rollback-target-restored", AfterCut::RollsBack),
    ("rollback-target-restored", AfterCut::RollsBack),
    ("version-retired", AfterCut::RollsBack),
    ("journal-removed", AfterCut::AlreadyRolledBack),
];

const RECOVERY_ROLLBACK_PATH: &[(&str, AfterCut)] = &[
    ("rollback-pending", AfterCut::RollsBack),
    ("prepared:rollback-target-restored", AfterCut::RollsBack),
    ("rollback-target-restored", AfterCut::RollsBack),
    ("version-retired", AfterCut::RollsBack),
    ("journal-removed", AfterCut::AlreadyRolledBack),
];

const RECOVERY_RESUME_PATH: &[(&str, AfterCut)] = &[
    ("prepared:channels-reminted", AfterCut::ResumesThenCommits),
    ("channels-reminted", AfterCut::ResumesThenCommits),
    ("candidate-selected", AfterCut::ResumesThenCommits),
    ("awaiting-health", AfterCut::RollsBack),
];

const SCENARIOS: [Scenario; 6] = [
    Scenario {
        case: "commit",
        prior_commit: true,
        first_crash: None,
        cuts: COMMIT_PATH,
    },
    Scenario {
        case: "first-commit",
        prior_commit: false,
        first_crash: None,
        cuts: COMMIT_PATH,
    },
    Scenario {
        case: "rollback",
        prior_commit: true,
        first_crash: None,
        cuts: ROLLBACK_PATH,
    },
    Scenario {
        case: "first-rollback",
        prior_commit: false,
        first_crash: None,
        cuts: ROLLBACK_PATH,
    },
    Scenario {
        case: "recover",
        prior_commit: true,
        first_crash: Some("awaiting-health"),
        cuts: RECOVERY_ROLLBACK_PATH,
    },
    Scenario {
        case: "recover",
        prior_commit: true,
        first_crash: Some("floor-advanced"),
        cuts: RECOVERY_RESUME_PATH,
    },
];

#[test]
fn every_persisted_activation_cut_resumes_commits_rolls_back_or_halts() {
    support::assert_user_principal_token();
    for scenario in SCENARIOS {
        for &(cut, after) in scenario.cuts {
            // The first update has no superseded previous-known-good to retire.
            if cut == "version-retired" && scenario.case == "first-commit" {
                continue;
            }
            run_crash_cut(scenario, cut, after);
        }
    }
}

fn run_crash_cut(scenario: Scenario, cut: &str, after: AfterCut) {
    let fixture = tempfile::tempdir().expect("activation crash-cut fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let (prior, candidate, prior_previous) = if scenario.prior_commit {
        commit(&trust, "2.0.0");
        ("2.0.0", "3.0.0", Some("1.0.0"))
    } else {
        ("1.0.0", "2.0.0", None)
    };
    let label = format!("{}/{:?}/{cut}", scenario.case, scenario.first_crash);
    let before = observe(&trust);
    let (lost_channel, at_cut) = crash_children(&trust, fixture.path(), scenario, cut, &label);
    assert!(
        [prior, candidate].contains(&at_cut.floor.as_str()),
        "{label}: the floor never drops or skips: {at_cut:?}"
    );
    assert!(
        [prior, candidate].contains(&at_cut.last_known_good.as_str()),
        "{label}: {at_cut:?}"
    );
    assert_remint_boundary(cut, &at_cut, lost_channel, &label);

    let committed_versions = [prior, candidate];
    let rolled_back_versions: Vec<&str> = prior_previous.into_iter().chain([prior]).collect();
    match after {
        AfterCut::OrphanHalts => {
            assert_orphan_halts_then_repairs(&trust, &before, &at_cut, &label, (prior, candidate));
        }
        AfterCut::AlreadyCommitted | AfterCut::AlreadyRolledBack => {
            let committed = after == AfterCut::AlreadyCommitted;
            let (current, previous, retained): (&str, Option<&str>, Vec<&str>) = if committed {
                (candidate, Some(prior), committed_versions.to_vec())
            } else {
                (prior, prior_previous, rolled_back_versions.clone())
            };
            // Commit retires only when a superseded version exists; rollback always does.
            let retired = !committed || scenario.prior_commit;
            let next = if scenario.prior_commit {
                "4.0.0"
            } else {
                "3.0.0"
            };
            assert_leftovers_then_next_commit(
                &trust,
                &at_cut,
                &label,
                (current, previous, &retained, retired),
                next,
            );
        }
        AfterCut::ResumesThenCommits | AfterCut::FinishesCommit => {
            assert_eq!(
                recover_exact(&trust).unwrap_or_else(|error| panic!("{label}: {error}")),
                WindowsActivationOutcome::Committed,
                "{label}"
            );
            assert_resolved(
                &trust,
                candidate,
                Some(prior),
                candidate,
                &committed_versions,
            );
        }
        AfterCut::RollsBack => {
            assert_eq!(
                recover_exact(&trust).unwrap_or_else(|error| panic!("{label}: {error}")),
                WindowsActivationOutcome::RolledBack,
                "{label}"
            );
            assert_resolved(
                &trust,
                prior,
                prior_previous,
                candidate,
                &rolled_back_versions,
            );
        }
    }
}

/// Runs the optional first live-owner crash, then the scenario owner up to `cut`.
///
/// Returns the lifecycle channel journaled before the scenario owner ran and the
/// protected state the scenario owner left at its cut.
fn crash_children(
    trust: &WindowsBaselineTrust,
    root: &std::path::Path,
    scenario: Scenario,
    cut: &str,
    label: &str,
) -> (Option<[u8; 32]>, Observed) {
    if let Some(first) = scenario.first_crash {
        let stdout = support::child(CRASH_HELPER, root, "commit", first, CRASH_EXIT);
        assert!(
            stdout.contains(&format!("KELD_ACTIVATION_CUT={first}")),
            "{stdout}"
        );
    }
    let lost_channel = observe(trust)
        .journal
        .map(|journal| journal.lifecycle_channel_id);
    let stdout = support::child(CRASH_HELPER, root, scenario.case, cut, CRASH_EXIT);
    assert!(
        stdout.contains(&format!("KELD_ACTIVATION_CUT={cut}")),
        "{label}: child must stop at the named boundary: {stdout}"
    );
    (lost_channel, observe(trust))
}

/// At a re-mint cut, a prepared sibling leaves the lost channel journaled and the durable
/// rewrite replaces it.
fn assert_remint_boundary(
    cut: &str,
    at_cut: &Observed,
    lost_channel: Option<[u8; 32]>,
    label: &str,
) {
    if !cut.ends_with("channels-reminted") {
        return;
    }
    let journaled = at_cut
        .journal
        .as_ref()
        .expect("a resumed attempt stays journaled")
        .lifecycle_channel_id;
    let lost = lost_channel.expect("the lost attempt was journaled");
    if cut == "channels-reminted" {
        assert_ne!(
            journaled, lost,
            "{label}: the resumed owner durably re-mints its lifecycle channel"
        );
    } else {
        assert_eq!(
            journaled, lost,
            "{label}: a prepared re-mint is not yet the journaled channel"
        );
    }
}

/// A crash between publication and the journal leaves an orphan: the ordinary loader
/// halts and selects nothing, and only the explicit repair retires it.
fn assert_orphan_halts_then_repairs(
    trust: &WindowsBaselineTrust,
    before: &Observed,
    at_cut: &Observed,
    label: &str,
    (prior, candidate): (&str, &str),
) {
    assert_eq!(at_cut.journal, None, "{label}");
    assert_eq!(
        (&at_cut.floor, &at_cut.current, &at_cut.last_known_good),
        (&before.floor, &before.current, &before.last_known_good),
        "{label}: nothing was selected before the journal"
    );
    let error = load_windows_activation_write_snapshot(trust, &verifier(trust))
        .expect_err("an unjournaled complete version halts startup");
    assert!(
        error.to_string().contains("unreferenced version entry"),
        "{label}: {error}"
    );
    assert_eq!(
        crate::repair_windows_unjournaled_versions(trust, &verifier(trust))
            .unwrap_or_else(|error| panic!("{label}: {error}")),
        1,
        "{label}: the explicit repair retires the crash orphan"
    );
    commit(trust, candidate);
    assert_resolved(
        trust,
        candidate,
        Some(prior),
        candidate,
        &[prior, candidate],
    );
}

/// After a cut that follows journal removal, only never-read leftovers remain: the
/// renamed journal and any retired tree. Both are admitted as diagnostics, and the next
/// transaction removes them before and after its own steps.
fn assert_leftovers_then_next_commit(
    trust: &WindowsBaselineTrust,
    at_cut: &Observed,
    label: &str,
    (current, previous, retained, retired): (&str, Option<&str>, &[&str], bool),
    next: &str,
) {
    assert_eq!(at_cut.journal, None, "{label}");
    assert_eq!(at_cut.pending_records, 1, "{label}: {at_cut:?}");
    assert_eq!(at_cut.current, current, "{label}");
    assert_eq!(at_cut.previous_known_good.as_deref(), previous, "{label}");
    let mut expected = names(retained);
    if retired {
        expected.insert("retired-*".to_owned());
    }
    assert_eq!(at_cut.versions, expected, "{label}");
    commit(trust, next);
    assert_resolved(trust, next, Some(current), next, &[current, next]);
}

fn crash_at_requested_cut(durable: bool, label: &'static str) {
    let requested = std::env::var(CUT_ENV).expect("requested activation cut");
    let reached = if durable {
        label.to_owned()
    } else {
        format!("prepared:{label}")
    };
    if reached == requested {
        println!("KELD_ACTIVATION_CUT={reached}");
        std::io::stdout()
            .flush()
            .expect("flush the exact crash-boundary witness");
        std::process::exit(CRASH_EXIT);
    }
}

#[test]
#[ignore = "private activation crash-cut subprocess entry point"]
fn windows_activation_crash_helper() {
    support::assert_user_principal_token();
    let root = std::path::PathBuf::from(std::env::var_os(ROOT_ENV).expect("fixture root"));
    let case = std::env::var(support::CASE_ENV).expect("activation case");
    let mut trust = support::trust_for(&root.join("KeldPerUserFixture"));
    trust.installation.install_mode = crate::DirectInstallMode::PerUserDirect;
    crate::windows_baseline::CRASH_CUT_HOOK
        .set(crash_at_requested_cut)
        .expect("install the crash-cut hook once");
    if case == "recover" {
        // The parent observed the lost owner's exit before starting this successor.
        let _ = recover_exact(&trust);
        panic!("the requested recovery cut was not reached");
    }
    let candidate = if case.starts_with("first-") {
        "2.0.0"
    } else {
        "3.0.0"
    };
    let attempt = begin(&trust, candidate);
    if case.ends_with("commit") {
        let health = receipt(&attempt);
        let _ = attempt.accept_health(&health);
    } else if case.ends_with("rollback") {
        let retirement = retirement(&attempt);
        let _ = attempt.roll_back(ActivationFailureClass::HealthRejected, &retirement);
    } else {
        panic!("unknown activation case {case}");
    }
    panic!("the requested activation cut was not reached");
}
