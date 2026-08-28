//! Validated no-flag application boot and host-owned primary session (KEL-96).

use std::fmt;
#[cfg(target_os = "macos")]
use std::fs::File;
#[cfg(target_os = "macos")]
use std::io::{self, Read};
#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt;
#[cfg(any(target_os = "macos", test))]
use std::path::{Component, Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::{Command, ExitStatus, Stdio};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
#[cfg(target_os = "macos")]
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use std::thread::{self, JoinHandle};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

#[cfg(any(target_os = "macos", test))]
use serde::Deserialize;

#[cfg(target_os = "macos")]
use keld_ipc::codec::{decode, encode};
#[cfg(target_os = "macos")]
use keld_ipc::frame::{CorrelationId, FrameKind};
#[cfg(target_os = "macos")]
use keld_ipc::link::{AppLinkDeadlines, read_frame_interruptible, write_frame};
#[cfg(target_os = "macos")]
use keld_ipc::{
    APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL, BootstrapAdmission, BootstrapListener,
    BootstrapRejection, BootstrapRejectionObserver, ECHO_CHANNEL, IpcError, LIFECYCLE_CHANNEL,
    LifecycleEvent, LifecycleRequest, LifecycleResponse,
};
#[cfg(target_os = "macos")]
use keld_runtime::macos_guardian::{GuardianBootstrap, HostGuardian};
#[cfg(target_os = "macos")]
use keld_wv::wkwebview::{AppWindowCommand, AppWindowEvent, WkWebViewEngine};
#[cfg(target_os = "macos")]
use keld_wv::{NavTarget, WebviewSpec, WvError};

/// Maximum accepted `keld.boot.json` size.
#[cfg(any(target_os = "macos", test))]
const MAX_BOOT_BYTES: usize = 64 * 1024;
#[cfg(target_os = "macos")]
const BOOT_FILE: &str = "keld.boot.json";
#[cfg(any(target_os = "macos", test))]
const PERMISSIONS_FILE: &str = "keld.permissions.jsonc";
#[cfg(any(target_os = "macos", test))]
const DIGEST_PREFIX: &str = "sha256:";
#[cfg(target_os = "macos")]
const GUARDIAN_OWNER_REPLY_DEADLINE: Duration = Duration::from_secs(6);

#[cfg(target_os = "macos")]
static LISTENER_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "macos")]
static CHILD_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "macos")]
static WINDOW_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
/// Opaque host-owned selection minted only from the staged executable layout.
pub struct ValidatedBootSelection {
    #[cfg(target_os = "macos")]
    app: AppBootSelection,
}

#[cfg(not(target_os = "macos"))]
impl Drop for ValidatedBootSelection {
    fn drop(&mut self) {}
}

#[cfg(target_os = "macos")]
struct AppBootSelection {
    root: PathBuf,
    name: String,
    entry_path: PathBuf,
    entry_file: File,
    renderer_html: Vec<u8>,
    permissions_file: File,
    permissions_digest: [u8; 32],
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
        #[cfg(not(target_os = "macos"))]
        {
            Err(HostAppError::new(
                "KELD-CORE-034",
                "platform availability",
                "no-flag host support is unavailable on this platform",
                "Complete and prove the named KEL-96/T4 platform slice before launching the host.",
            ))
        }
        #[cfg(target_os = "macos")]
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
            validate_from_root(root).map(|app| Self { app })
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
}

#[derive(Debug, Clone, Copy, Default)]
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
        }
    }

    #[cfg(target_os = "macos")]
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
            .finish_non_exhaustive()
    }
}

impl std::error::Error for HostAppError {}

fn startup_resource_snapshot() -> StartupResourceSnapshot {
    #[cfg(target_os = "macos")]
    {
        StartupResourceSnapshot {
            listener: LISTENER_ATTEMPTS.load(Ordering::Acquire),
            child: CHILD_ATTEMPTS.load(Ordering::Acquire),
            window: WINDOW_ATTEMPTS.load(Ordering::Acquire),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        StartupResourceSnapshot::default()
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BootDocument {
    schema: u8,
    name: String,
    entry: String,
    renderer: String,
    permissions: PermissionsDocument,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissionsDocument {
    file: String,
    content_sha256: String,
}

#[cfg(any(target_os = "macos", test))]
struct ParsedBoot {
    name: String,
    entry: PathBuf,
    renderer: PathBuf,
    permissions_digest: [u8; 32],
}

#[cfg(any(target_os = "macos", test))]
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

#[cfg(any(target_os = "macos", test))]
fn boot_error(detail: impl Into<String>, fix: &'static str) -> HostAppError {
    HostAppError::new("KELD-CORE-035", "boot descriptor validation", detail, fix)
}

#[cfg(any(target_os = "macos", test))]
fn target_error(kind: &'static str, detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-036",
        "staged target validation",
        format!("{kind}: {}", detail.into()),
        "Regenerate the owner-private stage with readable regular files and no symlinks.",
    )
}

#[cfg(any(target_os = "macos", test))]
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

#[cfg(any(target_os = "macos", test))]
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
fn validate_from_root(root: &Path) -> Result<AppBootSelection, HostAppError> {
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
    Ok(AppBootSelection {
        root,
        name: parsed.name,
        entry_path: parsed.entry,
        entry_file,
        renderer_html,
        permissions_file,
        permissions_digest: parsed.permissions_digest,
    })
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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
        run_app(boot.app)
    }
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_lines)] // one startup/cleanup state machine keeps every owned handle transition contiguous
fn run_app(boot: AppBootSelection) -> Result<(), HostAppError> {
    let AppBootSelection {
        root,
        name,
        entry_path,
        entry_file,
        renderer_html,
        permissions_file,
        permissions_digest,
    } = boot;
    // T1a retains the already-open fixed file and decoded digest as immutable
    // descriptor handoff data, then deliberately drops them without reading,
    // hashing, or parsing policy. KEL-102/T2 owns that first same-handle read.
    let _permissions_handoff = (permissions_file, permissions_digest);
    let html = String::from_utf8(renderer_html).map_err(|source| {
        HostAppError::new(
            "KELD-CORE-036",
            "renderer",
            source.to_string(),
            "Regenerate the stage with UTF-8 renderer HTML.",
        )
    })?;

    LISTENER_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    let bootstrap =
        BootstrapListener::bind().map_err(|source| app_io("app-link provision", &source))?;
    let app_link = bootstrap.app_link();
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
        .env("KELD_APP_LINK", &app_link)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    CHILD_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    let pending = GuardianBootstrap::spawn_supervised(guardian_command)
        .map_err(|source| app_runtime("guardian bootstrap", &source))?;
    drop(entry_file);
    let guardian = pending
        .register_until(Instant::now() + APP_LINK_IO_DEADLINE)
        .map_err(|source| app_runtime("guardian registration", &source))?;

    let stream = match bootstrap
        .accept_authenticated_until(
            Instant::now() + APP_LINK_IO_DEADLINE,
            &NoopBootstrapObserver,
        )
        .map_err(|source| app_io("app-link authentication", &source))?
    {
        BootstrapAdmission::Authenticated(stream) => stream,
        BootstrapAdmission::Cancelled => {
            return Err(app_detail(
                "app-link authentication",
                "bootstrap was cancelled before HELLO",
            ));
        }
        BootstrapAdmission::DeadlineElapsed => {
            return Err(app_detail(
                "app-link authentication",
                "Bun did not authenticate before the startup deadline",
            ));
        }
    };

    let (window_commands_tx, window_commands_rx) = mpsc::channel();
    let guardian_owner = GuardianOwner::start(guardian, window_commands_tx.clone())?;
    let router = PrimaryRouter::start(stream, window_commands_tx.clone(), guardian_owner.handle())?;
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
        drop(window_events_tx);
        let event_result = event_coordinator
            .join()
            .map_err(|_| app_detail("window event coordinator", "thread panicked"))
            .and_then(std::convert::identity);
        let router_result = router.shutdown();
        drop(bootstrap);
        let guardian_result = guardian_owner.shutdown();
        return Err(collapse_app_failures(
            &primary,
            [event_result, router_result, guardian_result],
        ));
    }
    let window_result = engine.run_app_until_quit(window_commands_rx, window_events_tx);
    drop(window_commands_tx);
    let event_result = event_coordinator
        .join()
        .map_err(|_| app_detail("window event coordinator", "thread panicked"))?;
    let router_result = router.shutdown();
    drop(bootstrap);
    let guardian_result = guardian_owner.shutdown();

    match window_result {
        Err(source @ WvError::Navigate(_)) => {
            let primary = app_detail("initial navigation", source.to_string());
            Err(collapse_app_failures(
                &primary,
                [guardian_result, router_result, event_result],
            ))
        }
        result => {
            guardian_result?;
            router_result?;
            event_result?;
            result.map_err(|source| app_detail("macOS app window", source.to_string()))
        }
    }
}

#[cfg(target_os = "macos")]
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
}

#[cfg(target_os = "macos")]
enum GuardianOwnerCommand {
    PrepareQuit(std::sync::mpsc::SyncSender<Result<(), String>>),
    Shutdown(std::sync::mpsc::SyncSender<Result<(), String>>),
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct GuardianOwnerHandle {
    command_tx: Sender<GuardianOwnerCommand>,
}

#[cfg(target_os = "macos")]
impl GuardianOwner {
    fn start(
        mut guardian: HostGuardian,
        window_commands: Sender<AppWindowCommand>,
    ) -> Result<Self, HostAppError> {
        let (command_tx, command_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("keld-core-guardian-owner".to_owned())
            .spawn(move || {
                loop {
                    match command_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(GuardianOwnerCommand::PrepareQuit(reply)) => {
                            let result = guardian.accept_supervised_quit().map_err(|source| {
                                app_runtime("guardian Quit preparation", &source)
                            });
                            let observed = match &result {
                                Ok(()) => Ok(()),
                                Err(error) => Err(error.to_string()),
                            };
                            let _ = reply.send(observed);
                            result?;
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

    fn join(&mut self) -> Result<(), HostAppError> {
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };
        handle
            .join()
            .map_err(|_| app_detail("guardian owner", "thread panicked"))?
            .map(|_| ())
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
    fn prepare_quit(&self) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(GuardianOwnerCommand::PrepareQuit(reply_tx))
            .map_err(|_| app_detail("guardian Quit preparation", "guardian owner stopped"))?;
        match reply_rx.recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(detail)) => Err(app_detail("guardian Quit preparation", detail)),
            Err(RecvTimeoutError::Timeout) => Err(app_detail(
                "guardian Quit preparation",
                "guardian did not acknowledge Quit before the owner deadline",
            )),
            Err(RecvTimeoutError::Disconnected) => Err(app_detail(
                "guardian Quit preparation",
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

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct PrimaryRouterHandle {
    writer: Arc<Mutex<std::os::unix::net::UnixStream>>,
    quitting: Arc<AtomicBool>,
}

#[cfg(target_os = "macos")]
impl PrimaryRouterHandle {
    fn signal_ready(&self) -> Result<(), HostAppError> {
        self.write_event(LifecycleEvent::Ready)
    }

    fn signal_last_window_closed(&self) -> Result<(), HostAppError> {
        self.write_event(LifecycleEvent::LastWindowClosed)
    }

    fn write_event(&self, event: LifecycleEvent) -> Result<(), HostAppError> {
        let payload = encode(&event).map_err(|source| app_ipc("lifecycle event", &source))?;
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| app_detail("primary session writer", "writer lock poisoned"))?;
        if self.quitting.load(Ordering::Acquire) {
            return Ok(());
        }
        write_frame(
            &mut *writer,
            FrameKind::Event,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            &payload,
        )
        .map_err(|source| app_ipc("lifecycle event", &source))
    }
}

#[cfg(target_os = "macos")]
struct PrimaryRouter {
    handle: PrimaryRouterHandle,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<Result<(), HostAppError>>>,
}

#[cfg(target_os = "macos")]
impl PrimaryRouter {
    fn start(
        mut stream: std::os::unix::net::UnixStream,
        window_commands: Sender<AppWindowCommand>,
        guardian: GuardianOwnerHandle,
    ) -> Result<Self, HostAppError> {
        stream
            .set_app_link_read_deadline(Some(APP_LINK_READER_POLL))
            .map_err(|source| app_io("primary session reader deadline", &source))?;
        let writer_stream = stream
            .try_clone()
            .map_err(|source| app_io("primary session writer clone", &source))?;
        writer_stream
            .set_app_link_write_deadline(Some(APP_LINK_IO_DEADLINE))
            .map_err(|source| app_io("primary session writer deadline", &source))?;
        let writer = Arc::new(Mutex::new(writer_stream));
        let quitting = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let writer_for_reader = Arc::clone(&writer);
        let quitting_for_reader = Arc::clone(&quitting);
        let stop_for_reader = Arc::clone(&stop);
        let reader = thread::Builder::new()
            .name("keld-core-primary-router".to_owned())
            .spawn(move || {
                let result = read_primary_frames(
                    &mut stream,
                    writer_for_reader.as_ref(),
                    quitting_for_reader.as_ref(),
                    stop_for_reader.as_ref(),
                    &window_commands,
                    &guardian,
                );
                if result.is_err() && !stop_for_reader.load(Ordering::Acquire) {
                    let _ = window_commands.send(AppWindowCommand::Fatal);
                }
                result
            })
            .map_err(|source| app_io("primary session reader", &source))?;
        Ok(Self {
            handle: PrimaryRouterHandle { writer, quitting },
            stop,
            reader: Some(reader),
        })
    }

    fn handle(&self) -> PrimaryRouterHandle {
        self.handle.clone()
    }

    fn shutdown(mut self) -> Result<(), HostAppError> {
        self.stop_and_join()
    }

    fn stop_and_join(&mut self) -> Result<(), HostAppError> {
        self.handle.quitting.store(true, Ordering::Release);
        self.stop.store(true, Ordering::Release);
        {
            let writer = match self.handle.writer.lock() {
                Ok(writer) => writer,
                Err(poisoned) => poisoned.into_inner(),
            };
            let _ = writer.shutdown_app_link();
        }
        let Some(reader) = self.reader.take() else {
            return Ok(());
        };
        reader
            .join()
            .map_err(|_| app_detail("primary session reader", "thread panicked"))?
    }
}

#[cfg(target_os = "macos")]
impl Drop for PrimaryRouter {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

#[cfg(target_os = "macos")]
fn read_primary_frames(
    reader: &mut std::os::unix::net::UnixStream,
    writer: &Mutex<std::os::unix::net::UnixStream>,
    quitting: &AtomicBool,
    stop: &AtomicBool,
    window_commands: &Sender<AppWindowCommand>,
    guardian: &GuardianOwnerHandle,
) -> Result<(), HostAppError> {
    loop {
        let (header, payload) = match read_frame_interruptible(reader, stop) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(()),
            Err(IpcError::Io(source)) if source.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(app_detail(
                    "primary session reader",
                    "Bun closed the app link",
                ));
            }
            Err(source) => return Err(app_ipc("primary session reader", &source)),
        };
        match (header.kind, header.channel) {
            (FrameKind::Call, ECHO_CHANNEL) if !quitting.load(Ordering::Acquire) => {
                let reply = keld_ipc::echo::handle_echo(&payload)
                    .map_err(|source| app_ipc("echo dispatch", &source))?;
                write_primary_reply(writer, ECHO_CHANNEL, header.corr, &reply)?;
            }
            (FrameKind::Call, LIFECYCLE_CHANNEL) => {
                let request: LifecycleRequest =
                    decode(&payload).map_err(|source| app_ipc("lifecycle request", &source))?;
                match request {
                    LifecycleRequest::Quit => {
                        let reply = encode(&LifecycleResponse::Quit)
                            .map_err(|source| app_ipc("lifecycle Quit reply", &source))?;
                        let writer_guard = writer.lock().map_err(|_| {
                            app_detail("primary session writer", "writer lock poisoned")
                        })?;
                        if quitting.swap(true, Ordering::AcqRel) {
                            return Err(app_detail(
                                "lifecycle Quit",
                                "session is already quitting",
                            ));
                        }
                        drop(writer_guard);
                        guardian.prepare_quit()?;
                        let mut writer_guard = writer.lock().map_err(|_| {
                            app_detail("primary session writer", "writer lock poisoned")
                        })?;
                        write_frame(
                            &mut *writer_guard,
                            FrameKind::Reply,
                            0,
                            LIFECYCLE_CHANNEL,
                            header.corr,
                            &reply,
                        )
                        .map_err(|source| app_ipc("lifecycle Quit reply", &source))?;
                        writer_guard
                            .shutdown_app_link()
                            .map_err(|source| app_io("lifecycle Quit link close", &source))?;
                        drop(writer_guard);
                        guardian.shutdown_and_wait()?;
                        window_commands.send(AppWindowCommand::Quit).map_err(|_| {
                            app_detail("lifecycle Quit", "UI event loop is unavailable")
                        })?;
                        return Ok(());
                    }
                }
            }
            (FrameKind::Ping, _) => {
                let mut writer = writer
                    .lock()
                    .map_err(|_| app_detail("primary session writer", "writer lock poisoned"))?;
                write_frame(
                    &mut *writer,
                    FrameKind::Ping,
                    0,
                    header.channel,
                    header.corr,
                    &[],
                )
                .map_err(|source| app_ipc("primary session Ping", &source))?;
            }
            (FrameKind::Call, _) if quitting.load(Ordering::Acquire) => {
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

#[cfg(target_os = "macos")]
fn write_primary_reply(
    writer: &Mutex<std::os::unix::net::UnixStream>,
    channel: keld_ipc::ChannelId,
    correlation: CorrelationId,
    payload: &[u8],
) -> Result<(), HostAppError> {
    let mut writer = writer
        .lock()
        .map_err(|_| app_detail("primary session writer", "writer lock poisoned"))?;
    write_frame(
        &mut *writer,
        FrameKind::Reply,
        0,
        channel,
        correlation,
        payload,
    )
    .map_err(|source| app_ipc("primary session reply", &source))
}

#[cfg(target_os = "macos")]
struct NoopBootstrapObserver;

#[cfg(target_os = "macos")]
impl BootstrapRejectionObserver for NoopBootstrapObserver {
    fn rejected(&self, _rejection: BootstrapRejection) {}
}

#[cfg(target_os = "macos")]
fn app_detail(phase: &'static str, detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-037",
        phase,
        detail,
        "Fix the app session failure and relaunch the no-flag host.",
    )
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn app_io(phase: &'static str, source: &io::Error) -> HostAppError {
    HostAppError::io(
        "KELD-CORE-037",
        phase,
        source,
        "Fix the app session I/O failure and relaunch the no-flag host.",
    )
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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
        assert_eq!(selection.renderer_html, b"<p id=fixture>exact</p>\n");
        assert_eq!(selection.name, "Fixture");

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
            selection.renderer_html, b"<p id=fixture>exact</p>\n",
            "post-selection renderer substitution changed consumed bytes"
        );
        assert_eq!(
            selection.name, "Fixture",
            "post-selection sidecar substitution changed owned fields"
        );
        let error = must_err(validate_from_root(&root), "symlink escape must fail");
        assert!(error.to_string().contains("renderer"), "{error}");
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
        assert_eq!(selection.renderer_html, renderer);
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
                GuardianOwnerCommand::PrepareQuit(reply) => reply,
                GuardianOwnerCommand::Shutdown(_) => panic!("Quit reply lacked preparation"),
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
                GuardianOwnerCommand::PrepareQuit(_) => panic!("duplicate Quit preparation"),
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
