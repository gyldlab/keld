//! No-flag host diagnostic contract and startup-resource attempt snapshot.

use std::fmt;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::io;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::sync::atomic::{AtomicU32, Ordering};

use keld_guard::ManifestError;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_native::fs::FsPrepareError;

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use super::{CHILD_ATTEMPTS, LISTENER_ATTEMPTS, WINDOW_ATTEMPTS};

/// Typed no-flag host boot/session failure.
pub struct HostAppError {
    pub(super) code: &'static str,
    pub(super) phase: &'static str,
    pub(super) detail: String,
    fix: &'static str,
    pub(super) resources: StartupResourceSnapshot,
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub(super) manifest_source: Option<Box<ManifestError>>,
    #[cfg(target_os = "macos")]
    pub(super) runtime_source: Option<Box<keld_runtime::RuntimeError>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct StartupResourceSnapshot {
    listener: u32,
    child: u32,
    window: u32,
}

impl HostAppError {
    pub(super) fn new(
        code: &'static str,
        phase: &'static str,
        detail: impl Into<String>,
        fix: &'static str,
    ) -> Self {
        Self {
            code,
            phase,
            detail: detail.into(),
            fix,
            resources: startup_resource_snapshot(),
            #[cfg(any(target_os = "macos", target_os = "linux", windows))]
            manifest_source: None,
            #[cfg(target_os = "macos")]
            runtime_source: None,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub(super) fn manifest(source: ManifestError) -> Self {
        let code = source.code();
        let detail = source.to_string();
        Self {
            code,
            phase: "permissions manifest preflight",
            detail,
            fix: "Apply the keld-guard correction above, rebuild the staged boot artifact, and relaunch.",
            resources: startup_resource_snapshot(),
            manifest_source: Some(Box::new(source)),
            #[cfg(target_os = "macos")]
            runtime_source: None,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub(super) fn fs_prepare(source: &FsPrepareError) -> Self {
        Self::new(
            source.code(),
            "filesystem broker preflight",
            source.to_string(),
            "Correct the filesystem scopes in keld.permissions.jsonc, rebuild the staged boot artifact, and relaunch.",
        )
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub(super) fn io(
        code: &'static str,
        phase: &'static str,
        source: &io::Error,
        fix: &'static str,
    ) -> Self {
        Self::new(code, phase, source.to_string(), fix)
    }

    /// Stable registered diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for HostAppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: no-flag host failed during {} — {}. {} \
             [startup-resource-attempts listener={} child={} window={}]",
            self.code,
            self.phase,
            self.detail,
            self.fix,
            self.resources.listener,
            self.resources.child,
            self.resources.window,
        )
    }
}

impl fmt::Debug for HostAppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostAppError")
            .field("code", &self.code)
            .field("phase", &self.phase)
            .field("detail", &self.detail)
            .field("resources", &self.resources)
            .field("runtime_source", &{
                #[cfg(target_os = "macos")]
                {
                    self.runtime_source
                        .as_ref()
                        .map(std::string::ToString::to_string)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Option::<String>::None
                }
            })
            .field("manifest_source", &{
                #[cfg(any(target_os = "macos", target_os = "linux", windows))]
                {
                    self.manifest_source.as_ref()
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
                {
                    Option::<&ManifestError>::None
                }
            })
            .finish_non_exhaustive()
    }
}

impl std::error::Error for HostAppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        #[cfg(any(target_os = "macos", target_os = "linux", windows))]
        {
            self.manifest_source
                .as_ref()
                .map(|source| source.as_ref() as &(dyn std::error::Error + 'static))
                .or_else(|| {
                    #[cfg(target_os = "macos")]
                    {
                        self.runtime_source
                            .as_ref()
                            .map(|source| source.as_ref() as &(dyn std::error::Error + 'static))
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        None
                    }
                })
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        {
            None
        }
    }
}

pub(super) fn startup_resource_snapshot() -> StartupResourceSnapshot {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        snapshot_counters(&LISTENER_ATTEMPTS, &CHILD_ATTEMPTS, &WINDOW_ATTEMPTS)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        StartupResourceSnapshot::default()
    }
}

/// Maps each attempt counter to its own snapshot field; the process-global counters
/// are passed in so the field mapping is testable without racing other tests.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn snapshot_counters(
    listener: &AtomicU32,
    child: &AtomicU32,
    window: &AtomicU32,
) -> StartupResourceSnapshot {
    StartupResourceSnapshot {
        listener: listener.load(Ordering::Acquire),
        child: child.load(Ordering::Acquire),
        window: window.load(Ordering::Acquire),
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
mod tests {
    use std::sync::atomic::AtomicU32;

    use super::{HostAppError, StartupResourceSnapshot, snapshot_counters};

    #[test]
    fn startup_resource_snapshot_maps_each_counter_to_its_own_field() {
        let snapshot = snapshot_counters(
            &AtomicU32::new(11),
            &AtomicU32::new(22),
            &AtomicU32::new(33),
        );
        assert_eq!(snapshot.listener, 11, "listener field");
        assert_eq!(snapshot.child, 22, "child field");
        assert_eq!(snapshot.window, 33, "window field");

        let error = HostAppError {
            resources: snapshot,
            ..HostAppError::new("KELD-CORE-037", "phase", "detail", "fix")
        };
        assert_eq!(
            error.resources,
            StartupResourceSnapshot {
                listener: 11,
                child: 22,
                window: 33,
            }
        );
        assert!(
            error
                .to_string()
                .ends_with("[startup-resource-attempts listener=11 child=22 window=33]"),
            "{error}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn startup_cleanup_keeps_first_typed_guardian_cause() {
        use std::error::Error as _;
        use std::io;

        use keld_runtime::RuntimeError;

        let first = RuntimeError::Lifecycle {
            phase: "original guardian failure",
            source: io::Error::from_raw_os_error(nix::libc::ECHILD),
        };
        let second = RuntimeError::Lifecycle {
            phase: "cleanup guardian failure",
            source: io::Error::from_raw_os_error(nix::libc::ETIMEDOUT),
        };
        let primary = super::super::app_guardian_fatal("guardian failed", &first);
        let cleanup = super::super::app_guardian_fatal("guardian cleanup", &second);
        let combined = super::super::append_app_cleanup(primary, [Err(cleanup)]);
        let cause = combined
            .source()
            .and_then(|source| source.downcast_ref::<RuntimeError>())
            .expect("original typed Guardian cause remains first");
        assert!(
            matches!(cause, RuntimeError::Lifecycle { phase: "original guardian failure", source }
                if source.raw_os_error() == Some(nix::libc::ECHILD)),
            "cleanup must not replace original typed failure: {cause:?}"
        );
        assert!(
            combined.to_string().contains("cleanup guardian failure"),
            "later failure remains in diagnostic text: {combined}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn startup_cleanup_preserves_typed_guardian_failure_if_primary_is_plain() {
        use std::error::Error as _;
        use std::io;

        use keld_runtime::RuntimeError;

        let primary = HostAppError::new(
            "KELD-CORE-037",
            "primary startup",
            "primary startup failed",
            "fix the primary error",
        );
        let runtime = RuntimeError::Lifecycle {
            phase: "guardian native wait",
            source: io::Error::from_raw_os_error(nix::libc::ECHILD),
        };
        let cleanup = super::super::app_guardian_fatal("guardian shutdown", &runtime);
        let combined = super::super::append_app_cleanup(primary, [Err(cleanup)]);
        assert!(combined.to_string().contains("cleanup:"), "{combined}");
        let cause = combined
            .source()
            .and_then(|source| source.downcast_ref::<RuntimeError>())
            .expect("original typed Guardian failure must survive startup cleanup");
        assert!(
            matches!(cause, RuntimeError::Lifecycle { phase: "guardian native wait", source }
                if source.raw_os_error() == Some(nix::libc::ECHILD)),
            "original ECHILD and phase must remain inspectable: {cause:?}"
        );
    }
}
