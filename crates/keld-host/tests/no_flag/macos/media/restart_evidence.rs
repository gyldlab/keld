use crate::media::source_evidence::media_label_from_hex;
use crate::media::source_evidence::media_probe_sheet_count;
use crate::media::source_evidence::media_probe_tcc_authorized;
use crate::media::source_evidence::selected_media_source_class;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct MediaRestartProof {
    pub(crate) seed: MediaSeedProof,
    pub(crate) continuity: MediaContinuityProof,
    pub(crate) denial: MediaDenialProof,
    pub(crate) same_origin: bool,
    pub(crate) nonce_survived: bool,
    pub(crate) allow_capture: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct MediaSeedProof {
    pub(crate) qualified_capture: bool,
    pub(crate) site_prompt_observed: bool,
    pub(crate) same_page_repeat_capture: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct MediaContinuityProof {
    pub(crate) clean_restart: bool,
    pub(crate) same_signed_identity: bool,
    pub(crate) same_store: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct MediaDenialProof {
    pub(crate) tcc_authorized: bool,
    pub(crate) guarded_callback: bool,
    pub(crate) no_capture_or_prompt: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl MediaRestartProof {
    pub(crate) fn is_complete(&self) -> bool {
        self.seed.qualified_capture
            && self.seed.site_prompt_observed
            && self.seed.same_page_repeat_capture
            && self.continuity.clean_restart
            && self.continuity.same_signed_identity
            && self.continuity.same_store
            && self.denial.tcc_authorized
            && self.same_origin
            && self.nonce_survived
            && self.denial.guarded_callback
            && self.denial.no_capture_or_prompt
            && self.allow_capture
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone)]
pub(crate) struct MediaRecordedRun {
    pub(crate) report: std::collections::BTreeMap<String, String>,
    pub(crate) log: String,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) struct MediaScenario {
    pub(crate) kind: &'static str,
    pub(crate) track: &'static str,
    pub(crate) seed: MediaRecordedRun,
    pub(crate) denied: MediaRecordedRun,
    pub(crate) allowed: MediaRecordedRun,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn same_present_run_fact(runs: &[&MediaRecordedRun], key: &str) -> bool {
    let Some(first) = runs[0].report.get(key).filter(|value| !value.is_empty()) else {
        return false;
    };
    runs.iter().all(|run| run.report.get(key) == Some(first))
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn same_verified_signed_run(runs: &[&MediaRecordedRun]) -> bool {
    [
        "host_executable",
        "host_sha256",
        "host_cdhash",
        "signed_team_id",
        "signed_signing_identifier",
        "signed_profile_identity",
        "signed_store_uuid",
    ]
    .iter()
    .all(|key| same_present_run_fact(runs, key))
        && runs.iter().all(|run| {
            run.report
                .get("signed_signature_validated_before_identity_read")
                .map(String::as_str)
                == Some("true")
        })
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn distinct_clean_host_runs(runs: &[&MediaRecordedRun]) -> bool {
    let pids = runs
        .iter()
        .map(|run| run.report.get("host_pid").filter(|value| !value.is_empty()))
        .collect::<Vec<_>>();
    pids.iter().all(Option::is_some)
        && pids
            .iter()
            .enumerate()
            .all(|(index, pid)| pids.iter().skip(index + 1).all(|other| pid != other))
        && runs
            .iter()
            .all(|run| run.report.get("host_clean_exit").map(String::as_str) == Some("true"))
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn media_callback_matches(log: &str, kind: &str, response: &str) -> bool {
    log.matches("KELD_KEL135_MEDIA_CALLBACK").count() == 1
        && log.contains(&format!(
            "KELD_KEL135_MEDIA_CALLBACK kind={kind} response={response}"
        ))
        && log.contains("principal=Webview {")
        && log.contains("guard_decision=Some(Deny(")
        && log.contains("policy=PermissionsManifest { app: {} }")
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn media_qualified_label(
    kind: &str,
    track: &str,
    report: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let expected_live = format!("resolved-{track}-live");
    if report.get("media").map(String::as_str) != Some(expected_live.as_str()) {
        return None;
    }
    if kind == "camera" && report.get("frame_progress").map(String::as_str) != Some("progressed") {
        return None;
    }
    let label = report
        .get("device_label_hex")
        .filter(|value| !value.is_empty())?;
    let label = media_label_from_hex(label);
    let required_class = if kind == "camera" {
        "os-virtual-camo"
    } else {
        "physical-builtin"
    };
    (selected_media_source_class(kind, &label) == required_class).then_some(label)
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn media_restart_proof(
    kind: &str,
    track: &str,
    nonce: &str,
    seed: &MediaRecordedRun,
    restarted: &MediaRecordedRun,
    allowed: &MediaRecordedRun,
) -> MediaRestartProof {
    let seed_report = &seed.report;
    let restart_report = &restarted.report;
    let allow_report = &allowed.report;
    let seed_label = media_qualified_label(kind, track, seed_report);
    let allow_label = media_qualified_label(kind, track, allow_report);
    let expected_live = format!("resolved-{track}-live");
    let seed_sheet_count = media_probe_sheet_count(seed_report);
    let runs = [seed, restarted, allowed];
    let same_app = same_verified_signed_run(&runs);
    let same_store = same_present_run_fact(&runs, "store_actual_identifier")
        && same_present_run_fact(&runs, "store_profile_identity")
        && seed_report
            .get("store_actual_identifier")
            .is_some_and(|value| value != "none")
        && runs.iter().all(|run| {
            run.report.get("store_persistent").map(String::as_str) == Some("true")
                && run.report.get("store_actual_identifier")
                    == run.report.get("store_expected_store_uuid")
                && run.report.get("store_actual_identifier") == run.report.get("signed_store_uuid")
                && run.report.get("store_profile_identity")
                    == run.report.get("signed_profile_identity")
        });
    let same_origin = same_present_run_fact(&runs, "origin");
    MediaRestartProof {
        seed: MediaSeedProof {
            qualified_capture: seed_label.is_some()
                && media_callback_matches(&seed.log, kind, "prompt"),
            site_prompt_observed: seed_sheet_count > 0,
            same_page_repeat_capture: seed_report.get("media_repeat").map(String::as_str)
                == Some(expected_live.as_str())
                && seed_report.get("repeat_label_hex") == seed_report.get("device_label_hex")
                && (kind != "camera"
                    || seed_report.get("repeat_frame_progress").map(String::as_str)
                        == Some("progressed"))
                && seed.log.matches("KELD_KEL135_MEDIA_CALLBACK").count() == 1,
        },
        continuity: MediaContinuityProof {
            clean_restart: distinct_clean_host_runs(&runs),
            same_signed_identity: same_app,
            same_store,
        },
        same_origin,
        nonce_survived: seed_report.get("local").map(String::as_str) == Some(nonce)
            && restart_report.get("local").map(String::as_str) == Some(nonce)
            && allow_report.get("local").map(String::as_str) == Some(nonce),
        denial: MediaDenialProof {
            tcc_authorized: media_probe_tcc_authorized(restart_report, kind)
                && media_probe_tcc_authorized(allow_report, kind),
            guarded_callback: media_callback_matches(&restarted.log, kind, "deny"),
            no_capture_or_prompt: restart_report.get("media").map(String::as_str)
                == Some("error-NotAllowedError")
                && restart_report
                    .get("device_label_hex")
                    .is_some_and(String::is_empty)
                && restart_report.get("media_repeat").map(String::as_str) == Some("not-requested")
                && seed_sheet_count > 0
                && media_probe_sheet_count(restart_report) == 0,
        },
        allow_capture: allow_label.is_some()
            && seed_label == allow_label
            && media_callback_matches(&allowed.log, kind, "allow")
            && media_probe_sheet_count(allow_report) == 0,
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[test]
fn kel135_macos_media_restart_oracle_rejects_missing_boundaries() {
    let complete = MediaRestartProof {
        seed: MediaSeedProof {
            qualified_capture: true,
            site_prompt_observed: true,
            same_page_repeat_capture: true,
        },
        continuity: MediaContinuityProof {
            clean_restart: true,
            same_signed_identity: true,
            same_store: true,
        },
        denial: MediaDenialProof {
            tcc_authorized: true,
            guarded_callback: true,
            no_capture_or_prompt: true,
        },
        same_origin: true,
        nonce_survived: true,
        allow_capture: true,
    };
    assert!(complete.is_complete());
    let query_only = MediaRestartProof {
        nonce_survived: true,
        ..MediaRestartProof::default()
    };
    assert!(!query_only.is_complete());
    let mut missing_seed = complete;
    missing_seed.seed.qualified_capture = false;
    assert!(!missing_seed.is_complete());
    let mut wrong_store = complete;
    wrong_store.continuity.same_store = false;
    assert!(!wrong_store.is_complete());
    let mut retained_page = complete;
    retained_page.continuity.clean_restart = false;
    assert!(!retained_page.is_complete());
    let mut allow_response = complete;
    allow_response.denial.no_capture_or_prompt = false;
    assert!(!allow_response.is_complete());
}
