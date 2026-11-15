//! Signed same-origin package purge and interrupted-intent recovery acceptance on
//! installed packages.

use crate::profile_state_run_nonce;
use crate::support::installed::{INSTALLER_ENV, SIGNED_HOST_A_P1, fixture_env};
use crate::support::product::ProductFixture;
use crate::support::profile_run::{SignedProfileStateCase, run_signed_profile_state_case};
use crate::support::profile_server::ProfileStateServer;
use crate::support::signed_purge::{assert_signed_purge_success, run_signed_purge_fixture};
use std::env;
use std::net::TcpListener;

#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1, KELD_KEL254_INSTALLER_FIXTURE and the KELD_KEL135_SIGNED_PURGE_FIXTURE package-purge fixture"]
fn kel135_signed_host_purge_removes_same_origin_state() {
    let signed_host = fixture_env(SIGNED_HOST_A_P1);
    let installer = fixture_env(INSTALLER_ENV);
    let signed_purge = env::var_os("KELD_KEL135_SIGNED_PURGE_FIXTURE")
        .expect("KELD_KEL135_SIGNED_PURGE_FIXTURE must point to a signed A/P1 core fixture");
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind purge control");
    let state_server = ProfileStateServer::new();
    let run_nonce = profile_state_run_nonce(&fixture);
    let seeded_state = format!("{run_nonce}-seed");
    let recovered_state = format!("{run_nonce}-recovered");
    let seed = SignedProfileStateCase {
        name: "purge-seed",
        host: &signed_host,
        installer: &installer,
        before: "",
        after: &seeded_state,
    };
    run_signed_profile_state_case(
        &fixture,
        &control_listener,
        &state_server,
        &run_nonce,
        &seed,
    );

    assert_signed_purge_success(run_signed_purge_fixture(&signed_purge, false));

    let recovered = SignedProfileStateCase {
        name: "purge-recovered",
        host: &signed_host,
        installer: &installer,
        before: "",
        after: &recovered_state,
    };
    run_signed_profile_state_case(
        &fixture,
        &control_listener,
        &state_server,
        &run_nonce,
        &recovered,
    );
}

#[test]
#[ignore = "requires KELD_KEL254_SIGNED_HOST_A_P1, KELD_KEL254_INSTALLER_FIXTURE and the KELD_KEL135_SIGNED_PURGE_FIXTURE package-purge fixture"]
fn kel135_signed_host_recovers_an_interrupted_purge() {
    let signed_host = fixture_env(SIGNED_HOST_A_P1);
    let installer = fixture_env(INSTALLER_ENV);
    let signed_purge = env::var_os("KELD_KEL135_SIGNED_PURGE_FIXTURE")
        .expect("KELD_KEL135_SIGNED_PURGE_FIXTURE must point to a signed A/P1 core fixture");
    let fixture = ProductFixture::new();
    let control_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind recovery control");
    let state_server = ProfileStateServer::new();
    let run_nonce = profile_state_run_nonce(&fixture);
    let seeded_state = format!("{run_nonce}-seed");
    let recovered_state = format!("{run_nonce}-recovered");
    let seed = SignedProfileStateCase {
        name: "purge-crash-seed",
        host: &signed_host,
        installer: &installer,
        before: "",
        after: &seeded_state,
    };
    run_signed_profile_state_case(
        &fixture,
        &control_listener,
        &state_server,
        &run_nonce,
        &seed,
    );

    let interrupted = run_signed_purge_fixture(&signed_purge, true);
    assert!(
        !interrupted.status.success(),
        "purge fault fixture unexpectedly survived the post-intent crash"
    );
    let interrupted_stdout =
        String::from_utf8(interrupted.stdout).expect("interrupted purge stdout UTF-8");
    assert!(
        interrupted_stdout.contains("KELD_KEL135_PURGE_FAULT prepared"),
        "purge fault fixture did not reach the durable prepared intent: {interrupted_stdout}"
    );
    assert_signed_purge_success(run_signed_purge_fixture(&signed_purge, false));

    let recovered = SignedProfileStateCase {
        name: "purge-crash-recovered",
        host: &signed_host,
        installer: &installer,
        before: "",
        after: &recovered_state,
    };
    run_signed_profile_state_case(
        &fixture,
        &control_listener,
        &state_server,
        &run_nonce,
        &recovered,
    );
}
