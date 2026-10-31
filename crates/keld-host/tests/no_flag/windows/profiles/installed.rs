//! Installed-package refusals (KEL-254 A3 AC2/AC11, T3 Part B): signature success alone
//! never admits installed mode, and an installation never boots a host whose verified
//! publisher or app id differs from the protected record (`KELD-UPDATE-020`, the
//! keld-update signer rule since KEL-270 T4d S9a), nor one whose embedded build
//! expectation names another app than the record (`KELD-UPDATE-003`, the provenance
//! binding checked before the signer rule). These are separate negative controls.
//!
//! Each launch is lease-less, as a user starts the app, from a fresh per-run
//! installation (`support::installed`). These rows are native controls of the installed
//! route, not acceptance of the KEL-19 container (KEL-19 T2).

use crate::support::installed::{INSTALLER_ENV, InstalledApp, SIGNED_HOST_A_P1, fixture_env};
use crate::support::product::ProductFixture;
use std::path::Path;
use std::process::{Command, Output};

/// The other app id: the `keld.app-id/v1:` signature app id of both
/// `KELD_KEL254_SIGNED_HOST_FOREIGN_APP` (whose embedded expectation names the fixture
/// app) and `KELD_KEL254_SIGNED_HOST_OTHER_APP` (whose embedded expectation names this
/// app id too).
const OTHER_APP_ID: &str = "dev.keld.other";

fn refused_before_resources(output: &Output) -> String {
    assert!(!output.status.success(), "the installed host booted");
    let stderr = String::from_utf8(output.stderr.clone()).expect("refusal stderr UTF-8");
    println!(
        "KELD_KEL254_REFUSAL status={} stderr={}",
        output.status,
        stderr.trim_end()
    );
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
    // KEL-270 T4d S9a moved the signer rule into keld-update, which owns it for both images.
    assert!(stderr.contains("KELD-UPDATE-020"), "{stderr}");
    assert!(stderr.contains("records a different publisher"), "{stderr}");
}

#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_FOREIGN_APP (the fixture-app host signed with keld.app-id/v1:dev.keld.other) and KELD_KEL254_INSTALLER_FIXTURE"]
fn kel254_installed_host_signed_for_another_app_is_refused() {
    let fixture = ProductFixture::new();
    // The installer records the embedded fixture app id and this host's publisher, so
    // the provenance expectation matches and only the verified signer's app id differs.
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
    assert!(stderr.contains("KELD-UPDATE-020"), "{stderr}");
    let mismatch = format!(
        "records app id `{}`, not the verified `{OTHER_APP_ID}`",
        installed.recorded_app_id()
    );
    assert!(stderr.contains(&mismatch), "{stderr}");
}

#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1, KELD_KEL254_SIGNED_HOST_OTHER_APP (a host embedding and signed with keld.app-id/v1:dev.keld.other) and KELD_KEL254_INSTALLER_FIXTURE"]
fn kel254_installed_host_embedding_another_apps_expectation_is_refused() {
    let fixture = ProductFixture::new();
    // Install app A first: installing the other app's image directly would record the
    // app id it embeds, and its expectation would match.
    let installed = InstalledApp::install(
        &fixture.project,
        &fixture_env(SIGNED_HOST_A_P1),
        &fixture_env(INSTALLER_ENV),
    );
    // A substituted, validly signed image of the other app in app A's selected host
    // slot: its embedded expectation no longer matches the protected record.
    std::fs::copy(
        Path::new(&fixture_env("KELD_KEL254_SIGNED_HOST_OTHER_APP")),
        installed.host(),
    )
    .expect("substitute the other app's signed image into the selected host slot");
    let output = installed
        .command()
        .output()
        .expect("launch the installed host");
    let stderr = refused_before_resources(&output);
    // The provenance binding refuses before the signer rule runs.
    assert!(stderr.contains("KELD-UPDATE-003"), "{stderr}");
    let mismatch = format!(
        "protected provenance `app.id` mismatch: expected `{OTHER_APP_ID}`, found `{}`",
        installed.recorded_app_id()
    );
    assert!(stderr.contains(&mismatch), "{stderr}");
}
