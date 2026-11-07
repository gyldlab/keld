//! GH473: a read-only, Host-admitted webapp source from GH499's captured graph.
//!
//! The graph owner supplies the retained webapp directory FD and exact relative
//! entries after pre-evaluation admission; this module neither parses nor
//! validates the installed graph again. Each read verifies bytes through the
//! SAME opened nofollow descriptor before releasing a complete response.
//! Source access is independent of the Bun role's FS broker and restart.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use nix::fcntl::{OFlag, openat};
use nix::sys::stat::Mode;
use sha2::{Digest as _, Sha256};

use super::macos_document::VerifiedSourceId;

/// Consumed from the ONE GH499 validated retained resource subgraph. These
/// values are not read from JavaScript or a renderer request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedWebAsset {
    /// Relative to the retained graph-selected code-root directory FD.
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
/// construct and pass to the macOS engine's first disposable Legacy window.
/// The scheme/authority are locators, NOT webview/IPC/doc credentials.
pub struct AdmittedWebAppSource {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    lifecycle_gate: std::sync::Mutex<Option<super::s1_resource_lifecycle::ReadGate>>,
    webapp_root: OwnedFd,
    root_device: u64,
    source: VerifiedSourceId,
    scheme: String,
    authority: String,
    entry: String,
    policy: VerifiedResourceResponsePolicy,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fixture_response_override: std::sync::OnceLock<String>,
    assets: BTreeMap<String, VerifiedWebAsset>,
    /// Resource results already queued into Tao can outlive one native View.
    /// One owner-wide account prevents two successive windows from each
    /// admitting a full 256 MiB while old pending results are retained.
    retained_bytes: Arc<AtomicUsize>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResourceSourceError {
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
pub(super) struct SelectedRead {
    pub(super) path: String,
    pub(super) size: u64,
    pub(super) sha256: [u8; 32],
    pub(super) mime: &'static str,
}

pub(super) struct VerifiedBytes {
    pub(super) bytes: Vec<u8>,
    pub(super) mime: &'static str,
}

/// Immutable actual native URL or source-qualified native origin metadata.
/// Explicit aliases share the allocation and retain source-wide byte credit.
pub struct MacNavigationUrl {
    allocation: Arc<NavigationUrlAllocation>,
    _alias_credit: Option<super::macos_resource_native::RetainedByteCredit>,
}

struct NavigationUrlAllocation {
    text: String,
    credit: super::macos_resource_native::RetainedByteCredit,
}

impl MacNavigationUrl {
    /// Modeled callback metadata for the debug-only lease-owner component fixture.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn fixture_capture(
        text: &str,
        account: &Arc<AtomicUsize>,
    ) -> Result<Self, super::macos_resource_intake::IntakeReject> {
        Self::capture_bounded(text, text.encode_utf16().count(), account)
    }
    /// Borrow the qualified native metadata without allocating or releasing credit.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.allocation.text
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

    pub(super) fn capture(
        actual: &objc2_foundation::NSString,
        account: &Arc<AtomicUsize>,
    ) -> Result<Self, super::macos_resource_intake::IntakeReject> {
        Self::capture_bounded(actual, actual.length(), account)
    }

    // Native URL strings and qualified native-origin tuples share one bounded
    // formatting/reservation owner. The caller supplies native UTF16 units.
    fn capture_bounded(
        actual: impl std::fmt::Display,
        units: usize,
        account: &Arc<AtomicUsize>,
    ) -> Result<Self, super::macos_resource_intake::IntakeReject> {
        use super::macos_resource_intake::{
            IntakeReject, ORIGINAL_WEBAPP_LIMITS, worst_case_charge,
        };
        use super::macos_resource_native::RetainedByteCredit;
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
impl std::fmt::Debug for MacNavigationUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MacNavigationUrl")
            .field(&self.as_str())
            .finish()
    }
}
impl PartialEq for MacNavigationUrl {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for MacNavigationUrl {}

impl AdmittedWebAppSource {
    /// Core calls only after the immutable GH499 graph, Host-selected
    /// disposable Legacy profile and GH552 CSP resolve successfully.
    /// The retained directory is the SAME graph-selected code root. The
    /// entry is an independently admitted path relative to that descriptor.
    ///
    /// # Errors
    /// Refuses absent CSP, malformed GH471 fixed scheme/authority or an
    /// incomplete/ambiguous source selection before any native webview load.
    pub fn from_admitted_graph(
        webapp_root: OwnedFd,
        verified_graph_sha256: [u8; 32],
        assets: Vec<VerifiedWebAsset>,
        selected_entry_relative_to_code_root: &Path,
        policy: VerifiedResourceResponsePolicy,
        fixed_scheme: String,
        host_authority: String,
    ) -> Result<Arc<Self>, &'static str> {
        if policy.response_header().trim().is_empty()
            || policy.response_header().len() > 4096
            || policy.response_header().contains('\r')
            || policy.response_header().contains('\n')
            || !valid_scheme(&fixed_scheme)
            || host_authority.len() != 32
            || !host_authority
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err("verified source/policy/scheme admission missing");
        }
        let metadata = File::from(
            nix::unistd::dup(webapp_root.as_fd())
                .map_err(|_| "cannot inspect retained webapp root")?,
        )
        .metadata()
        .map_err(|_| "cannot stat retained webapp root")?;
        if !metadata.is_dir() || assets.is_empty() || assets.len() > 32768 {
            return Err("incomplete retained webapp source");
        }
        let mut selected = BTreeMap::new();
        for asset in assets {
            let path = asset.path.to_str().ok_or("non-UTF-8 captured asset path")?;
            if path.len() > 1024 || path.is_empty() || selected.contains_key(path) {
                return Err("ambiguous captured asset path");
            }
            check_relative_asset(path).map_err(|_| "invalid captured asset path")?;
            if asset.size > (32 * 1024 * 1024) {
                return Err("captured source exceeds original asset ceiling");
            }
            selected.insert(path.to_owned(), asset);
        }
        let entry = selected_entry_relative_to_code_root
            .to_str()
            .ok_or("selected document entry is not UTF-8")?;
        check_relative_asset(entry).map_err(|_| "selected document entry is not canonical")?;
        if !selected.contains_key(entry) {
            return Err("selected document entry is absent from the verified asset graph");
        }
        Ok(Arc::new(Self {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            lifecycle_gate: std::sync::Mutex::new(None),
            webapp_root,
            root_device: metadata.dev(),
            source: VerifiedSourceId(verified_graph_sha256),
            scheme: fixed_scheme,
            authority: host_authority,
            entry: entry.to_owned(),
            policy,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            fixture_response_override: std::sync::OnceLock::new(),
            assets: selected,
            retained_bytes: Arc::new(AtomicUsize::new(0)),
        }))
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn arm_lifecycle_gate(
        &self,
        gate: super::s1_resource_lifecycle::ReadGate,
    ) -> Result<(), &'static str> {
        let mut slot = self
            .lifecycle_gate
            .lock()
            .map_err(|_| "lifecycle gate poisoned")?;
        if slot.is_some() {
            return Err("lifecycle gate already armed");
        }
        *slot = Some(gate);
        Ok(())
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn lifecycle_root_fd(&self) -> i32 {
        use std::os::fd::AsRawFd as _;
        self.webapp_root.as_raw_fd()
    }

    pub(super) const fn source_id(&self) -> VerifiedSourceId {
        self.source
    }

    pub(super) fn scheme(&self) -> &str {
        &self.scheme
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

    pub(super) fn response_policy(&self) -> &str {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(header) = self.fixture_response_override.get() {
            return header;
        }
        self.policy.response_header()
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn fixture_omit_wasm_token(&self) -> Result<(), &'static str> {
        self.fixture_response_override
            .set(
                self.policy
                    .response_header()
                    .replace(" 'wasm-unsafe-eval'", ""),
            )
            .map_err(|_| "fixed first-party negative header already selected")
    }

    pub(super) fn retained_credit_account(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.retained_bytes)
    }

    /// Only the admitted first main document may carry arbitrary original
    /// queryObj/appLang. The query does not change source/path/mint.
    pub(super) fn admitted_entry_url(&self, query: &str) -> Option<String> {
        if query.len() > 2048 || query.chars().any(|c| c == '#' || c.is_control()) {
            return None;
        }
        let base = format!("{}://{}/{}", self.scheme, self.authority, self.entry);
        if query.is_empty() {
            Some(base)
        } else {
            Some(format!("{base}?{query}"))
        }
    }

    /// Serialize only the actual native tuple qualified for this exact source.
    /// The inert WK probe corroborates keld-resource with this selected host and
    /// native port zero. Other custom/opaque tuple semantics remain unknown.
    /// This metadata grants no document identity or IPC authority.
    pub(super) fn qualify_native_origin(
        &self,
        protocol: &str,
        host: &str,
        port: i64,
    ) -> Result<MacNavigationUrl, ResourceSourceError> {
        if self.scheme != "keld-resource"
            || protocol != self.scheme
            || host != self.authority
            || port != 0
        {
            return Err(ResourceSourceError::InvalidUrl);
        }
        // Both exact matches are admitted ASCII. Format the actual native tuple,
        // never a URL, cached document or source fields substituted for input.
        let units = protocol
            .len()
            .checked_add(3)
            .and_then(|units| units.checked_add(host.len()))
            .ok_or(ResourceSourceError::TooLarge)?;
        MacNavigationUrl::capture_bounded(
            format_args!("{protocol}://{host}"),
            units,
            &self.retained_bytes,
        )
        .map_err(|_| ResourceSourceError::TooLarge)
    }

    /// Qualify the configured document entry using the one source URL parser.
    /// Native callers separately corroborate actual view/frame/document state.
    pub(super) fn qualify_document_url(&self, actual_url: &str) -> Result<(), ResourceSourceError> {
        let selected = self.qualify_url(actual_url)?;
        if selected.path != self.entry {
            return Err(ResourceSourceError::InvalidEntry);
        }
        Ok(())
    }

    /// This requires matching the ACTUAL browser URL plus its receiving native
    /// `WKWebView` and GH542 attempt elsewhere; URI alone never proves identity.
    pub(super) fn qualify_url(
        &self,
        actual_url: &str,
    ) -> Result<SelectedRead, ResourceSourceError> {
        let prefix = format!("{}://{}/", self.scheme, self.authority);
        let Some(rest) = actual_url.strip_prefix(&prefix) else {
            return Err(ResourceSourceError::WrongAuthority);
        };
        if actual_url.chars().any(char::is_control) {
            return Err(ResourceSourceError::InvalidUrl);
        }
        // A fragment is never sent as a network resource selector. Refusing a
        // malformed or caller-fabricated fragment prevents alternate lookup.
        if rest.contains('#') {
            return Err(ResourceSourceError::InvalidUrl);
        }
        let file_part = rest.split_once('?').map_or(rest, |(file, _)| file);
        let decoded = decode_path_once(file_part)?;
        let asset = self
            .assets
            .get(&decoded)
            .ok_or(ResourceSourceError::AbsentAsset)?;
        Ok(SelectedRead {
            path: decoded,
            size: asset.size,
            sha256: asset.sha256,
            mime: mime_for(&asset.path),
        })
    }

    /// Must run off the native UI thread, on the one source I/O worker.
    /// No filesystem path, symlink or stream is supplied by a renderer.
    pub(super) fn verified_read(
        &self,
        request: &SelectedRead,
        max_asset_bytes: usize,
        cancelled: &AtomicBool,
    ) -> Result<VerifiedBytes, ResourceSourceError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(ResourceSourceError::Cancelled);
        }
        if request.size > max_asset_bytes as u64 {
            return Err(ResourceSourceError::TooLarge);
        }
        let mut parent: Option<OwnedFd> = None;
        let parts: Vec<_> = Path::new(&request.path).components().collect();
        let Some((leaf, dirs)) = parts.split_last() else {
            return Err(ResourceSourceError::InvalidPath);
        };
        for directory in dirs {
            if cancelled.load(Ordering::Acquire) {
                return Err(ResourceSourceError::Cancelled);
            }
            let Component::Normal(name) = directory else {
                return Err(ResourceSourceError::InvalidPath);
            };
            let dir = parent
                .as_ref()
                .map_or_else(|| self.webapp_root.as_fd(), AsFd::as_fd);
            parent = Some(
                openat(
                    dir,
                    *name,
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| ResourceSourceError::EscapingFile)?,
            );
        }
        let Component::Normal(name) = leaf else {
            return Err(ResourceSourceError::InvalidPath);
        };
        let anchor = parent
            .as_ref()
            .map_or_else(|| self.webapp_root.as_fd(), AsFd::as_fd);
        if cancelled.load(Ordering::Acquire) {
            return Err(ResourceSourceError::Cancelled);
        }
        let opened = openat(
            anchor,
            *name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| ResourceSourceError::EscapingFile)?;
        let file = File::from(opened);
        let meta = file.metadata().map_err(|_| ResourceSourceError::Io)?;
        if !meta.is_file() {
            return Err(ResourceSourceError::FileNotRegular);
        }
        if meta.dev() != self.root_device {
            return Err(ResourceSourceError::EscapingFile);
        }
        if meta.len() != request.size || meta.len() > max_asset_bytes as u64 {
            return Err(ResourceSourceError::SourceChanged);
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        {
            let gate = {
                let mut slot = self
                    .lifecycle_gate
                    .lock()
                    .map_err(|_| ResourceSourceError::Io)?;
                if slot.as_ref().is_some_and(|gate| gate.path == request.path) {
                    slot.take()
                } else {
                    None
                }
            };
            if let Some(gate) = gate {
                gate.hold(&file, self.lifecycle_root_fd())?;
            }
        }
        let checked_size =
            usize::try_from(meta.len()).map_err(|_| ResourceSourceError::TooLarge)?;
        read_verified_file(file, request, checked_size, max_asset_bytes, cancelled)
    }
}

// The worker's existing admitted result allocation is the read buffer. The
// one-byte stack probe detects growth without reserving any extra retained byte.
fn read_verified_file(
    mut file: File,
    request: &SelectedRead,
    checked_size: usize,
    max_asset_bytes: usize,
    cancelled: &AtomicBool,
) -> Result<VerifiedBytes, ResourceSourceError> {
    let mut result = vec![0_u8; checked_size];
    let mut digest = Sha256::new();
    let mut read_bytes = 0_usize;
    while read_bytes < checked_size {
        // Regular-file reads are not preemptible; the same worker owns this
        // descriptor/buffer until the call returns and cancellation is observed.
        if cancelled.load(Ordering::Acquire) {
            return Err(ResourceSourceError::Cancelled);
        }
        let end = read_bytes
            .checked_add((64 * 1024).min(checked_size - read_bytes))
            .ok_or(ResourceSourceError::SourceChanged)?;
        let count = file
            .read(&mut result[read_bytes..end])
            .map_err(|_| ResourceSourceError::Io)?;
        if count == 0 {
            break;
        }
        let next = read_bytes
            .checked_add(count)
            .ok_or(ResourceSourceError::SourceChanged)?;
        digest.update(&result[read_bytes..next]);
        read_bytes = next;
    }
    result.truncate(read_bytes);
    if cancelled.load(Ordering::Acquire) {
        return Err(ResourceSourceError::Cancelled);
    }
    if read_bytes == checked_size {
        let mut overrun = [0_u8; 1];
        if file
            .read(&mut overrun)
            .map_err(|_| ResourceSourceError::Io)?
            != 0
        {
            return Err(ResourceSourceError::SourceChanged);
        }
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(ResourceSourceError::Cancelled);
    }
    let actual_sha256: [u8; 32] = digest.finalize().into();
    if result.len() as u64 != request.size
        || result.len() > max_asset_bytes
        || actual_sha256 != request.sha256
    {
        return Err(ResourceSourceError::SourceChanged);
    }
    Ok(VerifiedBytes {
        bytes: result,
        mime: request.mime,
    })
}

fn valid_scheme(scheme: &str) -> bool {
    !scheme.is_empty()
        && scheme.len() <= 32
        && scheme.bytes().enumerate().all(|(i, c)| {
            if i == 0 {
                c.is_ascii_lowercase()
            } else {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'
            }
        })
        && scheme != "file"
        && scheme != "http"
        && scheme != "https"
}

/// Closed grammar: percent decode exactly once and then check each component.
/// An encoded '/' or '\\', encoded '%' for double-decode or non-ASCII source
/// path is refused; source graph paths themselves must be portable.
fn decode_path_once(encoded: &str) -> Result<String, ResourceSourceError> {
    if encoded.is_empty() || encoded.len() > 3072 || encoded.starts_with('/') {
        return Err(ResourceSourceError::InvalidPath);
    }
    let mut out = Vec::with_capacity(encoded.len());
    let mut chars = encoded.bytes();
    while let Some(b) = chars.next() {
        match b {
            b'%' => {
                let (Some(a), Some(c)) = (chars.next(), chars.next()) else {
                    return Err(ResourceSourceError::InvalidPath);
                };
                let val = hex_byte(a)
                    .and_then(|hi| hex_byte(c).map(|lo| hi * 16 + lo))
                    .ok_or(ResourceSourceError::InvalidPath)?;
                if val == b'/'
                    || val == b'\\'
                    || val == b'%'
                    || val == 0
                    || !(32..127).contains(&val)
                {
                    return Err(ResourceSourceError::InvalidPath);
                }
                out.push(val);
            }
            b'\\' | 0..=31 | 127..=255 => return Err(ResourceSourceError::InvalidPath),
            other => out.push(other),
        }
    }
    let decoded = String::from_utf8(out).map_err(|_| ResourceSourceError::InvalidPath)?;
    check_relative_asset(&decoded)?;
    Ok(decoded)
}

fn hex_byte(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn check_relative_asset(path: &str) -> Result<(), ResourceSourceError> {
    if path.is_empty()
        || path.len() > 1024
        || path.contains('\\')
        || path.contains('%')
        || path.contains('?')
        || path.contains('#')
        || path.bytes().any(|c| !(32..127).contains(&c))
    {
        return Err(ResourceSourceError::InvalidPath);
    }
    let mut count = 0;
    for component in Path::new(path).components() {
        let Component::Normal(name) = component else {
            return Err(ResourceSourceError::InvalidPath);
        };
        let component = name.to_str().ok_or(ResourceSourceError::InvalidPath)?;
        if component == "." || component == ".." || component.is_empty() || component.ends_with(' ')
        {
            return Err(ResourceSourceError::InvalidPath);
        }
        count += 1;
        if count > 16 {
            return Err(ResourceSourceError::InvalidPath);
        }
    }
    if path.contains("//") || path.starts_with('/') {
        return Err(ResourceSourceError::InvalidPath);
    }
    Ok(())
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "ttf" => "font/ttf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        "txt" | "md" | "properties" | "mf" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        "jar" => "application/java-archive",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("keld-gh473-asset-{}-{id}", std::process::id()));
            std::fs::create_dir(&path).expect("new isolated root");
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn original_source(root: &Path, assets: Vec<VerifiedWebAsset>) -> Arc<AdmittedWebAppSource> {
        original_source_with_entry(root, assets, Path::new("index.html"))
    }

    fn original_source_with_entry(
        root: &Path,
        assets: Vec<VerifiedWebAsset>,
        selected_entry: &Path,
    ) -> Arc<AdmittedWebAppSource> {
        use nix::fcntl::open;
        let root_fd = open(
            root,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .expect("owned directory FD");
        AdmittedWebAppSource::from_admitted_graph(
            root_fd,
            [7; 32],
            assets,
            selected_entry,
            {
                const POLICY: &str = "{\"profile\":\"legacy\",\"app\":{},\"windows\":{\"fixture\":{\"web\":{\"csp\":\"original-editor-offline\"}}}}\n";
                const EXPECTED: [u8; 32] = [215, 46, 191, 72, 140, 173, 92, 255, 48, 59, 90, 236, 196, 197, 67, 68, 75, 213, 119, 101, 68, 44, 13, 125, 203, 40, 242, 147, 25, 32, 233, 33];
                let path = root.join(".inert-verified-policy.jsonc");
                std::fs::write(&path, POLICY).expect("fixed first-party policy bytes");
                keld_guard::verified_manifest::load_verified_manifest(std::fs::File::open(&path).expect("fixed policy handle"), path, EXPECTED)
                    .expect("same expected fixed policy digest").resolve_window_csp("fixture").expect("fixed trusted test key")
            },
            String::from("keld-resource"),
            String::from("0123456789abcdef0123456789abcdef"),
        )
        .expect("same retained verified asset selections")
    }

    fn asset(path: &str, bytes: &[u8]) -> VerifiedWebAsset {
        VerifiedWebAsset {
            path: PathBuf::from(path),
            size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
        }
    }

    #[test]
    fn native_origin_serialization_is_literal_source_qualified_and_rejects_unknown_tuples() {
        const HOST: &str = "0123456789abcdef0123456789abcdef";
        let fixture = Fixture::new();
        let mut source = original_source(&fixture.0, vec![asset("index.html", b"entry")]);
        let origin = source
            .qualify_native_origin("keld-resource", HOST, 0)
            .expect("qualified actual tuple");
        assert_eq!(
            origin.as_str(),
            "keld-resource://0123456789abcdef0123456789abcdef"
        );
        drop(origin);
        for (protocol, host, port) in [
            ("", HOST, 0),
            ("https", HOST, 0),
            ("KELD-resource", HOST, 0),
            ("keld-resource:", HOST, 0),
            ("keld-resource\0", HOST, 0),
            ("keld-resource", "", 0),
            ("keld-resource", "fedcba9876543210fedcba9876543210", 0),
            ("keld-resource", "0123456789ABCDEF0123456789ABCDEF", 0),
            ("keld-resource", "0123456789abcdef0123456789abcdef:0", 0),
            ("keld-resource", HOST, -1),
            ("keld-resource", HOST, 1),
            ("keld-resource", HOST, 443),
            ("keld-resource", HOST, i64::MAX),
        ] {
            assert!(
                source.qualify_native_origin(protocol, host, port).is_err(),
                "unknown tuple {protocol:?}/{host:?}/{port}"
            );
        }
        // Even another admitted custom scheme has no serialization evidence.
        Arc::get_mut(&mut source)
            .expect("sole component source")
            .scheme = String::from("keld-other");
        assert!(source.qualify_native_origin("keld-other", HOST, 0).is_err());
        Arc::get_mut(&mut source)
            .expect("sole component source")
            .scheme = String::from("keld-resource");
        Arc::get_mut(&mut source)
            .expect("sole component source")
            .authority = String::from("fedcba9876543210fedcba9876543210");
        assert!(
            source
                .qualify_native_origin("keld-resource", HOST, 0)
                .is_err(),
            "original actual tuple cannot qualify for a different selected source authority"
        );
    }

    #[test]
    fn native_origin_uses_and_releases_the_existing_source_retained_byte_account() {
        use super::super::macos_resource_intake::ORIGINAL_WEBAPP_LIMITS;
        use super::super::macos_resource_native::RetainedByteCredit;
        let fixture = Fixture::new();
        let source = original_source(&fixture.0, vec![asset("index.html", b"entry")]);
        let account = source.retained_credit_account();
        let full = RetainedByteCredit::reserve(&account, ORIGINAL_WEBAPP_LIMITS.max_retained_bytes)
            .expect("existing source account full");
        assert!(
            source
                .qualify_native_origin("keld-resource", "0123456789abcdef0123456789abcdef", 0)
                .is_err()
        );
        assert_eq!(
            account.load(Ordering::Acquire),
            ORIGINAL_WEBAPP_LIMITS.max_retained_bytes
        );
        drop(full);
        let origin = source
            .qualify_native_origin("keld-resource", "0123456789abcdef0123456789abcdef", 0)
            .expect("same account recovered");
        assert!(account.load(Ordering::Acquire) > origin.as_str().len());
        let alias = origin
            .try_clone()
            .expect("origin alias shares admission owner");
        drop(origin);
        assert!(account.load(Ordering::Acquire) > 0);
        drop(alias);
        assert_eq!(account.load(Ordering::Acquire), 0);
    }

    #[test]
    fn document_qualification_reuses_source_parser_and_rejects_other_captured_assets() {
        let fixture = Fixture::new();
        let source = original_source(
            &fixture.0,
            vec![asset("index.html", b"entry"), asset("asset.css", b"body{}")],
        );
        let base = "keld-resource://0123456789abcdef0123456789abcdef/";
        assert_eq!(
            source.qualify_document_url(&(base.to_owned() + "index.html?x=%252f")),
            Ok(())
        );
        assert!(source.qualify_url(&(base.to_owned() + "asset.css")).is_ok());
        assert_eq!(
            source.qualify_document_url(&(base.to_owned() + "asset.css")),
            Err(ResourceSourceError::InvalidEntry)
        );
        assert!(source.qualify_document_url("about:blank").is_err());
        assert!(
            source
                .qualify_document_url("keld-resource://fedcba9876543210fedcba9876543210/index.html")
                .is_err()
        );
    }

    #[test]
    fn source_url_parser_keeps_query_outside_root_and_refuses_escape() {
        assert_eq!(
            decode_path_once("images/drawlogo256.png"),
            Ok(String::from("images/drawlogo256.png"))
        );
        for outside in [
            "../outside.png",
            "x/../outside.png",
            "x/%2e%2e/out.png",
            "x/%2f/out.png",
            "x/%5c/out.png",
            "x/%25ff/out.png",
            "x//out.png",
            "%00",
            "x/%zz",
            "x/%252f",
            "x\\y",
            "/etc/passwd",
            "",
        ] {
            assert_eq!(
                decode_path_once(outside),
                Err(ResourceSourceError::InvalidPath),
                "{outside}"
            );
        }
    }

    #[test]
    fn original_content_types_are_not_all_generic_binary() {
        for (path, expected) in [
            ("index.html", "text/html; charset=utf-8"),
            ("js/app.min.js", "text/javascript; charset=utf-8"),
            ("styles/grapheditor.css", "text/css; charset=utf-8"),
            ("resources/dia.txt", "text/plain; charset=utf-8"),
            ("stencils/aws4.xml", "application/xml"),
            ("images/drawlogo256.png", "image/png"),
        ] {
            assert_eq!(mime_for(Path::new(path)), expected, "{path}");
        }
    }

    #[test]
    fn read_buffer_keeps_exact_capacity_and_detects_growth_after_descriptor_open() {
        use std::io::Write as _;
        for size in [0_usize, 1, 64 * 1024] {
            let fixture = Fixture::new();
            let path = fixture.0.join("index.html");
            let original = vec![0x6d; size];
            std::fs::write(&path, &original).expect("source bytes");
            let source = original_source(&fixture.0, vec![asset("index.html", &original)]);
            let selected = source
                .qualify_url("keld-resource://0123456789abcdef0123456789abcdef/index.html")
                .expect("captured source");
            let bytes = source
                .verified_read(&selected, size, &AtomicBool::new(false))
                .expect("exact bound")
                .bytes;
            assert_eq!(bytes, original);
            assert_eq!(bytes.capacity(), size);
            let opened = File::open(&path).expect("same actual descriptor before growth");
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("append")
                .write_all(&[0x77])
                .expect("growth");
            assert_eq!(
                read_verified_file(opened, &selected, size, size, &AtomicBool::new(false)).err(),
                Some(ResourceSourceError::SourceChanged)
            );
            std::fs::write(&path, &original).expect("restore");
            assert_eq!(
                source
                    .verified_read(&selected, size, &AtomicBool::new(false))
                    .expect("healthy after rejected growth")
                    .bytes,
                original
            );
        }
    }

    #[test]
    fn retained_root_reads_real_bytes_then_refuses_modified_same_path() {
        let fixture = Fixture::new();
        let original = b"<html><body>proof</body></html>";
        std::fs::write(fixture.0.join("index.html"), original).expect("original bytes");
        let source = original_source(&fixture.0, vec![asset("index.html", original)]);
        let selection = source
            .qualify_url("keld-resource://0123456789abcdef0123456789abcdef/index.html?appLang=en")
            .expect("query must not become source path");
        assert_eq!(
            source
                .verified_read(&selection, 32 * 1024 * 1024, &AtomicBool::new(false))
                .expect("actual opened original source")
                .bytes,
            original,
        );
        std::fs::write(fixture.0.join("index.html"), b"<html>changed</html>")
            .expect("replaced same-path contents");
        assert_eq!(
            source
                .verified_read(&selection, 32 * 1024 * 1024, &AtomicBool::new(false))
                .err(),
            Some(ResourceSourceError::SourceChanged),
        );
    }

    #[cfg(unix)]
    #[test]
    fn external_symlink_and_wrong_authority_do_not_serve_bytes() {
        let fixture = Fixture::new();
        let outside = Fixture::new();
        let main = b"<html>healthy</html>";
        let forbidden = b"SECRET-SENTINEL";
        std::fs::write(fixture.0.join("index.html"), main).expect("main");
        std::fs::write(outside.0.join("outside.svg"), forbidden).expect("outside");
        std::os::unix::fs::symlink(outside.0.join("outside.svg"), fixture.0.join("outside.svg"))
            .expect("escape");
        let source = original_source(
            &fixture.0,
            vec![asset("index.html", main), asset("outside.svg", forbidden)],
        );
        let escape = source
            .qualify_url("keld-resource://0123456789abcdef0123456789abcdef/outside.svg")
            .expect("test-only declared source path");
        assert_eq!(
            source
                .verified_read(&escape, 32 * 1024 * 1024, &AtomicBool::new(false))
                .err(),
            Some(ResourceSourceError::EscapingFile),
        );
        assert_eq!(
            source
                .qualify_url("keld-resource://fedcba9876543210fedcba9876543210/index.html")
                .err(),
            Some(ResourceSourceError::WrongAuthority),
        );
        let healthy = source
            .qualify_url("keld-resource://0123456789abcdef0123456789abcdef/index.html")
            .expect("same view positive");
        assert_eq!(
            source
                .verified_read(&healthy, 32 * 1024 * 1024, &AtomicBool::new(false))
                .expect("healthy in-root content")
                .bytes,
            main,
        );
    }

    #[test]
    fn cancelled_source_cannot_read_a_healthy_verified_asset_and_recovers() {
        let fixture = Fixture::new();
        let main = b"<html>recover</html>";
        std::fs::write(fixture.0.join("index.html"), main).expect("source");
        let source = original_source(&fixture.0, vec![asset("index.html", main)]);
        let selection = source
            .qualify_url("keld-resource://0123456789abcdef0123456789abcdef/index.html")
            .expect("selected original");
        let cancelled = AtomicBool::new(true);
        assert_eq!(
            source
                .verified_read(&selection, 32 * 1024 * 1024, &cancelled)
                .err(),
            Some(ResourceSourceError::Cancelled),
        );
        // A subsequent independently admitted session has its own cancellation
        // flag; old reader stop intent is never inherited as a source grant.
        assert_eq!(
            source
                .verified_read(&selection, 32 * 1024 * 1024, &AtomicBool::new(false))
                .expect("healthy new session")
                .bytes,
            main,
        );
    }

    #[test]
    fn entry_selector_preserves_serialized_query_and_existing_rejections() {
        let fixture = Fixture::new();
        let main = b"<html>selector</html>";
        let source = original_source(&fixture.0, vec![asset("index.html", main)]);
        let prefix = "keld-resource://0123456789abcdef0123456789abcdef/index.html";
        assert_eq!(source.admitted_entry_url("").as_deref(), Some(prefix));
        let query = "appLang=fr&queryObj=%257b&literal=%23%00&path=%252f";
        assert_eq!(
            source.admitted_entry_url(query),
            Some(format!("{prefix}?{query}"))
        );
        for denied in [
            "#",
            "q=x#fragment",
            "q=bad\0value",
            "line\nfeed",
            "line\rfeed",
            "q=\u{007f}",
        ] {
            assert!(source.admitted_entry_url(denied).is_none(), "{denied:?}");
        }
        // Selector/carrier admission is not final native request admission.
        // Preserve this existing input rule; final full URL normalization and
        // budget refusal now belong to the sole native request preparation.
        let query_only_max = "x".repeat(
            super::super::macos_resource_intake::ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
        );
        assert!(source.admitted_entry_url(&query_only_max).is_some());
        assert!(source.admitted_entry_url(&(query_only_max + "x")).is_none());
    }

    #[test]
    fn nested_non_drawio_entry_is_selected_from_the_exact_admitted_graph_row() {
        let fixture = Fixture::new();
        let nested = fixture.0.join("pages").join("deep");
        std::fs::create_dir_all(&nested).expect("nested code root");
        let entry = b"<html>unrelated nested entry</html>";
        let asset_body = b"body { color: teal }";
        std::fs::write(nested.join("editor.html"), entry).expect("nested verified entry");
        std::fs::write(fixture.0.join("theme.css"), asset_body).expect("other selected resource");
        let source = original_source_with_entry(
            &fixture.0,
            vec![
                asset("pages/deep/editor.html", entry),
                asset("theme.css", asset_body),
            ],
            Path::new("pages/deep/editor.html"),
        );
        let url = source
            .admitted_entry_url("lang=es")
            .expect("selected entry");
        assert_eq!(
            url,
            "keld-resource://0123456789abcdef0123456789abcdef/pages/deep/editor.html?lang=es"
        );
        assert_eq!(source.qualify_document_url(&url), Ok(()));
        let selected = source.qualify_url(&url).expect("selected row");
        assert_eq!(
            source
                .verified_read(&selected, 32 * 1024 * 1024, &AtomicBool::new(false))
                .expect("verified nested entry read")
                .bytes,
            entry
        );
        for wrong in [
            "keld-resource://0123456789abcdef0123456789abcdef/index.html",
            "keld-resource://0123456789abcdef0123456789abcdef/theme.css",
            "keld-resource://0123456789abcdef0123456789abcdef/pages/%2e%2e/theme.css",
            "keld-resource://0123456789abcdef0123456789abcdef/pages/deep/%2e%2e/%2e%2e/secret",
        ] {
            assert_ne!(source.qualify_document_url(wrong), Ok(()), "{wrong}");
        }
    }
}

#[cfg(test)]
mod navigation_url_tests {
    use super::super::macos_resource_intake::{IntakeReject, ORIGINAL_WEBAPP_LIMITS};
    use super::super::macos_resource_native::RetainedByteCredit;
    use super::*;
    use objc2_foundation::NSString;
    use std::sync::atomic::Ordering;

    #[test]
    fn navigation_url_clones_share_charge_until_last_queued_alias_drops() {
        let account = Arc::new(AtomicUsize::new(0));
        let url = MacNavigationUrl::capture(
            &NSString::from_str("keld-resource://host/index.html?q=1"),
            &account,
        )
        .expect("bounded native URL");
        let charged = account.load(Ordering::Acquire);
        assert!(charged > url.as_str().len());
        let queued = url.try_clone().expect("explicit shared alias admission");
        assert!(std::ptr::eq(
            url.as_str().as_ptr(),
            queued.as_str().as_ptr()
        ));
        let with_alias = account.load(Ordering::Acquire);
        assert!(with_alias > charged, "each queued alias reserves its slot");
        drop(url);
        assert_eq!(
            account.load(Ordering::Acquire),
            with_alias,
            "a queued alias still owns its charge"
        );
        drop(queued);
        assert_eq!(account.load(Ordering::Acquire), 0);
    }

    #[test]
    fn navigation_url_cannot_overbook_existing_resource_credit() {
        let account = Arc::new(AtomicUsize::new(0));
        let held = RetainedByteCredit::reserve(&account, ORIGINAL_WEBAPP_LIMITS.max_retained_bytes)
            .expect("exact source-wide maximum");
        assert!(matches!(
            MacNavigationUrl::capture(
                &NSString::from_str("keld-resource://host/index.html"),
                &account
            ),
            Err(IntakeReject::RetainedByteLimit)
        ));
        assert_eq!(
            account.load(Ordering::Acquire),
            ORIGINAL_WEBAPP_LIMITS.max_retained_bytes
        );
        drop(held);
        let next = MacNavigationUrl::capture(
            &NSString::from_str("keld-resource://host/index.html"),
            &account,
        )
        .expect("same account recovered");
        drop(next);
        assert_eq!(account.load(Ordering::Acquire), 0);
    }

    #[test]
    fn navigation_url_alias_refuses_without_releasing_the_original() {
        let account = Arc::new(AtomicUsize::new(0));
        let url = MacNavigationUrl::capture(
            &NSString::from_str("keld-resource://host/index.html"),
            &account,
        )
        .expect("one URL");
        let used = account.load(Ordering::Acquire);
        let held =
            RetainedByteCredit::reserve(&account, ORIGINAL_WEBAPP_LIMITS.max_retained_bytes - used)
                .expect("fill exact remaining credit");
        assert!(matches!(
            url.try_clone(),
            Err(crate::engine::NativeNavigationRefusal::RetainedByteLimit)
        ));
        assert_eq!(
            account.load(Ordering::Acquire),
            ORIGINAL_WEBAPP_LIMITS.max_retained_bytes
        );
        drop(held);
        assert_eq!(account.load(Ordering::Acquire), used);
        let alias = url.try_clone().expect("same source account recovers");
        drop(url);
        assert_ne!(alias.as_str(), "");
        assert!(account.load(Ordering::Acquire) > 0);
        drop(alias);
        assert_eq!(account.load(Ordering::Acquire), 0);
    }

    #[test]
    fn navigation_url_shared_limit_and_failed_send_reclaim_credit() {
        let account = Arc::new(AtomicUsize::new(0));
        let limit = ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units;
        let url = MacNavigationUrl::capture(&NSString::from_str(&"x".repeat(limit)), &account)
            .expect("exact URL limit");
        assert!(matches!(
            MacNavigationUrl::capture(&NSString::from_str(&"x".repeat(limit + 1)), &account),
            Err(IntakeReject::MetadataLimit)
        ));
        let (send, receive) = std::sync::mpsc::channel::<MacNavigationUrl>();
        drop(receive);
        drop(send.send(url));
        assert_eq!(
            account.load(Ordering::Acquire),
            0,
            "failed queued delivery releases its actual owned URL"
        );
    }
}
