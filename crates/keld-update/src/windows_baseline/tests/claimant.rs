//! The connect-back claimant's reads over real per-user Windows state (KEL-53 §4
//! "Candidate connect-back", criterion 20; KEL-270 T4d S6b).
//!
//! Oracles are Windows sharing behavior on the owner's share-zero lease and on records
//! held with share mode zero, the typed effect and step of each refusal, the exact bytes
//! of every protected record before and after, the owner's own expected digest, and the
//! BLAKE3 of the owner image computed from its bytes in memory.

use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

use super::locate::{expected_for, host_path, open_image, state};
use super::support::{self, ATTEMPT_OWNER, host_package_content};
use super::transaction::{begin_by, commit_with, downgrade_journal_to_v1, receipt};
use super::writer::seed_per_user_baseline_with;
use crate::records::PointerKind;
use crate::windows_baseline::{
    WindowsActivationAttempt, WindowsActivationOutcome, WindowsBaselineTrust,
    WindowsCandidateClaimant, WindowsHealthAcceptedAttempt, locate_candidate_claimant,
};
use crate::{ActivationEffect, AttemptOwner, DirectInstallMode, UpdateError};

/// Every mutable record of an installation that has committed one update.
const MUTABLE_RECORDS: [&str; 5] = [
    "activation-journal",
    "current",
    "version-floor",
    "last-known-good",
    "previous-known-good",
];

/// Several read buffers long with no repeating block, so a digest of part of the image
/// differs from the whole image's.
fn owner_image_bytes() -> Vec<u8> {
    (0..40_000_u32)
        .map(|index| u8::try_from(index % 241).expect("below 241"))
        .collect()
}

/// A per-user installation that committed 2.0.0 and whose owner, with its own image
/// outside the installation, has 3.0.0 in `AwaitingHealth`.
struct Candidate {
    fixture: tempfile::TempDir,
    trust: WindowsBaselineTrust,
    owner_image: PathBuf,
}

impl Candidate {
    fn begin() -> (Self, WindowsActivationAttempt) {
        let fixture = tempfile::tempdir().expect("connect-back claimant fixture");
        let content = host_package_content();
        let trust = seed_per_user_baseline_with(fixture.path(), &content);
        commit_with(&trust, "2.0.0", &content);
        let owner_image = fixture.path().join("owner-host.exe");
        let bytes = owner_image_bytes();
        std::fs::write(&owner_image, &bytes).expect("write the owner image");
        // The independent oracle: BLAKE3 of the image bytes, in memory.
        let attempt = begin_by(&trust, "3.0.0", &content, *blake3::hash(&bytes).as_bytes());
        (
            Self {
                fixture,
                trust,
                owner_image,
            },
            attempt,
        )
    }

    /// Locates the claimant from `version`'s `keld-host.exe`, as the launched candidate
    /// does from its verified image.
    fn claimant(&self, version: &str) -> Result<WindowsCandidateClaimant, UpdateError> {
        let path = host_path(&self.trust, version);
        let executable = open_image(&path);
        locate_candidate_claimant(
            &path,
            &executable,
            &expected_for(&self.trust),
            &self.trust.publisher_scope,
            &self.trust.installation.app_id,
        )
    }

    fn update(&self, leaf: &str) -> PathBuf {
        self.trust.installation.update_root.join(leaf)
    }

    fn version(&self, version: &str) -> PathBuf {
        self.update("versions").join(version)
    }
}

/// Opens `path` for reading with share mode zero, as a writer that excludes every
/// reader would.
fn hold_exclusively(path: &Path) -> std::fs::File {
    OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap_or_else(|error| panic!("hold {} with share mode zero: {error}", path.display()))
}

/// The negative control of a hold: a snapshot reader's open is a sharing violation.
fn assert_held(path: &Path) {
    let error = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .expect_err("a held record refuses a reader");
    assert_eq!(
        error.raw_os_error(),
        Some(ERROR_SHARING_VIOLATION.cast_signed()),
        "{}: {error}",
        path.display()
    );
}

#[test]
fn the_claimant_locates_from_immutable_provenance_while_every_mutable_record_is_held() {
    support::assert_user_principal_token();
    let (candidate, attempt) = Candidate::begin();
    let before = state(&candidate.trust);
    let held: Vec<std::fs::File> = MUTABLE_RECORDS
        .iter()
        .map(|leaf| hold_exclusively(&candidate.update(leaf)))
        .collect();
    for leaf in MUTABLE_RECORDS.iter().chain(&["activation.lock"]) {
        assert_held(&candidate.update(leaf));
    }

    let claimant = candidate
        .claimant("3.0.0")
        .expect("the claimant reads only its immutable provenance before its claim");
    assert_eq!(claimant.install_mode(), DirectInstallMode::PerUserDirect);
    assert_eq!(
        claimant.lifecycle_installation_id(),
        attempt.lifecycle_installation_id(),
        "KELD-AH1 carries the installation ID the owner minted its attempt under"
    );
    drop(held);
    assert_eq!(state(&candidate.trust), before, "locating writes nothing");
}

#[test]
fn the_accepted_candidate_boots_from_its_own_tree_while_the_owner_holds_the_lease() {
    support::assert_user_principal_token();
    let (candidate, attempt) = Candidate::begin();
    assert_held(&candidate.update("activation.lock"));
    let before = state(&candidate.trust);
    let owner_image = open_image(&candidate.owner_image);
    let boot = candidate
        .claimant("3.0.0")
        .expect("locate the claimant")
        .read_candidate_boot(
            attempt.attempt_id(),
            attempt.health_channel_id(),
            ATTEMPT_OWNER,
            &owner_image,
        )
        .expect("the accepted candidate reads its boot selection without the lease");
    assert_eq!(
        boot.health_receipt_digest(),
        attempt.health_receipt_digest(),
        "the candidate's KELD-AB1 digest is the one its owner expects"
    );
    assert_eq!(boot.selection().artifact(), attempt.candidate());
    assert_eq!(
        Some(boot.selection().tree_root()),
        host_path(&candidate.trust, "3.0.0").parent(),
        "the candidate selects its own tree"
    );
    assert_eq!(
        state(&candidate.trust),
        before,
        "the boot read writes nothing"
    );
    // The boot read closed every mutable record: each admits a holder of share mode zero.
    for leaf in MUTABLE_RECORDS {
        drop(hold_exclusively(&candidate.update(leaf)));
    }

    // The owner replaces mutable records and retires a superseded tree during candidate
    // health, while the candidate's immutable pins hold its own tree.
    let health = receipt(&attempt);
    let resolution = attempt
        .accept_health(&health)
        .and_then(WindowsHealthAcceptedAttempt::complete)
        .expect("the owner commits while the candidate's selection is held");
    assert_eq!(resolution.outcome(), WindowsActivationOutcome::Committed);
    assert!(candidate.version("3.0.0").is_dir());
    assert!(
        !candidate.version("1.0.0").exists(),
        "the superseded tree was retired"
    );
    let selection = boot.into_selection();
    assert!(selection.tree_root().join("keld-host.exe").is_file());
}

/// How one refused boot read departs from the accepted claim.
#[derive(Debug, Clone, Copy)]
enum Departure {
    InstallMode,
    NoJournal,
    Phase,
    Attempt,
    HealthChannel,
    V1Journal,
    OwnerProcess,
    OwnerCreationTime,
    OwnerImage,
    Tree,
    State,
    Census,
    Completion,
    Policy,
    Scope,
}

/// Each departure and the step its typed `WriterActive` refusal names. Each case leaves
/// every earlier check satisfied, so a removed or reordered check names another step.
const DEPARTURES: [(Departure, &str); 15] = [
    (Departure::InstallMode, "candidate install mode"),
    (Departure::NoJournal, "candidate attempt"),
    (Departure::Phase, "candidate attempt phase"),
    (Departure::Attempt, "candidate attempt identity"),
    (Departure::HealthChannel, "candidate health channel"),
    (Departure::V1Journal, "candidate attempt owner"),
    (Departure::OwnerProcess, "candidate owner process"),
    (
        Departure::OwnerCreationTime,
        "candidate owner creation time",
    ),
    (Departure::OwnerImage, "candidate owner image"),
    (Departure::Tree, "candidate tree"),
    (Departure::State, "candidate attempt state"),
    (Departure::Census, "activation versions"),
    (Departure::Completion, "completion marker"),
    (Departure::Policy, "package policy"),
    (Departure::Scope, "activation pointer scope"),
];

#[test]
fn every_candidate_boot_departure_refuses_with_a_typed_writer_active_and_writes_nothing() {
    support::assert_user_principal_token();
    for (departure, step) in DEPARTURES {
        let refusal = refused_boot(departure);
        match refusal {
            UpdateError::Activation {
                step: refused,
                effect: ActivationEffect::WriterActive,
                ..
            } => assert_eq!(refused, step, "{departure:?}: {refusal}"),
            other => panic!("{departure:?} must refuse with WriterActive at {step}: {other:?}"),
        }
    }
}

/// Builds one departure on a fresh installation and returns the boot read's refusal,
/// after asserting that the read wrote nothing.
fn refused_boot(departure: Departure) -> UpdateError {
    let (candidate, attempt) = Candidate::begin();
    let mut attempt_id = *attempt.attempt_id();
    let mut health_channel_id = *attempt.health_channel_id();
    let mut owner = ATTEMPT_OWNER;
    let mut owner_image = candidate.owner_image.clone();
    let mut version = "3.0.0";
    let mut live = Some(attempt);
    match departure {
        Departure::InstallMode => {}
        Departure::NoJournal => {
            let attempt = live.take().expect("live attempt");
            let health = receipt(&attempt);
            attempt
                .accept_health(&health)
                .and_then(WindowsHealthAcceptedAttempt::complete)
                .expect("the attempt commits");
        }
        Departure::Phase => {
            let attempt = live.take().expect("live attempt");
            let health = receipt(&attempt);
            drop(
                attempt
                    .accept_health(&health)
                    .expect("health is durably accepted"),
            );
        }
        Departure::Attempt => attempt_id[0] ^= 1,
        // A real identity of the same attempt, but not its health channel.
        Departure::HealthChannel => {
            health_channel_id = *live.as_ref().expect("live attempt").lifecycle_channel_id();
        }
        Departure::V1Journal => downgrade_journal_to_v1(&candidate.trust),
        Departure::OwnerProcess => {
            owner = AttemptOwner::new(owner.process_id() + 1, owner.creation_time());
        }
        Departure::OwnerCreationTime => {
            owner = AttemptOwner::new(owner.process_id(), owner.creation_time() + 1);
        }
        Departure::OwnerImage => {
            // The same length, one byte apart: a digest over every byte tells them apart.
            let mut other = owner_image_bytes();
            other[20_000] ^= 1;
            owner_image = candidate.fixture.path().join("other-host.exe");
            std::fs::write(&owner_image, other).expect("write another image");
        }
        // The prior known-good tree holds an equally verified keld-host.exe.
        Departure::Tree => version = "2.0.0",
        Departure::State
        | Departure::Census
        | Departure::Completion
        | Departure::Policy
        | Departure::Scope => damage_records(&candidate, departure, &mut live),
    }
    let before = state(&candidate.trust);
    let mut claimant = candidate
        .claimant(version)
        .unwrap_or_else(|error| panic!("{departure:?}: the claimant locates: {error}"));
    if matches!(departure, Departure::InstallMode) {
        claimant = claimant.with_install_mode(DirectInstallMode::MachineUacDirect);
    }
    let image = open_image(&owner_image);
    let refusal = claimant
        .read_candidate_boot(&attempt_id, &health_channel_id, owner, &image)
        .expect_err("the departure refuses the boot read");
    assert_eq!(
        state(&candidate.trust),
        before,
        "{departure:?}: a refused boot read writes nothing"
    );
    drop(live);
    refusal
}

/// Damages what the boot read validates after its claim binding. Damage inside the
/// candidate's version needs a lost owner first: the live owner's version pins share no
/// write access.
fn damage_records(
    candidate: &Candidate,
    departure: Departure,
    live: &mut Option<WindowsActivationAttempt>,
) {
    match departure {
        Departure::State => {
            let last_known_good = decode_pointer(candidate, "last-known-good");
            write_pointer(candidate, "current", PointerKind::Current, &last_known_good);
        }
        Departure::Census => {
            std::fs::create_dir(candidate.version("9.9.9")).expect("an unreferenced version entry");
        }
        Departure::Completion => {
            // The owner is lost: its lease and version pins are released, the journal stays.
            drop(live.take());
            let marker = candidate.version("3.0.0").join(".complete");
            let mut complete = crate::records::decode_complete(
                &std::fs::read(&marker).expect("candidate completion bytes"),
            )
            .expect("canonical completion record");
            complete.artifact.content_blake3[0] ^= 1;
            std::fs::write(
                &marker,
                crate::records::encode_complete(&complete.artifact, complete.content_size)
                    .expect("encode completion"),
            )
            .expect("rewrite the candidate's completion record");
        }
        Departure::Policy => {
            drop(live.take());
            std::fs::write(
                candidate
                    .version("3.0.0")
                    .join("tree")
                    .join(keld_pack::UPDATE_POLICY_PATH),
                b"{\"schema\":1,\"dataMigration\":\"rewrite\"}\n",
            )
            .expect("change the candidate's package policy");
        }
        Departure::Scope => {
            drop(live.take());
            move_to_another_app(candidate);
        }
        other => panic!("{other:?} damages no record"),
    }
}

fn decode_pointer(candidate: &Candidate, leaf: &str) -> crate::ArtifactIdentity {
    let kind = match leaf {
        "current" => PointerKind::Current,
        "last-known-good" => PointerKind::LastKnownGood,
        _ => PointerKind::PreviousKnownGood,
    };
    crate::records::decode_pointer(
        kind,
        &std::fs::read(candidate.update(leaf)).expect("protected pointer bytes"),
    )
    .expect("canonical pointer")
}

/// Rewrites one pointer in place, keeping the record's protected descriptor.
fn write_pointer(
    candidate: &Candidate,
    leaf: &str,
    kind: PointerKind,
    artifact: &crate::ArtifactIdentity,
) {
    std::fs::write(
        candidate.update(leaf),
        crate::records::encode_pointer(kind, artifact).expect("encode pointer"),
    )
    .expect("rewrite pointer");
}

/// Renames the application in every record the boot read checks against each other: the
/// journal's artifacts, the three pointers and the candidate's completion record. They
/// stay mutually consistent, so only the trusted baseline's scope tells them apart.
fn move_to_another_app(candidate: &Candidate) {
    const OTHER: &str = "dev.keld.other";
    let journal_path = candidate.update("activation-journal");
    let mut journal = crate::records::decode_activation_journal(
        &std::fs::read(&journal_path).expect("journal bytes"),
    )
    .expect("canonical journal");
    for artifact in [
        &mut journal.candidate,
        &mut journal.rollback_target,
        &mut journal.prior_last_known_good,
    ]
    .into_iter()
    .chain(journal.prior_previous_known_good.as_mut())
    {
        OTHER.clone_into(&mut artifact.app_id);
    }
    std::fs::write(
        &journal_path,
        crate::records::encode_activation_journal(&journal).expect("encode journal"),
    )
    .expect("rewrite journal");
    for (leaf, kind) in [
        ("current", PointerKind::Current),
        ("last-known-good", PointerKind::LastKnownGood),
        ("previous-known-good", PointerKind::PreviousKnownGood),
    ] {
        let mut artifact = decode_pointer(candidate, leaf);
        OTHER.clone_into(&mut artifact.app_id);
        write_pointer(candidate, leaf, kind, &artifact);
    }
    let marker = candidate.version("3.0.0").join(".complete");
    let mut complete =
        crate::records::decode_complete(&std::fs::read(&marker).expect("completion bytes"))
            .expect("canonical completion record");
    OTHER.clone_into(&mut complete.artifact.app_id);
    std::fs::write(
        &marker,
        crate::records::encode_complete(&complete.artifact, complete.content_size)
            .expect("encode completion"),
    )
    .expect("rewrite completion");
}
