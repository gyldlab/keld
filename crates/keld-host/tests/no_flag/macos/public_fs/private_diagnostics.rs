//! Privacy-safe, non-authoritative public-FS test diagnostics.
//!
//! These markers are not protocol records or process authority. Their exact
//! literal contents must not depend on an untrusted observation, path, PID,
//! token, process image, or captured Host stdout/stderr.

use serde_json::Value;
use std::process::Output;

#[derive(Clone, Copy, Debug)]
pub(super) enum Phase {
    PublicWitness,
    PublicCleanup,
    RecoveryBoot,
    RecoveryConsumer,
    RecoveryPage,
    Observation,
    ProcessDisplay,
    ProcessImageMatch,
    StagedFixtureObservation,
    StagedBoot,
}

impl Phase {
    pub(super) const fn marker(self) -> &'static str {
        match self {
            Self::PublicWitness => "KELD_KEL140_PUBLIC_WITNESS",
            Self::PublicCleanup => "KELD_KEL140_PUBLIC_CLEANUP",
            Self::RecoveryBoot => "KELD_KEL140_RECOVERY_BOOT",
            Self::RecoveryConsumer => "KELD_KEL140_RECOVERY_CONSUMER",
            Self::RecoveryPage => "KELD_KEL140_RECOVERY_PAGE",
            Self::Observation => "KELD_KEL140_OBSERVATION",
            Self::ProcessDisplay => "KELD_KEL140_PROCESS_DISPLAY",
            Self::ProcessImageMatch => "KELD_KEL140_PROCESS_IMAGE_MATCH",
            Self::StagedFixtureObservation => "KELD_KEL143_STAGED_FIXTURE_OBSERVATION",
            Self::StagedBoot => "KELD_KEL140_STAGED_BOOT",
        }
    }
}

pub(super) fn observed_marker(_untrusted_report: &Value, phase: Phase) -> &'static str {
    // Deliberately never interpolate the report, including any optional token,
    // endpoint, embedded private root or executable image.
    phase.marker()
}

/// Assert an untrusted report's `phase` field. A failure names only the fixed
/// marker and the expected literal, never the report or the observed value.
pub(super) fn assert_report_phase(report: &Value, expected: &str, phase: Phase) {
    assert!(
        report["phase"] == expected,
        "{} expected phase {expected}",
        observed_marker(report, phase)
    );
}

/// The original equality predicate over private values (tokens, nonces,
/// endpoints, or records that carry them). A failure names only `what`.
#[track_caller]
pub(super) fn assert_private_eq<L, R>(left: &L, right: &R, what: &str)
where
    L: PartialEq<R> + ?Sized,
    R: ?Sized,
{
    assert!(left == right, "{what} (private values withheld)");
}

/// The original inequality predicate over private values; see
/// [`assert_private_eq`].
#[track_caller]
pub(super) fn assert_private_ne<L, R>(left: &L, right: &R, what: &str)
where
    L: PartialEq<R> + ?Sized,
    R: ?Sized,
{
    assert!(left != right, "{what} (private values withheld)");
}

/// Fixed kind and byte length of an observer HTTP route, never the path:
/// the pending-handler route carries a request nonce.
pub(super) fn route_summary(path: &str) -> String {
    const ROUTES: [(&str, &str); 6] = [
        ("/observe", "observe"),
        ("/quit", "quit"),
        ("/recovery-page", "recovery-page"),
        ("/recovery-pending/", "recovery-pending"),
        ("/recovery-admit/", "recovery-admit"),
        ("/recovery-quit/", "recovery-quit"),
    ];
    let kind = ROUTES
        .iter()
        .find(|(prefix, _)| path.starts_with(prefix))
        .map_or("unknown", |(_, kind)| kind);
    format!("route_kind={kind} path_bytes={}", path.len())
}

/// Success and byte counts of a captured child `Output`, never its bytes.
pub(super) fn output_summary(output: &Output) -> String {
    format!(
        "success={} stdout_bytes={} stderr_bytes={}",
        output.status.success(),
        output.stdout.len(),
        output.stderr.len()
    )
}

pub(super) fn host_exit_summary(output: &Output, report_count: usize) -> String {
    // The exit status is already asserted by the original test. Retain only
    // non-secret counts and success, not any stdout/stderr bytes or payloads.
    format!(
        "KELD_KEL140_PUBLIC_DIAGNOSTICS host_{} reports={report_count}",
        output_summary(output)
    )
}

/// Test-only: run a privacy assertion that must fail and return its
/// formatted panic message, so a sentinel test can prove the message names
/// only the fixed reason. One owner for every `public_fs` sentinel test.
#[cfg(test)]
pub(super) fn private_failure_message(check: impl FnOnce() + std::panic::UnwindSafe) -> String {
    std::panic::catch_unwind(check)
        .expect_err("the original predicate still fails")
        .downcast_ref::<String>()
        .expect("formatted assertion message")
        .clone()
}

#[cfg(test)]
#[path = "private_diagnostics/source_guard.rs"]
mod source_guard;

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt as _;

    #[test]
    fn private_host_and_observation_bytes_cannot_reach_fixed_diagnostics() {
        const PRIVATE: &str = "KELD_TEST_PRIVATE_SENTINEL_token_endpoint_root";
        let raw_report = serde_json::json!({
            "phase": "app-ready",
            "token": PRIVATE,
            "endpoint": PRIVATE,
            "executable": PRIVATE
        });
        let output = Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: PRIVATE.as_bytes().to_vec(),
            stderr: PRIVATE.as_bytes().to_vec(),
        };
        let route = route_summary(&format!("/recovery-pending/42/{PRIVATE}"));
        assert_eq!(
            route,
            format!(
                "route_kind=recovery-pending path_bytes={}",
                21 + PRIVATE.len()
            )
        );
        assert!(!route.contains(PRIVATE));
        assert_eq!(
            route_summary(PRIVATE),
            format!("route_kind=unknown path_bytes={}", PRIVATE.len())
        );
        let safe_host = host_exit_summary(&output, 3);
        assert!(safe_host.contains("host_success=true"));
        assert!(safe_host.contains("reports=3"));
        assert!(safe_host.contains("stderr_bytes="));
        assert!(!safe_host.contains(PRIVATE));
        assert!(!safe_host.contains("token"));
        assert!(!safe_host.contains("endpoint"));
        for phase in [
            Phase::PublicWitness,
            Phase::PublicCleanup,
            Phase::RecoveryBoot,
            Phase::RecoveryConsumer,
            Phase::RecoveryPage,
            Phase::Observation,
            Phase::ProcessDisplay,
            Phase::ProcessImageMatch,
            Phase::StagedFixtureObservation,
            Phase::StagedBoot,
        ] {
            let marker = observed_marker(&raw_report, phase);
            assert!(marker.starts_with("KELD_KEL"));
            assert!(!marker.contains(PRIVATE));
            assert!(!marker.contains("token"));
            assert!(!marker.contains("endpoint"));
        }
    }

    #[test]
    fn report_phase_failure_names_only_the_fixed_marker_and_expected_phase() {
        const PRIVATE: &str = "KELD_TEST_PRIVATE_SENTINEL_phase_token";
        assert_report_phase(
            &serde_json::json!({ "phase": "app-ready", "token": PRIVATE }),
            "app-ready",
            Phase::RecoveryBoot,
        );
        let leaked = serde_json::json!({ "phase": PRIVATE, "token": PRIVATE });
        let message = private_failure_message(|| {
            assert_report_phase(&leaked, "app-ready", Phase::RecoveryBoot);
        });
        assert!(message.contains("KELD_KEL140_RECOVERY_BOOT"), "{message}");
        assert!(message.contains("app-ready"), "{message}");
        assert!(!message.contains(PRIVATE));
    }

    #[test]
    fn private_equality_failures_name_only_the_field() {
        const PRIVATE: &str = "KELD_TEST_PRIVATE_SENTINEL_equality_nonce";
        let report = serde_json::json!({ "nonce": PRIVATE });
        assert_private_eq(&report["nonce"], PRIVATE, "request nonce");
        assert_private_ne(&report["nonce"], "other", "fresh nonce");
        for message in [
            private_failure_message(|| {
                assert_private_eq(&report["nonce"], "other", "request nonce");
            }),
            private_failure_message(|| {
                assert_private_ne(&report["nonce"], PRIVATE, "request nonce");
            }),
        ] {
            assert!(message.contains("request nonce"), "{message}");
            assert!(!message.contains(PRIVATE));
            assert!(!message.contains("other"));
        }
    }
}
