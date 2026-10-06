//! Installed-package refusals (KEL-254 A3 AC2/AC11, T3 Part B): signature success alone
//! never admits installed mode, and an installation never boots a host whose verified
//! publisher or app id differs from the protected record.
//!
//! Each launch is lease-less, as a user starts the app, from a fresh per-run
//! installation (`support::installed`). These rows are native controls of the installed
//! route, not acceptance of the KEL-19 container (KEL-19 T2).

use crate::support::installed::{INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env};
use crate::support::product::ProductFixture;
use std::path::Path;
use std::process::{Command, Output};

/// The app id in the signature of `KELD_KEL254_SIGNED_HOST_FOREIGN_APP`, whose embedded
/// expectation names the fixture app.
const FOREIGN_SIGNED_APP_ID: &str = "dev.keld.other";

fn refused_before_resources(output: &Output) -> String {
    assert!(!output.status.success(), "the installed host booted");
    let stderr = String::from_utf8(output.stderr.clone()).expect("refusal stderr UTF-8");
    assert!(stderr.contains("listener=0 child=0 window=0"), "{stderr}");
    stderr
}

#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1: an operator-signed host that embeds the fixture expectation"]
fn kel254_signed_host_outside_an_installation_is_refused_before_resources() {
    let fixture = ProductFixture::new();
    let stage =
        keld_cli::boot::stage_dev_boot(&fixture.project, Path::new(&fixture_env(SIGNED_HOST_A_P1)))
            .expect("stage the signed embedded host");
    let output = Command::new(stage.host())
        .current_dir(stage.root())
        .env_remove("KELD_DEV_LEASE")
        .output()
        .expect("launch the staged signed host without a dev lease");
    let stderr = refused_before_resources(&output);
    // Verified and carrying its expectation, the host still locates no installation.
    assert!(stderr.contains("KELD-UPDATE-018"), "{stderr}");
    assert!(stderr.contains("locator refused"), "{stderr}");
}

#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1, KELD_KEL254_SIGNED_HOST_A_P2 (another publisher) and KELD_KEL254_INSTALLER_FIXTURE"]
fn kel254_installed_host_under_another_recorded_publisher_is_refused() {
    let fixture = ProductFixture::new();
    let installed = InstalledApp::install_recording(
        &fixture.project,
        &fixture_env(SIGNED_HOST_A_P1),
        &fixture_env(INSTALLER_ENV),
        Some(&fixture_env("KELD_KEL254_SIGNED_HOST_A_P2")),
    );
    let output = installed
        .command()
        .output()
        .expect("launch the installed host");
    let stderr = refused_before_resources(&output);
    assert!(stderr.contains("KELD-WV-009"), "{stderr}");
    assert!(stderr.contains("records a different publisher"), "{stderr}");
}

#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_FOREIGN_APP (the fixture-app host signed with keld.app-id/v1:dev.keld.other) and KELD_KEL254_INSTALLER_FIXTURE"]
fn kel254_installed_host_signed_for_another_app_is_refused() {
    let fixture = ProductFixture::new();
    let installed = InstalledApp::install(
        &fixture.project,
        &fixture_env("KELD_KEL254_SIGNED_HOST_FOREIGN_APP"),
        &fixture_env(INSTALLER_ENV),
    );
    let output = installed
        .command()
        .output()
        .expect("launch the installed host");
    let stderr = refused_before_resources(&output);
    assert!(stderr.contains("KELD-WV-009"), "{stderr}");
    let mismatch = format!(
        "records app id `{}`, not the verified `{FOREIGN_SIGNED_APP_ID}`",
        installed.recorded_app_id()
    );
    assert!(stderr.contains(&mismatch), "{stderr}");
}
