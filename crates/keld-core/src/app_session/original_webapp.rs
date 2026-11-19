//! Retained BOOT8 graph, Guard-selected source and authenticated pre-app facts.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use keld_guard::ResolvedWindowCsp;
use keld_runtime::primary::{BoundPrimaryGeneration, RoleOwner};
use keld_wv::ApplicationNameForUserAgent;
use keld_wv::preload_image::AppPreloadBudget;
use keld_wv::resource_source::AdmittedWebAppSource;

use super::graph_contract::AdmittedStageGraph;
use super::window_registry::OriginalWindowResources;
use super::{HostAppError, app_detail};

pub(super) struct PreparedOriginalSource {
    graph: Arc<AdmittedStageGraph>,
    source: Arc<AdmittedWebAppSource>,
    preload_budget: AppPreloadBudget,
    application_name_for_user_agent: Option<ApplicationNameForUserAgent>,
}

impl PreparedOriginalSource {
    pub(super) fn prepare(
        graph: Arc<AdmittedStageGraph>,
        policy: ResolvedWindowCsp,
        application_name_for_user_agent: Option<ApplicationNameForUserAgent>,
    ) -> Result<Self, HostAppError> {
        let source = graph.prepare_windows_web_source(policy)?;
        Ok(Self {
            graph,
            source,
            preload_budget: AppPreloadBudget::default(),
            application_name_for_user_agent,
        })
    }

    pub(super) fn bind_runtime(
        self: &Arc<Self>,
        bound: &BoundPrimaryGeneration,
    ) -> Result<OriginalLoadSelection, HostAppError> {
        let versions = bound
            .runtime_versions()
            .filter(|versions| {
                bound.principal().owner() == RoleOwner::Primary && !versions.is_empty()
            })
            .ok_or_else(|| {
                app_detail(
                    "Windows original runtime facts",
                    "the authenticated primary did not supply the required pre-app process facts",
                )
            })?;
        Ok(OriginalLoadSelection {
            prepared: Arc::clone(self),
            versions: versions.clone(),
        })
    }

    pub(super) fn launch_graph(&self) -> &Arc<AdmittedStageGraph> {
        &self.graph
    }

    pub(super) fn launch_policy(&self) -> &ResolvedWindowCsp {
        self.source.resolved_window_policy()
    }
}

pub(super) struct OriginalLoadSelection {
    prepared: Arc<PreparedOriginalSource>,
    versions: BTreeMap<String, String>,
}

impl OriginalLoadSelection {
    pub(super) fn validate_runtime_versions(
        &self,
        versions: Option<&BTreeMap<String, String>>,
    ) -> Result<(), HostAppError> {
        if versions == Some(&self.versions) {
            return Ok(());
        }
        Err(app_detail(
            "Windows original runtime versions",
            "the successor did not supply the same immutable preload Runtime process facts",
        ))
    }

    pub(super) fn prepare_preload(
        &self,
        absolute_selector: &str,
        additional_arguments: Vec<String>,
    ) -> Result<OriginalWindowResources, HostAppError> {
        let selected = self.prepared.graph.selected_source()?;
        let reservation = self
            .prepared
            .preload_budget
            .reserve_window()
            .map_err(|error| app_detail("selected original preload budget", error.to_string()))?;
        let bytes = self
            .prepared
            .graph
            .read_selected_preload_bytes(Path::new(absolute_selector))?;
        let preload = reservation
            .from_qualified_graph(
                bytes,
                selected.preload_sha256(),
                additional_arguments,
                self.versions.clone(),
            )
            .map_err(|error| app_detail("selected original preload image", error.to_string()))?;
        Ok(OriginalWindowResources {
            source: Arc::clone(&self.prepared.source),
            preload: Arc::new(preload),
        })
    }

    pub(super) fn application_name_for_user_agent(&self) -> Option<&ApplicationNameForUserAgent> {
        self.prepared.application_name_for_user_agent.as_ref()
    }
}
