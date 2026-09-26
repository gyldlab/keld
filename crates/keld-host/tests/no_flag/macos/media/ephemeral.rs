use crate::media::dev_evidence::dev_media_proof;
use crate::media::restart_evidence::MediaRecordedRun;
use crate::media::restart_evidence::media_callback_matches;
use crate::media::source_evidence::media_label_from_hex;
use crate::media::source_evidence::media_probe_sheet_count;
use crate::media::source_evidence::media_probe_tcc_authorized;
use crate::media::source_evidence::selected_media_source_class;
use crate::support::product::ProductFixture;
use crate::support::profile_evidence::assert_ephemeral_store_report;
use crate::support::profile_evidence::run_signed_identity_report;
use crate::support::profile_evidence::sw_vers_value;
use crate::support::profile_evidence::webkit_version;
use crate::support::profile_origin::ProfileOrigin;
use crate::support::signed_app::build_signed_profile_app;
use crate::support::signed_app::signed_host_executable;
use crate::support::signed_app::valid_macos_codesign_hashes;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn checked_dev_seed_report(
    report: &std::collections::BTreeMap<String, String>,
    log: &str,
    nonce: &str,
    kind: &str,
    track: &str,
) -> (&'static str, String) {
    assert_eq!(report.get("local").map(String::as_str), Some(nonce));
    assert_eq!(report.get("reuse_local").map(String::as_str), Some(nonce));
    assert_eq!(
        report.get("reuse_media").map(String::as_str),
        Some("not-requested")
    );
    let live = format!("resolved-{track}-live");
    for field in ["media", "media_repeat"] {
        assert_eq!(report.get(field).map(String::as_str), Some(live.as_str()));
    }
    if kind == "camera" {
        for field in ["frame_progress", "repeat_frame_progress"] {
            assert_eq!(report.get(field).map(String::as_str), Some("progressed"));
        }
    }
    let label = media_label_from_hex(report.get("device_label_hex").expect("dev device label"));
    assert!(!label.is_empty());
    let source_class = selected_media_source_class(kind, &label);
    assert_eq!(
        source_class,
        if kind == "camera" {
            "os-virtual-camo"
        } else {
            "physical-builtin"
        },
        "dev {kind} selected an unqualified source"
    );
    assert_ephemeral_store_report(log);
    assert!(media_callback_matches(log, kind, "allow"));
    assert_eq!(media_probe_sheet_count(report), 0);
    (source_class, label)
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_dev_media_kind(
    origin: &mut ProfileOrigin,
    app: &Path,
    support_root: &Path,
    nonce: &str,
    kind: &str,
    track: &str,
) -> String {
    let seed_phase = format!("media-seed-{kind}");
    let seeded = origin.run_profile_executable(
        signed_host_executable(app),
        support_root,
        &seed_phase,
        &format!("dev-{seed_phase}"),
        Some(nonce),
        true,
        None,
        Some("allow-reuse"),
    );
    let seed_log = fs::read_to_string(support_root.join(format!("dev-{seed_phase}.log")))
        .expect("read dev seed log");
    let (source_class, seed_label) =
        checked_dev_seed_report(&seeded, &seed_log, nonce, kind, track);

    let deny_phase = format!("media-deny-{kind}");
    let denied = origin.run_profile_executable(
        signed_host_executable(app),
        support_root,
        &deny_phase,
        &format!("dev-{deny_phase}"),
        None,
        true,
        None,
        None,
    );
    assert_eq!(denied.get("local").map(String::as_str), Some(""));
    assert_eq!(
        denied.get("media").map(String::as_str),
        Some("error-NotAllowedError")
    );
    assert!(media_probe_tcc_authorized(&denied, kind));
    assert_eq!(media_probe_sheet_count(&denied), 0);
    let deny_log = fs::read_to_string(support_root.join(format!("dev-{deny_phase}.log")))
        .expect("read dev denial log");
    assert_ephemeral_store_report(&deny_log);
    assert_eq!(deny_log.matches("KELD_KEL135_MEDIA_CALLBACK").count(), 1);
    assert!(
        deny_log.contains(&format!("kind={kind} response=deny"))
            && deny_log.contains("principal=Webview {")
            && deny_log.contains("guard_decision=Some(Deny(")
            && deny_log.contains("policy=PermissionsManifest { app: {} }")
    );

    let seed_run = MediaRecordedRun {
        report: seeded,
        log: seed_log,
    };
    let denied_run = MediaRecordedRun {
        report: denied,
        log: deny_log,
    };
    assert!(
        dev_media_proof(kind, track, nonce, &seed_run, &denied_run).is_complete(),
        "dev {kind} fresh-launch denial proof is incomplete"
    );
    let mut reused_store_run = seed_run.clone();
    reused_store_run.report.insert(
        String::from("local"),
        seed_run.report["reuse_local"].clone(),
    );
    reused_store_run.report.insert(
        String::from("media"),
        seed_run.report["reuse_media"].clone(),
    );
    let reused_result = dev_media_proof(kind, track, nonce, &seed_run, &reused_store_run);
    assert!(
        !reused_result.fresh.nonce_absent && !reused_result.is_complete(),
        "reusing dev A's nonpersistent store falsely passed the fresh-store oracle"
    );
    let retained_result = dev_media_proof(kind, track, nonce, &seed_run, &seed_run);
    assert!(
        !retained_result.continuity.clean_restart && !retained_result.is_complete(),
        "retaining dev A's granted page falsely passed the lifecycle oracle"
    );
    format!(
        "{kind}=source:{source_class},label:{seed_label},seed-live,second-view-nonce,fresh-deny"
    )
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires a local Apple signing identity and live camera/microphone devices"]
fn kel135_macos_dev_media_grants_do_not_survive_fresh_ephemeral_launch() {
    let fixture = ProductFixture::new("kel135-signed-dev-media-ephemeral");
    let stage = fixture.stage();
    let app_root = fixture.root.path().join("signed-apps");
    fs::create_dir(&app_root).expect("create signed dev media fixture parent");
    let signer = valid_macos_codesign_hashes()
        .into_iter()
        .next()
        .expect("a valid Apple code-signing identity is required");
    let app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppDevMedia",
        &format!("dev.keld.fixture.profile.dev-media.{}", std::process::id()),
        &signer,
    );
    let identity = run_signed_identity_report(&app);
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let mut origin = ProfileOrigin::new();
    let nonce = format!("keld-kel135-dev-media-{}", std::process::id());
    let evidence = [("camera", "video"), ("microphone", "audio")]
        .into_iter()
        .map(|(kind, track)| {
            run_dev_media_kind(&mut origin, &app, &support_root, &nonce, kind, track)
        })
        .collect::<Vec<_>>();
    eprintln!(
        "KELD_KEL135_MACOS_DEV_MEDIA macos_media_contract=public-grant-restart-v1 os={} webkit={} team={} identifier={} origin={} nonce_store=indexeddb seed_store=ephemeral second_view_store=reused fresh_store=ephemeral fresh_nonce_absent=true fresh_denial=guarded/no-track/no-sheet controls=reused-store,retained-page results={}",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        origin.address,
        evidence.join(","),
    );
}
