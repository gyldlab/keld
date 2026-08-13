//! keld-guard — the capability engine.
//!
//! Every privileged operation in Keld passes through this crate's
//! `(principal, capability, args) -> Decision` check. Normative spec:
//! `docs/architecture/03-security.md`.
//!
//! v0: parse `keld.permissions.jsonc` and default-deny `evaluate` for
//! dotted capabilities (`fs.read`) against path/host scopes. Principals,
//! `$VARS` resolution, symlink/`..` canonicalization beyond rejecting a
//! `..` segment, and channel grants are not in this slice.

mod jsonc;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::jsonc::strip_jsonc_comments;

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
}

impl DenyReason {
    /// Stable `KELD-GUARD*` code for this variant.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotGranted { .. } => "KELD-GUARD001",
            Self::OutOfScope { .. } => "KELD-GUARD002",
            Self::ChannelForbidden { .. } => "KELD-GUARD003",
        }
    }

    /// Snake-case kind for MCP `deny_reason.kind`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NotGranted { .. } => "not_granted",
            Self::OutOfScope { .. } => "out_of_scope",
            Self::ChannelForbidden { .. } => "channel_forbidden",
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
                     Fix the JSON (comments are allowed; trailing commas are not).",
                    p.display()
                ),
                None => write!(
                    f,
                    "KELD-GUARD005: permissions manifest is not valid JSONC — {detail}. \
                     Fix the JSON (comments are allowed; trailing commas are not)."
                ),
            },
        }
    }
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
/// Returns [`ManifestError::Parse`] when the comment-stripped text is not JSON
/// or is not a JSON object.
pub fn parse_manifest(text: &str) -> Result<PermissionsManifest, ManifestError> {
    parse_manifest_at(text, None)
}

/// Reads and parses `keld.permissions.jsonc` from `path`.
///
/// # Errors
///
/// Returns [`ManifestError::NotFound`] when the file is missing,
/// [`ManifestError::Read`] on other I/O errors, or [`ManifestError::Parse`]
/// when the contents are not JSONC.
pub fn load_manifest(path: &Path) -> Result<PermissionsManifest, ManifestError> {
    let text = fs::read_to_string(path).map_err(|e| {
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
    parse_manifest_at(&text, Some(path))
}

fn parse_manifest_at(
    text: &str,
    path: Option<&Path>,
) -> Result<PermissionsManifest, ManifestError> {
    let stripped = strip_jsonc_comments(text);
    serde_json::from_str(&stripped).map_err(|e| ManifestError::Parse {
        path: path.map(Path::to_path_buf),
        detail: e.to_string(),
    })
}

/// Default-deny check of `operation` (capability id, e.g. `fs.read`) against `path`.
///
/// v0 matching: exact string, or a pattern ending in `/**` (the prefix itself
/// or `prefix/` + remainder). A `..` path segment is always out of scope.
/// `$VARS` are matched literally.
///
/// The `Allow` path does not allocate (`json_pointer_for` and `Vec` are deny-only).
#[must_use]
pub fn evaluate(manifest: &PermissionsManifest, operation: &str, path: &str) -> Decision {
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

fn path_in_scope(path: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        if path == prefix {
            return true;
        }
        return path.starts_with(prefix) && path.as_bytes().get(prefix.len()) == Some(&b'/');
    }
    path == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let decision = evaluate(&manifest, "fs.read", "$DOCUMENTS/notes.txt");
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
        let decision = evaluate(&manifest, "fs.write", "$APPDATA/x");
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
        let decision = evaluate(&manifest, "fs.read", "$DOCUMENTS/notes.txt");
        match decision {
            Decision::Deny(DenyReason::OutOfScope {
                scope, requested, ..
            }) => {
                assert!(scope.contains("$APPDATA/**"), "{scope}");
                assert_eq!(requested, "$DOCUMENTS/notes.txt");
            }
            other => panic!("expected OutOfScope, got {other:?}"),
        }
        let swallowed = evaluate(&manifest, "fs.read", "$APPDATAevil/x");
        assert!(
            matches!(swallowed, Decision::Deny(DenyReason::OutOfScope { .. })),
            "prefix without slash must not match /**: {swallowed:?}"
        );
        let traversal = evaluate(&manifest, "fs.read", "$APPDATA/../secret");
        assert!(
            matches!(traversal, Decision::Deny(DenyReason::OutOfScope { .. })),
            ".. segment must not ride a prefix grant: {traversal:?}"
        );
    }

    #[test]
    fn allow_fails_if_deny_inverted() {
        let manifest =
            parse_manifest(r#"{"app":{"fs":{"read":["$APPDATA/**"]}}}"#).expect("manifest");
        assert_eq!(
            evaluate(&manifest, "fs.read", "$APPDATA/notes.txt"),
            Decision::Allow,
            "in-scope path must allow — inverted deny/allow would fail this"
        );
        assert_eq!(evaluate(&manifest, "fs.read", "$APPDATA"), Decision::Allow);
        assert_ne!(
            evaluate(&manifest, "fs.read", "$DOCUMENTS/notes.txt"),
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
            evaluate(&manifest, "fs.read", "https://example.com/x"),
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
        match evaluate(&manifest, "fs.read", "$APPDATA/x") {
            Decision::Deny(DenyReason::NotGranted { .. }) => {}
            other => panic!("non-string grant must fail closed, got {other:?}"),
        }
    }
}
