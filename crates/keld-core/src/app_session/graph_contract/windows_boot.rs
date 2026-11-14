//! Windows BOOT8 admission on the existing retained graph owner.
//! A provider receipt is separate from CLI's exact Host-image receipt,
//! avoiding a self-hash cycle in the running Host.

use super::{
    AdmittedStageGraph, GRAPH_FILE, HostAppError, MAX_GRAPH_BYTES, SelectedRendererSource,
    bind_bound_graph, borrow_retained_root, graph_error, graph_hash, open_relative_file,
    parse_bound_graph, parse_sha, read_bounded, root_identity, verify_provider_rows,
    verify_selected_rows,
};
use crate::app_session::original_runtime_entry::{
    ORIGINAL_RUNTIME_ENTRY_PATH, original_entry_bootstrap_source,
};
use std::fs::File;
use std::path::{Path, PathBuf};

/// Read-only distribution qualification for an exact target/provider set.
/// Fields are private; no project, environment, or wire constructor exists.
/// This receipt does not qualify a Host image or an application's own source.
#[derive(Debug, Clone, Copy)]
pub struct WindowsDistributionQualification {
    pub(super) target: &'static str,
    pub(super) provider_trees: [[u8; 32]; 3],
}

impl WindowsDistributionQualification {
    /// The independently qualified build target.
    #[must_use]
    pub const fn target(&self) -> &'static str {
        self.target
    }
    /// Fixed provider order: electron, @keld/api, @keld/kipc.
    #[must_use]
    pub const fn provider_trees(&self) -> &[[u8; 32]; 3] {
        &self.provider_trees
    }
}

/// Fixed `x86_64` Windows providers, independent of the exact Host-image receipt.
/// The release owner enables this tuple only after its required acceptance receipts.
#[must_use]
pub const fn qualified_windows_distribution() -> Option<WindowsDistributionQualification> {
    #[cfg(all(target_arch = "x86_64", target_env = "msvc"))]
    {
        Some(WindowsDistributionQualification {
            target: "x86_64-pc-windows-msvc",
            provider_trees: [
                [
                    0xb4, 0xc8, 0xe8, 0xf1, 0x4f, 0xba, 0x45, 0xd8, 0xf3, 0xef, 0x5e, 0xa0, 0x44,
                    0xdb, 0x90, 0x39, 0x93, 0xb2, 0x57, 0xc6, 0xf3, 0xec, 0xb8, 0x97, 0x22, 0xc0,
                    0xb7, 0x64, 0x38, 0x38, 0x01, 0x18,
                ],
                [
                    0x10, 0xba, 0xb8, 0x6f, 0x55, 0xdd, 0xba, 0x99, 0x96, 0x8f, 0x93, 0x52, 0x0a,
                    0x6d, 0xbd, 0x90, 0xbc, 0x15, 0x3d, 0x1a, 0x14, 0x9b, 0x3a, 0xf8, 0x4c, 0xf1,
                    0xb7, 0xfe, 0x5c, 0xa4, 0x8f, 0xa7,
                ],
                [
                    0x81, 0xc2, 0x53, 0xdf, 0x16, 0xa3, 0xc5, 0x27, 0xdc, 0xaf, 0x72, 0x84, 0x5d,
                    0xf5, 0xaa, 0x96, 0x05, 0xef, 0xbe, 0xef, 0x12, 0x73, 0xe8, 0xa4, 0x54, 0xf4,
                    0xb8, 0x3e, 0x43, 0xc5, 0x05, 0x62,
                ],
            ],
        })
    }
    #[cfg(not(all(target_arch = "x86_64", target_env = "msvc")))]
    {
        None
    }
}

/// The supported Windows target spelling shared by CLI and Host.
#[must_use]
pub const fn windows_graph_target() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64-pc-windows-msvc"
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        "unsupported-windows-target"
    }
}

pub(in crate::app_session) fn read_windows_bound_graph_from_retained_root(
    root_path: &Path,
    root_fd: &File,
    expected_graph_sha256: [u8; 32],
    main_entry: &Path,
    selected: &SelectedRendererSource,
) -> Result<AdmittedStageGraph, HostAppError> {
    bind_with_qualification(
        root_path,
        root_fd,
        expected_graph_sha256,
        main_entry,
        selected,
        qualified_windows_distribution().as_ref(),
    )
}

// The only nonproduction caller is the private retained-filesystem fixture.
// Production always uses the compiled getter, never descriptor receipts.
pub(super) fn bind_with_qualification(
    root_path: &Path,
    root_fd: &File,
    expected_graph_sha256: [u8; 32],
    main_entry: &Path,
    selected: &SelectedRendererSource,
    qualification: Option<&WindowsDistributionQualification>,
) -> Result<AdmittedStageGraph, HostAppError> {
    let retained = borrow_retained_root(root_path, root_fd)?;
    let file = open_relative_file(&retained, Path::new(GRAPH_FILE), "graph leaf")?;
    let bytes = read_bounded(file, MAX_GRAPH_BYTES, "graph size")?;
    let graph = parse_bound_graph(&bytes, expected_graph_sha256, main_entry)?;
    verify_selected_rows(&graph, selected)?;
    let qualified = qualification.ok_or_else(|| {
        graph_error(
            "Windows distribution qualification",
            "no independently qualified Windows target/provider tuple is compiled",
        )
    })?;
    if qualified.target != windows_graph_target() {
        return Err(graph_error(
            "Windows distribution qualification",
            "foreign build target",
        ));
    }
    verify_provider_rows(&graph.files, &qualified.provider_trees)?;
    // One physical census/package validation owns the same parsed graph.
    let mut admitted =
        bind_bound_graph(root_path, &retained, &bytes, expected_graph_sha256, graph)?;
    admitted.bind_selected_source(selected)?;
    admitted.retain_windows_runtime_entry()?;
    if root_identity(&admitted.root, &admitted.retained_root)? != admitted.root_identity {
        return Err(graph_error(
            "retained root",
            "root replaced after Windows binding",
        ));
    }
    Ok(admitted)
}

impl AdmittedStageGraph {
    fn retain_windows_runtime_entry(&mut self) -> Result<(), HostAppError> {
        if self.windows_runtime_entry.is_some() {
            return Err(graph_error(
                "Windows runtime entry",
                "entry already retained",
            ));
        }
        let expected = original_entry_bootstrap_source(&self.graph.entry)?;
        let row = self
            .graph
            .files
            .iter()
            .find(|row| row.path == ORIGINAL_RUNTIME_ENTRY_PATH)
            .ok_or_else(|| graph_error("Windows runtime entry", "canonical entry is absent"))?;
        if row.size != expected.len() as u64 || parse_sha(&row.sha256)? != graph_hash(&expected) {
            return Err(graph_error(
                "Windows runtime entry",
                "graph does not pin canonical bytes",
            ));
        }
        let file = open_relative_file(
            &self.retained_root,
            Path::new(ORIGINAL_RUNTIME_ENTRY_PATH),
            "Windows runtime entry",
        )?;
        let read = file
            .try_clone()
            .map_err(|e| graph_error("Windows runtime entry", e))?;
        let actual = read_bounded(read, expected.len(), "Windows runtime entry")?;
        if actual != expected {
            return Err(graph_error(
                "Windows runtime entry",
                "physical canonical bytes differ",
            ));
        }
        // The existing Windows adapter opened FILE_SHARE_READ only. Keep this
        // exact File; a path/hash alone cannot prevent write/delete replacement.
        self.windows_runtime_entry = Some(file);
        Ok(())
    }

    pub(in crate::app_session) fn windows_runtime_entry(
        &self,
        root: &Path,
        main_entry: &Path,
    ) -> Result<PathBuf, HostAppError> {
        if self.windows_runtime_entry.is_none()
            || root != self.root
            || main_entry != Path::new(&self.graph.entry)
        {
            return Err(graph_error(
                "Windows runtime entry",
                "unbound root/entry selection",
            ));
        }
        if root_identity(&self.root, &self.retained_root)? != self.root_identity {
            return Err(graph_error(
                "Windows runtime entry",
                "retained root changed",
            ));
        }
        Ok(self.root.join(ORIGINAL_RUNTIME_ENTRY_PATH))
    }
}
