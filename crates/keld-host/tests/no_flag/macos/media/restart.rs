use crate::media::restart_evidence::MediaRecordedRun;
use crate::media::restart_evidence::MediaScenario;
use crate::media::restart_evidence::media_restart_proof;
use crate::media::source_evidence::media_label_from_hex;
use crate::media::source_evidence::media_probe_sheet_count;
use crate::media::source_evidence::media_probe_tcc_authorized;
use crate::media::source_evidence::selected_media_source_class;
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

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
#[ignore = "requires Apple signing, live camera/microphone devices, and operator acceptance of the test site's permission prompt"]
fn kel135_macos_saved_media_grant_is_tested_against_restart_policy() {
    let fixture = ProductFixture::new("kel135-signed-profile-saved-media");
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
        "ProfileAppSavedMedia",
        &format!("dev.keld.fixture.profile.media.{}", std::process::id()),
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
    let omitted = origin.run_media_profile(
        &app,
        &support_root,
        "media-nonce-omitted",
        "media-nonce-omitted",
        None,
        None,
    );
    assert_eq!(omitted.get("local").map(String::as_str), Some(""));
    assert_eq!(
        omitted.get("media").map(String::as_str),
        Some("not-requested")
    );
    let omitted_log = fs::read_to_string(support_root.join("media-nonce-omitted.log"))
        .expect("read omitted-seed control log");
    assert_store_report_matches(&omitted_log, &identity["store_uuid"]);
    let nonce = format!("keld-kel135-media-{}", std::process::id());
    let mut evidence = Vec::new();
    let mut scenarios = Vec::new();

    for (kind, track) in [("camera", "video"), ("microphone", "audio")] {
        let seeded = origin.run_media_profile(
            &app,
            &support_root,
            &format!("media-seed-{kind}"),
            &format!("media-seed-{kind}"),
            Some(&nonce),
            Some("prompt"),
        );
        assert_eq!(seeded.get("local"), Some(&nonce));
        assert_eq!(
            seeded.get("media").map(String::as_str),
            Some(&format!("resolved-{track}-live")[..]),
            "qualified live {kind} seed"
        );
        assert_eq!(
            seeded.get("media_repeat").map(String::as_str),
            Some(&format!("resolved-{track}-live")[..]),
            "same-page {kind} grant reuse control"
        );
        let selected_label = seeded
            .get("device_label_hex")
            .filter(|label| !label.is_empty())
            .expect("seed capture reported its selected device label");
        let selected_label = media_label_from_hex(selected_label);
        assert_eq!(
            seeded
                .get("repeat_label_hex")
                .map(|label| media_label_from_hex(label)),
            Some(selected_label.clone()),
            "same-page {kind} repeat selected a different device"
        );
        let selected_class = selected_media_source_class(kind, &selected_label);
        let qualified_class = if kind == "camera" {
            "os-virtual-camo"
        } else {
            "physical-builtin"
        };
        assert_eq!(
            selected_class, qualified_class,
            "selected {kind} source requires independent qualification: {selected_label}"
        );
        if kind == "camera" {
            assert_eq!(
                seeded.get("frame_progress").map(String::as_str),
                Some("progressed"),
                "camera seed did not deliver advancing video frames"
            );
            assert_eq!(
                seeded.get("repeat_frame_progress").map(String::as_str),
                Some("progressed"),
                "same-page camera repeat did not deliver advancing video frames"
            );
        }
        assert!(
            media_probe_sheet_count(&seeded) > 0,
            "public Prompt seed did not produce an observable AppKit sheet for {kind}"
        );
        let seed_log = fs::read_to_string(support_root.join(format!("media-seed-{kind}.log")))
            .expect("read saved-media seed callback evidence");
        assert!(
            seed_log.contains(&format!(
                "KELD_KEL135_MEDIA_CALLBACK kind={kind} response=prompt"
            )),
            "seed callback did not defer {kind} to WebKit's user permission prompt"
        );
        let seed_callback_count = seed_log.matches("KELD_KEL135_MEDIA_CALLBACK").count();
        assert_eq!(
            seed_callback_count, 1,
            "same-page {kind} repeat unexpectedly invoked a new delegate decision"
        );
        assert_store_report_matches(&seed_log, &identity["store_uuid"]);

        let denied = origin.run_media_profile(
            &app,
            &support_root,
            &format!("media-deny-{kind}"),
            &format!("media-deny-{kind}"),
            None,
            None,
        );
        assert_eq!(denied.get("local"), Some(&nonce));
        assert_eq!(
            denied.get("media").map(String::as_str),
            Some("error-NotAllowedError"),
            "restarted Keld policy denies after {kind} Allow seed"
        );
        assert!(
            media_probe_tcc_authorized(&denied, kind),
            "signed denying host lacked pre-request {kind} TCC authorization"
        );
        assert_eq!(
            media_probe_sheet_count(&denied),
            0,
            "restarted {kind} denial presented an AppKit permission sheet"
        );
        let deny_log = fs::read_to_string(support_root.join(format!("media-deny-{kind}.log")))
            .expect("read saved-media deny callback evidence");
        assert!(
            deny_log.contains(&format!(
                "KELD_KEL135_MEDIA_CALLBACK kind={kind} response=deny"
            )),
            "Keld's denying callback did not run for saved {kind}"
        );
        assert_eq!(
            deny_log.matches("KELD_KEL135_MEDIA_CALLBACK").count(),
            1,
            "restarted {kind} request did not reach the guarded callback exactly once"
        );
        assert!(
            deny_log.contains("principal=Webview {")
                && deny_log.contains("guard_decision=Some(Deny(")
                && deny_log.contains("policy=PermissionsManifest { app: {} }"),
            "restarted {kind} denial lacks requesting principal or guard decision provenance"
        );
        let allow_phase = format!("media-allow-{kind}");
        let allowed = origin.run_media_profile(
            &app,
            &support_root,
            &allow_phase,
            &allow_phase,
            None,
            Some("allow"),
        );
        assert_eq!(allowed.get("local"), Some(&nonce));
        assert!(
            media_probe_tcc_authorized(&allowed, kind),
            "signed Allow counterfactual host lacked pre-request {kind} TCC authorization"
        );
        assert_eq!(
            allowed.get("media").map(String::as_str),
            Some(&format!("resolved-{track}-live")[..]),
            "fixture Allow must falsify the restarted {kind} denial oracle"
        );
        let allowed_label = allowed
            .get("device_label_hex")
            .filter(|label| !label.is_empty())
            .expect("Allow capture reported its selected device label");
        let allowed_label = media_label_from_hex(allowed_label);
        assert_eq!(
            selected_media_source_class(kind, &allowed_label),
            selected_class,
            "Allow control used a different source class"
        );
        if kind == "camera" {
            assert_eq!(
                allowed.get("frame_progress").map(String::as_str),
                Some("progressed"),
                "Allow control camera capture did not deliver advancing video frames"
            );
        }
        let allow_log = fs::read_to_string(support_root.join(format!("{allow_phase}.log")))
            .expect("read Allow counterfactual log");
        assert_store_report_matches(&allow_log, &identity["store_uuid"]);
        assert!(
            allow_log.contains(&format!(
                "KELD_KEL135_MEDIA_CALLBACK kind={kind} response=allow"
            )) && allow_log.contains("guard_decision=Some(Deny("),
            "Allow counterfactual did not override a real guarded denial"
        );

        let ephemeral = origin.run_ephemeral_media_profile(
            stage.host(),
            &support_root,
            &format!("media-control-{kind}"),
            &format!("media-control-{kind}"),
        );
        assert_eq!(
            ephemeral.get("media").map(String::as_str),
            Some("error-NotAllowedError"),
            "fresh ephemeral {kind} control must deny through Keld's callback"
        );
        let seed_permission = seeded
            .get("permission_after")
            .map_or("missing", String::as_str);
        let restart_permission = denied
            .get("permission_before")
            .map_or("missing", String::as_str);
        let fresh_permission = ephemeral
            .get("permission_before")
            .map_or("missing", String::as_str);
        let control_log =
            fs::read_to_string(support_root.join(format!("media-control-{kind}.log")))
                .expect("read fresh ephemeral media permission control");
        assert_ephemeral_store_report(&control_log);
        assert_store_report_matches(&deny_log, &identity["store_uuid"]);
        evidence.push(format!(
            "{kind}=live-track,seed-label:{selected_label},allow-label:{allowed_label},source-class:{selected_class},seed-after:{seed_permission},restart-before:{restart_permission},fresh-profile-before:{fresh_permission},restart-deny"
        ));
        scenarios.push(MediaScenario {
            kind,
            track,
            seed: MediaRecordedRun {
                report: seeded,
                log: seed_log,
            },
            denied: MediaRecordedRun {
                report: denied,
                log: deny_log,
            },
            allowed: MediaRecordedRun {
                report: allowed,
                log: allow_log,
            },
        });
    }

    let mut wrong_origin = ProfileOrigin::new();
    assert_ne!(wrong_origin.address, origin.address);
    let changed_origin = wrong_origin.run_media_profile(
        &app,
        &support_root,
        "media-nonce-wrong-origin",
        "media-nonce-wrong-origin",
        None,
        None,
    );
    assert_eq!(changed_origin.get("local").map(String::as_str), Some(""));
    let changed_origin_log = fs::read_to_string(support_root.join("media-nonce-wrong-origin.log"))
        .expect("read changed-origin control log");
    assert_store_report_matches(&changed_origin_log, &identity["store_uuid"]);

    let other_app = build_signed_profile_app(
        stage.root(),
        &app_root,
        "ProfileAppOtherMedia",
        &format!(
            "dev.keld.fixture.profile.other-media.{}",
            std::process::id()
        ),
        &signer,
    );
    cleanup.apps.push(other_app.clone());
    let other_identity = run_signed_identity_report(&other_app);
    assert_ne!(other_identity["store_uuid"], identity["store_uuid"]);
    let changed_profile = origin.run_media_profile(
        &other_app,
        &support_root,
        "media-nonce-wrong-profile",
        "media-nonce-wrong-profile",
        None,
        None,
    );
    assert_eq!(changed_profile.get("local").map(String::as_str), Some(""));
    let changed_profile_log =
        fs::read_to_string(support_root.join("media-nonce-wrong-profile.log"))
            .expect("read changed-profile control log");
    assert_store_report_matches(&changed_profile_log, &other_identity["store_uuid"]);

    let omitted = MediaRecordedRun {
        report: omitted,
        log: omitted_log,
    };
    let changed_origin = MediaRecordedRun {
        report: changed_origin,
        log: changed_origin_log,
    };
    let changed_profile = MediaRecordedRun {
        report: changed_profile,
        log: changed_profile_log,
    };

    let other_purge = run_signed_purge_report(&other_app, &support_root);
    assert!(other_purge.contains("store_absent=true"));
    let purge = run_signed_purge_report(&app, &support_root);
    assert!(purge.contains("store_absent=true"));
    cleanup.armed = false;
    fs::remove_dir_all(&fixture_root)
        .expect("remove media fixture after exact WebKit purge verification");
    for scenario in &scenarios {
        let kind = scenario.kind;
        let evaluate = |seed: &MediaRecordedRun, restarted: &MediaRecordedRun| {
            media_restart_proof(
                kind,
                scenario.track,
                &nonce,
                seed,
                restarted,
                &scenario.allowed,
            )
        };
        assert!(
            evaluate(&scenario.seed, &scenario.denied).is_complete(),
            "{kind} restart-denial proof is incomplete"
        );
        let omitted_result = evaluate(&omitted, &scenario.denied);
        assert!(!omitted_result.seed.qualified_capture && !omitted_result.is_complete());
        let changed_origin_result = evaluate(&scenario.seed, &changed_origin);
        assert!(!changed_origin_result.same_origin && !changed_origin_result.is_complete());
        let changed_profile_result = evaluate(&scenario.seed, &changed_profile);
        assert!(
            !changed_profile_result.continuity.same_store && !changed_profile_result.is_complete()
        );
        let retained_page_result = evaluate(&scenario.seed, &scenario.seed);
        assert!(
            !retained_page_result.continuity.clean_restart && !retained_page_result.is_complete()
        );
        let allow_result = evaluate(&scenario.seed, &scenario.allowed);
        assert!(
            !allow_result.denial.no_capture_or_prompt && !allow_result.is_complete(),
            "fixture Allow {kind} capture falsely passed the denial oracle"
        );
    }
    eprintln!(
        "KELD_KEL135_MACOS_MEDIA_SAVED_GRANT macos_media_contract=public-grant-restart-v1 os={} webkit={} team={} identifier={} uuid={} origin={} nonce_store=indexeddb nonce_survived=true seed_decision=WKPermissionDecisionPrompt site_prompt_observed=true post_restart_policy=deny denial_site_prompt_absent=true tcc_status=authorized allow_counterfactual=captured controls=omitted-seed,changed-origin,changed-store,retained-page,allow-response-rejected results={}",
        sw_vers_value("-productVersion"),
        webkit_version(),
        identity["team_id"],
        identity["signing_identifier"],
        identity["store_uuid"],
        origin.address,
        evidence.join(","),
    );
}
