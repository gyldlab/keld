//! Signed saved-media grant revocation acceptance and existing fixture runner.

use crate::support::signed_identity::signed_fixture_profile_namespace;
use std::env;
use std::net::TcpListener;
use std::process::Command;

#[test]
#[ignore = "requires signed KEL-135 identity and media fixtures"]
fn kel135_signed_profile_saved_media_grants_are_revoked() {
    let signed_identity = env::var_os("KELD_KEL135_SIGNED_IDENTITY_FIXTURE")
        .expect("KELD_KEL135_SIGNED_IDENTITY_FIXTURE must point to signed A/P1 core fixture");
    let media_fixture = env::var_os("KELD_KEL135_MEDIA_FIXTURE")
        .expect("KELD_KEL135_MEDIA_FIXTURE must point to the media-acceptance libtest");
    let namespace = signed_fixture_profile_namespace(&signed_identity, None);
    for (kind, run_id) in [
        ("camera", "f1e2d3c4b5a69788796a5b4c3d2e1f00"),
        ("microphone", "001f2e3d4c5b6a798897a6b5c4d3e2f1"),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("reserve media origin");
        let address = listener.local_addr().expect("media origin address");
        drop(listener);
        let address = address.to_string();
        run_signed_media_phase(
            &media_fixture,
            &namespace,
            kind,
            run_id,
            &address,
            "seed",
            "resolved",
        );
        run_signed_media_phase(
            &media_fixture,
            &namespace,
            kind,
            run_id,
            &address,
            "deny",
            "error:NotAllowedError",
        );
    }
}

fn run_signed_media_phase(
    media_fixture: &std::ffi::OsStr,
    namespace: &str,
    kind: &str,
    run_id: &str,
    address: &str,
    phase: &str,
    expected: &str,
) {
    let output = Command::new(media_fixture)
        .args([
            "webview2::media_acceptance::tests::windows_saved_grant_phase_subprocess",
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KELD_PROFILE_SAVED_SIGNED_NAMESPACE", namespace)
        .env("KELD_PROFILE_SAVED_KIND", kind)
        .env("KELD_PROFILE_SAVED_RUN_ID", run_id)
        .env("KELD_PROFILE_SAVED_ADDRESS", address)
        .env("KELD_PROFILE_SAVED_PHASE", phase)
        .output()
        .expect("run signed media phase");
    let stdout = String::from_utf8(output.stdout).expect("signed media stdout UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("signed media stderr UTF-8");
    assert!(
        output.status.success(),
        "signed {kind} {phase} failed: {stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("KELD_PROFILE_SAVED_RESULT") && stdout.contains(expected),
        "signed {kind} {phase} receipt missing expected {expected}: {stdout}"
    );
}
