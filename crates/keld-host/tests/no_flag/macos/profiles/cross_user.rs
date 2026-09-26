use crate::support::cross_user::MacSecondUserPreflightCleanup;
use crate::support::cross_user::MacSecondUserProfileCleanup;
use crate::support::cross_user::account_groups;
use crate::support::cross_user::account_home;
use crate::support::cross_user::account_numeric_value;
use crate::support::cross_user::current_account_numeric_id;
use crate::support::cross_user::make_signed_fixture_readable_by_standard_users;
use crate::support::cross_user::run_as_local_user;
use crate::support::cross_user::run_signed_identity_report_as_user;
use crate::support::cross_user::run_signed_purge_report_as_user;
use crate::support::product::ProductFixture;
use crate::support::profile_evidence::assert_store_report_matches;
use crate::support::profile_evidence::run_signed_identity_report;
use crate::support::profile_evidence::run_signed_purge_report;
use crate::support::profile_evidence::sw_vers_value;
use crate::support::profile_evidence::webkit_version;
use crate::support::profile_origin::ProfileOrigin;
use crate::support::signed_app::build_signed_profile_app;
use crate::support::signed_app::path_text;
use crate::support::signed_app::valid_macos_codesign_hashes;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires a second standard macOS account, an authenticated sudo session, and a signed host fixture"]
fn kel135_macos_second_user_cannot_read_same_signed_profile_state() {
    let username = std::env::var("KELD_KEL135_SECOND_USER")
        .expect("set KELD_KEL135_SECOND_USER to the temporary standard macOS login");
    assert!(
        !username.is_empty()
            && username
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
        "second-user login name must be lowercase ASCII, digits, or underscore"
    );
    let second_uid = account_numeric_value(&username);
    let first_uid = current_account_numeric_id();
    assert!(
        first_uid != second_uid,
        "second user UID must differ from the current user"
    );
    assert!(second_uid >= 501, "second user is a system/service account");
    let groups = account_groups(&username);
    assert!(
        !groups.split_whitespace().any(|group| group == "admin"),
        "second account must be an ordinary non-admin user"
    );
    let home = account_home(&username);
    let current_home = std::env::var_os("HOME").map(PathBuf::from);
    assert!(
        home.starts_with("/Users/") && Some(&home) != current_home.as_ref(),
        "second account must have its own local home directory: {}",
        home.display()
    );

    let sudo_probe = run_as_local_user(&username, "/usr/bin/id", &["-u"], &[]);
    assert!(
        sudo_probe.status.success(),
        "authenticate this Mac's administrator account in Terminal with `sudo -v`, then rerun this acceptance row"
    );
    let run_as_uid_matches = String::from_utf8_lossy(&sudo_probe.stdout)
        .trim()
        .parse::<u32>()
        .ok()
        == Some(second_uid);
    assert!(
        run_as_uid_matches,
        "run-as identity must match the isolated standard user"
    );

    let fixture = ProductFixture::new("kel135-second-user-profile");
    let stage = fixture.stage();
    let shared_temp = tempfile::Builder::new()
        .prefix("keld-kel135-second-user-")
        .tempdir_in("/Users/Shared")
        .expect("create cross-user-readable signed fixture directory");
    fs::set_permissions(shared_temp.path(), fs::Permissions::from_mode(0o755))
        .expect("allow the standard test account to traverse its private fixture directory");
    let first_profile_temp = tempfile::Builder::new()
        .prefix("keld-kel135-first-profile-")
        .tempdir()
        .expect("create persistent first-user test metadata root");
    fs::set_permissions(first_profile_temp.path(), fs::Permissions::from_mode(0o700))
        .expect("protect first-user profile metadata root");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("an Apple Development signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        shared_temp.path(),
        "ProfileAppSecondUser",
        &format!(
            "dev.keld.fixture.profile.second-user.{}",
            std::process::id()
        ),
        &signer,
    );
    make_signed_fixture_readable_by_standard_users(&app);
    let first_identity = run_signed_identity_report(&app);

    let mut origin = ProfileOrigin::new();
    let user_fixture_root = home
        .join("Library/Caches/Keld/KEL-135")
        .join(format!("second-user-{}", std::process::id()));
    let user_temp = user_fixture_root.join("tmp");
    let user_profile_root = user_temp.join("profile");
    let mut preflight_cleanup = MacSecondUserPreflightCleanup {
        username: username.clone(),
        user_fixture_root: user_fixture_root.clone(),
        armed: true,
    };
    let create_roots = run_as_local_user(
        &username,
        "/bin/mkdir",
        &["-p", path_text(&user_temp), path_text(&user_profile_root)],
        &[],
    );
    assert!(
        create_roots.status.success(),
        "create owner-private second-user profile roots: {create_roots:?}"
    );
    let second_codesign = run_as_local_user(
        &username,
        "/usr/bin/codesign",
        &["--verify", "--deep", "--strict", path_text(&app)],
        &[],
    );
    assert!(
        second_codesign.status.success(),
        "second user cannot verify the readable signed fixture: {second_codesign:?}"
    );
    let second_identity = run_signed_identity_report_as_user(&username, &user_temp, &app);
    for field in [
        "team_id",
        "signing_identifier",
        "profile_identity",
        "store_uuid",
    ] {
        assert_eq!(
            first_identity.get(field),
            second_identity.get(field),
            "the OS user is the only identity dimension changed"
        );
    }

    let shared = shared_temp.keep();
    let first_profile_root = first_profile_temp.keep();
    eprintln!(
        "KELD_KEL135_MACOS_SECOND_USER_RECOVERY app={} first_profile_root={} second_profile_root={}",
        app.display(),
        first_profile_root.display(),
        user_profile_root.display(),
    );
    let mut cleanup = MacSecondUserProfileCleanup {
        username: username.clone(),
        user_temp: user_temp.clone(),
        user_fixture_root: user_fixture_root.clone(),
        user_profile_root: user_profile_root.clone(),
        app: app.clone(),
        first_profile_root: first_profile_root.clone(),
        shared_fixture_root: shared.clone(),
        first_profile_purged: false,
        second_profile_purged: false,
        armed: true,
    };
    preflight_cleanup.armed = false;
    let seeded = origin.run_profile(
        &app,
        &first_profile_root,
        "seed",
        "same-user-seed",
        Some("keld-kel135-second-user-state"),
    );
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            seeded.get(key).map(String::as_str),
            Some("keld-kel135-second-user-state"),
            "same-user positive control for {key}"
        );
    }
    assert_eq!(seeded.get("sw").map(String::as_str), Some("true"));

    let (second_user_state, second_user_output) = origin.run_profile_as_user(
        &username,
        &user_temp,
        &user_profile_root,
        &app,
        "read",
        "other-user-read",
        None,
    );
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            second_user_state.get(key).map(String::as_str),
            Some(""),
            "second standard user's same-origin {key} must start empty"
        );
    }
    assert_eq!(
        second_user_state.get("sw").map(String::as_str),
        Some("false")
    );
    let second_user_log = format!(
        "{}{}",
        String::from_utf8_lossy(&second_user_output.stdout),
        String::from_utf8_lossy(&second_user_output.stderr)
    );
    assert_store_report_matches(&second_user_log, &first_identity["store_uuid"]);

    let second_purge =
        run_signed_purge_report_as_user(&username, &user_temp, &user_profile_root, &app);
    assert!(
        second_purge.status.success()
            && String::from_utf8_lossy(&second_purge.stdout)
                .contains(&format!("store_uuid={}", first_identity["store_uuid"]))
            && String::from_utf8_lossy(&second_purge.stdout).contains("store_absent=true"),
        "second user's exact-identity purge failed: {second_purge:?}"
    );
    cleanup.second_profile_purged = true;
    let first_after = origin.run_profile(
        &app,
        &first_profile_root,
        "read",
        "same-user-after-other-user-purge",
        None,
    );
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            first_after.get(key),
            seeded.get(key),
            "second-user access and purge must not alter the first user's {key}"
        );
    }
    assert_eq!(first_after.get("sw"), seeded.get("sw"));
    let first_purge = run_signed_purge_report(&app, &first_profile_root);
    assert!(first_purge.contains("store_absent=true"));
    cleanup.first_profile_purged = true;

    let user_root_cleanup = run_as_local_user(
        &username,
        "/bin/rm",
        &["-rf", "--", path_text(&user_fixture_root)],
        &[],
    );
    assert!(
        user_root_cleanup.status.success(),
        "remove second-user test roots: {user_root_cleanup:?}"
    );
    fs::remove_dir_all(&first_profile_root)
        .expect("remove first-user profile metadata only after WebKit purge verification");
    fs::remove_dir_all(&shared)
        .expect("remove signed fixture after both users pass exact-profile purge");
    cleanup.armed = false;
    eprintln!(
        "KELD_KEL135_MACOS_SECOND_USER_SAFE_TO_DELETE_ACCOUNT exact_purge_a=true exact_purge_b=true second_user_test_root_removed=true"
    );
    eprintln!(
        "KELD_KEL135_MACOS_SECOND_USER os={} webkit={} origin={} team={} identifier={} profile_identity={} store_uuid={} uid_a_and_b_distinct=true state=localStorage,cookie,IndexedDB,CacheStorage,serviceWorker user_b_same_origin=empty user_a_after_b=preserved lifecycle=clean-stop purge=both-user-exact-identity platform_path_acl_claim=none",
        sw_vers_value("-productVersion"),
        webkit_version(),
        origin.address,
        first_identity["team_id"],
        first_identity["signing_identifier"],
        first_identity["profile_identity"],
        first_identity["store_uuid"],
    );
}
