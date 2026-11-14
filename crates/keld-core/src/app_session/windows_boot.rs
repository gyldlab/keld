//! Windows original boot joins the already admitted graph, verified policy,
//! and authenticated Runtime facts. Generic facades retain selection=None.

use super::launch_facts::{DevelopmentLaunchFacts, configure_original_primary_launch};
use super::original_webapp::{OriginalLoadSelection, PreparedOriginalSource};
use super::{DirectSessionInputs, HostAppError, PrimaryRoleConfig, app_detail};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

mod recheck;

pub(super) fn prepare_original(
    inputs: &DirectSessionInputs<'_>,
) -> Result<Option<PreparedOriginalSource>, HostAppError> {
    let Some(graph) = inputs.original_graph else {
        return Ok(None);
    };
    if inputs.html.is_some() {
        return Err(app_detail(
            "Windows original boot",
            "a retained graph requires the managed facade path",
        ));
    }
    let snapshot = inputs.guard_snapshot.ok_or_else(|| {
        app_detail(
            "Windows original policy",
            "verified Guard snapshot is absent",
        )
    })?;
    let profile = snapshot.fs.admitted_profile.ok_or_else(|| {
        app_detail(
            "Windows original policy",
            "actual profile admission is absent",
        )
    })?;
    let policy = snapshot
        .verified
        .resolve_unique_original_window_csp()
        .map_err(HostAppError::manifest)?;
    PreparedOriginalSource::prepare(
        Arc::clone(graph),
        profile,
        policy,
        keld_wv::preload_image::AppPreloadBudget::default(),
    )
    .map(Some)
}

pub(super) fn original_primary_config(
    root: &Path,
    original_entry: &Path,
    prepared: &PreparedOriginalSource,
    development_launch_facts: Option<&DevelopmentLaunchFacts>,
) -> Result<PrimaryRoleConfig, HostAppError> {
    let graph = prepared.graph();
    let entry = graph.windows_runtime_entry(root, original_entry)?;
    // The canonical entry imports this fixed API owner. Validate its exact
    // graph row before launching; do not select the legacy staged sidecar.
    graph.installed_runtime_preload()?;
    Ok(configure_original_primary_launch(
        PrimaryRoleConfig::new("bun")
            .source_recheck(graph.clone())
            .env_remove(keld_runtime::primary::BUN_OPTIONS_ENV)
            .arg("--define=process.env.KELD_NATIVE_WINDOW_FACADE:\"1\"")
            .arg("--define=__KELD_RUNTIME_PROCESS_FACTS__:true"),
        development_launch_facts,
        root,
        prepared,
    )?
    .arg("run")
    .arg(entry)
    .arg(root.join(original_entry))
    .require_runtime_process_facts())
}

pub(super) fn bind_runtime_facts(
    prepared: Option<PreparedOriginalSource>,
    versions: Option<&BTreeMap<String, String>>,
) -> Result<Option<OriginalLoadSelection>, HostAppError> {
    prepared
        .map(|prepared| {
            let versions = versions.ok_or_else(|| {
                app_detail(
                    "Windows original runtime facts",
                    "authenticated generation has no pre-app Runtime process facts",
                )
            })?;
            // No source reread, substitute graph, or guessed Bun/Host version.
            Ok(prepared.with_runtime_versions(versions.clone()))
        })
        .transpose()
}

#[cfg(test)]
mod tests;
