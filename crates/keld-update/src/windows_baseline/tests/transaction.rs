//! Common journaled activation over real per-user Windows records and versions.
//!
//! Oracles are on-disk protected records decoded by the canonical codecs, the version
//! directory census, OS sharing behavior, and child-process exit at each persisted cut.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::os::windows::fs::OpenOptionsExt as _;

use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

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
    root.begin_activation(&published, COORDINATOR)
        .expect("journal and select the published candidate")
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

fn assert_unchanged_refusal(result: Result<impl std::fmt::Debug, UpdateError>, step: &str) {
    match result.expect_err("binding mismatch must refuse") {
        UpdateError::Activation {
            step: refused,
            effect: ActivationEffect::ProtectedStateUnchanged,
            ..
        } => assert_eq!(refused, step),
        other => panic!("expected an unchanged {step} refusal, got {other:?}"),
    }
}

#[test]
fn record_replacement_refuses_every_leaf_outside_the_fixed_slots_before_renaming() {
    let fixture = tempfile::tempdir().expect("record slot fixture");
    let path = fixture.path();
    std::fs::write(path.join("install-provenance"), b"protected").expect("existing record");
    std::fs::write(path.join("pending-source"), b"replacement").expect("prepared sibling");
    let parent = support::directory(path);
    for leaf in [
        "install-provenance",
        "activation.lock",
        "bootstrap.lock",
        ".complete",
        "1.0.0",
        "versions",
    ] {
        assert!(
            crate::windows_fs::replace_record_slot(&parent, "pending-source", leaf).is_err(),
            "{leaf} is not a replaceable activation record slot"
        );
    }
    assert_eq!(
        std::fs::read(path.join("install-provenance")).expect("record after refusals"),
        b"protected"
    );
    assert!(
        path.join("pending-source").exists(),
        "a refused replacement never consumes the prepared sibling"
    );

    std::fs::write(path.join("current"), b"previous").expect("existing slot");
    crate::windows_fs::replace_record_slot(&parent, "pending-source", "current")
        .expect("a fixed slot is replaced in place of the existing record");
    assert_eq!(
        std::fs::read(path.join("current")).expect("replaced slot"),
        b"replacement"
    );
    assert!(!path.join("pending-source").exists());
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
fn substituted_health_receipts_refuse_without_protected_writes() {
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
        assert_unchanged_refusal(
            attempt.accept_health(&substituted),
            "health receipt binding",
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
    assert_unchanged_refusal(
        attempt.roll_back(ActivationFailureClass::ProcessCrash, &substituted[0]),
        "process-family retirement binding",
    );
    assert_eq!(
        observe(&trust),
        before,
        "live rollback writes nothing without exact proof"
    );

    for wrong in &substituted {
        let inspection =
            load_windows_recovery_inspection(&trust, &verifier).expect("inspect the lost attempt");
        assert_unchanged_refusal(
            inspection.recover(wrong, COORDINATOR),
            "process-family retirement binding",
        );
        assert_eq!(observe(&trust), before);
    }
    let exact =
        ProcessFamilyRetirement::from_exact_zero_observation(installation, attempt_id, channel);
    let inspection =
        load_windows_recovery_inspection(&trust, &verifier).expect("inspect the lost attempt");
    assert_unchanged_refusal(
        inspection.recover(&exact, flip(COORDINATOR)),
        "coordinator identity",
    );
    assert_eq!(observe(&trust), before);

    assert_eq!(
        recover_exact(&trust).expect("exact retirement recovers the lost attempt"),
        WindowsActivationOutcome::RolledBack,
        "an attempt that lost its owner during health rolls back"
    );
    assert_resolved(&trust, "1.0.0", None, "2.0.0", &["1.0.0"]);
}

#[test]
fn an_open_handle_in_the_retiring_tree_preserves_the_journal_for_recovery() {
    let fixture = tempfile::tempdir().expect("blocked retirement fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let attempt = begin(&trust, "3.0.0");
    let retiree = trust
        .installation
        .update_root
        .join("versions")
        .join("1.0.0")
        .join("content.tar");
    let holder = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&retiree)
        .expect("hold a file inside the version being retired");
    let health = receipt(&attempt);
    let error = attempt
        .accept_health(&health)
        .expect_err("Windows refuses to rename a directory with an open descendant");
    assert!(
        matches!(
            error,
            UpdateError::Activation {
                step: "version retirement",
                effect: ActivationEffect::JournalBoundRecoveryRequired,
                ..
            }
        ),
        "{error:?}"
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

/// Recovers the protected journal with a retirement binding read back from it.
///
/// The caller has already observed the prior owner's exit; that observation is the
/// family-retirement evidence this binding names.
fn recover_exact(trust: &WindowsBaselineTrust) -> Result<WindowsActivationOutcome, UpdateError> {
    let inspection = load_windows_recovery_inspection(trust, &verifier(trust))?;
    let retirement = ProcessFamilyRetirement::from_exact_zero_observation(
        *inspection.lifecycle_installation_id(),
        *inspection.attempt_id(),
        *inspection.lifecycle_channel_id(),
    );
    match inspection.recover(&retirement, COORDINATOR)? {
        WindowsRecoveryOutcome::Resolved(resolution) => {
            assert!(resolution.cleanup_error().is_none());
            Ok(resolution.outcome())
        }
        WindowsRecoveryOutcome::AwaitingHealth(attempt) => {
            assert_eq!(
                observe(trust).journal.map(|journal| journal.phase),
                Some(ActivationPhase::AwaitingHealth)
            );
            let health = receipt(&attempt);
            let resolution = attempt.accept_health(&health)?;
            Ok(resolution.outcome())
        }
    }
}

/// Expected recovery for one persisted cut of the 2.0.0 -> 3.0.0 attempt.
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

const COMMIT_CUTS: [(&str, AfterCut); 16] = [
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

const ROLLBACK_CUTS: [(&str, AfterCut); 6] = [
    ("prepared:rollback-pending", AfterCut::RollsBack),
    ("rollback-pending", AfterCut::RollsBack),
    ("prepared:rollback-target-restored", AfterCut::RollsBack),
    ("rollback-target-restored", AfterCut::RollsBack),
    ("version-retired", AfterCut::RollsBack),
    ("journal-removed", AfterCut::AlreadyRolledBack),
];

#[test]
fn every_persisted_activation_cut_resumes_commits_rolls_back_or_halts() {
    support::assert_user_principal_token();
    for (case, cuts) in [
        ("commit", &COMMIT_CUTS[..]),
        ("rollback", &ROLLBACK_CUTS[..]),
    ] {
        for &(cut, after) in cuts {
            run_crash_cut(case, cut, after);
        }
    }
}

fn run_crash_cut(case: &str, cut: &str, after: AfterCut) {
    let fixture = tempfile::tempdir().expect("activation crash-cut fixture");
    let trust = seed_per_user_baseline(fixture.path());
    commit(&trust, "2.0.0");
    let committed = observe(&trust);

    let stdout = support::child(CRASH_HELPER, fixture.path(), case, cut, CRASH_EXIT);
    assert!(
        stdout.contains(&format!("KELD_ACTIVATION_CUT={cut}")),
        "{case}/{cut}: child must stop at the named boundary: {stdout}"
    );
    let at_cut = observe(&trust);
    assert!(
        ["2.0.0", "3.0.0"].contains(&at_cut.floor.as_str()),
        "{case}/{cut}: the floor never drops or skips: {at_cut:?}"
    );
    assert!(
        at_cut.last_known_good == "2.0.0" || at_cut.last_known_good == "3.0.0",
        "{case}/{cut}: {at_cut:?}"
    );

    match after {
        AfterCut::OrphanHalts => {
            assert_eq!(at_cut.journal, None);
            assert_eq!(
                (&at_cut.floor, &at_cut.current, &at_cut.last_known_good),
                (
                    &committed.floor,
                    &committed.current,
                    &committed.last_known_good
                ),
                "{case}/{cut}: nothing was selected before the journal"
            );
            let error = load_windows_activation_write_snapshot(&trust, &verifier(&trust))
                .expect_err("an unjournaled complete version halts startup");
            assert!(
                error.to_string().contains("unreferenced version entry"),
                "{case}/{cut}: {error}"
            );
        }
        AfterCut::AlreadyCommitted | AfterCut::AlreadyRolledBack => {
            // The journal name is durably gone; only never-read leftovers remain: the
            // renamed journal and the retired tree. Both are admitted as diagnostics,
            // and the next transaction removes them before and after its own steps.
            assert_eq!(at_cut.journal, None, "{case}/{cut}");
            assert_eq!(at_cut.pending_records, 1, "{case}/{cut}: {at_cut:?}");
            assert!(
                at_cut.versions.contains("retired-*"),
                "{case}/{cut}: {at_cut:?}"
            );
            let (current, previous, versions) = if after == AfterCut::AlreadyCommitted {
                ("3.0.0", "2.0.0", ["2.0.0", "3.0.0"])
            } else {
                ("2.0.0", "1.0.0", ["1.0.0", "2.0.0"])
            };
            assert_eq!(at_cut.current, current, "{case}/{cut}");
            assert_eq!(at_cut.previous_known_good.as_deref(), Some(previous));
            let mut retained = names(&versions);
            retained.insert("retired-*".to_owned());
            assert_eq!(at_cut.versions, retained, "{case}/{cut}");
            commit(&trust, "4.0.0");
            assert_resolved(&trust, "4.0.0", Some(current), "4.0.0", &[current, "4.0.0"]);
        }
        AfterCut::ResumesThenCommits | AfterCut::FinishesCommit => {
            assert_eq!(
                recover_exact(&trust).unwrap_or_else(|error| panic!("{case}/{cut}: {error}")),
                WindowsActivationOutcome::Committed,
                "{case}/{cut}"
            );
            assert_resolved(&trust, "3.0.0", Some("2.0.0"), "3.0.0", &["2.0.0", "3.0.0"]);
        }
        AfterCut::RollsBack => {
            assert_eq!(
                recover_exact(&trust).unwrap_or_else(|error| panic!("{case}/{cut}: {error}")),
                WindowsActivationOutcome::RolledBack,
                "{case}/{cut}"
            );
            assert_resolved(&trust, "2.0.0", Some("1.0.0"), "3.0.0", &["1.0.0", "2.0.0"]);
        }
    }
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
    crate::windows_baseline::activate::CRASH_CUT_HOOK
        .set(crash_at_requested_cut)
        .expect("install the crash-cut hook once");
    let attempt = begin(&trust, "3.0.0");
    match case.as_str() {
        "commit" => {
            let health = receipt(&attempt);
            let _ = attempt.accept_health(&health);
        }
        "rollback" => {
            let retirement = retirement(&attempt);
            let _ = attempt.roll_back(ActivationFailureClass::HealthRejected, &retirement);
        }
        other => panic!("unknown activation case {other}"),
    }
    panic!("the requested activation cut was not reached");
}
