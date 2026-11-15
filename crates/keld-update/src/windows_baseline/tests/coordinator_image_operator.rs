//! Operator acceptance for the verified coordinator image (KEL-53 §7 row "10, 17
//! (coordinator image)"; KEL-270 T4d slice S6b2): one real
//! `keld_guard::VerifiedWindowsImage` of a signed build passes through each public entry
//! point, and the journaled `helper_image_blake3` equals the BLAKE3 of every byte of that
//! build, computed here from the file's bytes in memory.
//!
//! Run unelevated as an ordinary user with `KELD_KEL254_SIGNED_HOST` naming a
//! `keld-host.exe` signed by a KEL-135 acceptance publisher with a `keld.app-id/v1:`
//! description (the KEL-254 T3 Part B operator fixture's signed host). The cell writes
//! only inside its own temporary per-user fixtures.

use super::installed_host_operator::{SIGNED_HOST_ENV, env_path};
use super::support::{self, ATTEMPT_OWNER, INITIATING_LOGON};
use super::transaction::{complete, retirement, verifier};
use super::writer::{seed_pending_activation_journal_with, seed_per_user_baseline};
use crate::error::hex_digest;
use crate::records::{ActivationJournal, ActivationPhase, decode_activation_journal};
use crate::windows_baseline::{
    WindowsActivationOutcome, WindowsBaselineTrust, WindowsJournaledAttempt,
    WindowsRecoveryOutcome, load_windows_recovery_inspection,
};

fn journal(trust: &WindowsBaselineTrust) -> Option<ActivationJournal> {
    let path = trust.installation.update_root.join("activation-journal");
    path.exists().then(|| {
        decode_activation_journal(&std::fs::read(path).expect("protected journal bytes"))
            .expect("canonical protected journal")
    })
}

#[test]
#[ignore = "operator cell (KEL-270 T4d S6b2): run unelevated with KELD_KEL254_SIGNED_HOST naming a signed keld-host.exe"]
fn a_verified_signed_build_passes_each_public_entry_point() {
    support::assert_ordinary_token();
    let signed = env_path(SIGNED_HOST_ENV);
    let bytes = std::fs::read(&signed).expect("read the signed build");
    // The independent oracle: BLAKE3 of the signed build's bytes, in memory.
    let expected = *blake3::hash(&bytes).as_bytes();
    let verified = keld_guard::WindowsAuthenticodeImage::open(&signed)
        .and_then(keld_guard::WindowsAuthenticodeImage::verify)
        .expect("the signed build verifies through the KEL-135 owner");
    println!(
        "KELD_KEL270_S6B2_VERIFIED_IMAGE app_id={} bytes={} blake3={}",
        verified.identity().app_id(),
        bytes.len(),
        hex_digest(&expected)
    );

    // begin_activation journals the verified image's digest; recover then accepts the
    // same image for the attempt whose owner was lost after launch.
    let fixture = tempfile::tempdir().expect("verified coordinator fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let (root, stage) = complete(&trust, "2.0.0");
    let minted = root
        .begin_activation(stage, &verified)
        .expect("the verified image mints the attempt");
    let WindowsJournaledAttempt::AwaitingHealth(attempt) = minted
        .journal(ATTEMPT_OWNER, INITIATING_LOGON)
        .expect("journal, publish and select the candidate")
    else {
        panic!("a staged candidate reaches its health decision");
    };
    let journaled = journal(&trust).expect("the live attempt is journaled");
    assert_eq!(journaled.phase, ActivationPhase::AwaitingHealth);
    assert_eq!(
        journaled.helper_image_blake3, expected,
        "begin_activation journals the BLAKE3 of every byte of the verified image"
    );
    println!("KELD_KEL270_S6B2_PASS entry=begin_activation phase=awaiting-health");
    let binding = retirement(&attempt);
    // The owner is lost after launch: AwaitingHealth stays journaled, the lease is free.
    drop(attempt);
    let inspection = load_windows_recovery_inspection(&trust, &verifier(&trust))
        .expect("inspect the lost attempt");
    let WindowsRecoveryOutcome::Resolved(resolution) = inspection
        .recover(&binding, &verified)
        .expect("the verified image recovers the lost attempt")
    else {
        panic!("an awaiting-health attempt that lost its owner rolls back");
    };
    assert_eq!(resolution.outcome(), WindowsActivationOutcome::RolledBack);
    assert_eq!(journal(&trust), None, "resolution removes the journal");
    println!("KELD_KEL270_S6B2_PASS entry=recover outcome=rolled-back");

    // resume_unlaunched accepts a never-launched journal whose digest the oracle
    // computed from the signed build's bytes.
    let fixture = tempfile::tempdir().expect("verified resume fixture");
    let trust = seed_per_user_baseline(fixture.path());
    let (attempt_id, _) =
        seed_pending_activation_journal_with(&trust, ActivationPhase::PublishPending, expected);
    let inspection = load_windows_recovery_inspection(&trust, &verifier(&trust))
        .expect("inspect the unlaunched attempt");
    let minted = inspection
        .resume_unlaunched(&verified)
        .expect("the verified image resumes the unlaunched attempt");
    assert_eq!(minted.attempt_id(), &attempt_id);
    assert_eq!(
        journal(&trust).map(|journal| journal.phase),
        Some(ActivationPhase::PublishPending),
        "minting writes nothing"
    );
    println!(
        "KELD_KEL270_S6B2_PASS entry=resume_unlaunched attempt={}",
        hex_digest(&attempt_id)
    );
}
