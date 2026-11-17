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
    use super::*;

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
