//! Retained resource inventory and window policy for a graph-selected source.
//! The Windows reader uses the directory handle held by this allocation.

use crate::native_document::VerifiedSourceId;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

#[path = "resource_source_io.rs"]
mod io;
pub(crate) use io::{mime_for, read_verified_file};
#[path = "resource_source_windows.rs"]
pub(crate) mod windows;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResourceSourceError {
    InvalidEntry,
    InvalidUrl,
    WrongAuthority,
    InvalidPath,
    AbsentAsset,
    FileNotRegular,
    EscapingFile,
    SourceChanged,
    TooLarge,
    Cancelled,
    Io,
}

#[derive(Clone, Debug)]
pub(crate) struct SelectedRead {
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) sha256: [u8; 32],
    pub(crate) mime: &'static str,
}

pub(crate) struct VerifiedBytes {
    pub(crate) bytes: Vec<u8>,
    pub(crate) mime: &'static str,
}

/// Consumed from the ONE GH499 validated retained resource subgraph. These
/// values are not read from JavaScript or a renderer request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedWebAsset {
    /// Relative to the retained graph-selected code-root directory.
    pub path: PathBuf,
    /// Exact captured file length authenticated by the selected graph.
    pub size: u64,
    /// Exact captured content digest; reads must still authenticate actual bytes.
    pub sha256: [u8; 32],
}

/// The sole opaque Guard-owned digest-verified window response policy.
/// No raw header or unverified manifest can construct this source input.
pub use keld_guard::ResolvedWindowCsp as VerifiedResourceResponsePolicy;

/// A capability that only the Core authenticated graph/preflight owner may
/// construct and pass to the selected Legacy window.
/// The scheme/authority are locators, NOT webview/IPC/doc credentials.
pub struct AdmittedWebAppSource {
    pub(crate) webapp_root: std::fs::File,
    pub(crate) root_identity: (u64, u64),
    pub(crate) root_path: Arc<PathBuf>,
    pub(crate) root_url: Arc<keld_guard::AdmittedRootUrl>,
    pub(crate) entry_components: Arc<keld_guard::RootRelativeComponents>,
    pub(crate) source: VerifiedSourceId,
    pub(crate) scheme: String,
    pub(crate) entry: String,
    pub(crate) policy: VerifiedResourceResponsePolicy,
    pub(crate) assets: Arc<BTreeMap<String, VerifiedWebAsset>>,
}

impl std::fmt::Debug for AdmittedWebAppSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmittedWebAppSource")
            .field("source", &"<opaque>")
            .field("scheme", &self.scheme)
            .field("entry", &self.entry)
            .finish_non_exhaustive()
    }
}

impl AdmittedWebAppSource {
    pub(crate) const fn source_id(&self) -> VerifiedSourceId {
        self.source
    }

    /// Borrow the exact immutable Guard resolution retained by this source.
    #[must_use]
    pub const fn resolved_window_policy(&self) -> &keld_guard::ResolvedWindowCsp {
        &self.policy
    }

    /// Actual trusted manifest key bound to this source's immutable policy.
    #[must_use]
    pub fn window_key(&self) -> &str {
        self.policy.window_key()
    }

    /// Digest verified over the exact permissions bytes by the sole Guard owner.
    #[must_use]
    pub const fn verified_manifest_sha256(&self) -> [u8; 32] {
        self.policy.verified_sha256()
    }
}

pub(crate) fn valid_response_policy(policy: &VerifiedResourceResponsePolicy) -> bool {
    !policy.response_header().trim().is_empty()
        && policy.response_header().len() <= 4096
        && !policy.response_header().contains(['\r', '\n'])
}

pub(crate) fn admit_asset_census(
    assets: Vec<VerifiedWebAsset>,
    selected_entry_relative_to_code_root: &std::path::Path,
    validate_path: impl Fn(&str) -> Result<(), ResourceSourceError>,
) -> Result<(BTreeMap<String, VerifiedWebAsset>, String), &'static str> {
    if assets.is_empty() || assets.len() > 32_768 {
        return Err("incomplete retained webapp source");
    }
    let mut selected = BTreeMap::new();
    for asset in assets {
        let path = asset.path.to_str().ok_or("non-UTF-8 captured asset path")?;
        if path.len() > 1024 || path.is_empty() || selected.contains_key(path) {
            return Err("ambiguous captured asset path");
        }
        validate_path(path).map_err(|_| "invalid captured asset path")?;
        selected.insert(path.to_owned(), asset);
    }
    let entry = selected_entry_relative_to_code_root
        .to_str()
        .ok_or("selected document entry is not UTF-8")?;
    validate_path(entry).map_err(|_| "selected document entry is not canonical")?;
    if !selected.contains_key(entry) {
        return Err("selected document entry is absent from the verified asset graph");
    }
    Ok((selected, entry.to_owned()))
}
