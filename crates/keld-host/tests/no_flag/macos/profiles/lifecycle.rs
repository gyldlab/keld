use crate::support::PROCESS_DEADLINE;
use crate::support::product::ProductFixture;
use crate::support::profile_evidence::assert_ephemeral_store_report;
use crate::support::profile_evidence::assert_store_report_matches;
use crate::support::profile_evidence::run_signed_identity_report;
use crate::support::profile_evidence::run_signed_purge_report;
use crate::support::profile_evidence::sw_vers_value;
use crate::support::profile_evidence::webkit_version;
use crate::support::profile_origin::ProfileOrigin;
use crate::support::signed_app::build_signed_profile_app;
use crate::support::signed_app::signed_host_executable;
use crate::support::signed_app::spawn_profile_host;
use crate::support::signed_app::stop_profile_host;
use crate::support::signed_app::valid_macos_codesign_hashes;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires the real macOS WKWebView event loop"]
fn kel135_macos_fatal_fixture_command_reports_failure_after_cleanup() {
    let fixture = ProductFixture::new("kel135-macos-fatal-profile-fixture");
    let stage = fixture.stage();
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create fatal fixture support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect fatal fixture support root");
    let mut origin = ProfileOrigin::new();
    let log_path = support_root.join("fatal-fixture.log");
    let log = fs::File::create(&log_path).expect("create fatal fixture log");
    let stderr = log.try_clone().expect("clone fatal fixture log");
    let mut child = Command::new(stage.host())
        .arg("--keld-profile-webview-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", &support_root)
        .env("KELD_PROFILE_FIXTURE_EPHEMERAL", "1")
        .env("KELD_PROFILE_FIXTURE_FATAL_ON_STDIN", "1")
        .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
        .env(
            "KELD_PROFILE_FIXTURE_URL",
            format!("http://{}/read", origin.address),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("launch real macOS fatal fixture host");
    origin
        .wait_for_report("read", None)
        .expect("fatal fixture rendered before command");
    drop(child.stdin.take());
    let deadline = Instant::now() + PROCESS_DEADLINE;
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll fatal fixture host") {
            break status;
        }
        assert!(Instant::now() < deadline, "fatal fixture did not exit");
        thread::sleep(Duration::from_millis(20));
    };
    assert!(!status.success(), "Fatal was reported as a successful Quit");
    let output = fs::read_to_string(&log_path).expect("read fatal fixture result");
    assert_ephemeral_store_report(&output);
    assert!(output.contains("fatal app session command"), "{output}");
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires local Apple code-signing identities and a real signed host fixture"]
fn kel135_macos_same_signed_profile_rejects_concurrent_owner() {
    let fixture = ProductFixture::new("kel135-signed-profile-concurrency");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppConcurrent",
        &format!("dev.keld.fixture.profile.concurrent.{}", std::process::id()),
        &signer,
    );
    let identity = run_signed_identity_report(&app);
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let mut origin = ProfileOrigin::new();

    let mut first = spawn_profile_host(
        signed_host_executable(&app),
        &support_root,
        &origin.address,
        "seed",
        "concurrency-owner",
    );
    let state = origin
        .wait_for_report("seed", Some("keld-kel135-concurrency-state"))
        .expect("owning app rendered the shared origin");
    assert_eq!(
        state.get("local").map(String::as_str),
        Some("keld-kel135-concurrency-state")
    );

    let second = Command::new(signed_host_executable(&app))
        .arg("--keld-profile-webview-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", &support_root)
        .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
        .env(
            "KELD_PROFILE_FIXTURE_URL",
            format!("http://{}/unused", origin.address),
        )
        .stdin(Stdio::null())
        .output()
        .expect("launch competing same-identity host");
    assert!(!second.status.success(), "second same-profile host started");
    let second_error = String::from_utf8_lossy(&second.stderr);
    assert!(
        second_error.contains("already in use"),
        "competing host failed for an unexpected reason: {second_error}"
    );
    stop_profile_host(&mut first);
    let log = fs::read_to_string(support_root.join("concurrency-owner.log"))
        .expect("read active owner WebKit store evidence");
    assert_store_report_matches(&log, &identity["store_uuid"]);
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    eprintln!(
        "KELD_KEL135_MACOS_CONCURRENCY os={} webkit={} team={} identifier={} uuid={} first_owner=active second_owner=rejected_by_lock cleanup=exact-identity-purge",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires local Apple code-signing identities and a real signed host fixture"]
fn kel135_macos_binding_recovers_after_process_crash() {
    let fixture = ProductFixture::new("kel135-signed-profile-binding-recovery");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppBindingRecovery",
        &format!("dev.keld.fixture.profile.binding.{}", std::process::id()),
        &signer,
    );
    let identity = run_signed_identity_report(&app);
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let interrupted = Command::new(signed_host_executable(&app))
        .arg("--keld-profile-webview-fixture-v1")
        .env("KELD_PROFILE_TEST_ROOT", &support_root)
        .env("KELD_PROFILE_TEST_BINDING_CRASH_AFTER_REVERSE", "1")
        .env("KELD_PROFILE_FIXTURE_URL", "http://127.0.0.1:1/unused")
        .stdin(Stdio::null())
        .output()
        .expect("launch binding interruption fixture");
    assert_eq!(interrupted.status.code(), Some(87));
    assert!(
        String::from_utf8_lossy(&interrupted.stderr).contains("after_reverse_record=true"),
        "binding interruption did not follow a durable reverse record"
    );

    let mut origin = ProfileOrigin::new();
    let seeded = origin.run_profile(
        &app,
        &support_root,
        "seed",
        "binding-recovered-seed",
        Some("keld-kel135-binding-recovered"),
    );
    assert_eq!(
        seeded.get("local").map(String::as_str),
        Some("keld-kel135-binding-recovered")
    );
    let report = fs::read_to_string(support_root.join("binding-recovered-seed.log"))
        .expect("read recovered binding store report");
    assert_store_report_matches(&report, &identity["store_uuid"]);
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    let mut recovered_phases = vec!["after-reverse-record"];
    for (phase, expected_status) in [("store-created", 88), ("store-verified", 89)] {
        let phase_app = build_signed_profile_app(
            stage.root(),
            &app_root,
            &format!("ProfileAppBindingRecovery-{phase}"),
            &format!(
                "dev.keld.fixture.profile.binding.{}.{}",
                std::process::id(),
                phase
            ),
            &signer,
        );
        let phase_identity = run_signed_identity_report(&phase_app);
        let phase_support = support_root.join(phase);
        fs::create_dir(&phase_support).expect("create binding-recovery support root");
        fs::set_permissions(&phase_support, fs::Permissions::from_mode(0o700))
            .expect("protect binding-recovery support root");
        let interrupted = Command::new(signed_host_executable(&phase_app))
            .arg("--keld-profile-webview-fixture-v1")
            .env("KELD_PROFILE_TEST_ROOT", &phase_support)
            .env("KELD_PROFILE_TEST_BINDING_CRASH_AFTER", phase)
            .env("KELD_PROFILE_FIXTURE_URL", "http://127.0.0.1:1/unused")
            .stdin(Stdio::null())
            .output()
            .expect("launch binding phase interruption fixture");
        assert_eq!(interrupted.status.code(), Some(expected_status));
        assert!(
            String::from_utf8_lossy(&interrupted.stderr).contains(&format!("after={phase}")),
            "binding interruption missed {phase}"
        );
        let phase_state = origin.run_profile_after_test_boot_change(
            &phase_app,
            &phase_support,
            "seed",
            "binding-phase-recovered",
            Some("keld-kel135-binding-phase"),
        );
        assert_eq!(
            phase_state.get("local").map(String::as_str),
            Some("keld-kel135-binding-phase"),
            "binding recovery after {phase}"
        );
        let report = fs::read_to_string(phase_support.join("binding-phase-recovered.log"))
            .expect("read recovered phase store report");
        assert_store_report_matches(&report, &phase_identity["store_uuid"]);
        assert!(run_signed_purge_report(&phase_app, &phase_support).contains("store_absent=true"));
        recovered_phases.push(phase);
    }
    eprintln!(
        "KELD_KEL135_MACOS_BINDING_RECOVERY os={} webkit={} team={} identifier={} uuid={} crash_phases={} result=recovered-and-opened purge=exact-identity",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
        recovered_phases.join(","),
    );
}
