use crate::support::product::ProductFixture;
use crate::support::profile_evidence::assert_ephemeral_store_report;
use crate::support::profile_evidence::assert_store_report_matches;
use crate::support::profile_evidence::run_signed_identity_report;
use crate::support::profile_evidence::run_signed_purge_report;
use crate::support::profile_evidence::sw_vers_value;
use crate::support::profile_evidence::webkit_version;
use crate::support::profile_origin::ProfileOrigin;
use crate::support::signed_app::MacProfilePurgeCleanup;
use crate::support::signed_app::build_signed_profile_app;
use crate::support::signed_app::valid_macos_codesign_hashes;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn run_persistent_media_query_modes(
    origin: &mut ProfileOrigin,
    app: &Path,
    support_root: &Path,
    store_uuid: &str,
    nonce: &str,
) -> (Vec<String>, (String, String)) {
    let mut states = Vec::new();
    let mut baseline = None;
    for (policy, seed) in [("deny", Some(nonce)), ("prompt", None), ("allow", None)] {
        let phase = format!("query-{policy}");
        let report = origin.run_media_profile(
            app,
            support_root,
            &phase,
            &phase,
            seed,
            if policy == "deny" { None } else { Some(policy) },
        );
        assert_eq!(
            report.get("local").map(String::as_str),
            Some(nonce),
            "{policy} lost persistent nonce"
        );
        assert_eq!(
            report.get("media").map(String::as_str),
            Some("not-requested")
        );
        let log = fs::read_to_string(support_root.join(format!("{phase}.log")))
            .expect("read signed query-only log");
        assert_store_report_matches(&log, store_uuid);
        assert!(
            !log.contains("KELD_KEL135_MEDIA_CALLBACK"),
            "query-only {policy} unexpectedly invoked a capture permission callback"
        );
        let camera = report.get("camera").expect("camera query result");
        let microphone = report.get("microphone").expect("microphone query result");
        for state in [camera.as_str(), microphone.as_str()] {
            assert!(
                matches!(state, "prompt" | "granted" | "denied"),
                "invalid permission query state: {state}"
            );
        }
        let pair = (camera.clone(), microphone.clone());
        if let Some(expected) = &baseline {
            assert_eq!(&pair, expected, "callback mode changed query state");
        } else {
            baseline = Some(pair);
        }
        states.push(format!("{policy}:camera={camera},microphone={microphone}"));
    }
    (states, baseline.expect("persistent query baseline"))
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires a local Apple signing identity and a real signed WKWebView host"]
fn kel135_macos_media_permission_query_is_observed_without_capture() {
    let fixture = ProductFixture::new("kel135-signed-profile-media-query");
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
        "ProfileAppMediaQuery",
        &format!(
            "dev.keld.fixture.profile.media-query.{}",
            std::process::id()
        ),
        &signer,
    );
    let identity = run_signed_identity_report(&app);
    let support_root = fixture.root.path().join("application-support");
    fs::create_dir(&support_root).expect("create isolated support root");
    fs::set_permissions(&support_root, fs::Permissions::from_mode(0o700))
        .expect("protect isolated support root");
    let fixture_root = fixture.root.keep();
    let mut cleanup = MacProfilePurgeCleanup {
        apps: vec![app.clone()],
        support_root: support_root.clone(),
        fixture_root: fixture_root.clone(),
        armed: true,
    };
    let mut origin = ProfileOrigin::new();
    let nonce = format!("keld-kel135-query-{}", std::process::id());
    let (mut states, baseline) = run_persistent_media_query_modes(
        &mut origin,
        &app,
        &support_root,
        &identity["store_uuid"],
        &nonce,
    );

    let ephemeral = origin.run_ephemeral_media_profile(
        stage.host(),
        &support_root,
        "query-ephemeral",
        "query-ephemeral",
    );
    assert_eq!(ephemeral.get("local").map(String::as_str), Some(""));
    assert_eq!(
        ephemeral.get("media").map(String::as_str),
        Some("not-requested")
    );
    let ephemeral_log = fs::read_to_string(support_root.join("query-ephemeral.log"))
        .expect("read ephemeral query-only log");
    assert_ephemeral_store_report(&ephemeral_log);
    assert!(
        !ephemeral_log.contains("KELD_KEL135_MEDIA_CALLBACK"),
        "ephemeral query unexpectedly invoked a capture permission callback: {ephemeral_log}"
    );
    let camera = ephemeral
        .get("camera")
        .expect("ephemeral camera query result");
    let microphone = ephemeral
        .get("microphone")
        .expect("ephemeral microphone query result");
    assert_eq!(
        &(camera.clone(), microphone.clone()),
        &baseline,
        "ephemeral and persistent query states differ before any capture request"
    );
    states.push(format!("ephemeral:camera={camera},microphone={microphone}"));
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    cleanup.armed = false;
    fs::remove_dir_all(&fixture_root).expect("remove query fixture after exact WebKit purge");
    eprintln!(
        "KELD_KEL135_MACOS_MEDIA_QUERY os={} webkit={} team={} identifier={} uuid={} origin={} persistent_nonce=true ephemeral_nonce=false callback_count=0 capture=none states={}",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
        origin.address,
        states.join(","),
    );
}
