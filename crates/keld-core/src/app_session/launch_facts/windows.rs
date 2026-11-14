//! Existing schema-3 development facts from the validated Windows boot owner.
//! Explicit metadata and native observations are captured once; they grant no access.

use std::path::Path;

use serde::Serialize;

use super::{
    HostAppError, PrimaryRoleConfig, SessionShutdownState, WindowsDevLeaseMonitor,
    WindowsProfileMode, run_direct_startup_if_session_running,
};

#[path = "windows_native.rs"]
mod native;
#[path = "windows_paths.rs"]
mod paths;

const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_FIELD_BYTES: usize = 4096;

#[derive(Debug, Default, Serialize)]
struct LaunchDirectories {
    #[serde(rename = "appData", skip_serializing_if = "Option::is_none")]
    app_data: Option<String>,
    #[serde(rename = "userData", skip_serializing_if = "Option::is_none")]
    user_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    documents: Option<String>,
}

/// One immutable record retained by `DirectSessionInputs` and each recovered launch.
#[derive(Debug, Serialize)]
pub(super) struct DevelopmentLaunchFacts {
    schema: u8,
    kind: &'static str,
    name: String,
    root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locale: Option<String>,
    paths: LaunchDirectories,
}

impl DevelopmentLaunchFacts {
    /// Pure construction from already admitted, explicit metadata. This value is
    /// unpublished until the existing final running-session cut accepts it.
    fn from_validated_development(
        name: &str,
        root: &Path,
        version: Option<&str>,
    ) -> Result<Option<Self>, HostAppError> {
        let Some(root) = root.to_str() else {
            return Ok(None);
        };
        if paths::retained_drive_tail(root).is_none() {
            return Ok(None);
        }
        if !valid_text(name) || !valid_text(root) || version.is_some_and(|value| !valid_text(value))
        {
            return Err(invalid(
                "development name, root or declared version is empty, contains NUL, or exceeds 4096 UTF-8 bytes",
            ));
        }
        if !paths::retained_root(root) {
            return Ok(None);
        }
        let facts = Self {
            schema: 3,
            kind: "development",
            name: name.to_owned(),
            root: root.to_owned(),
            version: version.map(str::to_owned),
            locale: None,
            paths: LaunchDirectories::default(),
        };
        facts.private_argument()?;
        Ok(Some(facts))
    }

    fn with_observation(mut self, observed: native::Observation) -> Result<Self, HostAppError> {
        let user_data = paths::user_data(observed.app_data.as_deref(), &self.name);
        if user_data.as_deref().is_some_and(|value| !valid_text(value)) {
            return Err(invalid("derived userData exceeds 4096 UTF-8 bytes"));
        }
        self.locale = observed.locale;
        self.paths = LaunchDirectories {
            app_data: observed.app_data,
            user_data,
            documents: observed.documents,
        };
        // Escaping and all simultaneously present fields share the existing bound.
        self.private_argument()?;
        Ok(self)
    }

    fn private_argument(&self) -> Result<String, HostAppError> {
        let encoded = serde_json::to_string(self).map_err(|source| invalid(source.to_string()))?;
        if encoded.len() > MAX_RECORD_BYTES {
            return Err(invalid("private development launch facts exceed 16 KiB"));
        }
        Ok(encoded)
    }

    pub(super) fn bun_define_argument(&self) -> Result<String, HostAppError> {
        self.private_argument()
            .map(|encoded| format!("--define=__KELD_HOST_LAUNCH_FACTS__:{encoded}"))
    }
}

/// The real platform query remains outside canonical locks; the final existing
/// running-session cut alone publishes this one captured record.
pub(super) fn capture_development_launch_facts(
    name: &str,
    root: &Path,
    version: Option<&str>,
    native_facade: bool,
    profile: WindowsProfileMode,
    dev_lease: Option<&WindowsDevLeaseMonitor>,
    shutdown: &SessionShutdownState,
) -> Result<Option<DevelopmentLaunchFacts>, HostAppError> {
    if !native_facade
        || !matches!(profile, WindowsProfileMode::EphemeralDev)
        || dev_lease.is_none()
        || run_direct_startup_if_session_running(shutdown, || ()).is_none()
    {
        return Ok(None);
    }
    let Some(facts) = DevelopmentLaunchFacts::from_validated_development(name, root, version)?
    else {
        return Ok(None);
    };
    capture_with_observer(facts, shutdown, native::capture)
}

fn capture_with_observer(
    facts: DevelopmentLaunchFacts,
    shutdown: &SessionShutdownState,
    observe: impl FnOnce() -> Result<native::Observation, HostAppError>,
) -> Result<Option<DevelopmentLaunchFacts>, HostAppError> {
    if run_direct_startup_if_session_running(shutdown, || ()).is_none() {
        return Ok(None);
    }
    // No transition/current/Registry guard crosses native retrieval. A lease EOF
    // that wins during observation makes the final cut refuse even a valid result.
    let captured = observe().and_then(|observed| facts.with_observation(observed));
    run_direct_startup_if_session_running(shutdown, || captured).transpose()
}

/// The same parse-time constants precede run for both generic and original launches.
pub(super) fn configure_primary_launch(
    config: PrimaryRoleConfig,
    facts: Option<&DevelopmentLaunchFacts>,
    desktop_selected: bool,
) -> Result<PrimaryRoleConfig, HostAppError> {
    let argument = facts
        .map(DevelopmentLaunchFacts::bun_define_argument)
        .transpose()?
        .unwrap_or_else(|| String::from("--define=__KELD_HOST_LAUNCH_FACTS__:undefined"));
    Ok(config
        .arg("--define=__KELD_WINDOWS_HOST__:true")
        .arg(if desktop_selected {
            "--define=__KELD_WINDOWS_DESKTOP_SNAPSHOT__:true"
        } else {
            "--define=__KELD_WINDOWS_DESKTOP_SNAPSHOT__:false"
        })
        .arg(argument))
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_FIELD_BYTES && !value.contains('\0')
}

fn invalid(detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-037",
        "private development launch facts",
        detail,
        "Restage and launch through the validated no-flag Host.",
    )
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
