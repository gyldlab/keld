//! Transfer the verified selected asset closure to the existing webview source owner.

use std::sync::Arc;

use keld_guard::ResolvedWindowCsp;
use keld_wv::resource_source::{AdmittedWebAppSource, VerifiedWebAsset};

use super::{
    AdmittedStageGraph, HostAppError, census_directory, file_metadata, graph_error, parse_sha,
    path_metadata, relative_path, same_identity,
};

impl AdmittedStageGraph {
    /// Returns the single Windows document source backed by the retained graph.
    ///
    /// # Errors
    /// Rejects changed graph bytes or identities, an unbound selected source or
    /// an invalid native source/policy combination.
    pub(in crate::app_session) fn prepare_windows_web_source(
        &self,
        policy: ResolvedWindowCsp,
    ) -> Result<Arc<AdmittedWebAppSource>, HostAppError> {
        self.recheck()?;
        let selected = self.selected_source()?;
        let prefix = selected
            .root()
            .to_str()
            .ok_or_else(|| graph_error("resource root", "non UTF-8 selected directory"))?;
        let held = census_directory(&self.retained_root, prefix)?;
        let opened = file_metadata(&held).map_err(|source| graph_error("resource root", source))?;
        let named = path_metadata(self.root.join(selected.root()))
            .map_err(|source| graph_error("resource root", source))?;
        if !opened.is_dir() || !same_identity(&opened, &named) {
            return Err(graph_error(
                "resource root",
                "selected directory changed or was substituted",
            ));
        }

        let mut assets = Vec::new();
        for row in &self.graph.files {
            let Some(relative) = row
                .path
                .strip_prefix(prefix)
                .and_then(|rest| rest.strip_prefix('/'))
            else {
                continue;
            };
            assets.push(VerifiedWebAsset {
                path: relative_path(relative)?,
                size: row.size,
                sha256: parse_sha(&row.sha256)?,
            });
        }
        AdmittedWebAppSource::from_admitted_windows_graph(
            held,
            self.graph_sha256(),
            assets,
            selected.entry(),
            policy,
        )
        .map_err(|detail| graph_error("Windows resource source", detail))
    }
}
