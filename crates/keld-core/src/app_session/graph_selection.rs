//! Shared lexical selected-source and graph path contract.

use std::path::{Component, Path, PathBuf};

use super::{HostAppError, target_error};

pub(super) const MAX_COMPONENTS: usize = 64;
pub(super) const MAX_PATH_BYTES: usize = 4_096;

fn graph_error(phase: &'static str, detail: impl std::fmt::Display) -> HostAppError {
    target_error("graph admission", format!("{phase}: {detail}"))
}

/// Data selected by the one closed installed boot-6 descriptor.
///
/// `root` selects a directory relative to the signed app root; `entry` is
/// relative to that directory, and `preload_path` is relative to the app root.
/// This value verifies lexical shape only. It is not a graph grant, trusted
/// package receipt, native `WebView` authority, or verified publisher identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedRendererSource {
    root: PathBuf,
    entry: PathBuf,
    preload_path: PathBuf,
    preload_sha256: [u8; 32],
}

impl SelectedRendererSource {
    /// Construct a canonical source selector from already parsed closed boot
    /// data. File existence, digests and filesystem authority are checked only
    /// when this selector is bound by the retained graph owner.
    ///
    /// # Errors
    /// Refuses missing, escaping, protected or noncanonical selected paths.
    pub fn new(
        root: &str,
        entry: &str,
        preload_path: &str,
        preload_sha256: [u8; 32],
    ) -> Result<Self, HostAppError> {
        let (root_text, entry_text) = (root, entry);
        let root = relative_path(root)?;
        let entry = relative_path(entry)?;
        let preload_path = relative_path(preload_path)?;
        for path in [&root, &entry, &preload_path] {
            if path.components().any(|component| {
                matches!(component, Component::Normal(name) if {
                    let name = name.to_string_lossy();
                    matches!(name.as_ref(), ".git" | ".keld" | ".keld-work" | "node_modules"
                        | "tests" | "__tests__" | "test-harness" | "private-evidence"
                        | "keld.config.ts" | "keld.build.ts" | "keld.permissions.jsonc"
                        | "keld.boot.json" | "keld.stage-graph.json" | ".npmrc" | ".netrc")
                        || name == ".env" || name.starts_with(".env.")
                })
            }) {
                return Err(graph_error(
                    "selected source",
                    "protected source or preload member",
                ));
            }
        }
        // Validate the combined wire path with its canonical slash separator.
        // Path::join uses backslashes on Windows, which are forbidden on the wire.
        relative_path(&format!("{root_text}/{entry_text}"))?;
        Ok(Self {
            root,
            entry,
            preload_path,
            preload_sha256,
        })
    }

    /// Selected directory relative to the signed application root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Selected entry relative to the selected resource directory.
    #[must_use]
    pub fn entry(&self) -> &Path {
        &self.entry
    }

    /// Selected preload member relative to the signed application root.
    #[must_use]
    pub fn preload_path(&self) -> &Path {
        &self.preload_path
    }

    /// Expected exact preload bytes; possession is not native execution authority.
    #[must_use]
    pub fn preload_sha256(&self) -> [u8; 32] {
        self.preload_sha256
    }

    #[cfg(target_os = "macos")]
    pub(super) fn full_entry(&self) -> PathBuf {
        self.root.join(&self.entry)
    }
}

/// Apply the single GH499 component/length grammar at both CLI capture and
/// Host/Guardian stage readback; an app cannot introduce a second path reader.
///
/// # Errors
/// Refuses an empty, noncanonical, escaping, or overlong path.
pub fn validate_graph_relative_path(input: &str) -> Result<(), HostAppError> {
    relative_path(input).map(|_| ())
}

pub(super) fn relative_path(path: &str) -> Result<PathBuf, HostAppError> {
    if path.len() > MAX_PATH_BYTES
        || path.is_empty()
        || path.contains('\\')
        || path.contains('\0')
        || path.contains(':')
    {
        return Err(graph_error("path", "invalid length or forbidden character"));
    }
    let components = path.split('/').collect::<Vec<_>>();
    if components.len() > MAX_COMPONENTS
        || components
            .iter()
            .any(|name| name.is_empty() || *name == "." || *name == "..")
    {
        return Err(graph_error("path", "too many components or traversal"));
    }
    let parsed = Path::new(path);
    if parsed
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(graph_error("path", "not project-relative"));
    }
    Ok(parsed.to_owned())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)] // fixture assertions report contract failures
mod tests {
    use super::*;

    #[test]
    fn source_paths_use_canonical_wire_separators_on_every_platform() {
        let source = SelectedRendererSource::new(
            "client/assets",
            "nested/editor.html",
            "runtime/preload.cjs",
            [7; 32],
        )
        .expect("portable canonical selection");
        assert_eq!(source.root(), Path::new("client/assets"));
        assert_eq!(source.entry(), Path::new("nested/editor.html"));
        for invalid in ["../escape", "client\\escape", "C:/escape", "client//escape"] {
            assert!(
                SelectedRendererSource::new(invalid, "editor.html", "runtime/preload.cjs", [7; 32])
                    .is_err(),
                "{invalid}"
            );
        }
    }
}
