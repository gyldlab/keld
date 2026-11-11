//! Windows retained-handle adapter for the shared verified source allocation.
//! It authenticates data and selected-entry URLs; it cannot credit a native load.

use super::{
    AdmittedWebAppSource, ResourceSourceError, SelectedRead, VerifiedBytes,
    VerifiedResourceResponsePolicy, VerifiedWebAsset, admit_asset_census, mime_for,
    read_verified_file, valid_response_policy,
};
use crate::native_document::VerifiedSourceId;
use crate::resource_intake::ORIGINAL_WEBAPP_LIMITS;
use keld_guard::{
    AdmittedRootUrl, ParsedFileUrl, RootRelativeComponents, file_components_equal,
    file_components_match_graph_path, file_url_root_relative,
};
use keld_update::{
    WindowsGraphChildKind, graph_locator_metadata, open_retained_graph_child,
    retained_graph_dos_path, retained_graph_metadata,
};
use std::fs::File;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

impl AdmittedWebAppSource {
    /// Consume Core's retained selected directory, immutable graph inventory and
    /// opaque Guard policy after its complete graph and Legacy-profile checks.
    ///
    /// The DOS locator is observed from the retained handle, never supplied by
    /// app code. The caller must be the authenticated graph owner; this method
    /// validates the handoff and cannot authenticate a self-selected graph hash.
    ///
    /// # Errors
    /// Refuses an invalid retained root, policy, namespace or selected entry.
    pub fn from_admitted_windows_graph(
        webapp_root: File,
        verified_graph_sha256: [u8; 32],
        assets: Vec<VerifiedWebAsset>,
        selected_entry_relative_to_code_root: &Path,
        policy: VerifiedResourceResponsePolicy,
    ) -> Result<Arc<Self>, &'static str> {
        if !valid_response_policy(&policy) {
            return Err("verified source response policy is unavailable");
        }
        let held = retained_graph_metadata(&webapp_root)
            .map_err(|_| "cannot inspect retained resource root")?;
        if !held.is_dir() {
            return Err("retained resource root is not a directory");
        }
        let root_path = retained_graph_dos_path(&webapp_root)
            .map_err(|_| "retained resource root has no normalized local DOS locator")?;
        let named = graph_locator_metadata(&root_path)
            .map_err(|_| "cannot corroborate retained resource root locator")?;
        if !named.is_dir() || (named.dev(), named.ino()) != (held.dev(), held.ino()) {
            return Err("resource root locator no longer names the retained directory");
        }
        let root_url = AdmittedRootUrl::from_normalized_drive_path(&root_path)
            .map_err(|_| "invalid normalized resource root locator")?;
        let (assets, entry) =
            admit_asset_census(assets, selected_entry_relative_to_code_root, |path| {
                keld_guard::validate_windows_graph_namespace(&[path])
                    .map_err(|_| ResourceSourceError::InvalidPath)
            })?;
        let names = assets.keys().map(String::as_str).collect::<Vec<_>>();
        keld_guard::validate_windows_graph_namespace(&names)
            .map_err(|_| "ambiguous Windows resource namespace")?;
        let entry_components = RootRelativeComponents::from_graph_path(&entry)
            .map_err(|_| "selected resource entry is not a serviceable Windows file URL")?;
        Ok(Arc::new(Self {
            webapp_root,
            root_identity: (held.dev(), held.ino()),
            root_path: Arc::new(root_path),
            root_url: Arc::new(root_url),
            entry_components: Arc::new(entry_components),
            source: VerifiedSourceId(verified_graph_sha256),
            scheme: String::from("file"),
            entry,
            policy,
            assets: Arc::new(assets),
            _window_policy_credit: None,
            retained_bytes: Arc::new(AtomicUsize::new(0)),
        }))
    }

    /// Bind another trusted window key from the same verified manifest while
    /// retaining this exact resource census, root custody and shared byte account.
    /// The returned Arc remains a DISTINCT document/IPC source binding.
    ///
    /// # Errors
    /// Refuses another manifest, changed custody, invalid policy or exhausted
    /// shared metadata credit before allocating the source alias.
    pub fn with_windows_window_policy(
        &self,
        policy: VerifiedResourceResponsePolicy,
    ) -> Result<Arc<Self>, &'static str> {
        if !valid_response_policy(&policy)
            || policy.verified_sha256() != self.verified_manifest_sha256()
        {
            return Err("window policy does not belong to the admitted source manifest");
        }
        self.recheck_root()
            .map_err(|_| "retained source custody changed")?;
        let charge = std::mem::size_of::<Self>()
            .checked_add(self.entry.len())
            .and_then(|n| n.checked_add(self.scheme.len()))
            .and_then(|n| n.checked_add(policy.window_key().len()))
            .and_then(|n| n.checked_add(256))
            .ok_or("source policy alias metadata overflow")?;
        let credit =
            crate::resource_intake::RetainedByteCredit::reserve(&self.retained_bytes, charge)
                .map_err(|_| "source policy alias metadata budget exhausted")?;
        let root = self
            .webapp_root
            .try_clone()
            .map_err(|_| "cannot duplicate retained source custody")?;
        Ok(Arc::new(Self {
            webapp_root: root,
            root_identity: self.root_identity,
            root_path: Arc::clone(&self.root_path),
            root_url: Arc::clone(&self.root_url),
            entry_components: Arc::clone(&self.entry_components),
            source: self.source,
            scheme: self.scheme.clone(),
            entry: self.entry.clone(),
            policy,
            assets: Arc::clone(&self.assets),
            _window_policy_credit: Some(credit),
            retained_bytes: Arc::clone(&self.retained_bytes),
        }))
    }

    /// Resource-session admission only. Never use this predicate in place of
    /// the exact source Arc identity required by document and IPC leases.
    pub(crate) fn shares_windows_resource_custody(&self, other: &Self) -> bool {
        self.source == other.source
            && self.root_identity == other.root_identity
            && self.entry == other.entry
            && self.verified_manifest_sha256() == other.verified_manifest_sha256()
            && Arc::ptr_eq(&self.assets, &other.assets)
            && Arc::ptr_eq(&self.retained_bytes, &other.retained_bytes)
    }

    /// Qualify the original primary's file URL through the same parser used by
    /// native requests. Only the selected entry can produce this bounded query.
    ///
    /// # Errors
    /// Refuses another source/document, malformed URI or over-budget query.
    pub fn qualify_original_entry_uri(
        &self,
        uri: &str,
    ) -> Result<crate::OriginalEntryQuery, &'static str> {
        let parsed = self
            .qualify_document_url(uri)
            .map_err(|_| "expected the exact selected file entry in the retained source")?;
        crate::OriginalEntryQuery::from_optional_query(parsed.raw_query.as_deref())
    }

    pub(crate) fn response_policy(&self) -> &str {
        self.policy.response_header()
    }

    /// The caller reserves its native attempt metadata before this bounded
    /// construction. Query omission and an explicit empty query stay distinct.
    pub(crate) fn admitted_entry_url(&self, raw_query: Option<&str>) -> Option<String> {
        crate::OriginalEntryQuery::from_optional_query(raw_query).ok()?;
        let url = self
            .root_url
            .url_for(&self.entry_components, raw_query)
            .ok()?;
        (url.encode_utf16().count() <= ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units).then_some(url)
    }

    fn parse_url(&self, actual_url: &str) -> Result<ParsedFileUrl, ResourceSourceError> {
        if actual_url.encode_utf16().count() > ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units {
            return Err(ResourceSourceError::TooLarge);
        }
        file_url_root_relative(actual_url, &self.root_url).map_err(|error| match error {
            keld_guard::FileUrlRefusal::OutsideRoot => ResourceSourceError::WrongAuthority,
            keld_guard::FileUrlRefusal::TooLong => ResourceSourceError::TooLarge,
            _ => ResourceSourceError::InvalidUrl,
        })
    }

    pub(crate) fn qualify_document_url(
        &self,
        actual_url: &str,
    ) -> Result<ParsedFileUrl, ResourceSourceError> {
        let parsed = self.parse_url(actual_url)?;
        if !file_components_equal(&parsed.components, &self.entry_components)
            .map_err(|_| ResourceSourceError::InvalidPath)?
        {
            return Err(ResourceSourceError::InvalidEntry);
        }
        self.select_asset(&parsed.components)?;
        Ok(parsed)
    }

    pub(crate) fn qualify_url(
        &self,
        actual_url: &str,
    ) -> Result<SelectedRead, ResourceSourceError> {
        let parsed = self.parse_url(actual_url)?;
        self.select_asset(&parsed.components)
    }

    fn select_asset(
        &self,
        components: &RootRelativeComponents,
    ) -> Result<SelectedRead, ResourceSourceError> {
        let requested = components.to_wire_path();
        if let Some(asset) = self.assets.get(&requested) {
            return Ok(SelectedRead {
                path: requested,
                size: asset.size,
                sha256: asset.sha256,
                mime: mime_for(&asset.path),
            });
        }
        // The common map retains canonical graph spelling. An uncommon case
        // variant uses the same fallible Windows ordinal owner, never lowercase
        // or an ambient filesystem lookup. The admitted census bounds this scan.
        for (path, asset) in self.assets.iter() {
            if file_components_match_graph_path(components, path)
                .map_err(|_| ResourceSourceError::InvalidPath)?
            {
                return Ok(SelectedRead {
                    path: path.clone(),
                    size: asset.size,
                    sha256: asset.sha256,
                    mime: mime_for(&asset.path),
                });
            }
        }
        Err(ResourceSourceError::AbsentAsset)
    }

    fn recheck_root(&self) -> Result<(), ResourceSourceError> {
        let held = retained_graph_metadata(&self.webapp_root)
            .map_err(|_| ResourceSourceError::SourceChanged)?;
        let named = graph_locator_metadata(&self.root_path)
            .map_err(|_| ResourceSourceError::SourceChanged)?;
        if !held.is_dir()
            || !named.is_dir()
            || (held.dev(), held.ino()) != self.root_identity
            || (named.dev(), named.ino()) != self.root_identity
        {
            return Err(ResourceSourceError::SourceChanged);
        }
        Ok(())
    }

    /// Run on the source's bounded I/O worker after its task/byte reservation.
    /// Every expected value is reselected from the immutable admitted inventory.
    /// This result authenticates bytes; native attempt attribution is separate.
    pub(crate) fn verified_read(
        &self,
        request: &SelectedRead,
        max_asset_bytes: usize,
        cancelled: &AtomicBool,
    ) -> Result<VerifiedBytes, ResourceSourceError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(ResourceSourceError::Cancelled);
        }
        let asset = self
            .assets
            .get(&request.path)
            .ok_or(ResourceSourceError::AbsentAsset)?;
        if request.size != asset.size
            || request.sha256 != asset.sha256
            || request.mime != mime_for(&asset.path)
        {
            return Err(ResourceSourceError::SourceChanged);
        }
        let ceiling = max_asset_bytes.min(ORIGINAL_WEBAPP_LIMITS.max_asset_bytes);
        if request.size > ceiling as u64 {
            return Err(ResourceSourceError::TooLarge);
        }
        self.recheck_root()?;
        let parts = request.path.split('/').collect::<Vec<_>>();
        let (leaf, parents) = parts.split_last().ok_or(ResourceSourceError::InvalidPath)?;
        let mut directories = Vec::with_capacity(parents.len());
        for component in parents {
            if cancelled.load(Ordering::Acquire) {
                return Err(ResourceSourceError::Cancelled);
            }
            let parent = directories.last().unwrap_or(&self.webapp_root);
            directories.push(
                open_retained_graph_child(parent, component, WindowsGraphChildKind::Directory)
                    .map_err(|_| ResourceSourceError::EscapingFile)?,
            );
        }
        let parent = directories.last().unwrap_or(&self.webapp_root);
        let file = open_retained_graph_child(parent, leaf, WindowsGraphChildKind::RegularFile)
            .map_err(|_| ResourceSourceError::EscapingFile)?;
        let before = retained_graph_metadata(&file).map_err(|_| ResourceSourceError::Io)?;
        if !before.is_file() {
            return Err(ResourceSourceError::FileNotRegular);
        }
        if before.dev() != self.root_identity.0 || before.len() != request.size {
            return Err(ResourceSourceError::SourceChanged);
        }
        let checked_size =
            usize::try_from(before.len()).map_err(|_| ResourceSourceError::TooLarge)?;
        let retained = file.try_clone().map_err(|_| ResourceSourceError::Io)?;
        let result = read_verified_file(file, request, checked_size, ceiling, cancelled)?;
        let after = retained_graph_metadata(&retained).map_err(|_| ResourceSourceError::Io)?;
        if before != after {
            return Err(ResourceSourceError::SourceChanged);
        }
        self.recheck_root()?;
        if cancelled.load(Ordering::Acquire) {
            return Err(ResourceSourceError::Cancelled);
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "resource_source_windows_tests.rs"]
pub(crate) mod tests;
