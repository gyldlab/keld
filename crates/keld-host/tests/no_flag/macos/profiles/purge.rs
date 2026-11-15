use crate::support::product::ProductFixture;
use crate::support::profile_evidence::assert_store_report_matches;
use crate::support::profile_evidence::run_signed_identity_report;
use crate::support::profile_evidence::run_signed_purge_phase_crash_probe;
use crate::support::profile_evidence::run_signed_purge_report;
use crate::support::profile_evidence::run_signed_store_presence_report;
use crate::support::profile_evidence::sw_vers_value;
use crate::support::profile_evidence::webkit_version;
use crate::support::profile_origin::ProfileOrigin;
use crate::support::signed_app::build_signed_profile_app;
use crate::support::signed_app::valid_macos_codesign_hashes;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
fn exercise_fsynced_purge_phase_recovery(
    stage: &keld_cli::boot::DevBootStage,
    app_root: &Path,
    support_root: &Path,
    signer: &str,
    origin: &mut ProfileOrigin,
    phase: &str,
) -> String {
    let app = build_signed_profile_app(
        stage.root(),
        app_root,
        &format!("ProfileAppPurgeRecovery-{phase}"),
        &format!(
            "dev.keld.fixture.profile.purge.{}.{}",
            std::process::id(),
            phase.to_ascii_lowercase()
        ),
        signer,
    );
    let identity = run_signed_identity_report(&app);
    let phase_support = support_root.join(phase);
    fs::create_dir(&phase_support).expect("create isolated purge recovery metadata root");
    fs::set_permissions(&phase_support, fs::Permissions::from_mode(0o700))
        .expect("protect isolated purge recovery metadata root");
    let nonce = format!("keld-kel135-purge-{phase}");
    let seeded = origin.run_profile(
        &app,
        &phase_support,
        "seed",
        &format!("purge-{phase}-seed"),
        Some(&nonce),
    );
    assert_eq!(seeded.get("local"), Some(&nonce));
    let seed_log = fs::read_to_string(phase_support.join(format!("purge-{phase}-seed.log")))
        .expect("read purge recovery seed store report");
    assert_store_report_matches(&seed_log, &identity["store_uuid"]);
    let interrupted = run_signed_purge_phase_crash_probe(&app, &phase_support, phase);
    assert_eq!(
        interrupted.status.code(),
        Some(88),
        "phase {phase} missed crash"
    );
    assert!(
        String::from_utf8_lossy(&interrupted.stderr)
            .contains(&format!("after_fsynced_phase={phase}")),
        "phase {phase} crash was not after its durable intent write"
    );
    let resumed = run_signed_purge_report(&app, &phase_support);
    assert!(resumed.contains(&format!("store_uuid={}", identity["store_uuid"])));
    assert!(resumed.contains("store_absent=true"));
    let presence = run_signed_store_presence_report(&app);
    assert!(presence.contains(&format!("store_uuid={}", identity["store_uuid"])));
    assert!(presence.contains("present=false"));
    format!("{phase}:crash88,recovered,enumerated-absent")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires Apple signing and real WKWebsiteDataStore purge callbacks"]
fn kel135_macos_purge_recovers_after_fsynced_later_phase_writes() {
    let fixture = ProductFixture::new("kel135-signed-purge-later-phase-recovery");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed purge recovery fixture parent");
    fs::set_permissions(&app_root, fs::Permissions::from_mode(0o700))
        .expect("protect signed purge recovery fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated purge recovery support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated purge recovery support root");
    let mut origin = ProfileOrigin::new();
    let results = [
        "StoreAbsent",
        "ReverseRemoved",
        "ForwardRemoved",
        "Inactive",
    ]
    .into_iter()
    .map(|phase| {
        exercise_fsynced_purge_phase_recovery(
            &stage,
            &app_root,
            &support_root,
            &signer,
            &mut origin,
            phase,
        )
    })
    .collect::<Vec<_>>();
    eprintln!(
        "KELD_KEL135_MACOS_PURGE_PHASE_RECOVERY os={} webkit={} phases={} exact_enumeration=true",
        sw_vers_value("-productVersion"),
        webkit_version(),
        results.join(","),
    );
}
