//! GH459/GH473: the one explicitly selected Legacy original source boundary.
//!
//! The parent Host owns the retained GH499 graph and the GH552 resolved
//! window-specific CSP. The app supplies only the original file URL as input;
//! it cannot choose a source root, WK document stamp or a new grant.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use keld_guard::{ProfileState, ResolvedWindowCsp};
use keld_wv::OriginalEntryQuery;
use keld_wv::wkwebview::{
    AdmittedWebAppSource, AppPreloadBudget, AppPreloadImage, VerifiedWebAsset,
};

use super::graph_contract::AdmittedStageGraph;
use super::{HostAppError, app_detail};

const ORIGINAL_WEBAPP_RELATIVE_INDEX: &str = "drawio/src/main/webapp/index.html";
const ORIGINAL_SCHEME: &str = "keld-resource";

/// Convert the exact physical file:// URL from approved original main code
/// into harmless query data. The actual native URL comes from the retained
/// source owner and must still be loaded by its GH542 checked `WKNavigation`.
///
/// A malicious caller cannot choose another file, stage, renderer URL, query
/// fragment or Host principal through this adapter.
///
/// # Errors
/// Refuses every URI except the selected index in the retained graph's root.
pub(super) fn qualify_original_index_uri(
    selected_stage: &Path,
    original_uri: &str,
) -> Result<OriginalEntryQuery, HostAppError> {
    let error = || {
        app_detail(
            "original main file URL",
            "expected one qualified file:// index.html URL under the selected stage",
        )
    };
    let rest = original_uri.strip_prefix("file://").ok_or_else(error)?;
    let (encoded_path, query) = rest.split_once('?').unwrap_or((rest, ""));
    if !encoded_path.starts_with('/')
        || encoded_path.starts_with("//")
        || encoded_path.contains('#')
        || query.contains('#')
        || encoded_path.len() > 4096
    {
        return Err(error());
    }
    let bytes = encoded_path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let value = if bytes[i] == b'%' {
            let first = *bytes.get(i + 1).ok_or_else(error)?;
            let second = *bytes.get(i + 2).ok_or_else(error)?;
            let hex = |ch: u8| match ch {
                b'0'..=b'9' => Some(ch - b'0'),
                b'a'..=b'f' => Some(ch - b'a' + 10),
                b'A'..=b'F' => Some(ch - b'A' + 10),
                _ => None,
            };
            i += 3;
            hex(first)
                .zip(hex(second))
                .map(|(high, low)| (high << 4) | low)
                .ok_or_else(error)?
        } else {
            let literal = bytes[i];
            i += 1;
            literal
        };
        if value == 0 || value.is_ascii_control() {
            return Err(error());
        }
        decoded.push(value);
    }
    let selected = selected_stage.join(ORIGINAL_WEBAPP_RELATIVE_INDEX);
    if decoded != selected.as_os_str().as_encoded_bytes() {
        return Err(error());
    }
    OriginalEntryQuery::new(query).map_err(|message| app_detail("original query", message))
}

/// One successfully prepared source before Guardian launch and runtime capture.
/// Its private fields bind the same graph root, immutable assets, source policy
/// and Host budget. No runtime versions, page facts or readiness boolean can
/// construct this value or claim that a native window has loaded.
pub(super) struct PreparedOriginalSource {
    selected_stage: PathBuf,
    graph: Arc<AdmittedStageGraph>,
    preload_budget: AppPreloadBudget,
    source: Arc<AdmittedWebAppSource>,
}

impl PreparedOriginalSource {
    /// Prepare once before Guardian from the graph owner's retained pair.
    /// Runtime versions are attached only after the authenticated Bound result.
    ///
    /// # Errors
    /// Refuses Strict, missing graph/assets/policy or failed source capture.
    pub(super) fn prepare(
        graph: Arc<AdmittedStageGraph>,
        admitted_profile: ProfileState,
        policy: ResolvedWindowCsp,
        preload_budget: AppPreloadBudget,
    ) -> Result<Self, HostAppError> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        if admitted_profile != ProfileState::Legacy {
            return Err(app_detail(
                "original resource profile",
                "original editor requires explicitly selected Legacy admission",
            ));
        }
        // One existing graph operation rechecks once and returns descriptor,
        // immutable asset census and its own selected root path together.
        let (root, assets, selected_stage) = graph.verified_webapp_source()?.into_parts();
        let assets = assets
            .into_iter()
            .map(|asset| VerifiedWebAsset {
                path: asset.path,
                size: asset.size,
                sha256: asset.sha256,
            })
            .collect();
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce)
            .map_err(|_| app_detail("original source authority", "OS entropy unavailable"))?;
        let mut authority = String::with_capacity(32);
        for byte in nonce {
            authority.push(char::from(HEX[usize::from(byte >> 4)]));
            authority.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        let source = AdmittedWebAppSource::from_admitted_graph(
            root,
            graph.graph_sha256(),
            assets,
            policy,
            ORIGINAL_SCHEME.to_owned(),
            authority,
        )
        .map_err(|detail| app_detail("original source admission", detail))?;
        Ok(Self {
            selected_stage,
            graph,
            preload_budget,
            source,
        })
    }
    /// Consume the exact prepared object after authenticated runtime capture.
    /// This transition moves its source and graph; it performs no I/O or scan.
    pub(super) fn with_runtime_versions(
        self,
        versions: std::collections::BTreeMap<String, String>,
    ) -> OriginalLoadSelection {
        OriginalLoadSelection {
            prepared: self,
            versions,
        }
    }
    pub(super) fn selected_stage(&self) -> &Path {
        &self.selected_stage
    }
    pub(super) fn graph_sha256(&self) -> [u8; 32] {
        self.graph.graph_sha256()
    }
    pub(super) fn source(&self) -> &Arc<AdmittedWebAppSource> {
        &self.source
    }
    pub(super) fn window_key(&self) -> &str {
        self.source.window_key()
    }
    pub(super) fn policy_sha256(&self) -> [u8; 32] {
        self.source.verified_manifest_sha256()
    }
}

/// The same prepared source after actual runtime versions were captured.
/// Image configuration uses those immutable versions; source admission does not.
pub(super) struct OriginalLoadSelection {
    prepared: PreparedOriginalSource,
    versions: std::collections::BTreeMap<String, String>,
}

impl OriginalLoadSelection {
    pub(super) fn prepare_preload(
        &self,
        absolute_selector: &str,
        additional_arguments: Vec<String>,
    ) -> Result<Arc<AppPreloadImage>, HostAppError> {
        self.prepared
            .graph
            .prepare_original_preload(
                Path::new(absolute_selector),
                additional_arguments,
                self.versions.clone(),
                &self.prepared.preload_budget,
            )
            .map(Arc::new)
    }
    pub(super) fn qualify_uri(&self, uri: &str) -> Result<OriginalEntryQuery, HostAppError> {
        qualify_original_index_uri(self.prepared.selected_stage(), uri)
    }
    pub(super) fn source(&self) -> &Arc<AdmittedWebAppSource> {
        self.prepared.source()
    }
    pub(super) fn runtime_versions(&self) -> &std::collections::BTreeMap<String, String> {
        &self.versions
    }
    pub(super) fn window_key(&self) -> &str {
        self.prepared.window_key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_original_main_path_only_preserves_app_query() {
        let stage = Path::new("/tmp/example stage");
        let good =
            "file:///tmp/example%20stage/drawio/src/main/webapp/index.html?appLang=en&x=%252f";
        assert_eq!(
            qualify_original_index_uri(stage, good)
                .expect("one selected original path")
                .as_str(),
            Ok("appLang=en&x=%252f"),
        );
        assert_eq!(
            qualify_original_index_uri(
                stage,
                "file:///tmp/example%20stage/drawio/src/main/webapp/index.html"
            )
            .expect("original path without query")
            .as_str(),
            Ok(""),
        );
    }

    #[test]
    fn other_file_scheme_directory_fragment_and_query_control_are_refused() {
        let stage = Path::new("/tmp/example stage");
        for uri in [
            "https://example.com/index.html",
            "file:///tmp/elsewhere/drawio/src/main/webapp/index.html",
            "file://localhost/tmp/example%20stage/drawio/src/main/webapp/index.html",
            "file:///tmp/example%20stage/drawio/src/main/webapp/../index.html",
            "file:///tmp/example%20stage/drawio/src/main/webapp/index.html#fragment",
            "file:///tmp/example%20stage/drawio/src/main/webapp/index.html?x#fragment",
            "file:///tmp/example%20stage/drawio/src/main/webapp/index.html%00",
            "file:///tmp/example%20stage/drawio/src/main/webapp/%69ndex.html%0a",
            "file:///tmp/example%20stage/drawio/src/main/webapp/missing.svg",
            "file:///tmp/example%20stage/drawio/src/main/webapp/index.html?x=hello\nworld",
        ] {
            assert!(qualify_original_index_uri(stage, uri).is_err(), "{uri}");
        }
    }
}
