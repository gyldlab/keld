//! KEL-78 / GH #588 native macOS renderer/profile admission controls.
//! These shipping-path diagnostics are not GH-587 product evidence receipts
//! or proof that Keld Strict operating-system containment is available.

use super::project::{Exercise, PROFILE_APP_ENTRY_LINE, Policy, PublicFsProject, SENTINEL};
use super::run_launch;
use crate::boot_admission::assert_invalid_stage_is_resource_free;
use crate::support::admission::NativeAbsenceWatcher;
use std::fs;

impl super::LaunchEvidence {
    pub(super) fn capture_profile_diagnostics(
        &mut self,
        _fixture: &PublicFsProject,
        output: &std::process::Output,
    ) {
        // Keep the no-declaration control too: otherwise a stale Legacy
        // disclosure after removing the key would be unobservable here.
        self.profile_diagnostics =
            Some(String::from_utf8(output.stderr.clone()).expect("profile Host diagnostics UTF-8"));
    }
}

#[test]
fn declared_legacy_preserves_authenticated_public_fs_and_ordered_quit() {
    let fixture = PublicFsProject::new(Policy::Narrow).with_declared_profile("legacy");
    let marker = fixture.profile_entry_marker();
    assert!(!marker.exists(), "app marker existed before fixture launch");

    // Reuse the real no-flag Host/Guardian, one authenticated Bun app-link,
    // allowed broker write/read, denied outside write with unchanged OS
    // sentinel, observer-gated public Quit, and native family/window/stage
    // cleanup. These are all independently asserted in run_launch.
    let evidence = run_launch(&fixture, 0, Exercise::PublicAppComponent);
    assert_legacy_entry_and_disclosure(&fixture, &evidence);
}

/// Additional process/broker/retirement proof through the pre-existing
/// capture-free owner. It does not replace the GUI-presentation case above.
#[test]
fn declared_legacy_broker_and_quit_are_proven_without_graphics_capture() {
    let fixture = PublicFsProject::new(Policy::Narrow).with_declared_profile("legacy");
    let evidence = super::retirement::run_launch_without_window_census(
        &fixture,
        0,
        Exercise::PublicAppComponent,
        false,
    );
    assert_legacy_entry_and_disclosure(&fixture, &evidence);
}

#[test]
fn legacy_forfeit_is_visible_to_the_first_application_statement() {
    let fixture = PublicFsProject::new(Policy::Narrow)
        .with_declared_profile("legacy")
        .with_causal_profile_disclosure();
    let witness = fixture.profile_disclosure_witness();
    assert!(
        !witness.exists(),
        "disclosure witness predated this attempt"
    );
    let evidence = super::retirement::run_launch_without_window_census(
        &fixture,
        0,
        Exercise::PublicAppComponent,
        false,
    );
    assert_eq!(
        fs::read(witness).expect("first synchronous application-body observation"),
        b"visible\n",
        "the canonical forfeit was not visible when the app body began"
    );
    assert_legacy_entry_and_disclosure(&fixture, &evidence);
    // This proves visibility at the fixture's first body statement, not that
    // static dependency evaluation was absent or that a product receipt exists.
}

fn assert_legacy_entry_and_disclosure(fixture: &PublicFsProject, evidence: &super::LaunchEvidence) {
    assert!(evidence.family.is_some(), "no OS-attributed first Bun role");
    assert_eq!(
        fs::read(fixture.profile_entry_marker())
            .expect("real Bun entered before handler registration and Ready"),
        b"entered\n",
        "the Bun entry did not write the independent marker"
    );
    assert_eq!(
        fs::read(&fixture.outside).expect("outside sentinel after product Quit"),
        SENTINEL,
        "Legacy bypassed the existing guard's scope denial"
    );

    let stderr = evidence
        .profile_diagnostics
        .as_deref()
        .expect("shipping Host stderr");
    let forfeiture = "legacy — zero-authority claim forfeited";
    assert_eq!(
        stderr.matches(forfeiture).count(),
        1,
        "exactly one canonical Legacy forfeiture is required"
    );
    assert_eq!(
        stderr.matches(PROFILE_APP_ENTRY_LINE).count(),
        1,
        "the real Bun entry diagnostic was not forwarded exactly once"
    );
    // The assertion covers ordering in the collected diagnostic stream only.
    // Asynchronous Bun log forwarding cannot establish cross-process causal
    // timing. Neither line is a trusted runtime/product receipt.
    assert!(
        stderr.find(forfeiture) < stderr.find(PROFILE_APP_ENTRY_LINE),
        "forfeiture was not recorded ahead of Bun entry in Host diagnostics"
    );
}

#[test]
fn known_uncontained_strict_refuses_before_app_entry_or_native_resources() {
    let fixture = PublicFsProject::new(Policy::Narrow).with_declared_profile("strict");
    assert_strict_refused(&fixture, 0);
}

#[test]
fn fresh_hosts_apply_profile_removal_and_strict_without_cached_legacy() {
    let mut fixture = PublicFsProject::new(Policy::Narrow).with_declared_profile("legacy");
    let first = super::retirement::run_launch_without_window_census(
        &fixture,
        0,
        Exercise::PublicAppComponent,
        false,
    );
    assert_legacy_entry_and_disclosure(&fixture, &first);

    // Product Quit/reap/stage cleanup above precedes the policy change. Only
    // this fixture-owned marker is removed; no process cleanup is substituted.
    fs::remove_file(fixture.profile_entry_marker()).expect("retired generation marker");
    fixture.set_declared_profile(None);
    let second = super::retirement::run_launch_without_window_census(
        &fixture,
        1,
        Exercise::PublicAppComponent,
        false,
    );
    assert_ne!(
        first.family.expect("retired Legacy family").bun,
        second.family.expect("fresh default family").bun,
        "a fresh host must not retain the retired Bun process"
    );
    let diagnostics = second
        .profile_diagnostics
        .expect("default host diagnostics");
    assert!(diagnostics.contains("/profile): unverified"));
    assert!(!diagnostics.contains("/profile): legacy"));
    assert!(!diagnostics.contains("zero-authority claim forfeited"));
    assert!(!fixture.profile_entry_marker().exists());

    fixture.set_declared_profile(Some("strict"));
    assert_strict_refused(&fixture, 2);
}

fn assert_strict_refused(fixture: &PublicFsProject, launch: usize) {
    let marker = fixture.profile_entry_marker();
    // No HTTP observer is needed because the candidate must not execute.
    // Port zero cannot become a shortcut for application admission.
    let prepared = fixture.prepare_component(launch, 0);
    assert!(
        !marker.exists(),
        "staging ran the test-owned application body"
    );
    assert!(
        !prepared.target.exists(),
        "allowed target changed before launch"
    );

    let watcher = NativeAbsenceWatcher::compile(fixture.owned_root());
    // The valid live stdin lease excludes an unrelated missing-lease error.
    // The reused suspended-host watcher samples transient child/listener/
    // native-window resources and verifies no startup attempts or control.
    let output = assert_invalid_stage_is_resource_free(
        &prepared.stage,
        &watcher,
        &fixture.owned_root().join("strict-preflight-control.sock"),
        "known-uncontained-strict",
        "KELD-GUARD008",
        None,
        true,
    );
    let stderr = String::from_utf8(output.stderr).expect("Strict refusal stderr UTF-8");
    assert!(
        !stderr.contains("KELD-GUARD011"),
        "pure proof-missing backstop replaced the known-uncontained GUARD008"
    );
    assert!(
        !stderr.contains("legacy — zero-authority claim forfeited"),
        "Strict request produced Legacy fallback disclosure"
    );
    assert!(
        !marker.exists(),
        "refused Strict candidate entered its Bun application body"
    );
    assert!(
        !prepared.target.exists(),
        "refused candidate wrote an allowed target"
    );
    assert_eq!(
        fs::read(&fixture.outside).expect("outside sentinel after Strict refusal"),
        SENTINEL,
        "refused candidate changed the independent denied target"
    );
}
