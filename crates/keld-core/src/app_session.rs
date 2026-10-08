//! Validated no-flag application boot and host-owned primary session (KEL-96).

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::collections::HashMap;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::ffi::OsStr;
#[cfg(windows)]
use std::ffi::OsString;
use std::fmt;
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), windows))]
use std::fs;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::fs::File;
#[cfg(any(target_os = "linux", windows))]
use std::io::Write as _;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::io::{self, Read};
#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
#[cfg(windows)]
use std::os::windows::ffi::OsStringExt as _;
#[cfg(windows)]
use std::os::windows::io::{FromRawHandle as _, OwnedHandle};
#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
use std::path::{Component, Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::ExitStatus;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::sync::{Arc, Condvar, Mutex, Weak};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::thread::{self, JoinHandle};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::time::{Duration, Instant};

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
use serde::Deserialize;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use nix::sys::stat::{SFlag, fstat};

#[cfg(target_os = "macos")]
use crate::macos_profile_identity::verified_current_process_signing_info;
#[cfg(target_os = "macos")]
use getrandom::fill as fill_os_random;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_guard::verified_manifest::VerifiedManifest;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_guard::verified_manifest::load_verified_manifest;
use keld_guard::{ManifestError, Principal};
#[cfg(windows)]
use keld_guard::{VerifiedWindowsImage, WindowsAuthenticodeError, WindowsAuthenticodeImage};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::CallError;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::codec::{decode, encode};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::frame::{CorrelationId, FrameKind};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use keld_ipc::link::read_quit_drain_frame;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::link::{
    AppLinkDeadlines, read_primary_app_frame_interruptible,
    read_primary_app_frame_interruptible_with_privileged_call, write_frame,
};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::{
    APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL, BootstrapStream, ECHO_CHANNEL, IpcError,
    LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleRequest, LifecycleResponse,
};
#[cfg(all(test, target_os = "macos"))]
use keld_native::fs::FsResponse;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_native::fs::{FS_CHANNEL, FsBroker, FsPrepareError, FsRequest};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use keld_runtime::linux_strict::LinuxStrictProfile;
#[cfg(target_os = "macos")]
use keld_runtime::macos_guardian::{GuardedPrimary, GuardedPrimaryUpdate, GuardianBootstrap};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_runtime::primary::{BoundPrimaryGeneration, PrimaryRoleEvent};
#[cfg(windows)]
use keld_runtime::primary::{PrimaryRecoveryGate, PrimaryRoleConfig, PrimaryRoleSupervisor};
#[cfg(target_os = "linux")]
use keld_runtime::primary::{PrimaryRecoveryGate, PrimaryRoleConfig, PrimaryRoleSupervisor};
#[cfg(target_os = "macos")]
use keld_wv::profile::EphemeralProfile;
#[cfg(target_os = "linux")]
use keld_wv::webkitgtk::WebKitGtkEngine;
#[cfg(windows)]
use keld_wv::webview2::WebView2Engine;
#[cfg(target_os = "macos")]
use keld_wv::wkwebview::{
    AppWindowCommand, AppWindowEvent, RendererBridgeEndpoint, RendererBridgeOutcome,
    RendererBridgeRequest, WkWebViewEngine,
};
#[cfg(any(target_os = "linux", windows))]
use keld_wv::{AppWindowCommand, AppWindowEvent};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_wv::{NavTarget, WebviewSpec, WvError};
#[cfg(any(target_os = "macos", windows))]
use keld_wv::{ProfileIdentity, WebProfileSelection};
#[cfg(target_os = "macos")]
use sha2::{Digest as _, Sha256};
#[cfg(all(test, windows))]
use windows_sys::Win32::Foundation::GetHandleInformation;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation, WAIT_OBJECT_0,
};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
};
#[cfg(windows)]
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    QueryFullProcessImageNameW, WaitForSingleObject,
};

/// Maximum accepted `keld.boot.json` size.
#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
const MAX_BOOT_BYTES: usize = 64 * 1024;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const BOOT_FILE: &str = "keld.boot.json";
#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
const PERMISSIONS_FILE: &str = "keld.permissions.jsonc";
#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
const DIGEST_PREFIX: &str = "sha256:";
#[cfg(target_os = "macos")]
const GUARDIAN_OWNER_REPLY_DEADLINE: Duration = Duration::from_secs(6);
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const DEV_LEASE_ENV: &str = "KELD_DEV_LEASE";
#[cfg(windows)]
const WINDOWS_DEV_STAGE_DELETE_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(windows)]
const WINDOWS_SHARING_VIOLATION: i32 = 32;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const DEV_LEASE_STDIN_V1: &str = "stdin-v1";
/// Fix guidance for `KELD-CORE-034`, shared by every site that refuses no-flag boot.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
const NO_FLAG_UNAVAILABLE_FIX: &str =
    "Complete and prove the named KEL-96/T4 platform slice before launching the host.";
#[cfg(any(target_os = "macos", target_os = "linux"))]
const DEV_LEASE_DRAIN_READS: usize = 64;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const SESSION_RUNNING: u8 = 0;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const SESSION_LIFECYCLE_QUIT: u8 = 1;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const SESSION_CLI_LEASE_LOST: u8 = 2;

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
static LISTENER_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
static CHILD_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
static WINDOW_ATTEMPTS: AtomicU32 = AtomicU32::new(0);

#[cfg(any(target_os = "macos", target_os = "linux"))]
struct DevHostLease {
    input: io::Stdin,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[derive(Clone)]
struct SessionShutdownState {
    cause: Arc<AtomicU8>,
    transition: Arc<Mutex<()>>,
    reader_stop: Arc<AtomicBool>,
    generation_reader_stops: Arc<Mutex<Vec<Arc<AtomicBool>>>>,
    tail_started: Arc<AtomicBool>,
}
/// Opaque host-owned selection minted only from a validated owner-private dev stage
/// or, on Windows, from the authenticated installed package that holds the host.
pub struct ValidatedBootSelection {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    app: AppBootSelection,
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    permissions_file: File,
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    permissions_digest: [u8; 32],
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
impl Drop for ValidatedBootSelection {
    fn drop(&mut self) {}
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct AppBootSelection {
    root: PathBuf,
    name: String,
    entry_path: PathBuf,
    entry_file: File,
    renderer_html: Vec<u8>,
    #[cfg(windows)]
    mode: WindowsBootMode,
}

/// Which of the two admitted Windows boot states minted the selection (KEL-254 A3 AC2).
#[cfg(windows)]
#[derive(Debug)]
enum WindowsBootMode {
    /// A valid dev lease routed boot to the owner-private stage validator.
    DevStage,
    /// KEL-53 selected the installed package that holds the KEL-135-verified host.
    Installed {
        /// The identity verified at boot; it alone selects the persistent profile.
        identity: ValidatedAppIdentity,
        /// Pins the selected version tree and its protected ancestry while the
        /// session runs from it. Only keld-update can mint it; boxed because it is
        /// far larger than the dev variant.
        active: Box<keld_update::ActivePackageSelection>,
    },
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct GuardSnapshot {
    // T2 retains the verified policy for the whole app session. T3 prepares
    // the sole native FS broker from the same immutable snapshot before any
    // child/listener/window resource exists.
    verified: VerifiedManifest,
    fs: Arc<FsDispatchSession>,
    #[cfg(all(test, target_os = "macos"))]
    drop_observer: Option<Arc<AtomicBool>>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl GuardSnapshot {
    fn prepare(verified: VerifiedManifest) -> Result<Self, FsPrepareError> {
        let broker = FsBroker::prepare(&verified)?;
        let fs = Arc::new(FsDispatchSession::new(verified.clone(), broker));
        Ok(Self {
            verified,
            fs,
            #[cfg(all(test, target_os = "macos"))]
            drop_observer: None,
        })
    }

    fn fs_weak(&self) -> Weak<FsDispatchSession> {
        Arc::downgrade(&self.fs)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct FsDispatchState {
    accepting: bool,
    in_flight: usize,
    call_outstanding: bool,
    pending_call: Option<PendingFsCall>,
    failed_write_attempt: Option<u32>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[derive(Clone, Copy, PartialEq, Eq)]
struct PendingFsCall {
    attempt: u32,
    correlation: CorrelationId,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct FsDispatchSession {
    verified: VerifiedManifest,
    broker: FsBroker,
    state: Mutex<FsDispatchState>,
    handler_transition: Mutex<()>,
    drained: Condvar,
    #[cfg(all(test, target_os = "macos"))]
    drain_wait_observer: Mutex<Option<SyncSender<()>>>,
    #[cfg(all(test, target_os = "macos"))]
    terminal_write_hold: Mutex<Option<(SyncSender<()>, Receiver<()>)>>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl FsDispatchSession {
    fn new(verified: VerifiedManifest, broker: FsBroker) -> Self {
        Self {
            verified,
            broker,
            state: Mutex::new(FsDispatchState {
                accepting: true,
                in_flight: 0,
                call_outstanding: false,
                pending_call: None,
                failed_write_attempt: None,
            }),
            handler_transition: Mutex::new(()),
            drained: Condvar::new(),
            #[cfg(all(test, target_os = "macos"))]
            drain_wait_observer: Mutex::new(None),
            #[cfg(all(test, target_os = "macos"))]
            terminal_write_hold: Mutex::new(None),
        }
    }

    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    fn admit(self: &Arc<Self>) -> Result<Option<FsInFlight>, HostAppError> {
        self.admit_with_pending(None)
    }

    fn begin_call(
        self: &Arc<Self>,
        pending_call: PendingFsCall,
    ) -> Result<Option<FsInFlight>, HostAppError> {
        self.admit_with_pending(Some(pending_call))
    }

    fn admit_with_pending(
        self: &Arc<Self>,
        pending_call: Option<PendingFsCall>,
    ) -> Result<Option<FsInFlight>, HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("filesystem admission", "in-flight lock poisoned"))?;
        if !state.accepting {
            return Ok(None);
        }
        if pending_call.is_some_and(|pending| {
            state.call_outstanding || state.failed_write_attempt == Some(pending.attempt)
        }) {
            return Err(app_detail(
                "filesystem admission",
                "a privileged filesystem Call is already outstanding",
            ));
        }
        state.in_flight = state
            .in_flight
            .checked_add(1)
            .ok_or_else(|| app_detail("filesystem admission", "in-flight count overflow"))?;
        if let Some(pending_call) = pending_call {
            state.call_outstanding = true;
            state.pending_call = Some(pending_call);
        }
        Ok(Some(FsInFlight {
            session: Arc::clone(self),
            pending_call,
        }))
    }

    fn has_outstanding_call_for(&self, attempt: u32) -> bool {
        self.state.lock().map_or(true, |state| {
            state.call_outstanding || state.failed_write_attempt == Some(attempt)
        })
    }

    fn mark_failed_write_attempt(&self, attempt: u32) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("filesystem retirement", "in-flight lock poisoned"))?;
        state.failed_write_attempt = Some(attempt);
        Ok(())
    }

    #[cfg(any(target_os = "linux", windows))]
    fn failed_write_attempt_is(&self, attempt: u32) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.failed_write_attempt == Some(attempt))
    }

    fn clear_failed_write_attempt(&self, attempt: u32) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("filesystem retirement", "in-flight lock poisoned"))?;
        if state.failed_write_attempt == Some(attempt) {
            state.failed_write_attempt = None;
        }
        Ok(())
    }

    fn retire_pending_call(&self, pending_call: PendingFsCall) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("filesystem retirement", "in-flight lock poisoned"))?;
        if state.pending_call == Some(pending_call) {
            state.pending_call = None;
            state.call_outstanding = false;
        }
        Ok(())
    }

    /// The admitted, unanswered FS call of `attempt`, if any (GH-527 §4.9).
    fn pending_call_for(&self, attempt: u32) -> Option<PendingFsCall> {
        self.state.lock().ok().and_then(|state| {
            state
                .pending_call
                .filter(|pending| pending.attempt == attempt)
        })
    }

    fn retire_pending_call_for_attempt(&self, attempt: u32) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("filesystem retirement", "in-flight lock poisoned"))?;
        if state
            .pending_call
            .is_some_and(|pending| pending.attempt == attempt)
        {
            state.pending_call = None;
        }
        Ok(())
    }

    fn close_admission(&self) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("filesystem quiesce", "in-flight lock poisoned"))?;
        state.accepting = false;
        Ok(())
    }

    fn wait_for_handler_transition(&self) -> Result<(), HostAppError> {
        let _transition = self.handler_transition.lock().map_err(|_| {
            app_detail(
                "filesystem quiesce",
                "handler-entry transition lock poisoned",
            )
        })?;
        Ok(())
    }

    fn quiesce(&self) -> Result<(), HostAppError> {
        self.close_admission()?;
        // Returning after this acquisition publishes that every handler which
        // won entry before admission closed has reached its terminal outcome.
        self.wait_for_handler_transition()
    }

    #[cfg(all(test, target_os = "macos"))]
    fn observe_next_drain_wait(&self, observer: SyncSender<()>) {
        *self
            .drain_wait_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(observer);
    }

    /// Test hook: holds the next successful FS terminal write after it
    /// releases the generation lock and before its lease drops, until the
    /// test resumes it.
    #[cfg(all(test, target_os = "macos"))]
    fn hold_next_terminal_write(&self, written: SyncSender<()>, resume: Receiver<()>) {
        *self
            .terminal_write_hold
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((written, resume));
    }

    #[cfg(all(test, target_os = "macos"))]
    fn hold_after_terminal_write(&self) {
        let hold = self
            .terminal_write_hold
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some((written, resume)) = hold {
            let _ = written.send(());
            let _ = resume.recv_timeout(Duration::from_secs(5));
        }
    }

    fn drain(&self) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("filesystem drain", "in-flight lock poisoned"))?;
        while state.in_flight != 0 {
            #[cfg(all(test, target_os = "macos"))]
            if let Some(observer) = self
                .drain_wait_observer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            {
                let _ = observer.send(());
            }
            state = self
                .drained
                .wait(state)
                .map_err(|_| app_detail("filesystem drain", "in-flight lock poisoned"))?;
        }
        Ok(())
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct FsInFlight {
    session: Arc<FsDispatchSession>,
    pending_call: Option<PendingFsCall>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct FsWorkItem {
    admitted: FsInFlight,
    request: FsRequest,
    attempt: u32,
    correlation: CorrelationId,
    cancellation: Arc<AtomicBool>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
enum FsWorkerCommand {
    Work(FsWorkItem),
    Stop,
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
struct FsWorkerTestGate {
    taken: SyncSender<()>,
    release: Mutex<Receiver<()>>,
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
impl FsWorkerTestGate {
    fn wait_after_take(&self) -> Result<(), HostAppError> {
        self.taken
            .send(())
            .map_err(|_| app_detail("filesystem worker test gate", "observer disconnected"))?;
        self.release
            .lock()
            .map_err(|_| app_detail("filesystem worker test gate", "release lock poisoned"))?
            .recv_timeout(Duration::from_secs(5))
            .map_err(|error| {
                app_detail(
                    "filesystem worker test gate",
                    format!("release did not arrive before the test kill switch: {error}"),
                )
            })
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl FsInFlight {
    fn handle_with<T>(&self, handler: impl FnOnce() -> T) -> Result<Option<T>, HostAppError> {
        let _transition = self.session.handler_transition.lock().map_err(|_| {
            app_detail(
                "filesystem handler entry",
                "handler-entry transition lock poisoned",
            )
        })?;
        let state = self
            .session
            .state
            .lock()
            .map_err(|_| app_detail("filesystem handler entry", "in-flight lock poisoned"))?;
        if !state.accepting {
            return Ok(None);
        }
        // Admission state is independent of the transition retained across
        // native I/O, so readers can continue classifying and admitting work.
        drop(state);
        Ok(Some(handler()))
    }

    fn retire_pending_call(&self) -> Result<(), HostAppError> {
        self.pending_call.map_or(Ok(()), |pending_call| {
            self.session.retire_pending_call(pending_call)
        })
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl Drop for FsInFlight {
    fn drop(&mut self) {
        let mut state = self
            .session
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.in_flight != 0 {
            state.in_flight -= 1;
        }
        if let Some(pending_call) = self.pending_call {
            if state.pending_call == Some(pending_call) {
                state.pending_call = None;
                state.call_outstanding = false;
            } else if state.pending_call.is_none() {
                // Generation retirement may clear correlation metadata while
                // this lease still owns the global single-flight slot.
                state.call_outstanding = false;
            }
        }
        if state.in_flight == 0 {
            self.session.drained.notify_all();
        }
    }
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
    /// before reading the executable path or any boot file. Windows admits
    /// exactly two boot states (KEL-254 A3 AC2): a valid dev lease reaches only
    /// stage validation, and a lease-less launch boots only the authenticated
    /// installed package that holds the KEL-135-verified executable.
    ///
    /// # Errors
    ///
    /// Returns [`HostAppError`] for unsupported platforms, invalid descriptor
    /// bytes, an unsafe staged root, a missing/escaping/non-regular target, or,
    /// on Windows, an invalid dev lease, an unverified executable, a missing or
    /// invalid embedded expectation (`KELD-UPDATE-019`/`KELD-UPDATE-017`), an
    /// unadmitted installation (`KELD-UPDATE-*`), or installed provenance naming
    /// another publisher or app (`KELD-WV-009`).
    pub fn from_current_exe_unprivileged() -> Result<Self, HostAppError> {
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        {
            Err(HostAppError::new(
                "KELD-CORE-034",
                "platform availability",
                "no-flag host support is unavailable on this platform",
                NO_FLAG_UNAVAILABLE_FIX,
            ))
        }
        #[cfg(windows)]
        {
            select_windows_boot_route_with(
                windows_dev_lease_requested()?,
                validate_current_exe_stage,
                validate_installed_current_exe,
            )
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            validate_current_exe_stage()
        }
    }
}

/// Validates the owner-private stage beside the current executable.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn validate_current_exe_stage() -> Result<ValidatedBootSelection, HostAppError> {
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

/// Typed no-flag host boot/session failure.
pub struct HostAppError {
    code: &'static str,
    phase: &'static str,
    detail: String,
    fix: &'static str,
    resources: StartupResourceSnapshot,
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
            #[cfg(any(target_os = "macos", target_os = "linux", windows))]
            manifest_source: None,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
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

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn fs_prepare(source: &FsPrepareError) -> Self {
        Self::new(
            source.code(),
            "filesystem broker preflight",
            source.to_string(),
            "Correct the filesystem scopes in keld.permissions.jsonc, rebuild the staged boot artifact, and relaunch.",
        )
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
                #[cfg(any(target_os = "macos", target_os = "linux", windows))]
                {
                    self.manifest_source.as_ref()
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
                {
                    Option::<&ManifestError>::None
                }
            })
            .finish_non_exhaustive()
    }
}

impl std::error::Error for HostAppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        #[cfg(any(target_os = "macos", target_os = "linux", windows))]
        {
            self.manifest_source
                .as_ref()
                .map(|source| source.as_ref() as &(dyn std::error::Error + 'static))
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        {
            None
        }
    }
}

fn startup_resource_snapshot() -> StartupResourceSnapshot {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        StartupResourceSnapshot {
            listener: LISTENER_ATTEMPTS.load(Ordering::Acquire),
            child: CHILD_ATTEMPTS.load(Ordering::Acquire),
            window: WINDOW_ATTEMPTS.load(Ordering::Acquire),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        StartupResourceSnapshot::default()
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BootDocument {
    schema: u8,
    name: String,
    entry: String,
    renderer: String,
    permissions: PermissionsDocument,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissionsDocument {
    file: String,
    content_sha256: String,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
struct ParsedBoot {
    name: String,
    entry: PathBuf,
    renderer: PathBuf,
    permissions_digest: [u8; 32],
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
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

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
fn boot_error(detail: impl Into<String>, fix: &'static str) -> HostAppError {
    HostAppError::new("KELD-CORE-035", "boot descriptor validation", detail, fix)
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
fn target_error(kind: &'static str, detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-036",
        "staged target validation",
        format!("{kind}: {}", detail.into()),
        "Regenerate the owner-private stage with readable regular files and no symlinks.",
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
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

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
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
    validate_windows_dev_stage_acl(&root)
        .map_err(|source| target_error("app root DACL", source.to_string()))?;
    validate_windows_boot_files(root, WindowsBootMode::DevStage)
}

/// Opens the strict descriptor and its fixed targets beneath `root`, the dev stage
/// or the KEL-53-selected installed tree, through the one Windows path owner.
///
/// The descriptor, path and digest rules are those of every boot state; only the
/// caller's admission of `root` differs (the owner-private DACL for a dev stage, the
/// KEL-53 selection and its recorded-mode protection for an installed tree).
#[cfg(windows)]
fn validate_windows_boot_files(
    root: PathBuf,
    mode: WindowsBootMode,
) -> Result<ValidatedBootSelection, HostAppError> {
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
            mode,
        },
        permissions_file,
        permissions_digest: parsed.permissions_digest,
    })
}

/// Validates the one canonical owner-private Windows dev-stage ACL policy.
///
/// The boot compiler calls this after atomic creation and the host calls it
/// again before consuming any staged resource, so the producer and enforcer
/// cannot drift onto different owner/ACE rules.
///
/// # Errors
///
/// Returns an I/O error when `TokenUser` or descriptor readback fails, or when
/// the root is not protected by exactly one inheritable current-user
/// full-control allow ACE.
#[cfg(windows)]
pub fn validate_windows_dev_stage_acl(root: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let mut options = fs::OpenOptions::new();
    options.read(true);
    options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    let directory = options.open(root)?;
    keld_guard::validate_windows_owner_private_directory(&directory)
}

/// Prepared Windows dev-stage cleanup owner that survives the terminal CLI.
///
/// This owner contains only the validated nonce root and a live handle to the
/// exact staged host process. It owns no window, app-link, Bun process, or
/// application principal.
#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsDevStageCleanup {
    root: PathBuf,
    host: OwnedHandle,
}

#[cfg(windows)]
impl WindowsDevStageCleanup {
    /// Validates the owner-private stage and binds cleanup to the exact staged
    /// host process object before the CLI releases its namespace guards.
    ///
    /// # Errors
    ///
    /// Returns `KELD-CORE-037` when the root is not the canonical nonce layout,
    /// its DACL is not the approved current-user policy, the PID cannot be
    /// opened, or the live process image is not this stage's `keld-host.exe`.
    #[allow(unsafe_code)] // isolated Win32 process-handle acquisition and image readback
    pub fn prepare(root: &Path, host_pid: u32) -> Result<Self, HostAppError> {
        let root = root
            .canonicalize()
            .map_err(|source| app_io("Windows dev-stage cleanup root", &source))?;
        validate_windows_cleanup_root(&root)?;
        validate_windows_dev_stage_acl(&root)
            .map_err(|source| app_io("Windows dev-stage cleanup ACL", &source))?;
        let expected_host = root
            .join("keld-host.exe")
            .canonicalize()
            .map_err(|source| app_io("Windows dev-stage cleanup host", &source))?;

        // SAFETY: `host_pid` is the PID returned by the CLI's live Child. The
        // requested rights are observation/wait only; a non-null result is one
        // fresh owning handle converted exactly once below.
        let raw_host = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                host_pid,
            )
        };
        if raw_host.is_null() {
            return Err(app_io(
                "Windows dev-stage cleanup process open",
                &io::Error::last_os_error(),
            ));
        }
        // SAFETY: `raw_host` is the fresh non-null owning handle returned above.
        let host = unsafe { OwnedHandle::from_raw_handle(raw_host.cast()) };
        let observed_host = windows_process_image(&host)?
            .canonicalize()
            .map_err(|source| app_io("Windows dev-stage cleanup process image", &source))?;
        if observed_host != expected_host {
            return Err(app_detail(
                "Windows dev-stage cleanup process identity",
                format!(
                    "live PID {host_pid} image `{}` does not equal staged host `{}`",
                    observed_host.display(),
                    expected_host.display()
                ),
            ));
        }
        Ok(Self { root, host })
    }

    /// Waits for the exact staged host object, verifies the exact attempt Job
    /// has no active processes, and then deletes only its validated owner-private
    /// nonce directory.
    ///
    /// # Errors
    ///
    /// Returns `KELD-CORE-037` if waiting, final ACL validation, or deletion
    /// fails. A concurrent orderly CLI deletion is accepted as idempotent.
    #[allow(unsafe_code)] // read-only wait on the owned staged-host process handle
    pub fn wait_and_delete_after_family_exit(
        self,
        attempt_job: &keld_runtime::windows_job::WindowsProcessJob,
    ) -> Result<(), HostAppError> {
        use std::os::windows::io::AsRawHandle as _;

        // SAFETY: `host` is a live owning process handle. An infinite kernel
        // wait is event-driven and returns only after that exact object exits.
        if unsafe { WaitForSingleObject(self.host.as_raw_handle().cast(), u32::MAX) }
            != WAIT_OBJECT_0
        {
            return Err(app_io(
                "Windows dev-stage cleanup host wait",
                &io::Error::last_os_error(),
            ));
        }
        let active_processes = attempt_job.active_processes().map_err(|source| {
            app_detail(
                "Windows dev-stage cleanup attempt Job query",
                source.to_string(),
            )
        })?;
        if active_processes != 0 {
            return Err(app_detail(
                "Windows dev-stage cleanup attempt Job proof",
                format!(
                    "refusing stage deletion while the exact attempt Job reports {active_processes} active processes"
                ),
            ));
        }
        validate_windows_dev_stage_acl(&self.root)
            .map_err(|source| app_io("Windows dev-stage cleanup final ACL", &source))?;
        let Self { root, host } = self;
        drop(host);
        remove_windows_dev_stage(&root)
            .map_err(|source| app_io("Windows dev-stage cleanup deletion", &source))
    }
}

#[cfg(windows)]
fn remove_windows_dev_stage(root: &Path) -> io::Result<()> {
    let deadline = Instant::now() + WINDOWS_DEV_STAGE_DELETE_TIMEOUT;
    remove_windows_dev_stage_with(root, || Instant::now() < deadline, thread::yield_now)
}

#[cfg(windows)]
fn remove_windows_dev_stage_with(
    root: &Path,
    mut may_retry: impl FnMut() -> bool,
    mut on_transient_sharing_violation: impl FnMut(),
) -> io::Result<()> {
    loop {
        match fs::remove_dir_all(root) {
            Ok(()) => return Ok(()),
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(source)
                if source.raw_os_error() == Some(WINDOWS_SHARING_VIOLATION) && may_retry() =>
            {
                // Windows can release a terminating Job member's current-directory
                // handle just after the process object becomes signaled. Rust's
                // remove_dir_all already uses this bounded yield strategy for child
                // entry sharing violations; repeat the hardened whole-tree operation
                // because its final-directory path does not cover that error. A
                // monotonic deadline, rather than a scheduler-yield count, bounds the
                // cold cleanup under load.
                on_transient_sharing_violation();
            }
            Err(source) => return Err(source),
        }
    }
}

#[cfg(windows)]
fn validate_windows_cleanup_root(root: &Path) -> Result<(), HostAppError> {
    let nonce = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| app_detail("Windows dev-stage cleanup root", "nonce is not UTF-8"))?;
    let exact_nonce = nonce.len() == 32
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !exact_nonce
        || root
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            != Some("dev")
        || root
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            != Some(".keld")
    {
        return Err(app_detail(
            "Windows dev-stage cleanup root",
            "expected canonical .keld/dev/<32-lowerhex> nonce layout",
        ));
    }
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)] // read-only Win32 process image query on an owned process handle
fn windows_process_image(process: &OwnedHandle) -> Result<PathBuf, HostAppError> {
    use std::os::windows::io::AsRawHandle as _;

    let mut buffer = vec![0_u16; 32_768];
    let mut length = u32::try_from(buffer.len()).map_err(|_| {
        app_detail(
            "Windows dev-stage cleanup process image",
            "image buffer exceeds u32",
        )
    })?;
    // SAFETY: `process` is live and buffer supplies `length` writable UTF-16
    // units. The API writes the resulting unit count back to `length`.
    if unsafe {
        QueryFullProcessImageNameW(
            process.as_raw_handle().cast(),
            0,
            buffer.as_mut_ptr(),
            &raw mut length,
        )
    } == 0
    {
        return Err(app_io(
            "Windows dev-stage cleanup process image",
            &io::Error::last_os_error(),
        ));
    }
    let length = usize::try_from(length).map_err(|_| {
        app_detail(
            "Windows dev-stage cleanup process image",
            "returned image length exceeds usize",
        )
    })?;
    if length > buffer.len() {
        return Err(app_detail(
            "Windows dev-stage cleanup process image",
            "returned image length exceeds buffer",
        ));
    }
    buffer.truncate(length);
    Ok(PathBuf::from(OsString::from_wide(&buffer)))
}

#[cfg(windows)]
fn open_relative_file_windows(
    root: &Path,
    path: &Path,
    kind: &'static str,
) -> Result<File, HostAppError> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};

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
    // Open the leaf reparse point itself instead of following it. The
    // owner-private dev root, or the selected installed tree's recorded-mode
    // protection, makes component replacement by another principal unavailable;
    // same-user mutation remains outside the dev and per-user boundaries.
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&candidate)
        .map_err(|source| target_error(kind, source.to_string()))?;
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn read_target(mut file: File, kind: &'static str) -> Result<Vec<u8>, HostAppError> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|source| target_error(kind, source.to_string()))?;
    Ok(bytes)
}

/// Runs the validated no-flag host selection until ordered application exit.
/// On Linux, process entry must call [`crate::prepare_webview_process`] first.
///
/// # Errors
///
/// Returns [`HostAppError`] for startup, authenticated session, window,
/// guardian, Bun self-termination, or ordered-shutdown failure.
pub fn run_unprivileged(boot: ValidatedBootSelection) -> Result<(), HostAppError> {
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        drop(boot);
        Err(HostAppError::new(
            "KELD-CORE-034",
            "platform availability",
            "no-flag host support is unavailable on this platform",
            NO_FLAG_UNAVAILABLE_FIX,
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
    #[cfg(windows)]
    {
        let ValidatedBootSelection {
            app,
            permissions_file,
            permissions_digest,
        } = boot;
        drop((permissions_file, permissions_digest));
        run_app_direct(app, None)
    }
    #[cfg(target_os = "linux")]
    {
        let ValidatedBootSelection {
            app,
            permissions_file,
            permissions_digest,
        } = boot;
        drop((permissions_file, permissions_digest));
        run_app_direct(app, None)
    }
}

/// Runs a validated no-flag host session with one immutable verified policy snapshot.
///
/// Policy preflight consumes the already-open KEL-96 permissions handle and
/// decoded digest before the app can create a child, listener, or window.
/// On Linux, process entry must call [`crate::prepare_webview_process`] first.
///
/// # Errors
///
/// Returns [`HostAppError`] for a typed manifest preflight failure or any
/// existing no-flag startup, session, window, guardian, Bun, or shutdown error.
pub fn run_guarded(boot: ValidatedBootSelection) -> Result<(), HostAppError> {
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        drop(boot);
        Err(HostAppError::new(
            "KELD-CORE-034",
            "platform availability",
            "no-flag host support is unavailable on this platform",
            NO_FLAG_UNAVAILABLE_FIX,
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
        let guard_snapshot =
            GuardSnapshot::prepare(verified).map_err(|source| HostAppError::fs_prepare(&source))?;
        run_app_direct(app, Some(&guard_snapshot))
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
        let guard_snapshot =
            GuardSnapshot::prepare(verified).map_err(|source| HostAppError::fs_prepare(&source))?;
        // The owner stays in this frame while run_app performs every startup,
        // event-loop, and ordered-cleanup step. run_app receives only a borrow,
        // so it cannot destroy the verified session policy early.
        run_app(app, Some(&guard_snapshot))
    }
    #[cfg(target_os = "linux")]
    {
        let ValidatedBootSelection {
            app,
            permissions_file,
            permissions_digest,
        } = boot;
        let display_path = app.root.join(PERMISSIONS_FILE);
        let verified = load_verified_manifest(permissions_file, display_path, permissions_digest)
            .map_err(HostAppError::manifest)?;
        let guard_snapshot =
            GuardSnapshot::prepare(verified).map_err(|source| HostAppError::fs_prepare(&source))?;
        run_app_direct(app, Some(&guard_snapshot))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl SessionShutdownState {
    fn new() -> Self {
        Self {
            cause: Arc::new(AtomicU8::new(SESSION_RUNNING)),
            transition: Arc::new(Mutex::new(())),
            reader_stop: Arc::new(AtomicBool::new(false)),
            generation_reader_stops: Arc::new(Mutex::new(Vec::new())),
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

    #[cfg(any(unix, all(test, windows)))]
    fn claim_cli_lease_lost(&self) -> bool {
        self.claim(SESSION_CLI_LEASE_LOST)
    }

    #[cfg(any(target_os = "macos", target_os = "linux", test))]
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
        let readers = self
            .generation_reader_stops
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.reader_stop.store(true, Ordering::Release);
        for reader in readers.iter() {
            reader.store(true, Ordering::Release);
        }
    }

    fn register_generation_reader(&self) -> Arc<AtomicBool> {
        let mut readers = self
            .generation_reader_stops
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let reader = Arc::new(AtomicBool::new(self.reader_stop.load(Ordering::Acquire)));
        readers.push(Arc::clone(&reader));
        reader
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
    let mut dev_lease = DevHostLease::from_environment()?;
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
    // Authenticate the running package, or mint one explicit dev nonce, before
    // starting the guardian child, listener, app-link, or WebKit engine.
    let profile_selection = macos_profile_selection(dev_lease.as_ref())?;

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
    let Some(initial) = await_bound_generation(
        &mut guardian,
        dev_lease.as_mut(),
        Instant::now() + APP_LINK_IO_DEADLINE,
        "initial app-link authentication",
    )?
    else {
        return Ok(());
    };

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
        guard_snapshot.map(GuardSnapshot::fs_weak),
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

    let (renderer_requests_tx, renderer_requests_rx) = mpsc::sync_channel(1);
    let (renderer_outcomes_tx, renderer_outcomes_rx) = mpsc::channel();
    let renderer_dispatch = match start_renderer_dispatch(
        renderer_requests_rx,
        renderer_outcomes_tx,
        router.handle(),
    ) {
        Ok(dispatch) => dispatch,
        Err(primary) => {
            drop(renderer_requests_tx);
            drop(renderer_outcomes_rx);
            return cleanup_window_start_failure(
                &primary,
                guard_snapshot,
                window_events_tx,
                event_coordinator,
                None,
                router,
                guardian_owner,
            );
        }
    };

    let mut engine = match WkWebViewEngine::new(profile_selection) {
        Ok(engine) => engine,
        Err(source) => {
            let primary = app_webview_error("macOS profile store initialization", &source);
            drop(renderer_requests_tx);
            drop(renderer_outcomes_rx);
            return cleanup_window_start_failure(
                &primary,
                guard_snapshot,
                window_events_tx,
                event_coordinator,
                Some(renderer_dispatch),
                router,
                guardian_owner,
            );
        }
    };
    let spec = WebviewSpec {
        title: name,
        initial: NavTarget::Html(html),
        ..WebviewSpec::default()
    };
    WINDOW_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    // The bridge admits exactly the host's echo entry (GH-508 §4.6); the
    // request loop below re-checks the same id before dispatch.
    let renderer_endpoint =
        RendererBridgeEndpoint::new(renderer_requests_tx, renderer_outcomes_rx, ECHO_CHANNEL.0);
    if let Err(source) =
        engine.create_app_with_renderer_bridge(&spec, window_events_tx.clone(), renderer_endpoint)
    {
        let primary = app_webview_error("initial renderer bridge window", &source);
        return cleanup_window_start_failure(
            &primary,
            guard_snapshot,
            window_events_tx,
            event_coordinator,
            Some(renderer_dispatch),
            router,
            guardian_owner,
        );
    }
    let window_result = engine.run_app_until_quit(window_commands_rx, window_events_tx);
    finish_guarded_session(guard_snapshot, |_| {
        drop(window_commands_tx);
        let event_result = event_coordinator
            .join()
            .map_err(|_| app_detail("window event coordinator", "thread panicked"))
            .and_then(std::convert::identity);
        let router_result = router.shutdown();
        let renderer_result = join_renderer_dispatch(renderer_dispatch);
        let guardian_result = guardian_owner.shutdown();

        match window_result {
            Err(source @ WvError::Navigate(_)) => {
                let primary = app_webview_error("initial navigation", &source);
                Err(collapse_app_failures(
                    &primary,
                    [
                        guardian_result,
                        renderer_result,
                        router_result,
                        event_result,
                    ],
                ))
            }
            result => collapse_app_results([
                result.map_err(|source| app_webview_error("macOS app window", &source)),
                event_result,
                router_result,
                renderer_result,
                guardian_result,
            ]),
        }
    })
}

#[cfg(target_os = "macos")]
const MACOS_PUBLISHER_SCOPE_DOMAIN: &[u8] = b"keld.publisher.macos/v1\0";

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct MacValidatedAppIdentity {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    team_identifier: Box<str>,
    publisher_scope: [u8; 32],
    signing_identifier: Box<str>,
    profile_identity: ProfileIdentity,
}

#[cfg(target_os = "macos")]
impl MacValidatedAppIdentity {
    fn from_verified_parts(
        team_identifier: &str,
        signing_identifier: &str,
    ) -> Result<Self, HostAppError> {
        if team_identifier.len() != 10
            || !team_identifier
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        {
            return Err(macos_identity_error(
                "the validated code signature has no canonical 10-character Team Identifier",
            ));
        }
        let mut publisher_hasher = Sha256::new();
        publisher_hasher.update(MACOS_PUBLISHER_SCOPE_DOMAIN);
        publisher_hasher.update(team_identifier.as_bytes());
        let publisher_digest = publisher_hasher.finalize();
        let mut publisher_scope = [0_u8; 32];
        publisher_scope.copy_from_slice(&publisher_digest);
        let profile_identity = ProfileIdentity::from_host_verified_parts(
            publisher_scope,
            signing_identifier,
        )
        .map_err(|source| {
            macos_identity_error(format!(
                "the validated code-signing identifier is not a canonical Keld app id: {source}"
            ))
        })?;
        Ok(Self {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            team_identifier: team_identifier.into(),
            publisher_scope,
            signing_identifier: signing_identifier.into(),
            profile_identity,
        })
    }

    fn into_profile_selection(self) -> Result<WebProfileSelection, HostAppError> {
        debug_assert_eq!(
            ProfileIdentity::from_host_verified_parts(
                self.publisher_scope,
                &self.signing_identifier,
            ),
            Ok(self.profile_identity)
        );
        WebProfileSelection::persistent(Some(self.profile_identity))
            .map_err(|source| macos_identity_error(source.to_string()))
    }
}

#[cfg(target_os = "macos")]
fn select_macos_profile_mode_with(
    has_authenticated_dev_lease: bool,
    release_identity: impl FnOnce() -> Result<MacValidatedAppIdentity, HostAppError>,
) -> Result<WebProfileSelection, HostAppError> {
    if has_authenticated_dev_lease {
        let mut launch_nonce = [0_u8; 32];
        fill_os_random(&mut launch_nonce)
            .map_err(|source| macos_identity_error(format!("host randomness failed: {source}")))?;
        let profile = EphemeralProfile::from_host_random(launch_nonce)
            .map_err(|source| macos_identity_error(source.to_string()))?;
        return Ok(WebProfileSelection::ephemeral_dev(profile));
    }
    release_identity()?.into_profile_selection()
}

#[cfg(target_os = "macos")]
fn macos_profile_selection(
    dev_lease: Option<&DevHostLease>,
) -> Result<WebProfileSelection, HostAppError> {
    select_macos_profile_mode_with(dev_lease.is_some(), verified_macos_app_identity)
}

#[cfg(target_os = "macos")]
fn verified_macos_app_identity() -> Result<MacValidatedAppIdentity, HostAppError> {
    let signing_info = verified_current_process_signing_info().map_err(macos_identity_error)?;
    MacValidatedAppIdentity::from_verified_parts(
        &signing_info.team_identifier,
        &signing_info.signing_identifier,
    )
}

#[cfg(target_os = "macos")]
fn macos_identity_error(detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-WV-009",
        "macOS authenticated app identity",
        detail,
        "Launch a validly signed macOS app with a canonical signed identifier, or use `keld dev` for a fresh nonpersistent profile.",
    )
}

/// Reports only facts read from this process after successful Security.framework validation.
///
/// This entry point is compiled only for the debug KEL-135 signed-fixture role.
///
/// # Errors
///
/// Returns an error if the running signature is invalid or lacks a canonical identity.
#[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
pub fn macos_profile_identity_fixture_report() -> Result<String, HostAppError> {
    use core::fmt::Write as _;

    let identity = verified_macos_app_identity()?;
    let publisher_scope =
        identity
            .publisher_scope
            .iter()
            .fold(String::with_capacity(64), |mut text, byte| {
                // Writing into a pre-sized String cannot fail.
                let _ = write!(&mut text, "{byte:02x}");
                text
            });
    let profile_identity = identity.profile_identity.namespace_segment();
    let store_uuid = identity.profile_identity.apple_store_uuid();
    Ok(format!(
        "KELD_KEL135_SIGNED_IDENTITY team_id={} signing_identifier={} publisher_scope={} profile_identity={} store_uuid={} signature_validated_before_identity_read=true",
        identity.team_identifier,
        identity.signing_identifier,
        publisher_scope,
        profile_identity,
        store_uuid
    ))
}

/// Reports whether the validated current app's `WebKit` registry contains its deterministic UUID.
///
/// This read-only fixture probe uses a memory-only bootstrap store and does not open the
/// requested persistent store.
///
/// # Errors
///
/// Returns an error if the running signature is invalid or `WebKit` cannot enumerate its
/// current-app store identifiers.
#[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
pub fn macos_profile_store_presence_fixture_report() -> Result<String, HostAppError> {
    let identity = verified_macos_app_identity()?;
    let store_uuid = identity.profile_identity.apple_store_uuid();
    let present =
        WkWebViewEngine::persistent_store_identifier_present_for_test(*store_uuid.as_bytes())
            .map_err(|source| app_webview_error("macOS profile store presence fixture", &source))?;
    Ok(format!(
        "KELD_KEL135_STORE_PRESENCE profile_identity={} store_uuid={} present={present}",
        identity.profile_identity.namespace_segment(),
        store_uuid
    ))
}

/// Purges this signed fixture's exact profile from a fresh clean host process.
///
/// The test host returns the evidence line only after `WebKit`'s removal callback
/// and identifier re-enumeration complete.
///
/// # Errors
///
/// Returns an error if the running identity is invalid or the exact store cannot be purged.
#[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
pub fn macos_profile_purge_fixture() -> Result<String, HostAppError> {
    let identity = verified_macos_app_identity()?;
    let profile_identity = identity.profile_identity;
    let store_uuid = profile_identity.apple_store_uuid();
    WkWebViewEngine::purge_persistent_profile(profile_identity)
        .map_err(|source| app_webview_error("macOS profile purge fixture", &source))?;
    Ok(format!(
        "KELD_KEL135_PURGE_COMPLETE profile_identity={} store_uuid={} store_absent=true",
        profile_identity.namespace_segment(),
        store_uuid
    ))
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
fn profile_fixture_loopback_authority(
    url: &str,
    expected_origin: Option<std::net::SocketAddrV4>,
) -> Option<std::net::SocketAddrV4> {
    let rest = url.strip_prefix("http://")?;
    let (authority, _) = rest.split_once('/')?;
    if url
        .chars()
        .any(|character| character.is_whitespace() || character.is_control() || character == '\\')
    {
        return None;
    }
    let address = authority.parse::<std::net::SocketAddrV4>().ok()?;
    (*address.ip() == std::net::Ipv4Addr::LOCALHOST
        && address.port() != 0
        && expected_origin.is_none_or(|expected| expected == address))
    .then_some(address)
}

#[cfg(all(
    test,
    target_os = "macos",
    feature = "profile-test-hooks",
    debug_assertions
))]
#[test]
fn profile_fixture_urls_require_exact_loopback_authorities() {
    for invalid in [
        "https://127.0.0.1:1234/a",
        "HTTP://127.0.0.1:1234/a",
        "http://127.0.0.1:1234",
        "http://127.0.0.1/a",
        "http://example.com:1234/a",
        "http://127.0.0.2:1234/a",
        "http://[::1]:1234/a",
        "http://user@127.0.0.1:1234/a",
        "http://user:password@127.0.0.1:1234/a",
        "http://127.0.0.1:0/a",
        "http://127.0.0.1:65536/a",
        "http://127.0.0.1:1234\\a",
        "http://127.0.0.1:1234/a\\b",
        "http://127.0.0.1:1234/a b",
        "http://127.0.0.1:1234/a\n",
        "http://127.0.0.1:1234/a\u{00a0}",
    ] {
        assert!(profile_fixture_loopback_authority(invalid, None).is_none());
    }
    for port in [1, 1234, u16::MAX] {
        let first = format!("http://127.0.0.1:{port}/a?b=c");
        let second = format!("http://127.0.0.1:{port}/other");
        let origin = profile_fixture_loopback_authority(&first, None);
        assert_eq!(origin.map(|address| address.port()), Some(port));
        assert_eq!(profile_fixture_loopback_authority(&second, origin), origin);
    }
    let origin = profile_fixture_loopback_authority("http://127.0.0.1:1234/a", None);
    assert!(origin.is_some());
    assert!(profile_fixture_loopback_authority("http://127.0.0.1:1235/b", origin).is_none());
}

/// Runs the real `WKWebView` profile backend from a signed debug acceptance fixture.
///
/// The fixture URL selects only page content; the persistent profile is still
/// derived from the validated current-process signature.
///
/// # Errors
///
/// Returns an error when signature validation, profile setup, `WebView` creation, or
/// the fixture lifecycle fails.
#[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
pub fn run_macos_profile_webview_fixture() -> Result<(), HostAppError> {
    if std::env::var_os("KELD_PROFILE_FIXTURE_SIGNED_ATTEST")
        .as_deref()
        .is_some_and(|value| value == "1")
    {
        eprintln!("{}", macos_profile_identity_fixture_report()?);
    }
    let dev_ephemeral = std::env::var_os("KELD_PROFILE_FIXTURE_EPHEMERAL")
        .as_deref()
        .is_some_and(|value| value == "1");
    let media_seed_allow = std::env::var_os("KELD_PROFILE_TEST_MEDIA_SEED_ALLOW")
        .as_deref()
        .is_some_and(|value| value == "1");
    if media_seed_allow
        && (std::env::var_os("KELD_PROFILE_TEST_ROOT").is_none()
            || std::env::var_os("KELD_PROFILE_ACCEPTANCE_REPORT").is_none())
    {
        return Err(macos_identity_error(
            "fixture media Allow requires an isolated acceptance root and evidence report",
        ));
    }
    let profile = select_macos_profile_mode_with(dev_ephemeral, verified_macos_app_identity)?;
    let url = std::env::var("KELD_PROFILE_FIXTURE_URL").map_err(|source| {
        macos_identity_error(format!("profile fixture URL is unavailable: {source}"))
    })?;
    let origin = profile_fixture_loopback_authority(&url, None)
        .ok_or_else(|| macos_identity_error("profile fixture URL is not a local test origin"))?;
    let second_url = std::env::var("KELD_PROFILE_FIXTURE_SECOND_URL").ok();
    if let Some(second_url) = &second_url
        && profile_fixture_loopback_authority(second_url, Some(origin)).is_none()
    {
        return Err(macos_identity_error(
            "second profile fixture URL must use the first fixture's exact loopback origin",
        ));
    }

    let mut engine = WkWebViewEngine::new_profile_test_fixture(profile, media_seed_allow)
        .map_err(|source| app_webview_error("macOS profile fixture initialization", &source))?;
    let (commands_tx, commands_rx) = mpsc::channel();
    let (events_tx, events_rx) = mpsc::channel();
    let fatal_on_stdin_close = std::env::var_os("KELD_PROFILE_FIXTURE_FATAL_ON_STDIN")
        .as_deref()
        .is_some_and(|value| value == "1");
    let shutdown = thread::Builder::new()
        .name("keld-kel135-profile-fixture-control".to_owned())
        .spawn(move || {
            let mut byte = [0_u8; 1];
            loop {
                match io::stdin().read(&mut byte) {
                    Ok(0) | Err(_) => {
                        let command = if fatal_on_stdin_close {
                            AppWindowCommand::Fatal
                        } else {
                            AppWindowCommand::Quit
                        };
                        let _ = commands_tx.send(command);
                        return;
                    }
                    Ok(_) => {}
                }
            }
        })
        .map_err(|source| app_io("profile fixture shutdown reader", &source))?;
    let spec = WebviewSpec {
        title: String::from("KEL-135 signed profile fixture"),
        initial: NavTarget::Url(url),
        ..WebviewSpec::default()
    };
    engine
        .create_app(&spec, events_tx.clone())
        .map_err(|source| app_webview_error("macOS profile fixture window", &source))?;
    if let Some(second_url) = second_url {
        let second_spec = WebviewSpec {
            title: String::from("KEL-135 shared-store fixture view"),
            initial: NavTarget::Url(second_url),
            ..WebviewSpec::default()
        };
        keld_wv::WebEngine::create(&mut engine, &second_spec)
            .map_err(|source| app_webview_error("macOS second profile fixture view", &source))?;
    }
    let run_result = engine
        .run_app_until_quit(commands_rx, events_tx)
        .map_err(|source| app_webview_error("macOS profile fixture event loop", &source));
    drop(events_rx);
    let shutdown_result = shutdown
        .join()
        .map_err(|_| app_detail("profile fixture shutdown reader", "thread panicked"));
    collapse_app_results([run_result, shutdown_result])
}

#[cfg(windows)]
#[derive(Debug)]
struct ValidatedAppIdentity {
    publisher_scope: [u8; 32],
    app_id: Box<str>,
    profile_identity: ProfileIdentity,
}

#[cfg(windows)]
impl ValidatedAppIdentity {
    /// Builds the profile identity from the publisher scope and app id that the
    /// `keld-guard` Authenticode owner verified; the app id must also be canonical.
    fn from_verified_parts(publisher_scope: [u8; 32], app_id: &str) -> Result<Self, HostAppError> {
        let profile_identity = ProfileIdentity::from_host_verified_parts(publisher_scope, app_id)
            .map_err(|source| {
                windows_identity_error(
                    source.to_string(),
                    "Sign the package with an exact canonical `keld.app-id/v1:<app.id>` description, then rebuild it.",
                )
            })?;
        Ok(Self {
            publisher_scope,
            app_id: app_id.into(),
            profile_identity,
        })
    }

    fn into_profile_selection(self) -> Result<WebProfileSelection, HostAppError> {
        debug_assert_eq!(
            ProfileIdentity::from_host_verified_parts(self.publisher_scope, &self.app_id),
            Ok(self.profile_identity)
        );
        WebProfileSelection::persistent(Some(self.profile_identity)).map_err(|source| {
            windows_identity_error(
                source.to_string(),
                "Restore the authenticated Windows package identity and relaunch.",
            )
        })
    }
}

#[cfg(windows)]
#[derive(Debug)]
enum WindowsProfileMode {
    Persistent(WebProfileSelection),
    EphemeralDev,
}

#[cfg(windows)]
fn windows_identity_error(detail: impl Into<String>, fix: &'static str) -> HostAppError {
    HostAppError::new(
        "KELD-WV-009",
        "Windows authenticated app identity",
        detail,
        fix,
    )
}

/// Reports a `keld-guard` Authenticode refusal as `KELD-WV-009`; a refusal whose
/// `WinTrust` state also failed to close is reported as one startup-cleanup failure.
#[cfg(windows)]
fn windows_authenticode_error(error: &WindowsAuthenticodeError) -> HostAppError {
    let primary = windows_identity_error(error.detail(), error.fix());
    match error.close_failure() {
        None => primary,
        Some(close_failure) => collapse_app_failures(
            &primary,
            [Err(windows_identity_error(
                close_failure.detail(),
                close_failure.fix(),
            ))],
        ),
    }
}

/// Routes Windows boot to exactly one of its two admitted states (KEL-254 A3 AC2).
///
/// A valid dev lease routes only to the `DevStage` validator: the lease grants no
/// authority, the owner-private stage checks do. Without one, only the
/// authenticated installed package that holds the verified executable may boot,
/// and every other launch, including a lease-less staged layout, is refused there.
#[cfg(windows)]
fn select_windows_boot_route_with(
    dev_lease: bool,
    dev_stage: impl FnOnce() -> Result<ValidatedBootSelection, HostAppError>,
    installed: impl FnOnce() -> Result<ValidatedBootSelection, HostAppError>,
) -> Result<ValidatedBootSelection, HostAppError> {
    if dev_lease { dev_stage() } else { installed() }
}

/// Boots the authenticated installed package that holds the running executable
/// (KEL-254 A3 §4, T3).
///
/// The executable is verified once through the `keld-guard` KEL-135 owner, which
/// pins its image without write or delete sharing; that one handle then supplies
/// the embedded expectation and the executable identity. The canonical path only
/// locates the installation.
#[cfg(windows)]
fn validate_installed_current_exe() -> Result<ValidatedBootSelection, HostAppError> {
    let executable = current_windows_executable()?;
    let (verified, identity) = verified_windows_image(&executable)?;
    let locator = executable.canonicalize().map_err(|source| {
        HostAppError::io(
            "KELD-CORE-036",
            "installed executable locator",
            &source,
            "Launch keld-host.exe from its installation's selected version tree.",
        )
    })?;
    validate_installed_from_verified(&locator, verified.file(), identity)
}

/// Selects and validates the installed package for `image`, which must be the
/// handle whose image produced `identity`.
///
/// The expectation is read from `image` before any installation is located; KEL-53
/// then requires the record to name `identity`'s publisher and app, the one signer rule
/// it owns for both installed images, and binds `image` to the selected tree by file
/// identity. Every refusal precedes any listener, child or window.
#[cfg(windows)]
fn validate_installed_from_verified(
    locator: &Path,
    image: &File,
    identity: ValidatedAppIdentity,
) -> Result<ValidatedBootSelection, HostAppError> {
    let expected = keld_update::ExpectedAppIdentity::from_signed_image(image)
        .map_err(|source| installed_package_error(&source))?;
    let active = keld_update::select_active_package_for_executable(
        keld_update::WindowsLocatedImage::Host,
        locator,
        image,
        &expected,
        &identity.publisher_scope,
        &identity.app_id,
    )
    .map_err(|source| installed_package_error(&source))?;
    let root = active.tree_root().to_path_buf();
    validate_windows_boot_files(
        root,
        WindowsBootMode::Installed {
            identity,
            active: Box::new(active),
        },
    )
}

/// Reports a KEL-53 installed-package refusal under its own `KELD-UPDATE-*` code.
///
/// The refusal's text already opens with that code and closes with its own
/// correction, so the host error carries the code once, as its prefix, and drops
/// the closing period its own sentence break supplies.
#[cfg(windows)]
fn installed_package_error(source: &keld_update::UpdateError) -> HostAppError {
    let code = source.code();
    let rendered = source.to_string();
    let message = rendered
        .strip_prefix(code)
        .and_then(|rest| rest.strip_prefix(": "))
        .unwrap_or(&rendered);
    let message = message.strip_suffix('.').unwrap_or(message);
    HostAppError::new(
        code,
        "Windows installed package selection",
        message,
        "Then relaunch the installed host.",
    )
}

/// Whether this launch presents the KEL-96 dev lease: the one Windows parser of
/// `KELD_DEV_LEASE`, shared by boot routing and the lease monitor.
#[cfg(windows)]
fn windows_dev_lease_requested() -> Result<bool, HostAppError> {
    let Some(value) = std::env::var_os(DEV_LEASE_ENV) else {
        return Ok(false);
    };
    if value != std::ffi::OsStr::new(DEV_LEASE_STDIN_V1) {
        return Err(app_detail(
            "Windows dev-host lease",
            format!(
                "unsupported {DEV_LEASE_ENV} value `{}`",
                value.to_string_lossy()
            ),
        ));
    }
    Ok(true)
}

/// Selects the `WebView2` profile from the boot route's own result: a dev stage
/// runs only under its live lease with an ephemeral profile, and an installed
/// package runs without one under the persistent profile of the identity KEL-135
/// verified at boot, never a second verification.
#[cfg(windows)]
fn select_windows_profile_mode(
    installed_identity: Option<ValidatedAppIdentity>,
    has_dev_lease: bool,
) -> Result<WindowsProfileMode, HostAppError> {
    match (installed_identity, has_dev_lease) {
        (None, true) => Ok(WindowsProfileMode::EphemeralDev),
        (Some(identity), false) => identity
            .into_profile_selection()
            .map(WindowsProfileMode::Persistent),
        (None, false) => Err(app_detail(
            "Windows boot mode",
            "a dev-stage boot has no live dev lease",
        )),
        (Some(_), true) => Err(app_detail(
            "Windows boot mode",
            "an installed-package boot presented a dev lease",
        )),
    }
}

#[cfg(windows)]
fn current_windows_executable() -> Result<PathBuf, HostAppError> {
    std::env::current_exe().map_err(|source| {
        windows_identity_error(
            format!("the current executable path is unavailable: {source}"),
            "Restore the signed package executable and relaunch it.",
        )
    })
}

/// Verifies `executable` through the single `keld-guard` KEL-135 Authenticode owner
/// (KEL-270 D4) and returns the pinned verified image with the identity it proved.
#[cfg(windows)]
fn verified_windows_image(
    executable: &Path,
) -> Result<(VerifiedWindowsImage, ValidatedAppIdentity), HostAppError> {
    let verified = WindowsAuthenticodeImage::open(executable)
        .and_then(WindowsAuthenticodeImage::verify)
        .map_err(|error| windows_authenticode_error(&error))?;
    let identity = verified.identity();
    let identity =
        ValidatedAppIdentity::from_verified_parts(*identity.publisher_scope(), identity.app_id())?;
    Ok((verified, identity))
}

#[cfg(any(target_os = "linux", windows))]
#[allow(clippy::too_many_lines)] // one shared direct-owner state machine keeps Linux/Windows lifecycle transitions identical
fn run_app_direct(
    boot: AppBootSelection,
    guard_snapshot: Option<&GuardSnapshot>,
) -> Result<(), HostAppError> {
    let shutdown = SessionShutdownState::new();
    let AppBootSelection {
        root,
        name,
        entry_path,
        entry_file,
        renderer_html,
        #[cfg(windows)]
        mode,
    } = boot;
    // Held until this session returns, so the installed version tree it runs from
    // stays pinned through teardown.
    #[cfg(windows)]
    let (installed_identity, _installed_package) = match mode {
        WindowsBootMode::DevStage => (None, None),
        WindowsBootMode::Installed { identity, active } => (Some(identity), Some(active)),
    };
    let html = String::from_utf8(renderer_html).map_err(|source| {
        HostAppError::new(
            "KELD-CORE-036",
            "renderer",
            source.to_string(),
            "Regenerate the stage with UTF-8 renderer HTML.",
        )
    })?;
    drop(entry_file);
    #[cfg(windows)]
    let dev_lease = prepare_windows_dev_lease(&shutdown)?;
    #[cfg(windows)]
    let windows_profile_mode =
        select_windows_profile_mode(installed_identity, dev_lease.is_some())?;
    #[cfg(target_os = "linux")]
    let mut dev_lease = DevHostLease::from_environment()?;
    #[cfg(target_os = "linux")]
    if let Some(lease) = dev_lease.as_mut()
        && lease.poll_lost()?
    {
        return Ok(());
    }
    let prestart_lease_result = || {
        #[cfg(windows)]
        {
            take_direct_prestart_lease_result(dev_lease.as_ref())
        }
        #[cfg(target_os = "linux")]
        {
            Ok(())
        }
    };
    #[cfg(target_os = "linux")]
    let direct_engine = WebKitGtkEngine::new()
        .map_err(|source| app_detail("Linux GPU safe mode", source.to_string()))?;
    #[cfg(target_os = "linux")]
    let config = linux_strict_primary_config(&root, &entry_path)?;
    #[cfg(windows)]
    let config = PrimaryRoleConfig::new("bun")
        .arg("run")
        .arg(root.join(&entry_path))
        .current_dir(&root)
        .env_remove(DEV_LEASE_ENV)
        .env_remove(keld_runtime::windows_job::WINDOWS_LAUNCH_GATE_ENV);
    #[cfg(all(debug_assertions, windows))]
    let config = if std::env::var_os("KELD_TEST_WINDOWS_LEASE_CENSUS").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        let handle = dev_lease.as_ref().ok_or_else(|| {
            app_detail(
                "Windows dev-host lease census",
                "test census requested without a live dev lease",
            )
        })?;
        config
            .env(
                "KELD_TEST_WINDOWS_HOST_LEASE_HANDLE",
                format!("{:x}", handle.raw_handle_value),
            )
            .env_remove("KELD_TEST_WINDOWS_LEASE_CENSUS")
    } else {
        config
    };
    LISTENER_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    CHILD_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    let supervisor = run_direct_startup_if_session_running(&shutdown, || {
        PrimaryRoleSupervisor::start_with_bound_generations_gated(config)
            .map_err(|source| app_runtime("direct primary startup", &source))
    });
    let Some(supervisor) = supervisor else {
        return prestart_lease_result();
    };
    let (supervisor, recovery) = supervisor?;
    let initial = match await_direct_bound_generation(
        &supervisor,
        &recovery,
        &shutdown,
        Instant::now() + APP_LINK_IO_DEADLINE,
    ) {
        Ok(bound) => bound,
        Err(primary) => {
            if !shutdown.is_running() {
                let _ = recovery.deny();
                supervisor.shutdown();
                let cleanup = match supervisor.wait_for_outcome() {
                    keld_runtime::SupervisorOutcome::Stopped => Ok(()),
                    keld_runtime::SupervisorOutcome::CrashLoop(error)
                    | keld_runtime::SupervisorOutcome::Failed(error) => {
                        Err(app_runtime("direct primary startup cleanup", &error))
                    }
                };
                return collapse_app_results([prestart_lease_result(), cleanup]);
            }
            let output = supervisor.output();
            let primary = app_detail(
                "initial direct primary startup",
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
                    Err(app_runtime("direct primary startup cleanup", &error))
                }
            };
            return Err(collapse_app_failures(&primary, [cleanup]));
        }
    };

    if !shutdown.is_running() {
        let _ = recovery.deny();
        supervisor.shutdown();
        let cleanup = match supervisor.wait_for_outcome() {
            keld_runtime::SupervisorOutcome::Stopped => Ok(()),
            keld_runtime::SupervisorOutcome::CrashLoop(error)
            | keld_runtime::SupervisorOutcome::Failed(error) => {
                Err(app_runtime("direct primary startup cleanup", &error))
            }
        };
        return collapse_app_results([prestart_lease_result(), cleanup]);
    }

    let (window_commands_tx, window_commands_rx) = mpsc::channel();
    let primary_owner = DirectPrimaryOwner::start(
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
        guard_snapshot.map(GuardSnapshot::fs_weak),
    )?;
    primary_owner.attach_router(router.handle())?;
    let lease_errors = start_direct_dev_lease_tail(dev_lease, router.handle())?;
    let (window_events_tx, window_events_rx) = mpsc::channel();
    let router_handle = router.handle();
    let commands_for_events = window_commands_tx.clone();
    let event_coordinator = thread::Builder::new()
        .name("keld-core-direct-app-window-events".to_owned())
        .spawn(move || {
            coordinate_window_events(&window_events_rx, &router_handle, &commands_for_events)
        })
        .map_err(|source| app_io("direct window event coordinator", &source))?;

    let spec = WebviewSpec {
        title: name,
        initial: NavTarget::Html(html),
        ..WebviewSpec::default()
    };
    let engine = run_direct_startup_if_session_running(&shutdown, || {
        #[cfg(windows)]
        let mut direct_engine = match windows_profile_mode {
            WindowsProfileMode::Persistent(selection) => WebView2Engine::new(selection),
            WindowsProfileMode::EphemeralDev => WebView2Engine::new_dev_ephemeral(),
        }
        .map_err(|source| app_detail("Windows WebView2 initialization", source.to_string()))?;
        #[cfg(target_os = "linux")]
        let mut direct_engine = direct_engine;
        WINDOW_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
        direct_engine
            .create_app(&spec, window_events_tx.clone())
            .map_err(|source| app_detail("initial direct window", source.to_string()))?;
        Ok(direct_engine)
    });
    let Some(engine) = engine else {
        drop(window_events_tx);
        let event_result = event_coordinator
            .join()
            .map_err(|_| app_detail("direct window event coordinator", "thread panicked"))
            .and_then(std::convert::identity);
        let lease_result = take_direct_lease_error(lease_errors.as_ref());
        let router_result = router.shutdown();
        let owner_result = primary_owner.shutdown();
        let _retained_digest = guard_snapshot.map(|snapshot| snapshot.verified.verified_sha256());
        return collapse_app_results([lease_result, event_result, router_result, owner_result]);
    };
    let engine = match engine {
        Ok(engine) => engine,
        Err(primary) => {
            drop(window_events_tx);
            let event_result = event_coordinator
                .join()
                .map_err(|_| app_detail("direct window event coordinator", "thread panicked"))
                .and_then(std::convert::identity);
            let lease_result = take_direct_lease_error(lease_errors.as_ref());
            let router_result = router.shutdown();
            let owner_result = primary_owner.shutdown();
            let _retained_digest =
                guard_snapshot.map(|snapshot| snapshot.verified.verified_sha256());
            return Err(collapse_app_failures(
                &primary,
                [lease_result, event_result, router_result, owner_result],
            ));
        }
    };
    let window_result = engine.run_app_until_quit(window_commands_rx, window_events_tx);
    drop(window_commands_tx);
    let event_result = event_coordinator
        .join()
        .map_err(|_| app_detail("direct window event coordinator", "thread panicked"))
        .and_then(std::convert::identity);
    let lease_result = take_direct_lease_error(lease_errors.as_ref());
    let router_result = router.shutdown();
    let owner_result = primary_owner.shutdown();
    let _retained_digest = guard_snapshot.map(|snapshot| snapshot.verified.verified_sha256());

    let owner_result = match owner_result {
        Err(primary) if primary.code == "KELD-CORE-033" => {
            return Err(append_app_cleanup(
                primary,
                [
                    lease_result,
                    window_result
                        .map_err(|source| app_detail("direct app window", source.to_string())),
                    event_result,
                    router_result,
                ],
            ));
        }
        result => result,
    };

    match window_result {
        Err(source @ WvError::Navigate(_)) => {
            let primary = app_detail("initial direct navigation", source.to_string());
            Err(collapse_app_failures(
                &primary,
                [lease_result, owner_result, router_result, event_result],
            ))
        }
        result => collapse_app_results([
            lease_result,
            result.map_err(|source| app_detail("direct app window", source.to_string())),
            event_result,
            router_result,
            owner_result,
        ]),
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const LINUX_BUN_ENTRY: &str = "/code/main.ts";
/// Guest path Bun resolves for `from "./kipc-transport.ts"` after the entry
/// remaps to `/code/main.ts`. Directory-wide `/code` mounts stay forbidden.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const LINUX_BUN_TRANSPORT: &str = "/code/kipc-transport.ts";
// Deliberate Ubuntu/Debian x86_64 runtime manifest for the currently proved
// Linux product profile. KEL-28 owns non-Debian evidence; target-driven `ldd`
// execution here would run untrusted loader metadata outside containment.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const LINUX_UBUNTU_X86_RUNTIME: [(&str, &str, bool); 6] = [
    (
        "/usr/lib/x86_64-linux-gnu/libc.so.6",
        "/usr/lib/x86_64-linux-gnu/libc.so.6",
        true,
    ),
    (
        "/usr/lib/x86_64-linux-gnu/libpthread.so.0",
        "/usr/lib/x86_64-linux-gnu/libpthread.so.0",
        false,
    ),
    (
        "/usr/lib/x86_64-linux-gnu/libdl.so.2",
        "/usr/lib/x86_64-linux-gnu/libdl.so.2",
        false,
    ),
    (
        "/usr/lib/x86_64-linux-gnu/libm.so.6",
        "/usr/lib/x86_64-linux-gnu/libm.so.6",
        false,
    ),
    (
        "/usr/lib/x86_64-linux-gnu/libgcc_s.so.1",
        "/usr/lib/x86_64-linux-gnu/libgcc_s.so.1",
        true,
    ),
    (
        "/usr/lib64/ld-linux-x86-64.so.2",
        "/lib64/ld-linux-x86-64.so.2",
        true,
    ),
];

#[cfg(target_os = "linux")]
fn linux_strict_primary_config(
    root: &Path,
    entry_path: &Path,
) -> Result<PrimaryRoleConfig, HostAppError> {
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (root, entry_path);
        return Err(app_detail(
            "Linux strict runtime architecture",
            "the proved Ubuntu/Debian strict runtime manifest supports x86_64 only",
        ));
    }
    #[cfg(target_arch = "x86_64")]
    {
        linux_strict_primary_config_x86(root, entry_path)
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn linux_strict_primary_config_x86(
    root: &Path,
    entry_path: &Path,
) -> Result<PrimaryRoleConfig, HostAppError> {
    let bun = resolve_linux_bun()?;
    let launcher = root.join("keld-role-launcher");
    let role_root = root.join(".keld-primary");
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    builder
        .create(&role_root)
        .map_err(|source| app_io("strict role root", &source))?;
    let mut profile = LinuxStrictProfile::new(Path::new("/usr/bin/bwrap"), &launcher, &role_root)
        .map_err(|source| app_detail("Linux strict profile", source.to_string()))?;
    for (source, destination, required) in LINUX_UBUNTU_X86_RUNTIME {
        if !required && !Path::new(source).exists() {
            continue;
        }
        profile = profile
            .readonly_runtime(Path::new(source), Path::new(destination))
            .map_err(|source| app_detail("Linux strict runtime", source.to_string()))?;
    }
    profile = profile
        .readonly_runtime(&root.join(entry_path), Path::new(LINUX_BUN_ENTRY))
        .map_err(|source| app_detail("Linux strict entry", source.to_string()))?;
    profile = bind_linux_kipc_transport(profile, root)?;

    let config = PrimaryRoleConfig::new(bun)
        .arg("run")
        .arg(LINUX_BUN_ENTRY)
        .env("HOME", "/app")
        .env("TMPDIR", "/tmp")
        .env_remove(DEV_LEASE_ENV);
    #[cfg(debug_assertions)]
    let config = {
        let mut config = config;
        if let Some(control) = std::env::var_os("KELD_T1B_CONTROL") {
            let control = PathBuf::from(control)
                .canonicalize()
                .map_err(|source| app_io("Linux strict test control", &source))?;
            profile = profile
                .debug_readonly_socket(&control)
                .map_err(|source| app_detail("Linux strict test control", source.to_string()))?;
            config = config.env("KELD_T1B_CONTROL", control);
        }
        if let Some(value) = std::env::var_os("KELD_T2_EXIT_ON_LINK_EOF") {
            config = config.env("KELD_T2_EXIT_ON_LINK_EOF", value);
        }
        config = config.env("KELD_T4_LINUX_STRICT", "1");
        config
    };
    Ok(config.linux_strict(profile))
}

/// Admits the staged hello sidecar before any Linux `--ro-bind`.
/// `Ok(true)` binds this exact path; `Ok(false)` is `NotFound` for
/// self-contained entries. Symlinks fail closed so
/// `LinuxReadonlyMount::new` cannot canonicalize them to an external file.
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), test))]
fn admit_kipc_transport_sidecar(path: &Path) -> Result<bool, HostAppError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(app_detail(
            "Linux strict transport",
            "src/kipc-transport.ts must be a regular file, not a symbolic link",
        )),
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(app_detail(
            "Linux strict transport",
            "src/kipc-transport.ts exists but is not a regular file",
        )),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(app_io("Linux strict transport", &err)),
    }
}

/// Binds the staged hello sidecar as its own file mount. `NotFound` stays
/// valid for fixtures that inline a self-contained entry. A present non-file
/// or symlink fails closed — `readonly_runtime` rejects directory-wide `/code`
/// mounts and must not follow a sidecar symlink into `--ro-bind`.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn bind_linux_kipc_transport(
    profile: LinuxStrictProfile,
    root: &Path,
) -> Result<LinuxStrictProfile, HostAppError> {
    // Two-component probe matches boot.rs: a single `src/...` component then
    // `.is_file()` can miss the sidecar on Windows; keep the same join here.
    let sidecar = root.join("src").join("kipc-transport.ts");
    if !admit_kipc_transport_sidecar(&sidecar)? {
        return Ok(profile);
    }
    profile
        .readonly_runtime(&sidecar, Path::new(LINUX_BUN_TRANSPORT))
        .map_err(|source| app_detail("Linux strict transport", source.to_string()))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn resolve_linux_bun() -> Result<PathBuf, HostAppError> {
    let path = std::env::var_os("PATH")
        .ok_or_else(|| app_detail("Linux Bun resolution", "PATH is unset"))?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join("bun");
        let Ok(metadata) = fs::metadata(&candidate) else {
            continue;
        };
        if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
            return candidate
                .canonicalize()
                .map_err(|source| app_io("Linux Bun resolution", &source));
        }
    }
    Err(app_detail(
        "Linux Bun resolution",
        "no executable `bun` exists on PATH",
    ))
}

#[cfg(any(target_os = "linux", windows))]
fn run_direct_startup_if_session_running<T>(
    shutdown: &SessionShutdownState,
    startup: impl FnOnce() -> T,
) -> Option<T> {
    let _transition = shutdown.transition_guard();
    if !shutdown.is_running() {
        return None;
    }
    Some(startup())
}

#[cfg(windows)]
enum WindowsDevLeaseObservation {
    Eof,
    ReadFailed(HostAppError),
}

#[cfg(windows)]
struct WindowsDevLeaseMonitor {
    observations: Receiver<WindowsDevLeaseObservation>,
    #[cfg(debug_assertions)]
    raw_handle_value: usize,
}

#[cfg(windows)]
fn prepare_windows_dev_lease(
    shutdown: &SessionShutdownState,
) -> Result<Option<WindowsDevLeaseMonitor>, HostAppError> {
    use std::os::windows::io::AsRawHandle as _;

    if !windows_dev_lease_requested()? {
        return Ok(None);
    }
    let input = io::stdin();
    let handle = input.as_raw_handle().cast();
    clear_windows_handle_inheritance(handle)
        .map_err(|source| app_io("Windows dev-host lease isolation", &source))?;
    let (observation_tx, observation_rx) = mpsc::channel();
    if !windows_lease_pipe_is_live(handle.addr())
        .map_err(|source| app_io("Windows dev-host lease preflight", &source))?
    {
        publish_windows_lease_observation(
            shutdown,
            &observation_tx,
            WindowsDevLeaseObservation::Eof,
        );
        return Ok(Some(WindowsDevLeaseMonitor {
            observations: observation_rx,
            #[cfg(debug_assertions)]
            raw_handle_value: handle.addr(),
        }));
    }
    let shutdown = shutdown.clone();
    let _handle = thread::Builder::new()
        .name("keld-core-windows-dev-lease-reader".to_owned())
        .spawn(move || {
            let mut input = input.lock();
            let mut buffer = [0_u8; 8 * 1024];
            loop {
                match input.read(&mut buffer) {
                    Ok(0) => {
                        publish_windows_lease_observation(
                            &shutdown,
                            &observation_tx,
                            WindowsDevLeaseObservation::Eof,
                        );
                        return;
                    }
                    Ok(_) => {}
                    Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
                    Err(source) => {
                        publish_windows_lease_observation(
                            &shutdown,
                            &observation_tx,
                            WindowsDevLeaseObservation::ReadFailed(app_io(
                                "Windows dev-host lease read",
                                &source,
                            )),
                        );
                        return;
                    }
                }
            }
        })
        .map_err(|source| app_io("Windows dev-host lease reader", &source))?;
    Ok(Some(WindowsDevLeaseMonitor {
        observations: observation_rx,
        #[cfg(debug_assertions)]
        raw_handle_value: handle.addr(),
    }))
}

#[cfg(windows)]
#[allow(unsafe_code)] // read-only state query on the borrowed stdin pipe handle
fn windows_lease_pipe_is_live(handle: usize) -> io::Result<bool> {
    let handle = std::ptr::with_exposed_provenance_mut(handle);
    // SAFETY: the handle value comes from the live process stdin handle, which
    // remains owned by `io::stdin` for the process lifetime. Null optional
    // outputs make this a state-only query and no pointer is retained.
    if unsafe {
        PeekNamedPipe(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } != 0
    {
        return Ok(true);
    }
    let source = io::Error::last_os_error();
    if source.kind() == io::ErrorKind::BrokenPipe {
        Ok(false)
    } else {
        Err(source)
    }
}

#[cfg(windows)]
fn publish_windows_lease_observation(
    shutdown: &SessionShutdownState,
    observations: &Sender<WindowsDevLeaseObservation>,
    observation: WindowsDevLeaseObservation,
) {
    let _transition = shutdown.transition_guard();
    if shutdown.claim_guarded(SESSION_CLI_LEASE_LOST) {
        let _ = observations.send(observation);
    }
}

#[cfg(windows)]
#[allow(unsafe_code)] // one reviewed Win32 flag mutation on a live borrowed standard-input handle
fn clear_windows_handle_inheritance(handle: HANDLE) -> io::Result<()> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "stdin-v1 has no valid standard-input handle",
        ));
    }
    // SAFETY: `handle` is borrowed from the live process standard-input
    // object. SetHandleInformation neither closes nor retains it; the mask
    // changes only HANDLE_FLAG_INHERIT and zero clears that flag.
    if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn start_direct_dev_lease_tail(
    lease: Option<DevHostLease>,
    router: PrimaryRouterHandle,
) -> Result<Option<Receiver<HostAppError>>, HostAppError> {
    let Some(mut lease) = lease else {
        return Ok(None);
    };
    let (error_tx, error_rx) = mpsc::channel();
    thread::Builder::new()
        .name("keld-core-linux-dev-lease".to_owned())
        .spawn(move || {
            loop {
                match lease.poll_lost() {
                    Ok(false) => thread::park_timeout(Duration::from_millis(10)),
                    Ok(true) => {
                        if router.shutdown.claim_cli_lease_lost()
                            && let Err(error) = router.cli_lease_lost()
                        {
                            let _ = error_tx.send(error);
                            let _ = router.window_commands.send(AppWindowCommand::Fatal);
                        }
                        return;
                    }
                    Err(primary) => {
                        if !router.shutdown.claim_cli_lease_lost() {
                            return;
                        }
                        let error = match router.cli_lease_lost() {
                            Ok(()) => primary,
                            Err(cleanup) => collapse_app_failures(&primary, [Err(cleanup)]),
                        };
                        let _ = error_tx.send(error);
                        let _ = router.window_commands.send(AppWindowCommand::Fatal);
                        return;
                    }
                }
            }
        })
        .map_err(|source| app_io("Linux dev-host lease owner", &source))?;
    Ok(Some(error_rx))
}

#[cfg(target_os = "linux")]
fn take_direct_lease_error(errors: Option<&Receiver<HostAppError>>) -> Result<(), HostAppError> {
    errors
        .and_then(|errors| errors.try_recv().ok())
        .map_or(Ok(()), Err)
}

#[cfg(windows)]
fn start_direct_dev_lease_tail(
    monitor: Option<WindowsDevLeaseMonitor>,
    router: PrimaryRouterHandle,
) -> Result<Option<Receiver<HostAppError>>, HostAppError> {
    let Some(monitor) = monitor else {
        return Ok(None);
    };
    let (error_tx, error_rx) = mpsc::channel();
    let _handle = thread::Builder::new()
        .name("keld-core-windows-dev-lease-tail".to_owned())
        .spawn(move || {
            let Ok(observation) = monitor.observations.recv() else {
                return;
            };
            match observation {
                WindowsDevLeaseObservation::Eof => publish_windows_lease_result(
                    router.cli_lease_lost(),
                    &error_tx,
                    &router.window_commands,
                ),
                WindowsDevLeaseObservation::ReadFailed(primary) => {
                    let tail = router.cli_lease_lost();
                    let error = match tail {
                        Ok(()) => primary,
                        Err(cleanup) => collapse_app_failures(&primary, [Err(cleanup)]),
                    };
                    let _ = error_tx.send(error);
                    let _ = router.window_commands.send(AppWindowCommand::Fatal);
                }
            }
        })
        .map_err(|source| app_io("Windows dev-host lease tail", &source))?;
    Ok(Some(error_rx))
}

#[cfg(windows)]
fn publish_windows_lease_result(
    result: Result<(), HostAppError>,
    error_tx: &Sender<HostAppError>,
    window_commands: &Sender<AppWindowCommand>,
) {
    if let Err(error) = result {
        let _ = error_tx.send(error);
        let _ = window_commands.send(AppWindowCommand::Fatal);
    }
}

#[cfg(windows)]
fn take_direct_lease_error(errors: Option<&Receiver<HostAppError>>) -> Result<(), HostAppError> {
    errors
        .and_then(|errors| errors.try_recv().ok())
        .map_or(Ok(()), Err)
}

#[cfg(windows)]
fn take_direct_prestart_lease_result(
    monitor: Option<&WindowsDevLeaseMonitor>,
) -> Result<(), HostAppError> {
    match monitor.and_then(|monitor| monitor.observations.try_recv().ok()) {
        Some(WindowsDevLeaseObservation::ReadFailed(error)) => Err(error),
        Some(WindowsDevLeaseObservation::Eof) | None => Ok(()),
    }
}

#[cfg(any(target_os = "linux", windows))]
fn await_direct_bound_generation(
    supervisor: &PrimaryRoleSupervisor,
    recovery: &PrimaryRecoveryGate,
    shutdown: &SessionShutdownState,
    deadline: Instant,
) -> Result<BoundPrimaryGeneration, HostAppError> {
    loop {
        if !shutdown.is_running() {
            let _ = recovery.deny();
            return Err(app_detail(
                "initial direct app-link authentication",
                "startup was cancelled by CLI lease loss",
            ));
        }
        if let Some(bound) = supervisor.try_recv_bound_generation() {
            return Ok(bound);
        }
        while let Some(event) = supervisor.try_recv_event() {
            if matches!(event, PrimaryRoleEvent::Revoked { .. }) {
                let _ = recovery.deny();
                return Err(app_detail(
                    "initial direct app-link authentication",
                    "Bun terminated before its initial authenticated generation bound",
                ));
            }
        }
        if let Some(outcome) = supervisor.try_wait_for_outcome() {
            let _ = recovery.deny();
            return Err(match outcome {
                keld_runtime::SupervisorOutcome::Stopped => app_detail(
                    "initial direct app-link authentication",
                    "Bun stopped before its initial authenticated generation bound",
                ),
                keld_runtime::SupervisorOutcome::CrashLoop(error)
                | keld_runtime::SupervisorOutcome::Failed(error) => {
                    app_runtime("initial direct app-link authentication", &error)
                }
            });
        }
        if Instant::now() >= deadline {
            let _ = recovery.deny();
            return Err(app_detail(
                "initial direct app-link authentication",
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
fn app_webview_error(phase: &'static str, source: &WvError) -> HostAppError {
    if matches!(source, WvError::ProfileSelection(_)) {
        HostAppError::new(
            "KELD-WV-009",
            phase,
            source.to_string(),
            "Restore the validated app identity and supported macOS profile state, then relaunch.",
        )
    } else {
        app_detail(phase, source.to_string())
    }
}

#[cfg(target_os = "macos")]
struct RendererDispatch {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<Result<(), HostAppError>>,
}

#[cfg(target_os = "macos")]
fn start_renderer_dispatch(
    requests: Receiver<RendererBridgeRequest>,
    outcomes: Sender<RendererBridgeOutcome>,
    router: PrimaryRouterHandle,
) -> Result<RendererDispatch, HostAppError> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = Arc::clone(&stop);
    let handle = thread::Builder::new()
        .name("keld-core-renderer-dispatch".to_owned())
        .spawn(move || {
            loop {
                let request = match requests.recv_timeout(APP_LINK_READER_POLL) {
                    Ok(request) => request,
                    Err(RecvTimeoutError::Timeout) if stop_for_thread.load(Ordering::Acquire) => {
                        return Ok(());
                    }
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => return Ok(()),
                };
                let webview = request.webview();
                let navigation = request.navigation();
                let local_request = request.request();
                if request.channel() != ECHO_CHANNEL.0 {
                    if outcomes
                        .send(RendererBridgeOutcome::Error {
                            webview,
                            navigation,
                            request: local_request,
                            code: String::from("KELD-WV-011"),
                            detail: String::from("renderer channel is not declared by this build"),
                        })
                        .is_err()
                    {
                        return Ok(());
                    }
                    continue;
                }

                let Ok(call) = router.begin_echo_call(request.payload()) else {
                    if outcomes
                        .send(RendererBridgeOutcome::Error {
                            webview,
                            navigation,
                            request: local_request,
                            code: String::from("KELD-WV-011"),
                            detail: String::from(
                                "application call is unavailable or already pending",
                            ),
                        })
                        .is_err()
                    {
                        return Ok(());
                    }
                    continue;
                };
                if std::env::var_os("KELD_KEL142_ACCEPTANCE_REPORT").is_some() {
                    eprintln!(
                        "KELD_KEL142_KIPC_CALL webview={} navigation={} request={} corr={}",
                        webview.0, navigation, local_request, call.correlation.0
                    );
                }

                let reply = loop {
                    match call.reply.recv_timeout(APP_LINK_READER_POLL) {
                        Ok(reply) => break Some(reply),
                        Err(RecvTimeoutError::Timeout)
                            if stop_for_thread.load(Ordering::Acquire) =>
                        {
                            return Ok(());
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break None,
                    }
                };
                let outcome = match reply {
                    Some(Ok(PrimaryEchoReply::Reply(payload))) => RendererBridgeOutcome::Reply {
                        webview,
                        navigation,
                        request: local_request,
                        payload,
                    },
                    Some(Ok(PrimaryEchoReply::Err(error))) => RendererBridgeOutcome::Error {
                        webview,
                        navigation,
                        request: local_request,
                        code: error.code,
                        detail: error.message,
                    },
                    Some(Err(_)) | None => RendererBridgeOutcome::Error {
                        webview,
                        navigation,
                        request: local_request,
                        code: String::from("KELD-WV-011"),
                        detail: String::from("application link ended before the renderer reply"),
                    },
                };
                if outcomes.send(outcome).is_err() {
                    return Ok(());
                }
            }
        })
        .map_err(|source| app_io("renderer dispatch thread", &source))?;
    Ok(RendererDispatch { stop, handle })
}

#[cfg(target_os = "macos")]
fn join_renderer_dispatch(dispatch: RendererDispatch) -> Result<(), HostAppError> {
    dispatch.stop.store(true, Ordering::Release);
    dispatch
        .handle
        .join()
        .map_err(|_| app_detail("renderer dispatch thread", "thread panicked"))?
}

#[cfg(target_os = "macos")]
fn cleanup_window_start_failure(
    primary: &HostAppError,
    guard_snapshot: Option<&GuardSnapshot>,
    window_events: Sender<AppWindowEvent>,
    event_coordinator: JoinHandle<Result<(), HostAppError>>,
    renderer_dispatch: Option<RendererDispatch>,
    router: PrimaryRouter,
    guardian_owner: GuardianOwner,
) -> Result<(), HostAppError> {
    finish_guarded_session(guard_snapshot, |_| {
        drop(window_events);
        let event_result = event_coordinator
            .join()
            .map_err(|_| app_detail("window event coordinator", "thread panicked"))
            .and_then(std::convert::identity);
        let router_result = router.shutdown();
        let renderer_result = renderer_dispatch.map_or(Ok(()), join_renderer_dispatch);
        let guardian_result = guardian_owner.shutdown();
        Err(collapse_app_failures(
            primary,
            [
                event_result,
                router_result,
                renderer_result,
                guardian_result,
            ],
        ))
    })
}

#[cfg(target_os = "macos")]
fn await_bound_generation(
    guardian: &mut GuardedPrimary,
    mut dev_lease: Option<&mut DevHostLease>,
    deadline: Instant,
    phase: &'static str,
) -> Result<Option<BoundPrimaryGeneration>, HostAppError> {
    loop {
        if let Some(lease) = dev_lease.as_deref_mut()
            && lease.poll_lost()?
        {
            guardian.deny_recovery();
            guardian
                .accept_shutdown()
                .map_err(|source| app_runtime("accepted startup cancellation", &source))?;
            guardian
                .shutdown()
                .map_err(|source| app_guardian_fatal("accepted startup cancellation", &source))?;
            return Ok(None);
        }
        let now = Instant::now();
        if now >= deadline {
            guardian.deny_recovery();
            return Err(app_detail(
                phase,
                "Bun did not authenticate before the generation deadline",
            ));
        }
        if let Some(update) = guardian.recv_update(
            deadline
                .saturating_duration_since(now)
                .min(APP_LINK_READER_POLL),
        ) {
            match update {
                GuardedPrimaryUpdate::Bound(bound) => return Ok(Some(bound)),
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
    FailRetiredGeneration(u32, std::sync::mpsc::SyncSender<Result<(), String>>),
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
                            let result = match guardian.poll_fatal() {
                                Err(source) => Err(app_guardian_fatal(
                                    "guardian unexpected primary exit before accepted shutdown",
                                    &source,
                                )),
                                Ok(()) => guardian.accept_shutdown().map_err(|source| {
                                    app_runtime("guardian accepted-shutdown preparation", &source)
                                }),
                            };
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
                        Ok(GuardianOwnerCommand::FailRetiredGeneration(attempt, reply)) => {
                            let result =
                                guardian.fail_current_generation(attempt).map_err(|source| {
                                    app_runtime("filesystem reply recovery", &source)
                                });
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
                            let primary = guardian.poll_fatal().err().map(|source| {
                                app_guardian_fatal("guardian unexpected primary exit", &source)
                            });
                            let shutdown = guardian
                                .shutdown()
                                .map_err(|source| app_guardian_fatal("guardian shutdown", &source));
                            let result = match primary {
                                Some(primary) => {
                                    Err(append_app_cleanup(primary, [shutdown.map(|_| ())]))
                                }
                                None => shutdown,
                            };
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
                                    return Err(error.into());
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

    fn request_recovery_arm(&self) -> Result<Receiver<Result<(), String>>, HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(GuardianOwnerCommand::ArmRecovery(reply_tx))
            .map_err(|_| app_detail("primary recovery arm", "guardian owner stopped"))?;
        Ok(reply_rx)
    }

    fn await_recovery_arm(reply_rx: &Receiver<Result<(), String>>) -> Result<(), HostAppError> {
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
    fn fail_retired_generation(&self, attempt: u32) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(GuardianOwnerCommand::FailRetiredGeneration(
                attempt, reply_tx,
            ))
            .map_err(|_| app_detail("filesystem reply recovery", "guardian owner stopped"))?;
        match reply_rx.recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(detail)) => Err(app_detail("filesystem reply recovery", detail)),
            Err(RecvTimeoutError::Timeout) => Err(app_detail(
                "filesystem reply recovery",
                "guardian owner did not acknowledge retired generation",
            )),
            Err(RecvTimeoutError::Disconnected) => Err(app_detail(
                "filesystem reply recovery",
                "guardian owner ended before acknowledging retired generation",
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

#[cfg(any(target_os = "linux", windows))]
const DIRECT_PRIMARY_OWNER_POLL: Duration = Duration::from_millis(10);
#[cfg(any(target_os = "linux", windows))]
const DIRECT_PRIMARY_OWNER_REPLY_DEADLINE: Duration = Duration::from_secs(6);

#[cfg(any(target_os = "linux", windows))]
struct DirectPrimaryOwnerHandle {
    command_tx: Sender<DirectPrimaryOwnerCommand>,
}

#[cfg(any(target_os = "linux", windows))]
impl Clone for DirectPrimaryOwnerHandle {
    fn clone(&self) -> Self {
        Self {
            command_tx: self.command_tx.clone(),
        }
    }
}

#[cfg(any(target_os = "linux", windows))]
enum DirectPrimaryOwnerCommand {
    AttachRouter(PrimaryRouterHandle, mpsc::SyncSender<Result<(), String>>),
    ArmRecovery(mpsc::SyncSender<Result<(), String>>),
    DenyRecovery,
    FailGeneration(u32, mpsc::SyncSender<Result<(), String>>),
    FailRetiredGeneration(u32, mpsc::SyncSender<Result<(), String>>),
    PrepareAcceptedShutdown(mpsc::SyncSender<Result<(), String>>),
    Shutdown(mpsc::SyncSender<Result<(), String>>),
}

#[cfg(any(target_os = "linux", windows))]
struct DirectPrimaryOwner {
    command_tx: Sender<DirectPrimaryOwnerCommand>,
    handle: Option<JoinHandle<Result<(), HostAppError>>>,
}

#[cfg(any(target_os = "linux", windows))]
impl DirectPrimaryOwner {
    #[allow(clippy::too_many_lines)] // one owner loop keeps event/bound fan-in, recovery decisions, shutdown and terminal outcome causally ordered
    fn start(
        supervisor: PrimaryRoleSupervisor,
        recovery: PrimaryRecoveryGate,
        window_commands: Sender<AppWindowCommand>,
        shutdown: SessionShutdownState,
    ) -> Result<Self, HostAppError> {
        let (command_tx, command_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("keld-core-direct-primary-owner".to_owned())
            .spawn(move || {
                let mut router: Option<PrimaryRouterHandle> = None;
                loop {
                    // These are separate channels, so preserve the generation
                    // owner's causal order explicitly at the fan-in: revoke
                    // authority before installing a queued successor stream.
                    if let Err(error) =
                        drain_direct_primary_role_events(&supervisor, router.as_ref())
                    {
                        return Err(terminate_direct_primary_owner(
                            error,
                            &recovery,
                            &supervisor,
                            &window_commands,
                        ));
                    }
                    while let Some(bound) = supervisor.try_recv_bound_generation() {
                        // Bound is sent only after its predecessor's Revoked
                        // event, but the two messages use independent channels.
                        // Re-drain after receiving Bound so a scheduler gap
                        // between the outer drains cannot invert that edge.
                        if let Err(error) =
                            drain_direct_primary_role_events(&supervisor, router.as_ref())
                        {
                            return Err(terminate_direct_primary_owner(
                                error,
                                &recovery,
                                &supervisor,
                                &window_commands,
                            ));
                        }
                        let Some(router) = router.as_ref() else {
                            let error = app_detail(
                                "direct primary owner",
                                "successor bound before the app router was attached",
                            );
                            return Err(terminate_direct_primary_owner(
                                error,
                                &recovery,
                                &supervisor,
                                &window_commands,
                            ));
                        };
                        let attempt = bound.attempt();
                        if let Err(error) =
                            router.apply_generation_update(PrimaryOwnerUpdate::Bound(bound))
                        {
                            if restart_failed_bound_generation(router, attempt, &error, |attempt| {
                                supervisor.restart_generation(attempt);
                            }) {
                                continue;
                            }
                            let _ = recovery.deny();
                            supervisor.shutdown();
                            let _ = window_commands.send(AppWindowCommand::Fatal);
                            return Err(error.into());
                        }
                    }
                    if let Some(outcome) = supervisor.try_wait_for_outcome() {
                        let session_running = shutdown.is_running();
                        if session_running {
                            let _ = window_commands.send(AppWindowCommand::Fatal);
                        }
                        forward_direct_primary_output(&supervisor)?;
                        return match outcome {
                            keld_runtime::SupervisorOutcome::Stopped if session_running => {
                                let ledger = supervisor.crash_ledger();
                                ledger.last_self_termination.map_or_else(
                                    || {
                                        Err(app_detail(
                                            "primary self-termination",
                                            "the primary process stopped without a retained termination record",
                                        ))
                                    },
                                    |record| Err(app_self_termination(record)),
                                )
                            }
                            keld_runtime::SupervisorOutcome::Stopped => Ok(()),
                            keld_runtime::SupervisorOutcome::CrashLoop(error)
                            | keld_runtime::SupervisorOutcome::Failed(error) => {
                                Err(app_runtime_fatal("primary supervisor", &error))
                            }
                        };
                    }
                    match command_rx.recv_timeout(DIRECT_PRIMARY_OWNER_POLL) {
                        Ok(DirectPrimaryOwnerCommand::AttachRouter(attached, reply)) => {
                            let result = if router.is_some() {
                                Err(String::from("primary router was already attached"))
                            } else {
                                router = Some(attached);
                                Ok(())
                            };
                            let _ = reply.send(result);
                        }
                        Ok(DirectPrimaryOwnerCommand::ArmRecovery(reply)) => {
                            let result = recovery
                                .arm()
                                .then_some(())
                                .ok_or_else(|| String::from("recovery was already denied"));
                            acknowledge_direct_recovery_arm(router.as_ref(), result, |result| {
                                let _ = reply.send(result);
                            });
                        }
                        Ok(DirectPrimaryOwnerCommand::DenyRecovery) => {
                            let _ = recovery.deny();
                        }
                        Ok(DirectPrimaryOwnerCommand::FailGeneration(attempt, reply)) => {
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
                        Ok(DirectPrimaryOwnerCommand::FailRetiredGeneration(
                            attempt,
                            reply,
                        )) => {
                            let result = router.as_ref().map_or_else(
                                || Err(String::from("primary router is unavailable")),
                                |router| {
                                    recover_retired_fs_generation(router, attempt, |attempt| {
                                        supervisor.restart_generation(attempt);
                                    })
                                },
                            );
                            let _ = reply.send(result);
                        }
                        Ok(DirectPrimaryOwnerCommand::PrepareAcceptedShutdown(reply)) => {
                            let _ = recovery.deny();
                            supervisor.accept_shutdown();
                            let _ = reply.send(Ok(()));
                        }
                        Ok(DirectPrimaryOwnerCommand::Shutdown(reply)) => {
                            let _ = recovery.deny();
                            supervisor.shutdown();
                            let result = match supervisor.wait_for_outcome() {
                                keld_runtime::SupervisorOutcome::Stopped => Ok(()),
                                keld_runtime::SupervisorOutcome::CrashLoop(error)
                                | keld_runtime::SupervisorOutcome::Failed(error) => {
                                    Err(error.to_string())
                                }
                            };
                            let result = result.and_then(|()| {
                                forward_direct_primary_output(&supervisor)
                                    .map_err(|error| error.to_string())
                            });
                            let _ = reply.send(result.clone());
                            return result
                                .map_err(|detail| app_detail("primary shutdown", detail));
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => {
                            let _ = recovery.deny();
                            supervisor.shutdown();
                            let result = match supervisor.wait_for_outcome() {
                                keld_runtime::SupervisorOutcome::Stopped => Ok(()),
                                keld_runtime::SupervisorOutcome::CrashLoop(error)
                                | keld_runtime::SupervisorOutcome::Failed(error) => {
                                    Err(app_runtime("direct primary owner", &error))
                                }
                            };
                            return result.and_then(|()| forward_direct_primary_output(&supervisor));
                        }
                    }
                }
            })
            .map_err(|source| app_io("direct primary owner", &source))?;
        Ok(Self {
            command_tx,
            handle: Some(handle),
        })
    }

    fn handle(&self) -> DirectPrimaryOwnerHandle {
        DirectPrimaryOwnerHandle {
            command_tx: self.command_tx.clone(),
        }
    }

    fn attach_router(&self, router: PrimaryRouterHandle) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(DirectPrimaryOwnerCommand::AttachRouter(router, reply_tx))
            .map_err(|_| app_detail("primary router attachment", "owner stopped"))?;
        receive_direct_owner_reply(&reply_rx, "primary router attachment", false)
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
                .map_err(|_| app_detail("direct primary owner", "thread panicked"))?
        })
    }
}

#[cfg(any(target_os = "linux", windows))]
fn acknowledge_direct_recovery_arm(
    router: Option<&PrimaryRouterHandle>,
    result: Result<(), String>,
    acknowledge: impl FnOnce(Result<(), String>),
) {
    if result.is_ok()
        && let Some(router) = router
    {
        router.recovery_armed.store(true, Ordering::Release);
    }
    acknowledge(result);
}

#[cfg(any(target_os = "linux", windows))]
fn restart_failed_bound_generation(
    router: &PrimaryRouterHandle,
    attempt: u32,
    failure: &PrimaryGenerationFailure,
    restart: impl FnOnce(u32),
) -> bool {
    let _transition = router.shutdown.transition_guard();
    if !router.window_ready.load(Ordering::Acquire)
        || !router.recovery_armed.load(Ordering::Acquire)
        || !router.shutdown.is_running()
        || router.shutdown.reader_stop.load(Ordering::Acquire)
        || attempt <= router.last_revoked_attempt.load(Ordering::Acquire)
    {
        return false;
    }
    let Ok(current) = router.current.lock() else {
        return false;
    };
    let owned = current
        .as_ref()
        .is_some_and(|active| active.attempt == attempt)
        || (failure.retired_after_failed_ready == Some(attempt) && current.is_none());
    drop(current);
    if owned {
        // The runtime revalidates this attempt before stopping/restarting a
        // child. A queued natural revocation/successor cannot grant it authority.
        restart(attempt);
    }
    owned
}

#[cfg(any(target_os = "linux", windows))]
fn recover_retired_fs_generation(
    router: &PrimaryRouterHandle,
    attempt: u32,
    restart: impl FnOnce(u32),
) -> Result<(), String> {
    let _transition = router.shutdown.transition_guard();
    if !router.shutdown.is_running()
        || router.shutdown.reader_stop.load(Ordering::Acquire)
        || attempt <= router.last_revoked_attempt.load(Ordering::Acquire)
    {
        return Ok(());
    }
    let current = router
        .current
        .lock()
        .map_err(|_| String::from("primary generation lock poisoned"))?;
    if let Some(active) = current.as_ref() {
        return if active.attempt > attempt {
            Ok(())
        } else {
            Err(String::from(
                "retired filesystem generation is still current or ownership regressed",
            ))
        };
    }
    drop(current);
    if !router.window_ready.load(Ordering::Acquire)
        || !router.recovery_armed.load(Ordering::Acquire)
    {
        return Err(String::from(
            "retired filesystem generation is not recovery-eligible",
        ));
    }
    let fs = router
        .fs
        .as_ref()
        .and_then(Weak::upgrade)
        .ok_or_else(|| String::from("guarded filesystem session is unavailable"))?;
    if !fs.failed_write_attempt_is(attempt) {
        return Err(String::from(
            "retired filesystem generation lacks exact failed-write evidence",
        ));
    }
    restart(attempt);
    Ok(())
}

#[cfg(any(target_os = "linux", windows))]
fn forward_direct_primary_output(supervisor: &PrimaryRoleSupervisor) -> Result<(), HostAppError> {
    let output = supervisor.output();
    io::stdout()
        .write_all(output.stdout.as_bytes())
        .map_err(|source| app_io("primary stdout forwarding", &source))?;
    if let Some(notice) = keld_runtime::CapturedOutput::elision_notice(output.stdout_dropped_bytes)
    {
        io::stdout()
            .write_all(notice.as_bytes())
            .map_err(|source| app_io("primary stdout forwarding", &source))?;
    }
    io::stderr()
        .write_all(output.stderr.as_bytes())
        .map_err(|source| app_io("primary stderr forwarding", &source))?;
    if let Some(notice) = keld_runtime::CapturedOutput::elision_notice(output.stderr_dropped_bytes)
    {
        io::stderr()
            .write_all(notice.as_bytes())
            .map_err(|source| app_io("primary stderr forwarding", &source))?;
    }
    Ok(())
}

#[cfg(any(target_os = "linux", windows))]
fn drain_direct_primary_role_events(
    supervisor: &PrimaryRoleSupervisor,
    router: Option<&PrimaryRouterHandle>,
) -> Result<(), HostAppError> {
    while let Some(event) = supervisor.try_recv_event() {
        let Some(router) = router else {
            if matches!(event, PrimaryRoleEvent::Revoked { .. }) {
                return Err(app_detail(
                    "primary generation update",
                    "generation changed before the primary router attached",
                ));
            }
            continue;
        };
        router.apply_generation_update(PrimaryOwnerUpdate::Role(event))?;
    }
    Ok(())
}

#[cfg(any(target_os = "linux", windows))]
fn terminate_direct_primary_owner(
    error: HostAppError,
    recovery: &PrimaryRecoveryGate,
    supervisor: &PrimaryRoleSupervisor,
    window_commands: &Sender<AppWindowCommand>,
) -> HostAppError {
    let _ = recovery.deny();
    supervisor.shutdown();
    let _ = window_commands.send(AppWindowCommand::Fatal);
    error
}

#[cfg(any(target_os = "linux", windows))]
impl Drop for DirectPrimaryOwner {
    fn drop(&mut self) {
        if self.handle.is_none() {
            return;
        }
        let _ = self.handle().shutdown_and_wait();
        let _ = self.join();
    }
}

#[cfg(any(target_os = "linux", windows))]
impl DirectPrimaryOwnerHandle {
    fn deny_recovery(&self) {
        let _ = self
            .command_tx
            .send(DirectPrimaryOwnerCommand::DenyRecovery);
    }

    fn request_recovery_arm(&self) -> Result<Receiver<Result<(), String>>, HostAppError> {
        self.enqueue_request(
            DirectPrimaryOwnerCommand::ArmRecovery,
            "primary recovery arm",
        )
    }

    fn await_recovery_arm(reply_rx: &Receiver<Result<(), String>>) -> Result<(), HostAppError> {
        receive_direct_owner_reply(reply_rx, "primary recovery arm", false)
    }

    fn fail_generation(&self, attempt: u32) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(DirectPrimaryOwnerCommand::FailGeneration(attempt, reply_tx))
            .map_err(|_| app_detail("primary app-link failure", "owner stopped"))?;
        receive_direct_owner_reply(&reply_rx, "primary app-link failure", false)
    }
    fn fail_retired_generation(&self, attempt: u32) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(DirectPrimaryOwnerCommand::FailRetiredGeneration(
                attempt, reply_tx,
            ))
            .map_err(|_| app_detail("filesystem reply recovery", "owner stopped"))?;
        receive_direct_owner_reply(&reply_rx, "filesystem reply recovery", false)
    }

    fn prepare_accepted_shutdown(&self) -> Result<(), HostAppError> {
        self.request(
            DirectPrimaryOwnerCommand::PrepareAcceptedShutdown,
            "accepted-shutdown preparation",
        )
    }

    fn shutdown_and_wait(&self) -> Result<(), HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        if self
            .command_tx
            .send(DirectPrimaryOwnerCommand::Shutdown(reply_tx))
            .is_err()
        {
            return Ok(());
        }
        receive_direct_owner_reply(&reply_rx, "primary shutdown", true)
    }

    fn request(
        &self,
        command: impl FnOnce(mpsc::SyncSender<Result<(), String>>) -> DirectPrimaryOwnerCommand,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        let reply_rx = self.enqueue_request(command, phase)?;
        receive_direct_owner_reply(&reply_rx, phase, false)
    }

    fn enqueue_request(
        &self,
        command: impl FnOnce(mpsc::SyncSender<Result<(), String>>) -> DirectPrimaryOwnerCommand,
        phase: &'static str,
    ) -> Result<Receiver<Result<(), String>>, HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(command(reply_tx))
            .map_err(|_| app_detail(phase, "owner stopped"))?;
        Ok(reply_rx)
    }
}

#[cfg(any(target_os = "linux", windows))]
fn receive_direct_owner_reply(
    reply_rx: &Receiver<Result<(), String>>,
    phase: &'static str,
    owner_exit_is_success: bool,
) -> Result<(), HostAppError> {
    match reply_rx.recv_timeout(DIRECT_PRIMARY_OWNER_REPLY_DEADLINE) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(detail)) => Err(app_detail(phase, detail)),
        Err(RecvTimeoutError::Timeout) => Err(app_detail(
            phase,
            "owner did not acknowledge before deadline",
        )),
        Err(RecvTimeoutError::Disconnected) if owner_exit_is_success => Ok(()),
        Err(RecvTimeoutError::Disconnected) => {
            Err(app_detail(phase, "owner ended before acknowledgment"))
        }
    }
}

// A failed lifecycle write retires its writer before the bound-generation owner
// sees the error. Keep that exact attempt in the error, not in another lifecycle
// flag, so recovery cannot mistake an absent current generation for authority.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[derive(Debug)]
struct PrimaryGenerationFailure {
    error: HostAppError,
    #[cfg(any(target_os = "linux", windows))]
    retired_after_failed_ready: Option<u32>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl From<HostAppError> for PrimaryGenerationFailure {
    fn from(error: HostAppError) -> Self {
        Self {
            error,
            #[cfg(any(target_os = "linux", windows))]
            retired_after_failed_ready: None,
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl From<PrimaryGenerationFailure> for HostAppError {
    fn from(failure: PrimaryGenerationFailure) -> Self {
        failure.error
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
#[cfg(any(target_os = "linux", windows))]
type PlatformPrimaryOwnerHandle = DirectPrimaryOwnerHandle;

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[derive(Clone)]
struct PrimaryRouterHandle {
    current: Arc<Mutex<Option<ActivePrimaryGeneration>>>,
    readers: Arc<Mutex<HashMap<u32, PrimaryReader>>>,
    #[cfg(target_os = "macos")]
    pending_echo: Arc<Mutex<Option<PendingPrimaryEcho>>>,
    #[cfg(target_os = "macos")]
    pending_echo_attempt: Arc<AtomicU32>,
    #[cfg(target_os = "macos")]
    pending_echo_corr: Arc<AtomicU32>,
    #[cfg(target_os = "macos")]
    next_host_corr: Arc<AtomicU32>,
    window_ready: Arc<AtomicBool>,
    last_window_closed: Arc<AtomicBool>,
    recovery_armed: Arc<AtomicBool>,
    last_revoked_attempt: Arc<AtomicU32>,
    shutdown: SessionShutdownState,
    fs: Option<Weak<FsDispatchSession>>,
    fs_worker_commands: Option<SyncSender<FsWorkerCommand>>,
    guardian: PlatformPrimaryOwnerHandle,
    window_commands: Sender<AppWindowCommand>,
    #[cfg(all(test, target_os = "macos"))]
    quit_drain_hooks: Arc<Mutex<QuitDrainTestHooks>>,
}

/// Test hooks for the post-Quit drain: [`PrimaryRouterHandle::stall_next_quit_drain`]
/// and [`PrimaryRouterHandle::observe_next_quit_drain_end`].
#[cfg(all(test, target_os = "macos"))]
#[derive(Default)]
struct QuitDrainTestHooks {
    stall: Option<(SyncSender<()>, Receiver<()>)>,
    end: Option<SyncSender<QuitDrainEnd>>,
}

/// How a post-Quit drain ended (GH-527 §4.9). Every end leads to the same
/// host-initiated close; tests observe which one it was.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuitDrainEnd {
    /// The role ended the link: its EOF, or a reset (the normal end).
    PeerClosed,
    /// No byte for `HOST_ANSWER_BUDGET` (the idle backstop).
    IdleBackstop,
    /// The drain's `APP_LINK_IO_DEADLINE`, or a frame that stalled.
    Deadline,
    /// A frame the session does not admit.
    NotAdmitted,
    /// A `KELD-IPC-024` answer could not be written: the link is lost.
    AnswerLost,
    /// The generation is gone or its lock is poisoned.
    GenerationGone,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
type PrimaryReader = JoinHandle<Result<(), HostAppError>>;

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct ActivePrimaryGeneration {
    attempt: u32,
    writer: BootstrapStream,
    reader_stop: Arc<AtomicBool>,
    /// Correlation id of this generation's accepted-but-unanswered `Quit`
    /// while it waits for its FS drain (GH-527 §4.9). Set and cleared under
    /// the generation lock, so retirement answers it at most once and never
    /// after its real REPLY.
    pending_quit: Option<CorrelationId>,
}

#[cfg(target_os = "macos")]
struct PendingPrimaryEcho {
    attempt: u32,
    correlation: CorrelationId,
    reply: SyncSender<Result<PrimaryEchoReply, HostAppError>>,
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
enum PrimaryEchoReply {
    Reply(Vec<u8>),
    Err(CallError),
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct PrimaryEchoCall {
    correlation: CorrelationId,
    reply: Receiver<Result<PrimaryEchoReply, HostAppError>>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl PrimaryRouterHandle {
    fn signal_ready(&self) -> Result<(), HostAppError> {
        let arm_reply = {
            let _transition = self.shutdown.transition_guard();
            if !self.shutdown.is_running() {
                return Ok(());
            }
            if self.recovery_armed.load(Ordering::Acquire) {
                self.write_event_guarded(LifecycleEvent::Ready)?;
                self.window_ready.store(true, Ordering::Release);
                return Ok(());
            }
            if let Err(error) = self.write_event_guarded(LifecycleEvent::Ready) {
                self.guardian.deny_recovery();
                return Err(error.into());
            }
            // Publish Ready and enqueue its arm under the same transition as
            // accepted shutdown. A later tail cannot overtake this command.
            self.window_ready.store(true, Ordering::Release);
            self.guardian.request_recovery_arm()
        };
        // The owner can process generation updates requiring the transition
        // guard before acknowledging. Never hold that guard across this wait.
        let arm =
            arm_reply.and_then(|reply| PlatformPrimaryOwnerHandle::await_recovery_arm(&reply));
        if let Err(error) = arm {
            self.guardian.deny_recovery();
            return Err(error);
        }
        self.recovery_armed.store(true, Ordering::Release);
        Ok(())
    }

    fn signal_last_window_closed(&self) -> Result<(), HostAppError> {
        let _transition = self.shutdown.transition_guard();
        self.last_window_closed.store(true, Ordering::Release);
        self.write_event_guarded(LifecycleEvent::LastWindowClosed)
            .map_err(Into::into)
    }

    #[cfg(target_os = "macos")]
    fn mint_host_correlation(&self) -> CorrelationId {
        loop {
            let raw = self.next_host_corr.fetch_add(1, Ordering::AcqRel);
            if raw != 0 {
                return CorrelationId(raw);
            }
        }
    }

    #[cfg_attr(not(target_os = "macos"), allow(clippy::unused_self))]
    fn pending_echo_corr_for(&self, attempt: u32) -> Option<CorrelationId> {
        #[cfg(target_os = "macos")]
        {
            let correlation = self.pending_echo_corr.load(Ordering::Acquire);
            if correlation == 0 || self.pending_echo_attempt.load(Ordering::Acquire) != attempt {
                None
            } else {
                Some(CorrelationId(correlation))
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = attempt;
            None
        }
    }

    #[cfg(target_os = "macos")]
    fn begin_echo_call(&self, payload: &[u8]) -> Result<PrimaryEchoCall, HostAppError> {
        let transition = self.shutdown.transition_guard();
        if !self.shutdown.is_running() {
            return Err(app_detail(
                "renderer Echo call",
                "application session is quiescing",
            ));
        }
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("renderer Echo call", "generation lock poisoned"))?;
        let active = current.as_mut().ok_or_else(|| {
            app_detail(
                "renderer Echo call",
                "no admitted primary generation is available",
            )
        })?;
        let attempt = active.attempt;
        let mut pending = self
            .pending_echo
            .lock()
            .map_err(|_| app_detail("renderer Echo call", "pending-call lock poisoned"))?;
        if pending.is_some() {
            return Err(app_detail(
                "renderer Echo call",
                "another application call is already pending",
            ));
        }

        let correlation = self.mint_host_correlation();
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        *pending = Some(PendingPrimaryEcho {
            attempt,
            correlation,
            reply: reply_tx,
        });
        self.pending_echo_attempt.store(attempt, Ordering::Release);
        self.pending_echo_corr
            .store(correlation.0, Ordering::Release);

        if let Err(source) = write_frame(
            &mut active.writer,
            FrameKind::Call,
            0,
            ECHO_CHANNEL,
            correlation,
            payload,
        ) {
            self.pending_echo_corr.store(0, Ordering::Release);
            self.pending_echo_attempt.store(0, Ordering::Release);
            pending.take();
            drop(pending);
            let primary = app_ipc("renderer Echo call write", &source);
            let retirement = self.retire_current_generation_locked(
                &mut current,
                attempt,
                "application call retired after a failed primary write",
                "failed renderer Echo write link close",
            );
            drop(current);
            drop(transition);
            let owner = self.link_failed(attempt);
            return Err(append_app_cleanup(primary, [retirement, owner]));
        }

        Ok(PrimaryEchoCall {
            correlation,
            reply: reply_rx,
        })
    }

    #[cfg(target_os = "macos")]
    fn finish_pending_echo(
        &self,
        attempt: u32,
        correlation: CorrelationId,
        outcome: PrimaryEchoReply,
    ) -> Result<(), HostAppError> {
        let mut pending = self
            .pending_echo
            .lock()
            .map_err(|_| app_detail("renderer Echo reply", "pending-call lock poisoned"))?;
        let Some(waiter) = pending.as_ref() else {
            return Err(app_detail(
                "renderer Echo reply",
                "reply arrived without a pending host call",
            ));
        };
        if waiter.attempt != attempt || waiter.correlation != correlation {
            return Err(app_detail(
                "renderer Echo reply",
                "reply does not match the pending generation/correlation",
            ));
        }
        self.pending_echo_corr.store(0, Ordering::Release);
        self.pending_echo_attempt.store(0, Ordering::Release);
        let Some(waiter) = pending.take() else {
            return Err(app_detail(
                "renderer Echo reply",
                "pending call disappeared while its lock was held",
            ));
        };
        drop(pending);
        let _ = waiter.reply.try_send(Ok(outcome));
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn fail_pending_echo(
        &self,
        attempt: Option<u32>,
        detail: &'static str,
    ) -> Result<(), HostAppError> {
        let mut pending = self
            .pending_echo
            .lock()
            .map_err(|_| app_detail("renderer Echo retirement", "pending-call lock poisoned"))?;
        if pending
            .as_ref()
            .is_none_or(|waiter| attempt.is_some_and(|expected| waiter.attempt != expected))
        {
            return Ok(());
        }
        self.pending_echo_corr.store(0, Ordering::Release);
        self.pending_echo_attempt.store(0, Ordering::Release);
        let Some(waiter) = pending.take() else {
            return Err(app_detail(
                "renderer Echo retirement",
                "pending call disappeared while its lock was held",
            ));
        };
        drop(pending);
        let _ = waiter
            .reply
            .try_send(Err(app_detail("renderer Echo call", detail)));
        Ok(())
    }

    // KEL-142 renderer waiters exist only on macOS; the cross-platform
    // generation owner keeps the same cleanup call sites as intentional no-ops.
    #[cfg_attr(
        not(target_os = "macos"),
        allow(clippy::unused_self, clippy::unnecessary_wraps)
    )]
    fn fail_pending_echo_for_attempt(
        &self,
        attempt: u32,
        detail: &'static str,
    ) -> Result<(), HostAppError> {
        #[cfg(target_os = "macos")]
        {
            self.fail_pending_echo(Some(attempt), detail)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (attempt, detail);
            Ok(())
        }
    }

    #[cfg_attr(
        not(target_os = "macos"),
        allow(clippy::unused_self, clippy::unnecessary_wraps)
    )]
    fn fail_any_pending_echo(&self, detail: &'static str) -> Result<(), HostAppError> {
        #[cfg(target_os = "macos")]
        {
            self.fail_pending_echo(None, detail)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = detail;
            Ok(())
        }
    }

    fn retire_current_generation_locked(
        &self,
        current: &mut Option<ActivePrimaryGeneration>,
        attempt: u32,
        pending_detail: &'static str,
        link_phase: &'static str,
    ) -> Result<(), HostAppError> {
        if current
            .as_ref()
            .is_none_or(|active| active.attempt != attempt)
        {
            return Ok(());
        }
        let Some(active) = current.take() else {
            return Err(app_detail(
                "primary generation retirement",
                "current generation disappeared while its lock was held",
            ));
        };
        active.reader_stop.store(true, Ordering::Release);
        collapse_app_results([
            self.fail_pending_echo_for_attempt(attempt, pending_detail),
            self.fs
                .as_ref()
                .and_then(Weak::upgrade)
                .map_or(Ok(()), |fs| fs.retire_pending_call_for_attempt(attempt)),
            finish_link_shutdown(active.writer.shutdown_app_link(), link_phase),
        ])
    }

    /// Answers `attempt`'s pending role calls on ERR-declaring channels with
    /// `error` before its link closes (GH-527 §4.9, criterion 6): the admitted
    /// FS call and a `Quit` still waiting in its FS drain. Echo calls are
    /// answered synchronously by the reader and are never pending, so echo
    /// keeps KEL-133's REPLY-only rule. The caller holds the generation lock,
    /// which also orders this against an FS handler's own terminal write: a
    /// real reply written first has already cleared its pending entry.
    ///
    /// Delivery is best effort by contract ([`write_best_effort_answers`]):
    /// each write is bounded by [`HOST_ANSWER_BUDGET`], so a peer that stops
    /// reading holds the generation lock for at most one budget, and a link
    /// that cannot carry the ERR is lost. The role then observes the close as
    /// `KELD-IPC-022`. The retirement outcome does not depend on it.
    fn answer_pending_calls_locked(
        &self,
        current: &mut Option<ActivePrimaryGeneration>,
        attempt: u32,
        error: &CallError,
    ) {
        let Some(active) = current.as_mut().filter(|active| active.attempt == attempt) else {
            return;
        };
        let fs_call = self
            .fs
            .as_ref()
            .and_then(Weak::upgrade)
            .and_then(|fs| fs.pending_call_for(attempt));
        let pending = [
            fs_call.map(|call| (FS_CHANNEL, call.correlation)),
            active
                .pending_quit
                .take()
                .map(|corr| (LIFECYCLE_CHANNEL, corr)),
        ];
        write_best_effort_answers(&mut active.writer, pending.into_iter().flatten(), error);
    }

    /// After an accepted `Quit`'s real REPLY and its FS drain, answers each
    /// CALL the peer wrote before it ended the link, on an ERR-declaring
    /// channel, with `KELD-IPC-024`, and runs none of them (GH-527 §4.9,
    /// criterion 7). An echo CALL gets no frame: KEL-133 keeps echo
    /// REPLY-only. The host then closes the link as before, so KEL-139 AC6's
    /// `reply -> quiesce/drain -> close` order holds.
    ///
    /// The drain ends on the peer's EOF; a `WorkerLink` ends its link after
    /// the Quit REPLY (#528 T3). A frame that has already arrived is always
    /// read and answered, however late the host runs. Only a liveness
    /// backstop ends it otherwise: [`HOST_ANSWER_BUDGET`] without a byte,
    /// checked only on an idle poll, for a peer that never ends the link, and
    /// [`APP_LINK_IO_DEADLINE`] in total for one that never stops sending.
    /// Each answer is one [`write_best_effort_answers`] write.
    ///
    /// It cannot fail: every way the drain ends leads to the same
    /// host-initiated close, so the caller's link close and tail always run.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn answer_calls_after_quit(&self, attempt: u32, reader: &mut BootstrapStream) {
        let started = Instant::now();
        let (Some(idle_deadline), Some(hard_deadline)) = (
            started.checked_add(HOST_ANSWER_BUDGET),
            started.checked_add(APP_LINK_IO_DEADLINE),
        ) else {
            return;
        };
        #[cfg(all(test, target_os = "macos"))]
        let idle_deadline = self
            .stall_quit_drain_for_test(started)
            .unwrap_or(idle_deadline);
        let end = self.drain_calls_after_quit(attempt, reader, idle_deadline, hard_deadline);
        #[cfg(all(test, target_os = "macos"))]
        self.report_quit_drain_end(end);
        #[cfg(not(all(test, target_os = "macos")))]
        let _ = end;
    }

    /// The drain loop of [`Self::answer_calls_after_quit`]. Every way it ends
    /// leads to the same host-initiated close, so none is a fault of the
    /// accepted shutdown: a role that closes right after its Quit REPLY ends
    /// it at that EOF, and an answer it can no longer receive (`EPIPE`, a
    /// reset, `NotConnected`) ends it quietly as a lost link.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn drain_calls_after_quit(
        &self,
        attempt: u32,
        reader: &mut BootstrapStream,
        idle_deadline: Instant,
        hard_deadline: Instant,
    ) -> QuitDrainEnd {
        let error = CallError::quit_drained();
        let privileged_call = self.fs.is_some().then_some(&keld_ipc::channel_table::FS);
        loop {
            let header = match read_quit_drain_frame(
                reader,
                privileged_call,
                idle_deadline,
                hard_deadline,
            ) {
                Ok(Some((header, _payload))) => header,
                Ok(None) => return QuitDrainEnd::IdleBackstop,
                Err(IpcError::Io(_)) => return QuitDrainEnd::PeerClosed,
                Err(IpcError::Timeout) => return QuitDrainEnd::Deadline,
                Err(_) => return QuitDrainEnd::NotAdmitted,
            };
            let declares_err = header.channel() == LIFECYCLE_CHANNEL
                || (privileged_call.is_some() && header.channel() == FS_CHANNEL);
            if header.kind() != FrameKind::Call || !declares_err {
                continue;
            }
            // A poisoned lock ends the drain; the caller's next acquisition
            // for the link close reports it.
            let Ok(mut current) = self.current.lock() else {
                return QuitDrainEnd::GenerationGone;
            };
            let Some(active) = current.as_mut().filter(|active| active.attempt == attempt) else {
                return QuitDrainEnd::GenerationGone;
            };
            if !write_best_effort_answers(
                &mut active.writer,
                [(header.channel(), header.corr())],
                &error,
            ) {
                return QuitDrainEnd::AnswerLost;
            }
        }
    }

    /// Test hook: reports how the next post-Quit drain ended.
    #[cfg(all(test, target_os = "macos"))]
    fn observe_next_quit_drain_end(&self, end: SyncSender<QuitDrainEnd>) {
        self.quit_drain_hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .end = Some(end);
    }

    #[cfg(all(test, target_os = "macos"))]
    fn report_quit_drain_end(&self, end: QuitDrainEnd) {
        let observer = self
            .quit_drain_hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .end
            .take();
        if let Some(observer) = observer {
            let _ = observer.send(end);
        }
    }

    /// Test hook: holds the next post-Quit drain before its first read until
    /// the test resumes it, then gives it an idle deadline that has already
    /// passed, as a host stall longer than its idle backstop would.
    #[cfg(all(test, target_os = "macos"))]
    fn stall_next_quit_drain(&self, stalled: SyncSender<()>, resume: Receiver<()>) {
        self.quit_drain_hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stall = Some((stalled, resume));
    }

    /// Returns the injected idle deadline (`started`, already passed) when
    /// the hook is armed.
    #[cfg(all(test, target_os = "macos"))]
    fn stall_quit_drain_for_test(&self, started: Instant) -> Option<Instant> {
        let (stalled, resume) = self
            .quit_drain_hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stall
            .take()?;
        let _ = stalled.send(());
        let _ = resume.recv_timeout(Duration::from_secs(5));
        Some(started)
    }

    fn has_outstanding_fs_call(&self, attempt: u32) -> bool {
        let _transition = self.shutdown.transition_guard();
        let Some(fs) = self.fs.as_ref().and_then(Weak::upgrade) else {
            return self.fs.is_some();
        };
        fs.has_outstanding_call_for(attempt)
    }

    fn begin_fs_call(
        &self,
        attempt: u32,
        correlation: CorrelationId,
    ) -> Result<FsInFlight, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        if !self.shutdown.is_running() {
            return Err(app_detail(
                "filesystem admission",
                "application session is quiescing",
            ));
        }
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("filesystem admission", "generation lock poisoned"))?;
        if current
            .as_ref()
            .is_none_or(|active| active.attempt != attempt)
        {
            return Err(app_detail(
                "filesystem admission",
                "filesystem Call belongs to a stale primary generation",
            ));
        }
        let fs = self.fs.as_ref().and_then(Weak::upgrade).ok_or_else(|| {
            app_detail(
                "filesystem admission",
                "guarded filesystem session is unavailable",
            )
        })?;
        drop(current);
        fs.begin_call(PendingFsCall {
            attempt,
            correlation,
        })?
        .ok_or_else(|| {
            app_detail(
                "filesystem admission",
                "guarded filesystem session is quiescing",
            )
        })
    }

    fn finish_fs_terminal_write(
        &self,
        admitted: FsInFlight,
        attempt: u32,
        write: impl FnOnce(&mut BootstrapStream) -> Result<(), HostAppError>,
    ) -> Result<(), HostAppError> {
        let transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("filesystem terminal reply", "generation lock poisoned"))?;
        admitted.retire_pending_call()?;
        if !self.shutdown.is_running() {
            return Ok(());
        }
        let Some(active) = current.as_mut() else {
            return Ok(());
        };
        if active.attempt != attempt {
            return Ok(());
        }
        let Err(primary) = write(&mut active.writer) else {
            #[cfg(all(test, target_os = "macos"))]
            {
                drop(current);
                drop(transition);
                admitted.session.hold_after_terminal_write();
            }
            return Ok(());
        };

        let retirement = self.retire_current_generation_locked(
            &mut current,
            attempt,
            "application call retired after a failed filesystem terminal write",
            "failed filesystem terminal write link close",
        );
        if let Err(cleanup) = retirement {
            drop(current);
            drop(transition);
            return Err(append_app_cleanup(primary, [Err(cleanup)]));
        }
        let Some(fs) = self.fs.as_ref().and_then(Weak::upgrade) else {
            drop(current);
            drop(transition);
            return Err(append_app_cleanup(
                primary,
                [Err(app_detail(
                    "filesystem reply recovery",
                    "guarded filesystem session disappeared after generation retirement",
                ))],
            ));
        };
        if let Err(cleanup) = fs.mark_failed_write_attempt(attempt) {
            drop(current);
            drop(transition);
            return Err(append_app_cleanup(primary, [Err(cleanup)]));
        }
        drop(current);
        drop(transition);
        // The failed-write marker retains the recovery evidence. Release the
        // native-work lease before waiting on the generation owner: that owner
        // may already be installing a successor which waits in fs.drain().
        drop(admitted);

        if let Err(cleanup) = self.guardian.fail_retired_generation(attempt) {
            return Err(append_app_cleanup(primary, [Err(cleanup)]));
        }
        if let Err(cleanup) = fs.clear_failed_write_attempt(attempt) {
            return Err(append_app_cleanup(primary, [Err(cleanup)]));
        }
        Ok(())
    }

    fn quiesce_and_drain_fs(&self) -> Result<(), HostAppError> {
        let Some(fs) = self.fs.as_ref().and_then(Weak::upgrade) else {
            return Ok(());
        };
        fs.quiesce()?;
        fs.drain()
    }

    // Caller retains shutdown.transition through the write and any admission
    // command that must precede a terminal claim.
    fn write_event_guarded(&self, event: LifecycleEvent) -> Result<(), PrimaryGenerationFailure> {
        let payload = encode(&event).map_err(|source| app_ipc("lifecycle event", &source))?;
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
        let attempt = active.attempt;
        if let Err(source) = write_frame(
            &mut active.writer,
            FrameKind::Event,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            &payload,
        ) {
            let primary = app_ipc("lifecycle event", &source);
            let retirement = self.retire_current_generation_locked(
                &mut current,
                attempt,
                "application call retired after a failed lifecycle write",
                "failed lifecycle event link close",
            );
            return Err(PrimaryGenerationFailure {
                #[cfg(any(target_os = "linux", windows))]
                retired_after_failed_ready: (retirement.is_ok()
                    && matches!(event, LifecycleEvent::Ready))
                .then_some(attempt),
                error: append_app_cleanup(primary, [retirement]),
            });
        }
        Ok(())
    }

    fn lifecycle_quit(
        &self,
        attempt: u32,
        correlation: CorrelationId,
        reply: &[u8],
        reader: &mut BootstrapStream,
    ) -> Result<(), HostAppError> {
        {
            let transition = self.shutdown.transition_guard();
            let mut current_guard = self.current.lock().map_err(|_| {
                app_detail("primary session generation", "generation lock poisoned")
            })?;
            let Some(active) = current_guard
                .as_mut()
                .filter(|active| active.attempt == attempt)
            else {
                return Ok(());
            };
            if !self.shutdown.is_running() {
                drop(current_guard);
                drop(transition);
                return if self.shutdown.cause() == SESSION_CLI_LEASE_LOST {
                    self.cli_lease_lost()
                } else {
                    Ok(())
                };
            }
            // Pending until its REPLY: a retirement during the FS drain below
            // answers it with KELD-IPC-023 (GH-527 §4.9).
            active.pending_quit = Some(correlation);
        }

        // This reader is paused in Quit, so no later FS Call can enter. Keep
        // the session live until every already-admitted FS call has written its
        // terminal Reply/Err and released its lease; clients close immediately
        // after the correlated Quit Reply.
        if let Some(fs) = self.fs.as_ref().and_then(Weak::upgrade) {
            fs.drain()?;
        }

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
        self.fail_pending_echo_for_attempt(
            attempt,
            "application call retired by accepted lifecycle Quit",
        )?;
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
        active.pending_quit = None;
        write_frame(
            &mut active.writer,
            FrameKind::Reply,
            0,
            LIFECYCLE_CHANNEL,
            correlation,
            reply,
        )
        .map_err(|source| app_ipc("lifecycle Quit reply", &source))?;
        drop(current_guard);
        self.quiesce_and_drain_fs()?;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        self.answer_calls_after_quit(attempt, reader);
        let mut current_guard = self
            .current
            .lock()
            .map_err(|_| app_detail("primary session generation", "generation lock poisoned"))?;
        let active = current_guard.as_mut().ok_or_else(|| {
            app_detail(
                "lifecycle Quit link close",
                "current primary generation disappeared after the reply",
            )
        })?;
        #[cfg(windows)]
        let peer_close = await_windows_quit_peer_close(reader);
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let peer_close = Ok(());
        let link_close = finish_link_shutdown(
            active.writer.shutdown_app_link(),
            "lifecycle Quit link close",
        );
        current_guard.take();
        drop(current_guard);
        collapse_app_results([peer_close, link_close, self.finish_tail("lifecycle Quit")])
    }

    fn cli_lease_lost(&self) -> Result<(), HostAppError> {
        if self.shutdown.cause() != SESSION_CLI_LEASE_LOST || !self.shutdown.begin_tail() {
            return Ok(());
        }
        self.guardian.prepare_accepted_shutdown()?;
        self.quiesce_and_drain_fs()?;
        let mut current_guard = self
            .current
            .lock()
            .map_err(|_| app_detail("primary session generation", "generation lock poisoned"))?;
        let pending = self.fail_any_pending_echo("application call retired by CLI lease loss");
        let link = current_guard.as_mut().map_or(Ok(()), |active| {
            finish_link_shutdown(
                active.writer.shutdown_app_link(),
                "CLI lease-loss link close",
            )
        });
        current_guard.take();
        drop(current_guard);
        collapse_app_results([pending, link, self.finish_tail("CLI lease loss")])
    }

    fn apply_generation_update(
        &self,
        update: PrimaryOwnerUpdate,
    ) -> Result<(), PrimaryGenerationFailure> {
        match update {
            PrimaryOwnerUpdate::Role(PrimaryRoleEvent::Revoked { attempt, .. }) => {
                self.last_revoked_attempt
                    .fetch_max(attempt, Ordering::AcqRel);
                if !self.window_ready.load(Ordering::Acquire) {
                    return Err(app_detail(
                        "primary generation before Ready",
                        "Bun terminated before the initial window became ready",
                    )
                    .into());
                }
                self.retire_generation(attempt).map_err(Into::into)
            }
            PrimaryOwnerUpdate::Role(_) => Ok(()),
            PrimaryOwnerUpdate::Bound(bound) => {
                let attempt = bound.attempt();
                if attempt <= self.last_revoked_attempt.load(Ordering::Acquire) {
                    let stream = bound.into_stream();
                    let _ = stream.shutdown_app_link();
                    return Ok(());
                }
                self.install_generation(attempt, bound.into_stream())
            }
        }
    }

    fn install_generation(
        &self,
        attempt: u32,
        mut stream: BootstrapStream,
    ) -> Result<(), PrimaryGenerationFailure> {
        stream
            .set_app_link_read_deadline(Some(APP_LINK_READER_POLL))
            .map_err(|source| app_io("primary session reader deadline", &source))?;
        let writer_stream = stream
            .try_clone()
            .map_err(|source| app_io("primary session writer clone", &source))?;
        writer_stream
            .set_app_link_write_deadline(Some(APP_LINK_IO_DEADLINE))
            .map_err(|source| app_io("primary session writer deadline", &source))?;

        // Recovery must not publish a successor while filesystem authority from
        // its retired predecessor is still alive. Check retirement under the
        // shared transition, release every generation/shutdown lock, then wait
        // for the old native call to reach its terminal outcome. A genuinely
        // wedged synchronous syscall may therefore stall recovery, as KEL-130
        // permits, without overlapping old/new privileged authority.
        if let Some(fs) = self.fs.as_ref().and_then(Weak::upgrade) {
            {
                let _transition = self.shutdown.transition_guard();
                if !self.shutdown.is_running() || self.shutdown.reader_stop.load(Ordering::Acquire)
                {
                    let _ = writer_stream.shutdown_app_link();
                    return Ok(());
                }
                let current = self.current.lock().map_err(|_| {
                    app_detail("primary session generation", "generation lock poisoned")
                })?;
                if current.is_some() {
                    return Err(app_detail(
                        "primary session generation",
                        "successor bound before the retired generation was revoked",
                    )
                    .into());
                }
            }
            fs.drain()?;
        }

        let reader_stop;
        {
            let _transition = self.shutdown.transition_guard();
            if !self.shutdown.is_running() || self.shutdown.reader_stop.load(Ordering::Acquire) {
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
                )
                .into());
            }
            reader_stop = self.shutdown.register_generation_reader();
            *current = Some(ActivePrimaryGeneration {
                attempt,
                writer: writer_stream,
                reader_stop: Arc::clone(&reader_stop),
                pending_quit: None,
            });
            drop(current);
            // Publication and retained replay are one transition with live
            // window delivery. Start the reader only after replay, so a call
            // cannot overtake Ready or observe the same Close via both paths.
            if self.window_ready.load(Ordering::Acquire) {
                self.write_event_guarded(LifecycleEvent::Ready)?;
            }
            if self.last_window_closed.load(Ordering::Acquire) {
                self.write_event_guarded(LifecycleEvent::LastWindowClosed)?;
            }
        }
        let handle = self.clone();
        let reader = thread::Builder::new()
            .name(format!("keld-core-primary-router-{attempt}"))
            .spawn(move || {
                let mut result = read_primary_frames(&mut stream, &handle, attempt, &reader_stop);
                if result.is_err()
                    && let Err(cleanup) = handle.fail_pending_echo_for_attempt(
                        attempt,
                        "application call ended with the primary reader",
                    )
                    && let Err(primary) = result
                {
                    result = Err(append_app_cleanup(primary, [Err(cleanup)]));
                }
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
            self.answer_pending_calls_locked(
                &mut current,
                attempt,
                &CallError::generation_retired(),
            );
            self.retire_current_generation_locked(
                &mut current,
                attempt,
                "application call retired with the primary generation",
                "retired primary generation link close",
            )?;
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct PrimaryRouter {
    handle: PrimaryRouterHandle,
    fs_worker: Option<FsWorker>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct FsWorker {
    commands: SyncSender<FsWorkerCommand>,
    join: Option<JoinHandle<Result<(), HostAppError>>>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl FsWorker {
    fn start(
        handle: &mut PrimaryRouterHandle,
        #[cfg(all(test, any(target_os = "macos", target_os = "linux")))] test_gate: Option<
            Arc<FsWorkerTestGate>,
        >,
    ) -> Result<Self, HostAppError> {
        let (commands, receiver) = mpsc::sync_channel(1);
        let worker_handle = handle.clone();
        let window_commands = handle.window_commands.clone();
        let join = thread::Builder::new()
            .name(String::from("keld-core-fs-worker"))
            .spawn(move || {
                let result = run_fs_worker(
                    &receiver,
                    &worker_handle,
                    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
                    test_gate.as_deref(),
                );
                if result.is_err() {
                    let _ = window_commands.send(AppWindowCommand::Fatal);
                }
                result
            })
            .map_err(|source| app_io("filesystem worker", &source))?;
        handle.fs_worker_commands = Some(commands.clone());
        Ok(Self {
            commands,
            join: Some(join),
        })
    }

    fn stop_and_join(&mut self) -> Result<(), HostAppError> {
        let stop = match self.commands.try_send(FsWorkerCommand::Stop) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => Ok(()),
            Err(TrySendError::Full(_)) => Err(app_detail(
                "filesystem worker shutdown",
                "worker queue remained full after filesystem drain",
            )),
        };
        let join = self.join.take().map_or(Ok(()), |join| {
            join.join()
                .map_err(|_| app_detail("filesystem worker", "thread panicked"))?
        });
        collapse_app_results([stop, join])
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl PrimaryRouter {
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    fn start(
        stream: BootstrapStream,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
    ) -> Result<Self, HostAppError> {
        Self::start_with_fs(stream, window_commands, guardian, shutdown, None)
    }

    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    fn start_with_fs(
        stream: BootstrapStream,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
        fs: Option<Weak<FsDispatchSession>>,
    ) -> Result<Self, HostAppError> {
        Self::start_with_fs_test_gate(stream, window_commands, guardian, shutdown, fs, None)
    }

    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    fn start_with_fs_test_gate(
        stream: BootstrapStream,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
        fs: Option<Weak<FsDispatchSession>>,
        fs_worker_test_gate: Option<Arc<FsWorkerTestGate>>,
    ) -> Result<Self, HostAppError> {
        let handle = PrimaryRouterHandle {
            current: Arc::new(Mutex::new(None)),
            readers: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(target_os = "macos")]
            pending_echo: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "macos")]
            pending_echo_attempt: Arc::new(AtomicU32::new(0)),
            #[cfg(target_os = "macos")]
            pending_echo_corr: Arc::new(AtomicU32::new(0)),
            #[cfg(target_os = "macos")]
            next_host_corr: Arc::new(AtomicU32::new(1)),
            window_ready: Arc::new(AtomicBool::new(false)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(true)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown,
            fs,
            fs_worker_commands: None,
            guardian,
            window_commands,
            #[cfg(all(test, target_os = "macos"))]
            quit_drain_hooks: Arc::default(),
        };
        let router = Self::finish_start(
            handle,
            #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
            fs_worker_test_gate,
        )?;
        router.handle.install_generation(1, stream)?;
        Ok(router)
    }

    fn start_bound(
        bound: BoundPrimaryGeneration,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
        fs: Option<Weak<FsDispatchSession>>,
    ) -> Result<Self, HostAppError> {
        let attempt = bound.attempt();
        let stream = bound.into_stream();
        let handle = PrimaryRouterHandle {
            current: Arc::new(Mutex::new(None)),
            readers: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(target_os = "macos")]
            pending_echo: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "macos")]
            pending_echo_attempt: Arc::new(AtomicU32::new(0)),
            #[cfg(target_os = "macos")]
            pending_echo_corr: Arc::new(AtomicU32::new(0)),
            #[cfg(target_os = "macos")]
            next_host_corr: Arc::new(AtomicU32::new(1)),
            window_ready: Arc::new(AtomicBool::new(false)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(false)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown,
            fs,
            fs_worker_commands: None,
            guardian,
            window_commands,
            #[cfg(all(test, target_os = "macos"))]
            quit_drain_hooks: Arc::default(),
        };
        let router = Self::finish_start(
            handle,
            #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
            None,
        )?;
        router.handle.install_generation(attempt, stream)?;
        Ok(router)
    }

    fn finish_start(
        mut handle: PrimaryRouterHandle,
        #[cfg(all(test, any(target_os = "macos", target_os = "linux")))] test_gate: Option<
            Arc<FsWorkerTestGate>,
        >,
    ) -> Result<Self, HostAppError> {
        let fs_worker = if handle.fs.is_some() {
            Some(FsWorker::start(
                &mut handle,
                #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
                test_gate,
            )?)
        } else {
            None
        };
        Ok(Self { handle, fs_worker })
    }

    fn handle(&self) -> PrimaryRouterHandle {
        self.handle.clone()
    }

    fn shutdown(mut self) -> Result<(), HostAppError> {
        self.stop_and_join()
    }

    fn stop_and_join(&mut self) -> Result<(), HostAppError> {
        let fs = self.handle.fs.as_ref().and_then(Weak::upgrade);
        let (fs_close, pending) = {
            // Failed Ready recovery, FS admission closure, and successor publication
            // observe one serialized teardown transition. Publish cooperative
            // cancellation before waiting for a handler that may be inside native I/O.
            let _transition = self.handle.shutdown.transition_guard();
            let fs_close = fs
                .as_ref()
                .map_or(Ok(()), |session| session.close_admission());
            self.handle.shutdown.stop_reader();
            let mut current = match self.handle.current.lock() {
                Ok(current) => current,
                Err(poisoned) => poisoned.into_inner(),
            };
            let pending = self
                .handle
                .fail_any_pending_echo("application call retired by router shutdown");
            if let Some(active) = current.take() {
                let _ = active.writer.shutdown_app_link();
            }
            (fs_close, pending)
        };
        // The transition above must be released before this wait: an already-entered
        // native handler may retain its own entry transition until its terminal outcome.
        let fs_wait = fs
            .as_ref()
            .map_or(Ok(()), |session| session.wait_for_handler_transition());
        let fs_quiesce = collapse_app_results([fs_close, fs_wait]);
        let readers = match self.handle.readers.lock() {
            Ok(mut readers) => std::mem::take(&mut *readers),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };
        let mut results = vec![fs_quiesce, pending];
        for (_, reader) in readers {
            results.push(
                reader
                    .join()
                    .map_err(|_| app_detail("primary session reader", "thread panicked"))?,
            );
        }
        if let Some(fs) = fs {
            results.push(fs.drain());
        }
        if let Some(worker) = &mut self.fs_worker {
            results.push(worker.stop_and_join());
        }
        collapse_app_results(results)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn prepare_fs_work(
    handle: &PrimaryRouterHandle,
    attempt: u32,
    correlation: CorrelationId,
    payload: &[u8],
    cancellation: &Arc<AtomicBool>,
) -> Result<FsWorkItem, HostAppError> {
    let admitted = handle.begin_fs_call(attempt, correlation)?;
    let request = decode(payload).map_err(|source| app_ipc("filesystem request", &source))?;
    Ok(FsWorkItem {
        admitted,
        request,
        attempt,
        correlation,
        cancellation: Arc::clone(cancellation),
    })
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn run_fs_worker(
    receiver: &Receiver<FsWorkerCommand>,
    handle: &PrimaryRouterHandle,
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))] test_gate: Option<
        &FsWorkerTestGate,
    >,
) -> Result<(), HostAppError> {
    while let Ok(command) = receiver.recv() {
        let FsWorkerCommand::Work(work) = command else {
            return Ok(());
        };
        let FsWorkItem {
            admitted,
            request,
            attempt,
            correlation,
            cancellation,
        } = work;
        let outcome = admitted.handle_with(|| {
            #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
            if let Some(gate) = test_gate {
                gate.wait_after_take()?;
            }
            Ok::<_, HostAppError>(admitted.session.broker.handle_request(
                &admitted.session.verified,
                Principal::AppProcess,
                request,
                &cancellation,
            ))
        })?;
        let Some(outcome) = outcome else {
            continue;
        };
        match outcome? {
            Ok(response) => {
                let reply =
                    encode(&response).map_err(|source| app_ipc("filesystem response", &source))?;
                handle.finish_fs_terminal_write(admitted, attempt, |writer| {
                    write_frame(writer, FrameKind::Reply, 0, FS_CHANNEL, correlation, &reply)
                        .map_err(|source| app_ipc("primary session reply", &source))
                })?;
            }
            Err(error) => {
                let call_error = CallError::from(&error);
                handle.finish_fs_terminal_write(admitted, attempt, |writer| {
                    keld_ipc::write_call_error(writer, FS_CHANNEL, correlation, &call_error)
                        .map_err(|source| app_ipc("primary session call error", &source))
                })?;
            }
        }
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl Drop for PrimaryRouter {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[allow(clippy::too_many_lines)] // one reader owns the complete echo/lifecycle frame dispatch
fn read_primary_frames(
    reader: &mut BootstrapStream,
    handle: &PrimaryRouterHandle,
    attempt: u32,
    reader_stop: &Arc<AtomicBool>,
) -> Result<(), HostAppError> {
    loop {
        if handle.shutdown.cause() == SESSION_CLI_LEASE_LOST {
            handle.cli_lease_lost()?;
            return Ok(());
        }
        // KEL-133 owns every pre-payload semantic check. A guarded session
        // selects the existing privileged CALL policy for the `fs` channel
        // table entry; an unguarded session keeps the legacy
        // echo/lifecycle/PING policy.
        let frame = if handle.fs.is_some() {
            read_primary_app_frame_interruptible_with_privileged_call(
                reader,
                reader_stop,
                || handle.pending_echo_corr_for(attempt),
                &keld_ipc::channel_table::FS,
                || handle.has_outstanding_fs_call(attempt),
            )
        } else {
            read_primary_app_frame_interruptible(reader, reader_stop, || {
                handle.pending_echo_corr_for(attempt)
            })
        };
        let (header, payload) = match frame {
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
                    handle.fail_pending_echo_for_attempt(
                        attempt,
                        "application call ended because the app link disconnected",
                    )?;
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
        match (header.kind(), header.channel()) {
            #[cfg(target_os = "macos")]
            (FrameKind::Reply, ECHO_CHANNEL) => {
                handle.finish_pending_echo(
                    attempt,
                    header.corr(),
                    PrimaryEchoReply::Reply(payload),
                )?;
            }
            #[cfg(target_os = "macos")]
            (FrameKind::Err, ECHO_CHANNEL) => {
                let error: CallError =
                    decode(&payload).map_err(|source| app_ipc("renderer Echo Err", &source))?;
                handle.finish_pending_echo(attempt, header.corr(), PrimaryEchoReply::Err(error))?;
            }
            (FrameKind::Call, ECHO_CHANNEL) if handle.shutdown.is_running() => {
                let reply = keld_ipc::echo::handle_echo(&payload)
                    .map_err(|source| app_ipc("echo dispatch", &source))?;
                write_primary_reply(
                    &handle.current,
                    &handle.shutdown,
                    attempt,
                    ECHO_CHANNEL,
                    header.corr(),
                    &reply,
                )?;
            }
            (FrameKind::Call, FS_CHANNEL) if handle.shutdown.is_running() => {
                let work = prepare_fs_work(handle, attempt, header.corr(), &payload, reader_stop)?;
                let commands = handle.fs_worker_commands.as_ref().ok_or_else(|| {
                    app_detail(
                        "filesystem worker submission",
                        "guarded filesystem worker is unavailable",
                    )
                })?;
                match commands.try_send(FsWorkerCommand::Work(work)) {
                    Ok(()) => {}
                    Err(TrySendError::Full(rejected)) => {
                        drop(rejected);
                        return Err(app_detail(
                            "filesystem worker submission",
                            "bounded filesystem worker queue is full",
                        ));
                    }
                    Err(TrySendError::Disconnected(rejected)) => {
                        drop(rejected);
                        return Err(app_detail(
                            "filesystem worker submission",
                            "filesystem worker is disconnected",
                        ));
                    }
                }
            }
            (FrameKind::Call, LIFECYCLE_CHANNEL) => {
                let request: LifecycleRequest =
                    decode(&payload).map_err(|source| app_ipc("lifecycle request", &source))?;
                match request {
                    LifecycleRequest::Quit => {
                        let reply = encode(&LifecycleResponse::Quit)
                            .map_err(|source| app_ipc("lifecycle Quit reply", &source))?;
                        handle.lifecycle_quit(attempt, header.corr(), &reply, reader)?;
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
                    header.channel(),
                    header.corr(),
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
            _ => {
                // The validator admits only the declared combinations above;
                // this stays a typed error so the host never panics.
                return Err(app_detail(
                    "primary session dispatch",
                    "unexpected frame kind",
                ));
            }
        }
    }
}

/// The host's budget for one best-effort terminal answer (GH-527 §4.9): the
/// write deadline of each `KELD-IPC-023` and `KELD-IPC-024` `ERR`, in place of
/// the writer's [`APP_LINK_IO_DEADLINE`], and how long the post-Quit drain
/// waits without a byte for a peer that never ends the link. One reader poll,
/// so a peer that stops reading or never closes delays the close by at most
/// this much plus one poll per step.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const HOST_ANSWER_BUDGET: Duration = APP_LINK_READER_POLL;

/// Writes best-effort terminal `ERR` answers (`KELD-IPC-023` or
/// `KELD-IPC-024`, GH-527 §4.9) on a link the caller closes next, each write
/// bounded by [`HOST_ANSWER_BUDGET`]. Returns `false` once a write fails or
/// times out: that link is lost, so no later answer is attempted, and the
/// role observes the close as `KELD-IPC-022`. The writer's deadline is not
/// restored, because the caller closes the link.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn write_best_effort_answers(
    writer: &mut BootstrapStream,
    answers: impl IntoIterator<Item = (keld_ipc::ChannelId, CorrelationId)>,
    error: &CallError,
) -> bool {
    if writer
        .set_app_link_write_deadline(Some(HOST_ANSWER_BUDGET))
        .is_err()
    {
        return false;
    }
    answers
        .into_iter()
        .all(|(channel, corr)| keld_ipc::write_call_error(writer, channel, corr, error).is_ok())
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn finish_link_shutdown(result: io::Result<()>, phase: &'static str) -> Result<(), HostAppError> {
    match result {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotConnected => Ok(()),
        Err(source) => Err(app_io(phase, &source)),
    }
}

#[cfg(windows)]
fn await_windows_quit_peer_close(reader: &mut BootstrapStream) -> Result<(), HostAppError> {
    let deadline = Instant::now()
        .checked_add(APP_LINK_IO_DEADLINE)
        .ok_or_else(|| app_detail("lifecycle Quit drain", "peer-close deadline overflow"))?;
    await_windows_quit_peer_close_until(reader, deadline)
}

#[cfg(windows)]
fn await_windows_quit_peer_close_until(
    reader: &mut BootstrapStream,
    deadline: Instant,
) -> Result<(), HostAppError> {
    let mut unexpected = [0_u8; 1];
    loop {
        match reader.read(&mut unexpected) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                return Err(app_detail(
                    "lifecycle Quit drain",
                    "app sent bytes after the terminal Quit reply",
                ));
            }
            Err(error)
                if matches!(error.raw_os_error(), Some(109 | 232 | 233))
                    || error.kind() == io::ErrorKind::NotConnected =>
            {
                return Ok(());
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::WouldBlock
                ) =>
            {
                if Instant::now() >= deadline {
                    return Err(app_detail(
                        "lifecycle Quit drain",
                        "app did not close after the terminal Quit reply deadline",
                    ));
                }
            }
            Err(error) => return Err(app_io("lifecycle Quit drain", &error)),
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn app_detail(phase: &'static str, detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-037",
        phase,
        detail,
        "Fix the app session failure and relaunch the no-flag host.",
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn append_app_cleanup<const N: usize>(
    mut primary: HostAppError,
    cleanup: [Result<(), HostAppError>; N],
) -> HostAppError {
    for error in cleanup.into_iter().filter_map(Result::err) {
        primary.detail.push_str("; cleanup: ");
        primary.detail.push_str(&error.to_string());
    }
    primary
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn app_io(phase: &'static str, source: &io::Error) -> HostAppError {
    HostAppError::io(
        "KELD-CORE-037",
        phase,
        source,
        "Fix the app session I/O failure and relaunch the no-flag host.",
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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

#[cfg(any(target_os = "linux", windows))]
fn app_runtime_fatal(phase: &'static str, source: &keld_runtime::RuntimeError) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-033",
        phase,
        source.to_string(),
        "Fix the cause named by the nested KELD-RUNTIME diagnostic, then relaunch the no-flag host.",
    )
}

#[cfg(any(target_os = "linux", windows))]
fn app_self_termination(record: keld_runtime::SelfTerminationRecord) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-033",
        "primary self-termination",
        format!(
            "Bun process {} exited without a host shutdown request (status {:?})",
            record.pid, record.exit_code
        ),
        "Fix the primary process self-termination, then relaunch the no-flag host.",
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn app_ipc(phase: &'static str, source: &IpcError) -> HostAppError {
    app_detail(phase, source.to_string())
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use std::cell::Cell;
    use std::path::Path;

    use std::fs;
    #[cfg(windows)]
    use std::io::{BufRead as _, BufReader};
    #[cfg(target_os = "macos")]
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[cfg(windows)]
    use std::process::{Command, Stdio};

    use super::*;

    const DIGEST: &str = "sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356";

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    struct ReapedRouterProbe(std::process::Child);

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    impl Drop for ReapedRouterProbe {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    const RECOVERED_CHILD_ENV: &str = "KELD_CORE_TEST_RECOVERED_EVENT_ORDER_CHILD";
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    const RECOVERED_CHILD_OBSERVED: &str = "KELD_CORE_RECOVERED_EVENT_ORDER_OBSERVED";

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn assert_bounded_recovered_event_order_child() {
        use std::io::{Read as _, Seek as _, SeekFrom};
        use std::process::{Command, Stdio};

        const SELECTOR: &str = "app_session::tests::recovered_generation_orders_window_events_once_across_installation";
        let mut stdout = tempfile::tempfile().expect("private child stdout");
        let mut stderr = tempfile::tempfile().expect("private child stderr");
        let mut child = ReapedRouterProbe(
            Command::new(std::env::current_exe().expect("test binary"))
                .args(["--exact", SELECTOR, "--nocapture"])
                .env(RECOVERED_CHILD_ENV, "1")
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout.try_clone().expect("child stdout clone")))
                .stderr(Stdio::from(stderr.try_clone().expect("child stderr clone")))
                .spawn()
                .expect("spawn isolated event-order test"),
        );
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.0.try_wait().expect("inspect event-order child") {
                break status;
            }
            if started.elapsed() >= Duration::from_secs(10) {
                child.0.kill().expect("kill blocked event-order child");
                let status = child.0.wait().expect("reap blocked event-order child");
                stdout
                    .seek(SeekFrom::Start(0))
                    .expect("rewind child stdout");
                stderr
                    .seek(SeekFrom::Start(0))
                    .expect("rewind child stderr");
                let mut output = String::new();
                let mut detail = String::new();
                stdout
                    .read_to_string(&mut output)
                    .expect("read child stdout");
                stderr
                    .read_to_string(&mut detail)
                    .expect("read child stderr");
                panic!(
                    "event-order child exceeded 10-second cleanup bound: status={status}, stdout={output}, stderr={detail}"
                );
            }
            std::thread::park_timeout(Duration::from_millis(10));
        };
        stdout
            .seek(SeekFrom::Start(0))
            .expect("rewind child stdout");
        stderr
            .seek(SeekFrom::Start(0))
            .expect("rewind child stderr");
        let mut output = String::new();
        let mut detail = String::new();
        stdout
            .read_to_string(&mut output)
            .expect("read child stdout");
        stderr
            .read_to_string(&mut detail)
            .expect("read child stderr");
        assert!(
            status.success()
                && output.contains(RECOVERED_CHILD_OBSERVED)
                && output.contains("test result: ok. 1 passed; 0 failed;"),
            "isolated event-order proof failed: status={status}, stdout={output}, stderr={detail}"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_publisher_and_signed_identifier_both_separate_profiles() {
        let first = MacValidatedAppIdentity::from_verified_parts("ABCDEFGHIJ", "com.example.app")
            .expect("verified signing facts");
        let publisher_changed =
            MacValidatedAppIdentity::from_verified_parts("KLMNOPQRST", "com.example.app")
                .expect("verified publisher change");
        let app_changed =
            MacValidatedAppIdentity::from_verified_parts("ABCDEFGHIJ", "com.example.other")
                .expect("verified signing identifier change");
        assert_ne!(first.profile_identity, publisher_changed.profile_identity);
        assert_ne!(first.profile_identity, app_changed.profile_identity);
        assert!(
            MacValidatedAppIdentity::from_verified_parts("ABCDEFGHIJ", "Com.Example.App").is_err()
        );
        assert!(MacValidatedAppIdentity::from_verified_parts("", "com.example.app").is_err());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_dev_selection_never_reads_release_identity_and_changes_each_launch() {
        let first = select_macos_profile_mode_with(true, || {
            panic!("explicit dev lease must bypass release signing identity")
        })
        .expect("first ephemeral selection");
        let second = select_macos_profile_mode_with(true, || {
            panic!("explicit dev lease must bypass release signing identity")
        })
        .expect("second ephemeral selection");
        let (WebProfileSelection::EphemeralDev(first), WebProfileSelection::EphemeralDev(second)) =
            (first, second)
        else {
            panic!("dev lease selected a persistent profile")
        };
        assert_ne!(first.launch_nonce(), second.launch_nonce());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_release_selection_rejects_signature_failure() {
        let error = select_macos_profile_mode_with(false, || {
            Err(macos_identity_error(
                "synthetic signature validation failure",
            ))
        })
        .expect_err("release mode must fail closed");
        assert_eq!(error.code(), "KELD-WV-009");
        assert!(
            error
                .to_string()
                .contains("synthetic signature validation failure")
        );
    }

    #[cfg(windows)]
    struct ReapedTestChild(std::process::Child);

    #[cfg(windows)]
    impl Drop for ReapedTestChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

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
    #[cfg(windows)]
    fn synthetic_windows_verified_parts_keep_publisher_and_app_separate() {
        let first = ValidatedAppIdentity::from_verified_parts([1; 32], "com.example.app")
            .expect("synthetic verified identity");
        let publisher_changed =
            ValidatedAppIdentity::from_verified_parts([2; 32], "com.example.app")
                .expect("synthetic publisher change");
        let app_changed = ValidatedAppIdentity::from_verified_parts([1; 32], "com.example.other")
            .expect("synthetic app change");
        assert_ne!(first.profile_identity, publisher_changed.profile_identity);
        assert_ne!(first.profile_identity, app_changed.profile_identity);
        for noncanonical in ["Com.Example.App", "com.example.app\n"] {
            let error = ValidatedAppIdentity::from_verified_parts([1; 32], noncanonical)
                .expect_err("a noncanonical verified app id has no profile identity");
            assert_eq!(error.code(), "KELD-WV-009");
        }
    }

    #[test]
    #[cfg(windows)]
    fn windows_profile_follows_the_boot_route() {
        let development =
            select_windows_profile_mode(None, true).expect("a dev stage runs under its lease");
        assert!(matches!(development, WindowsProfileMode::EphemeralDev));

        let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
            .expect("synthetic verified identity");
        let verified_profile = identity.profile_identity;
        let installed = select_windows_profile_mode(Some(identity), false)
            .expect("an installed package runs without a lease");
        let WindowsProfileMode::Persistent(selection) = installed else {
            panic!("an installed package selected an ephemeral profile")
        };
        assert_eq!(
            selection,
            WebProfileSelection::Persistent(verified_profile),
            "the profile is the identity verified at boot"
        );

        let error = select_windows_profile_mode(None, false)
            .expect_err("a dev stage without a live lease must not run");
        assert_eq!(error.code(), "KELD-CORE-037");
        assert!(error.to_string().contains("no live dev lease"), "{error}");
        let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
            .expect("synthetic verified identity");
        let error = select_windows_profile_mode(Some(identity), true)
            .expect_err("an installed package must not run under a dev lease");
        assert_eq!(error.code(), "KELD-CORE-037");
        assert!(
            error.to_string().contains("presented a dev lease"),
            "{error}"
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_dev_lease_routes_only_to_the_dev_stage_validator() {
        let error = select_windows_boot_route_with(
            true,
            || Err(target_error("app root", "synthetic dev-stage refusal")),
            || panic!("a dev lease must not reach installed-package boot"),
        )
        .expect_err("the synthetic dev-stage validator decides the dev route");
        assert_eq!(error.code(), "KELD-CORE-036");
        assert!(error.to_string().contains("synthetic dev-stage refusal"));
    }

    #[test]
    #[cfg(windows)]
    fn windows_lease_less_boot_reaches_only_installed_package_boot() {
        let unsigned = select_windows_boot_route_with(
            false,
            || panic!("a lease-less launch must not reach the dev-stage validator"),
            || {
                Err(windows_identity_error(
                    "synthetic verifier rejection",
                    "Use a real signed package fixture.",
                ))
            },
        )
        .expect_err("an unverified lease-less executable is refused");
        assert_eq!(unsigned.code(), "KELD-WV-009");

        let unselected = select_windows_boot_route_with(
            false,
            || panic!("a lease-less launch must not reach the dev-stage validator"),
            || {
                Err(installed_package_error(
                    &keld_update::UpdateError::ExecutableBinding {
                        image: keld_update::WindowsLocatedImage::Host,
                        step: "locator",
                        detail: "synthetic staged layout".to_owned(),
                    },
                ))
            },
        )
        .expect_err("a verified host outside an installation is refused");
        assert_eq!(unselected.code(), "KELD-UPDATE-018");
        assert!(
            unselected.to_string().contains("synthetic staged layout"),
            "{unselected}"
        );
    }

    #[test]
    #[cfg(windows)]
    fn installed_package_refusal_carries_its_code_once() {
        let error = installed_package_error(&keld_update::UpdateError::ExecutableBinding {
            image: keld_update::WindowsLocatedImage::Host,
            step: "locator",
            detail: "synthetic staged layout".to_owned(),
        });
        let rendered = error.to_string();
        println!("KELD_KEL254_INSTALLED_PACKAGE_REFUSAL {rendered}");
        // The resource snapshot reads process-wide counters other tests advance.
        let (message, _resources) = rendered
            .split_once(" [startup-resource-attempts ")
            .expect("a host error ends with its startup-resource snapshot");
        assert_eq!(
            message,
            "KELD-UPDATE-018: no-flag host failed during Windows installed package selection \
             — installed executable locator refused (synthetic staged layout). Launch \
             keld-host.exe from its installation's selected version tree; repair or reinstall \
             through the trusted installer if the layout is damaged. Then relaunch the \
             installed host."
        );
        assert_eq!(rendered.matches("KELD-UPDATE-").count(), 1, "{rendered}");
        assert!(!rendered.contains(".."), "{rendered}");
    }

    #[test]
    #[cfg(windows)]
    fn installed_boot_of_an_unsigned_host_refuses_before_resources() {
        let before = startup_resource_snapshot();
        let error = validate_installed_current_exe()
            .expect_err("the unsigned test executable is not an installed package");
        assert_eq!(error.code(), "KELD-WV-009");
        // The running image opened as a pinned trust image and was refused by its owner.
        assert!(
            error
                .to_string()
                .contains("rejected the current executable with status"),
            "{error}"
        );
        assert_eq!(
            error.resources, before,
            "installed boot advanced app resources"
        );
    }

    #[test]
    #[cfg(windows)]
    fn installed_boot_reads_the_expectation_from_the_handle_before_selection() {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

        let executable = std::env::current_exe().expect("test executable path");
        // The test executable carries no `.keldeai` container and lies in no installation.
        let image = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&executable)
            .expect("open the test executable as the verified-image owner does");
        let locator = executable.canonicalize().expect("canonical locator");
        let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
            .expect("synthetic verified identity");
        let before = startup_resource_snapshot();
        let error = validate_installed_from_verified(&locator, &image, identity)
            .expect_err("a host without an embedded expectation is refused");
        // KELD-UPDATE-019, not the locator's KELD-UPDATE-018: the expectation is read from
        // the handle before any installation is located.
        assert_eq!(error.code(), "KELD-UPDATE-019");
        assert!(error.to_string().contains("KELD-PACK-007"), "{error}");
        assert_eq!(
            error.resources, before,
            "installed boot advanced app resources"
        );
    }

    #[test]
    #[cfg(windows)]
    fn installed_boot_reads_the_verified_handle_never_the_locator_path() {
        use std::io::{Seek as _, SeekFrom, Write as _};

        // The locator names this test executable, a valid PE image without a container
        // (KELD-PACK-007 if anything reopened it). The handle is an anonymous file, with
        // no path to reopen and its cursor at end of file, holding no PE image at all.
        let locator = std::env::current_exe()
            .expect("test executable path")
            .canonicalize()
            .expect("canonical locator");
        let mut image = tempfile::tempfile().expect("anonymous image handle");
        image
            .write_all(b"MZ is not a host image")
            .expect("write the malformed image");
        image.seek(SeekFrom::End(0)).expect("cursor to end of file");
        let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
            .expect("synthetic verified identity");
        let before = startup_resource_snapshot();
        let error = validate_installed_from_verified(&locator, &image, identity)
            .expect_err("a malformed verified image is refused");
        // KELD-PACK-006 is the handle's malformed image; a locator reopen would report the
        // test executable's missing container instead.
        assert_eq!(error.code(), "KELD-UPDATE-019");
        assert!(error.to_string().contains("KELD-PACK-006"), "{error}");
        assert_eq!(
            error.resources, before,
            "installed boot advanced app resources"
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_boot_files_validate_without_the_dev_stage_dacl() {
        let temp = tempfile::tempdir().expect("temp root");
        let root = temp.path().join("tree");
        fs::create_dir(&root).expect("tree");
        fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
        fs::write(root.join("index.html"), "<p>installed</p>\n").expect("renderer");
        fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
        fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");

        // The inherited temporary-directory DACL is not owner-private: the dev route
        // still refuses this tree, so its predicate is unchanged.
        let error = must_err(validate_from_root(&root), "a non-private stage must fail");
        assert_eq!(error.code(), "KELD-CORE-036");
        assert!(error.to_string().contains("app root DACL"), "{error}");

        let selection = validate_windows_boot_files(root.clone(), WindowsBootMode::DevStage)
            .expect("the selected tree's boot files validate");
        assert_eq!(selection.app.root, root);
        assert_eq!(selection.app.name, "Fixture");
        assert_eq!(selection.app.entry_path, Path::new("main.ts"));
        assert_eq!(selection.app.renderer_html, b"<p>installed</p>\n");
        assert_eq!(
            selection.permissions_digest,
            decode_digest(DIGEST).expect("fixture digest")
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_boot_files_keep_the_strict_descriptor_and_target_rules() {
        let temp = tempfile::tempdir().expect("temp root");
        let root = temp.path().join("tree");
        fs::create_dir(&root).expect("tree");
        fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
        fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
        fs::write(root.join(BOOT_FILE), br#"{"schema":1,"foreign":true}"#).expect("boot");
        let error = must_err(
            validate_windows_boot_files(root.clone(), WindowsBootMode::DevStage),
            "a malformed descriptor must fail",
        );
        assert_eq!(error.code(), "KELD-CORE-035");

        let outside = temp.path().join("outside.html");
        fs::write(&outside, "<p>outside</p>\n").expect("outside renderer");
        std::os::windows::fs::symlink_file(&outside, root.join("index.html"))
            .expect("renderer symlink");
        fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");
        let error = must_err(
            validate_windows_boot_files(root, WindowsBootMode::DevStage),
            "a reparse-point renderer must fail",
        );
        assert_eq!(error.code(), "KELD-CORE-036");
        assert!(error.to_string().contains("reparse point"), "{error}");
    }

    #[test]
    #[cfg(windows)]
    fn windows_trust_image_refusal_is_reported_as_wv009() {
        let directory = tempfile::tempdir().expect("trust-image fixture directory");
        let target = directory.path().join("target.exe");
        let link = directory.path().join("link.exe");
        fs::write(&target, b"trust image fixture").expect("write trust-image target");
        std::os::windows::fs::symlink_file(&target, &link).expect("create a leaf symlink");

        // keld-guard refuses the leaf reparse point; keld-core owns the code.
        let error = verified_windows_image(&link).expect_err("a leaf reparse point is refused");
        assert_eq!(error.code(), "KELD-WV-009");
        assert!(error.to_string().contains("reparse point"), "{error}");
    }

    /// Authenticode FFI names that only the `keld-guard` owner may use. Split
    /// literals keep these needles from matching this test's own source.
    const AUTHENTICODE_FFI_NEEDLES: [&str; 8] = [
        concat!("WinTrust", "::"),
        concat!("Cryptography", "::"),
        concat!("WinVerify", "Trust"),
        concat!("WT", "Helper"),
        concat!("CryptDecode", "Object"),
        concat!("CryptEncode", "Object"),
        concat!("CryptQuery", "Object"),
        concat!("Crypt", "Msg"),
    ];

    fn authenticode_ffi_needles_in(source: &str) -> Vec<&'static str> {
        AUTHENTICODE_FFI_NEEDLES
            .into_iter()
            .filter(|needle| source.contains(needle))
            .collect()
    }

    /// The KEL-135 Authenticode FFI has one owner in `keld-guard` (KEL-270 D4).
    #[test]
    fn keld_core_source_holds_no_authenticode_ffi() {
        let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut scanned = 0_usize;
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).expect("read a keld-core source directory") {
                let path = entry.expect("read a keld-core source entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    let source = fs::read_to_string(&path).expect("read a keld-core source file");
                    scanned += 1;
                    let found = authenticode_ffi_needles_in(&source);
                    assert!(
                        found.is_empty(),
                        "{} names {found:?}; call the keld-guard Authenticode owner instead",
                        path.display()
                    );
                }
            }
        }
        assert!(scanned > 1, "no keld-core source files were scanned");
    }

    #[test]
    fn authenticode_ffi_scan_flags_each_needle() {
        assert!(authenticode_ffi_needles_in("fn clean() {}").is_empty());
        for needle in AUTHENTICODE_FFI_NEEDLES {
            let planted = format!("fn planted() {{ let _ = {needle}; }}");
            assert_eq!(authenticode_ffi_needles_in(&planted), [needle]);
        }
    }

    #[test]
    #[cfg(windows)]
    fn current_test_executable_is_rejected_as_a_release_carrier() {
        let executable = current_windows_executable().expect("test executable path");
        let error = verified_windows_image(&executable)
            .expect_err("the test executable is not an approved signed Keld package carrier");
        assert_eq!(error.code(), "KELD-WV-009");
        // The running image opened as a pinned trust image and was refused by its owner.
        assert!(
            error
                .to_string()
                .contains("rejected the current executable with status"),
            "{error}"
        );
    }

    /// Verifies the specified host carrier, or this signed libtest for older fixtures.
    #[test]
    #[ignore = "requires a trusted Authenticode-signed fixture executable"]
    #[cfg(windows)]
    fn kel135_signed_package_acceptance_fixture() {
        let carrier = std::env::var_os("KELD_KEL135_CARRIER_UNDER_TEST")
            .map_or_else(current_windows_executable, |path| Ok(PathBuf::from(path)))
            .expect("the signed fixture carrier path");
        let (_verified, identity) = verified_windows_image(&carrier)
            .expect("the signed fixture must produce a verified Windows identity");
        let app_id = identity.app_id.clone();
        let publisher_scope = identity.publisher_scope.iter().fold(
            String::with_capacity(identity.publisher_scope.len() * 2),
            |mut text, byte| {
                use std::fmt::Write as _;
                write!(&mut text, "{byte:02x}").expect("format publisher scope");
                text
            },
        );
        let profile_namespace = identity.profile_identity.namespace_segment();
        let mode = select_windows_profile_mode(Some(identity), false)
            .expect("the verified signed fixture must select a persistent profile");
        assert!(matches!(mode, WindowsProfileMode::Persistent(_)));
        println!(
            "KELD_KEL135_SIGNED_IDENTITY app_id={app_id} publisher_scope={publisher_scope} profile_namespace={profile_namespace}"
        );
    }

    /// Runs only from a deliberately signed copy of this libtest binary.
    #[test]
    #[ignore = "requires a trusted Authenticode-signed fixture executable"]
    #[cfg(windows)]
    fn kel135_signed_package_purge_acceptance_fixture() {
        let executable = current_windows_executable().expect("signed fixture path");
        let (_verified, identity) = verified_windows_image(&executable)
            .expect("the signed fixture must produce a verified Windows identity");
        let profile_namespace = identity.profile_identity.namespace_segment();
        WebView2Engine::purge_persistent_profile(identity.profile_identity)
            .expect("the signed fixture must purge its authenticated idle profile");
        println!("KELD_KEL135_SIGNED_PURGE profile_namespace={profile_namespace}");
    }

    #[test]
    #[cfg(windows)]
    fn quit_peer_close_wait_is_bounded_for_a_non_closing_client() {
        let (mut server, client) = primary_test_stream_pair();
        server
            .set_app_link_read_deadline(Some(Duration::from_millis(10)))
            .expect("server poll deadline");

        let started = Instant::now();
        let error = await_windows_quit_peer_close_until(
            &mut server,
            Instant::now() + Duration::from_millis(50),
        )
        .expect_err("non-closing client must hit the peer-close deadline");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "peer-close deadline did not bound the wait"
        );
        assert!(error.to_string().contains("did not close"), "{error}");
        drop(client);
    }

    #[test]
    #[cfg(windows)]
    fn dev_stage_cleanup_retries_only_transient_final_directory_sharing() {
        let temp = tempfile::tempdir().expect("cleanup fixture root");
        let root = temp.path().join("stage");
        fs::create_dir(&root).expect("cleanup stage");
        fs::write(root.join("entry"), b"fixture").expect("cleanup stage entry");

        let mut blocker = ReapedTestChild(
            Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Write-Output (Get-Location).Path; $null = [Console]::In.ReadLine()",
                ])
                .current_dir(&root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn final-directory holder"),
        );
        let mut blocker_stdin = blocker.0.stdin.take();
        let mut ready = String::new();
        BufReader::new(blocker.0.stdout.take().expect("holder readiness pipe"))
            .read_line(&mut ready)
            .expect("holder readiness");
        let observed_ready = Path::new(ready.trim_end())
            .canonicalize()
            .expect("canonical holder working directory");
        let expected_root = root.canonicalize().expect("canonical cleanup stage");
        assert_eq!(observed_ready, expected_root);

        let bare_error = fs::remove_dir_all(&root)
            .expect_err("bare Windows removal must expose the transient sharing violation");
        assert_eq!(
            bare_error.raw_os_error(),
            Some(WINDOWS_SHARING_VIOLATION),
            "{bare_error}"
        );
        assert!(root.is_dir(), "sharing failure removed the held directory");

        let mut exhausted_retry_count = 0;
        let exhausted =
            remove_windows_dev_stage_with(&root, || false, || exhausted_retry_count += 1)
                .expect_err("expired cleanup deadline must preserve the sharing violation");
        assert_eq!(
            exhausted.raw_os_error(),
            Some(WINDOWS_SHARING_VIOLATION),
            "{exhausted}"
        );
        assert_eq!(
            exhausted_retry_count, 0,
            "expired cleanup deadline entered the retry path"
        );

        let mut retry_count = 0;
        let retry_budget = Cell::new(10_000_u32);
        remove_windows_dev_stage_with(
            &root,
            || {
                let remaining = retry_budget.get();
                if remaining == 0 {
                    false
                } else {
                    retry_budget.set(remaining - 1);
                    true
                }
            },
            || {
                retry_count += 1;
                if let Some(stdin) = blocker_stdin.take() {
                    drop(stdin);
                    blocker.0.wait().expect("reap final-directory holder");
                }
                thread::yield_now();
            },
        )
        .expect("bounded cleanup after holder exit");

        assert!(retry_count > 0, "sharing violation did not exercise retry");
        assert!(!root.exists(), "cleanup left the final directory");

        let file = temp.path().join("not-a-directory");
        fs::write(&file, b"fixture").expect("non-directory fixture");
        let mut unrelated_retry_count = 0;
        let unrelated =
            remove_windows_dev_stage_with(&file, || true, || unrelated_retry_count += 1)
                .expect_err("non-directory cleanup must fail");
        assert_ne!(
            unrelated.raw_os_error(),
            Some(WINDOWS_SHARING_VIOLATION),
            "{unrelated}"
        );
        assert_eq!(
            unrelated_retry_count, 0,
            "non-sharing error entered the retry path"
        );
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
    fn kipc_transport_sidecar_admission_skips_absent_and_accepts_regular_file() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let sidecar = temp.path().join("kipc-transport.ts");
        assert!(
            !admit_kipc_transport_sidecar(&sidecar).expect("absent sidecar"),
            "NotFound must allow self-contained entries"
        );
        fs::write(&sidecar, "export {}\n").expect("regular sidecar");
        assert!(
            admit_kipc_transport_sidecar(&sidecar).expect("regular sidecar"),
            "regular file must be eligible to bind"
        );
    }

    #[test]
    fn kipc_transport_sidecar_admission_rejects_directory() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let sidecar = temp.path().join("kipc-transport.ts");
        fs::create_dir(&sidecar).expect("directory sidecar");
        let error = must_err(
            admit_kipc_transport_sidecar(&sidecar),
            "directory sidecar must fail closed",
        );
        assert!(error.to_string().contains("not a regular file"), "{error}");
    }

    #[test]
    fn fs_route_keeps_guard_evaluation_out_of_core_production_source() {
        let source = include_str!("app_session.rs");
        let production = source
            .split_once("\n#[cfg(test)]\nmod tests")
            .map(|(production, _)| production)
            .expect("tests module marker");
        let forbidden_dispatch = ["dispatch", "_privileged("].concat();
        let forbidden_evaluate = ["keld_guard::", "evaluate("].concat();
        assert!(
            !production.contains(&forbidden_dispatch),
            "keld-core must route to native rather than call the privileged dispatcher"
        );
        assert!(
            !production.contains(&forbidden_evaluate),
            "keld-core must never become a second guard evaluator"
        );
    }

    #[test]
    fn kipc_transport_sidecar_admission_uses_lstat_not_follow() {
        let source = include_str!("app_session.rs");
        let start = source
            .find("fn admit_kipc_transport_sidecar")
            .expect("admission helper");
        let body = source
            .get(start..start.saturating_add(900))
            .expect("admission helper body");
        assert!(
            body.contains("symlink_metadata"),
            "admission must lstat so a sidecar symlink is not followed into --ro-bind"
        );
        assert!(
            !body.contains(".metadata()"),
            "Path::metadata follows symlinks and would reintroduce CWE-59"
        );
        assert!(
            body.contains("is_symlink()"),
            "admission must reject a symlink before readonly_runtime"
        );
    }

    #[test]
    #[cfg(unix)]
    fn kipc_transport_sidecar_admission_rejects_symlink_to_regular_file() {
        let temp = tempfile::tempdir().expect("sidecar root");
        let outside = temp.path().join("outside.ts");
        fs::write(&outside, "export const steal = 1;\n").expect("external target");
        let sidecar = temp.path().join("kipc-transport.ts");
        std::os::unix::fs::symlink(&outside, &sidecar).expect("sidecar symlink");
        // `metadata()` follows and would admit this as a regular file — CWE-59.
        assert!(
            fs::metadata(&sidecar).expect("follow").is_file(),
            "negative control: following the symlink sees a regular file"
        );
        let error = must_err(
            admit_kipc_transport_sidecar(&sidecar),
            "symlink sidecar must fail closed before --ro-bind",
        );
        assert!(error.to_string().contains("symbolic link"), "{error}");
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
    fn guarded_primary_route_reaches_native_fs_broker_with_allow_and_deny() {
        use std::os::unix::net::UnixStream;

        use keld_ipc::link::{read_frame, write_frame};
        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("FS route temp root");
        let allowed = temp.path().join("allowed");
        fs::create_dir(&allowed).expect("allowed root");
        let scope = allowed.display().to_string().replace('\\', "/");
        let manifest_text =
            format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, &manifest_text).expect("write FS manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open FS manifest"),
            manifest_path,
            digest,
        )
        .expect("verify FS manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare FS broker");

        let (server, mut client) = UnixStream::pair().expect("primary FS pair");
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("primary FS read deadline");
        let (window_tx, _window_rx) = mpsc::channel();
        let (guardian_tx, _guardian_rx) = mpsc::channel();
        let router = PrimaryRouter::start_with_fs(
            server,
            window_tx,
            PlatformPrimaryOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
            Some(snapshot.fs_weak()),
        )
        .expect("guarded primary router");

        let target = allowed.join("route.txt");
        let target_wire = target.display().to_string().replace('\\', "/");
        let write = FsRequest::Write {
            path: target_wire.clone(),
            bytes: b"through-primary".to_vec(),
        };
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(41),
            &encode(&write).expect("encode allowed write"),
        )
        .expect("send allowed write");
        let (write_header, write_payload) = read_frame(&mut client).expect("allowed write reply");
        assert_eq!(write_header.kind, FrameKind::Reply);
        assert_eq!(write_header.channel, FS_CHANNEL);
        assert_eq!(write_header.corr, CorrelationId(41));
        assert!(matches!(
            decode::<FsResponse>(&write_payload).expect("decode write response"),
            FsResponse::Write
        ));
        assert_eq!(
            fs::read(&target).expect("read written bytes"),
            b"through-primary"
        );

        let read = FsRequest::Read { path: target_wire };
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(42),
            &encode(&read).expect("encode allowed read"),
        )
        .expect("send allowed read");
        let (read_header, read_payload) = read_frame(&mut client).expect("allowed read reply");
        assert_eq!(read_header.kind, FrameKind::Reply);
        assert_eq!(read_header.channel, FS_CHANNEL);
        assert_eq!(read_header.corr, CorrelationId(42));
        match decode::<FsResponse>(&read_payload).expect("decode read response") {
            FsResponse::Read { bytes } => assert_eq!(bytes, b"through-primary"),
            FsResponse::Write => panic!("read returned write response"),
        }

        let denied = temp.path().join("denied.txt");
        let denied_wire = denied.display().to_string().replace('\\', "/");
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(43),
            &encode(&FsRequest::Write {
                path: denied_wire,
                bytes: b"must-not-write".to_vec(),
            })
            .expect("encode denied write"),
        )
        .expect("send denied write");
        let (deny_header, deny_payload) = read_frame(&mut client).expect("denied write Err");
        assert_eq!(deny_header.kind, FrameKind::Err);
        assert_eq!(deny_header.channel, FS_CHANNEL);
        assert_eq!(deny_header.corr, CorrelationId(43));
        let error: CallError = decode(&deny_payload).expect("decode guard denial");
        assert_eq!(error.code, "KELD-GUARD002");
        assert!(!denied.exists(), "denied primary route created its target");

        router.shutdown().expect("guarded primary router shutdown");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn fs_worker_reply_write_failure_retires_generation_and_notifies_owner() {
        use std::os::unix::net::UnixStream;

        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("FS worker write-failure root");
        let allowed = temp.path().join("allowed");
        fs::create_dir(&allowed).expect("FS worker write-failure allowed root");
        let target = allowed.join("input.txt");
        fs::write(&target, b"reply write failure").expect("FS worker write-failure input");
        let scope = allowed.display().to_string().replace('\\', "/");
        let manifest_text = format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"]}}}}}}"#);
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, &manifest_text).expect("write FS worker write-failure manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open FS worker write-failure manifest"),
            manifest_path,
            digest,
        )
        .expect("verify FS worker write-failure manifest");
        let snapshot =
            GuardSnapshot::prepare(verified).expect("prepare FS worker write-failure broker");

        let (server, peer) = UnixStream::pair().expect("FS worker write-failure pair");
        drop(peer);
        let reader_stop = Arc::new(AtomicBool::new(false));
        let current = Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
            attempt: 1,
            writer: server,
            reader_stop: Arc::clone(&reader_stop),
            pending_quit: None,
        })));
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let (window_tx, window_rx) = mpsc::channel();
        let shutdown = SessionShutdownState::new();
        let handle = PrimaryRouterHandle {
            current: Arc::clone(&current),
            readers: Arc::new(Mutex::new(HashMap::new())),
            pending_echo: Arc::new(Mutex::new(None)),
            pending_echo_attempt: Arc::new(AtomicU32::new(0)),
            pending_echo_corr: Arc::new(AtomicU32::new(0)),
            next_host_corr: Arc::new(AtomicU32::new(1)),
            window_ready: Arc::new(AtomicBool::new(true)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(true)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown,
            fs: Some(snapshot.fs_weak()),
            fs_worker_commands: None,
            guardian: GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            window_commands: window_tx,
            #[cfg(target_os = "macos")]
            quit_drain_hooks: Arc::default(),
        };

        let pending = PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(90),
        };
        let admitted = snapshot
            .fs
            .begin_call(pending)
            .expect("admit FS worker write-failure request")
            .expect("FS worker write-failure admission remains open");
        let work = FsWorkItem {
            admitted,
            request: FsRequest::Read {
                path: target.display().to_string().replace('\\', "/"),
            },
            attempt: 1,
            correlation: CorrelationId(90),
            cancellation: Arc::new(AtomicBool::new(false)),
        };
        let (commands, receiver) = mpsc::channel();
        commands
            .send(FsWorkerCommand::Work(work))
            .expect("queue FS worker write-failure work");
        commands
            .send(FsWorkerCommand::Stop)
            .expect("queue worker stop after failed reply");

        let owner = thread::spawn(move || {
            let command = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("retired FS generation recovery request");
            let GuardianOwnerCommand::FailRetiredGeneration(attempt, reply) = command else {
                panic!("failed FS reply used the wrong generation-owner command");
            };
            assert_eq!(attempt, 1);
            reply
                .send(Ok(()))
                .expect("acknowledge retired FS generation recovery");
        });

        run_fs_worker(&receiver, &handle, None)
            .expect("reply write failure must retire the generation without killing the worker");
        owner.join().expect("retired-generation owner joins");

        assert!(
            current.lock().expect("current generation").is_none(),
            "failed terminal write left the corrupted generation installed"
        );
        assert!(
            reader_stop.load(Ordering::Acquire),
            "failed terminal write did not stop the retired generation reader"
        );
        assert!(
            matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "recoverable FS reply failure incorrectly fataled the app session"
        );
        let state = snapshot
            .fs
            .state
            .lock()
            .expect("FS state after failed reply");
        assert_eq!(state.in_flight, 0);
        assert!(state.pending_call.is_none());
        assert!(state.failed_write_attempt.is_none());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn successor_install_does_not_wait_for_retired_fs_recovery_ack() {
        use std::os::unix::net::UnixStream;

        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("FS recovery ordering root");
        let allowed = temp.path().join("allowed");
        fs::create_dir(&allowed).expect("FS recovery ordering allowed root");
        let target = allowed.join("input.txt");
        fs::write(&target, b"recovery ordering").expect("FS recovery ordering input");
        let scope = allowed.display().to_string().replace('\\', "/");
        let manifest_text = format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"]}}}}}}"#);
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, &manifest_text).expect("write FS recovery ordering manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open FS recovery ordering manifest"),
            manifest_path,
            digest,
        )
        .expect("verify FS recovery ordering manifest");
        let snapshot =
            GuardSnapshot::prepare(verified).expect("prepare FS recovery ordering broker");

        let (g1_server, g1_peer) = UnixStream::pair().expect("G1 recovery ordering pair");
        drop(g1_peer);
        let reader_stop = Arc::new(AtomicBool::new(false));
        let current = Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
            attempt: 1,
            writer: g1_server,
            reader_stop: Arc::clone(&reader_stop),
            pending_quit: None,
        })));
        let (guardian_tx, guardian_rx) = mpsc::channel();
        let (window_tx, window_rx) = mpsc::channel();
        let handle = PrimaryRouterHandle {
            current: Arc::clone(&current),
            readers: Arc::new(Mutex::new(HashMap::new())),
            pending_echo: Arc::new(Mutex::new(None)),
            pending_echo_attempt: Arc::new(AtomicU32::new(0)),
            pending_echo_corr: Arc::new(AtomicU32::new(0)),
            next_host_corr: Arc::new(AtomicU32::new(1)),
            window_ready: Arc::new(AtomicBool::new(false)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(true)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown: SessionShutdownState::new(),
            fs: Some(snapshot.fs_weak()),
            fs_worker_commands: None,
            guardian: GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            window_commands: window_tx,
            #[cfg(target_os = "macos")]
            quit_drain_hooks: Arc::default(),
        };
        let router = PrimaryRouter {
            handle: handle.clone(),
            fs_worker: None,
        };

        let admitted = snapshot
            .fs
            .begin_call(PendingFsCall {
                attempt: 1,
                correlation: CorrelationId(120),
            })
            .expect("admit G1 recovery-ordering FS call")
            .expect("G1 recovery-ordering admission remains open");
        let work = FsWorkItem {
            admitted,
            request: FsRequest::Read {
                path: target.display().to_string().replace('\\', "/"),
            },
            attempt: 1,
            correlation: CorrelationId(120),
            cancellation: Arc::new(AtomicBool::new(false)),
        };
        let (commands, receiver) = mpsc::channel();
        commands
            .send(FsWorkerCommand::Work(work))
            .expect("queue recovery-ordering FS work");
        commands
            .send(FsWorkerCommand::Stop)
            .expect("queue recovery-ordering worker stop");

        let worker_handle = handle.clone();
        let worker = thread::spawn(move || run_fs_worker(&receiver, &worker_handle, None));
        let command = guardian_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("failed terminal write must request retired-generation recovery");
        let GuardianOwnerCommand::FailRetiredGeneration(attempt, recovery_reply) = command else {
            panic!("failed terminal write used the wrong owner command");
        };
        assert_eq!(attempt, 1);
        assert_eq!(
            snapshot
                .fs
                .state
                .lock()
                .expect("failed-write recovery state")
                .failed_write_attempt,
            Some(1),
            "failed-write recovery marker was not published before owner notification"
        );
        assert!(
            current
                .lock()
                .expect("retired current generation")
                .is_none(),
            "failed terminal write did not retire G1 before owner notification"
        );

        let (g2_server, _g2_client) = UnixStream::pair().expect("G2 recovery ordering pair");
        let g2_handle = handle.clone();
        let (installed_tx, installed_rx) = mpsc::sync_channel(1);
        let install = thread::spawn(move || {
            let result = g2_handle.install_generation(2, g2_server);
            installed_tx.send(result).expect("report G2 installation");
        });

        let install_report = installed_rx.recv_timeout(Duration::from_secs(2));
        if install_report.is_err() {
            let _ = recovery_reply.send(Ok(()));
            let _ = worker.join();
            let _ = install.join();
            panic!("G2 installation waited for the retired-generation recovery acknowledgment");
        }
        install_report
            .expect("G2 installation report")
            .expect("G2 installs while owner recovery acknowledgment is pending");
        assert!(
            matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "recovery ordering sent an unexpected fatal window command"
        );

        recovery_reply
            .send(Ok(()))
            .expect("acknowledge retired-generation recovery after G2 installation");
        worker
            .join()
            .expect("recovery-ordering worker joins")
            .expect("recovery-ordering worker stays healthy");
        install.join().expect("G2 installer joins");
        router
            .shutdown()
            .expect("recovery-ordering router shutdown");
    }

    /// One guarded primary router over a socket pair, with the FS worker held
    /// by a test gate after it takes its job (GH-528 T2 router tests).
    #[cfg(target_os = "macos")]
    struct GuardedTestRouter {
        router: PrimaryRouter,
        snapshot: GuardSnapshot,
        allowed: PathBuf,
        taken: Receiver<()>,
        release: Sender<()>,
        window: Receiver<AppWindowCommand>,
        guardian: Receiver<TestPrimaryOwnerCommand>,
        _temp: tempfile::TempDir,
    }

    /// A guarded router over a socket pair, and the pair's client end.
    #[cfg(target_os = "macos")]
    fn guarded_test_router() -> (GuardedTestRouter, std::os::unix::net::UnixStream) {
        use keld_ipc::link::AppLinkDeadlines as _;

        let (server, client) = std::os::unix::net::UnixStream::pair().expect("guarded pair");
        client
            .set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("guarded client deadlines");
        (guarded_router(server), client)
    }

    /// A guarded router over `server`: a pair end, or a Bun role's
    /// authenticated app link.
    #[cfg(target_os = "macos")]
    fn guarded_router(server: std::os::unix::net::UnixStream) -> GuardedTestRouter {
        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("guarded router root");
        let allowed = temp.path().join("allowed");
        fs::create_dir(&allowed).expect("guarded router allowed root");
        let scope = allowed.display().to_string().replace('\\', "/");
        let manifest_text =
            format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, &manifest_text).expect("write guarded router manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open guarded router manifest"),
            manifest_path,
            digest,
        )
        .expect("verify guarded router manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare guarded router broker");
        let (taken_tx, taken) = mpsc::sync_channel(1);
        let (release, release_rx) = mpsc::channel();
        let gate = Arc::new(FsWorkerTestGate {
            taken: taken_tx,
            release: Mutex::new(release_rx),
        });
        let (window_tx, window) = mpsc::channel();
        let (guardian_tx, guardian) = mpsc::channel();
        let router = PrimaryRouter::start_with_fs_test_gate(
            server,
            window_tx,
            PlatformPrimaryOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
            Some(snapshot.fs_weak()),
            Some(gate),
        )
        .expect("guarded test router");
        GuardedTestRouter {
            router,
            snapshot,
            allowed,
            taken,
            release,
            window,
            guardian,
            _temp: temp,
        }
    }

    /// GH-528 T2 end-to-end cases: a Bun `WorkerLink` role against this
    /// router (`app_session/tests/worker_link_e2e.rs`).
    #[cfg(target_os = "macos")]
    mod worker_link_e2e;

    #[cfg(target_os = "macos")]
    fn write_fs_call(client: &mut std::os::unix::net::UnixStream, corr: u32, target: &Path) {
        write_frame(
            client,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(corr),
            &encode(&FsRequest::Write {
                path: target.display().to_string().replace('\\', "/"),
                bytes: b"t2".to_vec(),
            })
            .expect("encode FS write"),
        )
        .expect("send FS write");
    }

    #[cfg(target_os = "macos")]
    fn write_quit_call(client: &mut std::os::unix::net::UnixStream, corr: u32) {
        write_frame(
            client,
            FrameKind::Call,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(corr),
            &encode(&LifecycleRequest::Quit).expect("encode Quit"),
        )
        .expect("send Quit");
    }

    /// Every frame the client reads before the link ends (EOF or reset).
    #[cfg(target_os = "macos")]
    fn read_frames_until_eof(
        client: &mut std::os::unix::net::UnixStream,
    ) -> Vec<(keld_ipc::FrameHeader, Vec<u8>)> {
        let mut frames = Vec::new();
        while let Ok(frame) = keld_ipc::link::read_frame(client) {
            frames.push(frame);
        }
        frames
    }

    /// GH-528 T2, spec gh527 criterion 6: retiring a generation answers its
    /// pending calls on ERR-declaring channels with `KELD-IPC-023` before the
    /// link closes. An admitted FS call held in its handler, and a Quit waiting
    /// in its FS drain, are both pending; an echo call is answered at once and
    /// never gets an ERR.
    #[test]
    #[cfg(target_os = "macos")]
    fn retire_answers_pending_fs_and_quit_calls_with_023_before_close() {
        let (t, mut client) = guarded_test_router();
        assert_echo_call(&mut client, 10, "answered before retire");
        write_fs_call(&mut client, 11, &t.allowed.join("held.txt"));
        t.taken
            .recv_timeout(Duration::from_secs(5))
            .expect("worker took the FS job");
        let (drain_tx, drain_rx) = mpsc::sync_channel(1);
        t.snapshot.fs.observe_next_drain_wait(drain_tx);
        write_quit_call(&mut client, 12);
        drain_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("Quit waits in its FS drain");

        t.router.handle().retire_generation(1).expect("retire g1");
        let frames = read_frames_until_eof(&mut client);
        let answers: Vec<(FrameKind, keld_ipc::ChannelId, CorrelationId, String)> = frames
            .iter()
            .map(|(header, payload)| {
                let error: CallError = decode(payload).expect("retire answer is a CallError");
                (header.kind, header.channel, header.corr, error.code)
            })
            .collect();
        assert_eq!(
            answers,
            vec![
                (
                    FrameKind::Err,
                    FS_CHANNEL,
                    CorrelationId(11),
                    "KELD-IPC-023".to_owned()
                ),
                (
                    FrameKind::Err,
                    LIFECYCLE_CHANNEL,
                    CorrelationId(12),
                    "KELD-IPC-023".to_owned()
                ),
            ],
            "exactly the two pending calls are answered, then the link closes"
        );

        t.release.send(()).expect("release FS worker");
        t.router.shutdown().expect("router shutdown after retire");
        assert!(matches!(
            t.guardian.try_recv(),
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected)
        ));
    }

    /// GH-528 T2: a peer that stops reading cannot hold an accepted Quit open.
    /// The client floods CALLs behind its Quit and never reads, so the host's
    /// `KELD-IPC-024` answers fill both socket buffers. Each answer is one
    /// best-effort write bounded by `HOST_ANSWER_BUDGET`, and the first that
    /// times out ends the drain, so the Quit tail reaches its guardian
    /// shutdown well before one writer deadline. The bound under test is
    /// itself a time limit; the margin is one budget plus a poll (under 0.5 s)
    /// against the 5 s `APP_LINK_IO_DEADLINE` that an unbounded write would
    /// wait.
    #[test]
    #[cfg(target_os = "macos")]
    fn quit_drain_answers_are_bounded_when_the_peer_stops_reading() {
        let (t, client) = guarded_test_router();
        let mut writer = client.try_clone().expect("flooding writer");
        let target = t.allowed.join("never.txt");
        let flood = std::thread::spawn(move || {
            write_quit_call(&mut writer, 30);
            for corr in 31..1_031 {
                let frame = write_frame(
                    &mut writer,
                    FrameKind::Call,
                    0,
                    FS_CHANNEL,
                    CorrelationId(corr),
                    &encode(&FsRequest::Write {
                        path: target.display().to_string().replace('\\', "/"),
                        bytes: b"t2".to_vec(),
                    })
                    .expect("encode flooding FS write"),
                );
                // Writes stop once the host closes the link.
                if frame.is_err() {
                    break;
                }
            }
        });
        let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("Quit attribution")
        else {
            panic!("Quit skipped shutdown attribution");
        };
        prepare.send(Ok(())).expect("acknowledge attribution");
        let bound = APP_LINK_IO_DEADLINE
            .checked_sub(Duration::from_secs(1))
            .expect("writer deadline exceeds one second");
        let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
            .guardian
            .recv_timeout(bound)
            .expect("the Quit tail reached its shutdown before one writer deadline")
        else {
            panic!("unexpected guardian command after the Quit REPLY");
        };
        shutdown
            .send(Ok(()))
            .expect("acknowledge guardian shutdown");
        assert_eq!(
            t.window
                .recv_timeout(Duration::from_secs(5))
                .expect("UI Quit"),
            AppWindowCommand::Quit
        );
        drop(client);
        flood.join().expect("flooding writer joins");
        t.router.shutdown().expect("router shutdown after Quit");
    }

    /// GH-528 T2, spec gh527 criterion 7: after an accepted Quit's real REPLY
    /// and its FS drain, a CALL already received on an ERR-declaring channel is
    /// answered with `KELD-IPC-024` and never executed; an echo CALL gets no
    /// frame (KEL-133 keeps echo REPLY-only). The host still closes the link.
    #[test]
    #[cfg(target_os = "macos")]
    fn quit_drain_answers_received_calls_with_024_and_runs_none() {
        let (t, mut client) = guarded_test_router();
        let target = t.allowed.join("after-quit.txt");
        // All three frames are buffered before the host can finish the Quit:
        // its REPLY waits on the shutdown attribution acknowledged below.
        write_quit_call(&mut client, 20);
        write_fs_call(&mut client, 21, &target);
        let echo = keld_ipc::echo::EchoRequest {
            message: "after quit".to_owned(),
            count: 22,
        };
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            ECHO_CHANNEL,
            CorrelationId(22),
            &encode(&echo).expect("encode echo"),
        )
        .expect("send echo after Quit");

        let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("Quit attribution")
        else {
            panic!("Quit skipped shutdown attribution");
        };
        prepare.send(Ok(())).expect("acknowledge attribution");
        let (quit, quit_payload) = keld_ipc::link::read_frame(&mut client).expect("Quit REPLY");
        assert_eq!(
            (quit.kind, quit.channel, quit.corr),
            (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(20))
        );
        assert_eq!(
            decode::<LifecycleResponse>(&quit_payload).expect("Quit response"),
            LifecycleResponse::Quit
        );
        let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian shutdown")
        else {
            panic!("unexpected guardian command after the Quit REPLY");
        };
        shutdown
            .send(Ok(()))
            .expect("acknowledge guardian shutdown");
        let after = read_frames_until_eof(&mut client);
        assert_eq!(after.len(), 1, "only the FS CALL is answered: {after:?}");
        let (header, payload) = &after[0];
        assert_eq!(
            (header.kind, header.channel, header.corr),
            (FrameKind::Err, FS_CHANNEL, CorrelationId(21))
        );
        assert_eq!(
            decode::<CallError>(payload).expect("024 CallError").code,
            "KELD-IPC-024"
        );
        assert!(!target.exists(), "a post-Quit FS CALL must not execute");
        assert_eq!(
            t.window
                .recv_timeout(Duration::from_secs(5))
                .expect("UI Quit"),
            AppWindowCommand::Quit
        );
        t.router.shutdown().expect("router shutdown after Quit");
    }

    /// GH-528 T2 (gate review of #636): the post-Quit drain ends on the peer's
    /// EOF and never drops a CALL that has already arrived. Three FS CALLs are
    /// buffered behind the Quit; the drain is then held before its first read
    /// and given an idle deadline that has already passed, as a host stall
    /// longer than its idle backstop would leave it, and only then does the
    /// role end the link. Every buffered call still gets `KELD-IPC-024`, in
    /// order, and none runs. *Negative control:* checking the idle deadline
    /// before every read (as the earlier timer-window drain did) answers none
    /// of them.
    #[test]
    #[cfg(target_os = "macos")]
    fn quit_drain_answers_every_buffered_call_after_a_host_stall_then_eof() {
        let (t, mut client) = guarded_test_router();
        let (stalled_tx, stalled) = mpsc::sync_channel(1);
        let (resume, resume_rx) = mpsc::channel();
        t.router
            .handle()
            .stall_next_quit_drain(stalled_tx, resume_rx);
        let target = t.allowed.join("stalled.txt");
        write_quit_call(&mut client, 50);
        for corr in 51..=53 {
            write_fs_call(&mut client, corr, &target);
        }
        let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("Quit attribution")
        else {
            panic!("Quit skipped shutdown attribution");
        };
        prepare.send(Ok(())).expect("acknowledge attribution");
        let (quit, _) = keld_ipc::link::read_frame(&mut client).expect("Quit REPLY");
        assert_eq!(
            (quit.kind, quit.channel, quit.corr),
            (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(50))
        );
        stalled
            .recv_timeout(Duration::from_secs(5))
            .expect("the drain fixed its deadlines and stalled");
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("the role ends the link after the Quit REPLY");
        resume.send(()).expect("resume the stalled drain");

        // The drain ends on that EOF, and the host's link close reaches the
        // half-closed role before its tail asks the guardian to stop it.
        let answers: Vec<(FrameKind, keld_ipc::ChannelId, CorrelationId, String)> =
            read_frames_until_eof(&mut client)
                .iter()
                .map(|(header, payload)| {
                    let error: CallError = decode(payload).expect("drain answer is a CallError");
                    (header.kind, header.channel, header.corr, error.code)
                })
                .collect();
        let expected: Vec<_> = (51..=53)
            .map(|corr| {
                (
                    FrameKind::Err,
                    FS_CHANNEL,
                    CorrelationId(corr),
                    "KELD-IPC-024".to_owned(),
                )
            })
            .collect();
        assert_eq!(
            answers, expected,
            "every CALL written before the role ended the link gets 024"
        );
        assert!(!target.exists(), "a post-Quit FS CALL must not execute");
        let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("guardian shutdown after the drain")
        else {
            panic!("unexpected guardian command after the Quit drain");
        };
        shutdown
            .send(Ok(()))
            .expect("acknowledge guardian shutdown");
        assert_eq!(
            t.window
                .recv_timeout(Duration::from_secs(5))
                .expect("UI Quit"),
            AppWindowCommand::Quit
        );
        t.router.shutdown().expect("router shutdown after Quit");
    }

    /// GH-528 (#636 follow-up): a role that half-closes after its Quit REPLY
    /// reads EOF from the host's link close itself, before the guardian is
    /// asked to stop it. The Quit tail sends the guardian `Shutdown` only
    /// after the link close (KEL-139 AC6), and the test does not acknowledge
    /// it until after the probe, so neither a kill nor the host dropping its
    /// link handles can have produced this EOF. The probe is a nonblocking
    /// read, so the result does not depend on a timeout. *Negative control:*
    /// `shutdown(Both)` in `shutdown_app_link` is `ENOTCONN` after the
    /// half-close on macOS and sends no FIN, so the probe reads `WouldBlock`.
    #[test]
    #[cfg(target_os = "macos")]
    fn quit_close_reaches_a_half_closed_role_before_the_guardian_stops_it() {
        use std::io::Read as _;

        let (t, mut client) = guarded_test_router();
        write_quit_call(&mut client, 80);
        let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("Quit attribution")
        else {
            panic!("Quit skipped shutdown attribution");
        };
        prepare.send(Ok(())).expect("acknowledge attribution");
        let (quit, _) = keld_ipc::link::read_frame(&mut client).expect("Quit REPLY");
        assert_eq!(
            (quit.kind, quit.channel, quit.corr),
            (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(80))
        );
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("the role ends the link after the Quit REPLY");

        let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("the Quit tail reached the guardian after the link close")
        else {
            panic!("unexpected guardian command after the Quit REPLY");
        };
        client.set_nonblocking(true).expect("nonblocking probe");
        let mut byte = [0_u8; 1];
        let probe = client.read(&mut byte);
        assert!(
            matches!(probe, Ok(0)),
            "the role reads the host's EOF before the guardian stops it: {probe:?}"
        );
        client.set_nonblocking(false).expect("blocking link");

        shutdown
            .send(Ok(()))
            .expect("acknowledge guardian shutdown");
        assert_eq!(
            t.window
                .recv_timeout(Duration::from_secs(5))
                .expect("UI Quit"),
            AppWindowCommand::Quit
        );
        t.router.shutdown().expect("router shutdown after Quit");
    }

    /// GH-528 T3 (#636 gate review): a role that fully closes right after its
    /// Quit REPLY can leave a CALL it wrote behind the Quit in the host's
    /// buffer. The drain still reads it, its `KELD-IPC-024` finds the role gone
    /// (`EPIPE`), and the drain ends quietly as a lost link: the Quit tail goes
    /// on to the guardian's shutdown and the UI Quit, with no link-failure
    /// report and no `Fatal`. The drain is held before its first read until
    /// the role has closed, so the answer's failure is certain. *Negative
    /// control:* a failed answer that propagates as a session error sends the
    /// UI `Fatal` instead of `Quit`.
    #[test]
    #[cfg(target_os = "macos")]
    fn quit_drain_ends_quietly_when_its_answer_finds_the_role_gone() {
        let (t, mut client) = guarded_test_router();
        let (stalled_tx, stalled) = mpsc::sync_channel(1);
        let (resume, resume_rx) = mpsc::channel();
        let (end_tx, drain_end) = mpsc::sync_channel(1);
        let handle = t.router.handle();
        handle.stall_next_quit_drain(stalled_tx, resume_rx);
        handle.observe_next_quit_drain_end(end_tx);
        let target = t.allowed.join("behind.txt");
        write_quit_call(&mut client, 90);
        write_fs_call(&mut client, 91, &target);
        let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare) = t
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("Quit attribution")
        else {
            panic!("Quit skipped shutdown attribution");
        };
        prepare.send(Ok(())).expect("acknowledge attribution");
        let (quit, _) = keld_ipc::link::read_frame(&mut client).expect("Quit REPLY");
        assert_eq!(
            (quit.kind, quit.channel, quit.corr),
            (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(90))
        );
        stalled
            .recv_timeout(Duration::from_secs(5))
            .expect("the drain stalled before its first read");
        drop(client);
        resume.send(()).expect("resume the drain");

        assert_eq!(
            drain_end
                .recv_timeout(Duration::from_secs(5))
                .expect("the post-Quit drain ended"),
            QuitDrainEnd::AnswerLost
        );
        let TestPrimaryOwnerCommand::Shutdown(shutdown) = t
            .guardian
            .recv_timeout(Duration::from_secs(5))
            .expect("the Quit tail reaches the guardian, not a link failure")
        else {
            panic!("a lost post-Quit answer must not report a link failure");
        };
        shutdown
            .send(Ok(()))
            .expect("acknowledge guardian shutdown");
        assert_eq!(
            t.window
                .recv_timeout(Duration::from_secs(5))
                .expect("UI Quit"),
            AppWindowCommand::Quit
        );
        assert!(!target.exists(), "a post-Quit FS CALL must not execute");
        t.router
            .shutdown()
            .expect("the accepted Quit stays a clean shutdown");
    }

    /// GH-528 T2 (gate review of #636, L2): a peer that stops reading cannot
    /// hold a retirement, and its generation lock, for the writer's deadline.
    /// The host-to-role direction is full before retirement and nothing reads
    /// it, so the `KELD-IPC-023` for the held FS call cannot be written. It is
    /// written under `HOST_ANSWER_BUDGET`, its failure marks the link lost,
    /// and retirement still succeeds and closes the link: the role reads the
    /// bytes queued before retirement, then the close, and no `ERR` byte.
    /// *Negative control:* leaving the writer's deadline in place fails the
    /// deadline assertion (and holds the lock for `APP_LINK_IO_DEADLINE`).
    #[test]
    #[cfg(target_os = "macos")]
    fn retire_answers_are_bounded_when_the_peer_stops_reading() {
        use std::io::{ErrorKind, Read as _, Write as _};

        use keld_ipc::link::AppLinkDeadlines as _;

        let (server, mut client) = std::os::unix::net::UnixStream::pair().expect("pair");
        client
            .set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("client deadlines");
        server.set_nonblocking(true).expect("nonblocking fill");
        let mut queued = 0_usize;
        let chunk = [0_u8; 4096];
        for size in [chunk.len(), 1] {
            loop {
                match (&server).write(&chunk[..size]) {
                    Ok(written) => queued += written,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) => panic!("fill the host-to-role direction: {error}"),
                }
            }
        }
        server.set_nonblocking(false).expect("blocking link");
        let probe = server.try_clone().expect("deadline probe");
        let t = guarded_router(server);
        write_fs_call(&mut client, 70, &t.allowed.join("held.txt"));
        t.taken
            .recv_timeout(Duration::from_secs(5))
            .expect("worker took the FS job");

        t.router
            .handle()
            .retire_generation(1)
            .expect("retirement succeeds although its ERR cannot be written");
        assert_eq!(
            probe.app_link_write_deadline().expect("write deadline"),
            Some(HOST_ANSWER_BUDGET),
            "the 023 write ran under the answer budget, not the writer's deadline"
        );
        let mut received = Vec::new();
        client
            .read_to_end(&mut received)
            .expect("the role reads to the close");
        assert_eq!(
            received.len(),
            queued,
            "the role reads the bytes queued before retirement, then the close"
        );

        t.release.send(()).expect("release FS worker");
        t.router.shutdown().expect("router shutdown after retire");
    }

    /// GH-528 T2 (gate review of #636, L4): a real reply written first wins.
    /// The FS worker writes its real REPLY and is held after it releases the
    /// generation lock, with its lease still alive; a retirement then runs and
    /// sends no `KELD-IPC-023` for the answered call. *Negative control:*
    /// removing the terminal write's `retire_pending_call` (so the lease drop
    /// alone clears the pending call) makes this retirement send a 023 after
    /// the REPLY.
    #[test]
    #[cfg(target_os = "macos")]
    fn retire_after_a_real_reply_sends_no_023() {
        let (t, mut client) = guarded_test_router();
        let target = t.allowed.join("replied.txt");
        let (written_tx, written) = mpsc::sync_channel(1);
        let (resume, resume_rx) = mpsc::channel();
        t.snapshot
            .fs
            .hold_next_terminal_write(written_tx, resume_rx);
        write_fs_call(&mut client, 60, &target);
        t.taken
            .recv_timeout(Duration::from_secs(5))
            .expect("worker took the FS job");
        t.release.send(()).expect("release FS worker");
        written
            .recv_timeout(Duration::from_secs(5))
            .expect("the FS REPLY was written");
        let (reply, _) = keld_ipc::link::read_frame(&mut client).expect("real FS REPLY");
        assert_eq!(
            (reply.kind, reply.channel, reply.corr),
            (FrameKind::Reply, FS_CHANNEL, CorrelationId(60))
        );

        t.router.handle().retire_generation(1).expect("retire g1");
        resume.send(()).expect("resume the FS worker");
        let after = read_frames_until_eof(&mut client);
        assert!(
            after.is_empty(),
            "an answered call gets no KELD-IPC-023: {after:?}"
        );
        assert!(target.exists(), "the answered FS write ran");
        t.router.shutdown().expect("router shutdown after retire");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn fs_worker_keeps_ping_responsive_and_quit_waits_for_fs_terminal_outcome() {
        use std::os::unix::net::UnixStream;

        use keld_ipc::link::{AppLinkDeadlines as _, read_frame, write_frame};
        use sha2::{Digest as _, Sha256};

        for (case, denied) in [("allowed", false), ("denied", true)] {
            let temp = tempfile::tempdir().expect("FS worker gate root");
            let allowed = temp.path().join("allowed");
            fs::create_dir(&allowed).expect("FS worker allowed root");
            let target = if denied {
                temp.path().join("denied-held.txt")
            } else {
                allowed.join("held.txt")
            };
            let scope = allowed.display().to_string().replace('\\', "/");
            let manifest_text =
                format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
            let manifest_path = temp.path().join(PERMISSIONS_FILE);
            fs::write(&manifest_path, &manifest_text).expect("write FS worker manifest");
            let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
            let verified = load_verified_manifest(
                File::open(&manifest_path).expect("open FS worker manifest"),
                manifest_path,
                digest,
            )
            .expect("verify FS worker manifest");
            let snapshot = GuardSnapshot::prepare(verified).expect("prepare FS worker broker");

            let (taken_tx, taken_rx) = mpsc::sync_channel(1);
            let (release_tx, release_rx) = mpsc::channel();
            let gate = Arc::new(FsWorkerTestGate {
                taken: taken_tx,
                release: Mutex::new(release_rx),
            });
            let (server, mut client) = UnixStream::pair().expect("FS worker primary pair");
            client
                .set_app_link_deadlines(Some(Duration::from_secs(5)))
                .expect("FS worker client deadlines");
            let (window_tx, window_rx) = mpsc::channel();
            let (guardian_tx, guardian_rx) = mpsc::channel();
            let router = PrimaryRouter::start_with_fs_test_gate(
                server,
                window_tx,
                PlatformPrimaryOwnerHandle {
                    command_tx: guardian_tx,
                },
                SessionShutdownState::new(),
                Some(snapshot.fs_weak()),
                Some(gate),
            )
            .expect("gated FS worker router");

            let committed = format!("{case} committed before quit").into_bytes();
            let base = if denied { 70 } else { 60 };
            write_frame(
                &mut client,
                FrameKind::Call,
                0,
                FS_CHANNEL,
                CorrelationId(base),
                &encode(&FsRequest::Write {
                    path: target.display().to_string().replace('\\', "/"),
                    bytes: committed.clone(),
                })
                .expect("encode gated FS write"),
            )
            .expect("send gated FS write");
            taken_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("worker took FS job");

            write_frame(
                &mut client,
                FrameKind::Ping,
                0,
                keld_ipc::ChannelId(99),
                CorrelationId(base + 1),
                &[],
            )
            .expect("send Ping while FS worker is held");
            let (quit_drain_tx, quit_drain_rx) = mpsc::sync_channel(1);
            snapshot.fs.observe_next_drain_wait(quit_drain_tx);
            write_frame(
                &mut client,
                FrameKind::Call,
                0,
                LIFECYCLE_CHANNEL,
                CorrelationId(base + 2),
                &encode(&LifecycleRequest::Quit).expect("encode held-worker Quit"),
            )
            .expect("send Quit while FS worker is held");

            let (ping, ping_payload) =
                read_frame(&mut client).expect("Ping while FS worker is held");
            assert_eq!(ping.kind, FrameKind::Ping, "{case}");
            assert_eq!(ping.channel, keld_ipc::ChannelId(99), "{case}");
            assert_eq!(ping.corr, CorrelationId(base + 1), "{case}");
            assert!(ping_payload.is_empty(), "{case}");
            assert!(
                matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
                "{case}: UI Quit occurred while admitted FS work was still held"
            );

            quit_drain_rx
                .recv_timeout(Duration::from_secs(2))
                .unwrap_or_else(|error| {
                    panic!("{case}: Quit did not reach FS drain before worker release: {error}")
                });
            assert!(
                router.handle.shutdown.is_running(),
                "{case}: Quit claimed shutdown before the admitted FS terminal outcome"
            );
            assert!(
                matches!(guardian_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
                "{case}: shutdown attribution began before the admitted FS terminal outcome"
            );
            assert!(
                !target.exists(),
                "{case}: held native operation changed the target before worker release"
            );

            release_tx.send(()).expect("release FS worker");

            let prepare = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("Quit preparation after FS terminal write");
            let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(prepare_reply) = prepare else {
                panic!("{case}: FS worker Quit skipped shutdown attribution");
            };
            prepare_reply
                .send(Ok(()))
                .expect("acknowledge FS worker shutdown attribution");

            let (fs_terminal, fs_payload) =
                read_frame(&mut client).expect("FS terminal outcome before Quit reply");
            assert_eq!(fs_terminal.channel, FS_CHANNEL, "{case}");
            assert_eq!(fs_terminal.corr, CorrelationId(base), "{case}");
            if denied {
                assert_eq!(fs_terminal.kind, FrameKind::Err, "{case}");
                let error: CallError =
                    decode(&fs_payload).expect("decode denied FS terminal outcome");
                assert_eq!(error.code, "KELD-GUARD002", "{case}");
                assert!(
                    !target.exists(),
                    "{case}: denied write produced a filesystem effect"
                );
            } else {
                assert_eq!(fs_terminal.kind, FrameKind::Reply, "{case}");
                assert!(matches!(
                    decode::<FsResponse>(&fs_payload).expect("decode FS terminal outcome"),
                    FsResponse::Write
                ));
                assert_eq!(
                    fs::read(&target).expect("read committed FS bytes"),
                    committed,
                    "{case}: write effect and terminal FS outcome diverged"
                );
            }

            let (quit, quit_payload) =
                read_frame(&mut client).expect("Quit reply after FS terminal");
            assert_eq!(quit.kind, FrameKind::Reply, "{case}");
            assert_eq!(quit.channel, LIFECYCLE_CHANNEL, "{case}");
            assert_eq!(quit.corr, CorrelationId(base + 2), "{case}");
            assert_eq!(
                decode::<LifecycleResponse>(&quit_payload).expect("decode held-worker Quit reply"),
                LifecycleResponse::Quit,
                "{case}"
            );

            let shutdown = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("FS worker guardian shutdown after drain");
            let TestPrimaryOwnerCommand::Shutdown(shutdown_reply) = shutdown else {
                panic!("{case}: FS worker Quit sent unexpected guardian command");
            };
            shutdown_reply
                .send(Ok(()))
                .expect("complete FS worker guardian shutdown");
            assert_eq!(
                window_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("UI Quit after FS terminal outcome"),
                AppWindowCommand::Quit,
                "{case}"
            );
            router
                .shutdown()
                .expect("completed FS worker router shutdown");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn retired_call_drop_cannot_clear_successor_single_flight() {
        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("successor single-flight root");
        let manifest_text = "{}\n";
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, manifest_text).expect("write successor manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open successor manifest"),
            manifest_path,
            digest,
        )
        .expect("verify successor manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare successor broker");

        let c1_pending = PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(96),
        };
        let c1 = snapshot
            .fs
            .begin_call(c1_pending)
            .expect("admit C1")
            .expect("C1 admission remains open");
        c1.retire_pending_call().expect("retire C1 terminal marker");

        let c2_pending = PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(97),
        };
        let c2 = snapshot
            .fs
            .begin_call(c2_pending)
            .expect("admit C2 after C1 terminal retirement")
            .expect("C2 admission remains open");

        drop(c1);
        let state = snapshot.fs.state.lock().expect("state after old C1 drop");
        assert!(
            state.pending_call == Some(c2_pending),
            "old C1 drop corrupted C2 correlation ownership"
        );
        assert!(
            state.call_outstanding,
            "old C1 drop cleared C2 single-flight occupancy"
        );
        drop(state);

        let c3 = snapshot.fs.begin_call(PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(98),
        });
        assert!(
            c3.is_err(),
            "C3 was admitted after old C1 drop erased C2 occupancy"
        );

        c2.retire_pending_call().expect("retire C2");
        drop(c2);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn terminal_retirement_releases_call_occupancy_before_native_drain() {
        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("terminal occupancy root");
        let manifest_text = "{}\n";
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, manifest_text).expect("write terminal occupancy manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open terminal occupancy manifest"),
            manifest_path,
            digest,
        )
        .expect("verify terminal occupancy manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare terminal occupancy broker");
        let pending = PendingFsCall {
            attempt: 1,
            correlation: CorrelationId(99),
        };
        let admitted = snapshot
            .fs
            .begin_call(pending)
            .expect("admit terminal occupancy call")
            .expect("terminal occupancy admission remains open");
        assert!(snapshot.fs.has_outstanding_call_for(1));
        assert_eq!(
            snapshot
                .fs
                .state
                .lock()
                .expect("pre-terminal state")
                .in_flight,
            1
        );

        admitted
            .retire_pending_call()
            .expect("retire terminal correlation before reply");
        assert!(
            !snapshot.fs.has_outstanding_call_for(1),
            "terminal correlation retirement must release single-flight occupancy before the native lease drains"
        );
        assert_eq!(
            snapshot
                .fs
                .state
                .lock()
                .expect("post-terminal state")
                .in_flight,
            1,
            "drain accounting must remain held until the native lease is dropped"
        );

        drop(admitted);
        snapshot.fs.drain().expect("terminal occupancy drain");
    }

    #[cfg(target_os = "macos")]
    fn successor_drain_test_router(
        snapshot: &GuardSnapshot,
        g1_server: std::os::unix::net::UnixStream,
    ) -> (PrimaryRouter, Receiver<AppWindowCommand>) {
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, _guardian_rx) = mpsc::channel();
        let handle = PrimaryRouterHandle {
            current: Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
                attempt: 1,
                writer: g1_server,
                reader_stop: Arc::new(AtomicBool::new(false)),
                pending_quit: None,
            }))),
            readers: Arc::new(Mutex::new(HashMap::new())),
            pending_echo: Arc::new(Mutex::new(None)),
            pending_echo_attempt: Arc::new(AtomicU32::new(0)),
            pending_echo_corr: Arc::new(AtomicU32::new(0)),
            next_host_corr: Arc::new(AtomicU32::new(1)),
            window_ready: Arc::new(AtomicBool::new(true)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(true)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown: SessionShutdownState::new(),
            fs: Some(snapshot.fs_weak()),
            fs_worker_commands: None,
            guardian: GuardianOwnerHandle {
                command_tx: guardian_tx,
            },
            window_commands: window_tx,
            #[cfg(target_os = "macos")]
            quit_drain_hooks: Arc::default(),
        };
        let router =
            PrimaryRouter::finish_start(handle, None).expect("successor drain router worker");
        (router, window_rx)
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn successor_reader_waits_for_retired_fs_drain_before_ready_and_call() {
        use std::os::unix::net::UnixStream;

        use keld_ipc::link::{AppLinkDeadlines as _, read_frame, write_frame};
        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("successor drain root");
        let manifest_text = "{}\n";
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, manifest_text).expect("write successor drain manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open successor drain manifest"),
            manifest_path,
            digest,
        )
        .expect("verify successor drain manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare successor drain broker");

        let (g1_server, _g1_client) = UnixStream::pair().expect("G1 drain pair");
        let (mut router, window_rx) = successor_drain_test_router(&snapshot, g1_server);
        let old_call = snapshot
            .fs
            .begin_call(PendingFsCall {
                attempt: 1,
                correlation: CorrelationId(110),
            })
            .expect("admit old generation FS call")
            .expect("old generation FS admission remains open");
        router
            .handle
            .retire_generation(1)
            .expect("retire G1 while native work is outstanding");

        let (drain_wait_tx, drain_wait_rx) = mpsc::sync_channel(1);
        snapshot.fs.observe_next_drain_wait(drain_wait_tx);
        let (g2_server, mut g2_client) = UnixStream::pair().expect("G2 drain pair");
        g2_client
            .set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("G2 deadlines");
        let install_handle = router.handle.clone();
        let (installed_tx, installed_rx) = mpsc::sync_channel(1);
        let install = thread::spawn(move || {
            let result = install_handle.install_generation(2, g2_server);
            installed_tx.send(result).expect("report G2 install");
        });

        drain_wait_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("G2 install must wait for the retired native call to drain");
        assert!(
            matches!(installed_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "G2 was published before old filesystem authority drained"
        );
        assert!(
            router
                .handle
                .current
                .lock()
                .expect("generation state")
                .is_none(),
            "G2 became current before retired native work drained"
        );
        assert!(
            matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "waiting for old FS work made the recovered session fatal"
        );

        drop(old_call);
        installed_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("G2 install completion")
            .expect("G2 installs after old FS drain");
        install.join().expect("G2 installer joins");

        let (ready, ready_payload) = read_frame(&mut g2_client).expect("G2 Ready replay");
        assert_eq!(ready.kind, FrameKind::Event);
        assert_eq!(ready.channel, LIFECYCLE_CHANNEL);
        assert_eq!(ready.corr, CorrelationId(0));
        assert_eq!(
            decode::<LifecycleEvent>(&ready_payload).expect("decode G2 Ready"),
            LifecycleEvent::Ready
        );

        write_frame(
            &mut g2_client,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(111),
            &encode(&FsRequest::Read {
                path: temp.path().join("denied.txt").display().to_string(),
            })
            .expect("encode G2 FS Call"),
        )
        .expect("send G2 FS Call");
        let (reply, payload) = read_frame(&mut g2_client).expect("G2 FS terminal response");
        assert_eq!(reply.kind, FrameKind::Err);
        assert_eq!(reply.channel, FS_CHANNEL);
        assert_eq!(reply.corr, CorrelationId(111));
        let error: CallError = decode(&payload).expect("decode G2 FS Err");
        assert_eq!(error.code, "KELD-GUARD001");
        assert!(
            matches!(window_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "G2's first well-formed FS Call became a fatal recovery error"
        );

        router
            .stop_and_join()
            .expect("successor drain router shutdown");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn retired_generation_correlation_does_not_release_global_fs_single_flight() {
        use std::io::Cursor;

        use keld_ipc::link::write_frame;
        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("global FS single-flight root");
        let allowed = temp.path().join("allowed");
        fs::create_dir(&allowed).expect("global FS single-flight allowed root");
        let scope = allowed.display().to_string().replace('\\', "/");
        let manifest_text = format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"]}}}}}}"#);
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, &manifest_text).expect("write global FS manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open global FS manifest"),
            manifest_path,
            digest,
        )
        .expect("verify global FS manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare global FS broker");

        let g1 = snapshot
            .fs
            .begin_call(PendingFsCall {
                attempt: 1,
                correlation: CorrelationId(100),
            })
            .expect("admit G1 FS call")
            .expect("G1 admission remains open");
        snapshot
            .fs
            .retire_pending_call_for_attempt(1)
            .expect("retire only G1 correlation metadata");
        assert_eq!(
            snapshot.fs.state.lock().expect("G1 state").in_flight,
            1,
            "generation retirement must not complete the admitted native operation"
        );

        let mut bytes = Vec::new();
        write_frame(
            &mut bytes,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(101),
            &[0xaa; 64],
        )
        .expect("encode G2 FS frame");
        let mut cursor = Cursor::new(bytes);
        let stop = AtomicBool::new(false);
        let error = read_primary_app_frame_interruptible_with_privileged_call(
            &mut cursor,
            &stop,
            || None,
            &keld_ipc::channel_table::FS,
            || snapshot.fs.has_outstanding_call_for(2),
        )
        .expect_err("G2 must be rejected while G1 native work is still outstanding");
        assert!(
            error.to_string().contains("KELD-IPC-005"),
            "wrong cross-generation single-flight classification: {error}"
        );
        assert_eq!(
            cursor.position(),
            u64::try_from(keld_ipc::HEADER_LEN).expect("header length fits u64"),
            "G2 payload was consumed before global single-flight rejection"
        );

        let second = snapshot.fs.begin_call(PendingFsCall {
            attempt: 2,
            correlation: CorrelationId(101),
        });
        assert!(
            second.is_err(),
            "G2 raced past the global single-flight backstop after G1 correlation retirement"
        );

        drop(g1);
        assert_eq!(
            snapshot.fs.state.lock().expect("post-G1 state").in_flight,
            0
        );
        snapshot
            .fs
            .begin_call(PendingFsCall {
                attempt: 3,
                correlation: CorrelationId(102),
            })
            .expect("G3 admission after G1 completion")
            .expect("G3 must admit after global occupancy clears");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn second_fs_call_is_rejected_before_its_payload_while_first_is_outstanding() {
        use std::io::Write as _;
        use std::os::unix::net::UnixStream;

        use keld_ipc::frame::FrameHeader;
        use keld_ipc::link::{AppLinkDeadlines as _, write_frame};
        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("second FS Call root");
        let allowed = temp.path().join("allowed");
        fs::create_dir(&allowed).expect("second FS Call allowed root");
        let first_target = allowed.join("held.txt");
        fs::write(&first_target, b"held worker bytes").expect("second FS Call input");
        let second_target = allowed.join("must-not-write.txt");
        let scope = allowed.display().to_string().replace('\\', "/");
        let manifest_text =
            format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, &manifest_text).expect("write second FS Call manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open second FS Call manifest"),
            manifest_path,
            digest,
        )
        .expect("verify second FS Call manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare second FS Call broker");

        let (taken_tx, taken_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let gate = Arc::new(FsWorkerTestGate {
            taken: taken_tx,
            release: Mutex::new(release_rx),
        });
        let (server, mut client) = UnixStream::pair().expect("second FS Call primary pair");
        client
            .set_app_link_deadlines(Some(Duration::from_secs(5)))
            .expect("second FS Call client deadlines");
        let (window_tx, window_rx) = mpsc::channel();
        let (guardian_tx, _guardian_rx) = mpsc::channel();
        let router = PrimaryRouter::start_with_fs_test_gate(
            server,
            window_tx,
            PlatformPrimaryOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
            Some(snapshot.fs_weak()),
            Some(gate),
        )
        .expect("second FS Call router");

        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(80),
            &encode(&FsRequest::Read {
                path: first_target.display().to_string().replace('\\', "/"),
            })
            .expect("encode held first FS Call"),
        )
        .expect("send held first FS Call");
        taken_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("worker took first FS Call");
        assert_eq!(
            snapshot.fs.state.lock().expect("FS state").in_flight,
            1,
            "the held first FS Call is the only native admission"
        );

        let second_payload = encode(&FsRequest::Write {
            path: second_target.display().to_string().replace('\\', "/"),
            bytes: b"must not be read".to_vec(),
        })
        .expect("encode withheld second FS payload");
        let second_len = u32::try_from(second_payload.len()).expect("second FS payload length");
        client
            .write_all(
                &FrameHeader {
                    kind: FrameKind::Call,
                    flags: 0,
                    channel: FS_CHANNEL,
                    corr: CorrelationId(81),
                    len: second_len,
                }
                .encode(),
            )
            .expect("send only second FS Call header");
        client.flush().expect("flush second FS Call header");

        let fatal = window_rx.recv_timeout(Duration::from_secs(1));
        assert_eq!(
            snapshot.fs.state.lock().expect("FS state").in_flight,
            1,
            "the withheld second payload reached native admission"
        );
        assert!(
            matches!(taken_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "the worker observed a second FS handler entry"
        );
        assert!(
            !second_target.exists(),
            "the header-only second FS Call produced a filesystem effect"
        );

        release_tx.send(()).expect("release first FS worker");
        let shutdown = router.shutdown();
        assert_eq!(
            fatal.expect("second FS Call must fail before its withheld payload is read"),
            AppWindowCommand::Fatal
        );
        let error = shutdown.expect_err("second outstanding FS Call must fail the session");
        assert!(
            error.to_string().contains("KELD-IPC-005"),
            "wrong second FS Call failure: {error}"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn stale_generation_precedes_malformed_fs_payload_classification() {
        use std::os::unix::net::UnixStream;

        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("stale FS request root");
        let manifest_text = "{}\n";
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, manifest_text).expect("write stale FS manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open stale FS manifest"),
            manifest_path,
            digest,
        )
        .expect("verify stale FS manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare stale FS broker");
        let (server, _client) = UnixStream::pair().expect("stale FS router pair");
        let (window_tx, _window_rx) = mpsc::channel();
        let (guardian_tx, _guardian_rx) = mpsc::channel();
        let router = PrimaryRouter::start_with_fs(
            server,
            window_tx,
            PlatformPrimaryOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
            Some(snapshot.fs_weak()),
        )
        .expect("stale FS router");

        let error = must_err(
            prepare_fs_work(
                &router.handle(),
                2,
                CorrelationId(70),
                &[0xff],
                &Arc::new(AtomicBool::new(false)),
            ),
            "stale generation must fail before malformed payload decoding",
        );
        router.shutdown().expect("stale FS router shutdown");
        let rendered = error.to_string();
        assert!(
            rendered.contains("stale primary generation"),
            "wrong stale-generation classification: {rendered}"
        );
        assert!(
            !rendered.contains("KELD-IPC-003"),
            "codec classification overtook stale generation: {rendered}"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn authenticated_primary_link_rejects_foreign_hello_then_routes_fs() {
        use std::os::unix::net::UnixStream;

        use keld_ipc::link::{handshake_client, read_frame, write_frame};
        use keld_ipc::{
            BootstrapAdmission, BootstrapListener, BootstrapRejection, BootstrapRejectionObserver,
            SessionToken, parse_app_link,
        };
        use sha2::{Digest as _, Sha256};

        struct RejectionRecorder(Arc<Mutex<Vec<BootstrapRejection>>>);
        impl BootstrapRejectionObserver for RejectionRecorder {
            fn rejected(&self, rejection: BootstrapRejection) {
                self.0.lock().expect("rejection recorder").push(rejection);
            }
        }

        let temp = tempfile::tempdir().expect("authenticated FS temp root");
        let allowed = temp.path().join("allowed");
        fs::create_dir(&allowed).expect("authenticated allowed root");
        let scope = allowed.display().to_string().replace('\\', "/");
        let manifest_text =
            format!(r#"{{"app":{{"fs":{{"read":["{scope}/**"],"write":["{scope}/**"]}}}}}}"#);
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, &manifest_text).expect("write authenticated FS manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open authenticated FS manifest"),
            manifest_path,
            digest,
        )
        .expect("verify authenticated FS manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare authenticated FS broker");

        let listener = Arc::new(BootstrapListener::bind().expect("bind production bootstrap"));
        let app_link = listener.app_link();
        let (endpoint, token) = parse_app_link(&app_link).expect("parse production app-link");

        let rejections = Arc::new(Mutex::new(Vec::new()));
        let listener_for_accept = Arc::clone(&listener);
        let rejection_for_accept = Arc::clone(&rejections);
        let accept = thread::spawn(move || {
            let observer = RejectionRecorder(rejection_for_accept);
            match listener_for_accept
                .accept_authenticated_until(Instant::now() + Duration::from_secs(5), &observer)
                .expect("bootstrap accept")
            {
                BootstrapAdmission::Authenticated(stream) => stream,
                BootstrapAdmission::Cancelled => panic!("bootstrap cancelled before valid peer"),
                BootstrapAdmission::DeadlineElapsed => {
                    panic!("bootstrap deadline elapsed before valid peer")
                }
            }
        });

        let mut foreign_hex = token.to_hex().into_bytes();
        foreign_hex[0] = if foreign_hex[0] == b'0' { b'1' } else { b'0' };
        let foreign = SessionToken::from_hex(
            std::str::from_utf8(&foreign_hex).expect("foreign token remains ASCII"),
        )
        .expect("foreign token");
        let mut hostile = UnixStream::connect(endpoint).expect("foreign bootstrap connect");
        let foreign_error =
            handshake_client(&mut hostile, &foreign).expect_err("foreign HELLO must fail");
        assert!(
            matches!(foreign_error, IpcError::HelloAuth { .. } | IpcError::Io(_)),
            "foreign peer must observe auth rejection or host close: {foreign_error}"
        );
        drop(hostile);

        let denied_before_auth = allowed.join("foreign-must-not-write.txt");
        assert!(
            !denied_before_auth.exists(),
            "foreign HELLO reached filesystem before authentication"
        );

        let mut client = UnixStream::connect(endpoint).expect("valid bootstrap connect");
        handshake_client(&mut client, &token).expect("valid HELLO authenticates");
        let server = accept.join().expect("accept thread");
        let observed = rejections.lock().expect("rejection readback").clone();
        assert_eq!(observed, vec![BootstrapRejection::HelloAuth]);
        assert_eq!(observed[0].code(), "KELD-IPC-007");
        assert!(
            !listener.path().exists(),
            "one-use locator remained after authenticated consume"
        );
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("authenticated FS read deadline");

        let (window_tx, _window_rx) = mpsc::channel();
        let (guardian_tx, _guardian_rx) = mpsc::channel();
        let router = PrimaryRouter::start_with_fs(
            server,
            window_tx,
            PlatformPrimaryOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
            Some(snapshot.fs_weak()),
        )
        .expect("authenticated primary router");

        let target = allowed.join("authenticated-route.txt");
        let request = FsRequest::Write {
            path: target.display().to_string().replace('\\', "/"),
            bytes: b"authenticated-primary".to_vec(),
        };
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(51),
            &encode(&request).expect("encode authenticated write"),
        )
        .expect("send authenticated FS write");
        let (header, payload) = read_frame(&mut client).expect("authenticated FS reply");
        assert_eq!(header.kind, FrameKind::Reply);
        assert_eq!(header.channel, FS_CHANNEL);
        assert_eq!(header.corr, CorrelationId(51));
        assert!(matches!(
            decode::<FsResponse>(&payload).expect("decode authenticated FS response"),
            FsResponse::Write
        ));
        assert_eq!(
            fs::read(&target).expect("authenticated bytes"),
            b"authenticated-primary"
        );

        router.shutdown().expect("authenticated router shutdown");

        let late_target = allowed.join("post-quiesce-must-not-write.txt");
        let late = FsRequest::Write {
            path: late_target.display().to_string().replace('\\', "/"),
            bytes: b"too-late".to_vec(),
        };
        if write_frame(
            &mut client,
            FrameKind::Call,
            0,
            FS_CHANNEL,
            CorrelationId(52),
            &encode(&late).expect("encode post-quiesce write"),
        )
        .is_ok()
        {
            read_frame(&mut client).expect_err("retained old stream must be closed/rejected");
        }
        assert!(
            !late_target.exists(),
            "retained old stream entered FS handler after quiescing"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn fs_quiesce_refuses_new_admission_and_drains_existing_lease() {
        use sha2::{Digest as _, Sha256};

        let temp = tempfile::tempdir().expect("FS quiesce root");
        let manifest_text = "{}\n";
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, manifest_text).expect("write empty manifest");
        let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open empty manifest"),
            manifest_path,
            digest,
        )
        .expect("verify empty manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare empty FS broker");

        let admitted = snapshot
            .fs
            .admit()
            .expect("first FS admission")
            .expect("session initially accepts");
        snapshot.fs.quiesce().expect("publish FS quiescing");
        assert!(
            snapshot
                .fs
                .admit()
                .expect("post-quiesce admission check")
                .is_none(),
            "new FS handler entered after quiescing"
        );
        assert_eq!(
            snapshot.fs.state.lock().expect("FS state").in_flight,
            1,
            "pre-admitted lease disappeared before its terminal outcome"
        );
        drop(admitted);
        snapshot.fs.drain().expect("drain admitted FS work");
        assert_eq!(
            snapshot.fs.state.lock().expect("FS state").in_flight,
            0,
            "drain returned before the admitted lease terminated"
        );
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn dequeued_fs_item_losing_quiesce_never_enters_handler() {
        let temp = tempfile::tempdir().expect("paused FS entry root");
        let manifest_text = "{}\n";
        let manifest_path = temp.path().join(PERMISSIONS_FILE);
        fs::write(&manifest_path, manifest_text).expect("write paused FS manifest");
        // SHA-256 of the exact static fixture bytes `{}\n`. Keep the Linux
        // lifecycle regression independent of the macOS-only sha2 dependency.
        let digest = [
            0xca, 0x3d, 0x16, 0x3b, 0xab, 0x05, 0x53, 0x81, 0x82, 0x72, 0x26, 0x14, 0x05, 0x68,
            0xf3, 0xbe, 0xf7, 0xea, 0xac, 0x18, 0x7c, 0xeb, 0xd7, 0x68, 0x78, 0xe0, 0xb6, 0x3e,
            0x9e, 0x44, 0x23, 0x56,
        ];
        let verified = load_verified_manifest(
            File::open(&manifest_path).expect("open paused FS manifest"),
            manifest_path,
            digest,
        )
        .expect("verify paused FS manifest");
        let snapshot = GuardSnapshot::prepare(verified).expect("prepare paused FS broker");
        let admitted = snapshot
            .fs
            .admit()
            .expect("paused FS admission")
            .expect("session initially accepts");
        let invoked = Arc::new(AtomicBool::new(false));
        let invoked_in_worker = Arc::clone(&invoked);
        let (dequeued_tx, dequeued_rx) = mpsc::sync_channel(1);
        let (enter_tx, enter_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            dequeued_tx.send(()).expect("report dequeued FS item");
            enter_rx.recv().expect("release paused handler entry");
            admitted
                .handle_with(|| invoked_in_worker.store(true, Ordering::Release))
                .expect("attempt paused handler entry")
                .is_none()
        });

        dequeued_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("worker dequeued admitted FS item");
        snapshot.fs.quiesce().expect("publish FS quiescence");
        enter_tx.send(()).expect("release handler-entry attempt");
        assert!(worker.join().expect("paused FS worker joins"));
        assert!(
            !invoked.load(Ordering::Acquire),
            "dequeued item invoked its handler after quiescence publication"
        );
        snapshot.fs.drain().expect("drain rejected FS item");
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
        let mut snapshot = GuardSnapshot::prepare(verified).expect("prepare guarded FS broker");
        snapshot.drop_observer = Some(Arc::clone(&dropped));

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
    #[cfg(any(target_os = "macos", target_os = "linux"))]
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
                TestPrimaryOwnerCommand::PrepareAcceptedShutdown(reply) => reply,
                TestPrimaryOwnerCommand::Shutdown(_) => panic!("Quit reply lacked preparation"),
                TestPrimaryOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
                TestPrimaryOwnerCommand::FailGeneration(_, _)
                | TestPrimaryOwnerCommand::FailRetiredGeneration(_, _) => {
                    panic!("unexpected link failure")
                }
                TestPrimaryOwnerCommand::ArmRecovery(_) => panic!("unexpected recovery arm"),
                TestPrimaryOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
            };
            guardian_prepared_in_thread.store(true, Ordering::Release);
            prepare_reply
                .send(Ok(()))
                .expect("guardian preparation reply");
            let shutdown_reply = match guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("guardian shutdown request")
            {
                TestPrimaryOwnerCommand::Shutdown(reply) => reply,
                TestPrimaryOwnerCommand::PrepareAcceptedShutdown(_) => {
                    panic!("duplicate preparation")
                }
                TestPrimaryOwnerCommand::AttachRouter(_, _) => panic!("unexpected router attach"),
                TestPrimaryOwnerCommand::FailGeneration(_, _)
                | TestPrimaryOwnerCommand::FailRetiredGeneration(_, _) => {
                    panic!("unexpected link failure")
                }
                TestPrimaryOwnerCommand::ArmRecovery(_) => panic!("unexpected recovery arm"),
                TestPrimaryOwnerCommand::DenyRecovery => panic!("unexpected recovery denial"),
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
            PlatformPrimaryOwnerHandle {
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
    #[cfg(any(target_os = "macos", target_os = "linux"))]
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
            PlatformPrimaryOwnerHandle {
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
        // A stale Quit returns before it reads; the peerless reader is unused.
        let (mut stale_reader, stale_peer) = UnixStream::pair().expect("stale Quit reader");
        drop(stale_peer);
        handle
            .lifecycle_quit(1, CorrelationId(72), &[0], &mut stale_reader)
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
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn recovered_generation_orders_window_events_once_across_installation() {
        use std::os::unix::net::UnixStream;
        if std::env::var_os(RECOVERED_CHILD_ENV).is_none() {
            assert_bounded_recovered_event_order_child();
            return;
        }

        // Gap, live delivery overlapping reader registration, and delivery after
        // installation are distinct schedules of the same real framed router.
        for close_at in ["gap", "registration", "installed"] {
            let (g1_server, mut g1_client) = UnixStream::pair().expect("g1 pair");
            g1_client
                .set_app_link_read_deadline(Some(Duration::from_secs(5)))
                .expect("g1 kill switch");
            let (window_tx, _window_rx) = mpsc::channel();
            let (command_tx, _command_rx) = mpsc::channel();
            let router = PrimaryRouter::start(
                g1_server,
                window_tx,
                PlatformPrimaryOwnerHandle { command_tx },
                SessionShutdownState::new(),
            )
            .expect("router");
            let handle = router.handle();
            handle.signal_ready().expect("initial Ready");
            assert_lifecycle_event(&mut g1_client, LifecycleEvent::Ready);
            handle.retire_generation(1).expect("retire g1");
            if close_at == "gap" {
                handle.signal_last_window_closed().expect("gap Close");
            }
            let (g2_server, mut g2_client) = UnixStream::pair().expect("g2 pair");
            g2_client
                .set_app_link_read_deadline(Some(Duration::from_secs(5)))
                .expect("test kill switch");
            let mut events = Vec::new();
            if close_at == "registration" {
                let registration = handle.readers.lock().expect("pause registration");
                let installer = handle.clone();
                let install = thread::spawn(move || installer.install_generation(2, g2_server));
                // A correlated reply proves the real reader is running while
                // installation cannot yet finish. No absence timeout or sleep.
                collect_events_before_echo(&mut g2_client, 90, &mut events);
                handle
                    .signal_last_window_closed()
                    .expect("overlapping Close");
                drop(registration);
                install
                    .join()
                    .expect("installer joins")
                    .expect("install g2");
            } else {
                handle.install_generation(2, g2_server).expect("install g2");
                if close_at == "installed" {
                    handle.signal_last_window_closed().expect("live Close");
                }
            }
            // Both producers have completed before this positive fence. Every
            // queued Event must precede this reply on the serialized writer.
            collect_events_before_echo(&mut g2_client, 91, &mut events);
            assert_eq!(
                events,
                [LifecycleEvent::Ready, LifecycleEvent::LastWindowClosed],
                "close during {close_at}"
            );
            let retire_deadline = Instant::now() + Duration::from_secs(3);
            let retired_reader_finished = loop {
                let finished = handle
                    .readers
                    .lock()
                    .expect("reader registry")
                    .get(&1)
                    .expect("retired reader remains registered")
                    .is_finished();
                if finished || Instant::now() >= retire_deadline {
                    break finished;
                }
                std::thread::yield_now();
            };
            assert!(
                retired_reader_finished,
                "retired G1 reader remained live with its peer open after {close_at}"
            );
            router.shutdown().expect("router shutdown");
        }
        println!("{RECOVERED_CHILD_OBSERVED}");
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn collect_events_before_echo(
        client: &mut std::os::unix::net::UnixStream,
        correlation: u32,
        events: &mut Vec<LifecycleEvent>,
    ) {
        use keld_ipc::echo::{EchoRequest, EchoResponse};
        use keld_ipc::link::read_frame;

        let request = EchoRequest {
            message: "fence".to_owned(),
            count: correlation,
        };
        write_frame(
            client,
            FrameKind::Call,
            0,
            ECHO_CHANNEL,
            CorrelationId(correlation),
            &encode(&request).expect("encode fence"),
        )
        .expect("write fence");
        loop {
            let (header, payload) = read_frame(client).expect("fence frame");
            if header.kind == FrameKind::Event {
                assert_eq!(header.channel, LIFECYCLE_CHANNEL);
                assert_eq!(header.corr, CorrelationId(0));
                events.push(decode(&payload).expect("lifecycle event"));
                assert!(events.len() <= 3, "unbounded lifecycle replay");
            } else {
                assert_eq!(header.kind, FrameKind::Reply);
                assert_eq!(header.channel, ECHO_CHANNEL);
                assert_eq!(header.corr, CorrelationId(correlation));
                assert_eq!(
                    decode::<EchoResponse>(&payload).expect("fence payload"),
                    EchoResponse {
                        message: request.message,
                        count: correlation
                    }
                );
                break;
            }
        }
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
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
            let TestPrimaryOwnerCommand::PrepareAcceptedShutdown(reply) = prepare else {
                panic!("Quit race skipped attribution");
            };
            observed_tx.send(()).expect("report blocked attribution");
            release_rx.recv().expect("release attribution");
            reply.send(Ok(())).expect("accepted Quit attribution reply");
            let shutdown = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("accepted Quit shutdown");
            let TestPrimaryOwnerCommand::Shutdown(reply) = shutdown else {
                panic!("Quit race skipped shutdown");
            };
            reply.send(Ok(())).expect("accepted Quit shutdown reply");
        });
        let router = PrimaryRouter::start(
            server,
            window_tx,
            PlatformPrimaryOwnerHandle {
                command_tx: guardian_tx,
            },
            SessionShutdownState::new(),
        )
        .expect("Quit race router");
        let handle = router.handle();
        let quit_handle = handle.clone();
        // The router's own reader serves the link; this direct Quit drains a
        // peerless stream, so its post-REPLY drain ends at EOF at once.
        let (mut quit_reader, quit_peer) =
            std::os::unix::net::UnixStream::pair().expect("direct Quit reader");
        drop(quit_peer);
        let quit_thread = std::thread::spawn(move || {
            quit_handle.lifecycle_quit(1, CorrelationId(81), &[0], &mut quit_reader)
        });
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

    #[cfg(any(target_os = "linux", windows))]
    use DirectPrimaryOwnerCommand as TestPrimaryOwnerCommand;
    #[cfg(target_os = "macos")]
    use GuardianOwnerCommand as TestPrimaryOwnerCommand;

    #[cfg(target_os = "macos")]
    fn primary_echo_test_router() -> (
        PrimaryRouter,
        PrimaryRouterHandle,
        std::os::unix::net::UnixStream,
        Receiver<TestPrimaryOwnerCommand>,
        Receiver<AppWindowCommand>,
    ) {
        let (server, client) = std::os::unix::net::UnixStream::pair().expect("Echo router pair");
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("Echo client read deadline");
        let (command_tx, command_rx) = mpsc::channel();
        let (window_tx, window_rx) = mpsc::channel();
        let router = PrimaryRouter::start(
            server,
            window_tx,
            PlatformPrimaryOwnerHandle { command_tx },
            SessionShutdownState::new(),
        )
        .expect("Echo primary router");
        let handle = router.handle();
        (router, handle, client, command_rx, window_rx)
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn renderer_dispatch_stop_does_not_depend_on_webview_sender_drop() {
        let (router, handle, _client, _owner_rx, _window_rx) = primary_echo_test_router();
        let (requests_tx, requests_rx) = mpsc::sync_channel(1);
        let (outcomes_tx, _outcomes_rx) = mpsc::channel();
        let dispatch =
            start_renderer_dispatch(requests_rx, outcomes_tx, handle).expect("renderer dispatch");
        let (done_tx, done_rx) = mpsc::channel();
        let joiner = std::thread::spawn(move || {
            let result = join_renderer_dispatch(dispatch);
            let _ = done_tx.send(result);
        });

        let observed = done_rx.recv_timeout(Duration::from_secs(2));
        drop(requests_tx);
        joiner.join().expect("renderer dispatch join probe");
        assert!(
            observed
                .expect("renderer dispatch ignored its explicit stop")
                .is_ok(),
            "renderer dispatch stop must not wait for WebKit to release its request sender"
        );
        router.shutdown().expect("renderer-stop router shutdown");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn host_echo_reply_shares_primary_reader_with_bun_call_and_ping() {
        use keld_ipc::echo::{EchoRequest, EchoResponse};
        use keld_ipc::link::{read_frame, write_frame};

        let (router, handle, mut client, _owner_rx, _window_rx) = primary_echo_test_router();
        let request = EchoRequest {
            message: "renderer".to_owned(),
            count: 42,
        };
        let request_bytes = encode(&request).expect("encode renderer Echo");
        let call = handle
            .begin_echo_call(&request_bytes)
            .expect("host-originated Echo call");

        let (outbound, outbound_payload) = read_frame(&mut client).expect("outbound renderer Call");
        assert_eq!(outbound.kind, FrameKind::Call);
        assert_eq!(outbound.channel, ECHO_CHANNEL);
        assert_eq!(outbound.corr, call.correlation);
        assert_eq!(outbound_payload, request_bytes);

        write_frame(
            &mut client,
            FrameKind::Ping,
            0,
            keld_ipc::ChannelId(99),
            CorrelationId(0),
            &[],
        )
        .expect("write interleaved Ping");
        let (pong, pong_payload) = read_frame(&mut client).expect("read interleaved Ping");
        assert_eq!(pong.kind, FrameKind::Ping);
        assert_eq!(pong.channel, keld_ipc::ChannelId(99));
        assert_eq!(pong.corr, CorrelationId(0));
        assert!(pong_payload.is_empty());

        let bun_request = EchoRequest {
            message: "bun".to_owned(),
            count: 7,
        };
        write_frame(
            &mut client,
            FrameKind::Call,
            0,
            ECHO_CHANNEL,
            CorrelationId(91),
            &encode(&bun_request).expect("encode Bun Echo"),
        )
        .expect("write interleaved Bun Call");
        let (bun_reply, bun_payload) = read_frame(&mut client).expect("read Bun Echo Reply");
        assert_eq!(bun_reply.kind, FrameKind::Reply);
        assert_eq!(bun_reply.channel, ECHO_CHANNEL);
        assert_eq!(bun_reply.corr, CorrelationId(91));
        assert_eq!(
            decode::<EchoResponse>(&bun_payload).expect("decode Bun Echo Reply"),
            EchoResponse {
                message: bun_request.message,
                count: bun_request.count,
            }
        );

        let expected = encode(&EchoResponse {
            message: request.message,
            count: request.count,
        })
        .expect("encode renderer Echo Reply");
        write_frame(
            &mut client,
            FrameKind::Reply,
            0,
            ECHO_CHANNEL,
            call.correlation,
            &expected,
        )
        .expect("write renderer Echo Reply");
        match call
            .reply
            .recv_timeout(Duration::from_secs(2))
            .expect("renderer Echo terminal outcome")
            .expect("renderer Echo success")
        {
            PrimaryEchoReply::Reply(payload) => assert_eq!(payload, expected),
            PrimaryEchoReply::Err(error) => panic!("unexpected renderer Echo Err: {error}"),
        }

        router.shutdown().expect("Echo router shutdown");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn failed_primary_writes_retire_the_generation() {
        let (echo_handle, _echo_client, echo_commands) = initial_ready_router();
        let owner = std::thread::spawn(move || {
            let command = echo_commands
                .recv_timeout(Duration::from_secs(2))
                .expect("failed-write owner notification");
            match command {
                GuardianOwnerCommand::FailGeneration(1, reply) => {
                    reply.send(Ok(())).expect("failed-write owner reply");
                }
                _ => panic!("failed primary write sent the wrong owner command"),
            }
        });
        {
            let mut current = echo_handle.current.lock().expect("generation lock");
            current
                .as_mut()
                .expect("active renderer generation")
                .writer
                .shutdown_app_link()
                .expect("close renderer writer before probe");
        }
        let error = echo_handle
            .begin_echo_call(&[0x14, 0x02])
            .expect_err("renderer Call write must fail");
        assert!(
            error.to_string().contains("renderer Echo call write"),
            "{error}"
        );
        assert!(
            echo_handle
                .current
                .lock()
                .expect("generation lock")
                .is_none(),
            "failed renderer write left a reusable primary generation"
        );
        owner.join().expect("failed-write owner joins");

        let (event_handle, _event_client, _event_commands) = initial_ready_router();
        {
            let mut current = event_handle.current.lock().expect("generation lock");
            current
                .as_mut()
                .expect("active lifecycle generation")
                .writer
                .shutdown_app_link()
                .expect("close lifecycle writer before probe");
        }
        let error = event_handle
            .signal_last_window_closed()
            .expect_err("lifecycle Event write must fail");
        assert!(error.to_string().contains("lifecycle event"), "{error}");
        assert!(
            event_handle
                .current
                .lock()
                .expect("generation lock")
                .is_none(),
            "failed lifecycle write left a reusable primary generation"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn second_host_echo_call_is_busy_until_the_first_reply_retires() {
        use keld_ipc::link::{read_frame, write_frame};

        let (router, handle, mut client, _owner_rx, _window_rx) = primary_echo_test_router();
        let first = handle.begin_echo_call(&[1, 2, 3]).expect("first Echo call");
        let (outbound, _) = read_frame(&mut client).expect("first Echo frame");
        assert_eq!(outbound.corr, first.correlation);

        let second = handle
            .begin_echo_call(&[4, 5])
            .expect_err("second Echo call must fail busy");
        assert!(
            second.to_string().contains("already pending"),
            "unexpected busy error: {second}"
        );

        write_frame(
            &mut client,
            FrameKind::Reply,
            0,
            ECHO_CHANNEL,
            first.correlation,
            &[9],
        )
        .expect("complete first Echo");
        match first
            .reply
            .recv_timeout(Duration::from_secs(2))
            .expect("first terminal outcome")
            .expect("first success")
        {
            PrimaryEchoReply::Reply(payload) => assert_eq!(payload, [9]),
            PrimaryEchoReply::Err(error) => panic!("unexpected first Err: {error}"),
        }

        let third = handle
            .begin_echo_call(&[6])
            .expect("third Echo after retirement");
        let (third_frame, _) = read_frame(&mut client).expect("third Echo frame");
        assert_eq!(third_frame.corr, third.correlation);
        write_frame(
            &mut client,
            FrameKind::Reply,
            0,
            ECHO_CHANNEL,
            third.correlation,
            &[8],
        )
        .expect("complete third Echo");
        assert!(third.reply.recv_timeout(Duration::from_secs(2)).is_ok());

        router.shutdown().expect("busy router shutdown");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn wrong_correlation_cannot_complete_host_echo_waiter() {
        use keld_ipc::link::{read_frame, write_frame};

        let (router, handle, mut client, _owner_rx, window_rx) = primary_echo_test_router();
        let call = handle.begin_echo_call(&[1]).expect("host Echo call");
        let (outbound, _) = read_frame(&mut client).expect("host Echo frame");
        assert_eq!(outbound.corr, call.correlation);
        let wrong = CorrelationId(call.correlation.0.wrapping_add(1).max(1));
        write_frame(&mut client, FrameKind::Reply, 0, ECHO_CHANNEL, wrong, &[2])
            .expect("wrong-corr reply bytes");

        let terminal = call
            .reply
            .recv_timeout(Duration::from_secs(2))
            .expect("wrong corr must terminalize the pending call")
            .expect_err("wrong corr cannot become a successful reply");
        assert!(
            terminal.to_string().contains("primary reader"),
            "unexpected waiter failure: {terminal}"
        );
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("protocol failure wakes UI"),
            AppWindowCommand::Fatal
        );
        assert!(
            router.shutdown().is_err(),
            "wrong correlation must remain a router/session failure"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn app_link_disconnect_terminalizes_pending_echo_and_fresh_session_still_works() {
        use keld_ipc::link::{read_frame, write_frame};

        let (router, handle, mut client, _owner_rx, window_rx) = primary_echo_test_router();
        let call = handle.begin_echo_call(&[1, 2]).expect("pending host Echo");
        let (outbound, _) = read_frame(&mut client).expect("pending Echo frame");
        assert_eq!(outbound.corr, call.correlation);
        drop(client);

        let terminal = call
            .reply
            .recv_timeout(Duration::from_secs(2))
            .expect("disconnect must settle the pending waiter")
            .expect_err("disconnect cannot become a successful reply");
        assert!(
            terminal.to_string().contains("primary reader"),
            "unexpected disconnect terminal outcome: {terminal}"
        );
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("disconnect wakes the fatal UI path"),
            AppWindowCommand::Fatal
        );
        assert!(
            router.shutdown().is_err(),
            "the failed reader remains a session failure"
        );

        let (fresh_router, fresh_handle, mut fresh_client, _owner_rx, _window_rx) =
            primary_echo_test_router();
        let fresh = fresh_handle
            .begin_echo_call(&[7])
            .expect("fresh non-recovery session Echo");
        let (fresh_frame, _) = read_frame(&mut fresh_client).expect("fresh Echo frame");
        assert_eq!(fresh_frame.corr, fresh.correlation);
        write_frame(
            &mut fresh_client,
            FrameKind::Reply,
            0,
            ECHO_CHANNEL,
            fresh.correlation,
            &[8],
        )
        .expect("fresh Echo Reply");
        match fresh
            .reply
            .recv_timeout(Duration::from_secs(2))
            .expect("fresh Echo terminal outcome")
            .expect("fresh Echo success")
        {
            PrimaryEchoReply::Reply(payload) => assert_eq!(payload, [8]),
            PrimaryEchoReply::Err(error) => panic!("unexpected fresh Echo Err: {error}"),
        }
        fresh_router.shutdown().expect("fresh router shutdown");
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn primary_test_stream_pair() -> (BootstrapStream, BootstrapStream) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            std::os::unix::net::UnixStream::pair().expect("primary stream pair")
        }
        #[cfg(windows)]
        {
            let listener = Arc::new(keld_ipc::BootstrapListener::bind().expect("bind pipe"));
            let link = listener.app_link();
            let (endpoint, token) = keld_ipc::parse_app_link(&link).expect("parse pipe link");
            let endpoint = endpoint.to_owned();
            let acceptor = Arc::clone(&listener);
            let worker = thread::spawn(move || acceptor.accept_authenticated());
            let mut client = BootstrapStream::connect(&endpoint).expect("connect pipe client");
            client
                .set_app_link_deadlines(Some(APP_LINK_IO_DEADLINE))
                .expect("client deadlines");
            keld_ipc::link::handshake_client(&mut client, &token).expect("client HELLO");
            let server = worker
                .join()
                .expect("accept join")
                .expect("accept result")
                .expect("authenticated server stream");
            (server, client)
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn initial_ready_router() -> (
        PrimaryRouterHandle,
        BootstrapStream,
        Receiver<TestPrimaryOwnerCommand>,
    ) {
        let (server, client) = primary_test_stream_pair();
        let (command_tx, command_rx) = mpsc::channel();
        let (window_tx, _window_rx) = mpsc::channel();
        let handle = PrimaryRouterHandle {
            current: Arc::new(Mutex::new(Some(ActivePrimaryGeneration {
                attempt: 1,
                writer: server,
                reader_stop: Arc::new(AtomicBool::new(false)),
                pending_quit: None,
            }))),
            readers: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(target_os = "macos")]
            pending_echo: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "macos")]
            pending_echo_attempt: Arc::new(AtomicU32::new(0)),
            #[cfg(target_os = "macos")]
            pending_echo_corr: Arc::new(AtomicU32::new(0)),
            #[cfg(target_os = "macos")]
            next_host_corr: Arc::new(AtomicU32::new(1)),
            window_ready: Arc::new(AtomicBool::new(false)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(false)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown: SessionShutdownState::new(),
            fs: None,
            fs_worker_commands: None,
            guardian: PlatformPrimaryOwnerHandle { command_tx },
            window_commands: window_tx,
            #[cfg(target_os = "macos")]
            quit_drain_hooks: Arc::default(),
        };
        (handle, client, command_rx)
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn failed_successor_ready_replay_requests_its_retired_attempt_restart() {
        let (handle, _g1_client, _owner_rx) = initial_ready_router();
        handle.window_ready.store(true, Ordering::Release);
        handle.recovery_armed.store(true, Ordering::Release);
        handle.last_revoked_attempt.store(1, Ordering::Release);
        handle.retire_generation(1).expect("retire g1");
        let (g2_server, g2_client) = primary_test_stream_pair();
        drop(g2_client);
        let error = handle
            .install_generation(2, g2_server)
            .expect_err("closed g2 replay");
        assert!(
            error.error.to_string().contains("lifecycle event"),
            "{error:?}"
        );
        assert_eq!(error.retired_after_failed_ready, Some(2));
        assert!(handle.current.lock().expect("generation lock").is_none());
        let mut restarted = Vec::new();
        assert!(restart_failed_bound_generation(
            &handle,
            2,
            &error,
            |attempt| {
                assert!(matches!(
                    handle.shutdown.transition.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                restarted.push(attempt);
            }
        ));
        assert_eq!(
            restarted,
            [2],
            "the direct owner must request the failed bound attempt"
        );

        // The restart-admitted-first order is the other linearization of the
        // same transition used by teardown. Once teardown wins afterward, a
        // late successor is closed without publishing current or a reader.
        let mut router = PrimaryRouter {
            handle: handle.clone(),
            fs_worker: None,
        };
        router
            .stop_and_join()
            .expect("teardown after admitted restart");
        assert!(handle.shutdown.reader_stop.load(Ordering::Acquire));
        let (late_server, _late_client) = primary_test_stream_pair();
        handle
            .install_generation(3, late_server)
            .expect("late bound after admitted restart closes cleanly");
        assert!(handle.current.lock().expect("generation lock").is_none());
        assert!(handle.readers.lock().expect("reader lock").is_empty());
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn failed_successor_ready_replay_cannot_restart_after_router_teardown() {
        let (handle, _g1_client, _owner_rx) = initial_ready_router();
        handle.window_ready.store(true, Ordering::Release);
        handle.recovery_armed.store(true, Ordering::Release);
        handle.last_revoked_attempt.store(1, Ordering::Release);
        handle.retire_generation(1).expect("retire g1");
        let (g2_server, g2_client) = primary_test_stream_pair();
        drop(g2_client);
        let failure = handle
            .install_generation(2, g2_server)
            .expect_err("closed g2 replay");
        assert_eq!(failure.retired_after_failed_ready, Some(2));
        let mut router = PrimaryRouter {
            handle: handle.clone(),
            fs_worker: None,
        };
        router.stop_and_join().expect("router teardown");
        assert!(
            handle.shutdown.is_running(),
            "UI failure need not claim accepted shutdown"
        );
        assert!(handle.shutdown.reader_stop.load(Ordering::Acquire));
        let mut restarted = Vec::new();
        assert!(!restart_failed_bound_generation(
            &handle,
            2,
            &failure,
            |attempt| restarted.push(attempt)
        ));
        assert!(restarted.is_empty());
        assert!(handle.current.lock().expect("generation lock").is_none());
        let (late_server, _late_client) = primary_test_stream_pair();
        handle
            .install_generation(3, late_server)
            .expect("late bind is harmless after teardown");
        assert!(handle.current.lock().expect("generation lock").is_none());
        assert!(handle.readers.lock().expect("reader lock").is_empty());
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn failed_ready_restart_overlapping_router_teardown_has_only_serial_outcomes() {
        let (handle, _g1_client, _owner_rx) = initial_ready_router();
        handle.window_ready.store(true, Ordering::Release);
        handle.recovery_armed.store(true, Ordering::Release);
        handle.last_revoked_attempt.store(1, Ordering::Release);
        handle.retire_generation(1).expect("retire g1");
        let (g2_server, g2_client) = primary_test_stream_pair();
        drop(g2_client);
        let failure = handle
            .install_generation(2, g2_server)
            .expect_err("closed g2 replay");
        assert_eq!(failure.retired_after_failed_ready, Some(2));

        // The owner and UI teardown race for the same transition. The lock
        // makes this equivalent to one of two observable orders: either the
        // supervisor restart is admitted first, or reader_stop wins first.
        let start = std::sync::Arc::new(std::sync::Barrier::new(3));
        let owner_start = std::sync::Arc::clone(&start);
        let owner_handle = handle.clone();
        let (restart_tx, restart_rx) = mpsc::channel();
        let owner = thread::spawn(move || {
            owner_start.wait();
            let mut restarted = Vec::new();
            let admitted = restart_failed_bound_generation(&owner_handle, 2, &failure, |attempt| {
                assert!(
                    !owner_handle.shutdown.reader_stop.load(Ordering::Acquire),
                    "restart callback ran after router stop publication"
                );
                restart_tx.send(attempt).expect("record restart request");
                restarted.push(attempt);
            });
            (admitted, restarted)
        });
        let teardown_start = std::sync::Arc::clone(&start);
        let mut router = PrimaryRouter {
            handle: handle.clone(),
            fs_worker: None,
        };
        let teardown = thread::spawn(move || {
            teardown_start.wait();
            router.stop_and_join().expect("overlapping router teardown");
        });
        start.wait();

        let (admitted, restarted) = owner.join().expect("owner decision joins");
        teardown.join().expect("teardown worker joins");
        let requests: Vec<u32> = restart_rx.try_iter().collect();
        assert_eq!(requests, restarted);
        assert!(requests.is_empty() || requests == [2], "{requests:?}");
        assert_eq!(admitted, !requests.is_empty());
        assert!(handle.shutdown.reader_stop.load(Ordering::Acquire));
        assert!(handle.current.lock().expect("generation lock").is_none());
        assert!(handle.readers.lock().expect("reader lock").is_empty());

        let (late_server, _late_client) = primary_test_stream_pair();
        handle
            .install_generation(3, late_server)
            .expect("late bound closes after teardown");
        assert!(handle.current.lock().expect("generation lock").is_none());
        assert!(handle.readers.lock().expect("reader lock").is_empty());
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn failed_bound_restart_rejects_unowned_or_terminal_transitions() {
        for case in [
            "mismatch",
            "stale",
            "pre-ready",
            "unarmed",
            "shutdown",
            "successor",
            "ordinary",
            "misordered",
        ] {
            let (handle, _client, _owner_rx) = initial_ready_router();
            handle.window_ready.store(true, Ordering::Release);
            handle.recovery_armed.store(true, Ordering::Release);
            handle.last_revoked_attempt.store(1, Ordering::Release);
            handle.retire_generation(1).expect("retire g1");
            let mut failure = PrimaryGenerationFailure {
                error: app_detail("lifecycle event", "forced failed replay"),
                retired_after_failed_ready: Some(2),
            };
            match case {
                "mismatch" => failure.retired_after_failed_ready = Some(1),
                "stale" => handle.last_revoked_attempt.store(2, Ordering::Release),
                "pre-ready" => handle.window_ready.store(false, Ordering::Release),
                "unarmed" => handle.recovery_armed.store(false, Ordering::Release),
                "shutdown" => assert!(handle.shutdown.claim_cli_lease_lost()),
                "successor" | "misordered" => {
                    let (writer, _peer) = primary_test_stream_pair();
                    *handle.current.lock().expect("generation lock") =
                        Some(ActivePrimaryGeneration {
                            attempt: if case == "successor" { 3 } else { 1 },
                            writer,
                            reader_stop: Arc::new(AtomicBool::new(false)),
                            pending_quit: None,
                        });
                    if case != "successor" {
                        failure.retired_after_failed_ready = None;
                    }
                }
                "ordinary" => failure.retired_after_failed_ready = None,
                _ => unreachable!(),
            }
            let mut restarted = Vec::new();
            assert!(
                !restart_failed_bound_generation(&handle, 2, &failure, |attempt| restarted
                    .push(attempt)),
                "{case}"
            );
            assert!(restarted.is_empty(), "{case}: {restarted:?}");
        }
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn failed_bound_keeps_the_existing_current_attempt_restart_path() {
        let (handle, _client, _owner_rx) = initial_ready_router();
        handle.window_ready.store(true, Ordering::Release);
        handle.recovery_armed.store(true, Ordering::Release);
        let failure = PrimaryGenerationFailure::from(app_detail(
            "primary session reader",
            "forced post-publication reader spawn failure",
        ));
        let mut restarted = Vec::new();
        assert!(restart_failed_bound_generation(
            &handle,
            1,
            &failure,
            |attempt| restarted.push(attempt)
        ));
        assert_eq!(restarted, [1]);
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn failed_close_write_cannot_mint_ready_replay_recovery() {
        let (handle, client, _owner_rx) = initial_ready_router();
        drop(client);
        let error = handle
            .write_event_guarded(LifecycleEvent::LastWindowClosed)
            .expect_err("closed primary must fail Close");
        assert!(
            error.error.to_string().contains("lifecycle event"),
            "{error:?}"
        );
        assert!(handle.current.lock().expect("generation lock").is_none());
        assert_eq!(error.retired_after_failed_ready, None);
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn direct_recovery_arm_is_published_before_acknowledgement() {
        for armed in [false, true] {
            let (handle, _client, _owner_rx) = initial_ready_router();
            let result = armed.then_some(()).ok_or_else(|| String::from("denied"));
            let mut acknowledged = false;
            acknowledge_direct_recovery_arm(Some(&handle), result, |result| {
                assert_eq!(result.is_ok(), armed);
                assert_eq!(handle.recovery_armed.load(Ordering::Acquire), armed);
                acknowledged = true;
            });
            assert!(acknowledged);
        }
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn initial_ready_after_accepted_shutdown_does_not_arm_a_stopped_owner() {
        use std::io::Read as _;

        let (handle, mut client, command_rx) = initial_ready_router();
        assert!(handle.shutdown.claim_cli_lease_lost());
        drop(command_rx); // The accepted tail has already stopped its owner.
        handle
            .signal_ready()
            .expect("late navigation must not restart admission");
        assert!(!handle.window_ready.load(Ordering::Acquire));
        assert!(!handle.recovery_armed.load(Ordering::Acquire));
        client
            .set_nonblocking(true)
            .expect("observe absence without waiting");
        let error = client
            .read(&mut [0])
            .expect_err("late Ready must write no bytes");
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn initial_ready_releases_transition_before_owner_acknowledgment() {
        let (handle, mut client, command_rx) = initial_ready_router();
        let shutdown = handle.shutdown.clone();
        let ready = Arc::clone(&handle.window_ready);
        let owner = thread::spawn(move || {
            let TestPrimaryOwnerCommand::ArmRecovery(reply) =
                command_rx.recv().expect("arm request")
            else {
                panic!("first owner command must arm recovery");
            };
            // A real owner may need this lock while applying a generation
            // update before it can acknowledge the already-enqueued arm.
            let _transition = shutdown.transition_guard();
            assert!(ready.load(Ordering::Acquire));
            reply.send(Ok(())).expect("arm acknowledgment");
        });
        handle
            .signal_ready()
            .expect("owner callback must not deadlock");
        let (header, payload) = keld_ipc::link::read_frame(&mut client).expect("Ready bytes");
        assert_eq!(header.kind, FrameKind::Event);
        assert_eq!(header.channel, LIFECYCLE_CHANNEL);
        assert_eq!(header.corr, CorrelationId(0));
        assert_eq!(payload, [0]); // Rust lifecycle Ready postcard discriminant.
        assert!(handle.recovery_armed.load(Ordering::Acquire));
        owner.join().expect("owner callback joins");
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn initial_ready_preserves_unaccepted_owner_failures() {
        let (handle, _client, command_rx) = initial_ready_router();
        drop(command_rx);
        let error = handle
            .signal_ready()
            .expect_err("a missing live-session owner is a fault");
        assert!(error.to_string().contains("KELD-CORE-037"), "{error}");
        assert!(
            error.to_string().contains("primary recovery arm"),
            "{error}"
        );

        for reject in [true, false] {
            let (handle, _client, command_rx) = initial_ready_router();
            let owner = thread::spawn(move || {
                let TestPrimaryOwnerCommand::ArmRecovery(reply) =
                    command_rx.recv().expect("arm request")
                else {
                    panic!("first owner command must arm recovery");
                };
                if reject {
                    reply
                        .send(Err(String::from("forced arm refusal")))
                        .expect("refusal reply");
                }
                // Dropping the reply without acknowledgment remains a real error.
            });
            let error = handle
                .signal_ready()
                .expect_err("arm fault must remain visible");
            assert!(error.to_string().contains("KELD-CORE-037"), "{error}");
            if reject {
                assert!(error.to_string().contains("forced arm refusal"), "{error}");
            }
            assert!(!handle.recovery_armed.load(Ordering::Acquire));
            owner.join().expect("owner probe joins");
        }
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
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
                reader_stop: Arc::new(AtomicBool::new(false)),
                pending_quit: None,
            }))),
            readers: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(target_os = "macos")]
            pending_echo: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "macos")]
            pending_echo_attempt: Arc::new(AtomicU32::new(0)),
            #[cfg(target_os = "macos")]
            pending_echo_corr: Arc::new(AtomicU32::new(0)),
            #[cfg(target_os = "macos")]
            next_host_corr: Arc::new(AtomicU32::new(1)),
            window_ready: Arc::new(AtomicBool::new(false)),
            last_window_closed: Arc::new(AtomicBool::new(false)),
            recovery_armed: Arc::new(AtomicBool::new(false)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown: SessionShutdownState::new(),
            fs: None,
            fs_worker_commands: None,
            guardian: PlatformPrimaryOwnerHandle {
                command_tx: guardian_tx,
            },
            window_commands: window_tx,
            #[cfg(target_os = "macos")]
            quit_drain_hooks: Arc::default(),
        };
        let error = handle
            .signal_ready()
            .expect_err("closed g1 must fail Ready");
        assert!(error.to_string().contains("lifecycle event"), "{error}");
        assert!(matches!(
            guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("Ready failure recovery denial"),
            TestPrimaryOwnerCommand::DenyRecovery
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

    #[cfg(target_os = "macos")]
    fn begin_pending_renderer_echo(
        handle: &PrimaryRouterHandle,
        client: &mut std::os::unix::net::UnixStream,
    ) -> PrimaryEchoCall {
        let pending = handle
            .begin_echo_call(&[0x14, 0x02])
            .expect("lease-loss pending renderer Echo");
        let (outbound, _) =
            keld_ipc::link::read_frame(client).expect("lease-loss renderer Echo frame");
        assert_eq!(outbound.kind, FrameKind::Call);
        assert_eq!(outbound.channel, ECHO_CHANNEL);
        assert_eq!(outbound.corr, pending.correlation);
        pending
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
            let GuardianOwnerCommand::PrepareAcceptedShutdown(prepare_reply) = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("guardian lease-loss attribution")
            else {
                panic!("unexpected guardian command before lease-loss attribution");
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
            let GuardianOwnerCommand::Shutdown(shutdown_reply) = guardian_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("guardian lease-loss shutdown")
            else {
                panic!("unexpected guardian command during lease-loss shutdown");
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

        let pending = begin_pending_renderer_echo(&router.handle(), &mut client);

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
        let terminal = pending
            .reply
            .recv_timeout(Duration::from_secs(2))
            .expect("lease loss must settle the pending renderer waiter")
            .expect_err("lease loss cannot produce an application reply");
        assert!(
            terminal.to_string().contains("CLI lease loss"),
            "unexpected lease-loss waiter result: {terminal}"
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
                GuardianOwnerCommand::FailGeneration(_, _)
                | GuardianOwnerCommand::FailRetiredGeneration(_, _) => {
                    panic!("unexpected link failure")
                }
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
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn generation_reader_stop_preserves_successor_and_terminal_scope() {
        let shutdown = SessionShutdownState::new();
        let retired = shutdown.register_generation_reader();
        let successor = shutdown.register_generation_reader();
        retired.store(true, Ordering::Release);
        assert!(retired.load(Ordering::Acquire));
        assert!(!successor.load(Ordering::Acquire));
        assert!(!shutdown.reader_stop.load(Ordering::Acquire));

        shutdown.stop_reader();
        assert!(successor.load(Ordering::Acquire));
        assert!(
            shutdown
                .register_generation_reader()
                .load(Ordering::Acquire),
            "a reader registered after terminal shutdown must start stopped"
        );
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

    #[cfg(any(target_os = "macos", target_os = "linux"))]
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

    #[test]
    #[cfg(windows)]
    fn windows_shutdown_disconnect_is_idempotent_but_other_requests_fail() {
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        drop(shutdown_tx);
        assert!(receive_direct_owner_reply(&shutdown_rx, "Windows primary shutdown", true).is_ok());

        let (request_tx, request_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        drop(request_tx);
        assert!(
            receive_direct_owner_reply(&request_rx, "Windows primary recovery arm", false).is_err()
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_lease_tail_failure_wakes_the_window_fatally() {
        let (window_tx, window_rx) = mpsc::channel();
        let (error_tx, error_rx) = mpsc::channel();
        publish_windows_lease_result(
            Err(app_detail("Windows lease test", "forced tail failure")),
            &error_tx,
            &window_tx,
        );
        assert_eq!(
            window_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("lease failure Fatal wake"),
            AppWindowCommand::Fatal
        );
        assert!(
            error_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("lease failure diagnostic")
                .to_string()
                .contains("forced tail failure")
        );

        publish_windows_lease_result(Ok(()), &error_tx, &window_tx);
        assert!(matches!(
            window_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert!(matches!(
            error_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }

    #[test]
    #[cfg(windows)]
    #[allow(unsafe_code)] // test owns the live tempfile handle for every Win32 flag query/mutation
    fn windows_lease_handle_inheritance_is_cleared_before_bun_spawn() {
        use std::os::windows::io::AsRawHandle as _;

        let file = tempfile::tempfile().expect("temporary handle");
        let handle = file.as_raw_handle().cast();
        // SAFETY: `handle` is borrowed from `file`, which remains live through
        // every call. The test changes and reads only HANDLE_FLAG_INHERIT.
        assert_ne!(
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
            0,
            "mark test handle inheritable: {}",
            io::Error::last_os_error()
        );
        let mut flags = 0;
        // SAFETY: `flags` is a live writable u32 and `handle` is live.
        assert_ne!(unsafe { GetHandleInformation(handle, &raw mut flags) }, 0);
        assert_ne!(flags & HANDLE_FLAG_INHERIT, 0);

        clear_windows_handle_inheritance(handle).expect("clear handle inheritance");
        // SAFETY: same live handle and output storage as above.
        assert_ne!(unsafe { GetHandleInformation(handle, &raw mut flags) }, 0);
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
    }

    #[test]
    #[cfg(windows)]
    #[allow(unsafe_code)] // test owns and closes both anonymous-pipe handles
    fn windows_preflight_rejects_a_lease_pipe_with_no_writer() {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
        use windows_sys::Win32::System::Pipes::CreatePipe;

        let mut reader = std::ptr::null_mut();
        let mut writer = std::ptr::null_mut();
        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>())
                .expect("SECURITY_ATTRIBUTES size"),
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 0,
        };
        // SAFETY: both output pointers and the attributes value remain live;
        // successful handles are closed below exactly once.
        assert_ne!(
            unsafe { CreatePipe(&raw mut reader, &raw mut writer, &raw const attributes, 0) },
            0
        );
        // SAFETY: `writer` is the unique live handle returned by CreatePipe.
        assert_ne!(unsafe { CloseHandle(writer) }, 0);
        assert!(!windows_lease_pipe_is_live(reader.addr()).expect("peek closed lease"));
        // SAFETY: `reader` is the remaining unique live pipe handle.
        assert_ne!(unsafe { CloseHandle(reader) }, 0);
    }

    #[test]
    #[cfg(windows)]
    fn windows_shutdown_claim_blocks_late_ui_startup() {
        let shutdown = SessionShutdownState::new();
        assert!(shutdown.claim_cli_lease_lost());
        let created = AtomicBool::new(false);
        let result = run_direct_startup_if_session_running(&shutdown, || {
            created.store(true, Ordering::Release);
        });
        assert!(result.is_none());
        assert!(!created.load(Ordering::Acquire));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
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
