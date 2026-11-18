//! Shared typed guardian lifecycle failure mapping and aggregation.
//! One error owner; child modules retain exact existing error surfaces.

use crate::RuntimeError;
use nix::errno::Errno;
use std::io;

/// Original typed failures from one macOS Guardian cleanup transaction.
///
/// A Guardian can fail both its exact Child wait (for example ECHILD) and
/// its attested process-group absence proof. The public outer error keeps its
/// original variant and diagnostics. Callers can downcast the Lifecycle or
/// `GuardianExited` cleanup `io::Error::get_ref()` to this type to inspect every
/// original typed cause, including its OS errno.
/// Standard `Error::source()` chains are linear and expose only the first
/// cause. `failures()` is the authoritative complete collection.
#[derive(Debug, Clone)]
pub struct GuardianCleanupCauses {
    failures: Vec<RuntimeError>,
}

impl GuardianCleanupCauses {
    /// Returns the failures in the exact order cleanup observed them.
    #[must_use]
    pub fn failures(&self) -> &[RuntimeError] {
        &self.failures
    }
}

impl std::fmt::Display for GuardianCleanupCauses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, error) in self.failures.iter().enumerate() {
            if index != 0 {
                write!(f, "; ")?;
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for GuardianCleanupCauses {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.failures.first().map(|cause| cause as _)
    }
}

pub(super) fn nix_io_error(source: Errno) -> io::Error {
    io::Error::from_raw_os_error(source as i32)
}

pub(super) fn lifecycle_error(phase: &'static str, source: io::Error) -> RuntimeError {
    RuntimeError::Lifecycle { phase, source }
}

/// Preserve a terminal Guardian's single typed cleanup cause in the same
/// clone-aware collection used by multi-error lifecycle cleanup.
pub(super) fn typed_cleanup_io_error(failure: RuntimeError) -> io::Error {
    io::Error::other(GuardianCleanupCauses {
        failures: vec![failure],
    })
}

pub(super) fn collapse_failures(mut failures: Vec<RuntimeError>) -> RuntimeError {
    if failures.len() == 1
        && let Some(error) = failures.pop()
    {
        return error;
    }
    lifecycle_error(
        "macOS guardian cleanup",
        io::Error::other(GuardianCleanupCauses { failures }),
    )
}

#[cfg(test)]
mod tests {
    use super::super::process_group::{GroupIdentity, reconcile_denied_signal_after_reap_for_test};
    use super::*;
    use std::time::{Duration, Instant};

    // Exercise the shipping denial classifier with a controlled absence
    // observation. This proves error-source fidelity and introduces no native
    // process-group signal or claim about an actual kernel EPERM event.
    fn denied_with_unproven_absence() -> (GroupIdentity, RuntimeError) {
        let group = GroupIdentity::unverified_for_test(7_331);
        let deadline = Instant::now() + Duration::from_secs(1);
        let before = deadline - Duration::from_millis(1);
        let after = deadline + Duration::from_millis(1);
        let mut clock_reads = 0;
        let denied = reconcile_denied_signal_after_reap_for_test(
            group,
            lifecycle_error(
                "macOS guardian process-group signal",
                io::Error::from_raw_os_error(nix::libc::EPERM),
            ),
            deadline,
            || {
                clock_reads += 1;
                if clock_reads == 1 { before } else { after }
            },
            |raw| {
                assert_eq!(raw, 7_331);
                (Err(Errno::ESRCH), Err(Errno::EPERM))
            },
        )
        .expect_err("a denied signal with a held group must remain fatal");
        assert_eq!(clock_reads, 2, "test must exercise the absence observer");
        (group, denied)
    }

    fn assert_original_denial_source(error: &RuntimeError) {
        let RuntimeError::Lifecycle { phase, source } = error else {
            panic!("the original denied signal must remain Lifecycle: {error}");
        };
        assert_eq!(*phase, "macOS guardian process-group signal");
        assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
        let original = source
            .get_ref()
            .and_then(|cause| cause.source())
            .and_then(|cause| cause.downcast_ref::<io::Error>())
            .expect("the nested denied signal must retain its original typed kernel I/O cause");
        assert_eq!(
            original.raw_os_error(),
            Some(nix::libc::EPERM),
            "original signal-phase errno must survive the error clone"
        );
    }

    #[test]
    fn cloned_unproven_denied_signal_keeps_original_eperm_and_reconciliation() {
        let (group, denied) = denied_with_unproven_absence();
        assert_original_denial_source(&denied);
        let copied = denied.clone();
        assert_original_denial_source(&copied);
        let mut probes = 0;
        let observed_at = Instant::now();
        reconcile_denied_signal_after_reap_for_test(
            group,
            copied,
            observed_at + Duration::from_secs(1),
            || observed_at,
            |raw| {
                assert_eq!(raw, 7_331);
                probes += 1;
                (Err(Errno::ESRCH), Err(Errno::ESRCH))
            },
        )
        .expect("a cloned typed denial permits read-only reconciliation after dual ESRCH");
        assert_eq!(probes, 1, "the original group absence must be observed");
    }

    #[test]
    fn cloned_guardian_cleanup_preserves_nested_unproven_eperm() {
        let (group, denied) = denied_with_unproven_absence();
        let wrapped = RuntimeError::GuardianExited {
            group_pid: group.pid,
            exit_code: None,
            cleanup_error: Some(typed_cleanup_io_error(denied)),
        };
        let copied = wrapped.clone();
        for returned in [&wrapped, &copied] {
            let RuntimeError::GuardianExited {
                cleanup_error: Some(cleanup),
                ..
            } = returned
            else {
                panic!("the Guardian terminal report must retain its typed cleanup: {returned}");
            };
            let causes = cleanup
                .get_ref()
                .and_then(|source| source.downcast_ref::<GuardianCleanupCauses>())
                .expect("original and cloned terminal errors must keep structured causes");
            assert_eq!(causes.failures().len(), 1);
            assert_original_denial_source(&causes.failures()[0]);
        }
    }

    #[test]
    fn aggregate_keeps_each_typed_failure_and_errno() {
        let failure = collapse_failures(vec![
            lifecycle_error(
                "macOS guardian generation shutdown wait",
                io::Error::from_raw_os_error(nix::libc::ECHILD),
            ),
            lifecycle_error(
                "macOS guardian group absence",
                io::Error::new(io::ErrorKind::TimedOut, "group still present"),
            ),
        ]);
        let RuntimeError::Lifecycle { phase, source } = failure else {
            panic!("aggregate must retain existing Lifecycle outer error");
        };
        assert_eq!(phase, "macOS guardian cleanup");
        let causes = source
            .get_ref()
            .and_then(|source| source.downcast_ref::<GuardianCleanupCauses>())
            .expect("original errors remain typed and discoverable");
        assert_eq!(causes.failures().len(), 2);
        assert!(
            matches!(&causes.failures()[0], RuntimeError::Lifecycle { phase, source }
                if *phase == "macOS guardian generation shutdown wait"
                    && source.raw_os_error() == Some(nix::libc::ECHILD))
        );
        assert!(
            matches!(&causes.failures()[1], RuntimeError::Lifecycle { phase, source }
                if *phase == "macOS guardian group absence"
                    && source.kind() == io::ErrorKind::TimedOut)
        );
        assert!(causes.to_string().contains("group still present"));
    }

    #[test]
    fn cloning_cleanup_error_keeps_typed_causes() {
        let original = collapse_failures(vec![
            lifecycle_error(
                "exact Guardian reap",
                io::Error::from_raw_os_error(nix::libc::ECHILD),
            ),
            lifecycle_error(
                "retained group",
                io::Error::new(io::ErrorKind::TimedOut, "group held"),
            ),
        ]);
        let copied = original.clone();
        let RuntimeError::Lifecycle { source, .. } = copied else {
            panic!("cloned aggregate must retain Lifecycle classification");
        };
        let causes = source
            .get_ref()
            .and_then(|source| source.downcast_ref::<GuardianCleanupCauses>())
            .expect("clone must retain structured failure collection");
        assert!(
            matches!(&causes.failures()[0], RuntimeError::Lifecycle { source, .. }
            if source.raw_os_error() == Some(nix::libc::ECHILD))
        );
        assert!(
            matches!(&causes.failures()[1], RuntimeError::Lifecycle { source, .. }
            if source.kind() == io::ErrorKind::TimedOut)
        );
    }

    #[test]
    fn single_failure_keeps_original_variant_and_errno() {
        let failure = collapse_failures(vec![lifecycle_error(
            "macOS guardian generation shutdown wait",
            io::Error::from_raw_os_error(nix::libc::ECHILD),
        )]);
        assert!(matches!(failure, RuntimeError::Lifecycle { phase, source }
            if phase == "macOS guardian generation shutdown wait"
                && source.raw_os_error() == Some(nix::libc::ECHILD)));
    }
}
