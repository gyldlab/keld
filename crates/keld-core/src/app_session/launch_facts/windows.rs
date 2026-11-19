//! Selected BOOT8 facts for the one retained Windows original Runtime.

use std::path::Path;
use std::sync::Arc;

use keld_runtime::primary::{BUN_OPTIONS_ENV, PrimaryRoleConfig};
use keld_update::WindowsInstanceLeaseFile;
use serde::Serialize;

use super::original_webapp::PreparedOriginalSource;
use super::windows_instance_election::{self, Election};
use super::{
    DEV_LEASE_ENV, HostAppError, SessionShutdownState, WindowsDevLeaseMonitor, WindowsProfileMode,
    run_direct_startup_if_session_running,
};
use crate::SingleInstanceDeclaration;

#[path = "windows_native.rs"]
mod native;
#[path = "original_policy.rs"]
mod original_policy;
#[path = "windows_paths.rs"]
mod paths;
use original_policy::OriginalResourcePolicyDescriptor;

const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_FIELD_BYTES: usize = 4096;

/// Once selected by the existing boot owner, the caller keeps the lease at
/// Host scope and can move it into retained terminal custody.
pub(super) struct PreparedLaunch {
    pub(super) config: PrimaryRoleConfig,
    pub(super) winner_lease: Option<WindowsInstanceLeaseFile>,
}

pub(super) struct SelectedLaunch<'a> {
    pub(super) root: &'a Path,
    pub(super) entry: &'a Path,
    pub(super) name: &'a str,
    pub(super) version: Option<&'a str>,
    pub(super) declaration: Option<&'a SingleInstanceDeclaration>,
    pub(super) source: &'a PreparedOriginalSource,
}

/// Capture and elect once, after BOOT8, Guard and the actual dev lease admit.
/// The existing startup cut owns publication. The normal session launch still
/// owns the final spawn check, common environment, Job and recovery decisions.
pub(super) fn prepare_original(
    selected: SelectedLaunch<'_>,
    profile: WindowsProfileMode,
    dev_lease: Option<&WindowsDevLeaseMonitor>,
    shutdown: &SessionShutdownState,
) -> Result<Option<PreparedLaunch>, HostAppError> {
    prepare_if_running(shutdown, || {
        if dev_lease.is_none() || !matches!(profile, WindowsProfileMode::EphemeralDev) {
            return Err(invalid(
                "selected original requires the live Windows development lease",
            ));
        }
        prepare_selected(
            selected,
            native::capture,
            windows_instance_election::acquire,
        )
    })
}

fn prepare_if_running<T>(
    shutdown: &SessionShutdownState,
    prepare: impl FnOnce() -> Result<T, HostAppError>,
) -> Result<Option<T>, HostAppError> {
    if run_direct_startup_if_session_running(shutdown, || ()).is_none() {
        return Ok(None);
    }
    // Native observation and lease I/O run outside the transition. EOF that
    // wins here discards the provisional result, including its actual lease.
    let provisional = prepare();
    run_direct_startup_if_session_running(shutdown, || provisional).transpose()
}

fn prepare_selected(
    selected: SelectedLaunch<'_>,
    observe: impl FnOnce() -> Result<native::Observation, HostAppError>,
    elect: impl FnOnce(&SingleInstanceDeclaration) -> Result<Election, HostAppError>,
) -> Result<PreparedLaunch, HostAppError> {
    let graph = selected.source.launch_graph();
    let wrapper = graph.windows_runtime_entry(selected.root, selected.entry)?;
    let mut facts = OriginalFacts::new(&selected)?;
    facts.observe(observe()?)?;
    let winner_lease = facts.set_election(selected.declaration.map(elect).transpose()?);
    let config = PrimaryRoleConfig::new("bun")
        .source_recheck(Arc::clone(graph))
        .current_dir(selected.root)
        .env_remove(BUN_OPTIONS_ENV)
        .env_remove(DEV_LEASE_ENV)
        .env_remove(keld_runtime::windows_job::WINDOWS_LAUNCH_GATE_ENV)
        .arg("--no-env-file")
        .arg("--no-install")
        .arg("--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\"")
        .arg("--define=__KELD_RUNTIME_PROCESS_FACTS__:true")
        .arg("--define=__KELD_WINDOWS_HOST__:true")
        .arg(facts.bun_argument()?)
        .arg("run")
        .arg(wrapper)
        .arg(selected.root.join(selected.entry))
        .require_runtime_process_facts();
    Ok(PreparedLaunch {
        config,
        winner_lease,
    })
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchDirectories {
    #[serde(skip_serializing_if = "Option::is_none")]
    app_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    documents: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum Verdict {
    Acquired,
    Contended,
}

#[derive(Debug, Serialize)]
struct SingleInstanceFact {
    verdict: Verdict,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OriginalFacts<'a> {
    schema: u8,
    kind: &'static str,
    name: &'a str,
    root: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locale: Option<String>,
    paths: LaunchDirectories,
    original_resource_policy: OriginalResourcePolicyDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    single_instance: Option<SingleInstanceFact>,
}

impl<'a> OriginalFacts<'a> {
    fn new(selected: &SelectedLaunch<'a>) -> Result<Self, HostAppError> {
        let root = selected
            .root
            .to_str()
            .filter(|root| valid_text(root) && paths::retained_root(root))
            .ok_or_else(|| invalid("selected root is not a bounded verbatim local Windows path"))?;
        if !valid_text(selected.name) || selected.version.is_some_and(|value| !valid_text(value)) {
            return Err(invalid(
                "selected name or version is empty, contains NUL, or exceeds 4096 UTF-8 bytes",
            ));
        }
        let policy = OriginalResourcePolicyDescriptor::from_verified(
            selected.source.launch_graph().graph_sha256(),
            selected.source.launch_policy(),
        );
        policy.validate()?;
        let facts = Self {
            schema: 8,
            kind: "development",
            name: selected.name,
            root,
            version: selected.version,
            locale: None,
            paths: LaunchDirectories::default(),
            original_resource_policy: policy,
            single_instance: None,
        };
        facts.encode()?;
        Ok(facts)
    }

    fn observe(&mut self, observed: native::Observation) -> Result<(), HostAppError> {
        let user_data = paths::user_data(observed.app_data.as_deref(), self.name);
        if user_data.as_deref().is_some_and(|value| !valid_text(value)) {
            return Err(invalid("derived userData exceeds 4096 UTF-8 bytes"));
        }
        self.locale = observed.locale;
        self.paths = LaunchDirectories {
            app_data: observed.app_data,
            user_data,
            documents: observed.documents,
        };
        Ok(())
    }

    fn set_election(&mut self, election: Option<Election>) -> Option<WindowsInstanceLeaseFile> {
        let (verdict, lease) = match election? {
            Election::Acquired(lease) => (Verdict::Acquired, Some(lease)),
            Election::Contended => (Verdict::Contended, None),
        };
        self.single_instance = Some(SingleInstanceFact { verdict });
        lease
    }

    fn bun_argument(&self) -> Result<String, HostAppError> {
        self.encode()
            .map(|encoded| format!("--define=__KELD_HOST_LAUNCH_FACTS__:{encoded}"))
    }

    fn encode(&self) -> Result<String, HostAppError> {
        let encoded = serde_json::to_string(self).map_err(|source| invalid(source.to_string()))?;
        if encoded.len() > MAX_RECORD_BYTES {
            return Err(invalid("private development launch facts exceed 16 KiB"));
        }
        Ok(encoded)
    }
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
#[path = "selected_windows_tests.rs"]
mod tests;
