//! Retained resource inventory and window policy for a graph-selected source.
//! The Windows reader uses the directory handle held by this allocation.

use crate::native_document::VerifiedSourceId;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicUsize};

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
    MissingFile,
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
    /// Shared across windows and live until all retained resource results release their credit.
    pub(crate) retained_bytes: Arc<AtomicUsize>,
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

    pub(crate) fn retained_credit_account(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.retained_bytes)
    }
}

/// Immutable actual native URL or source-qualified native origin metadata.
/// Explicit aliases share the allocation and retain source-wide byte credit.
pub(crate) struct NativeNavigationUrl {
    allocation: Arc<NavigationUrlAllocation>,
    _alias_credit: Option<crate::resource_intake::RetainedByteCredit>,
}

struct NavigationUrlAllocation {
    text: String,
    credit: crate::resource_intake::RetainedByteCredit,
}

impl NativeNavigationUrl {
    /// Borrow the qualified native metadata without allocating or releasing credit.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.allocation.text
    }

    /// Observe whether two credited carriers retain the same URL allocation.
    /// This is storage identity only: it does not establish source authority,
    /// native document liveness, or permission to publish for a current role.
    #[must_use]
    pub fn same_allocation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation)
    }

    /// Observe the existing URL allocation's actual source-account charge.
    /// This is storage custody only, never source/document/route authority.
    #[cfg(windows)]
    pub(crate) fn charged_to(&self, account: &Arc<AtomicUsize>) -> bool {
        self.allocation.credit.covers(account, 1)
    }

    /// Admit another shared URL alias against the same source budget.
    /// The extra queue/event slot stays charged until this alias is dropped.
    ///
    /// # Errors
    /// Refuses a full shared retained-byte account before creating an alias.
    pub fn try_clone(&self) -> Result<Self, crate::engine::NativeNavigationRefusal> {
        let credit = self
            .allocation
            .credit
            .reserve_more(navigation_url_slot_charge())
            .map_err(|_| crate::engine::NativeNavigationRefusal::RetainedByteLimit)?;
        Ok(Self {
            allocation: Arc::clone(&self.allocation),
            _alias_credit: Some(credit),
        })
    }

    // Native URL strings and qualified native-origin tuples share one bounded
    // formatting/reservation owner. The caller supplies native UTF16 units.
    pub(crate) fn capture_bounded(
        actual: impl std::fmt::Display,
        units: usize,
        account: &Arc<AtomicUsize>,
    ) -> Result<Self, crate::resource_intake::IntakeReject> {
        use crate::resource_intake::RetainedByteCredit;
        use crate::resource_intake::{IntakeReject, ORIGINAL_WEBAPP_LIMITS, worst_case_charge};
        use std::fmt::Write as _;
        if units == 0 || units > ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units {
            return Err(IntakeReject::MetadataLimit);
        }
        let buffer = units
            .checked_mul(4)
            .ok_or(IntakeReject::RetainedByteLimit)?;
        let copies = units
            .checked_mul(2)
            .ok_or(IntakeReject::RetainedByteLimit)?;
        let overhead = std::mem::size_of::<NavigationUrlAllocation>()
            .checked_add(4 * std::mem::size_of::<usize>())
            .and_then(|bytes| bytes.checked_add(navigation_url_slot_charge()))
            .ok_or(IntakeReject::RetainedByteLimit)?;
        let charged = worst_case_charge(0, copies, overhead, ORIGINAL_WEBAPP_LIMITS)?;
        // Existing source-wide account, reserved before any owned UTF8 copy.
        let credit = RetainedByteCredit::reserve(account, charged)?;
        let mut text = String::new();
        text.try_reserve_exact(buffer)
            .map_err(|_| IntakeReject::RetainedByteLimit)?;
        let mut writer = BoundedNavigationUrl {
            text,
            limit: buffer,
        };
        write!(&mut writer, "{actual}").map_err(|_| IntakeReject::MetadataLimit)?;
        Ok(Self {
            allocation: Arc::new(NavigationUrlAllocation {
                text: writer.text,
                credit,
            }),
            _alias_credit: None,
        })
    }
}

fn navigation_url_slot_charge() -> usize {
    std::mem::size_of::<crate::engine::AppWindowEvent>() + 4 * std::mem::size_of::<usize>()
}

// The precharged buffer may never grow through a formatting fallback.
struct BoundedNavigationUrl {
    text: String,
    limit: usize,
}
impl std::fmt::Write for BoundedNavigationUrl {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        if value.len() > self.limit.saturating_sub(self.text.len()) {
            return Err(std::fmt::Error);
        }
        self.text.push_str(value);
        Ok(())
    }
}
impl std::fmt::Debug for NativeNavigationUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NativeNavigationUrl")
            .field(&self.as_str())
            .finish()
    }
}
impl PartialEq for NativeNavigationUrl {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for NativeNavigationUrl {}

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
