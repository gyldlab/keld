//! Validated no-flag application boot and host-owned primary session (KEL-96).

#[cfg(any(target_os = "macos", windows))]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::ffi::OsStr;
use std::fmt;
#[cfg(any(target_os = "macos", windows))]
use std::fs::{self, File};
#[cfg(any(target_os = "macos", windows))]
use std::io::{self, Read};
#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt;
#[cfg(any(target_os = "macos", windows, test))]
use std::path::{Component, Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::ExitStatus;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(any(target_os = "macos", windows))]
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
#[cfg(any(target_os = "macos", windows))]
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
#[cfg(any(target_os = "macos", windows))]
use std::sync::{Arc, Mutex};
#[cfg(any(target_os = "macos", windows))]
use std::thread::{self, JoinHandle};
#[cfg(any(target_os = "macos", windows))]
use std::time::{Duration, Instant};

#[cfg(any(target_os = "macos", windows, test))]
use serde::Deserialize;

#[cfg(target_os = "macos")]
use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
#[cfg(target_os = "macos")]
use nix::sys::stat::{SFlag, fstat};

use keld_guard::ManifestError;
#[cfg(any(target_os = "macos", windows))]
use keld_guard::verified_manifest::VerifiedManifest;
#[cfg(any(target_os = "macos", windows))]
use keld_guard::verified_manifest::load_verified_manifest;
#[cfg(any(target_os = "macos", windows))]
use keld_ipc::codec::{decode, encode};
#[cfg(any(target_os = "macos", windows))]
use keld_ipc::frame::{CorrelationId, FrameKind};
#[cfg(any(target_os = "macos", windows))]
use keld_ipc::link::{AppLinkDeadlines, read_frame_interruptible, write_frame};
#[cfg(any(target_os = "macos", windows))]
use keld_ipc::{
    APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL, BootstrapStream, ECHO_CHANNEL, IpcError,
    LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleRequest, LifecycleResponse,
};
#[cfg(target_os = "macos")]
use keld_runtime::macos_guardian::{GuardedPrimary, GuardedPrimaryUpdate, GuardianBootstrap};
#[cfg(any(target_os = "macos", windows))]
use keld_runtime::primary::{BoundPrimaryGeneration, PrimaryRoleEvent};
#[cfg(windows)]
use keld_runtime::primary::{PrimaryRecoveryGate, PrimaryRoleConfig, PrimaryRoleSupervisor};
#[cfg(windows)]
use keld_wv::webview2::WebView2Engine;
#[cfg(target_os = "macos")]
use keld_wv::wkwebview::{AppWindowCommand, AppWindowEvent, WkWebViewEngine};
#[cfg(windows)]
use keld_wv::{AppWindowCommand, AppWindowEvent};
#[cfg(any(target_os = "macos", windows))]
use keld_wv::{NavTarget, WebviewSpec, WvError};
#[cfg(windows)]
use winapi::um::winnt::FILE_ATTRIBUTE_REPARSE_POINT;
#[cfg(windows)]
use windows_permissions::constants::{
    AccessRights, AceFlags, AceType, SeObjectType, SecurityInformation,
};
#[cfg(windows)]
use windows_permissions::utilities::current_process_sid;
#[cfg(windows)]
use windows_permissions::wrappers::GetNamedSecurityInfo;

/// Maximum accepted `keld.boot.json` size.
#[cfg(any(target_os = "macos", windows, test))]
const MAX_BOOT_BYTES: usize = 64 * 1024;
#[cfg(any(target_os = "macos", windows))]
const BOOT_FILE: &str = "keld.boot.json";
#[cfg(any(target_os = "macos", windows, test))]
const PERMISSIONS_FILE: &str = "keld.permissions.jsonc";
#[cfg(any(target_os = "macos", windows, test))]
const DIGEST_PREFIX: &str = "sha256:";
#[cfg(target_os = "macos")]
const GUARDIAN_OWNER_REPLY_DEADLINE: Duration = Duration::from_secs(6);
#[cfg(any(target_os = "macos", windows))]
const DEV_LEASE_ENV: &str = "KELD_DEV_LEASE";
#[cfg(any(target_os = "macos", windows))]
const DEV_LEASE_STDIN_V1: &str = "stdin-v1";
#[cfg(target_os = "macos")]
const DEV_LEASE_DRAIN_READS: usize = 64;
#[cfg(any(target_os = "macos", windows))]
const SESSION_RUNNING: u8 = 0;
#[cfg(any(target_os = "macos", windows))]
const SESSION_LIFECYCLE_QUIT: u8 = 1;
#[cfg(any(target_os = "macos", windows))]
const SESSION_CLI_LEASE_LOST: u8 = 2;

#[cfg(any(target_os = "macos", windows))]
static LISTENER_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
#[cfg(any(target_os = "macos", windows))]
static CHILD_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
#[cfg(any(target_os = "macos", windows))]
static WINDOW_ATTEMPTS: AtomicU32 = AtomicU32::new(0);

#[cfg(target_os = "macos")]
struct DevHostLease {
    input: io::Stdin,
}

#[cfg(any(target_os = "macos", windows))]
#[derive(Clone)]
struct SessionShutdownState {
    cause: Arc<AtomicU8>,
    transition: Arc<Mutex<()>>,
    reader_stop: Arc<AtomicBool>,
    tail_started: Arc<AtomicBool>,
}
/// Opaque host-owned selection minted only from the staged executable layout.
pub struct ValidatedBootSelection {
    #[cfg(any(target_os = "macos", windows))]
    app: AppBootSelection,
    #[cfg(any(target_os = "macos", windows))]
    permissions_file: File,
    #[cfg(any(target_os = "macos", windows))]
    permissions_digest: [u8; 32],
}

#[cfg(not(any(target_os = "macos", windows)))]
impl Drop for ValidatedBootSelection {
    fn drop(&mut self) {}
}

#[cfg(any(target_os = "macos", windows))]
struct AppBootSelection {
    root: PathBuf,
    name: String,
    entry_path: PathBuf,
    entry_file: File,
    renderer_html: Vec<u8>,
}

#[cfg(target_os = "macos")]
struct GuardSnapshot {
    // T2 retains the verified pair for the whole app session. T3 is the first
    // task allowed to read it at a privileged dispatch boundary.
    verified: VerifiedManifest,
    #[cfg(all(test, target_os = "macos"))]
    drop_observer: Option<Arc<AtomicBool>>,
}

#[cfg(all(target_os = "macos", test))]
impl Drop for GuardSnapshot {
    fn drop(&mut self) {
        if let Some(observer) = &self.drop_observer {
            observer.store(true, Ordering::Release);
        }
    }
}

impl fmt::Debug for ValidatedBootSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedBootSelection")
            .finish_non_exhaustive()
    }
}

impl ValidatedBootSelection {
    /// Validates the no-flag app staged beside the current executable.
    ///
    /// On platforms whose first KEL-96 host slice has not landed, this fails
    /// before reading the executable path or any boot file.
    ///
    /// # Errors
    ///
    /// Returns [`HostAppError`] for unsupported platforms, invalid descriptor
    /// bytes, an unsafe staged root, or a missing/escaping/non-regular target.
    pub fn from_current_exe_unprivileged() -> Result<Self, HostAppError> {
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            Err(HostAppError::new(
                "KELD-CORE-034",
                "platform availability",
                "no-flag host support is unavailable on this platform",
                "Complete and prove the named KEL-96/T4 platform slice before launching the host.",
            ))
        }
        #[cfg(any(target_os = "macos", windows))]
        {
            let executable = std::env::current_exe().map_err(|source| {
                HostAppError::io(
                    "KELD-CORE-036",
                    "current executable",
                    &source,
                    "Launch the staged keld-host executable from its owner-private app directory.",
                )
            })?;
            let executable = executable.canonicalize().map_err(|source| {
                HostAppError::io(
                    "KELD-CORE-036",
                    "current executable",
                    &source,
                    "Restore the staged host and relaunch it from the generated app directory.",
                )
            })?;
            let root = executable.parent().ok_or_else(|| {
                HostAppError::new(
                    "KELD-CORE-036",
                    "staged app root",
                    "the current executable has no parent directory",
                    "Launch the staged host from the generated owner-private app directory.",
                )
            })?;
            validate_from_root(root)
        }
    }
}

/// Typed no-flag host boot/session failure.
pub struct HostAppError {
    code: &'static str,
    phase: &'static str,
    detail: String,
    fix: &'static str,
    resources: StartupResourceSnapshot,
    #[cfg(any(target_os = "macos", windows))]
    manifest_source: Option<Box<ManifestError>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct StartupResourceSnapshot {
    listener: u32,
    child: u32,
    window: u32,
}

impl HostAppError {
    fn new(
        code: &'static str,
        phase: &'static str,
        detail: impl Into<String>,
        fix: &'static str,
    ) -> Self {
        Self {
            code,
            phase,
            detail: detail.into(),
            fix,
            resources: startup_resource_snapshot(),
            #[cfg(any(target_os = "macos", windows))]
            manifest_source: None,
        }
    }

    #[cfg(any(target_os = "macos", windows))]
    fn manifest(source: ManifestError) -> Self {
        let code = source.code();
        let detail = source.to_string();
        Self {
            code,
            phase: "permissions manifest preflight",
            detail,
            fix: "Apply the keld-guard correction above, rebuild the staged boot artifact, and relaunch.",
            resources: startup_resource_snapshot(),
            manifest_source: Some(Box::new(source)),
        }
    }

    #[cfg(any(target_os = "macos", windows))]
    fn io(code: &'static str, phase: &'static str, source: &io::Error, fix: &'static str) -> Self {
        Self::new(code, phase, source.to_string(), fix)
    }

    /// Stable registered diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for HostAppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: no-flag host failed during {} — {}. {} \
             [startup-resource-attempts listener={} child={} window={}]",
            self.code,
            self.phase,
            self.detail,
            self.fix,
            self.resources.listener,
            self.resources.child,
            self.resources.window,
        )
    }
}

impl fmt::Debug for HostAppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostAppError")
            .field("code", &self.code)
            .field("phase", &self.phase)
            .field("detail", &self.detail)
            .field("resources", &self.resources)
            .field("manifest_source", &{
                #[cfg(any(target_os = "macos", windows))]
                {
                    self.manifest_source.as_ref()
                }
                #[cfg(not(any(target_os = "macos", windows)))]
                {
                    Option::<&ManifestError>::None
                }
            })
            .finish_non_exhaustive()
    }
}

impl std::error::Error for HostAppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        #[cfg(any(target_os = "macos", windows))]
        {
            self.manifest_source
                .as_ref()
                .map(|source| source.as_ref() as &(dyn std::error::Error + 'static))
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            None
        }
    }
}

fn startup_resource_snapshot() -> StartupResourceSnapshot {
    #[cfg(any(target_os = "macos", windows))]
    {
        StartupResourceSnapshot {
            listener: LISTENER_ATTEMPTS.load(Ordering::Acquire),
            child: CHILD_ATTEMPTS.load(Ordering::Acquire),
            window: WINDOW_ATTEMPTS.load(Ordering::Acquire),
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        StartupResourceSnapshot::default()
    }
}

#[cfg(any(target_os = "macos", windows, test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BootDocument {
    schema: u8,
    name: String,
    entry: String,
    renderer: String,
    permissions: PermissionsDocument,
}

#[cfg(any(target_os = "macos", windows, test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissionsDocument {
    file: String,
    content_sha256: String,
}

#[cfg(any(target_os = "macos", windows, test))]
struct ParsedBoot {
    name: String,
    entry: PathBuf,
    renderer: PathBuf,
    permissions_digest: [u8; 32],
}

#[cfg(any(target_os = "macos", windows, test))]
fn parse_boot_bytes(bytes: &[u8]) -> Result<ParsedBoot, HostAppError> {
    if bytes.len() > MAX_BOOT_BYTES {
        return Err(boot_error(
            "keld.boot.json exceeds the 64 KiB limit",
            "Regenerate a bounded schema-v1 boot descriptor.",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|source| {
        boot_error(
            format!("keld.boot.json is not UTF-8: {source}"),
            "Write the descriptor as strict UTF-8 JSON.",
        )
    })?;
    let document: BootDocument = serde_json::from_str(text).map_err(|source| {
        boot_error(
            format!("strict schema-v1 JSON was rejected: {source}"),
            "Remove duplicate/unknown fields and regenerate keld.boot.json schema 1.",
        )
    })?;
    if document.schema != 1 {
        return Err(boot_error(
            format!("unsupported schema {}", document.schema),
            "Regenerate keld.boot.json with schema 1.",
        ));
    }
    if document.name.is_empty() {
        return Err(boot_error(
            "name must be a non-empty string",
            "Set the reviewed project name before compiling the boot descriptor.",
        ));
    }
    let entry = validate_relative_path("entry", &document.entry)?;
    let renderer = validate_relative_path("renderer", &document.renderer)?;
    if document.permissions.file != PERMISSIONS_FILE {
        return Err(boot_error(
            format!(
                "permissions.file must be the literal {PERMISSIONS_FILE}, found {}",
                document.permissions.file
            ),
            "Regenerate the descriptor with the fixed permissions filename.",
        ));
    }
    let permissions_digest = decode_digest(&document.permissions.content_sha256)?;
    Ok(ParsedBoot {
        name: document.name,
        entry,
        renderer,
        permissions_digest,
    })
}

#[cfg(any(target_os = "macos", windows, test))]
fn boot_error(detail: impl Into<String>, fix: &'static str) -> HostAppError {
    HostAppError::new("KELD-CORE-035", "boot descriptor validation", detail, fix)
}

#[cfg(any(target_os = "macos", windows, test))]
fn target_error(kind: &'static str, detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-036",
        "staged target validation",
        format!("{kind}: {}", detail.into()),
        "Regenerate the owner-private stage with readable regular files and no symlinks.",
    )
}

#[cfg(any(target_os = "macos", windows, test))]
fn validate_relative_path(kind: &'static str, value: &str) -> Result<PathBuf, HostAppError> {
    if value.is_empty()
        || value.contains('\\')
        || value.contains(':')
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(target_error(
            kind,
            "path is not a portable project-relative path",
        ));
    }
    let path = Path::new(value);
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(target_error(
                kind,
                "path contains a root, prefix, dot, or dot-dot",
            ));
        }
    }
    Ok(path.to_path_buf())
}

#[cfg(any(target_os = "macos", windows, test))]
fn decode_digest(value: &str) -> Result<[u8; 32], HostAppError> {
    let Some(hex) = value.strip_prefix(DIGEST_PREFIX) else {
        return Err(boot_error(
            "permissions.content_sha256 must start with sha256:",
            "Regenerate the exact lowercase SHA-256 descriptor value.",
        ));
    };
    if hex.len() != 64
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(boot_error(
            "permissions.content_sha256 must contain 64 lowercase hexadecimal digits",
            "Regenerate the exact lowercase SHA-256 descriptor value.",
        ));
    }
    let mut digest = [0_u8; 32];
    for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|source| {
            boot_error(
                format!("digest encoding is invalid: {source}"),
                "Regenerate the exact lowercase SHA-256 descriptor value.",
            )
        })?;
        digest[index] = u8::from_str_radix(text, 16).map_err(|source| {
            boot_error(
                format!("digest encoding is invalid: {source}"),
                "Regenerate the exact lowercase SHA-256 descriptor value.",
            )
        })?;
    }
    Ok(digest)
}

#[cfg(target_os = "macos")]
fn validate_from_root(root: &Path) -> Result<ValidatedBootSelection, HostAppError> {
    use nix::sys::stat::fstat;

    let root = root.canonicalize().map_err(|source| {
        HostAppError::io(
            "KELD-CORE-036",
            "staged app root",
            &source,
            "Restore the generated owner-private stage directory.",
        )
    })?;
    let root_fd = open_root(&root)?;
    let root_stat =
        fstat(&root_fd).map_err(|source| target_error("app root", source.to_string()))?;
    if root_stat.st_mode & 0o7777 != 0o700 {
        return Err(target_error(
            "app root",
            "directory mode must be exactly 0o700",
        ));
    }
    let boot_file = open_relative_file(&root_fd, Path::new(BOOT_FILE), "boot descriptor")?;
    let boot_bytes = read_bounded(boot_file, MAX_BOOT_BYTES, "boot descriptor")?;
    let parsed = parse_boot_bytes(&boot_bytes)?;
    let entry_file = open_relative_file(&root_fd, &parsed.entry, "entry")?;
    let renderer_file = open_relative_file(&root_fd, &parsed.renderer, "renderer")?;
    let renderer_html = read_target(renderer_file, "renderer")?;
    std::str::from_utf8(&renderer_html)
        .map_err(|source| target_error("renderer", format!("HTML is not UTF-8: {source}")))?;
    let permissions_file =
        open_relative_file(&root_fd, Path::new(PERMISSIONS_FILE), "permissions file")?;
    Ok(ValidatedBootSelection {
        app: AppBootSelection {
            root,
            name: parsed.name,
            entry_path: parsed.entry,
            entry_file,
            renderer_html,
        },
        permissions_file,
        permissions_digest: parsed.permissions_digest,
    })
}

#[cfg(windows)]
fn validate_from_root(root: &Path) -> Result<ValidatedBootSelection, HostAppError> {
    use std::os::windows::fs::MetadataExt as _;

    let root = root.canonicalize().map_err(|source| {
        HostAppError::io(
            "KELD-CORE-036",
            "staged app root",
            &source,
            "Restore the generated owner-private stage directory.",
        )
    })?;
    let root_metadata = fs::symlink_metadata(&root)
        .map_err(|source| target_error("app root", source.to_string()))?;
    if !root_metadata.is_dir()
        || root_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(target_error(
            "app root",
            "root must be a real directory, not a file or reparse point",
        ));
    }
    verify_windows_stage_acl(&root)?;
    let boot_file = open_relative_file_windows(&root, Path::new(BOOT_FILE), "boot descriptor")?;
    let boot_bytes = read_bounded(boot_file, MAX_BOOT_BYTES, "boot descriptor")?;
    let parsed = parse_boot_bytes(&boot_bytes)?;
    let entry_file = open_relative_file_windows(&root, &parsed.entry, "entry")?;
    let renderer_file = open_relative_file_windows(&root, &parsed.renderer, "renderer")?;
    let renderer_html = read_target(renderer_file, "renderer")?;
    std::str::from_utf8(&renderer_html)
        .map_err(|source| target_error("renderer", format!("HTML is not UTF-8: {source}")))?;
    let permissions_file =
        open_relative_file_windows(&root, Path::new(PERMISSIONS_FILE), "permissions file")?;
    Ok(ValidatedBootSelection {
        app: AppBootSelection {
            root,
            name: parsed.name,
            entry_path: parsed.entry,
            entry_file,
            renderer_html,
        },
        permissions_file,
        permissions_digest: parsed.permissions_digest,
    })
}

#[cfg(windows)]
fn verify_windows_stage_acl(root: &Path) -> Result<(), HostAppError> {
    let current = current_process_sid()
        .map_err(|source| target_error("app root DACL TokenUser", source.to_string()))?;
    let descriptor = GetNamedSecurityInfo(
        root.as_os_str(),
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
    .map_err(|source| target_error("app root DACL readback", source.to_string()))?;
    if descriptor.owner() != Some(&current) {
        return Err(target_error(
            "app root DACL",
            "owner does not equal the current process TokenUser SID",
        ));
    }
    let sddl = descriptor
        .as_sddl()
        .map_err(|source| target_error("app root DACL readback", source.to_string()))?;
    if !sddl.to_string_lossy().contains("D:P") {
        return Err(target_error(
            "app root DACL",
            "DACL inheritance is not protected",
        ));
    }
    let dacl = descriptor
        .dacl()
        .ok_or_else(|| target_error("app root DACL", "security descriptor contains no DACL"))?;
    if dacl.len() != 1 {
        return Err(target_error(
            "app root DACL",
            format!("expected one access rule, found {}", dacl.len()),
        ));
    }
    let ace = dacl
        .get_ace(0)
        .ok_or_else(|| target_error("app root DACL", "the one access rule is unreadable"))?;
    let required_flags = AceFlags::ContainerInherit | AceFlags::ObjectInherit;
    if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE
        || ace.mask() != AccessRights::FileAllAccess
        || ace.sid() != Some(&current)
        || ace.flags() != required_flags
    {
        return Err(target_error(
            "app root DACL",
            "expected one non-inherited current-user full-control rule for files and directories",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn open_relative_file_windows(
    root: &Path,
    path: &Path,
    kind: &'static str,
) -> Result<File, HostAppError> {
    use std::os::windows::fs::MetadataExt as _;

    let components = path
        .components()
        .map(|component| match component {
            Component::Normal(value) => Ok(value),
            _ => Err(target_error(kind, "path is not project-relative")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.is_empty() {
        return Err(target_error(kind, "path is empty"));
    }
    let mut candidate = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        candidate.push(component);
        let metadata = fs::symlink_metadata(&candidate)
            .map_err(|source| target_error(kind, source.to_string()))?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(target_error(kind, "path contains a reparse point"));
        }
        let is_leaf = index + 1 == components.len();
        if (is_leaf && !metadata.is_file()) || (!is_leaf && !metadata.is_dir()) {
            return Err(target_error(
                kind,
                if is_leaf {
                    "target is not a regular file"
                } else {
                    "parent component is not a directory"
                },
            ));
        }
    }
    let file = File::open(&candidate).map_err(|source| target_error(kind, source.to_string()))?;
    let metadata = file
        .metadata()
        .map_err(|source| target_error(kind, source.to_string()))?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(target_error(
            kind,
            "opened target is not a regular non-reparse file",
        ));
    }
    Ok(file)
}

#[cfg(target_os = "macos")]
fn open_root(path: &Path) -> Result<std::os::fd::OwnedFd, HostAppError> {
    use nix::fcntl::{OFlag, open};
    use nix::sys::stat::Mode;

    open(
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|source| target_error("app root", source.to_string()))
}

#[cfg(target_os = "macos")]
fn open_relative_file(
    root: &impl std::os::fd::AsFd,
    path: &Path,
    kind: &'static str,
) -> Result<File, HostAppError> {
    use nix::fcntl::{OFlag, openat};
    use nix::sys::stat::Mode;

    let components = path
        .components()
        .map(|component| match component {
            Component::Normal(value) => Ok(value),
            _ => Err(target_error(kind, "path is not project-relative")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let Some((leaf, parents)) = components.split_last() else {
        return Err(target_error(kind, "path is empty"));
    };
    let mut opened_parent = None;
    for component in parents {
        let parent = opened_parent
            .as_ref()
            .map_or_else(|| root.as_fd(), std::os::fd::AsFd::as_fd);
        let next = openat(
            parent,
            *component,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|source| target_error(kind, source.to_string()))?;
        opened_parent = Some(next);
    }
    let parent = opened_parent
        .as_ref()
        .map_or_else(|| root.as_fd(), std::os::fd::AsFd::as_fd);
    let fd = openat(
        parent,
        *leaf,
        OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|source| target_error(kind, source.to_string()))?;
    let file = File::from(fd);
    let metadata = file
        .metadata()
        .map_err(|source| target_error(kind, source.to_string()))?;
    if !metadata.is_file() {
        return Err(target_error(kind, "target is not a regular file"));
    }
    Ok(file)
}

#[cfg(any(target_os = "macos", windows))]
fn read_bounded(mut file: File, limit: usize, kind: &'static str) -> Result<Vec<u8>, HostAppError> {
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(u64::try_from(limit).unwrap_or(u64::MAX) + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| target_error(kind, source.to_string()))?;
    if bytes.len() > limit {
        return Err(if kind == "boot descriptor" {
            boot_error(
                "keld.boot.json exceeds the 64 KiB limit",
                "Regenerate a bounded schema-v1 boot descriptor.",
            )
        } else {
            target_error(kind, format!("file exceeds {limit} bytes"))
        });
    }
    Ok(bytes)
}

#[cfg(any(target_os = "macos", windows))]
fn read_target(mut file: File, kind: &'static str) -> Result<Vec<u8>, HostAppError> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|source| target_error(kind, source.to_string()))?;
    Ok(bytes)
}

/// Runs the validated no-flag host selection until ordered application exit.
///
/// # Errors
///
/// Returns [`HostAppError`] for startup, authenticated session, window,
/// guardian, Bun self-termination, or ordered-shutdown failure.
pub fn run_unprivileged(boot: ValidatedBootSelection) -> Result<(), HostAppError> {
    #[cfg(not(target_os = "macos"))]
    {
        drop(boot);
        Err(HostAppError::new(
            "KELD-CORE-034",
            "platform availability",
            "no-flag host support is unavailable on this platform",
            "Complete and prove the named KEL-96/T4 platform slice before launching the host.",
        ))
    }
    #[cfg(target_os = "macos")]
    {
        let ValidatedBootSelection {
            app,
            permissions_file,
            permissions_digest,
        } = boot;
        // This explicit diagnostic/test path remains incapable of registering
        // a privileged channel. It never serves as recovery from run_guarded.
        drop((permissions_file, permissions_digest));
        run_app(app, None)
    }
}

/// Runs a validated no-flag host session with one immutable verified policy snapshot.
///
/// Policy preflight consumes the already-open KEL-96 permissions handle and
/// decoded digest before the app can create a child, listener, or window.
///
/// # Errors
///
/// Returns [`HostAppError`] for a typed manifest preflight failure or any
/// existing no-flag startup, session, window, guardian, Bun, or shutdown error.
pub fn run_guarded(boot: ValidatedBootSelection) -> Result<(), HostAppError> {
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        drop(boot);
        Err(HostAppError::new(
            "KELD-CORE-034",
            "platform availability",
            "no-flag host support is unavailable on this platform",
            "Complete and prove the named KEL-96/T4 platform slice before launching the host.",
        ))
    }
    #[cfg(windows)]
    {
        let ValidatedBootSelection {
            app,
            permissions_file,
            permissions_digest,
        } = boot;
        let display_path = app.root.join(PERMISSIONS_FILE);
        let verified = load_verified_manifest(permissions_file, display_path, permissions_digest)
            .map_err(HostAppError::manifest)?;
        run_app_windows(app, &verified)
    }
    #[cfg(target_os = "macos")]
    {
        let ValidatedBootSelection {
            app,
            permissions_file,
            permissions_digest,
        } = boot;
        let display_path = app.root.join(PERMISSIONS_FILE);
        let verified = load_verified_manifest(permissions_file, display_path, permissions_digest)
            .map_err(HostAppError::manifest)?;
        let guard_snapshot = GuardSnapshot {
            verified,
            #[cfg(test)]
            drop_observer: None,
        };
        // The owner stays in this frame while run_app performs every startup,
        // event-loop, and ordered-cleanup step. run_app receives only a borrow,
        // so it cannot destroy the verified session policy early.
        run_app(app, Some(&guard_snapshot))
    }
}

#[cfg(target_os = "macos")]
impl DevHostLease {
    fn from_environment() -> Result<Option<Self>, HostAppError> {
        let Some(value) = std::env::var_os(DEV_LEASE_ENV) else {
            return Ok(None);
        };
        if value != OsStr::new(DEV_LEASE_STDIN_V1) {
            return Err(app_detail(
                "dev-host lease",
                format!(
                    "unsupported {DEV_LEASE_ENV} value `{}`",
                    value.to_string_lossy()
                ),
            ));
        }

        let input = io::stdin();
        configure_dev_lease_fd(&input)?;
        Ok(Some(Self { input }))
    }

    fn poll_lost(&mut self) -> Result<bool, HostAppError> {
        let mut input = self.input.lock();
        poll_dev_lease_reader(&mut input)
    }
}

#[cfg(target_os = "macos")]
fn poll_dev_lease_reader(reader: &mut impl Read) -> Result<bool, HostAppError> {
    let mut bytes = [0_u8; 8 * 1024];
    for _ in 0..DEV_LEASE_DRAIN_READS {
        match reader.read(&mut bytes) {
            Ok(0) => return Ok(true),
            Ok(_) => {}
            Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
            Err(source) if source.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(source) => return Err(app_io("dev-host lease read", &source)),
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn configure_dev_lease_fd(fd: &impl std::os::fd::AsFd) -> Result<(), HostAppError> {
    let status =
        fstat(fd).map_err(|source| app_detail("dev-host lease metadata", source.to_string()))?;
    let kind = SFlag::from_bits_truncate(status.st_mode);
    if !kind.contains(SFlag::S_IFIFO) {
        return Err(app_detail(
            "dev-host lease",
            "stdin-v1 requires the CLI-owned pipe reader on standard input",
        ));
    }
    let status_flags = OFlag::from_bits_truncate(
        fcntl(fd, FcntlArg::F_GETFL)
            .map_err(|source| app_detail("dev-host lease flags", source.to_string()))?,
    );
    if !(status_flags & OFlag::O_ACCMODE).is_empty() {
        return Err(app_detail(
            "dev-host lease",
            "stdin-v1 standard input is not the read-only end of its pipe",
        ));
    }
    let descriptor_flags = FdFlag::from_bits_truncate(
        fcntl(fd, FcntlArg::F_GETFD)
            .map_err(|source| app_detail("dev-host lease flags", source.to_string()))?,
    );
    fcntl(fd, FcntlArg::F_SETFD(descriptor_flags | FdFlag::FD_CLOEXEC))
        .map_err(|source| app_detail("dev-host lease isolation", source.to_string()))?;
    fcntl(fd, FcntlArg::F_SETFL(status_flags | OFlag::O_NONBLOCK))
        .map_err(|source| app_detail("dev-host lease monitoring", source.to_string()))?;
    Ok(())
}

#[cfg(any(target_os = "macos", windows))]
impl SessionShutdownState {
    fn new() -> Self {
        Self {
            cause: Arc::new(AtomicU8::new(SESSION_RUNNING)),
            transition: Arc::new(Mutex::new(())),
            reader_stop: Arc::new(AtomicBool::new(false)),
            tail_started: Arc::new(AtomicBool::new(false)),
        }
    }

    fn cause(&self) -> u8 {
        self.cause.load(Ordering::Acquire)
    }

    fn is_running(&self) -> bool {
        self.cause() == SESSION_RUNNING
    }

    #[cfg(all(test, target_os = "macos"))]
    fn claim_lifecycle_quit(&self) -> bool {
        self.claim(SESSION_LIFECYCLE_QUIT)
    }

    #[cfg(any(target_os = "macos", windows))]
    fn claim_cli_lease_lost(&self) -> bool {
        self.claim(SESSION_CLI_LEASE_LOST)
    }

    #[cfg(any(target_os = "macos", windows))]
    fn claim(&self, cause: u8) -> bool {
        let _transition = self.transition_guard();
        self.claim_guarded(cause)
    }

    fn claim_guarded(&self, cause: u8) -> bool {
        let claimed = self
            .cause
            .compare_exchange(SESSION_RUNNING, cause, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        if claimed {
            self.stop_reader();
        }
        claimed
    }

    fn transition_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.transition.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn stop_reader(&self) {
        self.reader_stop.store(true, Ordering::Release);
    }

    fn begin_tail(&self) -> bool {
        !self.tail_started.swap(true, Ordering::AcqRel)
    }
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_lines)] // one startup/cleanup state machine keeps every owned handle transition contiguous
fn run_app(
    boot: AppBootSelection,
    guard_snapshot: Option<&GuardSnapshot>,
) -> Result<(), HostAppError> {
    let dev_lease = DevHostLease::from_environment()?;
    let shutdown = SessionShutdownState::new();
    let AppBootSelection {
        root,
        name,
        entry_path,
        entry_file,
        renderer_html,
    } = boot;
    let html = String::from_utf8(renderer_html).map_err(|source| {
        HostAppError::new(
            "KELD-CORE-036",
            "renderer",
            source.to_string(),
            "Regenerate the stage with UTF-8 renderer HTML.",
        )
    })?;

    let entry_metadata = entry_file
        .metadata()
        .map_err(|source| app_io("validated entry identity", &source))?;
    let executable =
        std::env::current_exe().map_err(|source| app_io("current executable", &source))?;
    let mut guardian_command = Command::new(executable);
    guardian_command
        .arg(keld_runtime::macos_guardian::SUPERVISED_GUARDIAN_ARG)
        .arg(&root)
        .arg(&entry_path)
        .arg(entry_metadata.dev().to_string())
        .arg(entry_metadata.ino().to_string())
        .env_remove(DEV_LEASE_ENV)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    CHILD_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    let pending = GuardianBootstrap::spawn_supervised(guardian_command)
        .map_err(|source| app_runtime("guardian bootstrap", &source))?;
    drop(entry_file);
    let mut guardian = pending
        .register_guarded_primary_until(Instant::now() + APP_LINK_IO_DEADLINE)
        .map_err(|source| app_runtime("guardian registration", &source))?;
    LISTENER_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    let initial = await_bound_generation(
        &mut guardian,
        Instant::now() + APP_LINK_IO_DEADLINE,
        "initial app-link authentication",
    )?;

    let (window_commands_tx, window_commands_rx) = mpsc::channel();
    let guardian_owner = GuardianOwner::start(
        guardian,
        window_commands_tx.clone(),
        dev_lease,
        shutdown.clone(),
    )?;
    let router = PrimaryRouter::start_bound(
        initial,
        window_commands_tx.clone(),
        guardian_owner.handle(),
        shutdown,
    )?;
    guardian_owner.attach_router(router.handle())?;
    let (window_events_tx, window_events_rx) = mpsc::channel();
    let router_handle = router.handle();
    let commands_for_events = window_commands_tx.clone();
    let event_coordinator = thread::Builder::new()
        .name("keld-core-app-window-events".to_owned())
        .spawn(move || {
            coordinate_window_events(&window_events_rx, &router_handle, &commands_for_events)
        })
        .map_err(|source| app_io("window event coordinator", &source))?;

    let mut engine = WkWebViewEngine::new();
    let spec = WebviewSpec {
        title: name,
        initial: NavTarget::Html(html),
        ..WebviewSpec::default()
    };
    WINDOW_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    if let Err(source) = engine.create_app(&spec, window_events_tx.clone()) {
        let primary = app_detail("initial window", source.to_string());
        return finish_guarded_session(guard_snapshot, |_| {
            drop(window_events_tx);
            let event_result = event_coordinator
                .join()
                .map_err(|_| app_detail("window event coordinator", "thread panicked"))
                .and_then(std::convert::identity);
            let router_result = router.shutdown();
            let guardian_result = guardian_owner.shutdown();
            Err(collapse_app_failures(
                &primary,
                [event_result, router_result, guardian_result],
            ))
        });
    }
    let window_result = engine.run_app_until_quit(window_commands_rx, window_events_tx);
    finish_guarded_session(guard_snapshot, |_| {
        drop(window_commands_tx);
        let event_result = event_coordinator
            .join()
            .map_err(|_| app_detail("window event coordinator", "thread panicked"))
            .and_then(std::convert::identity);
        let router_result = router.shutdown();
        let guardian_result = guardian_owner.shutdown();

        match window_result {
            Err(source @ WvError::Navigate(_)) => {
                let primary = app_detail("initial navigation", source.to_string());
                Err(collapse_app_failures(
                    &primary,
                    [guardian_result, router_result, event_result],
                ))
            }
            result => collapse_app_results([
                result.map_err(|source| app_detail("macOS app window", source.to_string())),
                event_result,
                router_result,
                guardian_result,
            ]),
        }
    })
}

#[cfg(windows)]
#[allow(clippy::too_many_lines)] // one startup/cleanup state machine keeps every owned Windows handle transition contiguous
fn run_app_windows(
    boot: AppBootSelection,
    guard_snapshot: &VerifiedManifest,
) -> Result<(), HostAppError> {
    let shutdown = SessionShutdownState::new();
    let AppBootSelection {
        root,
        name,
        entry_path,
        entry_file,
        renderer_html,
    } = boot;
    let html = String::from_utf8(renderer_html).map_err(|source| {
        HostAppError::new(
            "KELD-CORE-036",
            "renderer",
            source.to_string(),
            "Regenerate the stage with UTF-8 renderer HTML.",
        )
    })?;
    drop(entry_file);
    let config = PrimaryRoleConfig::new("bun")
        .arg("run")
        .arg(root.join(&entry_path))
        .current_dir(&root)
        .env_remove(DEV_LEASE_ENV);
    LISTENER_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    CHILD_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    let (supervisor, recovery) = PrimaryRoleSupervisor::start_with_bound_generations_gated(config)
        .map_err(|source| app_runtime("Windows primary startup", &source))?;
    let initial = match await_windows_bound_generation(
        &supervisor,
        &recovery,
        Instant::now() + APP_LINK_IO_DEADLINE,
    ) {
        Ok(bound) => bound,
        Err(primary) => {
            let output = supervisor.output();
            let primary = app_detail(
                "initial Windows primary startup",
                format!(
                    "{primary}; captured stdout: {}; captured stderr: {}",
                    output.stdout, output.stderr
                ),
            );
            supervisor.shutdown();
            let cleanup = match supervisor.wait_for_outcome() {
                keld_runtime::SupervisorOutcome::Stopped => Ok(()),
                keld_runtime::SupervisorOutcome::CrashLoop(error)
                | keld_runtime::SupervisorOutcome::Failed(error) => {
                    Err(app_runtime("Windows primary startup cleanup", &error))
                }
            };
            return Err(collapse_app_failures(&primary, [cleanup]));
        }
    };

    let (window_commands_tx, window_commands_rx) = mpsc::channel();
    let primary_owner = WindowsPrimaryOwner::start(
        supervisor,
        recovery,
        window_commands_tx.clone(),
        shutdown.clone(),
    )?;
    let router = PrimaryRouter::start_bound(
        initial,
        window_commands_tx.clone(),
        primary_owner.handle(),
        shutdown.clone(),
    )?;
    primary_owner.attach_router(router.handle())?;
    let _lease_reader = start_windows_dev_lease(router.handle(), shutdown.clone())?;
    let (window_events_tx, window_events_rx) = mpsc::channel();
    let router_handle = router.handle();
    let commands_for_events = window_commands_tx.clone();
    let event_coordinator = thread::Builder::new()
        .name("keld-core-windows-app-window-events".to_owned())
        .spawn(move || {
            coordinate_window_events(&window_events_rx, &router_handle, &commands_for_events)
        })
        .map_err(|source| app_io("Windows window event coordinator", &source))?;

    let mut engine = WebView2Engine::new()
        .map_err(|source| app_detail("Windows WebView2 initialization", source.to_string()))?;
    let spec = WebviewSpec {
        title: name,
        initial: NavTarget::Html(html),
        ..WebviewSpec::default()
    };
    WINDOW_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    if let Err(source) = engine.create_app(&spec, window_events_tx.clone()) {
        let primary = app_detail("initial Windows window", source.to_string());
        drop(window_events_tx);
        let event_result = event_coordinator
            .join()
            .map_err(|_| app_detail("Windows window event coordinator", "thread panicked"))
            .and_then(std::convert::identity);
        let router_result = router.shutdown();
        let owner_result = primary_owner.shutdown();
        let _retained_digest = guard_snapshot.verified_sha256();
        return Err(collapse_app_failures(
            &primary,
            [event_result, router_result, owner_result],
        ));
    }
    let window_result = engine.run_app_until_quit(window_commands_rx, window_events_tx);
    drop(window_commands_tx);
    let event_result = event_coordinator
        .join()
        .map_err(|_| app_detail("Windows window event coordinator", "thread panicked"))
        .and_then(std::convert::identity);
    let router_result = router.shutdown();
    let owner_result = primary_owner.shutdown();
    let _retained_digest = guard_snapshot.verified_sha256();

    match window_result {
        Err(source @ WvError::Navigate(_)) => {
            let primary = app_detail("initial Windows navigation", source.to_string());
            Err(collapse_app_failures(
                &primary,
                [owner_result, router_result, event_result],
            ))
        }
        result => collapse_app_results([
            result.map_err(|source| app_detail("Windows app window", source.to_string())),
            event_result,
            router_result,
            owner_result,
        ]),
    }
}

#[cfg(windows)]
fn start_windows_dev_lease(
    router: PrimaryRouterHandle,
    shutdown: SessionShutdownState,
) -> Result<Option<JoinHandle<()>>, HostAppError> {
    use std::ffi::OsStr;

    let Some(value) = std::env::var_os(DEV_LEASE_ENV) else {
        return Ok(None);
    };
    if value != OsStr::new(DEV_LEASE_STDIN_V1) {
        return Err(app_detail(
            "Windows dev-host lease",
            format!(
                "unsupported {DEV_LEASE_ENV} value `{}`",
                value.to_string_lossy()
            ),
        ));
    }
    let handle = thread::Builder::new()
        .name("keld-core-windows-dev-lease".to_owned())
        .spawn(move || {
            let input = io::stdin();
            let mut input = input.lock();
            let mut buffer = [0_u8; 8 * 1024];
            loop {
                match input.read(&mut buffer) {
                    Ok(0) => {
                        if shutdown.claim_cli_lease_lost() {
                            let _ = router.cli_lease_lost();
                        }
                        return;
                    }
                    Ok(_) => {}
                    Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => {
                        let _ = router.window_commands.send(AppWindowCommand::Fatal);
                        return;
                    }
                }
            }
        })
        .map_err(|source| app_io("Windows dev-host lease reader", &source))?;
    Ok(Some(handle))
}

#[cfg(windows)]
fn await_windows_bound_generation(
    supervisor: &PrimaryRoleSupervisor,
    recovery: &PrimaryRecoveryGate,
    deadline: Instant,
) -> Result<BoundPrimaryGeneration, HostAppError> {
    loop {
        if let Some(bound) = supervisor.try_recv_bound_generation() {
            return Ok(bound);
        }
        while let Some(event) = supervisor.try_recv_event() {
            if matches!(event, PrimaryRoleEvent::Revoked { .. }) {
                let _ = recovery.deny();
                return Err(app_detail(
                    "initial Windows app-link authentication",
                    "Bun terminated before its initial authenticated generation bound",
                ));
            }
        }
        if let Some(outcome) = supervisor.try_wait_for_outcome() {
            let _ = recovery.deny();
            return Err(match outcome {
                keld_runtime::SupervisorOutcome::Stopped => app_detail(
                    "initial Windows app-link authentication",
                    "Bun stopped before its initial authenticated generation bound",
                ),
                keld_runtime::SupervisorOutcome::CrashLoop(error)
                | keld_runtime::SupervisorOutcome::Failed(error) => {
                    app_runtime("initial Windows app-link authentication", &error)
                }
            });
        }
        if Instant::now() >= deadline {
            let _ = recovery.deny();
            return Err(app_detail(
                "initial Windows app-link authentication",
                "Bun did not authenticate before the generation deadline",
            ));
        }
        thread::park_timeout(Duration::from_millis(10));
    }
}

#[cfg(target_os = "macos")]
fn finish_guarded_session<T>(
    guard_snapshot: Option<&GuardSnapshot>,
    cleanup: impl FnOnce(Option<&GuardSnapshot>) -> T,
) -> T {
    let result = cleanup(guard_snapshot);
    // Reading the verified identity after cleanup makes the retention order a
    // compile-checked part of the borrowed session lifetime rather than an
    // incidental use before cleanup.
    let _retained_digest = guard_snapshot.map(|snapshot| snapshot.verified.verified_sha256());
    result
}

#[cfg(target_os = "macos")]
fn await_bound_generation(
    guardian: &mut GuardedPrimary,
    deadline: Instant,
    phase: &'static str,
) -> Result<BoundPrimaryGeneration, HostAppError> {
    loop {
        let now = Instant::now();
        if now >= deadline {
            guardian.deny_recovery();
            return Err(app_detail(
                phase,
                "Bun did not authenticate before the generation deadline",
            ));
        }
        if let Some(update) = guardian.recv_update(deadline.saturating_duration_since(now)) {
            match update {
                GuardedPrimaryUpdate::Bound(bound) => return Ok(bound),
                GuardedPrimaryUpdate::Role(PrimaryRoleEvent::Revoked { .. }) => {
                    guardian.deny_recovery();
                    return Err(app_detail(
                        phase,
                        "Bun terminated before its initial authenticated generation bound",
                    ));
                }
                GuardedPrimaryUpdate::Role(_) => {}
            }
        } else {
            guardian
                .poll_fatal()
                .map_err(|source| app_guardian_fatal(phase, &source))?;
        }
    }
}

#[cfg(any(target_os = "macos", windows))]
fn coordinate_window_events(
    events: &Receiver<AppWindowEvent>,
    router: &PrimaryRouterHandle,
    commands: &Sender<AppWindowCommand>,
) -> Result<(), HostAppError> {
    while let Ok(event) = events.recv() {
        let result = match event {
            AppWindowEvent::NavigationReady => router.signal_ready(),
            AppWindowEvent::LastWindowClosed => router.signal_last_window_closed(),
        };
        if let Err(error) = result {
            let _ = commands.send(AppWindowCommand::Fatal);
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
struct GuardianOwner {
    command_tx: Sender<GuardianOwnerCommand>,
    handle: Option<JoinHandle<Result<ExitStatus, HostAppError>>>,
    lease_shutdown: Option<JoinHandle<()>>,
}

#[cfg(target_os = "macos")]
enum GuardianOwnerCommand {
    AttachRouter(
        PrimaryRouterHandle,
        std::sync::mpsc::SyncSender<Result<(), String>>,
    ),
    ArmRecovery(std::sync::mpsc::SyncSender<Result<(), String>>),
    DenyRecovery,
    FailGeneration(u32, std::sync::mpsc::SyncSender<Result<(), String>>),
    PrepareAcceptedShutdown(std::sync::mpsc::SyncSender<Result<(), String>>),
    Shutdown(std::sync::mpsc::SyncSender<Result<(), String>>),
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct GuardianOwnerHandle {
    command_tx: Sender<GuardianOwnerCommand>,
}

#[cfg(target_os = "macos")]
impl GuardianOwner {
    #[allow(clippy::too_many_lines)] // one thread serializes guardian commands, updates, lease loss and fatal observation
    fn start(
        mut guardian: GuardedPrimary,
        window_commands: Sender<AppWindowCommand>,
        mut dev_lease: Option<DevHostLease>,
        shutdown: SessionShutdownState,
    ) -> Result<Self, HostAppError> {
        let (command_tx, command_rx) = mpsc::channel();
        let (lease_tx, lease_rx) = mpsc::channel::<PrimaryRouterHandle>();
        let fatal_commands = window_commands.clone();
        let lease_shutdown = thread::Builder::new()
            .name("keld-core-cli-lease-shutdown".to_owned())
            .spawn(move || {
                if let Ok(router) = lease_rx.recv()
                    && router.cli_lease_lost().is_err()
                {
                    let _ = fatal_commands.send(AppWindowCommand::Fatal);
                }
            })
            .map_err(|source| app_io("CLI lease-loss shutdown owner", &source))?;
        let handle = thread::Builder::new()
            .name("keld-core-guardian-owner".to_owned())
            .spawn(move || {
                let mut router = None;
                loop {
                    match command_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(GuardianOwnerCommand::AttachRouter(attached, reply)) => {
                            let observed = if router.is_some() {
                                Err(String::from("router is already attached"))
                            } else {
                                router = Some(attached);
                                Ok(())
                            };
                            let _ = reply.send(observed);
                        }
                        Ok(GuardianOwnerCommand::PrepareAcceptedShutdown(reply)) => {
                            let result = guardian.accept_shutdown().map_err(|source| {
                                app_runtime("guardian accepted-shutdown preparation", &source)
                            });
                            let observed = match &result {
                                Ok(()) => Ok(()),
                                Err(error) => Err(error.to_string()),
                            };
                            let _ = reply.send(observed);
                            result?;
                        }
                        Ok(GuardianOwnerCommand::ArmRecovery(reply)) => {
                            guardian.arm_recovery();
                            let _ = reply.send(Ok(()));
                        }
                        Ok(GuardianOwnerCommand::DenyRecovery) => {
                            guardian.deny_recovery();
                        }
                        Ok(GuardianOwnerCommand::FailGeneration(attempt, reply)) => {
                            let result = guardian
                                .fail_current_generation(attempt)
                                .map_err(|source| app_runtime("primary app-link failure", &source));
                            let observed = result
                                .as_ref()
                                .copied()
                                .map_err(std::string::ToString::to_string);
                            let _ = reply.send(observed);
                            if let Err(error) = result {
                                let _ = window_commands.send(AppWindowCommand::Fatal);
                                return Err(error);
                            }
                        }
                        Ok(GuardianOwnerCommand::Shutdown(reply)) => {
                            let result = guardian
                                .shutdown()
                                .map_err(|source| app_guardian_fatal("guardian shutdown", &source));
                            let observed = result
                                .as_ref()
                                .map(|_| ())
                                .map_err(std::string::ToString::to_string);
                            let _ = reply.send(observed);
                            return result;
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            while let Some(update) = guardian.recv_update(Duration::ZERO) {
                                let Some(router) = &router else {
                                    if matches!(&update, GuardedPrimaryUpdate::Role(_))
                                        && !matches!(
                                            &update,
                                            GuardedPrimaryUpdate::Role(
                                                PrimaryRoleEvent::Revoked { .. }
                                            )
                                        )
                                    {
                                        continue;
                                    }
                                    let _ = window_commands.send(AppWindowCommand::Fatal);
                                    return Err(app_detail(
                                        "guardian generation update",
                                        "generation changed before the primary router attached",
                                    ));
                                };
                                if let Err(error) = router.apply_generation_update(update.into()) {
                                    guardian.deny_recovery();
                                    let _ = window_commands.send(AppWindowCommand::Fatal);
                                    return Err(error);
                                }
                            }
                            if let Some(lease) = dev_lease.as_mut() {
                                match lease.poll_lost() {
                                    Ok(true) => {
                                        if shutdown.claim_cli_lease_lost() {
                                            let Some(router) = &router else {
                                                let _ =
                                                    window_commands.send(AppWindowCommand::Fatal);
                                                return Err(app_detail(
                                                    "CLI lease loss",
                                                    "primary router is not attached",
                                                ));
                                            };
                                            lease_tx.send(router.clone()).map_err(|_| {
                                                let _ =
                                                    window_commands.send(AppWindowCommand::Fatal);
                                                app_detail(
                                                    "CLI lease-loss shutdown owner",
                                                    "shutdown executor stopped",
                                                )
                                            })?;
                                        }
                                    }
                                    Ok(false) => {}
                                    Err(error) => {
                                        let _ = window_commands.send(AppWindowCommand::Fatal);
                                        return Err(error);
                                    }
                                }
                            }
                            if shutdown.cause() == SESSION_CLI_LEASE_LOST {
                                continue;
                            }
                            if let Err(source) = guardian.poll_fatal() {
                                let _ = window_commands.send(AppWindowCommand::Fatal);
                                return Err(app_guardian_fatal("guardian watcher", &source));
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            return guardian.shutdown().map_err(|source| {
                                app_guardian_fatal("guardian owner disconnect", &source)
                            });
                        }
                    }
                }
            })
            .map_err(|source| app_io("guardian owner", &source))?;
        Ok(Self {
            command_tx,
            handle: Some(handle),
            lease_shutdown: Some(lease_shutdown),
        })
    }

    fn handle(&self) -> GuardianOwnerHandle {
        GuardianOwnerHandle {
            command_tx: self.command_tx.clone(),
        }
    }

    fn shutdown(mut self) -> Result<(), HostAppError> {
        let request = self.handle().shutdown_and_wait();
        let joined = self.join();
        joined.and(request)
    }

    fn attach_router(&self, router: PrimaryRouterHandle) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(GuardianOwnerCommand::AttachRouter(router, reply_tx))
            .map_err(|_| app_detail("guardian router attachment", "guardian owner stopped"))?;
        match reply_rx.recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(detail)) => Err(app_detail("guardian router attachment", detail)),
            Err(RecvTimeoutError::Timeout) => Err(app_detail(
                "guardian router attachment",
                "guardian owner did not acknowledge the router",
            )),
            Err(RecvTimeoutError::Disconnected) => Err(app_detail(
                "guardian router attachment",
                "guardian owner ended before attaching the router",
            )),
        }
    }

    fn join(&mut self) -> Result<(), HostAppError> {
        let owner = self.handle.take().map_or(Ok(()), |handle| {
            handle
                .join()
                .map_err(|_| app_detail("guardian owner", "thread panicked"))?
                .map(|_| ())
        });
        let lease = self.lease_shutdown.take().map_or(Ok(()), |handle| {
            handle
                .join()
                .map_err(|_| app_detail("CLI lease-loss shutdown owner", "thread panicked"))
        });
        owner.and(lease)
    }
}

#[cfg(target_os = "macos")]
impl Drop for GuardianOwner {
    fn drop(&mut self) {
        if self.handle.is_none() {
            return;
        }
        let _ = self.handle().shutdown_and_wait();
        let _ = self.join();
    }
}

#[cfg(target_os = "macos")]
impl GuardianOwnerHandle {
    fn deny_recovery(&self) {
        let _ = self.command_tx.send(GuardianOwnerCommand::DenyRecovery);
    }

    fn arm_recovery(&self) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(GuardianOwnerCommand::ArmRecovery(reply_tx))
            .map_err(|_| app_detail("primary recovery arm", "guardian owner stopped"))?;
        match reply_rx.recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(detail)) => Err(app_detail("primary recovery arm", detail)),
            Err(RecvTimeoutError::Timeout) => Err(app_detail(
                "primary recovery arm",
                "guardian owner did not acknowledge recovery activation",
            )),
            Err(RecvTimeoutError::Disconnected) => Err(app_detail(
                "primary recovery arm",
                "guardian owner ended before recovery activation",
            )),
        }
    }

    fn fail_generation(&self, attempt: u32) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(GuardianOwnerCommand::FailGeneration(attempt, reply_tx))
            .map_err(|_| app_detail("primary app-link failure", "guardian owner stopped"))?;
        match reply_rx.recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(detail)) => Err(app_detail("primary app-link failure", detail)),
            Err(RecvTimeoutError::Timeout) => Err(app_detail(
                "primary app-link failure",
                "guardian owner did not acknowledge link failure",
            )),
            Err(RecvTimeoutError::Disconnected) => Err(app_detail(
                "primary app-link failure",
                "guardian owner ended before acknowledging link failure",
            )),
        }
    }

    fn prepare_accepted_shutdown(&self) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(GuardianOwnerCommand::PrepareAcceptedShutdown(reply_tx))
            .map_err(|_| {
                app_detail(
                    "guardian accepted-shutdown preparation",
                    "guardian owner stopped",
                )
            })?;
        match reply_rx.recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(detail)) => Err(app_detail("guardian accepted-shutdown preparation", detail)),
            Err(RecvTimeoutError::Timeout) => Err(app_detail(
                "guardian accepted-shutdown preparation",
                "guardian did not acknowledge Quit before the owner deadline",
            )),
            Err(RecvTimeoutError::Disconnected) => Err(app_detail(
                "guardian accepted-shutdown preparation",
                "guardian owner ended before acknowledging Quit",
            )),
        }
    }

    fn shutdown_and_wait(&self) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        if self
            .command_tx
            .send(GuardianOwnerCommand::Shutdown(reply_tx))
            .is_err()
        {
            return Ok(());
        }
        match reply_rx.recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE) {
            Ok(Ok(())) | Err(RecvTimeoutError::Disconnected) => Ok(()),
            Ok(Err(detail)) => Err(app_detail("guardian shutdown", detail)),
            Err(RecvTimeoutError::Timeout) => Err(app_detail(
                "guardian shutdown",
                "guardian owner exceeded the shutdown reply deadline",
            )),
        }
    }
}

#[cfg(windows)]
const WINDOWS_PRIMARY_OWNER_POLL: Duration = Duration::from_millis(10);
#[cfg(windows)]
const WINDOWS_PRIMARY_OWNER_REPLY_DEADLINE: Duration = Duration::from_secs(6);

#[cfg(windows)]
struct WindowsPrimaryOwnerHandle {
    command_tx: Sender<WindowsPrimaryOwnerCommand>,
}

#[cfg(windows)]
impl Clone for WindowsPrimaryOwnerHandle {
    fn clone(&self) -> Self {
        Self {
            command_tx: self.command_tx.clone(),
        }
    }
}

#[cfg(windows)]
enum WindowsPrimaryOwnerCommand {
    AttachRouter(PrimaryRouterHandle, mpsc::SyncSender<Result<(), String>>),
    ArmRecovery(mpsc::SyncSender<Result<(), String>>),
    DenyRecovery,
    FailGeneration(u32, mpsc::SyncSender<Result<(), String>>),
    PrepareAcceptedShutdown(mpsc::SyncSender<Result<(), String>>),
    Shutdown(mpsc::SyncSender<Result<(), String>>),
}

#[cfg(windows)]
struct WindowsPrimaryOwner {
    command_tx: Sender<WindowsPrimaryOwnerCommand>,
    handle: Option<JoinHandle<Result<(), HostAppError>>>,
}

#[cfg(windows)]
impl WindowsPrimaryOwner {
    #[allow(clippy::too_many_lines)] // one owner loop keeps event/bound fan-in, recovery decisions, shutdown and terminal outcome causally ordered
    fn start(
        supervisor: PrimaryRoleSupervisor,
        recovery: PrimaryRecoveryGate,
        window_commands: Sender<AppWindowCommand>,
        shutdown: SessionShutdownState,
    ) -> Result<Self, HostAppError> {
        let (command_tx, command_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("keld-core-windows-primary-owner".to_owned())
            .spawn(move || {
                let mut router: Option<PrimaryRouterHandle> = None;
                loop {
                    // These are separate channels, so preserve the generation
                    // owner's causal order explicitly at the fan-in: revoke
                    // authority before installing a queued successor stream.
                    while let Some(event) = supervisor.try_recv_event() {
                        if let Some(router) = router.as_ref()
                            && let Err(error) =
                                router.apply_generation_update(PrimaryOwnerUpdate::Role(event))
                        {
                            let _ = recovery.deny();
                            supervisor.shutdown();
                            let _ = window_commands.send(AppWindowCommand::Fatal);
                            return Err(error);
                        }
                    }
                    while let Some(bound) = supervisor.try_recv_bound_generation() {
                        let Some(router) = router.as_ref() else {
                            let error = app_detail(
                                "Windows primary owner",
                                "successor bound before the app router was attached",
                            );
                            let _ = recovery.deny();
                            supervisor.shutdown();
                            let _ = window_commands.send(AppWindowCommand::Fatal);
                            return Err(error);
                        };
                        if let Err(error) =
                            router.apply_generation_update(PrimaryOwnerUpdate::Bound(bound))
                        {
                            let _ = recovery.deny();
                            supervisor.shutdown();
                            let _ = window_commands.send(AppWindowCommand::Fatal);
                            return Err(error);
                        }
                    }
                    if let Some(outcome) = supervisor.try_wait_for_outcome() {
                        if shutdown.is_running() {
                            let _ = window_commands.send(AppWindowCommand::Fatal);
                        }
                        return match outcome {
                            keld_runtime::SupervisorOutcome::Stopped => Ok(()),
                            keld_runtime::SupervisorOutcome::CrashLoop(error)
                            | keld_runtime::SupervisorOutcome::Failed(error) => {
                                Err(app_runtime("Windows primary supervisor", &error))
                            }
                        };
                    }
                    match command_rx.recv_timeout(WINDOWS_PRIMARY_OWNER_POLL) {
                        Ok(WindowsPrimaryOwnerCommand::AttachRouter(attached, reply)) => {
                            let result = if router.is_some() {
                                Err(String::from("primary router was already attached"))
                            } else {
                                router = Some(attached);
                                Ok(())
                            };
                            let _ = reply.send(result);
                        }
                        Ok(WindowsPrimaryOwnerCommand::ArmRecovery(reply)) => {
                            let result = recovery
                                .arm()
                                .then_some(())
                                .ok_or_else(|| String::from("recovery was already denied"));
                            let _ = reply.send(result);
                        }
                        Ok(WindowsPrimaryOwnerCommand::DenyRecovery) => {
                            let _ = recovery.deny();
                        }
                        Ok(WindowsPrimaryOwnerCommand::FailGeneration(attempt, reply)) => {
                            // The Supervisor is the sole process/restart owner.
                            // Reject a reader's stale request after a natural
                            // exit installed a successor; the worker also
                            // matches the attempt before classifying a live
                            // child as host-requested restart.
                            if router
                                .as_ref()
                                .is_some_and(|router| router.is_current(attempt))
                            {
                                supervisor.restart_generation(attempt);
                            }
                            let _ = reply.send(Ok(()));
                        }
                        Ok(WindowsPrimaryOwnerCommand::PrepareAcceptedShutdown(reply)) => {
                            let _ = recovery.deny();
                            let _ = reply.send(Ok(()));
                        }
                        Ok(WindowsPrimaryOwnerCommand::Shutdown(reply)) => {
                            let _ = recovery.deny();
                            supervisor.shutdown();
                            let result = match supervisor.wait_for_outcome() {
                                keld_runtime::SupervisorOutcome::Stopped => Ok(()),
                                keld_runtime::SupervisorOutcome::CrashLoop(error)
                                | keld_runtime::SupervisorOutcome::Failed(error) => {
                                    Err(error.to_string())
                                }
                            };
                            let _ = reply.send(result.clone());
                            return result
                                .map_err(|detail| app_detail("Windows primary shutdown", detail));
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => {
                            let _ = recovery.deny();
                            supervisor.shutdown();
                            return match supervisor.wait_for_outcome() {
                                keld_runtime::SupervisorOutcome::Stopped => Ok(()),
                                keld_runtime::SupervisorOutcome::CrashLoop(error)
                                | keld_runtime::SupervisorOutcome::Failed(error) => {
                                    Err(app_runtime("Windows primary owner", &error))
                                }
                            };
                        }
                    }
                }
            })
            .map_err(|source| app_io("Windows primary owner", &source))?;
        Ok(Self {
            command_tx,
            handle: Some(handle),
        })
    }

    fn handle(&self) -> WindowsPrimaryOwnerHandle {
        WindowsPrimaryOwnerHandle {
            command_tx: self.command_tx.clone(),
        }
    }

    fn attach_router(&self, router: PrimaryRouterHandle) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(WindowsPrimaryOwnerCommand::AttachRouter(router, reply_tx))
            .map_err(|_| app_detail("Windows primary router attachment", "owner stopped"))?;
        receive_windows_owner_reply(&reply_rx, "Windows primary router attachment")
    }

    fn shutdown(mut self) -> Result<(), HostAppError> {
        let requested = self.handle().shutdown_and_wait();
        let joined = self.join();
        joined.and(requested)
    }

    fn join(&mut self) -> Result<(), HostAppError> {
        self.handle.take().map_or(Ok(()), |handle| {
            handle
                .join()
                .map_err(|_| app_detail("Windows primary owner", "thread panicked"))?
        })
    }
}

#[cfg(windows)]
impl Drop for WindowsPrimaryOwner {
    fn drop(&mut self) {
        if self.handle.is_none() {
            return;
        }
        let _ = self.handle().shutdown_and_wait();
        let _ = self.join();
    }
}

#[cfg(windows)]
impl WindowsPrimaryOwnerHandle {
    fn deny_recovery(&self) {
        let _ = self
            .command_tx
            .send(WindowsPrimaryOwnerCommand::DenyRecovery);
    }

    fn arm_recovery(&self) -> Result<(), HostAppError> {
        self.request(
            WindowsPrimaryOwnerCommand::ArmRecovery,
            "Windows primary recovery arm",
        )
    }

    fn fail_generation(&self, attempt: u32) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(WindowsPrimaryOwnerCommand::FailGeneration(
                attempt, reply_tx,
            ))
            .map_err(|_| app_detail("Windows primary app-link failure", "owner stopped"))?;
        receive_windows_owner_reply(&reply_rx, "Windows primary app-link failure")
    }

    fn prepare_accepted_shutdown(&self) -> Result<(), HostAppError> {
        self.request(
            WindowsPrimaryOwnerCommand::PrepareAcceptedShutdown,
            "Windows accepted-shutdown preparation",
        )
    }

    fn shutdown_and_wait(&self) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        if self
            .command_tx
            .send(WindowsPrimaryOwnerCommand::Shutdown(reply_tx))
            .is_err()
        {
            return Ok(());
        }
        receive_windows_owner_reply(&reply_rx, "Windows primary shutdown")
    }

    fn request(
        &self,
        command: impl FnOnce(mpsc::SyncSender<Result<(), String>>) -> WindowsPrimaryOwnerCommand,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(command(reply_tx))
            .map_err(|_| app_detail(phase, "owner stopped"))?;
        receive_windows_owner_reply(&reply_rx, phase)
    }
}

#[cfg(windows)]
fn receive_windows_owner_reply(
    reply_rx: &Receiver<Result<(), String>>,
    phase: &'static str,
) -> Result<(), HostAppError> {
    match reply_rx.recv_timeout(WINDOWS_PRIMARY_OWNER_REPLY_DEADLINE) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(detail)) => Err(app_detail(phase, detail)),
        Err(RecvTimeoutError::Timeout) => Err(app_detail(
            phase,
            "owner did not acknowledge before deadline",
        )),
        Err(RecvTimeoutError::Disconnected) => {
            Err(app_detail(phase, "owner ended before acknowledgment"))
        }
    }
}

#[cfg(any(target_os = "macos", windows))]
enum PrimaryOwnerUpdate {
    Role(PrimaryRoleEvent),
    Bound(BoundPrimaryGeneration),
}

#[cfg(target_os = "macos")]
impl From<GuardedPrimaryUpdate> for PrimaryOwnerUpdate {
    fn from(update: GuardedPrimaryUpdate) -> Self {
        match update {
            GuardedPrimaryUpdate::Role(event) => Self::Role(event),
            GuardedPrimaryUpdate::Bound(bound) => Self::Bound(bound),
        }
    }
}

#[cfg(target_os = "macos")]
type PlatformPrimaryOwnerHandle = GuardianOwnerHandle;
#[cfg(windows)]
type PlatformPrimaryOwnerHandle = WindowsPrimaryOwnerHandle;

#[cfg(any(target_os = "macos", windows))]
#[derive(Clone)]
struct PrimaryRouterHandle {
    current: Arc<Mutex<Option<ActivePrimaryGeneration>>>,
    readers: Arc<Mutex<HashMap<u32, PrimaryReader>>>,
    window_ready: Arc<AtomicBool>,
    last_window_closed: Arc<AtomicBool>,
    recovery_armed: Arc<AtomicBool>,
    shutdown: SessionShutdownState,
    guardian: PlatformPrimaryOwnerHandle,
    window_commands: Sender<AppWindowCommand>,
}

#[cfg(any(target_os = "macos", windows))]
type PrimaryReader = JoinHandle<Result<(), HostAppError>>;

#[cfg(any(target_os = "macos", windows))]
struct ActivePrimaryGeneration {
    attempt: u32,
    writer: BootstrapStream,
}

#[cfg(any(target_os = "macos", windows))]
impl PrimaryRouterHandle {
    fn signal_ready(&self) -> Result<(), HostAppError> {
        if self.recovery_armed.load(Ordering::Acquire) {
            self.write_event(LifecycleEvent::Ready)?;
            self.window_ready.store(true, Ordering::Release);
            return Ok(());
        }
        if let Err(error) = self.write_event(LifecycleEvent::Ready) {
            self.guardian.deny_recovery();
            return Err(error);
        }
        // Ready is now externally observable. Mark that fact before waiting
        // for the guardian-owner acknowledgment; runtime's Pending decision
        // blocks successor preparation during this interval.
        self.window_ready.store(true, Ordering::Release);
        if let Err(error) = self.guardian.arm_recovery() {
            self.guardian.deny_recovery();
            return Err(error);
        }
        self.recovery_armed.store(true, Ordering::Release);
        Ok(())
    }

    fn signal_last_window_closed(&self) -> Result<(), HostAppError> {
        self.last_window_closed.store(true, Ordering::Release);
        self.write_event(LifecycleEvent::LastWindowClosed)
    }

    fn write_event(&self, event: LifecycleEvent) -> Result<(), HostAppError> {
        let payload = encode(&event).map_err(|source| app_ipc("lifecycle event", &source))?;
        let _transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("primary session generation", "generation lock poisoned"))?;
        if !self.shutdown.is_running() {
            return Ok(());
        }
        let Some(active) = current.as_mut() else {
            return Ok(());
        };
        write_frame(
            &mut active.writer,
            FrameKind::Event,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            &payload,
        )
        .map_err(|source| app_ipc("lifecycle event", &source))
    }

    fn lifecycle_quit(
        &self,
        attempt: u32,
        correlation: CorrelationId,
        reply: &[u8],
    ) -> Result<(), HostAppError> {
        let transition = self.shutdown.transition_guard();
        let current_guard = self
            .current
            .lock()
            .map_err(|_| app_detail("primary session generation", "generation lock poisoned"))?;
        if current_guard
            .as_ref()
            .is_none_or(|active| active.attempt != attempt)
        {
            return Ok(());
        }
        if !self.shutdown.claim_guarded(SESSION_LIFECYCLE_QUIT) {
            drop(current_guard);
            drop(transition);
            return if self.shutdown.cause() == SESSION_CLI_LEASE_LOST {
                self.cli_lease_lost()
            } else {
                Ok(())
            };
        }
        if !self.shutdown.begin_tail() {
            return Ok(());
        }
        drop(current_guard);
        drop(transition);
        self.guardian.prepare_accepted_shutdown()?;
        let mut current_guard = self
            .current
            .lock()
            .map_err(|_| app_detail("primary session generation", "generation lock poisoned"))?;
        let active = current_guard.as_mut().ok_or_else(|| {
            app_detail(
                "lifecycle Quit reply",
                "current primary generation disappeared before the reply",
            )
        })?;
        write_frame(
            &mut active.writer,
            FrameKind::Reply,
            0,
            LIFECYCLE_CHANNEL,
            correlation,
            reply,
        )
        .map_err(|source| app_ipc("lifecycle Quit reply", &source))?;
        finish_link_shutdown(
            active.writer.shutdown_app_link(),
            "lifecycle Quit link close",
        )?;
        current_guard.take();
        drop(current_guard);
        self.finish_tail("lifecycle Quit")
    }

    fn cli_lease_lost(&self) -> Result<(), HostAppError> {
        if self.shutdown.cause() != SESSION_CLI_LEASE_LOST || !self.shutdown.begin_tail() {
            return Ok(());
        }
        self.guardian.prepare_accepted_shutdown()?;
        let mut current_guard = self
            .current
            .lock()
            .map_err(|_| app_detail("primary session generation", "generation lock poisoned"))?;
        if let Some(active) = current_guard.as_mut() {
            finish_link_shutdown(
                active.writer.shutdown_app_link(),
                "CLI lease-loss link close",
            )?;
        }
        current_guard.take();
        drop(current_guard);
        self.finish_tail("CLI lease loss")
    }

    fn apply_generation_update(&self, update: PrimaryOwnerUpdate) -> Result<(), HostAppError> {
        match update {
            PrimaryOwnerUpdate::Role(PrimaryRoleEvent::Revoked { attempt, .. }) => {
                if !self.window_ready.load(Ordering::Acquire) {
                    return Err(app_detail(
                        "primary generation before Ready",
                        "Bun terminated before the initial window became ready",
                    ));
                }
                self.retire_generation(attempt)
            }
            PrimaryOwnerUpdate::Role(_) => Ok(()),
            PrimaryOwnerUpdate::Bound(bound) => {
                self.install_generation(bound.attempt(), bound.into_stream())
            }
        }
    }

    fn install_generation(
        &self,
        attempt: u32,
        mut stream: BootstrapStream,
    ) -> Result<(), HostAppError> {
        stream
            .set_app_link_read_deadline(Some(APP_LINK_READER_POLL))
            .map_err(|source| app_io("primary session reader deadline", &source))?;
        let writer_stream = stream
            .try_clone()
            .map_err(|source| app_io("primary session writer clone", &source))?;
        writer_stream
            .set_app_link_write_deadline(Some(APP_LINK_IO_DEADLINE))
            .map_err(|source| app_io("primary session writer deadline", &source))?;
        {
            let _transition = self.shutdown.transition_guard();
            if !self.shutdown.is_running() {
                let _ = writer_stream.shutdown_app_link();
                return Ok(());
            }
            let mut current = self.current.lock().map_err(|_| {
                app_detail("primary session generation", "generation lock poisoned")
            })?;
            if current.is_some() {
                return Err(app_detail(
                    "primary session generation",
                    "successor bound before the retired generation was revoked",
                ));
            }
            *current = Some(ActivePrimaryGeneration {
                attempt,
                writer: writer_stream,
            });
        }
        let handle = self.clone();
        let reader = thread::Builder::new()
            .name(format!("keld-core-primary-router-{attempt}"))
            .spawn(move || {
                let mut result = read_primary_frames(&mut stream, &handle, attempt);
                if result.is_err() && !handle.is_current(attempt) {
                    result = Ok(());
                }
                if result.is_err() && handle.is_current(attempt) {
                    let _ = handle.window_commands.send(AppWindowCommand::Fatal);
                }
                result
            })
            .map_err(|source| app_io("primary session reader", &source))?;
        self.readers
            .lock()
            .map_err(|_| app_detail("primary session readers", "reader list lock poisoned"))?
            .insert(attempt, reader);
        if self.window_ready.load(Ordering::Acquire) {
            self.write_event(LifecycleEvent::Ready)?;
        }
        if self.last_window_closed.load(Ordering::Acquire) {
            self.write_event(LifecycleEvent::LastWindowClosed)?;
        }
        Ok(())
    }

    fn retire_generation(&self, attempt: u32) -> Result<(), HostAppError> {
        {
            let _transition = self.shutdown.transition_guard();
            if !self.shutdown.is_running() {
                return Ok(());
            }
            let mut current = self.current.lock().map_err(|_| {
                app_detail("primary session generation", "generation lock poisoned")
            })?;
            if current
                .as_ref()
                .is_some_and(|active| active.attempt == attempt)
                && let Some(active) = current.take()
            {
                finish_link_shutdown(
                    active.writer.shutdown_app_link(),
                    "retired primary generation link close",
                )?;
            }
        }
        // The reader may be waiting for GuardianOwner to acknowledge the
        // link-failure request that caused this revocation. Removing and
        // joining it on that same owner thread deadlocks. Current-attempt
        // checks make it inert after retirement; the terminal router owner
        // retains and joins every reader handle.
        Ok(())
    }

    fn link_failed(&self, attempt: u32) -> Result<(), HostAppError> {
        self.guardian.fail_generation(attempt)
    }

    fn is_current(&self, attempt: u32) -> bool {
        self.current.lock().map_or(true, |current| {
            current
                .as_ref()
                .is_some_and(|active| active.attempt == attempt)
        })
    }

    fn finish_tail(&self, phase: &'static str) -> Result<(), HostAppError> {
        self.guardian.shutdown_and_wait()?;
        self.window_commands
            .send(AppWindowCommand::Quit)
            .map_err(|_| app_detail(phase, "UI event loop is unavailable"))
    }
}

#[cfg(any(target_os = "macos", windows))]
struct PrimaryRouter {
    handle: PrimaryRouterHandle,
}

#[cfg(any(target_os = "macos", windows))]
impl PrimaryRouter {
    #[cfg(all(test, target_os = "macos"))]
    fn start(
        stream: BootstrapStream,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
    ) -> Result<Self, HostAppError> {
        let handle = PrimaryRouterHandle {
            current: Arc::new(Mutex::new(None)),
            readers: Arc::new(Mutex::new(HashMap::new())),
            window_ready: Arc::new(AtomicBool::new(false)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(true)),
            shutdown,
            guardian,
            window_commands,
        };
        handle.install_generation(1, stream)?;
        Ok(Self { handle })
    }

    fn start_bound(
        bound: BoundPrimaryGeneration,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
    ) -> Result<Self, HostAppError> {
        let attempt = bound.attempt();
        let stream = bound.into_stream();
        let handle = PrimaryRouterHandle {
            current: Arc::new(Mutex::new(None)),
            readers: Arc::new(Mutex::new(HashMap::new())),
            window_ready: Arc::new(AtomicBool::new(false)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(false)),
            shutdown,
            guardian,
            window_commands,
        };
        handle.install_generation(attempt, stream)?;
        Ok(Self { handle })
    }

    fn handle(&self) -> PrimaryRouterHandle {
        self.handle.clone()
    }

    fn shutdown(mut self) -> Result<(), HostAppError> {
        self.stop_and_join()
    }

    fn stop_and_join(&mut self) -> Result<(), HostAppError> {
        self.handle.shutdown.stop_reader();
        {
            let mut current = match self.handle.current.lock() {
                Ok(current) => current,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(active) = current.take() {
                let _ = active.writer.shutdown_app_link();
            }
        }
        let readers = match self.handle.readers.lock() {
            Ok(mut readers) => std::mem::take(&mut *readers),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };
        let mut results = Vec::new();
        for (_, reader) in readers {
            results.push(
                reader
                    .join()
                    .map_err(|_| app_detail("primary session reader", "thread panicked"))?,
            );
        }
        collapse_app_results(results)
    }
}

#[cfg(any(target_os = "macos", windows))]
impl Drop for PrimaryRouter {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

#[cfg(any(target_os = "macos", windows))]
#[allow(clippy::too_many_lines)] // one reader owns the complete echo/lifecycle frame dispatch
fn read_primary_frames(
    reader: &mut BootstrapStream,
    handle: &PrimaryRouterHandle,
    attempt: u32,
) -> Result<(), HostAppError> {
    loop {
        if handle.shutdown.cause() == SESSION_CLI_LEASE_LOST {
            handle.cli_lease_lost()?;
            return Ok(());
        }
        let (header, payload) = match read_frame_interruptible(reader, &handle.shutdown.reader_stop)
        {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                if handle.shutdown.cause() == SESSION_CLI_LEASE_LOST {
                    handle.cli_lease_lost()?;
                }
                return Ok(());
            }
            Err(IpcError::Io(source))
                if matches!(
                    source.kind(),
                    io::ErrorKind::UnexpectedEof
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionAborted
                ) =>
            {
                if !handle.is_current(attempt) {
                    return Ok(());
                }
                if handle.window_ready.load(Ordering::Acquire) && handle.shutdown.is_running() {
                    // The KEL-75/KEL-78 owner decides whether this generation
                    // is recoverable. Its Revoked update retires this writer
                    // before a successor is installed; only its terminal
                    // outcome may close the already-ready window.
                    handle.link_failed(attempt)?;
                    return Ok(());
                }
                return Err(app_detail(
                    "primary session reader",
                    "Bun closed the app link",
                ));
            }
            Err(source) => return Err(app_ipc("primary session reader", &source)),
        };
        if handle.shutdown.cause() == SESSION_CLI_LEASE_LOST {
            handle.cli_lease_lost()?;
            return Ok(());
        }
        match (header.kind, header.channel) {
            (FrameKind::Call, ECHO_CHANNEL) if handle.shutdown.is_running() => {
                let reply = keld_ipc::echo::handle_echo(&payload)
                    .map_err(|source| app_ipc("echo dispatch", &source))?;
                write_primary_reply(
                    &handle.current,
                    &handle.shutdown,
                    attempt,
                    ECHO_CHANNEL,
                    header.corr,
                    &reply,
                )?;
            }
            (FrameKind::Call, LIFECYCLE_CHANNEL) => {
                let request: LifecycleRequest =
                    decode(&payload).map_err(|source| app_ipc("lifecycle request", &source))?;
                match request {
                    LifecycleRequest::Quit => {
                        let reply = encode(&LifecycleResponse::Quit)
                            .map_err(|source| app_ipc("lifecycle Quit reply", &source))?;
                        handle.lifecycle_quit(attempt, header.corr, &reply)?;
                        return Ok(());
                    }
                }
            }
            (FrameKind::Ping, _) => {
                let _transition = handle.shutdown.transition_guard();
                let mut writer = handle.current.lock().map_err(|_| {
                    app_detail("primary session generation", "generation lock poisoned")
                })?;
                if !handle.shutdown.is_running() {
                    return Ok(());
                }
                let Some(active) = writer.as_mut() else {
                    return Ok(());
                };
                if active.attempt != attempt {
                    return Ok(());
                }
                write_frame(
                    &mut active.writer,
                    FrameKind::Ping,
                    0,
                    header.channel,
                    header.corr,
                    &[],
                )
                .map_err(|source| app_ipc("primary session Ping", &source))?;
            }
            (FrameKind::Call, _) if !handle.shutdown.is_running() => {
                return Err(app_detail(
                    "primary session dispatch",
                    "new Call arrived after quiesce",
                ));
            }
            (FrameKind::Call, _) => {
                return Err(app_detail(
                    "primary session dispatch",
                    "unknown Call channel",
                ));
            }
            _ => {
                return Err(app_detail(
                    "primary session dispatch",
                    "unexpected frame kind",
                ));
            }
        }
    }
}

#[cfg(any(target_os = "macos", windows))]
fn write_primary_reply(
    current: &Mutex<Option<ActivePrimaryGeneration>>,
    shutdown: &SessionShutdownState,
    attempt: u32,
    channel: keld_ipc::ChannelId,
    correlation: CorrelationId,
    payload: &[u8],
) -> Result<(), HostAppError> {
    let _transition = shutdown.transition_guard();
    let mut current = current
        .lock()
        .map_err(|_| app_detail("primary session generation", "generation lock poisoned"))?;
    if !shutdown.is_running() {
        return Ok(());
    }
    let Some(active) = current.as_mut() else {
        return Ok(());
    };
    if active.attempt != attempt {
        return Ok(());
    }
    write_frame(
        &mut active.writer,
        FrameKind::Reply,
        0,
        channel,
        correlation,
        payload,
    )
    .map_err(|source| app_ipc("primary session reply", &source))
}

#[cfg(any(target_os = "macos", windows))]
fn finish_link_shutdown(result: io::Result<()>, phase: &'static str) -> Result<(), HostAppError> {
    match result {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotConnected => Ok(()),
        Err(source) => Err(app_io(phase, &source)),
    }
}

#[cfg(any(target_os = "macos", windows))]
fn app_detail(phase: &'static str, detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-037",
        phase,
        detail,
        "Fix the app session failure and relaunch the no-flag host.",
    )
}

#[cfg(any(target_os = "macos", windows))]
fn collapse_app_failures<const N: usize>(
    primary: &HostAppError,
    cleanup: [Result<(), HostAppError>; N],
) -> HostAppError {
    let mut detail = primary.to_string();
    for error in cleanup.into_iter().filter_map(Result::err) {
        detail.push_str("; cleanup: ");
        detail.push_str(&error.to_string());
    }
    app_detail("startup cleanup", detail)
}

#[cfg(any(target_os = "macos", windows))]
fn collapse_app_results(
    results: impl IntoIterator<Item = Result<(), HostAppError>>,
) -> Result<(), HostAppError> {
    let mut errors = results.into_iter().filter_map(Result::err);
    let Some(first) = errors.next() else {
        return Ok(());
    };
    let Some(second) = errors.next() else {
        return Err(first);
    };
    let mut detail = first.to_string();
    detail.push_str("; cleanup: ");
    detail.push_str(&second.to_string());
    for error in errors {
        detail.push_str("; cleanup: ");
        detail.push_str(&error.to_string());
    }
    Err(app_detail("session cleanup", detail))
}

#[cfg(any(target_os = "macos", windows))]
fn app_io(phase: &'static str, source: &io::Error) -> HostAppError {
    HostAppError::io(
        "KELD-CORE-037",
        phase,
        source,
        "Fix the app session I/O failure and relaunch the no-flag host.",
    )
}

#[cfg(any(target_os = "macos", windows))]
fn app_runtime(phase: &'static str, source: &keld_runtime::RuntimeError) -> HostAppError {
    app_detail(phase, source.to_string())
}

#[cfg(target_os = "macos")]
fn app_guardian_fatal(phase: &'static str, source: &keld_runtime::RuntimeError) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-033",
        phase,
        source.to_string(),
        "Fix the cause named by the nested KELD-RUNTIME diagnostic, then relaunch the no-flag host.",
    )
}

#[cfg(any(target_os = "macos", windows))]
fn app_ipc(phase: &'static str, source: &IpcError) -> HostAppError {
    app_detail(phase, source.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[cfg(target_os = "macos")]
    use std::fs;
    #[cfg(target_os = "macos")]
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;

    const DIGEST: &str = "sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356";

    fn valid_boot(entry: &str, renderer: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "name": "Fixture",
            "entry": entry,
            "renderer": renderer,
            "permissions": {
                "file": "keld.permissions.jsonc",
                "content_sha256": DIGEST,
            }
        }))
        .expect("serialize fixture boot")
    }

    fn must_err<T>(result: Result<T, HostAppError>, message: &str) -> HostAppError {
        match result {
            Ok(_) => panic!("{message}"),
            Err(error) => error,
        }
    }

    #[test]
    fn strict_boot_schema_accepts_exact_v1() {
        let parsed = parse_boot_bytes(&valid_boot("src/main.ts", "index.html"))
            .expect("exact schema-v1 descriptor");
        assert_eq!(parsed.name, "Fixture");
        assert_eq!(parsed.entry, Path::new("src/main.ts"));
        assert_eq!(parsed.renderer, Path::new("index.html"));
        assert_eq!(parsed.permissions_digest.len(), 32);
    }

    #[test]
    fn bounded_boot_rejects_zero_limit_plus_one_and_non_utf8() {
        assert!(parse_boot_bytes(&[]).is_err());
        let exact = vec![b' '; MAX_BOOT_BYTES];
        let exact_error = must_err(parse_boot_bytes(&exact), "spaces are not JSON");
        assert_eq!(exact_error.code(), "KELD-CORE-035");
        let over = vec![b' '; MAX_BOOT_BYTES + 1];
        let over_error = must_err(parse_boot_bytes(&over), "64 KiB + 1 must fail");
        assert!(over_error.to_string().contains("64 KiB"), "{over_error}");
        let utf8_error = must_err(parse_boot_bytes(&[0xff]), "non-UTF-8 must fail");
        assert!(utf8_error.to_string().contains("UTF-8"), "{utf8_error}");
    }

    #[test]
    fn duplicate_unknown_version_name_and_permissions_fields_fail_closed() {
        let cases = [
            r#"{"schema":1,"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
            r#"{"schema":1,"name":"x","entry":"a","renderer":"b","unknown":1,"permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
            r#"{"schema":2,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
            r#"{"schema":1,"name":"","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
            r#"{"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"wrong.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
            r#"{"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
            r#"{"schema":1,"name":"x","entry":"a","renderer":"b","permissions":{"file":"keld.permissions.jsonc","content_sha256":"SHA256:CA3D163BAB055381827226140568F3BEF7EAAC187CEBD76878E0B63E9E442356"}}"#,
        ];
        for bytes in cases {
            assert!(
                parse_boot_bytes(bytes.as_bytes()).is_err(),
                "accepted: {bytes}"
            );
        }
    }

    #[test]
    fn portable_relative_paths_reject_normalized_escape_and_empty_components() {
        for path in [
            "",
            ".",
            "..",
            "/abs",
            "a/../b",
            "a/./b",
            "a//b",
            "a/",
            "C:\\boot.ts",
            "C:/boot.ts",
            "\\\\server\\share",
            "a\\b",
        ] {
            let error = must_err(
                parse_boot_bytes(&valid_boot(path, "index.html")),
                "unsafe entry path must fail",
            );
            assert!(error.to_string().contains("entry"), "{path}: {error}");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn validated_root_opens_same_handle_targets_and_rejects_symlink_escape() {
        let temp = tempfile::tempdir().expect("temp root");
        let root = temp.path().join("stage");
        fs::create_dir(&root).expect("stage");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
        fs::create_dir(root.join("src")).expect("src");
        fs::write(root.join("src/main.ts"), "await new Promise(() => {});\n").expect("entry");
        fs::write(root.join("index.html"), "<p id=fixture>exact</p>\n").expect("renderer");
        fs::write(root.join("keld.permissions.jsonc"), b"{}\n").expect("permissions");
        fs::write(
            root.join("keld.boot.json"),
            valid_boot("src/main.ts", "index.html"),
        )
        .expect("boot");

        let selection = validate_from_root(&root).expect("validated selection");
        assert_eq!(selection.app.renderer_html, b"<p id=fixture>exact</p>\n");
        assert_eq!(selection.app.name, "Fixture");

        let mut substituted: serde_json::Value =
            serde_json::from_slice(&valid_boot("src/main.ts", "index.html"))
                .expect("substitute boot JSON");
        substituted["name"] = serde_json::Value::String("Substituted".to_owned());
        fs::write(
            root.join("keld.boot.json"),
            serde_json::to_vec(&substituted).expect("substitute boot bytes"),
        )
        .expect("replace sidecar after selection");

        let outside = temp.path().join("outside.html");
        fs::write(&outside, "outside").expect("outside");
        fs::remove_file(root.join("index.html")).expect("remove renderer");
        symlink(&outside, root.join("index.html")).expect("escape symlink");
        assert_eq!(
            selection.app.renderer_html, b"<p id=fixture>exact</p>\n",
            "post-selection renderer substitution changed consumed bytes"
        );
        assert_eq!(
            selection.app.name, "Fixture",
            "post-selection sidecar substitution changed owned fields"
        );
        let error = must_err(validate_from_root(&root), "symlink escape must fail");
        assert!(error.to_string().contains("renderer"), "{error}");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn guarded_preflight_preserves_read_error_before_resources() {
        use std::fs::OpenOptions;

        let temp = tempfile::tempdir().expect("temp root");
        let root = temp.path().join("stage");
        fs::create_dir(&root).expect("stage");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
        fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
        fs::write(root.join("index.html"), "<p>guarded</p>\n").expect("renderer");
        fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
        fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");

        let mut selection = validate_from_root(&root).expect("validated selection");
        selection.permissions_file = OpenOptions::new()
            .write(true)
            .open(root.join(PERMISSIONS_FILE))
            .expect("write-only retained handle");
        let before = startup_resource_snapshot();

        let error = run_guarded(selection).expect_err("read failure must fail preflight");
        assert_eq!(error.code(), "KELD-GUARD004");
        assert_eq!(error.resources, before, "preflight advanced app resources");
        let message = error.to_string();
        assert!(message.contains("Check the path"), "{message}");
        assert!(
            message.contains("rebuild the staged boot artifact"),
            "{message}"
        );
        let source = std::error::Error::source(&error).expect("manifest source");
        assert!(source.downcast_ref::<ManifestError>().is_some());
        assert_eq!(
            source.to_string(),
            error.manifest_source.as_ref().unwrap().to_string()
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn guard_snapshot_drops_only_after_ordered_cleanup() {
        let temp = tempfile::tempdir().expect("temp root");
        let root = temp.path().join("stage");
        fs::create_dir(&root).expect("stage");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
        fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
        fs::write(root.join("index.html"), "<p>guarded</p>\n").expect("renderer");
        fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
        fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");
        let ValidatedBootSelection {
            app,
            permissions_file,
            permissions_digest,
        } = validate_from_root(&root).expect("validated selection");
        let verified = load_verified_manifest(
            permissions_file,
            root.join(PERMISSIONS_FILE),
            permissions_digest,
        )
        .expect("verified manifest");
        drop(app);
        let dropped = Arc::new(AtomicBool::new(false));
        let snapshot = GuardSnapshot {
            verified,
            drop_observer: Some(Arc::clone(&dropped)),
        };

        let digest = finish_guarded_session(Some(&snapshot), |live| {
            assert!(
                !dropped.load(Ordering::Acquire),
                "snapshot dropped before cleanup"
            );
            live.expect("guarded cleanup receives the snapshot")
                .verified
                .verified_sha256()
        });

        assert_eq!(digest, permissions_digest);
        assert!(
            !dropped.load(Ordering::Acquire),
            "borrowed cleanup helper destroyed the outer session owner"
        );
        drop(snapshot);
        assert!(
            dropped.load(Ordering::Acquire),
            "outer session owner did not destroy the snapshot after cleanup"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn boot_sidecar_symlink_is_rejected_before_target_resolution() {
        let temp = tempfile::tempdir().expect("temp root");
        let root = temp.path().join("stage");
        fs::create_dir(&root).expect("stage");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
        let outside = temp.path().join("outside-boot.json");
        fs::write(&outside, valid_boot("src/main.ts", "index.html")).expect("outside boot");
        symlink(&outside, root.join("keld.boot.json")).expect("boot symlink");

        let error = must_err(validate_from_root(&root), "boot symlink must fail");
        assert!(error.to_string().contains("boot descriptor"), "{error}");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn descriptor_limit_does_not_narrow_renderer_size() {
        let temp = tempfile::tempdir().expect("temp root");
        let root = temp.path().join("stage");
        fs::create_dir(&root).expect("stage");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
        fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
        let renderer = vec![b'x'; MAX_BOOT_BYTES + 1];
        fs::write(root.join("index.html"), &renderer).expect("large renderer");
        fs::write(root.join("keld.permissions.jsonc"), b"{}\n").expect("permissions");
        fs::write(
            root.join("keld.boot.json"),
            valid_boot("main.ts", "index.html"),
        )
        .expect("boot");

        let selection = validate_from_root(&root).expect("large renderer is valid");
        assert_eq!(selection.app.renderer_html, renderer);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn dev_lease_reader_is_nonblocking_cloexec_and_read_only() {
        let (reader, writer) = nix::unistd::pipe().expect("lease pipe");
        configure_dev_lease_fd(&reader).expect("configure lease reader");
        let descriptor = FdFlag::from_bits_truncate(
            fcntl(&reader, FcntlArg::F_GETFD).expect("lease descriptor flags"),
        );
        let status = OFlag::from_bits_truncate(
            fcntl(&reader, FcntlArg::F_GETFL).expect("lease status flags"),
        );
        assert!(descriptor.contains(FdFlag::FD_CLOEXEC));
        assert!(status.contains(OFlag::O_NONBLOCK));
        assert!((status & OFlag::O_ACCMODE).is_empty());

        let error = configure_dev_lease_fd(&writer).expect_err("writer is not a lease reader");
        assert!(error.to_string().contains("read-only end"), "{error}");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn dev_lease_data_drain_yields_after_a_fixed_work_budget() {
        struct EndlessReader {
            reads: usize,
        }

        impl std::io::Read for EndlessReader {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                self.reads += 1;
                bytes.fill(b'x');
                Ok(bytes.len())
            }
        }

        let mut reader = EndlessReader { reads: 0 };
        assert!(!poll_dev_lease_reader(&mut reader).expect("bounded data drain"));
        assert_eq!(reader.reads, DEV_LEASE_DRAIN_READS);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn stage_directory_mode_is_mandatory() {
        let temp = tempfile::tempdir().expect("temp root");
        let root = temp.path().join("stage");
        fs::create_dir(&root).expect("stage");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o1700)).expect("wrong mode");
        let mode_error = must_err(validate_from_root(&root), "mode must be exact 0700");
        assert!(mode_error.to_string().contains("0o700"), "{mode_error}");
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[allow(clippy::too_many_lines)] // one test preserves the full Ready/calls/Quit ordering oracle
    fn one_router_carries_ready_two_echo_calls_and_ordered_quit() {
        use std::io::Read as _;
        use std::os::unix::net::UnixStream;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, mpsc};
        use std::time::Duration;

        use keld_ipc::codec::{decode, encode};
        use keld_ipc::link::{AppLinkDeadlines, read_frame};

        let (server, mut client) = UnixStream::pair().expect("session pair");
        client
            .set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("client deadlines");
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let (eof_tx, eof_rx) = mpsc::channel();
        let guardian_prepared = Arc::new(AtomicBool::new(false));
        let guardian_prepared_in_thread = Arc::clone(&guardian_prepared);
        let guardian_finished = Arc::new(AtomicBool::new(false));
        let guardian_finished_in_thread = Arc::clone(&guardian_finished);
        let guardian_thread = std::thread::spawn(move || {
            let prepare_reply = match guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("guardian Quit preparation")
            {
                GuardianOwnerCommand::PrepareAcceptedShutdown(reply) => reply,
                GuardianOwnerCommand::Shutdown(_) => panic!("Quit reply lacked preparation"),
                GuardianOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
                GuardianOwnerCommand::FailGeneration(_, _) => panic!("unexpected link failure"),
                GuardianOwnerCommand::ArmRecovery(_) => panic!("unexpected recovery arm"),
                GuardianOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
            };
            guardian_prepared_in_thread.store(true, Ordering::Release);
            prepare_reply
                .send(Ok(()))
                .expect("guardian preparation reply");
            let shutdown_reply = match guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("guardian shutdown request")
            {
                GuardianOwnerCommand::Shutdown(reply) => reply,
                GuardianOwnerCommand::PrepareAcceptedShutdown(_) => panic!("duplicate preparation"),
                GuardianOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
                GuardianOwnerCommand::FailGeneration(_, _) => panic!("unexpected link failure"),
                GuardianOwnerCommand::ArmRecovery(_) => panic!("unexpected recovery arm"),
                GuardianOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
            };
            eof_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("link EOF before guardian reap");
            guardian_finished_in_thread.store(true, Ordering::Release);
            shutdown_reply
                .send(Ok(()))
                .expect("guardian shutdown reply");
        });
        let router = PrimaryRouter::start(
            server,
            window_tx,
            GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
        )
        .expect("primary router");

        router.handle().signal_ready().expect("Ready");
        assert_lifecycle_event(&mut client, LifecycleEvent::Ready);

        for (correlation, message) in [(41_u32, "first"), (42_u32, "second")] {
            assert_echo_call(&mut client, correlation, message);
        }

        router
            .handle()
            .signal_last_window_closed()
            .expect("LastWindowClosed");
        assert_lifecycle_event(&mut client, LifecycleEvent::LastWindowClosed);

        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(43),
            &encode(&LifecycleRequest::Quit).expect("Quit request"),
        )
        .expect("write Quit Call");
        let (quit_header, quit_payload) = read_frame(&mut client).expect("Quit Reply");
        assert_eq!(quit_header.kind, FrameKind::Reply);
        assert_eq!(quit_header.channel, LIFECYCLE_CHANNEL);
        assert_eq!(quit_header.corr, CorrelationId(43));
        assert_eq!(
            decode::<LifecycleResponse>(&quit_payload).expect("Quit response"),
            LifecycleResponse::Quit
        );
        assert!(
            guardian_prepared.load(Ordering::Acquire),
            "accepted shutdown attribution must precede the Quit reply"
        );
        let mut byte = [0_u8; 1];
        assert_eq!(client.read(&mut byte).expect("link EOF"), 0);
        eof_tx.send(()).expect("record EOF");
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("UI Quit wake"),
            AppWindowCommand::Quit
        );
        assert!(
            guardian_finished.load(Ordering::Acquire),
            "UI Quit must follow guardian reap acknowledgement"
        );

        router.shutdown().expect("router shutdown");
        guardian_thread.join().expect("guardian thread joins");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn retired_generation_eof_cannot_fail_or_replace_the_successor() {
        use std::os::unix::net::UnixStream;
        use std::sync::mpsc;
        use std::time::Duration;

        use keld_ipc::link::AppLinkDeadlines as _;

        let (g1_server, mut g1_client) = UnixStream::pair().expect("g1 session pair");
        g1_client
            .set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("g1 deadlines");
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, _guardian_rx) = mpsc::channel();
        let router = PrimaryRouter::start(
            g1_server,
            window_tx,
            GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
        )
        .expect("generation router");
        let handle = router.handle();
        handle.signal_ready().expect("g1 Ready");
        assert_lifecycle_event(&mut g1_client, LifecycleEvent::Ready);

        handle.retire_generation(1).expect("retire g1");
        handle
            .signal_last_window_closed()
            .expect("record last-window close during gap");
        let (g2_server, mut g2_client) = UnixStream::pair().expect("g2 session pair");
        g2_client
            .set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("g2 deadlines");
        handle.install_generation(2, g2_server).expect("install g2");
        assert_lifecycle_event(&mut g2_client, LifecycleEvent::Ready);
        assert_lifecycle_event(&mut g2_client, LifecycleEvent::LastWindowClosed);
        handle
            .lifecycle_quit(1, CorrelationId(72), &[0])
            .expect("stale g1 Quit is ignored");
        assert!(
            handle.shutdown.is_running(),
            "stale g1 Quit claimed g2 shutdown"
        );
        drop(g1_client);
        assert!(
            window_rx.try_recv().is_err(),
            "retired g1 EOF woke the UI fatal path"
        );
        assert_echo_call(&mut g2_client, 73, "successor");
        drop(g2_client);
        router.shutdown().expect("generation router shutdown");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn revoke_racing_accepted_quit_does_not_join_on_the_guardian_owner() {
        use std::os::unix::net::UnixStream;
        use std::sync::mpsc;
        use std::time::Duration;

        let (server, mut client) = UnixStream::pair().expect("Quit race pair");
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let (observed_tx, observed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let guardian_thread = std::thread::spawn(move || {
            let prepare = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("accepted Quit preparation");
            let GuardianOwnerCommand::PrepareAcceptedShutdown(reply) = prepare else {
                panic!("Quit race skipped attribution");
            };
            observed_tx.send(()).expect("report blocked attribution");
            release_rx.recv().expect("release attribution");
            reply.send(Ok(())).expect("accepted Quit attribution reply");
            let shutdown = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("accepted Quit shutdown");
            let GuardianOwnerCommand::Shutdown(reply) = shutdown else {
                panic!("Quit race skipped shutdown");
            };
            reply.send(Ok(())).expect("accepted Quit shutdown reply");
        });
        let router = PrimaryRouter::start(
            server,
            window_tx,
            GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
        )
        .expect("Quit race router");
        let handle = router.handle();
        let quit_handle = handle.clone();
        let quit_thread =
            std::thread::spawn(move || quit_handle.lifecycle_quit(1, CorrelationId(81), &[0]));
        observed_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("Quit reached guardian owner");
        handle
            .retire_generation(1)
            .expect("terminal revoke must not join blocked reader");
        assert!(
            handle.is_current(1),
            "accepted Quit lost its current writer"
        );
        release_tx.send(()).expect("release Quit attribution");
        quit_thread
            .join()
            .expect("Quit thread joins")
            .expect("Quit race tail");
        let (header, _) = keld_ipc::link::read_frame(&mut client).expect("Quit race reply");
        assert_eq!(header.corr, CorrelationId(81));
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("Quit race UI exit"),
            AppWindowCommand::Quit
        );
        router.shutdown().expect("Quit race router shutdown");
        guardian_thread.join().expect("Quit race guardian joins");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn failed_initial_ready_write_denies_recovery_before_successor() {
        use std::collections::HashMap;
        use std::os::unix::net::UnixStream;
        use std::sync::atomic::Ordering;
        use std::sync::{Arc, Mutex, mpsc};
        use std::time::Duration;

        let (server, client) = UnixStream::pair().expect("failed Ready pair");
        drop(client);
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let (window_tx, _window_rx) = mpsc::channel();
        let handle = PrimaryRouterHandle {
            current: Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
                attempt: 1,
                writer: server,
            }))),
            readers: Arc::new(Mutex::new(HashMap::new())),
            window_ready: Arc::new(AtomicBool::new(false)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(false)),
            shutdown: SessionShutdownState::new(),
            guardian: GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            window_commands: window_tx,
        };
        let error = handle
            .signal_ready()
            .expect_err("closed g1 must fail Ready");
        assert!(error.to_string().contains("lifecycle event"), "{error}");
        assert!(matches!(
            guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("Ready failure recovery denial"),
            GuardianOwnerCommand::DenyRecovery
        ));
        assert!(!handle.recovery_armed.load(Ordering::Acquire));
        assert!(!handle.window_ready.load(Ordering::Acquire));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn failed_link_recovery_signal_wakes_ui_fatal() {
        use std::os::unix::net::UnixStream;
        use std::sync::mpsc;
        use std::time::Duration;

        let (server, client) = UnixStream::pair().expect("link failure pair");
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let guardian_thread = std::thread::spawn(move || {
            let command = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("link failure command");
            let GuardianOwnerCommand::FailGeneration(1, reply) = command else {
                panic!("reader sent the wrong guardian command");
            };
            reply
                .send(Err(String::from("forced group-signal failure")))
                .expect("link failure reply");
        });
        let router = PrimaryRouter::start(
            server,
            window_tx,
            GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
        )
        .expect("link failure router");
        router.handle().signal_ready().expect("link failure Ready");
        drop(client);
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("link failure UI wake"),
            AppWindowCommand::Fatal
        );
        assert!(
            router.shutdown().is_err(),
            "link failure vanished at shutdown"
        );
        guardian_thread.join().expect("link failure guardian joins");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn retire_does_not_join_reader_waiting_for_link_failure_ack() {
        use std::os::unix::net::UnixStream;
        use std::sync::mpsc;
        use std::time::Duration;

        let (server, client) = UnixStream::pair().expect("link retirement pair");
        let (window_tx, _window_rx) = mpsc::channel();
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let (blocked_tx, blocked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let guardian_thread = std::thread::spawn(move || {
            let command = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("link retirement command");
            let GuardianOwnerCommand::FailGeneration(1, reply) = command else {
                panic!("reader sent the wrong retirement command");
            };
            blocked_tx.send(()).expect("report blocked link failure");
            release_rx.recv().expect("release link failure");
            reply.send(Ok(())).expect("link failure reply");
        });
        let router = PrimaryRouter::start(
            server,
            window_tx,
            GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
        )
        .expect("link retirement router");
        router
            .handle()
            .signal_ready()
            .expect("link retirement Ready");
        drop(client);
        blocked_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("reader waits for link failure acknowledgment");
        let retire_handle = router.handle();
        let (retired_tx, retired_rx) = mpsc::channel();
        let retire_thread = std::thread::spawn(move || {
            retired_tx
                .send(retire_handle.retire_generation(1))
                .expect("return retirement result");
        });
        retired_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("guardian-owner retirement must not join the blocked reader")
            .expect("retire blocked generation");
        release_tx
            .send(())
            .expect("release link failure acknowledgment");
        retire_thread.join().expect("retire thread joins");
        guardian_thread.join().expect("guardian thread joins");
        router.shutdown().expect("link retirement router shutdown");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn cli_lease_loss_closes_link_before_reap_and_sends_no_quit_reply() {
        use std::io::Read as _;
        use std::os::unix::net::UnixStream;
        use std::sync::mpsc;
        use std::time::Duration;

        use keld_ipc::link::AppLinkDeadlines as _;

        let (server, mut client) = UnixStream::pair().expect("lease-loss session pair");
        client
            .set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("client deadlines");
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let (eof_tx, eof_rx) = mpsc::channel();
        let (writer_tx, writer_rx) = mpsc::channel();
        let guardian_thread = std::thread::spawn(move || {
            let prepare_reply = match guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("guardian lease-loss attribution")
            {
                GuardianOwnerCommand::PrepareAcceptedShutdown(reply) => reply,
                GuardianOwnerCommand::Shutdown(_) => {
                    panic!("lease loss skipped accepted-shutdown attribution")
                }
                GuardianOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
                GuardianOwnerCommand::FailGeneration(_, _) => panic!("unexpected link failure"),
                GuardianOwnerCommand::ArmRecovery(_) => panic!("unexpected recovery arm"),
                GuardianOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
            };
            let writer: Arc<Mutex<Option<ActivePrimaryGeneration>>> = writer_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("router writer identity");
            assert!(
                writer.try_lock().is_ok(),
                "guardian RPC ran while the app-link writer was locked"
            );
            prepare_reply
                .send(Ok(()))
                .expect("guardian lease-loss attribution reply");
            let shutdown_reply = match guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("guardian lease-loss shutdown")
            {
                GuardianOwnerCommand::Shutdown(reply) => reply,
                GuardianOwnerCommand::PrepareAcceptedShutdown(_) => {
                    panic!("duplicate lease-loss attribution")
                }
                GuardianOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
                GuardianOwnerCommand::FailGeneration(_, _) => panic!("unexpected link failure"),
                GuardianOwnerCommand::ArmRecovery(_) => panic!("unexpected recovery arm"),
                GuardianOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
            };
            eof_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("link EOF before guardian lease-loss reap");
            shutdown_reply
                .send(Ok(()))
                .expect("guardian lease-loss reply");
        });
        let shutdown = SessionShutdownState::new();
        let router = PrimaryRouter::start(
            server,
            window_tx,
            GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            shutdown.clone(),
        )
        .expect("primary router");
        writer_tx
            .send(Arc::clone(&router.handle.current))
            .expect("share router writer identity");

        assert!(shutdown.claim_cli_lease_lost());
        let mut byte = [0_u8; 1];
        assert_eq!(
            client.read(&mut byte).expect("lease-loss link EOF"),
            0,
            "lease loss must not write a fabricated lifecycle reply"
        );
        eof_tx.send(()).expect("record lease-loss EOF");
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("lease-loss UI Quit wake"),
            AppWindowCommand::Quit
        );

        router.shutdown().expect("router shutdown");
        guardian_thread.join().expect("guardian thread joins");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn cli_lease_loss_during_generation_gap_runs_the_shutdown_tail() {
        use std::os::unix::net::UnixStream;
        use std::sync::mpsc;
        use std::time::Duration;

        let (server, client) = UnixStream::pair().expect("gap session pair");
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let guardian_thread = std::thread::spawn(move || {
            let prepare = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("gap accepted-shutdown attribution");
            match prepare {
                GuardianOwnerCommand::PrepareAcceptedShutdown(reply) => {
                    reply.send(Ok(())).expect("gap attribution reply");
                }
                _ => panic!("gap skipped accepted-shutdown attribution"),
            }
            let shutdown = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("gap guardian shutdown");
            match shutdown {
                GuardianOwnerCommand::Shutdown(reply) => {
                    reply.send(Ok(())).expect("gap shutdown reply");
                }
                _ => panic!("gap skipped guardian shutdown"),
            }
        });
        let shutdown = SessionShutdownState::new();
        let router = PrimaryRouter::start(
            server,
            window_tx,
            GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            shutdown.clone(),
        )
        .expect("gap router");
        router.handle().signal_ready().expect("gap Ready");
        router.handle().retire_generation(1).expect("retire g1");
        drop(client);
        assert!(shutdown.claim_cli_lease_lost());
        router
            .handle()
            .cli_lease_lost()
            .expect("generation-gap lease-loss tail");
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("gap UI Quit"),
            AppWindowCommand::Quit
        );
        router.shutdown().expect("gap router shutdown");
        guardian_thread.join().expect("gap guardian thread joins");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn terminal_tail_failure_wakes_the_ui_fatal_path() {
        use std::os::unix::net::UnixStream;
        use std::sync::mpsc;
        use std::time::Duration;

        let (server, _client) = UnixStream::pair().expect("tail-failure session pair");
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let guardian_thread = std::thread::spawn(move || {
            let prepare_reply = match guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("guardian tail-failure attribution")
            {
                GuardianOwnerCommand::PrepareAcceptedShutdown(reply) => reply,
                GuardianOwnerCommand::Shutdown(_) => panic!("tail skipped attribution"),
                GuardianOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
                GuardianOwnerCommand::FailGeneration(_, _) => panic!("unexpected link failure"),
                GuardianOwnerCommand::ArmRecovery(_) => panic!("unexpected recovery arm"),
                GuardianOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
            };
            prepare_reply
                .send(Err(String::from("forced attribution failure")))
                .expect("guardian tail-failure reply");
        });
        let shutdown = SessionShutdownState::new();
        let router = PrimaryRouter::start(
            server,
            window_tx,
            GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            shutdown.clone(),
        )
        .expect("primary router");

        assert!(shutdown.claim_cli_lease_lost());
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("terminal tail Fatal wake"),
            AppWindowCommand::Fatal
        );
        let error = router
            .shutdown()
            .expect_err("terminal attribution failure must remain visible");
        assert!(error.to_string().contains("forced attribution failure"));
        guardian_thread.join().expect("guardian thread joins");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn session_shutdown_accepts_exactly_one_first_cause() {
        let shutdown = SessionShutdownState::new();
        assert!(shutdown.claim_cli_lease_lost());
        assert!(!shutdown.claim_lifecycle_quit());
        assert_eq!(shutdown.cause(), SESSION_CLI_LEASE_LOST);

        let lifecycle = SessionShutdownState::new();
        assert!(lifecycle.claim_lifecycle_quit());
        assert!(!lifecycle.claim_cli_lease_lost());
        assert_eq!(lifecycle.cause(), SESSION_LIFECYCLE_QUIT);

        let gated = SessionShutdownState::new();
        let transition = gated.transition_guard();
        let claimant = gated.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let claim = std::thread::spawn(move || {
            started_tx.send(()).expect("claim started");
            claimant.claim_cli_lease_lost()
        });
        started_rx.recv().expect("claim thread started");
        assert_eq!(
            gated.cause(),
            SESSION_RUNNING,
            "a terminal cause changed while a write transition was active"
        );
        drop(transition);
        assert!(claim.join().expect("claim thread joins"));
        assert!(!gated.claim_lifecycle_quit());
        assert_eq!(gated.cause(), SESSION_CLI_LEASE_LOST);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn terminal_cleanup_reports_independent_router_and_guardian_failures() {
        let error = collapse_app_results([
            Err(app_detail("router tail", "link close failed")),
            Err(app_detail("guardian tail", "group reap failed")),
        ])
        .expect_err("two terminal failures must be aggregated");
        let rendered = error.to_string();
        assert!(rendered.contains("link close failed"), "{rendered}");
        assert!(rendered.contains("group reap failed"), "{rendered}");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn link_shutdown_accepts_already_closed_but_propagates_other_io_errors() {
        finish_link_shutdown(
            Err(std::io::Error::from(std::io::ErrorKind::NotConnected)),
            "test link close",
        )
        .expect("already-closed link is idempotent success");
        let error = finish_link_shutdown(
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            "test link close",
        )
        .expect_err("unrelated link error must remain fatal");
        assert!(error.to_string().contains("permission denied"), "{error}");
    }

    #[cfg(target_os = "macos")]
    fn assert_echo_call(
        client: &mut std::os::unix::net::UnixStream,
        correlation: u32,
        message: &str,
    ) {
        use keld_ipc::codec::{decode, encode};
        use keld_ipc::echo::{EchoRequest, EchoResponse};
        use keld_ipc::link::read_frame;

        let request = EchoRequest {
            message: message.to_owned(),
            count: correlation,
        };
        write_frame(
            client,
            FrameKind::Call,
            0,
            ECHO_CHANNEL,
            CorrelationId(correlation),
            &encode(&request).expect("echo request"),
        )
        .expect("write echo Call");
        let (header, payload) = read_frame(client).expect("echo Reply");
        assert_eq!(header.kind, FrameKind::Reply);
        assert_eq!(header.channel, ECHO_CHANNEL);
        assert_eq!(header.corr, CorrelationId(correlation));
        assert_eq!(
            decode::<EchoResponse>(&payload).expect("echo response"),
            EchoResponse {
                message: message.to_owned(),
                count: correlation,
            }
        );
    }

    #[cfg(target_os = "macos")]
    fn assert_lifecycle_event(
        client: &mut std::os::unix::net::UnixStream,
        expected: LifecycleEvent,
    ) {
        let (header, payload) = keld_ipc::link::read_frame(client).expect("lifecycle Event");
        assert_eq!(header.kind, FrameKind::Event);
        assert_eq!(header.channel, LIFECYCLE_CHANNEL);
        assert_eq!(
            keld_ipc::codec::decode::<LifecycleEvent>(&payload).expect("lifecycle payload"),
            expected
        );
    }
}
