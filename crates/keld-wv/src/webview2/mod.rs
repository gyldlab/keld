//! Windows backend: `WebView2` via direct windows-rs COM bindings (KEL-65).
//!
//! This is the `docs/architecture/05-webview-and-native.md` §1 destination for
//! the create path — environment, controller, permission handler, and
//! navigation are Keld-owned COM calls through `webview2-com` — with tao still
//! providing the window and event loop until keld-core owns the command queue.
//! wry remains the interim scaffolding on macOS only; Windows no longer links
//! it. Layout still mirrors wry's `competitors/wry/src/webview2/` per-platform
//! module pattern.
//!
//! Why direct COM instead of wry's `InnerWebView` (measured under KEL-65):
//! wry 0.56.1 unconditionally injects its `window.ipc` bridge script through
//! a **blocking** cross-process `AddScriptToExecuteOnDocumentCreated`
//! round-trip against a renderer that is still booting — 96–109 ms for a bridge
//! Keld never uses (kipc has its own transport design). Owning the environment
//! also drops wry's default `AdditionalBrowserArguments`, which disabled
//! `SmartScreen` (KEL-66), and makes the KEL-62 initial-bounds fix structural:
//! the controller gets its size before the first navigation, so Chromium never
//! composites a zero-sized surface.
//!
//! Ordering contract inside `create`: **guard permission handler → devtools
//! policy → bounds + visibility → navigate**. Content must never run before
//! the `keld-guard` handler is registered (crate `AGENTS.md`), which the
//! compiler enforces: `navigate_initial` demands the `GuardInstalled` proof
//! only `install_guarded_media_permissions` can mint.
//!
//! Unlike the macOS backend this module owns one extra Windows-only concern:
//! the Evergreen runtime is a separate redistributable that may be absent.
//! [`runtime_version`] probes for it up front so the failure is a typed
//! `KELD-WV-008` with install guidance, not an opaque COM `HRESULT`.
//!
// SAFETY: this module speaks to WebView2 over COM. The WebView2 threading
// contract is a single-threaded apartment: the environment, controller, and
// webview are created on the shipping process main thread (tao's event-loop thread);
// the ignored media-acceptance libtest uses one dedicated Windows UI thread,
// and every later use — resize, navigate, eval, devtools, drop — happens on
// that same thread inside engine methods or tao's event loop, satisfying both
// the COM contract and the crate `AGENTS.md` "UI-thread-only mutations"
// invariant. Async completions are delivered by `webview2_com::wait_with_pump`,
// which pumps this thread's message queue — the documented WebView2 callback
// mechanism (learn.microsoft.com, "Threading model for WebView2 apps").
// `GetAvailableCoreWebView2BrowserVersionString` is a pure query whose
// COM-allocated out-string is released exactly once with `CoTaskMemFree`.
#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(all(feature = "media-acceptance", test))]
pub(crate) mod media_acceptance;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read as _, Write as _};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::os::windows::io::AsRawHandle as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use tao::dpi::PhysicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::EventLoopBuilderExtWindows;
use tao::platform::windows::WindowExtWindows;
use tao::window::{Window, WindowBuilder};

#[cfg(all(feature = "media-acceptance", test))]
use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PERMISSION_STATE_DEFAULT;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND_NORMAL, COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC,
    COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
    COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE,
    COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY,
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2_13,
    ICoreWebView2Controller, ICoreWebView2Environment, ICoreWebView2Environment5,
    ICoreWebView2EnvironmentOptions, ICoreWebView2Profile4,
};
use webview2_com::{
    BrowserProcessExitedEventHandler, CoreWebView2EnvironmentOptions,
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    ExecuteScriptCompletedHandler, GetNonDefaultPermissionSettingsCompletedHandler,
    NavigationCompletedEventHandler, NewWindowRequestedEventHandler,
    PermissionRequestedEventHandler, SetPermissionStateCompletedHandler, wait_with_pump,
};
use windows::Win32::Foundation::{
    E_POINTER, E_UNEXPECTED, ERROR_ACCESS_DENIED, ERROR_INVALID_STATE, FILETIME, HANDLE, HWND,
    RECT, WAIT_FAILED,
};
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW,
    GetFileInformationByHandle, GetFinalPathNameByHandleW, GetVolumePathNameW,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, READ_CONTROL, VOLUME_NAME_DOS,
};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, MSG, MWMO_INPUTAVAILABLE,
    MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT, TranslateMessage,
    WINDOW_EX_STYLE, WM_QUIT, WS_OVERLAPPED,
};
use windows::core::{BOOL, HSTRING, IUnknown, Interface, PCWSTR, PWSTR, w};
use windows_permissions::Acl;
use windows_permissions::constants::{AccessRights, SeObjectType, SecurityInformation};
use windows_permissions::utilities::current_process_sid;
use windows_permissions::wrappers::GetSecurityInfo;

use keld_guard::{PermissionsManifest, Principal};

use crate::WebviewId;
use crate::engine::{
    AppWindowCommand, AppWindowEvent, DevtoolsAction, NavTarget, Rect, WebEngine,
    WebView2EngineExt, WebviewSpec,
};
use crate::error::WvError;
use crate::media::{media_permission_allowed, webview_media_principal, webview2_media_kind};
use crate::profile::{
    EphemeralProfile, ProfileError, ProfileErrorKind, ProfileIdentity, ProfileLifecycleAction,
    ProfileLifecyclePhase, ProfileLifecycleRecord, ProfileMarker, ProfilePlatform,
    ProfileProcessIdentity, ProfilePurgePhase, ProfilePurgeRecord, ProfileRootRole,
    RecordedProcessObservation, WebProfileSelection, next_windows_lifecycle_action,
};

/// Returns the installed `WebView2` Evergreen runtime version.
///
/// This is the detection path Microsoft documents. We deliberately do not read
/// the `EdgeUpdate` registry keys ourselves: which key is authoritative differs
/// between the Evergreen, fixed-version, and per-user channels, and the loader
/// already encodes those rules.
///
/// # Errors
///
/// Returns [`WvError::WebView2RuntimeMissing`] when the runtime is absent, too
/// old, or the loader reports any other failure.
pub fn runtime_version() -> Result<String, WvError> {
    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::core::{PCWSTR, PWSTR};

    let mut version = PWSTR::null();
    // SAFETY: `PCWSTR::null()` selects the installed Evergreen runtime (the
    // documented "use the default install" argument). `&mut version` is a valid
    // out-pointer for the duration of the call. See the module SAFETY note.
    let probe =
        unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &raw mut version) };

    if let Err(err) = probe {
        return Err(WvError::WebView2RuntimeMissing {
            detail: err.to_string(),
        });
    }
    if version.is_null() {
        return Err(WvError::WebView2RuntimeMissing {
            detail: String::from("loader reported success but returned no version"),
        });
    }

    // SAFETY: the loader returned success and a non-null pointer, so `version`
    // points at a NUL-terminated UTF-16 string it allocated with the COM task
    // allocator. We read it before freeing, and free exactly once.
    let text = unsafe { version.to_string() };
    // SAFETY: `version` came from the COM task allocator and has not been freed.
    unsafe { CoTaskMemFree(Some(version.as_ptr().cast())) };

    text.map_err(|err| WvError::WebView2RuntimeMissing {
        detail: format!("runtime version string was not valid UTF-16: {err}"),
    })
}

const INITIAL_NAVIGATION_DEADLINE: Duration = Duration::from_secs(5);
const PROFILE_RELEASE_DEADLINE: Duration = Duration::from_secs(15);
const PROFILE_MARKER: &str = "profile.owner.v1";
const PROFILE_LEASE: &str = "profile.lock";
const PROFILE_LIFECYCLE: &str = "profile.lifecycle.v1";
const PROFILE_LIFECYCLE_PENDING: &str = "profile.lifecycle.pending";
const PROFILE_PURGE: &str = "profile.purge.v1";
const PROFILE_PURGE_PENDING: &str = "profile.purge.pending";
const MAX_PROFILE_CONTROL_RECORD_BYTES: u64 = 4 * 1024;
const MAX_EPHEMERAL_SCAVENGE_PER_LAUNCH: usize = 1;

/// Directory `WebView2` keeps its profile in.
///
/// Without an explicit user-data folder the loader defaults to
/// `<exe-name>.WebView2\` **beside the executable** (KEL-63). A shipped app
/// lives in `C:\Program Files\<App>\`, which a standard user cannot write, so
/// first launch would fail there while working fine from `target\release\`. It
/// is also per-install-path rather than per-user, so two OS users would share
/// one cookie/localStorage store.
///
#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsProfilePlan {
    local_app_data: PathBuf,
    control_dir: PathBuf,
    user_data_dir: PathBuf,
    marker: Vec<u8>,
    identity: Option<ProfileIdentity>,
    require_new_control: bool,
    cleanup_on_release: bool,
}

#[derive(Debug)]
struct SelectedWindowsProfile {
    plan: WindowsProfilePlan,
    ancestor_handles: Vec<File>,
    control_handle: File,
    user_data_handle: File,
    lease: File,
    lifecycle: Option<ProfileLifecycleRecord>,
    recovery_required: bool,
}

#[derive(Clone, Copy)]
enum WindowsLoopEvent {
    App(AppWindowCommand),
    ProfileReleaseWake,
}

struct PersistentPurgeOwner {
    plan: WindowsProfilePlan,
    ancestor_handles: Vec<File>,
    control_handle: File,
    user_data_handle: Option<File>,
    lease: File,
    lifecycle: ProfileLifecycleRecord,
}

#[derive(Clone)]
struct PersistentStopTransition {
    plan: WindowsProfilePlan,
    record: Rc<Cell<ProfileLifecycleRecord>>,
    failed: Arc<AtomicBool>,
}

impl PersistentStopTransition {
    fn from_profile(profile: Option<&SelectedWindowsProfile>) -> Option<Self> {
        let profile = profile?;
        let record = profile.lifecycle?;
        Some(Self {
            plan: profile.plan.clone(),
            record: Rc::new(Cell::new(record)),
            failed: Arc::new(AtomicBool::new(false)),
        })
    }

    fn commit(&self) {
        let record = self.record.get();
        if record.phase() == ProfileLifecyclePhase::Stopping {
            return;
        }
        let Ok(stopping) = record.advance(ProfileLifecyclePhase::Stopping) else {
            self.failed.store(true, Ordering::Release);
            return;
        };
        if replace_lifecycle(&self.plan, stopping).is_err() {
            self.failed.store(true, Ordering::Release);
        } else {
            self.record.set(stopping);
        }
    }

    fn update_profile(&self, profile: Option<&mut SelectedWindowsProfile>) -> Result<(), WvError> {
        if self.failed.load(Ordering::Acquire) {
            return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
        }
        let profile =
            profile.ok_or_else(|| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
        profile.lifecycle = Some(self.record.get());
        Ok(())
    }
}

fn profile_failure(kind: ProfileErrorKind) -> WvError {
    ProfileError::platform_failure(kind).into()
}

fn initialize_com_sta() -> Result<(), WvError> {
    // SAFETY: the caller invokes this before creating thread-affine WebView2
    // objects. S_OK/S_FALSE are success; RPC_E_CHANGED_MODE and other errors
    // fail before profile filesystem or engine work. Contract:
    // https://learn.microsoft.com/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))
}

fn initialize_process_dpi_awareness() -> Result<(), WvError> {
    // SAFETY: Keld calls this once before creating any HWND. The per-monitor-v2
    // context is the same preference tao otherwise applies while building its
    // event loop. Contract:
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setprocessdpiawarenesscontext
    match unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == windows::core::HRESULT::from_win32(ERROR_ACCESS_DENIED.0) => {
            // SAFETY: both DPI-context operations are process/thread queries.
            // A failed setter is accepted only when the effective context is
            // already the exact policy Keld requested. Contracts:
            // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getthreaddpiawarenesscontext
            // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-aredpiawarenesscontextsequal
            let matches = unsafe {
                AreDpiAwarenessContextsEqual(
                    GetThreadDpiAwarenessContext(),
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                )
            };
            if matches.as_bool() {
                Ok(())
            } else {
                Err(WvError::Window(error.to_string()))
            }
        }
        Err(error) => Err(WvError::Window(error.to_string())),
    }
}

fn wait_with_message_pump_until<T>(
    receiver: &Receiver<T>,
    deadline: Instant,
) -> Result<T, WvError> {
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
        }
        match receiver.try_recv() {
            Ok(value) => return Ok(value),
            Err(TryRecvError::Disconnected) => {
                return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
            }
            Err(TryRecvError::Empty) => {}
        }
        let milliseconds = u32::try_from(
            deadline
                .saturating_duration_since(now)
                .as_millis()
                .saturating_add(1)
                .min(u128::from(u32::MAX)),
        )
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
        // SAFETY: no handles are supplied; this thread waits only for its
        // Win32/COM queue or the remaining monotonic deadline. Contract:
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-msgwaitformultipleobjectsex
        let wait = unsafe {
            MsgWaitForMultipleObjectsEx(None, milliseconds, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if wait == WAIT_FAILED {
            return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
        }
        let mut message = MSG::default();
        // SAFETY: `message` is writable and each removed message is translated
        // and dispatched once on its owning thread. Contract:
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-peekmessagew
        if unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            if message.message == WM_QUIT {
                return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
            }
            // SAFETY: `message` is the live record just removed from this
            // thread's queue. Contracts:
            // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-translatemessage
            // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-dispatchmessagew
            unsafe {
                let _ = TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
    }
}

fn windows_profile_plan(
    local_app_data: &Path,
    selection: WebProfileSelection,
) -> Result<WindowsProfilePlan, WvError> {
    let root = local_app_data.join("Keld");
    match selection {
        WebProfileSelection::Persistent(identity) => {
            let control_dir = root
                .join("profiles")
                .join("v1")
                .join(identity.namespace_segment());
            let marker =
                ProfileMarker::new(identity, ProfilePlatform::Windows, ProfileRootRole::Control)
                    .to_record_bytes()?;
            Ok(WindowsProfilePlan {
                local_app_data: local_app_data.to_path_buf(),
                user_data_dir: control_dir.join("webview2"),
                control_dir,
                marker,
                identity: Some(identity),
                require_new_control: false,
                cleanup_on_release: false,
            })
        }
        WebProfileSelection::EphemeralDev(profile) => {
            let namespace = profile.namespace_segment();
            let control_dir = root.join("ephemeral").join("v1").join(&namespace);
            Ok(WindowsProfilePlan {
                local_app_data: local_app_data.to_path_buf(),
                user_data_dir: control_dir.join("webview2"),
                control_dir,
                marker: format!("keld.webview2.ephemeral/v1\n{namespace}\n").into_bytes(),
                identity: None,
                require_new_control: true,
                cleanup_on_release: true,
            })
        }
    }
}

fn known_local_app_data() -> Result<PathBuf, WvError> {
    // SAFETY: this noninteractive known-folder query writes one COM-allocated
    // NUL-terminated path for the current process token. The allocation is
    // copied and freed exactly once below.
    let raw = unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None) }
        .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    // SAFETY: a successful call returned a live NUL-terminated string.
    let text = unsafe { raw.to_string() };
    // SAFETY: `raw` is the one allocation returned above and is not reused.
    unsafe { CoTaskMemFree(Some(raw.as_ptr().cast())) };
    text.map(PathBuf::from)
        .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))
}

fn open_directory_handle(path: &Path) -> Result<File, WvError> {
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
        .custom_flags((FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).0)
        .open(path)
        .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    let metadata = file
        .metadata()
        .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    Ok(file)
}

#[derive(Clone, Copy)]
enum ControlFileMode {
    Read,
    CreateNew,
    Lease,
}

fn open_control_file(path: &Path, mode: ControlFileMode) -> Result<File, WvError> {
    let mut options = OpenOptions::new();
    options
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .share_mode(match mode {
            ControlFileMode::Read => FILE_SHARE_READ.0,
            ControlFileMode::CreateNew | ControlFileMode::Lease => 0,
        });
    match mode {
        ControlFileMode::Read => {
            options.read(true);
        }
        ControlFileMode::CreateNew => {
            options.write(true).create_new(true);
        }
        ControlFileMode::Lease => {
            options.read(true).write(true).create(true).truncate(false);
        }
    }
    let file = options.open(path).map_err(|_| {
        profile_failure(match mode {
            ControlFileMode::Lease => ProfileErrorKind::ProfileInUse,
            ControlFileMode::Read | ControlFileMode::CreateNew => ProfileErrorKind::MarkerMismatch,
        })
    })?;
    let metadata = file
        .metadata()
        .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    if file_information(&file)?.nNumberOfLinks != 1 {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    Ok(file)
}

fn control_file_present(path: &Path) -> Result<bool, WvError> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0 =>
        {
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Ok(_) | Err(_) => Err(profile_failure(ProfileErrorKind::MarkerMismatch)),
    }
}

fn read_control_record(path: &Path) -> Result<Vec<u8>, WvError> {
    let mut file = open_control_file(path, ControlFileMode::Read)?;
    if file
        .metadata()
        .map_err(|_| profile_failure(ProfileErrorKind::InvalidRecord))?
        .len()
        > MAX_PROFILE_CONTROL_RECORD_BYTES
    {
        return Err(profile_failure(ProfileErrorKind::InvalidRecord));
    }
    let mut bytes = Vec::new();
    std::io::Read::take(&mut file, MAX_PROFILE_CONTROL_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| profile_failure(ProfileErrorKind::InvalidRecord))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_PROFILE_CONTROL_RECORD_BYTES {
        return Err(profile_failure(ProfileErrorKind::InvalidRecord));
    }
    Ok(bytes)
}

fn retain_directory_chain(
    trusted_root: &Path,
    target: &Path,
    create_missing: bool,
    require_new_target: bool,
) -> Result<(Vec<File>, bool), WvError> {
    let relative = target
        .strip_prefix(trusted_root)
        .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    let mut path = trusted_root.to_path_buf();
    let mut handles = vec![open_directory_handle(&path)?];
    let mut target_created = false;
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
        };
        path.push(component);
        if create_missing {
            match fs::create_dir(&path) {
                Ok(()) => {
                    if path == target {
                        target_created = true;
                    }
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if path == target && require_new_target {
                        return Err(profile_failure(ProfileErrorKind::ProfileInUse));
                    }
                }
                Err(_) => return Err(profile_failure(ProfileErrorKind::MarkerMismatch)),
            }
        }
        handles.push(open_directory_handle(&path)?);
    }
    Ok((handles, target_created))
}

fn validate_or_write_marker(
    plan: &WindowsProfilePlan,
    control_created: bool,
) -> Result<(), WvError> {
    let marker_path = plan.control_dir.join(PROFILE_MARKER);
    if control_created {
        let mut marker = open_control_file(&marker_path, ControlFileMode::CreateNew)?;
        marker
            .write_all(&plan.marker)
            .and_then(|()| marker.sync_all())
            .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
        return Ok(());
    }
    let actual = read_control_record(&marker_path)?;
    if actual != plan.marker {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    Ok(())
}

fn current_profile_process() -> Result<ProfileProcessIdentity, WvError> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: `GetCurrentProcess` returns the caller's non-owning pseudo
    // handle. All four FILETIME outputs live for the synchronous query.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    }
    .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let process_birth =
        (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    ProfileProcessIdentity::from_host_observation(std::process::id(), process_birth)
        .map_err(Into::into)
}

fn write_new_control_record(path: &Path, bytes: &[u8]) -> Result<(), WvError> {
    let mut file = open_control_file(path, ControlFileMode::CreateNew)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))
}

fn replace_control_record(path: &Path, pending: &Path, bytes: &[u8]) -> Result<(), WvError> {
    if control_file_present(pending)? {
        let pending_file = open_control_file(pending, ControlFileMode::Read)?;
        drop(pending_file);
        fs::remove_file(pending)
            .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    }
    write_new_control_record(pending, bytes)?;
    let source = pending
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both vectors are live NUL-terminated paths inside the retained
    // control directory. The profile lease is held; replace+write-through is
    // the one atomic durable transition writer for this record.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))
}

fn write_new_lifecycle(path: &Path, record: ProfileLifecycleRecord) -> Result<(), WvError> {
    write_new_control_record(path, &record.to_record_bytes()?)
}

fn replace_lifecycle(
    plan: &WindowsProfilePlan,
    record: ProfileLifecycleRecord,
) -> Result<(), WvError> {
    replace_control_record(
        &plan.control_dir.join(PROFILE_LIFECYCLE),
        &plan.control_dir.join(PROFILE_LIFECYCLE_PENDING),
        &record.to_record_bytes()?,
    )
}

fn control_can_initialize_lifecycle(plan: &WindowsProfilePlan) -> Result<bool, WvError> {
    let mut expected_marker = false;
    let mut expected_lease = false;
    for entry in fs::read_dir(&plan.control_dir)
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
    {
        let name = entry
            .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
            .file_name();
        if name == PROFILE_MARKER {
            expected_marker = true;
        } else if name == PROFILE_LEASE {
            expected_lease = true;
        } else {
            return Ok(false);
        }
    }
    Ok(expected_marker && expected_lease)
}

fn prepare_persistent_lifecycle(
    plan: &WindowsProfilePlan,
    control_created: bool,
) -> Result<(ProfileLifecycleRecord, bool), WvError> {
    let lifecycle_path = plan.control_dir.join(PROFILE_LIFECYCLE);
    let record = if control_file_present(&lifecycle_path)? {
        let bytes = read_control_record(&lifecycle_path)?;
        ProfileLifecycleRecord::from_windows_record_bytes(&bytes)?
    } else if control_created || control_can_initialize_lifecycle(plan)? {
        let idle = ProfileLifecycleRecord::windows_idle();
        write_new_lifecycle(&lifecycle_path, idle)?;
        idle
    } else {
        return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
    };
    let current = current_profile_process()?;
    let observation = if record.owner() == Some(current) {
        RecordedProcessObservation::Live
    } else {
        // Acquiring the native share-denied lease proves that any different
        // process which owned the prior record released its host-side handle.
        // Windows still requires the independent exclusive-UDF recovery below.
        RecordedProcessObservation::Dead
    };
    match next_windows_lifecycle_action(record, observation)? {
        ProfileLifecycleAction::BeginStartup => {
            let starting = record.begin_startup(current)?;
            replace_lifecycle(plan, starting)?;
            Ok((starting, false))
        }
        ProfileLifecycleAction::WriteQuarantined => {
            let quarantined = record.quarantine()?;
            replace_lifecycle(plan, quarantined)?;
            Ok((quarantined, true))
        }
        ProfileLifecycleAction::RunWindowsExclusiveUdfRecovery => Ok((record, true)),
        ProfileLifecycleAction::RestoreIdleAfterBoot => {
            Err(profile_failure(ProfileErrorKind::LifecycleUnproven))
        }
    }
}

fn prepare_windows_profile_at(
    local_app_data: &Path,
    selection: WebProfileSelection,
) -> Result<SelectedWindowsProfile, WvError> {
    let plan = windows_profile_plan(local_app_data, selection)?;
    let (mut ancestor_handles, control_created) = retain_directory_chain(
        local_app_data,
        &plan.control_dir,
        true,
        plan.require_new_control,
    )?;
    let control_handle = ancestor_handles
        .pop()
        .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    validate_or_write_marker(&plan, control_created)?;
    let lease = open_control_file(
        &plan.control_dir.join(PROFILE_LEASE),
        ControlFileMode::Lease,
    )?;
    let (lifecycle, recovery_required) = if plan.cleanup_on_release {
        (None, false)
    } else {
        if control_file_present(&plan.control_dir.join(PROFILE_PURGE))?
            || control_file_present(&plan.control_dir.join(PROFILE_PURGE_PENDING))?
        {
            return Err(profile_failure(ProfileErrorKind::ActiveIntent));
        }
        let (record, recovery_required) = prepare_persistent_lifecycle(&plan, control_created)?;
        (Some(record), recovery_required)
    };
    let (mut udf_handles, _) =
        retain_directory_chain(&plan.control_dir, &plan.user_data_dir, true, false)?;
    let user_data_handle = udf_handles
        .pop()
        .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    let duplicate_control_handle = udf_handles
        .pop()
        .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    if !udf_handles.is_empty()
        || directory_identity(&control_handle)? != directory_identity(&duplicate_control_handle)?
    {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    validate_local_volume(&user_data_handle)?;
    validate_profile_acl(&control_handle)?;
    validate_profile_acl(&user_data_handle)?;
    Ok(SelectedWindowsProfile {
        plan,
        ancestor_handles,
        control_handle,
        user_data_handle,
        lease,
        lifecycle,
        recovery_required,
    })
}

fn prepare_profile_before_event_loop(
    local_app_data: &Path,
    selection: WebProfileSelection,
) -> Result<SelectedWindowsProfile, WvError> {
    initialize_process_dpi_awareness()?;
    initialize_com_sta()?;
    let mut profile = prepare_windows_profile_at(local_app_data, selection)?;
    recover_windows_profile(&mut profile)?;
    Ok(profile)
}

fn open_persistent_purge_owner(
    local_app_data: &Path,
    identity: ProfileIdentity,
) -> Result<PersistentPurgeOwner, WvError> {
    let plan = windows_profile_plan(local_app_data, WebProfileSelection::Persistent(identity))?;
    let (mut ancestor_handles, _) =
        retain_directory_chain(local_app_data, &plan.control_dir, false, false)?;
    let control_handle = ancestor_handles
        .pop()
        .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    validate_or_write_marker(&plan, false)?;
    let lease = open_control_file(
        &plan.control_dir.join(PROFILE_LEASE),
        ControlFileMode::Lease,
    )?;
    validate_local_volume(&control_handle)?;
    validate_profile_acl(&control_handle)?;
    let lifecycle = ProfileLifecycleRecord::from_windows_record_bytes(&read_control_record(
        &plan.control_dir.join(PROFILE_LIFECYCLE),
    )?)?;

    let user_data_handle = match fs::symlink_metadata(&plan.user_data_dir) {
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Ok(metadata)
            if metadata.is_dir()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0 =>
        {
            let (mut handles, _) =
                retain_directory_chain(&plan.control_dir, &plan.user_data_dir, false, false)?;
            let user_data = handles
                .pop()
                .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
            let duplicate_control = handles
                .pop()
                .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
            if !handles.is_empty()
                || directory_identity(&duplicate_control)? != directory_identity(&control_handle)?
            {
                return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
            }
            validate_local_volume(&user_data)?;
            validate_profile_acl(&user_data)?;
            Some(user_data)
        }
        Ok(_) | Err(_) => return Err(profile_failure(ProfileErrorKind::MarkerMismatch)),
    };
    Ok(PersistentPurgeOwner {
        plan,
        ancestor_handles,
        control_handle,
        user_data_handle,
        lease,
        lifecycle,
    })
}

fn recover_purge_owner(
    mut owner: PersistentPurgeOwner,
    allow_recovery_probe: bool,
) -> Result<PersistentPurgeOwner, WvError> {
    let current = current_profile_process()?;
    let observation = if owner.lifecycle.owner() == Some(current) {
        RecordedProcessObservation::Live
    } else {
        RecordedProcessObservation::Dead
    };
    match next_windows_lifecycle_action(owner.lifecycle, observation)? {
        ProfileLifecycleAction::BeginStartup => return Ok(owner),
        ProfileLifecycleAction::WriteQuarantined => {
            owner.lifecycle = owner.lifecycle.quarantine()?;
            replace_lifecycle(&owner.plan, owner.lifecycle)?;
        }
        ProfileLifecycleAction::RunWindowsExclusiveUdfRecovery => {}
        ProfileLifecycleAction::RestoreIdleAfterBoot => {
            return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
        }
    }
    if !allow_recovery_probe {
        return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
    }
    let user_data_handle = owner
        .user_data_handle
        .take()
        .ok_or_else(|| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let candidate = SelectedWindowsProfile {
        plan: owner.plan.clone(),
        ancestor_handles: owner.ancestor_handles,
        control_handle: owner.control_handle,
        user_data_handle,
        lease: owner.lease,
        lifecycle: Some(owner.lifecycle),
        recovery_required: true,
    };
    if prove_exclusive_udf_released_on_cleanup_sta(&candidate)? == ExclusiveUdfRelease::Busy {
        return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
    }
    let idle = owner.lifecycle.complete_windows_recovery()?;
    replace_lifecycle(&candidate.plan, idle)?;
    Ok(PersistentPurgeOwner {
        plan: candidate.plan,
        ancestor_handles: candidate.ancestor_handles,
        control_handle: candidate.control_handle,
        user_data_handle: Some(candidate.user_data_handle),
        lease: candidate.lease,
        lifecycle: idle,
    })
}

fn replace_purge_record(
    plan: &WindowsProfilePlan,
    record: ProfilePurgeRecord,
) -> Result<(), WvError> {
    replace_control_record(
        &plan.control_dir.join(PROFILE_PURGE),
        &plan.control_dir.join(PROFILE_PURGE_PENDING),
        &record.to_record_bytes()?,
    )
}

fn purge_persistent_profile_at(
    local_app_data: &Path,
    identity: ProfileIdentity,
    allow_recovery_probe: bool,
) -> Result<(), WvError> {
    let owner = open_persistent_purge_owner(local_app_data, identity)?;
    let intent_path = owner.plan.control_dir.join(PROFILE_PURGE);
    let mut intent = if control_file_present(&intent_path)? {
        ProfilePurgeRecord::from_record_bytes(&read_control_record(&intent_path)?)?
    } else {
        let prepared = ProfilePurgeRecord::prepared(identity, ProfilePlatform::Windows);
        write_new_control_record(&intent_path, &prepared.to_record_bytes()?)?;
        prepared
    };
    if intent.identity() != identity || intent.platform() != ProfilePlatform::Windows {
        return Err(profile_failure(ProfileErrorKind::ActiveIntent));
    }
    let owner = recover_purge_owner(owner, allow_recovery_probe)?;

    if intent.phase() == ProfilePurgePhase::Prepared {
        if let Some(user_data_handle) = owner.user_data_handle {
            same_directory_as_handle(&owner.plan.user_data_dir, &user_data_handle)?;
            drop(user_data_handle);
            fs::remove_dir_all(&owner.plan.user_data_dir)
                .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
        }
        intent = intent.advance(ProfilePurgePhase::DataRemoved)?;
        replace_purge_record(&owner.plan, intent)?;
    }
    if intent.phase() == ProfilePurgePhase::DataRemoved {
        if !directory_is_absent(&owner.plan.user_data_dir)? {
            return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
        }
        intent = intent.advance(ProfilePurgePhase::Completed)?;
        replace_purge_record(&owner.plan, intent)?;
    }
    if intent.phase() != ProfilePurgePhase::Completed
        || !directory_is_absent(&owner.plan.user_data_dir)?
    {
        return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
    }
    let intent_handle = open_control_file(&intent_path, ControlFileMode::Read)?;
    drop(intent_handle);
    fs::remove_file(intent_path)
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    drop(owner.lease);
    drop(owner.control_handle);
    drop(owner.ancestor_handles);
    Ok(())
}

fn directory_is_absent(path: &Path) -> Result<bool, WvError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(true),
        Ok(_) => Ok(false),
        Err(_) => Err(profile_failure(ProfileErrorKind::LifecycleUnproven)),
    }
}

fn environment_user_data_folder(
    environment: &ICoreWebView2Environment,
) -> Result<PathBuf, WvError> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment7;

    let environment7: ICoreWebView2Environment7 = environment
        .cast()
        .map_err(|_| profile_failure(ProfileErrorKind::RegistryCorruption))?;
    let mut raw = PWSTR::null();
    // SAFETY: live environment on its STA and writable COM-owned string output.
    unsafe { environment7.UserDataFolder(&raw mut raw) }
        .map_err(|_| profile_failure(ProfileErrorKind::RegistryCorruption))?;
    // SAFETY: the successful call returned a live NUL-terminated string.
    let text = unsafe { raw.to_string() };
    // SAFETY: `raw` is the one COM allocation returned above.
    unsafe { CoTaskMemFree(Some(raw.as_ptr().cast())) };
    text.map(PathBuf::from)
        .map_err(|_| profile_failure(ProfileErrorKind::RegistryCorruption))
}

fn same_directory_as_handle(path: &Path, expected: &File) -> Result<(), WvError> {
    let actual = open_directory_handle(path)?;
    if directory_identity(&actual)? != directory_identity(expected)? {
        return Err(profile_failure(ProfileErrorKind::RegistryCorruption));
    }
    Ok(())
}

fn directory_identity(file: &File) -> Result<(u32, u64), WvError> {
    let information = file_information(file)?;
    let index =
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
    Ok((information.dwVolumeSerialNumber, index))
}

fn file_information(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, WvError> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `file` owns a live directory handle and `information` is a valid
    // writable output for the duration of this synchronous call.
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle().cast()), &raw mut information)
    }
    .map_err(|_| profile_failure(ProfileErrorKind::RegistryCorruption))?;
    Ok(information)
}

fn validate_local_volume(directory: &File) -> Result<(), WvError> {
    let handle = HANDLE(directory.as_raw_handle().cast());
    let mut final_path = vec![0_u16; 32_768];
    // SAFETY: `directory` retains a live directory object while the writable
    // buffer is used. The DOS form is derived from that object, not from an
    // environment or caller-selected path.
    let length = unsafe { GetFinalPathNameByHandleW(handle, &mut final_path, VOLUME_NAME_DOS) };
    let length =
        usize::try_from(length).map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    if length == 0 || length >= final_path.len() {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    final_path[length] = 0;
    let mut volume_root = vec![0_u16; 32_768];
    // SAFETY: the first buffer contains the NUL-terminated final DOS path
    // returned for the retained handle and the second is writable.
    unsafe { GetVolumePathNameW(PCWSTR(final_path.as_ptr()), &mut volume_root) }
        .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    // SAFETY: `GetVolumePathNameW` returned a NUL-terminated volume root.
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(volume_root.as_ptr())) };
    if !matches!(drive_type, DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_RAMDISK) {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    Ok(())
}

fn validate_profile_acl(directory: &File) -> Result<(), WvError> {
    let current =
        current_process_sid().map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    let descriptor = GetSecurityInfo(
        directory,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Owner | SecurityInformation::Dacl,
    )
    .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    if descriptor.owner() != Some(&current) {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    let dacl = descriptor
        .dacl()
        .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    if dacl_has_untrusted_write(dacl, &current.to_string()) {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    Ok(())
}

fn dacl_has_untrusted_write(dacl: &Acl, current_user_sid: &str) -> bool {
    use windows_permissions::constants::AceType;

    let write = AccessRights::GenericWrite
        | AccessRights::GenericAll
        | AccessRights::Delete
        | AccessRights::WriteDac
        | AccessRights::WriteOwner
        | AccessRights::Bit1
        | AccessRights::Bit2
        | AccessRights::Bit4
        | AccessRights::Bit6
        | AccessRights::Bit8;
    (0..dacl.len()).any(|index| {
        let Some(ace) = dacl.get_ace(index) else {
            return true;
        };
        if !matches!(
            ace.ace_type(),
            AceType::ACCESS_ALLOWED_ACE_TYPE
                | AceType::ACCESS_ALLOWED_CALLBACK_ACE_TYPE
                | AceType::ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE
                | AceType::ACCESS_ALLOWED_OBJECT_ACE_TYPE
        ) || !ace.mask().intersects(write)
        {
            return false;
        }
        let Some(sid) = ace.sid() else {
            return true;
        };
        let sid = sid.to_string();
        sid != current_user_sid
            && sid != "S-1-5-18"
            && sid != "S-1-5-32-544"
            && !sid.starts_with("S-1-15-2-")
            && !sid.starts_with("S-1-15-3-")
    })
}

/// Creates the shared `WebView2` environment for this engine.
///
/// Options are Keld-owned and deliberately the `webview2-com` defaults, which
/// means **empty** `AdditionalBrowserArguments`: wry's default disabled
/// `SmartScreen` via `--disable-features=…,msSmartScreenProtection` (KEL-66),
/// and Keld does not inherit that. Environment creation is cheap (3–6 ms
/// measured) — it only resolves the runtime; the browser process launches at
/// first controller creation (`learn.microsoft.com`, `WebView2` process
/// model).
fn create_environment_for_profile(
    profile: &SelectedWindowsProfile,
) -> Result<ICoreWebView2Environment, WvError> {
    create_environment_for_profile_with_options(profile, CoreWebView2EnvironmentOptions::default())
}

fn create_environment_for_profile_with_options(
    profile: &SelectedWindowsProfile,
    options: CoreWebView2EnvironmentOptions,
) -> Result<ICoreWebView2Environment, WvError> {
    create_environment_for_profile_with_options_until(
        profile,
        options,
        Instant::now() + PROFILE_RELEASE_DEADLINE,
    )
}

fn create_environment_for_profile_with_options_until(
    profile: &SelectedWindowsProfile,
    options: CoreWebView2EnvironmentOptions,
    deadline: Instant,
) -> Result<ICoreWebView2Environment, WvError> {
    // SAFETY: options is private and unpublished. The pinned implementation
    // exposes the documented EnvironmentOptions2 exclusive-UDF property.
    unsafe { options.set_exclusive_user_data_folder_access(true) };
    let environment =
        create_environment_with_options_until(&profile.plan.user_data_dir, options, deadline)?;
    let actual = environment_user_data_folder(&environment)?;
    same_directory_as_handle(&actual, &profile.user_data_handle)?;
    validate_profile_acl(&profile.user_data_handle)?;
    Ok(environment)
}

struct BrowserExitObservation {
    receiver: Receiver<Result<(), WvError>>,
    environment: ICoreWebView2Environment5,
    token: i64,
}

const fn matching_browser_exit(expected: u32, actual: u32, normal: bool) -> bool {
    expected != 0 && expected == actual && normal
}

fn observe_profile_browser_exit(
    environment: &ICoreWebView2Environment,
    expected_pid: Arc<AtomicU32>,
    wake: Option<EventLoopProxy<WindowsLoopEvent>>,
) -> Result<BrowserExitObservation, WvError> {
    let environment5: ICoreWebView2Environment5 = environment
        .cast()
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let (tx, rx) = mpsc::channel();
    let mut token = 0_i64;
    let handler = BrowserProcessExitedEventHandler::create(Box::new(move |_, args| {
        let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
        let mut pid = 0_u32;
        let mut kind = COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND_NORMAL;
        // SAFETY: WebView2 supplies live event args on the environment STA.
        unsafe {
            args.BrowserProcessId(&raw mut pid)?;
            args.BrowserProcessExitKind(&raw mut kind)?;
        }
        let expected = expected_pid.load(Ordering::Acquire);
        let normal = kind == COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND_NORMAL;
        #[cfg(all(feature = "media-acceptance", test))]
        media_acceptance::observe_browser_exit(pid, expected, normal);
        if matching_browser_exit(expected, pid, normal) || (expected != 0 && expected == pid) {
            let result = if normal {
                Ok(())
            } else {
                Err(profile_failure(ProfileErrorKind::LifecycleUnproven))
            };
            tx.send(result)
                .map_err(|_| windows::core::Error::from(E_UNEXPECTED))?;
            if let Some(wake) = wake.as_ref() {
                wake.send_event(WindowsLoopEvent::ProfileReleaseWake)
                    .map_err(|_| windows::core::Error::from(E_UNEXPECTED))?;
            }
        }
        Ok(())
    }));
    // SAFETY: the environment, handler and writable token live on this STA;
    // WebView2 retains the callback until removal.
    unsafe { environment5.add_BrowserProcessExited(&handler, &raw mut token) }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    Ok(BrowserExitObservation {
        receiver: rx,
        environment: environment5,
        token,
    })
}

#[cfg(all(feature = "media-acceptance", test))]
fn create_environment_with_options(
    directory: &std::path::Path,
    options: CoreWebView2EnvironmentOptions,
) -> Result<ICoreWebView2Environment, WvError> {
    create_environment_with_options_until(
        directory,
        options,
        Instant::now() + PROFILE_RELEASE_DEADLINE,
    )
}

fn create_environment_with_options_until(
    directory: &std::path::Path,
    options: CoreWebView2EnvironmentOptions,
    deadline: Instant,
) -> Result<ICoreWebView2Environment, WvError> {
    let user_data = HSTRING::from(directory.as_os_str());
    let (tx, rx) = mpsc::channel();

    // SAFETY: called on the engine thread with a live STA (module note). The
    // handler is a one-shot completion callback delivered via this thread's
    // message pump; `tx` outlives the wait below because `rx` blocks in
    // `wait_with_pump` until the send happens or the loader fails the call.
    let launched = unsafe {
        CreateCoreWebView2EnvironmentWithOptions(
            windows::core::PCWSTR::null(),
            &user_data,
            &ICoreWebView2EnvironmentOptions::from(options),
            &CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                move |result, environment| {
                    let environment = result
                        .and(environment.ok_or_else(|| windows::core::Error::from(E_POINTER)));
                    tx.send(environment)
                        .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
                },
            )),
        )
    };
    if let Err(err) = launched {
        return Err(WvError::WebView2RuntimeMissing {
            detail: err.to_string(),
        });
    }

    let environment = wait_with_message_pump_until(&rx, deadline)?;
    environment.map_err(|err| WvError::WebView2RuntimeMissing {
        detail: err.to_string(),
    })
}

/// Creates the controller that hosts one webview inside `hwnd`.
///
/// This is where the runtime processes actually launch — 313–506 ms measured,
/// and per Microsoft "the bulk of starting a `WebView2` control"
/// (`WebView2Feedback` #1536). Everything Keld controls happens around it, not
/// inside it.
fn create_controller(
    environment: &ICoreWebView2Environment,
    hwnd: HWND,
) -> Result<ICoreWebView2Controller, WvError> {
    create_controller_observed(environment, hwnd).map_err(|error| WvError::WebView2RuntimeMissing {
        detail: error.to_string(),
    })
}

enum ControllerCreationError {
    Windows(windows::core::Error),
    Pump(WvError),
}

impl ControllerCreationError {
    fn is_invalid_state(&self) -> bool {
        matches!(
            self,
            Self::Windows(error)
                if error.code() == windows::core::HRESULT::from_win32(ERROR_INVALID_STATE.0)
        )
    }
}

impl fmt::Display for ControllerCreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Windows(error) => error.fmt(f),
            Self::Pump(error) => error.fmt(f),
        }
    }
}

fn create_controller_observed(
    environment: &ICoreWebView2Environment,
    hwnd: HWND,
) -> Result<ICoreWebView2Controller, ControllerCreationError> {
    create_controller_observed_until(environment, hwnd, Instant::now() + PROFILE_RELEASE_DEADLINE)
}

fn create_controller_observed_until(
    environment: &ICoreWebView2Environment,
    hwnd: HWND,
    deadline: Instant,
) -> Result<ICoreWebView2Controller, ControllerCreationError> {
    let (tx, rx) = mpsc::channel();

    // SAFETY: `environment` was created on this thread (STA) and `hwnd` is a
    // live tao window owned by the caller for the duration of the call. The
    // completion handler is one-shot and delivered via this thread's pump.
    let launched = unsafe {
        environment.CreateCoreWebView2Controller(
            hwnd,
            &CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                move |result, controller| {
                    let controller =
                        result.and(controller.ok_or_else(|| windows::core::Error::from(E_POINTER)));
                    tx.send(controller)
                        .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
                },
            )),
        )
    };
    launched.map_err(ControllerCreationError::Windows)?;
    wait_with_message_pump_until(&rx, deadline)
        .map_err(ControllerCreationError::Pump)?
        .map_err(ControllerCreationError::Windows)
}

fn webview_browser_process_id(webview: &ICoreWebView2) -> Option<u32> {
    let mut pid = 0_u32;
    // SAFETY: the webview is live on its creating STA thread.
    unsafe { webview.BrowserProcessId(&raw mut pid) }
        .ok()
        .and_then(|()| (pid != 0).then_some(pid))
}

fn remove_browser_exit_observer(
    environment: &ICoreWebView2Environment5,
    token: i64,
) -> Result<(), WvError> {
    // SAFETY: the handler token belongs to this live environment on its STA
    // and every caller removes it at most once.
    unsafe { environment.remove_BrowserProcessExited(token) }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))
}

struct RecoveryWindow(HWND);

impl RecoveryWindow {
    fn new() -> Result<Self, WvError> {
        // SAFETY: the predefined STATIC class needs no registration, all
        // optional owner/menu/instance/parameter inputs are absent, and the
        // hidden top-level window remains on this STA. Contract:
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-createwindowexw
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("Keld profile recovery"),
                WS_OVERLAPPED,
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )
        }
        .map_err(|error| WvError::Window(error.to_string()))?;
        Ok(Self(window))
    }
}

impl Drop for RecoveryWindow {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the live HWND on its creating STA and
        // destroys it once. Contract:
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-destroywindow
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

fn recover_windows_profile(profile: &mut SelectedWindowsProfile) -> Result<(), WvError> {
    if !profile.recovery_required {
        return Ok(());
    }
    let quarantined = profile
        .lifecycle
        .ok_or_else(|| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    if next_windows_lifecycle_action(quarantined, RecordedProcessObservation::Unknown)?
        != ProfileLifecycleAction::RunWindowsExclusiveUdfRecovery
    {
        return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
    }

    if prove_exclusive_udf_released(profile)? == ExclusiveUdfRelease::Busy {
        return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
    }
    let idle = quarantined.complete_windows_recovery()?;
    replace_lifecycle(&profile.plan, idle)?;
    let starting = idle.begin_startup(current_profile_process()?)?;
    replace_lifecycle(&profile.plan, starting)?;
    profile.lifecycle = Some(starting);
    profile.recovery_required = false;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExclusiveUdfRelease {
    Released,
    Busy,
}

fn prove_exclusive_udf_released_on_cleanup_sta(
    profile: &SelectedWindowsProfile,
) -> Result<ExclusiveUdfRelease, WvError> {
    thread::scope(|scope| {
        scope
            .spawn(|| {
                initialize_com_sta()?;
                let result = prove_exclusive_udf_released(profile);
                // SAFETY: balances this thread's successful CoInitializeEx
                // after all recovery COM objects were dropped. Contract:
                // https://learn.microsoft.com/windows/win32/api/combaseapi/nf-combaseapi-couninitialize
                unsafe { CoUninitialize() };
                result
            })
            .join()
            .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
    })
}

fn prove_exclusive_udf_released(
    profile: &SelectedWindowsProfile,
) -> Result<ExclusiveUdfRelease, WvError> {
    let window = RecoveryWindow::new()?;
    prove_exclusive_udf_released_on_hwnd(profile, window.0)
}

fn prove_exclusive_udf_released_on_hwnd(
    profile: &SelectedWindowsProfile,
    window: HWND,
) -> Result<ExclusiveUdfRelease, WvError> {
    let deadline = Instant::now() + PROFILE_RELEASE_DEADLINE;
    let environment = create_environment_for_profile_with_options_until(
        profile,
        CoreWebView2EnvironmentOptions::default(),
        deadline,
    )?;
    let expected_pid = Arc::new(AtomicU32::new(0));
    let observation = observe_profile_browser_exit(&environment, Arc::clone(&expected_pid), None)?;
    let controller = match create_controller_observed_until(&environment, window, deadline) {
        Ok(controller) => controller,
        Err(error) if error.is_invalid_state() => {
            remove_browser_exit_observer(&observation.environment, observation.token)?;
            return Ok(ExclusiveUdfRelease::Busy);
        }
        Err(error) => {
            remove_browser_exit_observer(&observation.environment, observation.token)?;
            return Err(WvError::WebView2RuntimeMissing {
                detail: error.to_string(),
            });
        }
    };
    // SAFETY: the controller was created on this STA for the live recovery
    // window. No navigation or content is created by this probe.
    let webview = unsafe { controller.CoreWebView2() }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let pid = webview_browser_process_id(&webview)
        .ok_or_else(|| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    expected_pid.store(pid, Ordering::Release);
    // SAFETY: the recovery controller is live on this STA and is closed once.
    unsafe { controller.Close() }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    drop(webview);
    drop(controller);
    let observed = wait_with_message_pump_until(&observation.receiver, deadline)?;
    remove_browser_exit_observer(&observation.environment, observation.token)?;
    observed?;
    Ok(ExclusiveUdfRelease::Released)
}

fn delete_ephemeral_profile(profile: SelectedWindowsProfile) -> Result<(), WvError> {
    if !profile.plan.cleanup_on_release {
        return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
    }
    validate_or_write_marker(&profile.plan, false)?;
    same_directory_as_handle(&profile.plan.control_dir, &profile.control_handle)?;
    same_directory_as_handle(&profile.plan.user_data_dir, &profile.user_data_handle)?;
    let user_data_dir = profile.plan.user_data_dir.clone();
    drop(profile.user_data_handle);
    fs::remove_dir_all(&user_data_dir)
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    delete_ephemeral_control(
        &profile.plan,
        profile.ancestor_handles,
        profile.control_handle,
        profile.lease,
    )
}

fn delete_ephemeral_control(
    plan: &WindowsProfilePlan,
    ancestor_handles: Vec<File>,
    control_handle: File,
    lease: File,
) -> Result<(), WvError> {
    let mut marker = false;
    let mut lock = false;
    for entry in fs::read_dir(&plan.control_dir)
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
    {
        let name = entry
            .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
            .file_name();
        if name == PROFILE_MARKER {
            marker = true;
        } else if name == PROFILE_LEASE {
            lock = true;
        } else {
            return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
        }
    }
    if !marker || !lock {
        return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
    }
    let marker_path = plan.control_dir.join(PROFILE_MARKER);
    let lease_path = plan.control_dir.join(PROFILE_LEASE);
    let control_dir = plan.control_dir.clone();
    drop(lease);
    drop(control_handle);
    drop(ancestor_handles);
    fs::remove_file(marker_path)
        .and_then(|()| fs::remove_file(lease_path))
        .and_then(|()| fs::remove_dir(control_dir))
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))
}

fn try_scavenge_ephemeral_profile(
    local_app_data: &Path,
    profile: EphemeralProfile,
) -> Result<bool, WvError> {
    let plan = windows_profile_plan(local_app_data, WebProfileSelection::ephemeral_dev(profile))?;
    let (mut ancestor_handles, _) =
        retain_directory_chain(local_app_data, &plan.control_dir, false, false)?;
    let control_handle = ancestor_handles
        .pop()
        .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    validate_or_write_marker(&plan, false)?;
    let lease = open_control_file(
        &plan.control_dir.join(PROFILE_LEASE),
        ControlFileMode::Lease,
    )?;
    validate_local_volume(&control_handle)?;
    validate_profile_acl(&control_handle)?;

    match fs::symlink_metadata(&plan.user_data_dir) {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            delete_ephemeral_control(&plan, ancestor_handles, control_handle, lease)?;
            return Ok(true);
        }
        Ok(metadata)
            if metadata.is_dir()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0 => {}
        Ok(_) | Err(_) => return Err(profile_failure(ProfileErrorKind::MarkerMismatch)),
    }
    let (mut user_data_handles, _) =
        retain_directory_chain(&plan.control_dir, &plan.user_data_dir, false, false)?;
    let user_data_handle = user_data_handles
        .pop()
        .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    let duplicate_control = user_data_handles
        .pop()
        .ok_or_else(|| profile_failure(ProfileErrorKind::MarkerMismatch))?;
    if !user_data_handles.is_empty()
        || directory_identity(&duplicate_control)? != directory_identity(&control_handle)?
    {
        return Err(profile_failure(ProfileErrorKind::MarkerMismatch));
    }
    validate_local_volume(&user_data_handle)?;
    validate_profile_acl(&user_data_handle)?;
    let candidate = SelectedWindowsProfile {
        plan,
        ancestor_handles,
        control_handle,
        user_data_handle,
        lease,
        lifecycle: None,
        recovery_required: false,
    };
    match prove_exclusive_udf_released(&candidate)? {
        ExclusiveUdfRelease::Busy => return Ok(false),
        ExclusiveUdfRelease::Released => {}
    }
    delete_ephemeral_profile(candidate)?;
    Ok(true)
}

fn scavenge_ephemeral_profiles(local_app_data: &Path) -> Result<usize, WvError> {
    // One old leaf is attempted during a subsequent dev host's graceful
    // teardown, after that host's own BrowserProcessExited barrier. This is a
    // bounded cleanup schedule, not a startup-path or performance claim.
    let root = local_app_data.join("Keld").join("ephemeral").join("v1");
    match fs::symlink_metadata(&root) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
        Ok(metadata)
            if metadata.is_dir()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0 => {}
        Ok(_) | Err(_) => return Err(profile_failure(ProfileErrorKind::MarkerMismatch)),
    }
    let (_root_handles, _) = retain_directory_chain(local_app_data, &root, false, false)?;
    let mut removed = 0;
    for entry in fs::read_dir(&root)
        .map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?
        .take(MAX_EPHEMERAL_SCAVENGE_PER_LAUNCH)
    {
        let entry = entry.map_err(|_| profile_failure(ProfileErrorKind::MarkerMismatch))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(profile) = EphemeralProfile::from_namespace_segment(&name) else {
            continue;
        };
        match try_scavenge_ephemeral_profile(local_app_data, profile) {
            Ok(true) => removed += 1,
            Ok(false) => {}
            Err(WvError::ProfileSelection(error))
                if error.kind() == ProfileErrorKind::ProfileInUse => {}
            Err(error) => return Err(error),
        }
    }
    Ok(removed)
}

struct SavedPermission {
    kind: COREWEBVIEW2_PERMISSION_KIND,
    state: COREWEBVIEW2_PERMISSION_STATE,
    origin: String,
}

fn saved_media_permission_needs_deny(permission: &SavedPermission) -> bool {
    matches!(
        permission.kind,
        COREWEBVIEW2_PERMISSION_KIND_CAMERA | COREWEBVIEW2_PERMISSION_KIND_MICROPHONE
    ) && permission.state != COREWEBVIEW2_PERMISSION_STATE_DENY
}

fn profile_permission_settings(
    profile: &ICoreWebView2Profile4,
) -> Result<Vec<SavedPermission>, WvError> {
    let (tx, rx) = mpsc::channel();
    let handler = GetNonDefaultPermissionSettingsCompletedHandler::create(Box::new(
        move |result, settings| {
            tx.send(result.and(settings.ok_or_else(|| windows::core::Error::from(E_POINTER))))
                .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
        },
    ));
    // SAFETY: the profile and one-shot completion handler live on this STA;
    // `wait_with_pump` retains the receiver until the callback completes.
    unsafe { profile.GetNonDefaultPermissionSettings(&handler) }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let settings = wait_with_pump(rx)
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let mut count = 0_u32;
    // SAFETY: the returned collection is live and `count` is writable.
    unsafe { settings.Count(&raw mut count) }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let mut permissions = Vec::new();
    for index in 0..count {
        // SAFETY: `index` is bounded by the collection's observed count.
        let setting = unsafe { settings.GetValueAtIndex(index) }
            .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
        let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
        let mut state = COREWEBVIEW2_PERMISSION_STATE::default();
        let mut origin = PWSTR::null();
        // SAFETY: this live setting owns all three synchronous getter outputs.
        let observed = unsafe { setting.PermissionKind(&raw mut kind) }
            .and_then(|()| unsafe { setting.PermissionState(&raw mut state) })
            .and_then(|()| unsafe { setting.PermissionOrigin(&raw mut origin) });
        observed.map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
        // SAFETY: the successful origin getter returned one COM-allocated,
        // NUL-terminated string. It is copied before being freed exactly once.
        let text = unsafe { origin.to_string() };
        // SAFETY: this is the one allocation returned by PermissionOrigin.
        unsafe { CoTaskMemFree(Some(origin.as_ptr().cast())) };
        permissions.push(SavedPermission {
            kind,
            state,
            origin: text.map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?,
        });
    }
    Ok(permissions)
}

fn set_profile_permission_state(
    profile: &ICoreWebView2Profile4,
    permission: &SavedPermission,
    state: COREWEBVIEW2_PERMISSION_STATE,
) -> Result<(), WvError> {
    let origin = permission
        .origin
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let (tx, rx) = mpsc::channel();
    let handler = SetPermissionStateCompletedHandler::create(Box::new(move |result| {
        tx.send(result)
            .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
    }));
    // SAFETY: the profile, NUL-terminated origin and one-shot handler live on
    // this STA through the pumped completion. The current v0 webview policy is
    // fail-closed, so persisted camera/microphone decisions are reconciled to
    // deny before any app navigation can consult them.
    unsafe {
        profile.SetPermissionState(permission.kind, PCWSTR(origin.as_ptr()), state, &handler)
    }
    .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    wait_with_pump(rx)
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))
}

fn webview_profile(webview: &ICoreWebView2) -> Result<ICoreWebView2Profile4, WvError> {
    let webview13: ICoreWebView2_13 = webview
        .cast()
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    // SAFETY: the webview is live on its owning STA.
    unsafe { webview13.Profile() }
        .and_then(|profile| profile.cast())
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))
}

fn reconcile_saved_media_permissions(webview: &ICoreWebView2) -> Result<(), WvError> {
    let profile = webview_profile(webview)?;
    for permission in profile_permission_settings(&profile)?
        .iter()
        .filter(|permission| saved_media_permission_needs_deny(permission))
    {
        set_profile_permission_state(&profile, permission, COREWEBVIEW2_PERMISSION_STATE_DENY)?;
    }
    Ok(())
}

fn saved_permission_reconciliation_enabled() -> bool {
    #[cfg(all(feature = "media-acceptance", test))]
    if std::env::var_os("KELD_PROFILE_TEST_SKIP_SAVED_RECONCILIATION").is_some() {
        return false;
    }
    true
}

/// Proof that media permissions and popup denial are registered on a webview.
///
/// [`navigate_initial`] demands it, so "content ran before the guard existed"
/// is a compile error rather than a review catch. Only
/// [`install_guarded_media_permissions`] mints one.
struct GuardInstalled<'view>(&'view ICoreWebView2);

fn webview2_permission_state(allowed: bool) -> COREWEBVIEW2_PERMISSION_STATE {
    if allowed {
        COREWEBVIEW2_PERMISSION_STATE_ALLOW
    } else {
        COREWEBVIEW2_PERMISSION_STATE_DENY
    }
}

fn canonical_webview_identity(webview: &ICoreWebView2) -> windows::core::Result<usize> {
    let identity: IUnknown = webview.cast()?;
    Ok(identity.as_raw() as usize)
}

/// Registers the default-deny media-capture handler backed by `keld-guard`
/// (KEL-59 parity with the macOS backend).
///
/// Without a handler `WebView2` falls back to its own **user prompt** —
/// default-ask, not default-deny. A prompt lets a renderer obtain capture the
/// manifest never granted, and the manifest is the authority
/// (`docs/architecture/03-security.md` §1). Kinds without a Keld capability
/// (geolocation, notifications, …) are denied outright: there is no v0
/// capability for them, and fail-closed is the crate rule.
fn install_guarded_media_permissions(
    webview: &ICoreWebView2,
    manifest: PermissionsManifest,
    principal: Principal,
) -> Result<GuardInstalled<'_>, WvError> {
    let registered_identity = canonical_webview_identity(webview)
        .map_err(|error| WvError::Webview(format!("permission identity: {error}")))?;
    // Built outside the registration's `unsafe` block so the COM calls inside
    // the callback carry their own SAFETY proofs instead of inheriting one
    // lexically.
    let handler = PermissionRequestedEventHandler::create(Box::new(move |sender, args| {
        // Fail closed: without args no state can be set, and `Ok(())` would
        // silently hand the decision back to WebView2's own prompt
        // (default-ask). An error at least refuses to report success.
        let Some(args) = args else {
            return Err(windows::core::Error::from(E_POINTER));
        };
        let Ok(Some(sender_identity)) = sender.as_ref().map(canonical_webview_identity).transpose()
        else {
            // SAFETY: failure to canonicalize the sender is an identity
            // failure, so complete the request as denied.
            return unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) };
        };
        if sender_identity != registered_identity {
            // SAFETY: live callback args; a foreign/missing sender must never
            // fall through to WebView2's DEFAULT prompt behavior.
            return unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) };
        }

        let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
        // SAFETY: `args` is live for the duration of the callback; the
        // out-pointer is valid.
        if unsafe { args.PermissionKind(&raw mut kind) }.is_err() {
            // SAFETY: the same live callback args accept a by-value state.
            // Enforcing DENY takes precedence over propagating a getter error
            // that could leave WebView2 on DEFAULT/prompt behavior.
            return unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) };
        }
        #[cfg(all(feature = "media-acceptance", test))]
        let before = {
            let mut before = COREWEBVIEW2_PERMISSION_STATE_DEFAULT;
            // SAFETY: same live callback arguments and writable enum output.
            if unsafe { args.State(&raw mut before) }.is_err() {
                // SAFETY: keep the evidence-only getter fail-closed too.
                return unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) };
            }
            before
        };

        let state = webview2_permission_state(media_permission_allowed(
            &manifest,
            Some(principal),
            webview2_media_kind(kind),
        ));
        // SAFETY: same liveness as above; `SetState` takes the enum by value.
        unsafe { args.SetState(state) }?;
        #[cfg(all(feature = "media-acceptance", test))]
        {
            let mut readback = COREWEBVIEW2_PERMISSION_STATE_DEFAULT;
            // SAFETY: the callback still owns the same live args. Reading the
            // value here proves the state before this callback returns.
            if unsafe { args.State(&raw mut readback) }.is_err() {
                return Ok(());
            }
            let Ok(uri) = media_acceptance::permission_uri(&args) else {
                return Ok(());
            };
            media_acceptance::observe_production_effect(
                kind,
                before,
                state,
                readback,
                uri,
                sender_identity,
            );
        }
        Ok(())
    }));

    let mut token = 0_i64;
    // SAFETY: `webview` lives on this thread (module note). The handler runs
    // on this same thread for the webview's whole life — WebView2 raises
    // events on the creating thread.
    let registered = unsafe { webview.add_PermissionRequested(&handler, &raw mut token) };
    registered.map_err(|err| WvError::Webview(format!("permission handler: {err}")))?;
    #[cfg(all(feature = "media-acceptance", test))]
    media_acceptance::observe_registration(registered_identity, token);

    // Windows WebView2 (KEL-168): an unhandled new-window request creates a
    // popup outside Keld's principal, permission, and lifecycle accounting.
    // v0 denies every popup; SetHandled(true) without NewWindow loads nothing.
    // https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2newwindowrequestedeventargs#put_handled
    let popup_handler = NewWindowRequestedEventHandler::create(Box::new(|_, args| {
        let Some(args) = args else {
            return Err(windows::core::Error::from(E_POINTER));
        };
        // SAFETY: WebView2 supplies live event args on the creating STA thread
        // for this callback. SetHandled takes a BOOL by value (contract above).
        unsafe { args.SetHandled(true) }
    }));
    // SAFETY: `webview` and the writable token are live on the creating STA.
    // WebView2 retains the COM handler and invokes it on that same thread.
    unsafe { webview.add_NewWindowRequested(&popup_handler, &raw mut token) }
        .map_err(|err| WvError::Webview(format!("popup handler: {err}")))?;
    Ok(GuardInstalled(webview))
}

/// Performs the first navigation of a freshly created webview.
///
/// Takes [`GuardInstalled`] so the type system enforces the crate rule that no
/// content runs before the permission guard is registered.
fn navigate_initial(guard: &GuardInstalled<'_>, target: &NavTarget) -> Result<(), WvError> {
    let webview = guard.0;
    // SAFETY: `webview` lives on this thread; both calls take an HSTRING by
    // reference that outlives the call.
    let loaded = match target {
        NavTarget::Html(html) => unsafe { webview.NavigateToString(&HSTRING::from(html.as_str())) },
        NavTarget::Url(url) => unsafe { webview.Navigate(&HSTRING::from(url.as_str())) },
    };
    loaded.map_err(|err| WvError::Navigate(err.to_string()))
}

fn install_app_navigation_handler(
    webview: &ICoreWebView2,
    events: Sender<AppWindowEvent>,
    ready: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
) -> Result<(), WvError> {
    let handler = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else {
            failed.store(true, Ordering::Release);
            return Ok(());
        };
        // SAFETY: WebView2 invokes this handler on the creating STA and keeps
        // the event args alive for this call. `IsSuccess` writes no caller
        // memory and returns its BOOL by value.
        let mut succeeded = BOOL::default();
        // SAFETY: `succeeded` is a live BOOL out-parameter for this call.
        unsafe { args.IsSuccess(&raw mut succeeded)? };
        if succeeded.as_bool() {
            if !ready.swap(true, Ordering::AcqRel) {
                let _ = events.send(AppWindowEvent::NavigationReady);
            }
        } else if initial_navigation_failure_is_fatal(ready.load(Ordering::Acquire)) {
            failed.store(true, Ordering::Release);
        }
        Ok(())
    }));
    let mut token = 0_i64;
    // SAFETY: `webview` and the callback are created and registered on the
    // engine STA. WebView2 retains the handler for the webview lifetime.
    unsafe { webview.add_NavigationCompleted(&handler, &raw mut token) }
        .map_err(|error| WvError::Webview(format!("navigation handler: {error}")))
}

/// One live webview and the host window it fills (v0: one per window).
struct View {
    window: Window,
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
}

impl View {
    /// Sizes the controller to fill the window's client area.
    ///
    /// Best-effort by design: a transient failure during a live resize must
    /// not tear down the event loop, and the next resize event re-applies the
    /// correct bounds anyway.
    fn fit_controller(&self, size: PhysicalSize<u32>) {
        let rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(size.width).unwrap_or(i32::MAX),
            bottom: i32::try_from(size.height).unwrap_or(i32::MAX),
        };
        // SAFETY: controller was created on this thread and `RECT` is plain
        // data passed by value.
        let _ = unsafe { self.controller.SetBounds(rect) };
    }

    fn browser_process_id(&self) -> Option<u32> {
        webview_browser_process_id(&self.webview)
    }
}

fn remember_browser_process_id(
    views: &BTreeMap<u32, View>,
    expected_browser_pid: &AtomicU32,
) -> Result<(), WvError> {
    if views.is_empty() {
        return Ok(());
    }
    let pid = views
        .values()
        .find_map(View::browser_process_id)
        .ok_or_else(|| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    expected_browser_pid.store(pid, Ordering::Release);
    Ok(())
}

fn commit_profile_stop(stop: Option<&PersistentStopTransition>) {
    if let Some(stop) = stop {
        stop.commit();
    }
}

struct ProfileReleaseWait {
    receiver: Option<Receiver<Result<(), WvError>>>,
    observed: Cell<bool>,
    failed: Cell<bool>,
    deadline: Cell<Option<Instant>>,
    timeout: Duration,
}

impl ProfileReleaseWait {
    fn new(receiver: Option<Receiver<Result<(), WvError>>>) -> Self {
        Self::with_timeout(receiver, PROFILE_RELEASE_DEADLINE)
    }

    fn with_timeout(receiver: Option<Receiver<Result<(), WvError>>>, timeout: Duration) -> Self {
        Self {
            receiver,
            observed: Cell::new(false),
            failed: Cell::new(false),
            deadline: Cell::new(None),
            timeout,
        }
    }

    fn arm(&self) {
        if self.deadline.get().is_none() {
            self.deadline.set(Some(Instant::now() + self.timeout));
        }
    }

    fn poll(&self) {
        if self.observed.get() {
            return;
        }
        if self
            .deadline
            .get()
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.failed.set(true);
            return;
        }
        let Some(receiver) = self.receiver.as_ref() else {
            self.observed.set(true);
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(())) => self.observed.set(true),
            Ok(Err(_)) | Err(TryRecvError::Disconnected) => {
                self.failed.set(true);
                self.observed.set(true);
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    fn exit_ready(&self, expected_pid: &AtomicU32) -> bool {
        expected_pid.load(Ordering::Acquire) == 0 || self.observed.get() || self.failed.get()
    }

    fn schedule(&self, control_flow: &mut ControlFlow) {
        let Some(deadline) = self.deadline.get() else {
            return;
        };
        if self.observed.get() || self.failed.get() {
            return;
        }
        if !matches!(*control_flow, ControlFlow::WaitUntil(current) if current <= deadline) {
            *control_flow = ControlFlow::WaitUntil(deadline);
        }
    }
}

fn synchronize_profile_stop(
    stop: Option<&PersistentStopTransition>,
    profile: Option<&mut SelectedWindowsProfile>,
) -> Result<(), WvError> {
    if let Some(stop) = stop {
        stop.update_profile(profile)?;
    }
    Ok(())
}

fn ephemeral_scavenge_root(profile: Option<&SelectedWindowsProfile>) -> Option<PathBuf> {
    let profile = profile?;
    profile
        .plan
        .cleanup_on_release
        .then(|| profile.plan.local_app_data.clone())
}

fn scavenge_after_release(root: Option<PathBuf>) -> Result<(), WvError> {
    let Some(root) = root else {
        return Ok(());
    };
    thread::Builder::new()
        .name("keld-wv-profile-scavenge".to_owned())
        .spawn(move || {
            initialize_com_sta()?;
            let result = scavenge_ephemeral_profiles(&root).map(|_| ());
            // SAFETY: balances this thread's successful CoInitializeEx after
            // all thread-affine cleanup objects were dropped. Contract:
            // https://learn.microsoft.com/windows/win32/api/combaseapi/nf-combaseapi-couninitialize
            unsafe { CoUninitialize() };
            result
        })
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
        .join()
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
}

fn close_requested_view(
    views: &mut BTreeMap<u32, View>,
    window_id: tao::window::WindowId,
    expected_browser_pid: &AtomicU32,
    stop: Option<&PersistentStopTransition>,
) -> bool {
    let closes_last = views.len() == 1 && views.values().any(|view| view.window.id() == window_id);
    if closes_last {
        commit_profile_stop(stop);
    }
    views.retain(|_, view| {
        let keep = view.window.id() != window_id;
        if !keep && let Some(pid) = view.browser_process_id() {
            expected_browser_pid.store(pid, Ordering::Release);
        }
        keep
    });
    views.is_empty()
}

impl Drop for View {
    fn drop(&mut self) {
        // SAFETY: dropped on the engine thread (module note). `Close` releases
        // the browser-side resources deterministically instead of waiting for
        // COM refcounts.
        let _ = unsafe { self.controller.Close() };
    }
}

/// The Windows [`WebEngine`] backend.
///
/// Owns the tao event loop until [`WebView2Engine::run_until_closed`] consumes
/// it. Uses tao `run_return` so the host can reap supervised children after the last
/// window closes (KEL-30 concurrent hello app-link).
pub struct WebView2Engine {
    /// Present until the run loop starts; consumed by `run_until_closed`.
    event_loop: Option<EventLoop<WindowsLoopEvent>>,
    /// One environment per engine: every webview shares its profile directory
    /// and browser process (`learn.microsoft.com`, `WebView2` process model).
    environment: ICoreWebView2Environment,
    /// Retained validated directory handles and native exclusive lease.
    profile: Option<SelectedWindowsProfile>,
    browser_exit: Option<Receiver<Result<(), WvError>>>,
    browser_exit_environment: Option<ICoreWebView2Environment5>,
    browser_exit_token: Option<i64>,
    expected_browser_pid: Arc<AtomicU32>,
    views: BTreeMap<u32, View>,
    next_id: u32,
    pending_app_events: Option<Sender<AppWindowEvent>>,
    navigation_ready: Arc<AtomicBool>,
    navigation_failed: Arc<AtomicBool>,
    app_window_created: bool,
    saved_permissions_reconciled: bool,
}

impl fmt::Debug for WebView2Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebView2Engine")
            .field("views", &self.views.len())
            .field("running", &self.event_loop.is_none())
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

impl WebView2Engine {
    /// Creates the engine, its event loop, and the shared environment.
    ///
    /// Must be called on the process main thread: tao pumps the Win32 message
    /// loop, and `WebView2` objects are affine to the thread that created
    /// them.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::WebView2RuntimeMissing`] when the Evergreen runtime
    /// is not installed, so callers fail with install guidance before a window
    /// ever appears.
    pub fn new(selection: WebProfileSelection) -> Result<Self, WvError> {
        runtime_version()?;
        let profile = prepare_profile_before_event_loop(&known_local_app_data()?, selection)?;
        let mut builder = EventLoopBuilder::<WindowsLoopEvent>::with_user_event();
        builder.with_dpi_aware(false);
        let event_loop = builder.build();
        let environment = create_environment_for_profile(&profile)?;
        Self::from_selected_environment(event_loop, environment, profile)
    }

    /// Creates the explicit owner-private profile used by unsigned dev hosts.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::ProfileSelection`] when OS randomness or the
    /// owner-private Windows namespace cannot be established.
    pub fn new_dev_ephemeral() -> Result<Self, WvError> {
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce)
            .map_err(|_| profile_failure(ProfileErrorKind::InvalidEphemeralNonce))?;
        let profile = crate::profile::EphemeralProfile::from_host_random(nonce)?;
        Self::new(WebProfileSelection::ephemeral_dev(profile))
    }

    /// Purges only the validated persistent identity's `WebView2` data while
    /// retaining its control marker, lease namespace, and lifecycle record.
    ///
    /// This is a packaging-owned primitive and must be called on the process
    /// main thread while the outer package lifecycle lock is held.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for an in-use, mismatched, redirected, non-idle,
    /// or incompletely recovered profile.
    pub fn purge_persistent_profile(identity: ProfileIdentity) -> Result<(), WvError> {
        // SAFETY: purge runs on its packaging process main thread and confines
        // any recovery probe COM objects to this STA.
        initialize_process_dpi_awareness()?;
        initialize_com_sta()?;
        purge_persistent_profile_at(&known_local_app_data()?, identity, true)
    }

    fn from_environment(
        event_loop: EventLoop<WindowsLoopEvent>,
        environment: ICoreWebView2Environment,
    ) -> Self {
        Self {
            event_loop: Some(event_loop),
            environment,
            profile: None,
            browser_exit: None,
            browser_exit_environment: None,
            browser_exit_token: None,
            expected_browser_pid: Arc::new(AtomicU32::new(0)),
            views: BTreeMap::new(),
            next_id: 1,
            pending_app_events: None,
            navigation_ready: Arc::new(AtomicBool::new(false)),
            navigation_failed: Arc::new(AtomicBool::new(false)),
            app_window_created: false,
            saved_permissions_reconciled: false,
        }
    }

    fn from_selected_environment(
        event_loop: EventLoop<WindowsLoopEvent>,
        environment: ICoreWebView2Environment,
        profile: SelectedWindowsProfile,
    ) -> Result<Self, WvError> {
        let expected_browser_pid = Arc::new(AtomicU32::new(0));
        let browser_exit = observe_profile_browser_exit(
            &environment,
            Arc::clone(&expected_browser_pid),
            Some(event_loop.create_proxy()),
        )?;
        let mut engine = Self::from_environment(event_loop, environment);
        engine.profile = Some(profile);
        engine.browser_exit = Some(browser_exit.receiver);
        engine.browser_exit_environment = Some(browser_exit.environment);
        engine.browser_exit_token = Some(browser_exit.token);
        engine.expected_browser_pid = expected_browser_pid;
        Ok(engine)
    }

    /// Runs the event loop until the user closes the last window, then
    /// returns so the caller can tear down host-owned app-link state.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::EventLoop`] if the run loop was already started, or if
    /// tao reports a non-zero `run_return` status.
    pub fn run_until_closed(mut self) -> Result<(), WvError> {
        let Some(mut event_loop) = self.event_loop.take() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; call run_until_closed once",
            )));
        };
        let scavenge_root = ephemeral_scavenge_root(self.profile.as_ref());
        let mut views = std::mem::take(&mut self.views);
        let expected_browser_pid = Arc::clone(&self.expected_browser_pid);
        remember_browser_process_id(&views, &expected_browser_pid)?;
        let stop_transition = PersistentStopTransition::from_profile(self.profile.as_ref());
        let stop_in_loop = stop_transition.clone();
        let release = Rc::new(ProfileReleaseWait::new(self.browser_exit.take()));
        let release_in_loop = Rc::clone(&release);
        let mut exit_requested = false;
        let code = event_loop.run_return(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            release_in_loop.poll();
            if exit_requested && release_in_loop.exit_ready(&expected_browser_pid) {
                *control_flow = ControlFlow::Exit;
                return;
            }
            match event {
                // Keld drives the controller size itself — there is no wry
                // WM_SIZE subclass anymore, and tao already delivers resizes
                // (DPI changes arrive as a scale-factor event followed by this
                // same `Resized`).
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    for view in views.values() {
                        if view.window.id() == window_id {
                            view.fit_controller(size);
                        }
                    }
                }
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    // v0 hello is one window. Drop by id so a second window
                    // would not tear down every view; exit when the map is
                    // empty. On Windows, closing the last window quits — the
                    // platform convention, and what KEL-57 V1-10 will assert.
                    exit_requested = close_requested_view(
                        &mut views,
                        window_id,
                        &expected_browser_pid,
                        stop_in_loop.as_ref(),
                    );
                    if exit_requested {
                        release_in_loop.arm();
                    }
                }
                _ => {}
            }
            release_in_loop.poll();
            if exit_requested && release_in_loop.exit_ready(&expected_browser_pid) {
                *control_flow = ControlFlow::Exit;
            } else {
                release_in_loop.schedule(control_flow);
            }
        });
        if release.failed.get() {
            return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
        }
        synchronize_profile_stop(stop_transition.as_ref(), self.profile.as_mut())?;
        self.finish_profile_release(release.observed.get())?;
        scavenge_after_release(scavenge_root)?;
        if code == 0 {
            Ok(())
        } else {
            Err(WvError::EventLoop(format!(
                "event loop exited with status {code}"
            )))
        }
    }

    fn finish_profile_release(&mut self, release_observed: bool) -> Result<(), WvError> {
        let expected_pid = self.expected_browser_pid.load(Ordering::Acquire);
        if expected_pid != 0 && !release_observed {
            return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
        }
        if let (Some(environment), Some(token)) = (
            self.browser_exit_environment.as_ref(),
            self.browser_exit_token.take(),
        ) {
            remove_browser_exit_observer(environment, token)?;
        }
        let Some(profile) = self.profile.take() else {
            return Ok(());
        };
        if !profile.plan.cleanup_on_release {
            let record = profile
                .lifecycle
                .ok_or_else(|| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
            let idle = record.advance(ProfileLifecyclePhase::Idle)?;
            replace_lifecycle(&profile.plan, idle)?;
            return Ok(());
        }
        delete_ephemeral_profile(profile)
    }

    fn mark_profile_running(&mut self) -> Result<(), WvError> {
        let Some(profile) = self.profile.as_mut() else {
            return Ok(());
        };
        let Some(record) = profile.lifecycle else {
            return Ok(());
        };
        if record.phase() == ProfileLifecyclePhase::Running {
            return Ok(());
        }
        let running = record.advance(ProfileLifecyclePhase::Running)?;
        replace_lifecycle(&profile.plan, running)?;
        profile.lifecycle = Some(running);
        Ok(())
    }

    /// Creates the initial app window and emits live navigation readiness.
    ///
    /// # Errors
    ///
    /// Returns [`WvError`] when the window, `WebView2` controller, guarded
    /// permission hook, navigation callback, or initial navigation fails.
    pub fn create_app(
        &mut self,
        spec: &WebviewSpec,
        events: Sender<AppWindowEvent>,
    ) -> Result<WebviewId, WvError> {
        if !app_window_slot_available(
            self.app_window_created,
            self.pending_app_events.is_some(),
            !self.views.is_empty(),
        ) {
            return Err(WvError::EventLoop(String::from(
                "the v0 app window was already created or is pending",
            )));
        }
        self.navigation_ready.store(false, Ordering::Release);
        self.navigation_failed.store(false, Ordering::Release);
        self.pending_app_events = Some(events);
        let result = self.create(spec);
        self.pending_app_events = None;
        if result.is_ok() {
            self.app_window_created = true;
        }
        result
    }

    /// Runs the Windows UI loop until Quit or a fatal app-session command.
    ///
    /// Commands cross tao's [`EventLoopProxy`], so I/O threads never mutate
    /// the HWND or `WebView2` controller directly. Closing the last window emits
    /// [`AppWindowEvent::LastWindowClosed`] and waits for the app's Quit.
    ///
    /// # Errors
    ///
    /// Returns [`WvError::Navigate`] when initial navigation fails or exceeds
    /// its deadline, and [`WvError::EventLoop`] for duplicate run, fatal
    /// session command, or a non-zero tao exit.
    #[allow(clippy::too_many_lines)] // one UI loop keeps terminal intent, controller drop, and profile-release ordering contiguous
    pub fn run_app_until_quit(
        mut self,
        commands: Receiver<AppWindowCommand>,
        events: Sender<AppWindowEvent>,
    ) -> Result<(), WvError> {
        let Some(mut event_loop) = self.event_loop.take() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; call run_app_until_quit once",
            )));
        };
        let scavenge_root = ephemeral_scavenge_root(self.profile.as_ref());
        let proxy = event_loop.create_proxy();
        let stop_bridge = Arc::new(AtomicBool::new(false));
        let terminal_intent = Arc::new(AtomicBool::new(false));
        let bridge = spawn_app_wake_bridge(
            commands,
            proxy,
            Arc::clone(&stop_bridge),
            Arc::clone(&terminal_intent),
        )?;
        let fatal = Arc::new(AtomicBool::new(false));
        let fatal_in_loop = Arc::clone(&fatal);
        let navigation_timed_out = Arc::new(AtomicBool::new(false));
        let navigation_timed_out_in_loop = Arc::clone(&navigation_timed_out);
        let navigation_ready = Arc::clone(&self.navigation_ready);
        let navigation_failed = Arc::clone(&self.navigation_failed);
        let navigation_deadline = Instant::now() + INITIAL_NAVIGATION_DEADLINE;
        let terminal_intent_in_loop = Arc::clone(&terminal_intent);
        let mut views = std::mem::take(&mut self.views);
        let expected_browser_pid = Arc::clone(&self.expected_browser_pid);
        remember_browser_process_id(&views, &expected_browser_pid)?;
        let stop_transition = PersistentStopTransition::from_profile(self.profile.as_ref());
        let stop_in_loop = stop_transition.clone();
        let release = Rc::new(ProfileReleaseWait::new(self.browser_exit.take()));
        let release_in_loop = Rc::clone(&release);
        let mut exit_requested = false;
        let code = event_loop.run_return(move |event, _, control_flow| {
            release_in_loop.poll();
            if release_in_loop.failed.get() {
                exit_requested = true;
            }
            if exit_requested && release_in_loop.exit_ready(&expected_browser_pid) {
                *control_flow = ControlFlow::Exit;
                return;
            }
            let terminal = matches!(
                event,
                Event::UserEvent(WindowsLoopEvent::App(
                    AppWindowCommand::Quit | AppWindowCommand::Fatal,
                ))
            ) || terminal_intent_in_loop.load(Ordering::Acquire);
            if navigation_failed.load(Ordering::Acquire) {
                commit_profile_stop(stop_in_loop.as_ref());
                views.clear();
                exit_requested = true;
                release_in_loop.arm();
                release_in_loop.schedule(control_flow);
                return;
            }
            if navigation_ready.load(Ordering::Acquire) {
                *control_flow = ControlFlow::Wait;
            } else if !terminal && Instant::now() >= navigation_deadline {
                navigation_timed_out_in_loop.store(true, Ordering::Release);
                commit_profile_stop(stop_in_loop.as_ref());
                views.clear();
                exit_requested = true;
                release_in_loop.arm();
                release_in_loop.schedule(control_flow);
                return;
            } else {
                *control_flow = ControlFlow::WaitUntil(navigation_deadline);
            }
            match event {
                Event::UserEvent(WindowsLoopEvent::App(AppWindowCommand::Quit)) => {
                    commit_profile_stop(stop_in_loop.as_ref());
                    views.clear();
                    exit_requested = true;
                    release_in_loop.arm();
                }
                Event::UserEvent(WindowsLoopEvent::App(AppWindowCommand::Fatal)) => {
                    fatal_in_loop.store(true, Ordering::Release);
                    commit_profile_stop(stop_in_loop.as_ref());
                    views.clear();
                    exit_requested = true;
                    release_in_loop.arm();
                }
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    for view in views.values() {
                        if view.window.id() == window_id {
                            view.fit_controller(size);
                        }
                    }
                }
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    let last_window_closed = close_requested_view(
                        &mut views,
                        window_id,
                        &expected_browser_pid,
                        stop_in_loop.as_ref(),
                    );
                    if last_window_closed {
                        release_in_loop.arm();
                        let _ = events.send(AppWindowEvent::LastWindowClosed);
                    }
                }
                _ => {}
            }
            release_in_loop.poll();
            if exit_requested && release_in_loop.exit_ready(&expected_browser_pid) {
                *control_flow = ControlFlow::Exit;
            } else {
                release_in_loop.schedule(control_flow);
            }
        });
        stop_app_wake_bridge(&stop_bridge, bridge);
        if release.failed.get() {
            return Err(profile_failure(ProfileErrorKind::LifecycleUnproven));
        }
        synchronize_profile_stop(stop_transition.as_ref(), self.profile.as_mut())?;
        let app_result = finish_app_run_flags(&self, &navigation_timed_out, &fatal, code);
        self.finish_profile_release(release.observed.get())?;
        scavenge_after_release(scavenge_root)?;
        app_result
    }

    fn view(&self, id: WebviewId) -> Result<&View, WvError> {
        self.views
            .get(&id.0)
            .ok_or(WvError::UnknownWebview { id: id.0 })
    }
}

const fn app_window_slot_available(created: bool, pending: bool, has_views: bool) -> bool {
    !created && !pending && !has_views
}

const fn initial_navigation_failure_is_fatal(initial_ready: bool) -> bool {
    !initial_ready
}

fn finish_app_run_flags(
    engine: &WebView2Engine,
    navigation_timed_out: &AtomicBool,
    fatal: &AtomicBool,
    code: i32,
) -> Result<(), WvError> {
    finish_app_run(
        engine.navigation_failed.load(Ordering::Acquire),
        navigation_timed_out.load(Ordering::Acquire),
        fatal.load(Ordering::Acquire),
        code,
    )
}

fn finish_app_run(
    navigation_failed: bool,
    navigation_timed_out: bool,
    fatal: bool,
    code: i32,
) -> Result<(), WvError> {
    if navigation_failed {
        return Err(WvError::Navigate(String::from(
            "initial renderer navigation failed",
        )));
    }
    if navigation_timed_out {
        return Err(WvError::Navigate(String::from(
            "initial renderer navigation did not finish before the startup deadline",
        )));
    }
    if fatal {
        return Err(WvError::EventLoop(String::from(
            "primary app session failed while the Windows event loop was live",
        )));
    }
    if code == 0 {
        Ok(())
    } else {
        Err(WvError::EventLoop(format!(
            "event loop exited with status {code}"
        )))
    }
}

fn spawn_app_wake_bridge(
    commands: Receiver<AppWindowCommand>,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    stop: Arc<AtomicBool>,
    terminal_intent: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, WvError> {
    thread::Builder::new()
        .name("keld-wv-windows-app-wake".to_owned())
        .spawn(move || {
            loop {
                match commands.recv_timeout(Duration::from_millis(100)) {
                    Ok(command) => {
                        let terminal =
                            matches!(command, AppWindowCommand::Quit | AppWindowCommand::Fatal);
                        if terminal {
                            terminal_intent.store(true, Ordering::Release);
                        }
                        let _ = proxy.send_event(WindowsLoopEvent::App(command));
                        if terminal {
                            return;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) if stop.load(Ordering::Acquire) => return,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        terminal_intent.store(true, Ordering::Release);
                        let _ = proxy.send_event(WindowsLoopEvent::App(AppWindowCommand::Fatal));
                        return;
                    }
                }
            }
        })
        .map_err(|error| WvError::EventLoop(format!("failed to start app wake bridge: {error}")))
}

fn stop_app_wake_bridge(stop: &AtomicBool, bridge: thread::JoinHandle<()>) {
    stop.store(true, Ordering::Release);
    let _ = bridge.join();
}

impl WebEngine for WebView2Engine {
    fn create(&mut self, spec: &WebviewSpec) -> Result<WebviewId, WvError> {
        let Some(event_loop) = self.event_loop.as_ref() else {
            return Err(WvError::EventLoop(String::from(
                "run loop already started; create webviews before run_until_closed",
            )));
        };
        let window = WindowBuilder::new()
            .with_title(&spec.title)
            // Logical (DPI-independent) points. tao declares per-monitor-v2 DPI
            // awareness for the process, so this is scaled by the monitor's
            // factor rather than bitmap-stretched by the OS.
            .with_inner_size(tao::dpi::LogicalSize::new(
                spec.size.width,
                spec.size.height,
            ))
            // KEL-25/KEL-27: standard Windows caption — minimize, maximize, close.
            .with_resizable(true)
            .with_minimizable(true)
            .with_closable(true)
            .build(event_loop)
            .map_err(|e| WvError::Window(e.to_string()))?;

        let hwnd = HWND(window.hwnd() as _);
        let controller = create_controller(&self.environment, hwnd)?;
        // SAFETY: controller was created on this thread a moment ago.
        let webview = unsafe { controller.CoreWebView2() }
            .map_err(|err| WvError::Webview(format!("controller returned no webview: {err}")))?;
        // Into a `View` before any further fallible step: its `Drop` closes the
        // controller, so an early `?` return below cannot leak the browser-side
        // resources behind a half-built webview.
        let view = View {
            window,
            controller,
            webview,
        };
        if !self.saved_permissions_reconciled && saved_permission_reconciliation_enabled() {
            reconcile_saved_media_permissions(&view.webview)?;
            self.saved_permissions_reconciled = true;
        }
        self.mark_profile_running()?;

        // Guard before content: nothing may load until default-deny is wired.
        // Empty manifest → deny everything (KEL-59). KEL-73: mint the webview
        // id first so capture cannot inherit AppProcess grants.
        let id = self.next_id;
        #[cfg(not(all(feature = "media-acceptance", test)))]
        let manifest = PermissionsManifest::default();
        #[cfg(all(feature = "media-acceptance", test))]
        let manifest = media_acceptance::fixture_manifest();
        let guard = install_guarded_media_permissions(
            &view.webview,
            manifest,
            webview_media_principal(WebviewId(id)),
        )?;

        // Devtools follow the build profile, like the macOS backend: wired in
        // debug, off in release until keld-guard owns `web.devtools`. WebView2
        // defaults them to on, so release must opt out explicitly.
        // SAFETY: settings object is used and dropped on this thread.
        let devtools = unsafe {
            view.webview
                .Settings()
                .and_then(|settings| settings.SetAreDevToolsEnabled(cfg!(debug_assertions)))
        };
        devtools.map_err(|err| WvError::Webview(format!("devtools setting: {err}")))?;

        // KEL-62: bounds before navigation. WebView2 starts with a zero-sized
        // controller, and Chromium does not composite a zero-sized surface —
        // measured as ~640 ms of dead time when the size arrived only from a
        // later window event. This is create-time wiring, so a failure here is
        // a real error, unlike the best-effort live resizes.
        let size = view.window.inner_size();
        let rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(size.width).unwrap_or(i32::MAX),
            bottom: i32::try_from(size.height).unwrap_or(i32::MAX),
        };
        // SAFETY: controller lives on this thread; `RECT` is plain data, and
        // `SetIsVisible` takes a BOOL by value.
        let shown = unsafe {
            view.controller
                .SetBounds(rect)
                .and_then(|()| view.controller.SetIsVisible(true))
        };
        shown.map_err(|err| WvError::Webview(format!("initial bounds: {err}")))?;

        if let Some(events) = self.pending_app_events.as_ref() {
            install_app_navigation_handler(
                &view.webview,
                events.clone(),
                Arc::clone(&self.navigation_ready),
                Arc::clone(&self.navigation_failed),
            )?;
        }

        navigate_initial(&guard, &spec.initial)?;

        // Keyboard focus lands in the page, matching what wry's build did and
        // what a single-webview window should do.
        // SAFETY: controller lives on this thread; best-effort like a resize.
        let _ = unsafe {
            view.controller
                .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC)
        };

        self.next_id += 1;
        self.views.insert(id, view);
        Ok(WebviewId(id))
    }

    fn navigate(&mut self, id: WebviewId, target: NavTarget) -> Result<(), WvError> {
        let view = self.view(id)?;
        // The guard handler registered at create time lives for the webview's
        // whole life, so later navigations need no new proof.
        // SAFETY: webview lives on this thread; HSTRINGs outlive the calls.
        let loaded = match target {
            NavTarget::Html(html) => unsafe {
                view.webview.NavigateToString(&HSTRING::from(html.as_str()))
            },
            NavTarget::Url(url) => unsafe { view.webview.Navigate(&HSTRING::from(url.as_str())) },
        };
        loaded.map_err(|err| WvError::Navigate(err.to_string()))
    }

    fn eval(&mut self, id: WebviewId, script: &str) -> Result<(), WvError> {
        let view = self.view(id)?;
        // Fire-and-forget per the trait contract: the completion handler only
        // satisfies the COM signature. Result plumbing belongs to the
        // command-queue design (engine.rs module docs).
        // SAFETY: webview lives on this thread; the handler is dropped after
        // the script completes on this same thread.
        let queued = unsafe {
            view.webview.ExecuteScript(
                &HSTRING::from(script),
                &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(()))),
            )
        };
        queued.map_err(|err| WvError::Script(err.to_string()))
    }

    fn set_bounds(&mut self, id: WebviewId, rect: Rect) -> Result<(), WvError> {
        let view = self.view(id)?;
        // v0 webviews fill their host window, so bounds apply to the window…
        view.window
            .set_outer_position(tao::dpi::LogicalPosition::new(rect.x, rect.y));
        view.window
            .set_inner_size(tao::dpi::LogicalSize::new(rect.width, rect.height));
        // …and the controller follows immediately. Before the run loop starts
        // no resize event fires, so waiting for one would leave a stale
        // controller size; `inner_size` reads back what the move actually
        // produced (the OS may clamp).
        view.fit_controller(view.window.inner_size());
        Ok(())
    }

    fn devtools(&mut self, id: WebviewId, action: DevtoolsAction) -> Result<(), WvError> {
        let view = self.view(id)?;
        match action {
            // SAFETY: webview lives on this thread.
            DevtoolsAction::Open => unsafe { view.webview.OpenDevToolsWindow() }
                .map_err(|err| WvError::Webview(format!("devtools open: {err}"))),
            // WebView2 has no close-devtools API (wry no-ops here too); the
            // window is the user's to close. Not an error: the contract is
            // "the inspector is not open afterwards because of us".
            DevtoolsAction::Close => Ok(()),
        }
    }

    fn destroy(&mut self, id: WebviewId) -> Result<(), WvError> {
        // Dropping the `View` closes the controller, then the window.
        self.views
            .remove(&id.0)
            .map(|_| ())
            .ok_or(WvError::UnknownWebview { id: id.0 })
    }
}

impl WebView2EngineExt for WebView2Engine {}

/// Opens a window from `spec` and runs until the user closes it.
///
/// Thin wrapper for the Phase 1 hello slice: probes the runtime, builds a
/// [`WebView2Engine`], creates one webview, and hands the thread to the run
/// loop.
///
/// # Errors
///
/// Returns [`WvError::WebView2RuntimeMissing`] if the Evergreen runtime is
/// absent, or another [`WvError`] if window or webview creation fails.
pub fn run_hello(spec: &WebviewSpec) -> Result<(), WvError> {
    let mut engine = WebView2Engine::new_dev_ephemeral()?;
    engine.create(spec)?;
    engine.run_until_closed()
}

#[cfg(test)]
mod tests {
    use super::{
        COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY, PROFILE_LEASE,
        PROFILE_LIFECYCLE, PROFILE_MARKER, ProfileReleaseWait, SavedPermission, WebView2Engine,
        app_window_slot_available, dacl_has_untrusted_write, initial_navigation_failure_is_fatal,
        initialize_com_sta, initialize_process_dpi_awareness, prepare_windows_profile_at,
        purge_persistent_profile_at, runtime_version, saved_media_permission_needs_deny,
        try_scavenge_ephemeral_profile, wait_with_message_pump_until, webview2_permission_state,
        windows_profile_plan,
    };
    use crate::error::WvError;
    use crate::profile::{
        EphemeralProfile, ProfileIdentity, ProfileLifecyclePhase, ProfileLifecycleRecord,
        ProfileProcessIdentity, ProfilePurgePhase, ProfilePurgeRecord, WebProfileSelection,
    };

    /// The CI runners and this developer machine both ship the Evergreen
    /// runtime, so the probe must succeed and return a dotted version. If it
    /// ever fails the message has to name the code and the fix, because that is
    /// the only thing a user sees when their runtime is missing.
    #[test]
    fn runtime_probe_reports_version_or_actionable_error() {
        match runtime_version() {
            Ok(version) => {
                assert!(
                    version.split('.').count() >= 2,
                    "expected a dotted WebView2 version, got: {version}"
                );
            }
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    matches!(err, WvError::WebView2RuntimeMissing { .. }),
                    "probe must fail as WebView2RuntimeMissing, got: {msg}"
                );
                assert!(msg.contains("KELD-WV-008"), "missing code in: {msg}");
                assert!(msg.contains("Evergreen Runtime"), "missing fix in: {msg}");
            }
        }
    }

    #[test]
    fn changed_com_apartment_fails_before_profile_work() {
        std::thread::spawn(|| {
            // SAFETY: this fresh test thread initializes MTA once and balances
            // it below. The production STA request must reject the mismatch.
            // Contract: https://learn.microsoft.com/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex
            unsafe {
                super::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED)
            }
            .ok()
            .expect("initialize test MTA");
            let error = initialize_com_sta().expect_err("changed COM mode must fail");
            assert!(error.to_string().contains("lifecycle state"));
            // SAFETY: balances the successful test-thread MTA initialization.
            // Contract: https://learn.microsoft.com/windows/win32/api/combaseapi/nf-combaseapi-couninitialize
            unsafe { super::CoUninitialize() };
        })
        .join()
        .expect("COM apartment test thread");
    }

    #[test]
    fn existing_per_monitor_v2_dpi_policy_is_accepted() {
        initialize_process_dpi_awareness().expect("initial DPI policy");
        initialize_process_dpi_awareness().expect("existing identical DPI policy");
    }

    #[test]
    fn absent_browser_exit_has_one_nonrenewable_deadline() {
        let (_sender, receiver) = std::sync::mpsc::channel::<Result<(), WvError>>();
        let release = ProfileReleaseWait::with_timeout(Some(receiver), std::time::Duration::ZERO);
        release.arm();
        let first = release.deadline.get().expect("armed deadline");
        release.arm();
        assert_eq!(
            release.deadline.get(),
            Some(first),
            "events cannot renew it"
        );
        let mut control_flow = super::ControlFlow::Wait;
        release.schedule(&mut control_flow);
        assert_eq!(control_flow, super::ControlFlow::WaitUntil(first));
        release.poll();
        assert!(release.failed.get());
        assert!(!release.observed.get());

        let (sender, receiver) = std::sync::mpsc::channel::<Result<(), WvError>>();
        let ready = ProfileReleaseWait::with_timeout(Some(receiver), std::time::Duration::ZERO);
        ready.arm();
        sender.send(Ok(())).expect("queue late success");
        ready.poll();
        assert!(ready.failed.get());
        assert!(!ready.observed.get());
        assert!(
            ready
                .receiver
                .as_ref()
                .expect("receiver")
                .try_recv()
                .is_ok(),
            "expired result must remain unconsumed"
        );
    }

    #[test]
    fn expired_pump_rejects_ready_result_before_queued_messages() {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::WindowsAndMessaging::{
            MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, WM_NULL,
        };

        let mut message = MSG::default();
        // SAFETY: the no-remove query creates this test thread's message queue;
        // the output is live. Contract:
        // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-peekmessagew
        let _ = unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE) };
        // SAFETY: posts inert WM_NULL to the current test thread's live queue.
        // Contract: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-postthreadmessagew
        unsafe { PostThreadMessageW(GetCurrentThreadId(), WM_NULL, WPARAM(0), LPARAM(0)) }
            .expect("queue inert message");
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(7_u8).expect("queue ready result");
        wait_with_message_pump_until(&receiver, std::time::Instant::now())
            .expect_err("expired work must lose to the absolute deadline");
        // SAFETY: removes only this test's inert WM_NULL records without
        // dispatch. Contract: PeekMessageW above.
        while unsafe { PeekMessageW(&raw mut message, None, WM_NULL, WM_NULL, PM_REMOVE) }.as_bool()
        {
        }
    }

    /// Keeps the engine type named from the test module so a rename fails the
    /// build here rather than silently orphaning these tests. Constructing one
    /// needs a tao `EventLoop`, which must be built on the process main thread —
    /// the harness gives each test its own thread, so the engine itself is
    /// exercised by the GUI pass in KEL-27, not here.
    fn _assert_engine_type(_: Option<&WebView2Engine>) {}

    /// A stale id must stay a typed error on this backend too (no panics in
    /// libs, per root `AGENTS.md`).
    #[test]
    fn unknown_webview_id_is_typed_not_panic() {
        let err = WvError::UnknownWebview { id: 3 };
        assert!(err.to_string().contains("KELD-WV-007"));
    }

    #[test]
    fn app_window_slot_is_single_use_and_retryable_only_after_failed_setup() {
        assert!(app_window_slot_available(false, false, false));
        assert!(!app_window_slot_available(true, false, false));
        assert!(!app_window_slot_available(false, true, false));
        assert!(!app_window_slot_available(false, false, true));
        assert!(app_window_slot_available(false, false, false));
    }

    #[test]
    fn only_initial_navigation_failure_is_startup_fatal() {
        assert!(initial_navigation_failure_is_fatal(false));
        assert!(!initial_navigation_failure_is_fatal(true));
    }

    #[test]
    fn permission_mapper_preserves_allow_and_deny() {
        assert_eq!(
            webview2_permission_state(true),
            COREWEBVIEW2_PERMISSION_STATE_ALLOW
        );
        assert_eq!(
            webview2_permission_state(false),
            COREWEBVIEW2_PERMISSION_STATE_DENY
        );
    }

    #[test]
    fn saved_media_grants_are_reconciled_without_touching_other_kinds() {
        for kind in [
            super::COREWEBVIEW2_PERMISSION_KIND_CAMERA,
            super::COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        ] {
            assert!(saved_media_permission_needs_deny(&SavedPermission {
                kind,
                state: COREWEBVIEW2_PERMISSION_STATE_ALLOW,
                origin: String::from("http://127.0.0.1:41000"),
            }));
            assert!(!saved_media_permission_needs_deny(&SavedPermission {
                kind,
                state: COREWEBVIEW2_PERMISSION_STATE_DENY,
                origin: String::from("http://127.0.0.1:41000"),
            }));
        }
        assert!(!saved_media_permission_needs_deny(&SavedPermission {
            kind: super::COREWEBVIEW2_PERMISSION_KIND(99),
            state: COREWEBVIEW2_PERMISSION_STATE_ALLOW,
            origin: String::from("http://127.0.0.1:41000"),
        }));
    }

    /// KEL-63: the profile must be per-user, not beside the executable.
    ///
    /// The loader default is `<exe>.WebView2\` next to the binary, which a
    /// standard user cannot create under `C:\Program Files\`. This asserts we
    /// are not on that default and that the directory is user-scoped.
    #[test]
    fn profile_dir_is_user_scoped_not_next_to_the_exe() {
        let root = std::path::Path::new(r"C:\Users\fixture\AppData\Local");
        let identity = ProfileIdentity::from_host_verified_parts([7; 32], "dev.keld.fixture")
            .expect("verified fixture identity");
        let plan = windows_profile_plan(root, WebProfileSelection::Persistent(identity))
            .expect("persistent profile plan");
        assert!(plan.user_data_dir.starts_with(root));
        assert_eq!(
            plan.user_data_dir,
            root.join("Keld")
                .join("profiles")
                .join("v1")
                .join(identity.namespace_segment())
                .join("webview2")
        );
    }

    #[test]
    fn dev_profile_is_not_the_global_legacy_namespace() {
        let root = std::path::Path::new(r"C:\Users\fixture\AppData\Local");
        let first = EphemeralProfile::from_host_random([1; 32]).expect("first dev launch");
        let second = EphemeralProfile::from_host_random([2; 32]).expect("second dev launch");
        let first = windows_profile_plan(root, WebProfileSelection::ephemeral_dev(first))
            .expect("first plan");
        let second = windows_profile_plan(root, WebProfileSelection::ephemeral_dev(second))
            .expect("second plan");
        assert_ne!(first.control_dir, second.control_dir);
        assert!(
            first
                .control_dir
                .starts_with(root.join("Keld").join("ephemeral").join("v1"))
        );
        assert_eq!(first.user_data_dir, first.control_dir.join("webview2"));
        assert!(!first.control_dir.ends_with("dev.keld"));
    }

    #[test]
    fn native_profile_lease_and_handles_prevent_alias_and_reuse() {
        let root = create_private_test_root("lease");
        let identity = ProfileIdentity::from_host_verified_parts([9; 32], "dev.keld.native-lease")
            .expect("identity");
        let selection = WebProfileSelection::Persistent(identity);
        let first = prepare_windows_profile_at(&root, selection).expect("first owner");
        let second = prepare_windows_profile_at(&root, selection)
            .expect_err("same profile must reject a second native lease");
        assert!(second.to_string().contains("already in use"));
        assert!(
            std::fs::rename(
                &first.plan.control_dir,
                first.plan.control_dir.with_extension("moved")
            )
            .is_err(),
            "retained handles must deny rename/delete substitution"
        );
        drop(first);
        prepare_windows_profile_at(&root, selection)
            .expect_err("dropping a host lease without durable idle must not admit reuse");
        std::fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn aliased_control_file_is_rejected_before_lock_or_record_use() {
        let root = create_private_test_root("control-alias");
        let identity = ProfileIdentity::from_host_verified_parts([11; 32], "dev.keld.alias")
            .expect("identity");
        let plan =
            windows_profile_plan(&root, WebProfileSelection::Persistent(identity)).expect("plan");
        std::fs::create_dir_all(&plan.control_dir).expect("create control tree");
        std::fs::write(plan.control_dir.join(PROFILE_MARKER), &plan.marker)
            .expect("write marker fixture");
        let outside = root.join("outside.lock");
        std::fs::write(&outside, []).expect("write outside lease target");
        std::fs::hard_link(&outside, plan.control_dir.join(PROFILE_LEASE))
            .expect("create native hard-link alias");

        let error = prepare_windows_profile_at(&root, WebProfileSelection::Persistent(identity))
            .expect_err("aliased lease file must fail");
        assert!(error.to_string().contains("ownership marker"));
        std::fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn dead_persistent_owner_is_durably_quarantined_before_recovery() {
        let root = create_private_test_root("lifecycle");
        let identity = ProfileIdentity::from_host_verified_parts([12; 32], "dev.keld.lifecycle")
            .expect("identity");
        let selection = WebProfileSelection::Persistent(identity);
        let initial = prepare_windows_profile_at(&root, selection).expect("initial owner");
        let lifecycle_path = initial.plan.control_dir.join(PROFILE_LIFECYCLE);
        drop(initial);

        let prior = ProfileLifecycleRecord::windows_idle()
            .begin_startup(
                ProfileProcessIdentity::from_host_observation(u32::MAX, u64::MAX)
                    .expect("dead fixture owner"),
            )
            .and_then(|record| record.advance(ProfileLifecyclePhase::Running))
            .expect("prior running record");
        std::fs::write(
            &lifecycle_path,
            prior.to_record_bytes().expect("encode prior state"),
        )
        .expect("replace prior state fixture");
        let recovered = prepare_windows_profile_at(&root, selection)
            .expect("dead owner reaches quarantined recovery preparation");
        assert!(recovered.recovery_required);
        assert_eq!(
            recovered.lifecycle.map(|record| record.phase()),
            Some(ProfileLifecyclePhase::Quarantined)
        );
        let durable = ProfileLifecycleRecord::from_windows_record_bytes(
            &std::fs::read(lifecycle_path).expect("read durable quarantine"),
        )
        .expect("decode durable quarantine");
        assert_eq!(durable.phase(), ProfileLifecyclePhase::Quarantined);
        drop(recovered);
        std::fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn persistent_purge_is_resumable_and_preserves_control_state() {
        let root = create_private_test_root("purge");
        let identity = ProfileIdentity::from_host_verified_parts([16; 32], "dev.keld.purge")
            .expect("identity");
        let selection = WebProfileSelection::Persistent(identity);
        let initial = prepare_windows_profile_at(&root, selection).expect("initial profile");
        let plan = initial.plan.clone();
        drop(initial);
        std::fs::write(
            plan.control_dir.join(PROFILE_LIFECYCLE),
            ProfileLifecycleRecord::windows_idle()
                .to_record_bytes()
                .expect("idle bytes"),
        )
        .expect("write idle fixture");
        std::fs::write(plan.user_data_dir.join("state.bin"), b"owned state")
            .expect("write data fixture");

        purge_persistent_profile_at(&root, identity, true).expect("first purge");
        assert!(!plan.user_data_dir.exists());
        assert!(plan.control_dir.exists());
        assert!(plan.control_dir.join(PROFILE_MARKER).exists());
        assert!(plan.control_dir.join(PROFILE_LIFECYCLE).exists());
        assert!(!plan.control_dir.join(super::PROFILE_PURGE).exists());

        purge_persistent_profile_at(&root, identity, true).expect("idempotent purge");
        assert!(!plan.user_data_dir.exists());
        assert!(!plan.control_dir.join(super::PROFILE_PURGE).exists());
        std::fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn purge_intent_precedes_recovery_and_blocks_normal_startup() {
        let root = create_private_test_root("purge-recovery-order");
        let identity =
            ProfileIdentity::from_host_verified_parts([20; 32], "dev.keld.purge-recovery")
                .expect("identity");
        let selection = WebProfileSelection::Persistent(identity);
        let initial = prepare_windows_profile_at(&root, selection).expect("initial profile");
        let plan = initial.plan.clone();
        drop(initial);
        let dead = ProfileLifecycleRecord::windows_idle()
            .begin_startup(
                ProfileProcessIdentity::from_host_observation(u32::MAX, u64::MAX)
                    .expect("dead owner"),
            )
            .and_then(|record| record.advance(ProfileLifecyclePhase::Running))
            .expect("dead running");
        std::fs::write(
            plan.control_dir.join(PROFILE_LIFECYCLE),
            dead.to_record_bytes().expect("dead bytes"),
        )
        .expect("write dead lifecycle");

        purge_persistent_profile_at(&root, identity, false)
            .expect_err("recovery needs a real event loop");
        let intent = ProfilePurgeRecord::from_record_bytes(
            &std::fs::read(plan.control_dir.join(super::PROFILE_PURGE))
                .expect("read committed purge intent"),
        )
        .expect("decode committed purge intent");
        assert_eq!(intent.phase(), ProfilePurgePhase::Prepared);
        let blocked = prepare_windows_profile_at(&root, selection)
            .expect_err("normal startup cannot bypass purge recovery");
        assert!(blocked.to_string().contains("durable profile intent"));
        std::fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn scavenger_deletes_only_marker_validated_inactive_leaf() {
        let root = create_private_test_root("scavenge");
        let profile = EphemeralProfile::from_host_random([17; 32]).expect("ephemeral");
        let plan =
            windows_profile_plan(&root, WebProfileSelection::ephemeral_dev(profile)).expect("plan");
        std::fs::create_dir_all(&plan.control_dir).expect("create old control");
        std::fs::write(plan.control_dir.join(PROFILE_MARKER), &plan.marker)
            .expect("write old marker");
        std::fs::write(plan.control_dir.join(PROFILE_LEASE), []).expect("write old lease");
        assert!(try_scavenge_ephemeral_profile(&root, profile).expect("scavenge old leaf"));
        assert!(!plan.control_dir.exists());

        let corrupt = EphemeralProfile::from_host_random([18; 32]).expect("ephemeral");
        let corrupt_plan = windows_profile_plan(&root, WebProfileSelection::ephemeral_dev(corrupt))
            .expect("corrupt plan");
        std::fs::create_dir_all(&corrupt_plan.control_dir).expect("create corrupt control");
        std::fs::write(corrupt_plan.control_dir.join(PROFILE_MARKER), b"foreign")
            .expect("write foreign marker");
        std::fs::write(corrupt_plan.control_dir.join(PROFILE_LEASE), [])
            .expect("write corrupt lease");
        assert!(try_scavenge_ephemeral_profile(&root, corrupt).is_err());
        assert!(corrupt_plan.control_dir.exists());
        std::fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn untrusted_write_is_rejected_without_replacing_engine_aces() {
        use windows_permissions::{LocalBox, SecurityDescriptor};

        let owner_and_engine_access = concat!(
            "D:(A;;FA;;;S-1-5-21-1-2-3-1001)",
            "(A;;FW;;;AC)(A;;FW;;;S-1-15-3-1)(A;;FA;;;SY)(A;;FA;;;BA)"
        )
        .parse::<LocalBox<SecurityDescriptor>>()
        .expect("safe descriptor");
        assert!(!dacl_has_untrusted_write(
            owner_and_engine_access.dacl().expect("safe DACL"),
            "S-1-5-21-1-2-3-1001"
        ));

        for broad_sid in [
            "WD",
            "AU",
            "BU",
            "S-1-5-21-3459511679-531530595-653566084-1006",
        ] {
            let descriptor = format!("D:(A;;FA;;;S-1-5-21-1-2-3-1001)(A;;FW;;;{broad_sid})")
                .parse::<LocalBox<SecurityDescriptor>>()
                .expect("broad-write descriptor");
            assert!(
                dacl_has_untrusted_write(
                    descriptor.dacl().expect("broad-write DACL"),
                    "S-1-5-21-1-2-3-1001"
                ),
                "{broad_sid} write access must fail"
            );
        }
    }

    #[test]
    fn reparse_ancestor_is_rejected_before_profile_creation() {
        let root = create_private_test_root("reparse");
        let outside = create_private_test_root("reparse-outside");
        let junction = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(root.join("Keld"))
            .arg(&outside)
            .output()
            .expect("run junction fixture command");
        assert!(
            junction.status.success(),
            "create directory junction: {}",
            String::from_utf8_lossy(&junction.stderr)
        );
        let identity = ProfileIdentity::from_host_verified_parts([10; 32], "dev.keld.reparse")
            .expect("identity");
        let error = prepare_windows_profile_at(&root, WebProfileSelection::Persistent(identity))
            .expect_err("reparse ancestor must fail");
        assert!(error.to_string().contains("ownership marker"));
        std::fs::remove_dir(root.join("Keld")).expect("remove reparse point");
        std::fs::remove_dir(root).expect("remove test LocalAppData");
        std::fs::remove_dir(outside).expect("remove reparse target");
    }

    #[test]
    fn dev_nonce_leaf_is_create_once_even_after_release() {
        let root = create_private_test_root("ephemeral");
        let first = EphemeralProfile::from_host_random([3; 32]).expect("first nonce");
        let selected = prepare_windows_profile_at(&root, WebProfileSelection::ephemeral_dev(first))
            .expect("first dev profile");
        drop(selected);
        prepare_windows_profile_at(&root, WebProfileSelection::ephemeral_dev(first))
            .expect_err("a later launch must not reuse an old dev leaf");
        let second = EphemeralProfile::from_host_random([4; 32]).expect("second nonce");
        prepare_windows_profile_at(&root, WebProfileSelection::ephemeral_dev(second))
            .expect("fresh nonce gets a fresh leaf");
        std::fs::remove_dir_all(&root).expect("remove test root");
    }

    fn create_private_test_root(label: &str) -> std::path::PathBuf {
        use std::fmt::Write as _;

        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).expect("test randomness");
        let mut suffix = String::with_capacity(32);
        for byte in nonce {
            write!(&mut suffix, "{byte:02x}").expect("write test suffix");
        }
        let root = super::known_local_app_data()
            .expect("resolve test LocalAppData")
            .join("Keld")
            .join("test-fixtures")
            .join(format!("keld-profile-{label}-{suffix}"));
        std::fs::create_dir_all(&root).expect("create test LocalAppData");
        let current_sid = super::current_process_sid()
            .expect("resolve test process SID")
            .to_string();
        let grant = format!("*{current_sid}:(OI)(CI)F");
        let output = std::process::Command::new("icacls.exe")
            .arg(&root)
            .args(["/inheritance:r", "/grant:r"])
            .arg(grant)
            .output()
            .expect("run owner-private DACL fixture command");
        assert!(
            output.status.success(),
            "set owner-private fixture DACL: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        root
    }

    /// KEL-66: the environment options this backend ships must not carry
    /// wry's SmartScreen-disabling browser arguments. `webview2-com` defaults
    /// to an empty argument string; if that default ever changes, or someone
    /// adds `--disable-features` here, this fails.
    #[test]
    fn environment_options_do_not_disable_smartscreen() {
        let options = webview2_com::CoreWebView2EnvironmentOptions::default();
        // SAFETY: reading the freshly constructed options object on this
        // thread; no COM aggregate exists yet.
        let args = unsafe { options.additional_browser_arguments() };
        assert!(
            args.is_empty(),
            "expected no default browser arguments, got: {args}"
        );
        // The options setter is the only way to hand the browser extra flags;
        // the module docs quote wry's old flag string, so scan for the API
        // call rather than the flag text.
        let src = include_str!("mod.rs");
        assert_eq!(
            src.matches("set_additional_browser_arguments").count(),
            1,
            "KEL-66: this backend must not pass browser arguments \
             (the one hit is this test's own scan string)"
        );
    }
}
