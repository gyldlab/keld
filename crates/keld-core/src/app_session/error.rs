//! No-flag host diagnostic contract and startup-resource attempt snapshot.

use std::fmt;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::io;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::sync::atomic::Ordering;

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
    pub(super) fix: &'static str,
    pub(super) resources: StartupResourceSnapshot,
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub(super) manifest_source: Option<Box<ManifestError>>,
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
        StartupResourceSnapshot {
            listener: LISTENER_ATTEMPTS.load(Ordering::Acquire),
            child: CHILD_ATTEMPTS.load(Ordering::Acquire),
            window: WINDOW_ATTEMPTS.load(Ordering::Acquire),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        StartupResourceSnapshot::default()
    }
}
