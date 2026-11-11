//! Windows development graph selection. Project pins are integrity data;
//! only the separately compiled framework tuple selects qualified providers.

use std::fs::File;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::Path;

use keld_core::app_session::{ApprovedGraphCaptureInput, SelectedRendererSource};
use keld_update::{
    WindowsGraphChildKind, WindowsGraphMetadata, open_retained_graph_child, retained_graph_metadata,
};
use sha2::{Digest as _, Sha256};

use super::{
    BootCompileError, DevBootStage, ensure_current_principal_owns, open_windows_launch_guard,
};
use crate::build::config::{AppConfig, parse_development_graph};

const AUTHORING_LIMIT: u64 = 64 * 1024;

/// Distribution-owned data. No deserializer, environment override or project
/// constructor exists. GH499/GH502 + Host review must provide the exact tuple.
#[derive(Clone, Copy)]
struct QualifiedWindowsFramework {
    target: &'static str,
    host_sha256: [u8; 32],
    provider_trees: [[u8; 32]; 3],
}

// Intentionally unavailable until exact Windows Host/provider receipts qualify
// a distribution. A self-consistent project cannot fill this value.
const QUALIFIED_WINDOWS_FRAMEWORK: Option<QualifiedWindowsFramework> = None;

fn target() -> &'static str {
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

fn error(phase: &'static str, detail: impl std::fmt::Display) -> BootCompileError {
    BootCompileError::new(phase, detail.to_string())
}

#[derive(Debug)]
struct CapturedAuthoring {
    name: &'static str,
    file: File,
    identity: WindowsGraphMetadata,
}

impl CapturedAuthoring {
    fn open(root: &File, project: &Path, name: &'static str) -> Result<Self, BootCompileError> {
        Self::open_optional(root, project, name)?
            .ok_or_else(|| error("graph authoring", format!("required {name} is absent")))
    }

    fn open_optional(
        root: &File,
        project: &Path,
        name: &'static str,
    ) -> Result<Option<Self>, BootCompileError> {
        let file = match open_retained_graph_child(root, name, WindowsGraphChildKind::RegularFile) {
            Ok(file) => file,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(error("graph authoring", source)),
        };
        ensure_current_principal_owns(&project.join(name))?;
        let identity = retained_graph_metadata(&file)
            .map_err(|source| error("graph authoring identity", source))?;
        Ok(Some(Self {
            name,
            file,
            identity,
        }))
    }

    fn text(&mut self) -> Result<String, BootCompileError> {
        let mut bytes = Vec::new();
        self.file
            .by_ref()
            .take(AUTHORING_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| error("graph authoring read", source))?;
        if bytes.len() as u64 > AUTHORING_LIMIT {
            return Err(error("graph authoring bound", "authoring exceeds 64 KiB"));
        }
        self.verify_metadata()?;
        String::from_utf8(bytes).map_err(|source| error("graph authoring UTF-8", source))
    }

    fn verify_metadata(&self) -> Result<(), BootCompileError> {
        if retained_graph_metadata(&self.file)
            .map_err(|source| error("graph authoring identity", source))?
            != self.identity
        {
            return Err(error(
                "graph authoring identity",
                "captured authoring changed",
            ));
        }
        Ok(())
    }
}

/// The one captured input crosses into boot.rs only after source/Host/framework
/// checks. It is not application-controlled serialized approval.
#[derive(Debug)]
pub(super) struct CapturedGraphInput {
    pub(super) config: String,
    pub(super) app: AppConfig,
    pub(super) selected: SelectedRendererSource,
    pub(super) approved: ApprovedGraphCaptureInput,
    pub(super) permissions: Vec<u8>,
    pub(super) root: File,
    pub(super) host: File,
    pub(super) expected_host_sha256: [u8; 32],
    authoring: Vec<CapturedAuthoring>,
    _host_parent: File,
}

impl CapturedGraphInput {
    pub(super) fn recheck(&self) -> Result<(), BootCompileError> {
        for captured in &self.authoring {
            captured.verify_metadata()?;
            let current = open_retained_graph_child(
                &self.root,
                captured.name,
                WindowsGraphChildKind::RegularFile,
            )
            .map_err(|source| error("graph authoring recheck", source))?;
            if retained_graph_metadata(&current)
                .map_err(|source| error("graph authoring recheck", source))?
                != captured.identity
            {
                return Err(error(
                    "graph authoring recheck",
                    "selected authoring was replaced",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn compile(&self, graph_bytes: &[u8]) -> Result<Vec<u8>, BootCompileError> {
        let relative = |path: &Path| -> Result<String, BootCompileError> {
            path.to_str()
                .map(|value| value.replace('\\', "/"))
                .ok_or_else(|| error("development graph boot", "non-UTF-8 selected path"))
        };
        let descriptor = serde_json::json!({
            "schema": 8,
            "name": self.app.name,
            "entry": self.app.entry,
            "permissions": {
                "file": "keld.permissions.jsonc",
                "content_sha256": format!("sha256:{:x}", Sha256::digest(&self.permissions)),
            },
            "graph": {
                "file": keld_core::app_session::GRAPH_FILE,
                "content_sha256": format!("sha256:{:x}", Sha256::digest(graph_bytes)),
            },
            "rendererSource": {
                "root": relative(self.selected.root())?,
                "entry": relative(self.selected.entry())?,
            },
            "preload": {
                "path": relative(self.selected.preload_path())?,
                "content_sha256": format!("sha256:{}", hex(&self.selected.preload_sha256())),
            },
        });
        let bytes = serde_json::to_vec(&descriptor)
            .map_err(|source| error("development graph boot", source))?;
        if bytes.len() > 64 * 1024 {
            return Err(error(
                "development graph boot",
                "compiled descriptor exceeds 64 KiB",
            ));
        }
        Ok(bytes)
    }
}

fn hex(bytes: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    encoded
}

fn capture_permissions(
    root: &File,
    project: &Path,
) -> Result<(CapturedAuthoring, Vec<u8>), BootCompileError> {
    let mut permission_file = CapturedAuthoring::open(root, project, "keld.permissions.jsonc")?;
    let permissions = keld_guard::read_manifest_bytes(
        &mut permission_file.file,
        &project.join("keld.permissions.jsonc"),
    )
    .map_err(|source| error("graph permissions", source))?;
    permission_file.verify_metadata()?;
    let manifest = keld_guard::parse_manifest(
        std::str::from_utf8(&permissions).map_err(|source| error("graph permissions", source))?,
    )
    .map_err(|source| error("graph permissions", source))?;
    if manifest.declared_profile() != Some(keld_guard::DeclaredProfile::Legacy) {
        return Err(error(
            "graph profile",
            "Windows selected-source development requires explicit profile: legacy",
        ));
    }
    Ok((permission_file, permissions))
}

fn capture_host(
    developer_host: &Path,
    qualified: &QualifiedWindowsFramework,
) -> Result<(File, File), BootCompileError> {
    let host_parent = open_windows_launch_guard(
        developer_host
            .parent()
            .ok_or_else(|| error("qualified Host", "Host has no adjacent directory"))?,
    )?;
    let host_name = developer_host
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| error("qualified Host", "Host leaf is not UTF-8"))?;
    let mut host =
        open_retained_graph_child(&host_parent, host_name, WindowsGraphChildKind::RegularFile)
            .map_err(|source| error("qualified Host", source))?;
    let before =
        retained_graph_metadata(&host).map_err(|source| error("qualified Host", source))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = host
            .read(&mut buffer)
            .map_err(|source| error("qualified Host", source))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let observed: [u8; 32] = digest.finalize().into();
    if observed != qualified.host_sha256
        || retained_graph_metadata(&host).map_err(|source| error("qualified Host", source))?
            != before
    {
        return Err(error(
            "framework qualification",
            "adjacent Host bytes differ from the independent qualification",
        ));
    }
    host.seek(SeekFrom::Start(0))
        .map_err(|source| error("qualified Host", source))?;
    Ok((host_parent, host))
}

fn capture_input(
    project: &Path,
    developer_host: &Path,
    qualified: Option<&QualifiedWindowsFramework>,
) -> Result<Option<CapturedGraphInput>, BootCompileError> {
    let root = open_windows_launch_guard(project)?;
    ensure_current_principal_owns(project)?;
    let mut config_file = CapturedAuthoring::open(&root, project, "keld.config.ts")?;
    let config = config_file.text()?;
    let selected = keld_core::renderer_source_declared_from_config_ts(&config)
        .map_err(|source| error("graph source selection", source))?;

    let mut build_file = CapturedAuthoring::open_optional(&root, project, "keld.build.ts")?;
    let build = build_file
        .as_mut()
        .map(CapturedAuthoring::text)
        .transpose()?;
    if !selected {
        let graph_declared = build.as_deref().is_some_and(|source| {
            keld_core::literal_config_from_ts(source)
                .is_ok_and(|fields| fields.contains_key("graphSource"))
        });
        if graph_declared {
            return Err(error(
                "graph source selection",
                "rendererSource and graphSource must appear together",
            ));
        }
        return Ok(None);
    }
    let build = build.ok_or_else(|| {
        error(
            "graph source selection",
            "rendererSource requires keld.build.ts graphSource",
        )
    })?;
    let (app, graph) = parse_development_graph(&config, &build).map_err(|source| {
        error(
            "graph authoring",
            format!("{}: {}", source.phase, source.detail),
        )
    })?;
    let (permission_file, permissions) = capture_permissions(&root, project)?;
    let qualified = qualified.ok_or_else(|| error(
        "framework qualification",
        "this CLI has no independently qualified Windows Host/provider tuple; project hashes cannot approve a distribution",
    ))?;
    if qualified.target != target() || graph.provider_trees != qualified.provider_trees {
        return Err(error(
            "framework qualification",
            "target/provider expectations differ from the compiled qualification tuple",
        ));
    }
    let selection = app
        .renderer_source
        .as_ref()
        .ok_or_else(|| error("graph source selection", "selected renderer is absent"))?;
    let selected = SelectedRendererSource::new(
        &selection.root,
        &selection.entry,
        &graph.preload.path,
        graph.preload.sha256,
    )
    .map_err(|source| error("graph source selection", source))?;

    let (host_parent, host) = capture_host(developer_host, qualified)?;
    keld_core::app_session::verify_provider_trees_from_retained_root(
        project,
        &root,
        &qualified.provider_trees,
    )
    .map_err(|source| error("framework provider preflight", source))?;
    let approved = ApprovedGraphCaptureInput {
        expected_package_sha256: graph.package_sha256,
        expected_lock_sha256: graph.lock_sha256,
        expected_package_trees: qualified.provider_trees,
    };
    let captured = CapturedGraphInput {
        config,
        app,
        selected,
        approved,
        permissions,
        root,
        host,
        expected_host_sha256: qualified.host_sha256,
        authoring: vec![
            config_file,
            build_file.ok_or_else(|| {
                error("graph source selection", "captured build source is absent")
            })?,
            permission_file,
        ],
        _host_parent: host_parent,
    };
    captured.recheck()?;
    Ok(Some(captured))
}

pub(super) fn stage_for_project(
    project: &Path,
    developer_host: &Path,
) -> Result<DevBootStage, BootCompileError> {
    stage_with_qualification(
        project,
        developer_host,
        QUALIFIED_WINDOWS_FRAMEWORK.as_ref(),
    )
}

fn stage_with_qualification(
    project: &Path,
    developer_host: &Path,
    qualified: Option<&QualifiedWindowsFramework>,
) -> Result<DevBootStage, BootCompileError> {
    let project = project
        .canonicalize()
        .map_err(|source| error("project root", source))?;
    match capture_input(&project, developer_host, qualified)? {
        Some(input) => super::stage_dev_boot_with_approved_graph(&project, developer_host, &input),
        None => super::stage_dev_boot(&project, developer_host),
    }
}

#[cfg(test)]
mod tests;
