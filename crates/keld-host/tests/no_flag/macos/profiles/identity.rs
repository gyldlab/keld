use crate::support::product::ProductFixture;
use crate::support::profile_evidence::run_signed_identity_report;
use crate::support::profile_evidence::sw_vers_value;
use crate::support::signed_app::build_signed_profile_app;
use crate::support::signed_app::copy_profile_fixture_tree;
use crate::support::signed_app::signed_host_executable;
use crate::support::signed_app::valid_macos_codesign_hashes;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires local Apple code-signing identities and a real signed host fixture"]
fn kel135_macos_package_identity_uses_only_validated_running_signature_facts() {
    let fixture = ProductFixture::new("kel135-signed-profile-host");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed fixture parent");
    let signing_hashes = valid_macos_codesign_hashes();
    assert!(
        !signing_hashes.is_empty(),
        "no valid Apple code-signing identity is available"
    );

    let app_a = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppA",
        "dev.keld.fixture.profile.a",
        &signing_hashes[0],
    );
    let app_b = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppB",
        "dev.keld.fixture.profile.b",
        &signing_hashes[0],
    );
    let identity_a = run_signed_identity_report(&app_a);
    let identity_b = run_signed_identity_report(&app_b);
    assert_eq!(identity_a["team_id"], identity_b["team_id"]);
    assert_ne!(
        identity_a["signing_identifier"],
        identity_b["signing_identifier"]
    );
    assert_ne!(
        identity_a["profile_identity"],
        identity_b["profile_identity"]
    );
    assert_ne!(identity_a["store_uuid"], identity_b["store_uuid"]);

    let mut other_publisher = None;
    for (index, signer) in signing_hashes.iter().skip(1).enumerate() {
        let candidate = build_signed_profile_app(
            stage.root(),
            &app_root,
            &format!("OtherPublisher{index}"),
            "dev.keld.fixture.profile.a",
            signer,
        );
        let observed = run_signed_identity_report(&candidate);
        if observed["team_id"] != identity_a["team_id"] {
            other_publisher = Some((candidate, observed));
            break;
        }
    }
    let (_other_publisher_app, identity_other) = other_publisher
        .expect("a valid different-publisher signing fixture is required for KEL-135/T3");
    assert_eq!(
        identity_a["signing_identifier"],
        identity_other["signing_identifier"]
    );
    assert_ne!(identity_a["team_id"], identity_other["team_id"]);
    assert_ne!(
        identity_a["profile_identity"],
        identity_other["profile_identity"]
    );
    assert_ne!(identity_a["store_uuid"], identity_other["store_uuid"]);

    let ad_hoc_app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppAdHoc",
        &format!("dev.keld.fixture.profile.adhoc.{}", std::process::id()),
        "-",
    );
    let ad_hoc_verification = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&ad_hoc_app)
        .output()
        .expect("verify ad-hoc control signature");
    assert!(
        ad_hoc_verification.status.success(),
        "ad-hoc control must have an intact signature: {ad_hoc_verification:?}"
    );
    let ad_hoc_output = Command::new(signed_host_executable(&ad_hoc_app))
        .arg("--keld-profile-identity-fixture-v1")
        .output()
        .expect("run ad-hoc identity negative control");
    let ad_hoc_text = format!(
        "{}{}",
        String::from_utf8_lossy(&ad_hoc_output.stdout),
        String::from_utf8_lossy(&ad_hoc_output.stderr)
    );
    assert!(!ad_hoc_output.status.success());
    assert!(
        !ad_hoc_text.contains("KELD_KEL135_SIGNED_IDENTITY"),
        "non-Apple ad-hoc signature disclosed package identity: {ad_hoc_text}"
    );

    let invalid_app = app_root.join("ProfileAppInvalid");
    fs::create_dir(&invalid_app).expect("create invalid-signature fixture directory");
    fs::set_permissions(&invalid_app, fs::Permissions::from_mode(0o700))
        .expect("protect invalid-signature fixture directory");
    copy_profile_fixture_tree(&app_a, &invalid_app);
    let invalid_executable = signed_host_executable(&invalid_app);
    fs::set_permissions(&invalid_executable, fs::Permissions::from_mode(0o700))
        .expect("enable signed Mach-O mutation control");
    fs::OpenOptions::new()
        .append(true)
        .open(&invalid_executable)
        .and_then(|mut file| file.write_all(&[0]))
        .expect("mutate signed Mach-O bytes");
    let invalid_verify = Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict"])
        .arg(&invalid_executable)
        .output()
        .expect("verify tampered signed app");
    assert!(
        !invalid_verify.status.success(),
        "tampered app signature verified"
    );
    let invalid_output = Command::new(&invalid_executable)
        .arg("--keld-profile-identity-fixture-v1")
        .output()
        .expect("launch tampered app identity control");
    let invalid_text = format!(
        "{}{}",
        String::from_utf8_lossy(&invalid_output.stdout),
        String::from_utf8_lossy(&invalid_output.stderr)
    );
    assert!(
        !invalid_text.contains("KELD_KEL135_SIGNED_IDENTITY"),
        "invalid signature disclosed an app identity: {invalid_text}"
    );
    eprintln!(
        "KELD_KEL135_MACOS_PACKAGE_IDENTITY macos={} team={} app_a_identifier={} app_b_identifier={} other_publisher_team={} other_publisher_identifier={} app_a_profile={} app_b_profile={} other_publisher_profile={} invalid_signature_rejected=true ad_hoc_rejected=true",
        sw_vers_value("-productVersion"),
        identity_a["team_id"],
        identity_a["signing_identifier"],
        identity_b["signing_identifier"],
        identity_other["team_id"],
        identity_other["signing_identifier"],
        identity_a["profile_identity"],
        identity_b["profile_identity"],
        identity_other["profile_identity"],
    );
}
