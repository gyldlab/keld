use crate::support::product::ProductFixture;
use crate::support::profile_evidence::assert_ephemeral_store_report;
use crate::support::profile_evidence::assert_store_report_matches;
use crate::support::profile_evidence::run_signed_identity_report;
use crate::support::profile_evidence::run_signed_purge_crash_probe;
use crate::support::profile_evidence::run_signed_purge_report;
use crate::support::profile_evidence::sw_vers_value;
use crate::support::profile_evidence::webkit_version;
use crate::support::profile_origin::ProfileOrigin;
use crate::support::signed_app::build_signed_profile_app;
use crate::support::signed_app::valid_macos_codesign_hashes;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires local Apple code-signing identities and a real signed host fixture"]
fn kel135_macos_signed_profiles_isolate_same_origin_state_across_launches() {
    let fixture = ProductFixture::new("kel135-signed-profile-ab");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signing_hashes = valid_macos_codesign_hashes();
    let signer = signing_hashes
        .first()
        .expect("a valid Apple code-signing identity is required");
    let fixture_id = format!("{}", std::process::id());
    let app_a = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppA",
        &format!("dev.keld.fixture.profile.a.{fixture_id}"),
        signer,
    );
    let app_b = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppB",
        &format!("dev.keld.fixture.profile.b.{fixture_id}"),
        signer,
    );
    let identity_a = run_signed_identity_report(&app_a);
    let identity_b = run_signed_identity_report(&app_b);
    assert_eq!(identity_a["team_id"], identity_b["team_id"]);
    assert_ne!(
        identity_a["signing_identifier"],
        identity_b["signing_identifier"]
    );
    assert_ne!(identity_a["store_uuid"], identity_b["store_uuid"]);

    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let mut origin = ProfileOrigin::new();

    let seeded = origin.run_profile(
        &app_a,
        &support_root,
        "seed",
        "app-a-seed",
        Some("keld-kel135-A-state"),
    );
    assert_eq!(
        seeded.get("local"),
        Some(&String::from("keld-kel135-A-state"))
    );
    assert_eq!(
        seeded.get("cookie"),
        Some(&String::from("keld-kel135-A-state"))
    );
    assert_eq!(
        seeded.get("idb"),
        Some(&String::from("keld-kel135-A-state"))
    );
    assert_eq!(
        seeded.get("cache"),
        Some(&String::from("keld-kel135-A-state"))
    );
    assert_eq!(seeded.get("sw").map(String::as_str), Some("true"));

    let persisted_a = origin.run_profile(&app_a, &support_root, "read", "app-a-read", None);
    for key in ["local", "cookie", "idb", "cache", "sw"] {
        assert_eq!(
            persisted_a.get(key),
            seeded.get(key),
            "same app {key} state"
        );
    }

    let isolated_b = origin.run_profile(&app_b, &support_root, "read", "app-b-read", None);
    assert_eq!(isolated_b.get("local").map(String::as_str), Some(""));
    assert_eq!(isolated_b.get("cookie").map(String::as_str), Some(""));
    assert_eq!(isolated_b.get("idb").map(String::as_str), Some(""));
    assert_eq!(isolated_b.get("cache").map(String::as_str), Some(""));
    assert_eq!(isolated_b.get("sw").map(String::as_str), Some("false"));

    let log_a = fs::read_to_string(support_root.join("app-a-read.log"))
        .expect("read signed app A report log");
    let log_b = fs::read_to_string(support_root.join("app-b-read.log"))
        .expect("read signed app B report log");
    assert_store_report_matches(&log_a, &identity_a["store_uuid"]);
    assert_store_report_matches(&log_b, &identity_b["store_uuid"]);
    let purge_crash = run_signed_purge_crash_probe(&app_a, &support_root);
    assert_eq!(purge_crash.status.code(), Some(86));
    assert!(
        String::from_utf8_lossy(&purge_crash.stderr).contains("after_removal_callback=true"),
        "purge interruption did not occur after WebKit's removal barrier"
    );
    let purge_a = run_signed_purge_report(&app_a, &support_root);
    let purge_b = run_signed_purge_report(&app_b, &support_root);
    assert!(purge_a.contains(&format!("store_uuid={}", identity_a["store_uuid"])));
    assert!(purge_a.contains("store_absent=true"));
    assert!(purge_b.contains(&format!("store_uuid={}", identity_b["store_uuid"])));
    assert!(purge_b.contains("store_absent=true"));
    eprintln!(
        "KELD_KEL135_MACOS_APP_AB os={} webkit={} origin={} app_a_team={} app_a_identifier={} app_a_uuid={} app_b_team={} app_b_identifier={} app_b_uuid={} state=localStorage,cookie,indexedDB,CacheStorage,serviceWorker lifecycle=clean-stop purge=callback-and-enumeration-and-recovery negative_control=same-origin-empty",
        sw_vers_value("-productVersion"),
        webkit_version(),
        origin.address,
        identity_a["team_id"],
        identity_a["signing_identifier"],
        identity_a["store_uuid"],
        identity_b["team_id"],
        identity_b["signing_identifier"],
        identity_b["store_uuid"],
    );
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
fn kel135_macos_dev_profiles_are_ephemeral_across_launches() {
    let fixture = ProductFixture::new("kel135-dev-ephemeral-profile");
    let stage = fixture.stage();
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let mut origin = ProfileOrigin::new();

    let seeded = origin.run_ephemeral_profile(
        stage.host(),
        &support_root,
        "seed",
        "dev-seed",
        Some("keld-kel135-dev-state"),
    );
    assert_eq!(
        seeded.get("local").map(String::as_str),
        Some("keld-kel135-dev-state")
    );
    assert_eq!(
        seeded.get("cookie").map(String::as_str),
        Some("keld-kel135-dev-state")
    );
    assert_eq!(
        seeded.get("idb").map(String::as_str),
        Some("keld-kel135-dev-state")
    );
    assert_eq!(
        seeded.get("cache").map(String::as_str),
        Some("keld-kel135-dev-state")
    );
    assert_eq!(seeded.get("sw").map(String::as_str), Some("true"));

    let next_launch =
        origin.run_ephemeral_profile(stage.host(), &support_root, "read", "dev-read", None);
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            next_launch.get(key).map(String::as_str),
            Some(""),
            "dev {key}"
        );
    }
    assert_eq!(next_launch.get("sw").map(String::as_str), Some("false"));
    let first_log = fs::read_to_string(support_root.join("dev-seed.log"))
        .expect("read first ephemeral store evidence");
    let second_log = fs::read_to_string(support_root.join("dev-read.log"))
        .expect("read second ephemeral store evidence");
    assert_ephemeral_store_report(&first_log);
    assert_ephemeral_store_report(&second_log);
    eprintln!(
        "KELD_KEL135_MACOS_DEV_EPHEMERAL os={} webkit={} origin={} first_launch_state=stored next_launch_state=empty store_identifier=none persistent=false",
        sw_vers_value("-productVersion"),
        webkit_version(),
        origin.address,
    );
}
