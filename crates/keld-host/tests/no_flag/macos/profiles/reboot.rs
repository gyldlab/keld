use crate::support::product::ProductFixture;
use crate::support::profile_evidence::assert_store_report_matches;
use crate::support::profile_evidence::boot_uuid_from_log;
use crate::support::profile_evidence::run_signed_identity_report;
use crate::support::profile_evidence::run_signed_purge_report;
use crate::support::profile_evidence::sw_vers_value;
use crate::support::profile_evidence::system_boot_time_seconds;
use crate::support::profile_evidence::system_boot_uuid_hex;
use crate::support::profile_evidence::webkit_version;
use crate::support::profile_origin::ProfileOrigin;
use crate::support::signed_app::build_signed_profile_app;
use crate::support::signed_app::kill_profile_host;
use crate::support::signed_app::refresh_signed_profile_app;
use crate::support::signed_app::signed_host_executable;
use crate::support::signed_app::spawn_profile_host;
use crate::support::signed_app::valid_macos_codesign_hashes;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "two-phase real reboot test; run prepare, restart macOS, then resume"]
fn kel135_macos_crash_quarantine_recovers_only_after_real_reboot() {
    assert!(
        std::env::var_os("KELD_PROFILE_TEST_BOOT_UUID").is_none(),
        "real reboot acceptance cannot use the synthetic boot UUID hook"
    );
    let phase = std::env::var("KELD_KEL135_REBOOT_PHASE")
        .expect("set KELD_KEL135_REBOOT_PHASE to prepare or resume");
    let run_root = PathBuf::from(
        std::env::var_os("KELD_KEL135_REBOOT_ROOT")
            .expect("set KELD_KEL135_REBOOT_ROOT below the persistent TMPDIR"),
    );
    let temp_root = std::env::temp_dir()
        .canonicalize()
        .expect("canonicalize configured TMPDIR");
    assert!(
        run_root.is_absolute(),
        "reboot fixture root must be absolute"
    );
    if phase == "prepare" {
        fs::create_dir(&run_root).expect("create durable reboot fixture root once");
        fs::set_permissions(&run_root, fs::Permissions::from_mode(0o700))
            .expect("protect durable reboot fixture root");
    } else {
        assert_eq!(phase, "resume", "unknown reboot fixture phase");
        assert!(
            run_root.is_dir(),
            "resume requires the preserved prepare root"
        );
    }
    let canonical_root = run_root.canonicalize().expect("canonicalize reboot root");
    assert!(canonical_root.starts_with(temp_root));
    let manifest_path = canonical_root.join("manifest.json");
    let support_root = canonical_root.join("application-support");
    let project_root = canonical_root.join("project");
    let mut fixture = ProductFixture::new("kel135-reboot-runner");
    fixture.project = project_root.clone();
    fs::create_dir_all(project_root.join("src"))
        .expect("create persistent reboot fixture project and entry directory");
    let stage = fixture.stage();

    if phase == "prepare" {
        fs::create_dir(&support_root).expect("create persistent profile metadata root");
        fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
            .expect("protect persistent profile metadata root");
        let app_root = canonical_root.join("signed-apps");
        fs::create_dir(&app_root).expect("create durable signed fixture parent");
        fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
            .expect("protect durable signed fixture parent");
        let signer = valid_macos_codesign_hashes()
            .into_iter()
            .next()
            .expect("an Apple Development signing identity is required");
        let bundle_id = format!("dev.keld.fixture.profile.reboot.{}", std::process::id());
        let app = build_signed_profile_app(
            stage.root(),
            &app_root,
            "ProfileAppRebootRecovery",
            &bundle_id,
            &signer,
        );
        let identity = run_signed_identity_report(&app);
        let os_boot_before = system_boot_uuid_hex();
        let mut origin = ProfileOrigin::new();
        let nonce = String::from("keld-kel135-reboot-preserved-state");
        let mut owner = spawn_profile_host(
            signed_host_executable(&app),
            &support_root,
            &origin.address,
            "seed",
            "reboot-crash-owner",
        );
        let seeded = origin
            .wait_for_report("seed", Some(&nonce))
            .expect("signed profile owner rendered its seed page");
        for key in ["local", "cookie", "idb", "cache"] {
            assert_eq!(seeded.get(key).map(String::as_str), Some(nonce.as_str()));
        }
        assert_eq!(seeded.get("sw").map(String::as_str), Some("true"));
        let owner_log = fs::read_to_string(support_root.join("reboot-crash-owner.log"))
            .expect("read active owner store evidence");
        assert_store_report_matches(&owner_log, &identity["store_uuid"]);
        let prior_boot = boot_uuid_from_log(&owner_log);
        assert!(
            prior_boot == os_boot_before,
            "Keld boot readback must equal the independent sysctl oracle"
        );
        let crashed = kill_profile_host(&mut owner);
        assert_eq!(crashed.signal(), Some(9), "owner must terminate by SIGKILL");

        let same_boot = Command::new(signed_host_executable(&app))
            .arg("--keld-profile-webview-fixture-v1")
            .env("KELD_PROFILE_TEST_ROOT", &support_root)
            .env("KELD_PROFILE_ACCEPTANCE_REPORT", "1")
            .env_remove("KELD_PROFILE_TEST_BOOT_UUID")
            .env(
                "KELD_PROFILE_FIXTURE_URL",
                format!("http://{}/unused", origin.address),
            )
            .stdin(Stdio::null())
            .output()
            .expect("launch same-boot quarantine negative control");
        assert!(!same_boot.status.success());
        let same_boot_text = String::from_utf8_lossy(&same_boot.stderr);
        assert!(
            same_boot_text.contains("recovery state cannot be proven"),
            "same-boot quarantine must report unproven recovery"
        );
        assert!(
            same_boot_text.contains("startup-resource-attempts listener=0 child=0 window=0"),
            "same-boot quarantine must fail before app resources"
        );

        let manifest = std::collections::BTreeMap::from([
            (String::from("app"), app.display().to_string()),
            (
                String::from("support_root"),
                support_root.display().to_string(),
            ),
            (String::from("origin"), origin.address.clone()),
            (String::from("team_id"), identity["team_id"].clone()),
            (
                String::from("signing_identifier"),
                identity["signing_identifier"].clone(),
            ),
            (
                String::from("profile_identity"),
                identity["profile_identity"].clone(),
            ),
            (String::from("store_uuid"), identity["store_uuid"].clone()),
            (String::from("nonce"), nonce),
            (String::from("boot_uuid_hex"), prior_boot.clone()),
        ]);
        let encoded = serde_json::to_vec(&manifest).expect("encode durable reboot manifest");
        fs::write(&manifest_path, encoded).expect("persist reboot resume manifest");
        fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o600))
            .expect("protect reboot resume manifest");
        eprintln!(
            "KELD_KEL135_MACOS_REBOOT_PREPARED os={} team={} identifier={} uuid={} boot_identity_recorded=true same_boot=quarantined owner_exit=SIGKILL root={} next=physically-restart-macOS",
            sw_vers_value("-productVersion"),
            identity["team_id"],
            identity["signing_identifier"],
            identity["store_uuid"],
            canonical_root.display(),
        );
        return;
    }

    let manifest: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read prepare manifest"))
            .expect("decode prepare manifest");
    let app = PathBuf::from(&manifest["app"]);
    let support_root = PathBuf::from(&manifest["support_root"]);
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("an Apple Development signing identity is required");
    refresh_signed_profile_app(stage.root(), &app, &signer);
    let identity = run_signed_identity_report(&app);
    let os_boot_after = system_boot_uuid_hex();
    let os_boot_time = system_boot_time_seconds();
    for key in [
        "team_id",
        "signing_identifier",
        "profile_identity",
        "store_uuid",
    ] {
        assert_eq!(
            identity[key], manifest[key],
            "stable reboot identity field {key}"
        );
    }
    let mut origin = ProfileOrigin::bind(&manifest["origin"]);
    let state = origin.run_profile(&app, &support_root, "read", "reboot-recovered-read", None);
    for key in ["local", "cookie", "idb", "cache"] {
        assert_eq!(
            state.get(key).map(String::as_str),
            Some(manifest["nonce"].as_str())
        );
    }
    assert_eq!(state.get("sw").map(String::as_str), Some("true"));
    let report = fs::read_to_string(support_root.join("reboot-recovered-read.log"))
        .expect("read post-reboot selected store report");
    assert_store_report_matches(&report, &manifest["store_uuid"]);
    let next_boot = boot_uuid_from_log(&report);
    assert!(
        next_boot == os_boot_after,
        "Keld boot readback must equal the independent sysctl oracle"
    );
    let previous_boot_evidence = if let Some(previous_boot) = manifest.get("boot_uuid_hex") {
        assert!(
            next_boot != *previous_boot,
            "recovered boot must differ from the retained pre-reboot boot"
        );
        "uuid-distinct"
    } else {
        let owner_log_mtime = fs::metadata(support_root.join("reboot-crash-owner.log"))
            .expect("read pre-reboot owner log metadata")
            .modified()
            .expect("read pre-reboot owner log timestamp")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("owner log timestamp follows UNIX epoch")
            .as_secs();
        assert!(
            os_boot_time > owner_log_mtime,
            "independent kern.boottime must postdate the SIGKILL owner log"
        );
        "legacy-prepare-omitted-uuid-boot-time-proven"
    };
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    drop(origin);
    eprintln!(
        "KELD_KEL135_MACOS_REBOOT_RECOVERY os={} webkit={} team={} identifier={} uuid={} boot_uuid_matches_sysctl=true previous_boot_evidence={} kern_boottime_epoch={} boot_transition=real-reboot same_boot_quarantine=passed state=all-five-preserved purge=exact-identity",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
        previous_boot_evidence,
        os_boot_time,
    );
    fs::remove_dir_all(&canonical_root).expect("remove completed isolated reboot fixture root");
}
