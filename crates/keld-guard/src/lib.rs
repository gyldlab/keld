//! keld-guard — the capability engine.
//!
//! Normative spec: `docs/architecture/03-security.md`.
//!
//! The engine parses `keld.permissions.jsonc` and default-denies through
//! [`evaluate`] for dotted capabilities (`fs.read`) against path and URL
//! scopes. One matcher serves both: a `/**` prefix grant may cover hierarchy
//! below the destination it names, but it must name one, so no glob grants
//! "any host" for a multi-character scheme. The residuals that carve out of
//! that — one-letter schemes, schemeless references, percent-encoded `..` —
//! are listed in `docs/architecture/03-security.md` §2.
//! [`evaluate`] requires a [`Principal`] and denies anything other than
//! [`Principal::AppProcess`] so `app` scopes cannot be applied to a webview
//! principal. Strict-profile admission is exposed through [`admit`]. Repository
//! maturity and evidence live in `docs/engineering/product-status.tsv`.
//! A caller cannot authorize a webview or plugin by omitting identity.
//! Webview-originated media capture must
//! present a minted [`Principal::Webview`]; missing identity and
//! [`Principal::AppProcess`] are [`DenyReason::MediaPrincipalRequired`]
//! (`KELD-GUARD007`).
//!
//! [`admit`] keeps Strict fail-closed without a complete
//! OS-containment catalog, matching profile digest, and observed §4
//! primitives. [`HostFacts::observe_uncontained`] reports every primitive
//! missing. That is not an OS-containment claim.

mod admit;
mod jsonc;
mod probe;
mod unique_json;
pub mod verified_manifest;

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::jsonc::strip_jsonc_comments;

const MAX_MANIFEST_BYTES: usize = 64 * 1024;

pub use admit::{
    AdmissionError, AdmissionRequest, ArchiveId, ArtifactDigest, CURRENT_POLICY_GENERATION,
    HostFacts, HostOs, MismatchField, OS_CONTAINMENT_PROBES, ProbeLayer, ProbeOracle, ProbeRecord,
    ProbeVerdict, ProfileDigest, ProfileState, ProofArchive, ProofIdentity, RoleInstance, admit,
    expected_layer_for,
};
pub use probe::{ProbeReport, run_synthetic_probes};

/// An unforgeable identity minted by the host for each peer.
///
/// Peers never self-identify; the host assigns ids at link/webview creation
/// and rotates webview principals on navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Principal {
    /// The supervised app process (developer's Bun main).
    AppProcess,
    /// A webview, identified by a host-assigned generation-tagged id.
    Webview {
        /// Host-assigned webview identifier.
        id: u32,
        /// Bumped on navigation so stale grants cannot carry over.
        generation: u32,
    },
    /// A native plugin registered at startup.
    Plugin {
        /// Registration index in load order.
        id: u16,
    },
}

impl Principal {
    /// Stable short name used in deny text (`app` / `webview` / `plugin`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AppProcess => "app",
            Self::Webview { .. } => "webview",
            Self::Plugin { .. } => "plugin",
        }
    }
}

/// The outcome of a guard check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Operation may proceed.
    Allow,
    /// Operation is denied; the reason is safe to surface to developers.
    Deny(DenyReason),
}

/// Why an operation was denied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    /// No grant covers this capability at all.
    NotGranted {
        /// The capability that was required, e.g. `fs.read`.
        capability: String,
        /// JSON pointer of the missing grant, e.g. `/app/fs/read`.
        json_pointer: String,
        /// Resource that would be appended to the grant (path, host, …).
        requested: String,
    },
    /// A grant exists but the arguments fall outside its scope.
    OutOfScope {
        /// The capability that was checked.
        capability: String,
        /// Human-readable description of the failing scope.
        scope: String,
        /// JSON pointer of the grant to widen, e.g. `/app/fs/read`.
        json_pointer: String,
        /// Resource that fell outside the grant.
        requested: String,
    },
    /// The principal is not allowed to use this channel.
    ChannelForbidden {
        /// The kipc channel name.
        channel: String,
    },
    /// v0 evaluate implements [`Principal::AppProcess`] grants only.
    NotAppProcess {
        /// The principal that was presented (never [`Principal::AppProcess`]).
        principal: Principal,
    },
    /// Camera/microphone (and other webview-originated ops) require a minted
    /// [`Principal::Webview`]. Missing identity and [`Principal::AppProcess`]
    /// both fail closed so `/app` media grants cannot apply to the wrong view.
    MediaPrincipalRequired {
        /// The capability that was requested, e.g. `web.camera`.
        capability: String,
        /// What was presented: `None` if identity was omitted.
        presented: Option<Principal>,
    },
}

impl DenyReason {
    /// Stable `KELD-GUARD*` code for this variant.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotGranted { .. } => "KELD-GUARD001",
            Self::OutOfScope { .. } => "KELD-GUARD002",
            Self::ChannelForbidden { .. } => "KELD-GUARD003",
            Self::NotAppProcess { .. } => "KELD-GUARD006",
            Self::MediaPrincipalRequired { .. } => "KELD-GUARD007",
        }
    }

    /// Snake-case kind for MCP `deny_reason.kind`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NotGranted { .. } => "not_granted",
            Self::OutOfScope { .. } => "out_of_scope",
            Self::ChannelForbidden { .. } => "channel_forbidden",
            Self::NotAppProcess { .. } => "not_app_process",
            Self::MediaPrincipalRequired { .. } => "media_principal_required",
        }
    }

    /// Imperative `keld.permissions.jsonc` edit that would satisfy this deny.
    #[must_use]
    pub fn fix(&self) -> String {
        match self {
            Self::NotGranted {
                json_pointer,
                requested,
                ..
            } if requested.is_empty() => {
                format!("Add a grant at `{json_pointer}` in keld.permissions.jsonc.")
            }
            Self::NotGranted {
                json_pointer,
                requested,
                ..
            } => format!("Append \"{requested}\" to `{json_pointer}` in keld.permissions.jsonc."),
            Self::OutOfScope {
                json_pointer,
                requested,
                ..
            } => format!(
                "Widen `{json_pointer}` in keld.permissions.jsonc so it includes `{requested}`."
            ),
            Self::ChannelForbidden { channel } => format!(
                "Add `{channel}` to this principal's channels list in keld.permissions.jsonc."
            ),
            Self::NotAppProcess { principal } => format!(
                "v0 keld-guard only evaluates AppProcess grants; `{}` principals are denied. \
                 Do not apply `/app` scopes to a webview or plugin — window-level grants are not in this slice.",
                principal.label()
            ),
            Self::MediaPrincipalRequired { capability, .. } => format!(
                "Mint the requesting webview principal before evaluating `{capability}`. \
                 Do not present AppProcess — that would apply `/app` media grants to every webview."
            ),
        }
    }
}

impl fmt::Display for DenyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotGranted { capability, .. } => write!(
                f,
                "KELD-GUARD001: capability `{capability}` is not granted. {}",
                self.fix()
            ),
            Self::OutOfScope {
                capability, scope, ..
            } => write!(
                f,
                "KELD-GUARD002: capability `{capability}` denied by scope `{scope}`. {}",
                self.fix()
            ),
            Self::ChannelForbidden { channel } => write!(
                f,
                "KELD-GUARD003: channel `{channel}` is not granted to this principal. {}",
                self.fix()
            ),
            Self::NotAppProcess { principal } => write!(
                f,
                "KELD-GUARD006: v0 evaluate does not apply app grants to principal `{}`. {}",
                principal.label(),
                self.fix()
            ),
            Self::MediaPrincipalRequired {
                capability,
                presented,
            } => {
                let who = presented.map_or("none", Principal::label);
                write!(
                    f,
                    "KELD-GUARD007: `{capability}` requires a minted webview principal \
                     (presented `{who}`). {}",
                    self.fix()
                )
            }
        }
    }
}

/// Failure loading or parsing `keld.permissions.jsonc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// No file at `path`.
    NotFound {
        /// Path that was tried.
        path: PathBuf,
    },
    /// The file exists but could not be read.
    Read {
        /// Path that was tried.
        path: PathBuf,
        /// `io::Error` display text.
        detail: String,
    },
    /// Comment-stripped text is not JSON.
    Parse {
        /// Path when loading from disk; `None` for [`parse_manifest`].
        path: Option<PathBuf>,
        /// `serde_json` error text.
        detail: String,
    },
    /// The input exceeded the bounded permissions-manifest size.
    TooLarge {
        /// Path when loading from disk; `None` for [`parse_manifest`].
        path: Option<PathBuf>,
        /// Maximum accepted byte length.
        max_bytes: usize,
    },
    /// The retained policy bytes are not valid UTF-8.
    InvalidUtf8 {
        /// Diagnostics-only path from the validated boot selection.
        path: PathBuf,
        /// UTF-8 decoder detail.
        detail: String,
    },
    /// The retained policy bytes do not match the boot descriptor digest.
    IntegrityMismatch {
        /// Diagnostics-only path from the validated boot selection.
        path: PathBuf,
        /// Digest decoded from the validated boot descriptor.
        expected: [u8; 32],
        /// Digest computed over the exact bytes supplied to the parser.
        actual: [u8; 32],
    },
}

impl ManifestError {
    /// Stable `KELD-GUARD*` code for this manifest failure.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } | Self::Read { .. } => "KELD-GUARD004",
            Self::Parse { .. } | Self::InvalidUtf8 { .. } => "KELD-GUARD005",
            Self::IntegrityMismatch { .. } => "KELD-GUARD016",
            Self::TooLarge { .. } => "KELD-GUARD017",
        }
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path } => write!(
                f,
                "KELD-GUARD004: permissions manifest not found at `{}`. \
                 Create keld.permissions.jsonc at that path.",
                path.display()
            ),
            Self::Read { path, detail } => write!(
                f,
                "KELD-GUARD004: cannot read permissions manifest at `{}` — {detail}. \
                 Check the path exists and is readable.",
                path.display()
            ),
            Self::Parse { path, detail } => match path {
                Some(p) => write!(
                    f,
                    "KELD-GUARD005: permissions manifest at `{}` is not valid JSONC — {detail}. \
                     Fix the JSON or remove duplicate object keys \
                     (comments are allowed; trailing commas are not).",
                    p.display()
                ),
                None => write!(
                    f,
                    "KELD-GUARD005: permissions manifest is not valid JSONC — {detail}. \
                     Fix the JSON or remove duplicate object keys \
                     (comments are allowed; trailing commas are not)."
                ),
            },
            Self::TooLarge { path, max_bytes } => {
                let max_kib = max_bytes / 1024;
                match path {
                    Some(path) => write!(
                        f,
                        "KELD-GUARD017: permissions manifest at `{}` exceeds the {max_kib} KiB limit. \
                         Reduce the manifest to {max_kib} KiB or less and retry.",
                        path.display()
                    ),
                    None => write!(
                        f,
                        "KELD-GUARD017: permissions manifest exceeds the {max_kib} KiB limit. \
                         Reduce the manifest to {max_kib} KiB or less and retry."
                    ),
                }
            }
            Self::InvalidUtf8 { path, detail } => write!(
                f,
                "KELD-GUARD005: permissions manifest at `{}` is not UTF-8 — {detail}. \
                 Write keld.permissions.jsonc as UTF-8 JSONC.",
                path.display()
            ),
            Self::IntegrityMismatch {
                path,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "KELD-GUARD016: permissions manifest integrity mismatch at `{}` \
                     (expected ",
                    path.display()
                )?;
                write_digest(f, expected)?;
                write!(f, ", actual ")?;
                write_digest(f, actual)?;
                write!(
                    f,
                    "). Rebuild or re-sign the boot artifact so its digest matches the exact policy bytes."
                )
            }
        }
    }
}

fn write_digest(f: &mut fmt::Formatter<'_>, digest: &[u8; 32]) -> fmt::Result {
    for byte in digest {
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

impl std::error::Error for ManifestError {}

/// Parsed `keld.permissions.jsonc`.
///
/// Unknown top-level keys (`$schema`, `windows`, `audit`) are ignored. v0
/// evaluate reads `app.<group>.<action>` string arrays as path/host scopes.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PermissionsManifest {
    #[serde(default)]
    app: Map<String, Value>,
}

/// JSON pointer under `app` for a dotted capability (`fs.read` → `/app/fs/read`).
#[must_use]
pub fn json_pointer_for(capability: &str) -> String {
    if capability.is_empty() {
        return "/app".to_owned();
    }
    let mut pointer = String::from("/app");
    for segment in capability.split('.') {
        pointer.push('/');
        pointer.push_str(segment);
    }
    pointer
}

/// Parses a `keld.permissions.jsonc` document (JSON with `//` and `/* */`).
///
/// # Errors
///
/// Returns [`ManifestError::TooLarge`] when `text` exceeds 64 KiB, or
/// [`ManifestError::Parse`] when the comment-stripped text is ambiguous,
/// malformed, or not a JSON object.
pub fn parse_manifest(text: &str) -> Result<PermissionsManifest, ManifestError> {
    ensure_manifest_size(text.len(), None)?;
    parse_manifest_at(text, None)
}

/// Reads and parses `keld.permissions.jsonc` from `path`.
///
/// # Errors
///
/// Returns [`ManifestError::NotFound`] when the file is missing,
/// [`ManifestError::Read`] on other I/O/encoding errors,
/// [`ManifestError::TooLarge`] above 64 KiB, or [`ManifestError::Parse`] when
/// the contents are ambiguous or malformed JSONC.
pub fn load_manifest(path: &Path) -> Result<PermissionsManifest, ManifestError> {
    let file = File::open(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ManifestError::NotFound {
                path: path.to_path_buf(),
            }
        } else {
            ManifestError::Read {
                path: path.to_path_buf(),
                detail: e.to_string(),
            }
        }
    })?;
    let bytes = read_manifest_bytes(file, path)?;
    let text = String::from_utf8(bytes).map_err(|error| ManifestError::Read {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })?;
    parse_manifest_at(&text, Some(path))
}

pub(crate) fn read_manifest_bytes<R: Read>(
    reader: R,
    path: &Path,
) -> Result<Vec<u8>, ManifestError> {
    let mut bytes = Vec::with_capacity(MAX_MANIFEST_BYTES + 1);
    reader
        .take((MAX_MANIFEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| ManifestError::Read {
            path: path.to_path_buf(),
            detail: error.to_string(),
        })?;
    ensure_manifest_size(bytes.len(), Some(path))?;
    Ok(bytes)
}

fn ensure_manifest_size(length: usize, path: Option<&Path>) -> Result<(), ManifestError> {
    if length > MAX_MANIFEST_BYTES {
        return Err(ManifestError::TooLarge {
            path: path.map(Path::to_path_buf),
            max_bytes: MAX_MANIFEST_BYTES,
        });
    }
    Ok(())
}

fn parse_manifest_at(
    text: &str,
    path: Option<&Path>,
) -> Result<PermissionsManifest, ManifestError> {
    let stripped = strip_jsonc_comments(text).map_err(|detail| ManifestError::Parse {
        path: path.map(Path::to_path_buf),
        detail: detail.to_owned(),
    })?;
    let value = unique_json::parse(&stripped).map_err(|e| ManifestError::Parse {
        path: path.map(Path::to_path_buf),
        detail: e.to_string(),
    })?;
    serde_json::from_value(value).map_err(|e| ManifestError::Parse {
        path: path.map(Path::to_path_buf),
        detail: e.to_string(),
    })
}

/// Default-deny check of `operation` (capability id, e.g. `fs.read`) against `path`.
///
/// `principal` is required so callers cannot apply `app` scopes by omitting
/// identity. v0 implements [`Principal::AppProcess`] grants only; any other
/// principal is [`DenyReason::NotAppProcess`] (`KELD-GUARD006`) even when the
/// path is in scope for `app`. That check runs before grant lookup so an empty
/// manifest cannot collapse into "add a grant at `/app/…`".
///
/// v0 matching: exact string, or a pattern ending in `/**` (the prefix itself
/// or `prefix/` + remainder). A `..` path segment is always out of scope.
/// `$VARS` are matched literally.
///
/// Two rules keep a `/**` grant from handing over an authority. A grant that
/// names only a scheme and separators (`"https://**"`, `"https:/**"`,
/// `"https:///**"`, `"file:///**"`) matches nothing; and the prefix then matches
/// only at a literal `/`, so `"https://api.example.com/**"` keeps its origin
/// subtree while denying the longer authority `api.example.com.evil.test`.
/// A `C:/**` Windows drive glob keeps working — a single character before the
/// colon is a drive, not a scheme — but the doubled-separator spellings
/// `C://**` and `C:///**` do carry separators and are refused, so "path scopes
/// are untouched" would be too strong. This is not URL normalization, and a
/// schemeless `//host/x` is matched as a path; `docs/architecture/03-security.md`
/// §2 carries the residual list.
///
/// The `Allow` path does not allocate (`json_pointer_for` and `Vec` are deny-only).
#[must_use]
pub fn evaluate(
    manifest: &PermissionsManifest,
    principal: Principal,
    operation: &str,
    path: &str,
) -> Decision {
    if principal != Principal::AppProcess {
        return Decision::Deny(DenyReason::NotAppProcess { principal });
    }
    let Some(node) = grant_node(manifest, operation) else {
        return deny_not_granted(operation, path);
    };
    let Some(arr) = node.as_array() else {
        return deny_not_granted(operation, path);
    };
    if arr.is_empty() || arr.iter().any(|value| !value.is_string()) {
        return deny_not_granted(operation, path);
    }
    if path_has_dotdot(path)
        || !arr.iter().any(|value| {
            value
                .as_str()
                .is_some_and(|scope| path_in_scope(path, scope))
        })
    {
        return deny_out_of_scope(operation, path, arr);
    }
    Decision::Allow
}

fn grant_node<'a>(manifest: &'a PermissionsManifest, capability: &str) -> Option<&'a Value> {
    if capability.is_empty() {
        return None;
    }
    let mut segments = capability.split('.');
    let first = segments.next()?;
    let mut node = manifest.app.get(first)?;
    for segment in segments {
        node = node.as_object()?.get(segment)?;
    }
    Some(node)
}

fn deny_not_granted(operation: &str, path: &str) -> Decision {
    Decision::Deny(DenyReason::NotGranted {
        capability: operation.to_owned(),
        json_pointer: json_pointer_for(operation),
        requested: path.to_owned(),
    })
}

fn deny_out_of_scope(operation: &str, path: &str, arr: &[Value]) -> Decision {
    let scopes: Vec<&str> = arr.iter().filter_map(Value::as_str).collect();
    Decision::Deny(DenyReason::OutOfScope {
        capability: operation.to_owned(),
        scope: scopes.join(", "),
        json_pointer: json_pointer_for(operation),
        requested: path.to_owned(),
    })
}

fn path_has_dotdot(path: &str) -> bool {
    path.split(['/', '\\']).any(|segment| segment == "..")
}

/// Whether `prefix` names a scheme and separators but no destination — the
/// `https:`, `https:/`, `https://`, `https:///`, `file:///` … family.
///
/// Such a grant leaves the authority entirely to the caller, and which spelling
/// actually reaches a host cannot be recovered from the resource: `https:/host`
/// carries no `//`, so RFC 3986 §3.2 sees a path and an authority rule keyed on
/// `://` never fires, yet WHATWG URL parsing — what a browser, `fetch` and a
/// webview do — still reads `host` out of it. Refusing the *grant* closes those
/// spellings together instead of chasing that divergence per resource. The
/// separator set follows the same source: `/` and `\` are interchangeable for a
/// special scheme, and tab/LF/CR are stripped from a URL before parsing. The
/// implementation is deliberately wider than that citation — it takes any ASCII
/// whitespace — because over-refusing a grant that names nothing costs nothing.
///
/// Two shapes are deliberately **not** treated as a scheme, because reading them
/// as one would silently disarm an ordinary path grant:
///
/// - anything with a path separator before the colon. RFC 3986 §3.1 anchors a
///   scheme at the start of the reference, so `$APPDATA/cache/https:` names a
///   directory, `/srv/backup:` names one too, and `\\?\C:` names a drive behind a
///   Windows device prefix. (Separately, and predating this rule: the `/**`
///   suffix and the match anchor are both forward-slash, so a grant whose
///   *separators before* `/**` are backslashes still globs — `C:\Users/**` and
///   `\\?\C:/**` both work — while one ending in `\**` does not glob at all.
///   `fs::canonicalize` returns an all-backslash path, so it needs its trailing
///   separator written as `/` to be globbable.)
/// - a bare `X:` — a drive root, so `C:/**` keeps covering the drive. The
///   exemption stops there: `X:/` and `X://` carry separators, so `a://**` is
///   refused like any other scheme glob. What remains is that `C:/**` and
///   `a:/**` are the same *shape*, so a one-letter scheme's `X:/**` is a path
///   glob; `docs/architecture/03-security.md` §2 records that, and the wider
///   `X://**` and every multi-letter scheme are closed.
fn names_no_destination(prefix: &str) -> bool {
    let Some(colon) = prefix.find(':') else {
        return false;
    };
    if prefix
        .bytes()
        .take(colon)
        .any(|byte| byte == b'/' || byte == b'\\')
    {
        return false;
    }
    if !prefix
        .bytes()
        .skip(colon + 1)
        .all(|byte| byte == b'/' || byte == b'\\' || byte.is_ascii_whitespace())
    {
        return false;
    }
    // A bare drive root (`C:`) still names a destination; `C:/` and `C://` do not.
    colon != 1 || prefix.len() > colon + 1
}

fn path_in_scope(path: &str, pattern: &str) -> bool {
    let Some(prefix) = pattern.strip_suffix("/**") else {
        return path == pattern;
    };
    // A prefix grant covers what lies *below* the node it names, so it has to
    // name one. Stripping `/**` off a scheme-qualified pattern can leave only a
    // scheme and separators, at which point the caller picks the authority.
    if names_no_destination(prefix) {
        return false;
    }
    if path == prefix {
        return true;
    }
    // The separator is what stops a longer sibling authority (or directory)
    // riding the grant: `https://api.example.com/**` requires a `/` exactly
    // where the grant ends, so `api.example.com.evil.test` never matches.
    path.starts_with(prefix) && path.as_bytes().get(prefix.len()) == Some(&b'/')
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    struct CountingReader {
        remaining: usize,
        bytes_read: Rc<Cell<usize>>,
    }

    impl Read for CountingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = buffer.len().min(self.remaining);
            buffer[..count].fill(b' ');
            self.remaining -= count;
            self.bytes_read.set(self.bytes_read.get() + count);
            Ok(count)
        }
    }

    #[test]
    fn bounded_reader_stops_after_limit_plus_one_byte() {
        let bytes_read = Rc::new(Cell::new(0));
        let reader = CountingReader {
            remaining: MAX_MANIFEST_BYTES * 16,
            bytes_read: Rc::clone(&bytes_read),
        };
        let path = Path::new("keld.permissions.jsonc");
        let error = read_manifest_bytes(reader, path).expect_err("oversize input must fail");
        assert!(matches!(error, ManifestError::TooLarge { .. }), "{error}");
        assert_eq!(
            bytes_read.get(),
            MAX_MANIFEST_BYTES + 1,
            "the reader must not consume the rest of an oversized source"
        );
    }

    #[test]
    fn deny_reasons_render_actionable_text() {
        let not_granted = DenyReason::NotGranted {
            capability: "fs.read".to_owned(),
            json_pointer: "/app/fs/read".to_owned(),
            requested: "$DOCUMENTS/notes.txt".to_owned(),
        };
        let not_granted_msg = not_granted.to_string();
        assert!(
            not_granted_msg.contains("KELD-GUARD001"),
            "{not_granted_msg}"
        );
        assert!(
            not_granted_msg.contains("/app/fs/read"),
            "{not_granted_msg}"
        );
        assert!(
            not_granted_msg.contains("$DOCUMENTS/notes.txt"),
            "{not_granted_msg}"
        );
        assert_eq!(
            not_granted.fix(),
            "Append \"$DOCUMENTS/notes.txt\" to `/app/fs/read` in keld.permissions.jsonc."
        );

        let reason = DenyReason::OutOfScope {
            capability: "fs.read".to_owned(),
            scope: "$APPDATA/**".to_owned(),
            json_pointer: "/app/fs/read".to_owned(),
            requested: "$DOCUMENTS/notes.txt".to_owned(),
        };
        assert_eq!(
            reason.to_string(),
            "KELD-GUARD002: capability `fs.read` denied by scope `$APPDATA/**`. \
             Widen `/app/fs/read` in keld.permissions.jsonc so it includes `$DOCUMENTS/notes.txt`."
        );

        let channel = DenyReason::ChannelForbidden {
            channel: "fs.readScoped".to_owned(),
        };
        let channel_msg = channel.to_string();
        assert!(channel_msg.contains("KELD-GUARD003"), "{channel_msg}");
        assert!(
            channel_msg.contains("keld.permissions.jsonc"),
            "{channel_msg}"
        );

        let webview = Principal::Webview {
            id: 1,
            generation: 1,
        };
        let not_app = DenyReason::NotAppProcess { principal: webview };
        let not_app_msg = not_app.to_string();
        assert!(not_app_msg.contains("KELD-GUARD006"), "{not_app_msg}");
        assert!(not_app_msg.contains("webview"), "{not_app_msg}");
        assert!(
            !not_app.fix().contains("/app/"),
            "must not recommend applying app scopes to a webview: {}",
            not_app.fix()
        );
        assert_eq!(not_app.code(), "KELD-GUARD006");
        assert_eq!(not_app.kind(), "not_app_process");

        let media = DenyReason::MediaPrincipalRequired {
            capability: "web.camera".to_owned(),
            presented: Some(Principal::AppProcess),
        };
        let media_msg = media.to_string();
        assert!(media_msg.contains("KELD-GUARD007"), "{media_msg}");
        assert!(media_msg.contains("web.camera"), "{media_msg}");
        assert!(media_msg.contains("app"), "{media_msg}");
        assert!(
            !media.fix().contains("/app/web"),
            "must not recommend applying app media grants: {}",
            media.fix()
        );
        assert_eq!(media.code(), "KELD-GUARD007");
        assert_eq!(media.kind(), "media_principal_required");
        let missing = DenyReason::MediaPrincipalRequired {
            capability: "web.microphone".to_owned(),
            presented: None,
        };
        assert!(missing.to_string().contains("none"), "{}", missing);
    }

    fn eval_app(manifest: &PermissionsManifest, operation: &str, path: &str) -> Decision {
        evaluate(manifest, Principal::AppProcess, operation, path)
    }

    #[test]
    fn webview_principals_distinguish_generations() {
        let before = Principal::Webview {
            id: 7,
            generation: 1,
        };
        let after = Principal::Webview {
            id: 7,
            generation: 2,
        };
        assert_ne!(before, after);
    }

    #[test]
    fn missing_file_is_not_found() {
        let path = std::env::temp_dir()
            .join(format!("keld-guard-missing-{}-nope", std::process::id()))
            .join("keld.permissions.jsonc");
        let err = load_manifest(&path).expect_err("missing file must fail");
        assert!(matches!(err, ManifestError::NotFound { .. }), "{err:?}");
        let msg = err.to_string();
        assert!(msg.contains("KELD-GUARD004"), "{msg}");
        assert!(msg.contains("keld.permissions.jsonc"), "{msg}");
        assert!(msg.contains(&path.display().to_string()), "{msg}");
    }

    #[test]
    fn empty_manifest_denies() {
        let manifest = parse_manifest("{}").expect("empty object");
        let decision = eval_app(&manifest, "fs.read", "$DOCUMENTS/notes.txt");
        match decision {
            Decision::Deny(reason) => {
                assert_eq!(reason.kind(), "not_granted");
                assert_eq!(reason.code(), "KELD-GUARD001");
                assert!(reason.fix().contains("/app/fs/read"), "{}", reason.fix());
            }
            Decision::Allow => panic!("empty manifest must default-deny"),
        }
    }

    #[test]
    fn unknown_operation_is_not_granted() {
        let manifest =
            parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("manifest");
        let decision = eval_app(&manifest, "fs.write", "$APPDATA/x");
        match decision {
            Decision::Deny(DenyReason::NotGranted { capability, .. }) => {
                assert_eq!(capability, "fs.write");
            }
            other => panic!("expected NotGranted, got {other:?}"),
        }
    }

    #[test]
    fn path_outside_scope_is_denied() {
        let manifest =
            parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("manifest");
        let decision = eval_app(&manifest, "fs.read", "$DOCUMENTS/notes.txt");
        match decision {
            Decision::Deny(DenyReason::OutOfScope {
                scope, requested, ..
            }) => {
                assert!(scope.contains("$APPDATA/**"), "{scope}");
                assert_eq!(requested, "$DOCUMENTS/notes.txt");
            }
            other => panic!("expected OutOfScope, got {other:?}"),
        }
        let swallowed = eval_app(&manifest, "fs.read", "$APPDATAevil/x");
        assert!(
            matches!(swallowed, Decision::Deny(DenyReason::OutOfScope { .. })),
            "prefix without slash must not match /**: {swallowed:?}"
        );
        let traversal = eval_app(&manifest, "fs.read", "$APPDATA/../secret");
        assert!(
            matches!(traversal, Decision::Deny(DenyReason::OutOfScope { .. })),
            ".. segment must not ride a prefix grant: {traversal:?}"
        );
    }

    /// Regression, KEL-208: a `/**` suffix is a *path* prefix wildcard, and
    /// applying it to a URL scope left the caller holding the authority. Every
    /// spelling below strips to a prefix that is a scheme followed only by
    /// separators or ASCII whitespace — `"https://**"` to `"https:/"`,
    /// `"https:/**"` to `"https:"` — so the operator named no destination at
    /// all. Whitespace counts because a URL parser strips tab/LF/CR outright;
    /// the rule takes any ASCII whitespace, which is wider than that and only
    /// ever refuses more.
    ///
    /// The spellings are not interchangeable to a *resource* parser, which is
    /// why the grant is what gets refused: `https:/evil.example.com` carries no
    /// `//`, so RFC 3986 reads a path and an authority rule keyed on `://`
    /// never fires, yet WHATWG URL parsing — a browser, `fetch`, a webview —
    /// still reads the host out of it. Refusing the grant closes every
    /// spelling at once, including ones no rule here anticipates.
    ///
    /// `origin_rooted_url_grant_still_covers_its_own_subtree` and
    /// `a_windows_drive_glob_is_a_path_not_a_scheme` are the paired allows that
    /// fail if this test were ever satisfied by denying everything.
    #[test]
    fn url_scope_glob_must_not_delegate_the_authority() {
        for (manifest_text, requested) in [
            (
                r#"{"app":{"net":{"connect":["https://**"]}}}"#,
                "https://evil.example.com",
            ),
            (
                r#"{"app":{"net":{"connect":["https://**"]}}}"#,
                "https://evil.example.com/steal?token=1",
            ),
            // No `//` in the resource: an authority rule keyed on `://` cannot
            // see this one, but a real URL parser reads `evil.example.com`.
            (
                r#"{"app":{"net":{"connect":["https:/**"]}}}"#,
                "https:/evil.example.com",
            ),
            (
                r#"{"app":{"net":{"connect":["https:/**"]}}}"#,
                "https://evil.example.com",
            ),
            // Extra separators: WHATWG ignores the surplus for a special
            // scheme and still finds a host.
            (
                r#"{"app":{"net":{"connect":["https:///**"]}}}"#,
                "https:///evil.example.com",
            ),
            (
                r#"{"app":{"net":{"connect":["https:///**"]}}}"#,
                "https:////evil.example.com",
            ),
            (
                r#"{"app":{"net":{"connect":["wss://**"]}}}"#,
                "wss://attacker.example/ws",
            ),
            (
                r#"{"app":{"net":{"connect":["file://**"]}}}"#,
                "file:///etc/shadow",
            ),
            // The empty (local) authority is still an authority the grant does
            // not name; `file:///etc/**` below is how you name one.
            (
                r#"{"app":{"net":{"connect":["file:///**"]}}}"#,
                "file:///etc/shadow",
            ),
            // `-a` is not a valid RFC 3986 scheme (§3.1 requires a leading
            // ALPHA), so a rule keyed on scheme validity would skip it. This
            // rule is keyed on the grant naming nothing, so it does not.
            (
                r#"{"app":{"net":{"connect":["-a://**"]}}}"#,
                "-a://any.host/x",
            ),
            // `a+b-c.d` *is* a valid scheme — `ALPHA *( ALPHA / DIGIT / "+" /
            // "-" / "." )` — which is why it belongs here: the exotic but legal
            // spellings must be refused just like `https`.
            (
                r#"{"app":{"net":{"connect":["a+b-c.d://**"]}}}"#,
                "a+b-c.d://any.host/x",
            ),
            // `\` is interchangeable with `/` to a URL parser for a special
            // scheme, and tab/LF/CR are stripped from a URL before parsing, so
            // those spellings name no destination either.
            (
                r#"{"app":{"net":{"connect":["https:\\/**"]}}}"#,
                "https:\\/evil.example.com",
            ),
            (
                r#"{"app":{"net":{"connect":["https:\\\\/**"]}}}"#,
                "https:\\\\/evil.example.com",
            ),
            (
                r#"{"app":{"net":{"connect":["https:\t//**"]}}}"#,
                "https:\t//evil.example.com",
            ),
            (
                r#"{"app":{"net":{"connect":["https:\n//**"]}}}"#,
                "https:\n//evil.example.com",
            ),
            // A one-letter scheme gets the drive-letter exemption only for a
            // bare `X:`. `X://` carries separators, so it is refused like any
            // other scheme glob.
            (
                r#"{"app":{"net":{"connect":["a://**"]}}}"#,
                "a://evil.example.com",
            ),
            (
                r#"{"app":{"net":{"connect":["z:///**"]}}}"#,
                "z:///evil.example.com",
            ),
        ] {
            let manifest = parse_manifest(manifest_text).expect("manifest");
            let decision = eval_app(&manifest, "net.connect", requested);
            assert!(
                matches!(decision, Decision::Deny(DenyReason::OutOfScope { .. })),
                "{manifest_text} must not authorize the unnamed authority in \
                 {requested}: {decision:?}"
            );
        }
    }

    /// A grant may still name an authority and own everything under it. This is
    /// a paired allow for `url_scope_glob_must_not_delegate_the_authority`; a
    /// matcher that refused every scheme-qualified grant would pass that test
    /// and fail this one.
    #[test]
    fn origin_rooted_url_grant_still_covers_its_own_subtree() {
        let manifest = parse_manifest(
            r#"{"app":{"net":{"connect":["https://api.myapp.com/**","file:///etc/**"]}}}"#,
        )
        .expect("manifest");
        for requested in [
            "https://api.myapp.com/v1",
            "https://api.myapp.com",
            // `file:///etc` names the empty authority *and* a path under it.
            "file:///etc/shadow",
        ] {
            assert_eq!(
                eval_app(&manifest, "net.connect", requested),
                Decision::Allow,
                "a grant that names its authority still owns its subtree: {requested}"
            );
        }
        let sibling = eval_app(
            &manifest,
            "net.connect",
            "https://api.myapp.com.evil.test/v1",
        );
        assert!(
            matches!(sibling, Decision::Deny(DenyReason::OutOfScope { .. })),
            "a longer sibling authority must not ride the origin grant: {sibling:?}"
        );
    }

    /// `C:` is a valid RFC 3986 scheme *and* the usual Windows drive. Reading it
    /// as a scheme would classify `C://Users/app/x` — which Windows accepts — as
    /// a URI whose authority is `Users`, and a `C:/**` grant would stop covering
    /// it. A single character before the colon is therefore a drive letter.
    #[test]
    fn a_windows_drive_glob_is_a_path_not_a_scheme() {
        let manifest =
            parse_manifest(r#"{"app":{"fs":{"read":["C:/**","d:/data/**"]}}}"#).expect("manifest");
        for requested in ["C:/Users/app/x", "C://Users/app/x", "d:/data//cache/x"] {
            assert_eq!(
                eval_app(&manifest, "fs.read", requested),
                Decision::Allow,
                "a drive-letter grant keeps plain path semantics: {requested}"
            );
        }
        let outside = eval_app(&manifest, "fs.read", "E:/other/x");
        assert!(
            matches!(outside, Decision::Deny(DenyReason::OutOfScope { .. })),
            "an ungranted drive is still out of scope: {outside:?}"
        );
    }

    /// The documented residual, pinned so it cannot drift in either direction.
    ///
    /// `C:/**` and `a:/**` are the same *shape* — one character, a colon, `/**`
    /// — so the drive-letter carve-out has to let both through, and a one-letter
    /// scheme's `X:/**` is therefore a path glob that *does* reach `a://host`.
    /// Architecture 03 §2 records it, alongside schemeless `/**` and `//**`, as
    /// a shape the guarantee does not cover.
    ///
    /// This test pins the *removal* direction: deleting the carve-out breaks
    /// every Windows drive-root grant and fails here. The *widening* direction
    /// is pinned by `url_scope_glob_must_not_delegate_the_authority` instead,
    /// because widening reopens `a://**`, which this test does not assert.
    #[test]
    fn a_one_letter_scheme_glob_is_a_path_glob_and_that_is_the_documented_residual() {
        let manifest =
            parse_manifest(r#"{"app":{"net":{"connect":["a:/**"]}}}"#).expect("manifest");
        assert_eq!(
            eval_app(&manifest, "net.connect", "a://evil.example.com"),
            Decision::Allow,
            "a one-letter scheme is indistinguishable from a drive root, so it \n             stays a path glob — architecture 03 §2 records it as the residual"
        );
        // The moment the scheme is longer than one character the ambiguity is
        // gone, and the same shape is refused.
        let two = parse_manifest(r#"{"app":{"net":{"connect":["ab:/**"]}}}"#).expect("manifest");
        let denied = eval_app(&two, "net.connect", "ab://evil.example.com");
        assert!(
            matches!(denied, Decision::Deny(DenyReason::OutOfScope { .. })),
            "a multi-character scheme glob is closed: {denied:?}"
        );
    }

    /// A filesystem path may itself contain `://` — a cache entry named after a
    /// URL is the ordinary case — and it must keep the enclosing path grant.
    /// Denying it would be fail-closed and still wrong.
    ///
    /// The drive-rooted resources matter as much as the `$VAR` ones: once the
    /// host resolves `$APPDATA` (architecture 03 §2, destination), the resource
    /// a Windows caller presents *starts with a letter*, which is the shape a
    /// scheme check is most likely to misread.
    #[test]
    fn a_path_containing_a_scheme_separator_keeps_its_path_grant() {
        let manifest = parse_manifest(
            r#"{"app":{"fs":{"read":["$APPDATA/**","/var/cache/**","C:/Users/me/AppData/**","cache/**"]}}}"#,
        )
        .expect("manifest");
        for requested in [
            "$APPDATA/cache/https://example.com/index.html",
            "$APPDATA/https://a",
            "/var/cache/wss://sync.example/y",
            "C:/Users/me/AppData/cache/https://example.com/index.html",
            "cache/https://example.com/index.html",
        ] {
            assert_eq!(
                eval_app(&manifest, "fs.read", requested),
                Decision::Allow,
                "an embedded `://` must not turn a path into a URI: {requested}"
            );
        }
        // The same manifest still denies a genuine scheme-qualified destination,
        // so the cases above are not passing because everything is allowed.
        let outside = eval_app(&manifest, "fs.read", "https://example.com/index.html");
        assert!(
            matches!(outside, Decision::Deny(DenyReason::OutOfScope { .. })),
            "a real URI is still outside the path grants: {outside:?}"
        );
    }

    /// A colon that is not in scheme position must not disarm a grant. RFC 3986
    /// §3.1 anchors a scheme at the start of the reference, so a separator
    /// before the colon means there is no scheme: these name a directory or a
    /// drive. The `\\?\C:` device prefix is covered because a Windows path can
    /// legitimately carry a colon that is not a scheme; note this is the
    /// forward-slash spelling, since a backslash grant cannot glob at all.
    #[test]
    fn a_colon_outside_scheme_position_still_names_a_destination() {
        for (manifest_text, requested) in [
            (
                r#"{"app":{"fs":{"read":["$APPDATA/cache/https:/**"]}}}"#,
                "$APPDATA/cache/https://example.com/index.html",
            ),
            (
                r#"{"app":{"fs":{"read":["/srv/backup:/**"]}}}"#,
                "/srv/backup:/x",
            ),
            (r#"{"app":{"fs":{"read":["/C:/**"]}}}"#, "/C:/Users/x"),
            (
                r#"{"app":{"fs":{"read":["\\\\?\\C:/**"]}}}"#,
                "\\\\?\\C:/Users/x",
            ),
            (r#"{"app":{"fs":{"read":["//?/C:/**"]}}}"#, "//?/C:/Users/x"),
        ] {
            let manifest = parse_manifest(manifest_text).expect("manifest");
            assert_eq!(
                eval_app(&manifest, "fs.read", requested),
                Decision::Allow,
                "{manifest_text} names a destination and must still cover {requested}"
            );
        }
        // A grant whose colon *is* in scheme position stays refused, so the
        // cases above are not passing because the rule was switched off.
        let scheme = parse_manifest(r#"{"app":{"fs":{"read":["https:/**"]}}}"#).expect("manifest");
        let denied = eval_app(&scheme, "fs.read", "https:/evil.example.com");
        assert!(
            matches!(denied, Decision::Deny(DenyReason::OutOfScope { .. })),
            "a scheme-position colon is still refused: {denied:?}"
        );
    }

    #[test]
    fn allow_fails_if_deny_inverted() {
        let manifest =
            parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("manifest");
        assert_eq!(
            eval_app(&manifest, "fs.read", "$APPDATA/notes.txt"),
            Decision::Allow,
            "in-scope path must allow — inverted deny/allow would fail this"
        );
        assert_eq!(eval_app(&manifest, "fs.read", "$APPDATA"), Decision::Allow);
        assert_ne!(
            eval_app(&manifest, "fs.read", "$DOCUMENTS/notes.txt"),
            Decision::Allow
        );
    }

    #[test]
    fn jsonc_comments_still_parse() {
        let text = r#"
// line comment
{
  /* block comment */
  "app": {
    "fs": { "read": ["https://example.com/**"] }
  }
}
"#;
        assert!(
            serde_json::from_str::<Value>(text).is_err(),
            "raw JSONC must not parse as JSON — otherwise this test cannot catch a missing stripper"
        );
        let manifest = parse_manifest(text).expect("jsonc with comments");
        assert_eq!(
            eval_app(&manifest, "fs.read", "https://example.com/x"),
            Decision::Allow,
            "https:// inside a string must survive comment stripping"
        );
    }

    #[test]
    fn json_pointer_for_dotted_capability() {
        assert_eq!(json_pointer_for("fs.read"), "/app/fs/read");
        assert_eq!(json_pointer_for(""), "/app");
    }

    #[test]
    fn non_string_scope_is_not_granted() {
        let manifest = parse_manifest(r#"{"app":{"fs":{"read":[1]}}}"#).expect("manifest");
        match eval_app(&manifest, "fs.read", "$APPDATA/x") {
            Decision::Deny(DenyReason::NotGranted { .. }) => {}
            other => panic!("non-string grant must fail closed, got {other:?}"),
        }
    }

    #[test]
    fn web_camera_and_microphone_default_deny() {
        let empty = parse_manifest("{}").expect("empty object");
        match eval_app(&empty, "web.camera", "*") {
            Decision::Deny(DenyReason::NotGranted {
                capability,
                json_pointer,
                requested,
            }) => {
                assert_eq!(capability, "web.camera");
                assert_eq!(json_pointer, "/app/web/camera");
                assert_eq!(requested, "*");
            }
            other => panic!("empty manifest must default-deny web.camera, got {other:?}"),
        }
        match eval_app(&empty, "web.microphone", "*") {
            Decision::Deny(DenyReason::NotGranted { capability, .. }) => {
                assert_eq!(capability, "web.microphone");
            }
            other => panic!("empty manifest must default-deny web.microphone, got {other:?}"),
        }

        let camera_only =
            parse_manifest(r#"{"app":{"web":{"camera":["*"]}}}"#).expect("camera grant");
        assert_eq!(
            eval_app(&camera_only, "web.camera", "*"),
            Decision::Allow,
            "in-scope web.camera must allow — inverted deny/allow would fail this"
        );
        assert!(
            matches!(
                eval_app(&camera_only, "web.microphone", "*"),
                Decision::Deny(DenyReason::NotGranted { .. })
            ),
            "camera grant must not imply microphone"
        );
        assert!(
            matches!(
                eval_app(&camera_only, "web.camera", "https://evil.example"),
                Decision::Deny(DenyReason::OutOfScope { .. })
            ),
            "v0 media sentinel is exact `*`, not an origin glob"
        );
    }

    #[test]
    fn webview_does_not_inherit_app_grants() {
        let manifest =
            parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("manifest");
        assert_eq!(
            eval_app(&manifest, "fs.read", "$APPDATA/notes.txt"),
            Decision::Allow,
            "control: AppProcess must still allow the in-scope path"
        );
        let webview = Principal::Webview {
            id: 7,
            generation: 1,
        };
        match evaluate(&manifest, webview, "fs.read", "$APPDATA/notes.txt") {
            Decision::Deny(reason @ DenyReason::NotAppProcess { principal }) => {
                assert_eq!(principal, webview);
                assert_eq!(principal.label(), "webview");
                assert_eq!(reason.code(), "KELD-GUARD006");
                assert_eq!(reason.kind(), "not_app_process");
                assert!(
                    !reason.fix().contains("/app/fs/read"),
                    "must not recommend applying app scopes: {}",
                    reason.fix()
                );
            }
            other => panic!("webview must not inherit app grants, got {other:?}"),
        }
    }

    #[test]
    fn plugin_does_not_inherit_app_grants() {
        let manifest =
            parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("manifest");
        let plugin = Principal::Plugin { id: 3 };
        match evaluate(&manifest, plugin, "fs.read", "$APPDATA/notes.txt") {
            Decision::Deny(DenyReason::NotAppProcess { principal }) => {
                assert_eq!(principal, plugin);
                assert_eq!(principal.label(), "plugin");
            }
            other => panic!("plugin must not inherit app grants, got {other:?}"),
        }
    }

    #[test]
    fn non_app_principal_is_denied_before_grant_lookup() {
        let empty = parse_manifest("{}").expect("empty object");
        match evaluate(
            &empty,
            Principal::Webview {
                id: 0,
                generation: 0,
            },
            "fs.read",
            "$APPDATA/x",
        ) {
            Decision::Deny(DenyReason::NotAppProcess { .. }) => {}
            other => panic!(
                "empty-manifest webview must be KELD-GUARD006, not NotGranted \
                 (that fix would say to add `/app` scopes): {other:?}"
            ),
        }
        match eval_app(&empty, "fs.read", "$APPDATA/x") {
            Decision::Deny(DenyReason::NotGranted { .. }) => {}
            other => panic!("AppProcess + empty manifest must stay NotGranted, got {other:?}"),
        }
    }
}
