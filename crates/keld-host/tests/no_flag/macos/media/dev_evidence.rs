use crate::media::restart_evidence::MediaRecordedRun;
use crate::media::restart_evidence::distinct_clean_host_runs;
use crate::media::restart_evidence::media_callback_matches;
use crate::media::restart_evidence::media_qualified_label;
use crate::media::restart_evidence::same_present_run_fact;
use crate::media::restart_evidence::same_verified_signed_run;
use crate::media::source_evidence::media_probe_sheet_count;
use crate::media::source_evidence::media_probe_tcc_authorized;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct DevMediaProof {
    pub(crate) seed: DevSeedProof,
    pub(crate) continuity: DevContinuityProof,
    pub(crate) fresh: DevFreshProof,
    pub(crate) denial: DevDenialProof,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct DevSeedProof {
    pub(crate) live_track: bool,
    pub(crate) nonce_committed: bool,
    pub(crate) second_view_saw_nonce: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct DevContinuityProof {
    pub(crate) same_signed_identity: bool,
    pub(crate) same_origin: bool,
    pub(crate) clean_restart: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct DevFreshProof {
    pub(crate) ephemeral: bool,
    pub(crate) nonce_absent: bool,
    pub(crate) tcc_authorized: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct DevDenialProof {
    pub(crate) guarded_callback: bool,
    pub(crate) no_capture: bool,
    pub(crate) no_prompt: bool,
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
impl DevMediaProof {
    pub(crate) fn is_complete(&self) -> bool {
        self.seed.live_track
            && self.seed.nonce_committed
            && self.seed.second_view_saw_nonce
            && self.continuity.same_signed_identity
            && self.continuity.same_origin
            && self.continuity.clean_restart
            && self.fresh.ephemeral
            && self.fresh.nonce_absent
            && self.fresh.tcc_authorized
            && self.denial.guarded_callback
            && self.denial.no_capture
            && self.denial.no_prompt
    }
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn dev_media_proof(
    kind: &str,
    track: &str,
    nonce: &str,
    seed: &MediaRecordedRun,
    restarted: &MediaRecordedRun,
) -> DevMediaProof {
    let seed_report = &seed.report;
    let restart_report = &restarted.report;
    let expected_live = format!("resolved-{track}-live");
    let runs = [seed, restarted];
    DevMediaProof {
        seed: DevSeedProof {
            live_track: media_qualified_label(kind, track, seed_report).is_some()
                && media_callback_matches(&seed.log, kind, "allow")
                && seed_report.get("media_repeat").map(String::as_str)
                    == Some(expected_live.as_str())
                && seed_report.get("repeat_label_hex") == seed_report.get("device_label_hex")
                && (kind != "camera"
                    || seed_report.get("repeat_frame_progress").map(String::as_str)
                        == Some("progressed")),
            nonce_committed: seed_report.get("local").map(String::as_str) == Some(nonce),
            second_view_saw_nonce: seed_report.get("reuse_local").map(String::as_str)
                == Some(nonce)
                && seed_report.get("reuse_media").map(String::as_str) == Some("not-requested"),
        },
        continuity: DevContinuityProof {
            same_signed_identity: same_verified_signed_run(&runs),
            same_origin: same_present_run_fact(&runs, "origin"),
            clean_restart: distinct_clean_host_runs(&runs),
        },
        fresh: DevFreshProof {
            ephemeral: seed_report.get("store_persistent").map(String::as_str) == Some("false")
                && restart_report.get("store_persistent").map(String::as_str) == Some("false")
                && seed_report
                    .get("store_actual_identifier")
                    .map(String::as_str)
                    == Some("none")
                && restart_report
                    .get("store_actual_identifier")
                    .map(String::as_str)
                    == Some("none")
                && seed_report.get("store_mode").map(String::as_str) == Some("ephemeral-dev")
                && restart_report.get("store_mode").map(String::as_str) == Some("ephemeral-dev"),
            nonce_absent: restart_report.get("local").map(String::as_str) == Some(""),
            tcc_authorized: media_probe_tcc_authorized(restart_report, kind),
        },
        denial: DevDenialProof {
            guarded_callback: media_callback_matches(&restarted.log, kind, "deny"),
            no_capture: restart_report.get("media").map(String::as_str)
                == Some("error-NotAllowedError")
                && restart_report
                    .get("device_label_hex")
                    .is_some_and(String::is_empty),
            no_prompt: media_probe_sheet_count(restart_report) == 0,
        },
    }
}
