//! Validated no-flag application boot and host-owned primary session (KEL-96).

#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
mod boot_contract;
#[cfg(any(test, target_os = "macos"))]
mod dialog_custody;
#[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
mod dialog_custody_contract;
#[cfg(target_os = "macos")]
mod dialog_session;
mod error;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod window_registry;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod window_router;
pub use self::error::HostAppError;
#[cfg(all(test, any(target_os = "macos", windows)))]
use self::error::startup_resource_snapshot;

#[cfg(all(test, windows))]
use self::boot_contract::decode_digest;
#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
use self::boot_contract::{InitialWindow, boot_error, parse_boot_bytes, target_error};

#[cfg(target_os = "macos")]
mod macos_profile_selection;
#[cfg(all(test, target_os = "macos"))]
use self::macos_profile_selection::MacValidatedAppIdentity;
#[cfg(target_os = "macos")]
use self::macos_profile_selection::macos_profile_selection;
#[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
use self::macos_profile_selection::verified_macos_app_identity;
#[cfg(all(
    target_os = "macos",
    any(test, all(feature = "profile-test-hooks", debug_assertions))
))]
use self::macos_profile_selection::{macos_identity_error, select_macos_profile_mode_with};

#[cfg(target_os = "macos")]
mod auth_router;
#[cfg(target_os = "macos")]
use auth_router::{AuthSession, AuthWork, AuthWorker};

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
use std::sync::OnceLock;
#[cfg(any(target_os = "linux", windows))]
use std::sync::atomic::AtomicU64;
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
use nix::fcntl::{FcntlArg, FdFlag, OFlag, fcntl};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use nix::sys::stat::{SFlag, fstat};

#[cfg(test)]
use keld_guard::ManifestError;
use keld_guard::Principal;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_guard::verified_manifest::VerifiedManifest;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_guard::verified_manifest::load_verified_manifest;
#[cfg(windows)]
use keld_guard::{VerifiedWindowsImage, WindowsAuthenticodeError, WindowsAuthenticodeImage};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::CallError;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::codec::{decode, encode};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::frame::{CorrelationId, FrameKind};
#[cfg(all(test, target_os = "macos"))]
use keld_ipc::link::read_primary_app_frame_interruptible_with_privileged_call;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::link::read_quit_drain_frame_with_privileged_calls;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::link::{
    AppLinkDeadlines, GuardedCallAdmission, read_primary_app_frame_interruptible_with_admission,
    write_frame,
};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_ipc::{
    APP_LINK_IO_DEADLINE, APP_LINK_READER_POLL, BootstrapStream, ECHO_CHANNEL, IpcError,
    LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleRequest, LifecycleResponse,
};
#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
use keld_native::auth::AuthConfiguration;
#[cfg(all(test, target_os = "macos", not(keld_core_native_renderer_main)))]
use keld_native::fs::FsResponse;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_native::fs::{FS_CHANNEL, FsBroker, FsPrepareError, FsRequest};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use keld_runtime::linux_strict::LinuxStrictProfile;
#[cfg(target_os = "macos")]
use keld_runtime::macos_guardian::{GuardedPrimary, GuardedPrimaryUpdate, GuardianBootstrap};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_runtime::primary::{BoundPrimaryGeneration, PrimaryRoleEvent, RoleOwner, RolePrincipal};
#[cfg(windows)]
use keld_runtime::primary::{PrimaryRecoveryGate, PrimaryRoleConfig, PrimaryRoleSupervisor};
#[cfg(target_os = "linux")]
use keld_runtime::primary::{PrimaryRecoveryGate, PrimaryRoleConfig, PrimaryRoleSupervisor};
#[cfg(windows)]
use keld_wv::ProfileIdentity;
#[cfg(any(windows, all(test, target_os = "macos")))]
use keld_wv::WebProfileSelection;
#[cfg(target_os = "linux")]
use keld_wv::webkitgtk::WebKitGtkEngine;
#[cfg(windows)]
use keld_wv::webview2::WebView2Engine;
#[cfg(target_os = "macos")]
use keld_wv::wkwebview::{
    AppReadyTrigger, AppWindowCommand, AppWindowEvent, RendererBridgeEndpoint,
    RendererBridgeOutcome, RendererBridgeRequest, WkWebViewEngine,
};
#[cfg(any(target_os = "linux", windows))]
use keld_wv::{AppWindowCommand, AppWindowEvent};
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use keld_wv::{NavTarget, WebviewSpec, WvError};
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
#[cfg(any(target_os = "macos", windows))]
const NATIVE_WINDOW_FACADE_ENV: &str = "KELD_NATIVE_WINDOW_FACADE";
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
/// The profile handover ended the session without the application's `Quit`: the quit
/// deadline [`HANDOVER_QUIT_DEADLINE`] passed after `LastWindowClosed` (KEL-53 §4
/// "Profile handover", step 3c). Claimed only by the handover, on the main thread.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const SESSION_HANDOVER: u8 = 3;

/// The quit deadline D of KEL-53 §4 *Margin and deadlines*: how long the application
/// has, after the host wrote `LastWindowClosed` at a profile handover, to request
/// `Quit` before the session is ended under [`SESSION_HANDOVER`].
///
/// Rule: the larger of 10 s and four times the maximum the reference application was
/// measured to take from the return of the host's `LastWindowClosed` write to the
/// lifecycle-Quit handler's entry for the frame, recorded only for the `Quit` that
/// claims the session, on the host's monotonic clock, through the §7 h11 cell on the
/// KEL-254/KEL-135 signed fixture host. A D that is too small force-ends an application still persisting its
/// state; one that is too large only delays an update whose application ignores
/// `LastWindowClosed`, so the rule errs large and the floor rules.
///
/// Measured 2026-10-14, Windows 11 10.0.26300 (RAMANI, Ryzen 7 5800H, 16 logical
/// cores), the S6c2a debug host of <https://github.com/gyldlab/keld/pull/722> (built at
/// that pull request's handover commit; its later commits leave the measured path
/// unchanged), Bun 1.4.2, `WebView2` runtime 155.0.4283.45:
/// - idle: 30 runs, median 0.6 ms, max 1.1 ms;
/// - full-core load (16 busy-loop burners on 16 logical cores, the F55 method): 30 runs,
///   median 1.0 ms, max 218.5 ms.
///
/// 4 x 218.5 ms is below the 10 s floor, so D is the floor (source: the samples and
/// run logs in the keld-benches artifact named by
/// <https://github.com/gyldlab/keld/pull/722>).
#[cfg(any(target_os = "linux", windows))]
const HANDOVER_QUIT_DEADLINE: Duration = Duration::from_secs(10);

/// The forced tail runs under CLI lease loss or the handover: the two causes the host
/// claims itself, without the application's `Quit`.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const fn is_forced_cause(cause: u8) -> bool {
    cause == SESSION_CLI_LEASE_LOST || cause == SESSION_HANDOVER
}

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
    reserved_cause: Arc<AtomicU8>,
    transition: Arc<Mutex<()>>,
    /// Paired with `transition`: notified on every successful reserve and at the end of
    /// a claimant's tail, for the handover's bounded waits on the main thread.
    changed: Arc<Condvar>,
    reader_stop: Arc<AtomicBool>,
    generation_reader_stops: Arc<Mutex<Vec<Arc<AtomicBool>>>>,
    tail_started: Arc<AtomicBool>,
    /// A profile handover has ended the UI loop (KEL-53 §4 "Profile handover", step
    /// 3a). Set in the same `transition` hold as the handover's `Quit` send, after that
    /// send succeeds (the loop's wake bridge drops its command receiver on the first
    /// terminal command), and read under `transition` by `finish_tail`, whose UI step is
    /// then a no-op.
    handover_requested: Arc<AtomicBool>,
    /// The claimant's tail has ended, with or without success. Cleared by the reserve
    /// that starts a tail; the main thread awaits it after the loop has returned.
    tail_ended: Arc<AtomicBool>,
    /// The direct path's handover instants; macOS runs no handover.
    #[cfg(any(target_os = "linux", windows))]
    handover_clock: Arc<HandoverClock>,
    /// The session runs under a CLI dev lease: set when `start_direct_dev_lease_tail`
    /// attaches the lease tail to the session's router, before the session's window and
    /// event coordinator exist, and read by `request_handover`, which refuses such a
    /// session because its lease tail would run beside the handover's teardown.
    #[cfg(any(target_os = "linux", windows))]
    under_dev_lease: Arc<AtomicBool>,
    /// The lifecycle-Quit handler of one generation entered for a `Quit` while the
    /// session ran and has neither claimed the session cause nor given up: `0`, or
    /// [`quit_admission_tag`] of that generation's attempt. Written and read only under
    /// `transition`; a [`QuitAdmission`] clears it on every path that does not claim,
    /// and the generation's retirement clears it because a retired generation's `Quit`
    /// can no longer claim. While it is set, the handover's deadline claim waits
    /// (`await_quit_or_claim_handover`), so a `Quit` whose handler entered inside D
    /// keeps its claim through its filesystem drain (KEL-53 §4 "Profile handover",
    /// steps 3b and 3c).
    #[cfg(any(target_os = "linux", windows))]
    quit_admission: Arc<AtomicU64>,
}

/// The [`SessionShutdownState::quit_admission`] value of `attempt`'s admitted `Quit`;
/// never `0`, which means none.
#[cfg(any(target_os = "linux", windows))]
fn quit_admission_tag(attempt: u32) -> u64 {
    (1 << 32) | u64::from(attempt)
}

/// One lifecycle-Quit handler's admitted `Quit` (`SessionShutdownState::quit_admission`).
/// The handler releases it under `transition` in the hold that decides its claim; every
/// other return path, including an error or an unwind out of the filesystem drain,
/// releases it on drop. Either release wakes the handover's wait.
#[cfg(any(target_os = "linux", windows))]
struct QuitAdmission<'a> {
    shutdown: &'a SessionShutdownState,
    attempt: u32,
    released: bool,
}

#[cfg(any(target_os = "linux", windows))]
impl QuitAdmission<'_> {
    /// Caller holds `transition`.
    fn release_guarded(mut self) {
        self.shutdown.clear_quit_admission_guarded(self.attempt);
        self.released = true;
    }
}

#[cfg(any(target_os = "linux", windows))]
impl Drop for QuitAdmission<'_> {
    fn drop(&mut self) {
        if !self.released {
            let _transition = self.shutdown.transition_guard();
            self.shutdown.clear_quit_admission_guarded(self.attempt);
        }
    }
}

/// The handover's instants on the host's monotonic clock, each set once: the request,
/// the UI loop's return, the handover's own `LastWindowClosed` write (D runs from it; a
/// window's earlier close of the last window is not recorded here), the
/// lifecycle-Quit handler's entry for the `Quit` frame that claims the session (the D
/// sample) and the forced claim at D.
#[cfg(any(target_os = "linux", windows))]
#[derive(Default)]
struct HandoverClock {
    #[cfg_attr(
        not(any(all(debug_assertions, windows), test)),
        expect(
            dead_code,
            reason = "read by the debug evidence seam; the PerUserDirect coordinator (S6c2e) is the production reader"
        )
    )]
    requested: OnceLock<Instant>,
    ui_returned: OnceLock<Instant>,
    last_window_closed_written: OnceLock<Instant>,
    quit_frame: OnceLock<Instant>,
    forced: OnceLock<Instant>,
}

/// Ends the claimant's tail on drop, so every return path of the tail, including a
/// failed step, releases the main thread's wait.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct TailRun<'a> {
    shutdown: &'a SessionShutdownState,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl Drop for TailRun<'_> {
    fn drop(&mut self) {
        self.shutdown.end_tail();
    }
}

/// What the main thread found when its bounded wait for the application's `Quit` ended
/// (KEL-53 §4 "Profile handover", step 3b).
#[cfg(any(target_os = "linux", windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandoverWait {
    /// Another claimant (the application's `Quit`) reserved the session cause; its
    /// tail runs on its own thread.
    ClaimedByOther,
    /// D passed with the session still running and this thread claimed
    /// [`SESSION_HANDOVER`] under the same guard hold; it runs the forced tail.
    ClaimedHandover,
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
    initial_window: ValidatedInitialWindow,
    auth_configuration: AuthConfiguration,
    #[cfg(windows)]
    mode: WindowsBootMode,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
enum ValidatedInitialWindow {
    Renderer(Vec<u8>),
    None,
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
    // T2 retains the verified policy for the whole host process. T3 prepares
    // the sole native FS broker from the same immutable snapshot before any
    // child/listener/window resource exists; `prepare_next_session` does the
    // same for each later application session of the process.
    verified: VerifiedManifest,
    fs: Arc<FsDispatchSession>,
    initial_broker: Mutex<Option<FsBroker>>,
    #[cfg(all(test, target_os = "macos"))]
    drop_observer: Option<Arc<AtomicBool>>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl GuardSnapshot {
    fn prepare(verified: VerifiedManifest) -> Result<Self, FsPrepareError> {
        let broker = FsBroker::prepare(&verified)?;
        let fs = Arc::new(FsDispatchSession::new(verified.clone()));
        Ok(Self {
            verified,
            fs,
            initial_broker: Mutex::new(Some(broker)),
            #[cfg(all(test, target_os = "macos"))]
            drop_observer: None,
        })
    }

    fn fs_weak(&self) -> Weak<FsDispatchSession> {
        Arc::downgrade(&self.fs)
    }

    fn take_initial_broker(&self) -> Result<FsBroker, HostAppError> {
        self.initial_broker
            .lock()
            .map_err(|_| app_detail("filesystem worker startup", "broker transfer lock poisoned"))?
            .take()
            .ok_or_else(|| {
                app_detail(
                    "filesystem worker startup",
                    "initial broker already transferred",
                )
            })
    }

    /// Prepares the filesystem state of the next application session from the
    /// immutable policy, before that session creates any child, listener or
    /// window (KEL-53 §5 `run_app_direct`): the dispatch session returns to its
    /// initial shape and a fresh sole broker replaces the one the previous
    /// session's worker consumed and dropped. Fails closed when the previous
    /// session never transferred its broker, which means it never ran its router.
    #[cfg(any(target_os = "linux", windows))]
    fn prepare_next_session(&self) -> Result<(), HostAppError> {
        let mut slot = self.initial_broker.lock().map_err(|_| {
            app_detail(
                "filesystem session preparation",
                "broker transfer lock poisoned",
            )
        })?;
        if slot.is_some() {
            return Err(app_detail(
                "filesystem session preparation",
                "the previous session never transferred its broker",
            ));
        }
        self.fs.reset_for_next_session()?;
        *slot = Some(
            FsBroker::prepare(&self.verified)
                .map_err(|source| HostAppError::fs_prepare(&source))?,
        );
        Ok(())
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct FsDispatchState {
    accepting: bool,
    prepared_attempt: Option<u32>,
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
    state: Mutex<FsDispatchState>,
    handler_transition: Mutex<()>,
    drained: Condvar,
    #[cfg(test)]
    drain_wait_observer: Mutex<Option<SyncSender<()>>>,
    #[cfg(test)]
    terminal_write_hold: Mutex<Option<(SyncSender<()>, Receiver<()>)>>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl FsDispatchState {
    /// The state of a session whose first generation's broker is prepared and
    /// accepting, with nothing admitted.
    const fn initial() -> Self {
        Self {
            accepting: true,
            prepared_attempt: Some(1),
            in_flight: 0,
            call_outstanding: false,
            pending_call: None,
            failed_write_attempt: None,
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl FsDispatchSession {
    fn new(verified: VerifiedManifest) -> Self {
        Self {
            verified,
            state: Mutex::new(FsDispatchState::initial()),
            handler_transition: Mutex::new(()),
            drained: Condvar::new(),
            #[cfg(test)]
            drain_wait_observer: Mutex::new(None),
            #[cfg(test)]
            terminal_write_hold: Mutex::new(None),
        }
    }

    /// Returns the dispatch state to its initial shape for the next application
    /// session of this process. Refuses while a call is admitted, outstanding or
    /// pending: the previous session's router must have quiesced and drained.
    #[cfg(any(target_os = "linux", windows))]
    fn reset_for_next_session(&self) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("filesystem session reset", "in-flight lock poisoned"))?;
        if state.in_flight != 0 || state.call_outstanding || state.pending_call.is_some() {
            return Err(app_detail(
                "filesystem session reset",
                "the previous session's filesystem calls have not drained",
            ));
        }
        *state = FsDispatchState::initial();
        Ok(())
    }

    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    #[cfg(not(keld_core_native_renderer_main))]
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

    fn install_prepared_attempt(&self, attempt: u32) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("filesystem generation", "in-flight lock poisoned"))?;
        if state.prepared_attempt != Some(attempt) || state.in_flight != 0 {
            return Err(app_detail(
                "filesystem generation",
                "generation broker is not prepared",
            ));
        }
        state.accepting = true;
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

    #[cfg(test)]
    #[cfg(not(keld_core_native_renderer_main))]
    fn observe_next_drain_wait(&self, observer: SyncSender<()>) {
        *self
            .drain_wait_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(observer);
    }

    /// Test hook: holds the next successful FS terminal write after it
    /// releases the generation lock and before its lease drops, until the
    /// test resumes it.
    #[cfg(test)]
    #[cfg(not(keld_core_native_renderer_main))]
    fn hold_next_terminal_write(&self, written: SyncSender<()>, resume: Receiver<()>) {
        *self
            .terminal_write_hold
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((written, resume));
    }

    #[cfg(test)]
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
            #[cfg(test)]
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
    #[cfg(target_os = "macos")]
    Retire(u32, SyncSender<FsWorkerCompletion>),
    #[cfg(target_os = "macos")]
    Prepare {
        retired: u32,
        attempt: u32,
        reply: SyncSender<FsWorkerCompletion>,
    },
    Stop,
}

#[cfg(target_os = "macos")]
enum FsWorkerCompletion {
    Retired(u32),
    Prepared {
        retired: u32,
        attempt: u32,
        result: Result<(), HostAppError>,
    },
}

#[cfg(target_os = "macos")]
impl FsWorkerNativeState {
    fn handle_generation_command(
        &mut self,
        session: &FsDispatchSession,
        command: FsWorkerCommand,
        retiring: &mut Option<(u32, SyncSender<FsWorkerCompletion>)>,
    ) -> Result<(), HostAppError> {
        match command {
            FsWorkerCommand::Retire(attempt, reply) => {
                if self.attempt != attempt || retiring.is_some() {
                    return Err(app_detail(
                        "filesystem retirement",
                        "unexpected retired broker attempt",
                    ));
                }
                session.close_admission()?;
                *retiring = Some((attempt, reply));
                Ok(())
            }
            FsWorkerCommand::Prepare {
                retired,
                attempt,
                reply,
            } => {
                let result = if retiring.is_some() {
                    Err(app_detail(
                        "filesystem preparation",
                        "old generation broker has not retired",
                    ))
                } else {
                    self.prepare_generation(session, retired, attempt)
                };
                let _ = reply.send(FsWorkerCompletion::Prepared {
                    retired,
                    attempt,
                    result,
                });
                Ok(())
            }
            FsWorkerCommand::Work(_) | FsWorkerCommand::Stop => Err(app_detail(
                "filesystem generation command",
                "work and stop commands belong to the worker loop",
            )),
        }
    }

    fn prepare_generation(
        &mut self,
        session: &FsDispatchSession,
        retired: u32,
        attempt: u32,
    ) -> Result<(), HostAppError> {
        if self.attempt != retired || self.broker.is_some() {
            return Err(app_detail(
                "filesystem preparation",
                "old generation broker has not retired",
            ));
        }
        let fresh = FsBroker::prepare(&session.verified)
            .map_err(|error| HostAppError::fs_prepare(&error))?;
        let mut state = session
            .state
            .lock()
            .map_err(|_| app_detail("filesystem preparation", "in-flight lock poisoned"))?;
        self.broker = Some(fresh);
        self.attempt = attempt;
        state.prepared_attempt = Some(attempt);
        state.accepting = false;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
struct FsRecoveryAttempt {
    retired: u32,
    prepared: u32,
    pending: Option<FsWorkerCommand>,
    completion: Option<Receiver<FsWorkerCompletion>>,
    ready: bool,
    preparing: bool,
    released: bool,
}

#[cfg(target_os = "macos")]
impl FsRecoveryAttempt {
    fn retire(router: &PrimaryRouterHandle, retired: u32) -> Result<Self, HostAppError> {
        let prepared = retired
            .checked_add(1)
            .ok_or_else(|| app_detail("filesystem recovery", "generation counter overflow"))?;
        let mut recovery = Self {
            retired,
            prepared,
            pending: None,
            completion: None,
            ready: router.fs.is_none(),
            preparing: false,
            released: false,
        };
        if let Some(session) = router.fs.as_ref().and_then(Weak::upgrade) {
            let _transition = router.shutdown.transition_guard();
            session.close_admission()?;
            let (reply, completion) = mpsc::sync_channel(1);
            recovery.pending = Some(FsWorkerCommand::Retire(retired, reply));
            recovery.completion = Some(completion);
        } else if router.fs.is_some() {
            return Err(app_detail(
                "filesystem recovery",
                "dispatch metadata disappeared",
            ));
        }
        Ok(recovery)
    }

    fn poll(&mut self, router: &PrimaryRouterHandle) -> Result<(), HostAppError> {
        if !router.shutdown.is_running() {
            self.pending.take();
            self.completion.take();
            return Ok(());
        }
        if let Some(command) = self.pending.take() {
            let sender = router
                .fs_worker_commands
                .as_ref()
                .ok_or_else(|| app_detail("filesystem recovery", "native worker is unavailable"))?;
            match sender.try_send(command) {
                Ok(()) => {}
                Err(TrySendError::Full(command)) => self.pending = Some(command),
                Err(TrySendError::Disconnected(_)) => {
                    return Err(app_detail("filesystem recovery", "native worker stopped"));
                }
            }
        }
        let Some(receiver) = self.completion.as_ref() else {
            return Ok(());
        };
        let completion = match receiver.try_recv() {
            Ok(completion) => completion,
            Err(mpsc::TryRecvError::Empty) => return Ok(()),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(app_detail(
                    "filesystem recovery",
                    "native completion owner stopped",
                ));
            }
        };
        match completion {
            FsWorkerCompletion::Retired(attempt)
                if attempt == self.retired && !self.ready && !self.preparing =>
            {
                self.preparing = true;
                let (reply, completion) = mpsc::sync_channel(1);
                self.pending = Some(FsWorkerCommand::Prepare {
                    retired: self.retired,
                    attempt: self.prepared,
                    reply,
                });
                self.completion = Some(completion);
            }
            FsWorkerCompletion::Prepared {
                retired,
                attempt,
                result,
            } if retired == self.retired
                && attempt == self.prepared
                && !self.ready
                && self.preparing =>
            {
                result?;
                self.completion = None;
                self.ready = true;
            }
            _ => {
                return Err(app_detail(
                    "filesystem recovery",
                    "unexpected native completion attempt",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[derive(Clone, Copy)]
enum RecoveryArm {
    Initial(u32),
    #[cfg(target_os = "macos")]
    Retired {
        retired: u32,
        prepared: u32,
    },
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct FsWorkerNativeState {
    broker: Option<FsBroker>,
    attempt: u32,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct PendingFsWriteFailure {
    attempt: u32,
    session: Arc<FsDispatchSession>,
    primary: HostAppError,
    reply: Receiver<Result<Instant, String>>,
    deadline: Instant,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl PendingFsWriteFailure {
    fn complete(
        self,
        observed: Result<Result<Instant, String>, RecvTimeoutError>,
    ) -> Result<(), HostAppError> {
        let acknowledged = match observed {
            Ok(Ok(received)) if received <= self.deadline => {
                self.session.clear_failed_write_attempt(self.attempt)
            }
            Ok(Ok(_)) | Err(RecvTimeoutError::Timeout) => Err(app_detail(
                "filesystem reply recovery",
                "guardian owner did not acknowledge retired generation",
            )),
            Ok(Err(detail)) => Err(app_detail("filesystem reply recovery", detail)),
            Err(RecvTimeoutError::Disconnected) => Err(app_detail(
                "filesystem reply recovery",
                "guardian owner ended before acknowledging retired generation",
            )),
        };
        acknowledged.map_err(|cleanup| append_app_cleanup(self.primary, [Err(cleanup)]))
    }

    fn poll(self) -> Result<Option<Self>, HostAppError> {
        match self.reply.try_recv() {
            Ok(reply) => self.complete(Ok(reply)).map(|()| None),
            Err(mpsc::TryRecvError::Disconnected) => self
                .complete(Err(RecvTimeoutError::Disconnected))
                .map(|()| None),
            Err(mpsc::TryRecvError::Empty) if Instant::now() >= self.deadline => {
                self.complete(Err(RecvTimeoutError::Timeout)).map(|()| None)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(Some(self)),
        }
    }
}

#[cfg(test)]
type FsWorkerTestInitializer =
    Box<dyn FnOnce() -> Result<Box<dyn std::any::Any>, HostAppError> + Send>;

#[cfg(test)]
struct FsWorkerTestGate {
    taken: SyncSender<()>,
    release: Mutex<Receiver<()>>,
    // The historical gate still follows accepted entry. This fence precedes it.
    before_entry: Option<Arc<Self>>,
    // The erased, possibly !Send guard is created and dropped on the FS worker.
    initializer: Mutex<Option<FsWorkerTestInitializer>>,
}

#[cfg(test)]
impl FsWorkerTestGate {
    fn initialize_worker(&self) -> Result<Option<Box<dyn std::any::Any>>, HostAppError> {
        let initialize = self
            .initializer
            .lock()
            .map_err(|_| app_detail("filesystem worker fixture", "initializer lock poisoned"))?
            .take();
        initialize.map_or(Ok(None), |initialize| initialize().map(Some))
    }

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

/// Top-level host-run failure. Worker failures remain the Send-compatible
/// `HostAppError`; this wrapper can retain main-thread-only macOS custody.
///
/// An owned unproved failure is fatal: the shipping Host reports it by borrow
/// and retains the payload through its existing failed-process exit. It is not
/// a completed app session and must not be flattened, sent to another thread,
/// discarded, or used to start another application session.
#[must_use = "retain unproved main-thread custody through the fatal Host outcome"]
pub struct HostRunFailure {
    error: HostAppError,
    #[cfg(target_os = "macos")]
    retained: Option<Box<RetainedMacSession>>,
}
impl HostRunFailure {
    /// Whether this failed run still owns unresolved main-thread resources.
    #[must_use]
    pub fn retains_main_custody(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.retained.is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
}
impl From<HostAppError> for HostRunFailure {
    fn from(error: HostAppError) -> Self {
        Self {
            error,
            #[cfg(target_os = "macos")]
            retained: None,
        }
    }
}
impl std::ops::Deref for HostRunFailure {
    type Target = HostAppError;
    fn deref(&self) -> &Self::Target {
        &self.error
    }
}
impl fmt::Display for HostRunFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}
impl fmt::Debug for HostRunFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostRunFailure")
            .field("error", &self.error)
            .field("retained_main_custody", &self.retains_main_custody())
            .finish_non_exhaustive()
    }
}
impl std::error::Error for HostRunFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.error)
    }
}
#[cfg(target_os = "macos")]
struct RetainedMacSession {
    _presentation: keld_native::auth::AsMainOwner,
    _dialog_main: Option<dialog_session::DialogMainOwner>,
    _engine: WkWebViewEngine,
    _router: PrimaryRouter,
    _guardian: GuardianOwner,
    _coordinator: JoinHandle<Result<(), HostAppError>>,
    _renderer: RendererDispatch,
    _commands: Sender<AppWindowCommand>,
    guard: Option<GuardSnapshot>,
}

#[cfg(any(target_os = "linux", windows))]
fn ensure_initial_window_supported(initial: &InitialWindow) -> Result<(), HostAppError> {
    if matches!(initial, InitialWindow::None) {
        return Err(HostAppError::new(
            "KELD-CORE-034",
            "facade boot platform",
            "schema 2 is supported on macOS only",
            "Remove initialWindow from keld.config.ts to use a renderer-declared boot on this platform.",
        ));
    }
    Ok(())
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
    #[cfg(target_os = "linux")]
    ensure_initial_window_supported(&parsed.initial)?;
    let entry_file = open_relative_file(&root_fd, &parsed.entry, "entry")?;
    let initial_window = match parsed.initial {
        InitialWindow::Renderer(renderer) => {
            let renderer_file = open_relative_file(&root_fd, &renderer, "renderer")?;
            let html = read_target(renderer_file, "renderer")?;
            std::str::from_utf8(&html).map_err(|source| {
                target_error("renderer", format!("HTML is not UTF-8: {source}"))
            })?;
            ValidatedInitialWindow::Renderer(html)
        }
        InitialWindow::None => ValidatedInitialWindow::None,
    };
    let permissions_file =
        open_relative_file(&root_fd, Path::new(PERMISSIONS_FILE), "permissions file")?;
    Ok(ValidatedBootSelection {
        app: AppBootSelection {
            root,
            name: parsed.name,
            entry_path: parsed.entry,
            entry_file,
            initial_window,
            auth_configuration: parsed.auth_configuration,
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
    ensure_initial_window_supported(&parsed.initial)?;
    let entry_file = open_relative_file_windows(&root, &parsed.entry, "entry")?;
    let initial_window = match parsed.initial {
        InitialWindow::Renderer(renderer) => {
            let renderer_file = open_relative_file_windows(&root, &renderer, "renderer")?;
            let html = read_target(renderer_file, "renderer")?;
            std::str::from_utf8(&html).map_err(|source| {
                target_error("renderer", format!("HTML is not UTF-8: {source}"))
            })?;
            ValidatedInitialWindow::Renderer(html)
        }
        InitialWindow::None => ValidatedInitialWindow::None,
    };
    let permissions_file =
        open_relative_file_windows(&root, Path::new(PERMISSIONS_FILE), "permissions file")?;
    Ok(ValidatedBootSelection {
        app: AppBootSelection {
            root,
            name: parsed.name,
            entry_path: parsed.entry,
            entry_file,
            initial_window,
            auth_configuration: parsed.auth_configuration,
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
/// Returns [`HostRunFailure`] preserving the original startup, authenticated session, window,
/// guardian, Bun self-termination, or ordered-shutdown failure.
pub fn run_unprivileged(
    boot: ValidatedBootSelection,
    #[cfg(target_os = "macos")] pre_init_terminal: fn(&HostAppError) -> !,
) -> Result<(), HostRunFailure> {
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        drop(boot);
        Err(HostAppError::new(
            "KELD-CORE-034",
            "platform availability",
            "no-flag host support is unavailable on this platform",
            NO_FLAG_UNAVAILABLE_FIX,
        )
        .into())
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
        run_app(app, None, pre_init_terminal)
    }
    #[cfg(windows)]
    {
        let ValidatedBootSelection {
            app,
            permissions_file,
            permissions_digest,
        } = boot;
        drop((permissions_file, permissions_digest));
        run_app_direct(app, None).map_err(HostRunFailure::from)
    }
    #[cfg(target_os = "linux")]
    {
        let ValidatedBootSelection {
            app,
            permissions_file,
            permissions_digest,
        } = boot;
        drop((permissions_file, permissions_digest));
        run_app_direct(app, None).map_err(HostRunFailure::from)
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
/// Returns [`HostRunFailure`] preserving a typed manifest preflight failure or any
/// existing no-flag startup, session, window, guardian, Bun, or shutdown error.
pub fn run_guarded(
    boot: ValidatedBootSelection,
    #[cfg(target_os = "macos")] pre_init_terminal: fn(&HostAppError) -> !,
) -> Result<(), HostRunFailure> {
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        drop(boot);
        Err(HostAppError::new(
            "KELD-CORE-034",
            "platform availability",
            "no-flag host support is unavailable on this platform",
            NO_FLAG_UNAVAILABLE_FIX,
        )
        .into())
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
        run_app_direct(app, Some(&guard_snapshot)).map_err(HostRunFailure::from)
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
        let mut result = run_app(app, Some(&guard_snapshot), pre_init_terminal);
        if let Err(failure) = &mut result
            && let Some(retained) = &mut failure.retained
        {
            let _ = retained.guard.get_or_insert(guard_snapshot);
        }
        result
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
        run_app_direct(app, Some(&guard_snapshot)).map_err(HostRunFailure::from)
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
            reserved_cause: Arc::new(AtomicU8::new(SESSION_RUNNING)),
            transition: Arc::new(Mutex::new(())),
            changed: Arc::new(Condvar::new()),
            reader_stop: Arc::new(AtomicBool::new(false)),
            generation_reader_stops: Arc::new(Mutex::new(Vec::new())),
            tail_started: Arc::new(AtomicBool::new(false)),
            handover_requested: Arc::new(AtomicBool::new(false)),
            tail_ended: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "linux", windows))]
            handover_clock: Arc::new(HandoverClock::default()),
            #[cfg(any(target_os = "linux", windows))]
            under_dev_lease: Arc::new(AtomicBool::new(false)),
            #[cfg(any(target_os = "linux", windows))]
            quit_admission: Arc::new(AtomicU64::new(0)),
        }
    }

    fn cause(&self) -> u8 {
        self.cause.load(Ordering::Acquire)
    }

    fn handover_requested(&self) -> bool {
        self.handover_requested.load(Ordering::Acquire)
    }

    /// The handover fact as a reader outside `transition` must see it.
    /// `request_handover` sends the handover's `Quit` and records the fact in one
    /// `transition` hold. A reader that saw the `Quit` take effect, such as the main
    /// thread after the UI loop returned, therefore reads under the same guard, so it
    /// can never land between the send and the record and misclassify a handover as
    /// an ordinary end.
    #[cfg(any(target_os = "linux", windows))]
    fn handover_requested_guarded(&self) -> bool {
        let _transition = self.transition_guard();
        self.handover_requested()
    }

    /// Caller holds `transition` and has just sent the handover's `Quit` to the UI loop.
    #[cfg(any(target_os = "linux", windows))]
    #[cfg_attr(
        not(any(all(debug_assertions, windows), test)),
        expect(
            dead_code,
            reason = "called by request_handover, whose production caller is the PerUserDirect coordinator (S6c2e)"
        )
    )]
    fn mark_handover_requested_guarded(&self) {
        let _ = self.handover_clock.requested.set(Instant::now());
        self.handover_requested.store(true, Ordering::Release);
    }

    /// Records that a CLI dev lease's tail runs beside this session.
    #[cfg(any(target_os = "linux", windows))]
    fn mark_under_dev_lease(&self) {
        self.under_dev_lease.store(true, Ordering::Release);
    }

    /// The claimant's tail has ended, with or without success.
    fn end_tail(&self) {
        let _transition = self.transition_guard();
        self.tail_ended.store(true, Ordering::Release);
        self.changed.notify_all();
    }

    /// Starts the tail of the claimant that just reserved the session cause.
    fn tail_run(&self) -> TailRun<'_> {
        TailRun { shutdown: self }
    }

    /// Caller holds `transition` and found the session running: records that the
    /// lifecycle-Quit handler of `attempt` entered for a `Quit` and may still claim the
    /// session after its filesystem drain.
    #[cfg(any(target_os = "linux", windows))]
    fn admit_quit_guarded(&self, attempt: u32) -> QuitAdmission<'_> {
        self.quit_admission
            .store(quit_admission_tag(attempt), Ordering::Release);
        self.changed.notify_all();
        QuitAdmission {
            shutdown: self,
            attempt,
            released: false,
        }
    }

    /// Caller holds `transition`: `attempt`'s admitted `Quit`, if it is the recorded
    /// one, can no longer claim the session (it claimed, gave up or failed, or its
    /// generation retired). Wakes the handover's wait either way.
    #[cfg(any(target_os = "linux", windows))]
    fn clear_quit_admission_guarded(&self, attempt: u32) {
        let _ = self.quit_admission.compare_exchange(
            quit_admission_tag(attempt),
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        self.changed.notify_all();
    }

    /// Waits for the application's `Quit` until `deadline`: returns as soon as any
    /// claimant reserved the session cause, or claims [`SESSION_HANDOVER`] at the
    /// deadline under the same guard hold as the last running check, so the first
    /// claim wins and no claim is lost between the check and the claim (KEL-53 §4
    /// "Profile handover", steps 3b and 3c).
    ///
    /// The deadline claim waits while a `Quit` is admitted (`quit_admission`): a
    /// `Quit` whose handler entered before the deadline keeps its claim through its
    /// filesystem drain, so the claim point agrees with the D sample, the handler's
    /// entry. Its admission ends with its claim, which ends this wait, or with its
    /// release, after which this wait claims the handover at once if the deadline has
    /// passed. This adds no unbounded wait: that drain waits only for filesystem calls
    /// admitted before the `Quit` on its generation (the reader is paused in the `Quit`;
    /// a successor generation exists only after a retirement, which clears the
    /// admission), and the forced tail this wait would otherwise start drains the same
    /// calls without a deadline (`cli_lease_lost`, `quiesce_and_drain_fs`).
    #[cfg(any(target_os = "linux", windows))]
    fn await_quit_or_claim_handover(&self, deadline: Instant) -> HandoverWait {
        let mut transition = self.transition_guard();
        loop {
            if !self.is_running() {
                return HandoverWait::ClaimedByOther;
            }
            if self.quit_admission.load(Ordering::Acquire) != 0 {
                transition = self
                    .changed
                    .wait(transition)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                continue;
            }
            let now = Instant::now();
            if now >= deadline {
                // Under the guard with the session running, both swaps succeed.
                let _ = self.claim_guarded(SESSION_HANDOVER);
                return HandoverWait::ClaimedHandover;
            }
            transition = self
                .changed
                .wait_timeout(transition, deadline - now)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    /// Waits until the claimant's tail has ended (the forced tail ends on this thread;
    /// the lifecycle-Quit tail ends on its reader thread). A CLI-lease-loss claim has no
    /// tail this thread may await: the lease observer runs it concurrently with the
    /// teardown, as before a handover existed, and `request_handover` refuses a session
    /// under a dev lease.
    #[cfg(any(target_os = "linux", windows))]
    fn await_tail_end(&self) {
        let mut transition = self.transition_guard();
        while !self.tail_ended.load(Ordering::Acquire) && self.cause() != SESSION_CLI_LEASE_LOST {
            transition = self
                .changed
                .wait(transition)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn is_running(&self) -> bool {
        self.cause() == SESSION_RUNNING
            && self.reserved_cause.load(Ordering::Acquire) == SESSION_RUNNING
    }

    #[cfg(all(test, target_os = "macos"))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn claim_lifecycle_quit(&self) -> bool {
        self.claim(SESSION_LIFECYCLE_QUIT)
    }

    #[cfg(any(target_os = "linux", test))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn claim_cli_lease_lost(&self) -> bool {
        self.claim(SESSION_CLI_LEASE_LOST)
    }

    #[cfg(any(target_os = "linux", test))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn claim(&self, cause: u8) -> bool {
        let _transition = self.transition_guard();
        self.claim_guarded(cause)
    }

    #[cfg(any(target_os = "linux", windows, test))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn claim_guarded(&self, cause: u8) -> bool {
        self.reserve_guarded(cause) && self.accept_reserved_guarded(cause)
    }

    fn reserve_guarded(&self, cause: u8) -> bool {
        let claimed = self
            .reserved_cause
            .compare_exchange(SESSION_RUNNING, cause, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        if claimed {
            self.tail_ended.store(false, Ordering::Release);
            self.stop_reader();
            self.changed.notify_all();
        }
        claimed
    }

    fn accept_reserved_guarded(&self, cause: u8) -> bool {
        self.reserved_cause.load(Ordering::Acquire) == cause
            && self
                .cause
                .compare_exchange(SESSION_RUNNING, cause, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }

    #[cfg(target_os = "macos")]
    fn reserve_cli_lease_lost(&self) -> bool {
        let _transition = self.transition_guard();
        self.reserve_guarded(SESSION_CLI_LEASE_LOST)
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
fn configure_guardian_boot_mode(command: &mut Command, ready_trigger: AppReadyTrigger) {
    // One validated boot decision supplies both the strict private argument and
    // the existing compatibility environment input. Neither reads caller env.
    let mode = if ready_trigger == AppReadyTrigger::HostInitialized {
        "1"
    } else {
        "0"
    };
    command.arg(mode).env(NATIVE_WINDOW_FACADE_ENV, mode);
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_lines)] // one startup/cleanup state machine keeps every owned handle transition contiguous
fn run_app(
    boot: AppBootSelection,
    guard_snapshot: Option<&GuardSnapshot>,
    pre_init_terminal: fn(&HostAppError) -> !,
) -> Result<(), HostRunFailure> {
    let mut dev_lease = DevHostLease::from_environment()?;
    let shutdown = SessionShutdownState::new();
    let AppBootSelection {
        root,
        name,
        entry_path,
        entry_file,
        initial_window,
        auth_configuration,
    } = boot;
    let (html, ready_trigger) = match initial_window {
        ValidatedInitialWindow::Renderer(renderer_html) => {
            let html = String::from_utf8(renderer_html).map_err(|source| {
                HostAppError::new(
                    "KELD-CORE-036",
                    "renderer",
                    source.to_string(),
                    "Regenerate the stage with UTF-8 renderer HTML.",
                )
            })?;
            (Some(html), AppReadyTrigger::NavigationReady)
        }
        ValidatedInitialWindow::None => (None, AppReadyTrigger::HostInitialized),
    };
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
        .arg(entry_metadata.ino().to_string());
    configure_guardian_boot_mode(&mut guardian_command, ready_trigger);
    guardian_command
        // The facade Host's blocked pre-Init main stack can retain an open
        // native profile when its coordinator terminates the process. Only
        // the parent Host may delete this dev stage after a proved return;
        // the Guardian must not race that custody decision on child exit.
        .env(
            "KELD_HOST_OWNS_FACADE_STAGE_CLEANUP",
            if ready_trigger == AppReadyTrigger::HostInitialized && dev_lease.is_some() {
                "1"
            } else {
                "0"
            },
        )
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
    let main_thread = objc2::MainThreadMarker::new().ok_or_else(|| {
        app_detail(
            "auth presentation",
            "application owner is not on the main thread",
        )
    })?;
    let presentation_commands = window_commands_tx.clone();
    let (presentation_port, mut presentation_owner) = keld_native::auth::prepare_as_presentation(
        main_thread,
        Arc::new(move || {
            let _ = presentation_commands.send(AppWindowCommand::MacAuthWake);
        }),
    );
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
        guard_snapshot,
        MacBoundStart {
            auth_configuration,
            presentation: Some(presentation_port),
            facade_windows: ready_trigger == AppReadyTrigger::HostInitialized,
        },
    )?;
    guardian_owner.attach_router(router.handle())?;
    let (window_events_tx, window_events_rx) = mpsc::channel();
    let mut dialog_presentation = router.handle.dialog.as_ref().map(|session| {
        dialog_session::DialogMainOwner::new(Arc::clone(session), window_events_tx.clone())
    });
    let router_handle = router.handle();
    let commands_for_events = window_commands_tx.clone();
    let init_deadline = Arc::new(OnceLock::new());
    let coordinator_deadline = Arc::clone(&init_deadline);
    let event_coordinator = thread::Builder::new()
        .name("keld-core-app-window-events".to_owned())
        .spawn(move || {
            coordinate_window_events(
                &window_events_rx,
                &router_handle,
                &commands_for_events,
                ready_trigger,
                |_| Ok(()),
                Some((&coordinator_deadline, pre_init_terminal)),
            )
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
                (router, presentation_owner, main_thread),
                guardian_owner,
            )
            .map_err(HostRunFailure::from);
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
                (router, presentation_owner, main_thread),
                guardian_owner,
            )
            .map_err(HostRunFailure::from);
        }
    };
    // Only renderer-declared boots create a host window. In a facade boot the
    // first window belongs to the application's later authenticated Create.
    // Do not fabricate an invisible anchor: a zero-window Init is observable.
    let primary_view = if let Some(html) = html {
        let spec = WebviewSpec {
            title: name,
            initial: NavTarget::Html(html),
            ..WebviewSpec::default()
        };
        WINDOW_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
        // GH-508: the renderer bridge admits only the host's echo channel.
        let renderer_endpoint = RendererBridgeEndpoint::new(
            renderer_requests_tx,
            renderer_outcomes_rx,
            keld_ipc::channel_table::ECHO.wire_id(),
        );
        match engine.create_app_with_renderer_bridge(
            &spec,
            window_events_tx.clone(),
            renderer_endpoint,
        ) {
            Ok(view) => Some(view),
            Err(source) => {
                let primary = app_webview_error("initial renderer bridge window", &source);
                return cleanup_window_start_failure(
                    &primary,
                    guard_snapshot,
                    window_events_tx,
                    event_coordinator,
                    Some(renderer_dispatch),
                    (router, presentation_owner, main_thread),
                    guardian_owner,
                )
                .map_err(HostRunFailure::from);
            }
        }
    } else {
        drop(renderer_requests_tx);
        drop(renderer_outcomes_rx);
        None
    };
    let presentation_router = router.handle();
    let mut presentation_unproved = None;
    let mut retains_main_custody = false;
    let window_result = engine.run_app_until_quit_with_main_owner(
        window_commands_rx,
        window_events_tx,
        primary_view,
        ready_trigger,
        &mut |main_thread, anchor, dialog_parent, stopping| {
            if let Err(error) = presentation_router.window_main_owner_stopping(stopping) {
                let detail = error.to_string();
                presentation_unproved.get_or_insert(error);
                return Err(WvError::EventLoop(detail));
            }
            let stop_scope = presentation_router.prepare_auth_presentation_drive(stopping);
            let (auth_drained, auth_deadline) =
                match presentation_owner.drive(main_thread, &mut |_| anchor(), stop_scope) {
                    keld_native::auth::AsMainProgress::Pending { next_deadline } => {
                        (false, next_deadline)
                    }
                    keld_native::auth::AsMainProgress::Drained => (true, None),
                    keld_native::auth::AsMainProgress::Unproved { fault, .. } => {
                        presentation_unproved.get_or_insert_with(|| {
                            app_detail("auth presentation custody", format!("{fault:?}"))
                        });
                        return Err(WvError::EventLoop(format!(
                            "native presentation custody unproved: {fault:?}"
                        )));
                    }
                };
            let (dialog_drained, dialog_deadline) = if let Some(dialog) = &mut dialog_presentation {
                dialog
                    .drive(main_thread, dialog_parent, stopping)
                    .map_err(|error| {
                        let detail = error.to_string();
                        presentation_unproved.get_or_insert(error);
                        WvError::EventLoop(detail)
                    })?
            } else {
                (true, None)
            };
            Ok((
                auth_drained && dialog_drained,
                match (auth_deadline, dialog_deadline) {
                    (Some(auth), Some(dialog)) => Some(auth.min(dialog)),
                    (auth, dialog) => auth.or(dialog),
                },
            ))
        },
        (&mut retains_main_custody, || {
            if ready_trigger == AppReadyTrigger::HostInitialized {
                let _ = init_deadline.set(Instant::now() + keld_wv::INITIAL_MILESTONE_DEADLINE);
            }
        }),
    );
    if retains_main_custody {
        // This is the owned fatal branch, not ordinary view-close intent.
        presentation_router.shutdown.stop_reader();
        // No blocking joins, profile/stage deletion, or owner Drop on this cut.
        let loop_error = window_result.err().map_or_else(
            || app_detail("main presentation tail", "main custody was not released"),
            |error| app_webview_error("main presentation tail", &error),
        );
        let error = match presentation_unproved {
            Some(native) => collapse_app_failures(&loop_error, [Err(native)]),
            None => loop_error,
        };
        return Err(HostRunFailure {
            error,
            retained: Some(Box::new(RetainedMacSession {
                _engine: engine,
                _presentation: presentation_owner,
                _dialog_main: dialog_presentation,
                _router: router,
                _guardian: guardian_owner,
                _coordinator: event_coordinator,
                _renderer: renderer_dispatch,
                _commands: window_commands_tx,
                guard: None,
            })),
        });
    }
    // The UI service has now returned a proved drained result. It retained a
    // dedicated AppWindowEvent sender for real native sheet completions; if we
    // join the event coordinator while this sender is still owned here, its
    // receiver can never observe Disconnected on a facade-window session.
    // Release only after the native main owner has finished its callback and
    // physical sheet custody; the fatal/unproved branch above retains it.
    drop(dialog_presentation.take());
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
    .map_err(HostRunFailure::from)
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

/// Selected once per process at boot; every application session of the process
/// constructs its engine from a copy.
#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
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

/// Inputs that outlive every application session of one host process: the boot
/// selection's paths and renderer, the profile mode selected at boot and the verified
/// guard snapshot. A session borrows them and owns nothing here.
#[cfg(any(target_os = "linux", windows))]
struct DirectSessionInputs<'a> {
    root: &'a Path,
    name: &'a str,
    entry_path: &'a Path,
    html: &'a str,
    #[cfg(windows)]
    windows_profile_mode: WindowsProfileMode,
    guard_snapshot: Option<&'a GuardSnapshot>,
}

/// What the session loop does once an application session has returned.
#[cfg(any(target_os = "linux", windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectSessionNext {
    /// Return the session's result from `run_app_direct`: the host exits.
    Exit,
    /// Run one more full application session in this process.
    RunAnother,
}

/// How an `Ok` application session ended; carried by the session's result, never
/// inferred from the session cause.
#[cfg(any(target_os = "linux", windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectSessionEnd {
    /// The UI loop ended on a tail's `Quit`: the application's own Quit, or CLI lease
    /// loss, with no handover.
    Ordinary,
    /// A profile handover ended the UI loop first, and the application session then
    /// ended under the lifecycle-Quit cause (its Quit inside D) or under
    /// [`SESSION_HANDOVER`] (forced at D).
    Handover,
}

/// Decides the loop's post-session action from the facts a session left behind
/// (KEL-53 §4 "Profile handover", *Rollback and restore*; §5 `run_app_direct`).
///
/// Another session follows only a session that returned `Ok` and that a requester
/// asked to follow: one under the application's lifecycle-Quit cause, or one a
/// handover ended (under either of its causes). A fatal, CLI lease loss or an error
/// always exits, as does a handover-cause session whose result does not carry the
/// handover fact. The only requester today is the debug-only evidence seam, so
/// production runs exactly one session.
#[cfg(any(target_os = "linux", windows))]
const fn next_direct_session(
    ended_ok: bool,
    cause: u8,
    ended_by_handover: bool,
    another_requested: bool,
) -> DirectSessionNext {
    if !ended_ok || !another_requested {
        return DirectSessionNext::Exit;
    }
    if cause == SESSION_LIFECYCLE_QUIT || (ended_by_handover && cause == SESSION_HANDOVER) {
        DirectSessionNext::RunAnother
    } else {
        DirectSessionNext::Exit
    }
}

/// Debug-only S6c evidence triggers, in the shape of `KELD_TEST_WINDOWS_LEASE_CENSUS`,
/// read once per process and absent from release builds (KEL-53 §7 "8, 20 (profile
/// handover)"): `KELD_TEST_WINDOWS_SECOND_SESSION=1` asks the session loop for exactly
/// one more application session after the first ended under the lifecycle-Quit cause
/// (cell h5).
#[cfg(all(debug_assertions, windows))]
const TEST_WINDOWS_SECOND_SESSION_ENV: &str = "KELD_TEST_WINDOWS_SECOND_SESSION";
/// `KELD_TEST_WINDOWS_HANDOVER=1` requests a profile handover right after the first
/// session's `Ready` and asks the loop for one more session after it (cells h11, h12
/// and the tail half of h9).
#[cfg(all(debug_assertions, windows))]
const TEST_WINDOWS_HANDOVER_ENV: &str = "KELD_TEST_WINDOWS_HANDOVER";

/// The one owner of both evidence triggers: the remaining extra sessions the seam may
/// request, and whether the first session's `Ready` requests a handover.
#[cfg(all(debug_assertions, windows))]
#[derive(Debug)]
struct TestSessionSeam {
    remaining: u8,
    handover_at_ready: bool,
}

#[cfg(all(debug_assertions, windows))]
impl TestSessionSeam {
    fn from_environment(has_dev_lease: bool) -> Result<Self, HostAppError> {
        let requested = |name| std::env::var_os(name).as_deref() == Some(std::ffi::OsStr::new("1"));
        Self::from_request(
            requested(TEST_WINDOWS_SECOND_SESSION_ENV),
            requested(TEST_WINDOWS_HANDOVER_ENV),
            has_dev_lease,
        )
    }

    /// A dev-stage host runs under its CLI lease, whose standard-input monitor belongs
    /// to the first session only, so the seam refuses a dev lease as the lease-census
    /// seam refuses the lack of one; one process carries one trigger.
    fn from_request(
        second_session: bool,
        handover: bool,
        has_dev_lease: bool,
    ) -> Result<Self, HostAppError> {
        if second_session && has_dev_lease {
            return Err(app_detail(
                "Windows session loop",
                "test second session requested under a dev lease",
            ));
        }
        if handover && has_dev_lease {
            return Err(app_detail(
                "Windows session loop",
                "test handover requested under a dev lease",
            ));
        }
        if second_session && handover {
            return Err(app_detail(
                "Windows session loop",
                "test second session and test handover requested together",
            ));
        }
        Ok(Self {
            remaining: u8::from(second_session || handover),
            handover_at_ready: handover,
        })
    }

    /// Consumes one requested extra session, if any remains.
    fn take_request(&mut self) -> bool {
        let requested = self.remaining > 0;
        self.remaining = self.remaining.saturating_sub(1);
        requested
    }

    /// Consumes the handover request, which belongs to the first session only.
    fn take_handover_at_ready(&mut self) -> bool {
        std::mem::take(&mut self.handover_at_ready)
    }
}

/// Steps 3a to 3c of KEL-53 §4 "Profile handover" once the UI loop has returned under
/// a handover request: the host writes `LastWindowClosed` itself (the `Quit` path of the
/// loop emits none), waits up to [`HANDOVER_QUIT_DEADLINE`] for the application's `Quit`,
/// and otherwise claims [`SESSION_HANDOVER`] and runs the forced tail on this thread;
/// when the application's `Quit` won, this thread awaits that tail's end before the
/// teardown, as it awaited the loop's return before. A `LastWindowClosed` write that
/// fails ends the session at once under the handover cause (fail closed, no D). D runs
/// from the return of this function's own write alone, recorded in the session's
/// [`HandoverClock`] beside the loop's return and the forced claim. `quit_deadline` is
/// D: [`HANDOVER_QUIT_DEADLINE`] in the host; the unit cells pass a shorter one.
#[cfg(any(target_os = "linux", windows))]
fn complete_profile_handover(
    router: &PrimaryRouterHandle,
    shutdown: &SessionShutdownState,
    quit_deadline: Duration,
) -> Result<(), HostAppError> {
    let clock = &shutdown.handover_clock;
    let _ = clock.ui_returned.set(Instant::now());
    let written = router.signal_last_window_closed();
    let deadline = match &written {
        Ok(written_at) => {
            let _ = clock.last_window_closed_written.set(*written_at);
            *written_at + quit_deadline
        }
        Err(_) => Instant::now(),
    };
    let tail = match shutdown.await_quit_or_claim_handover(deadline) {
        HandoverWait::ClaimedHandover => {
            let _ = clock.forced.set(Instant::now());
            router.cli_lease_lost()
        }
        HandoverWait::ClaimedByOther => {
            shutdown.await_tail_end();
            Ok(())
        }
    };
    collapse_app_results([written.map(drop), tail])
}

#[cfg(all(debug_assertions, windows))]
impl HandoverClock {
    /// The seam's evidence line for one completed handover: the cause and the three
    /// intervals in microseconds (request to loop return; `LastWindowClosed` write to
    /// the lifecycle-Quit handler's entry for the frame that claimed the session, the D
    /// sample; write to the forced claim at D).
    fn evidence_json(&self, cause: u8) -> String {
        let micros = |from: Option<&Instant>, to: Option<&Instant>| match (from, to) {
            (Some(from), Some(to)) => to.saturating_duration_since(*from).as_micros().to_string(),
            _ => String::from("null"),
        };
        let cause = match cause {
            SESSION_LIFECYCLE_QUIT => "lifecycle-quit",
            SESSION_HANDOVER => "forced",
            SESSION_CLI_LEASE_LOST => "cli-lease-lost",
            _ => "running",
        };
        let written = self.last_window_closed_written.get();
        format!(
            "{{\"cause\":\"{cause}\",\"ui_end_us\":{},\"quit_wait_us\":{},\"forced_after_us\":{},\"deadline_us\":{}}}",
            micros(self.requested.get(), self.ui_returned.get()),
            micros(written, self.quit_frame.get()),
            micros(written, self.forced.get()),
            HANDOVER_QUIT_DEADLINE.as_micros()
        )
    }
}

/// The Bun primary command of one Windows application session. Test-only variables
/// never reach the child: the lease-census seam hands over the host lease handle only
/// under a live dev lease, and the session-loop seams and the keld-wv barrier fault
/// fixture are removed from the child.
#[cfg(windows)]
fn windows_primary_config(
    root: &Path,
    entry_path: &Path,
    dev_lease: Option<&WindowsDevLeaseMonitor>,
) -> Result<PrimaryRoleConfig, HostAppError> {
    let config = PrimaryRoleConfig::new("bun")
        .arg("run")
        .arg(root.join(entry_path))
        .current_dir(root)
        // This boot path owns a legacy renderer; inherited Host hints cannot
        // select the facade subscription barrier in any Bun generation.
        .env(NATIVE_WINDOW_FACADE_ENV, "0")
        .env_remove(DEV_LEASE_ENV)
        .env_remove(keld_runtime::windows_job::WINDOWS_LAUNCH_GATE_ENV);
    #[cfg(debug_assertions)]
    let config = config
        .env_remove(TEST_WINDOWS_SECOND_SESSION_ENV)
        .env_remove(TEST_WINDOWS_HANDOVER_ENV);
    #[cfg(any(feature = "profile-test-hooks", feature = "kel135-profile-test-root"))]
    let config = config.env_remove(keld_wv::TEST_WINDOWS_BARRIER_GRACE_EXPIRY_ENV);
    #[cfg(debug_assertions)]
    let config = if std::env::var_os("KELD_TEST_WINDOWS_LEASE_CENSUS").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        let handle = dev_lease.ok_or_else(|| {
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
    #[cfg(not(debug_assertions))]
    let _ = dev_lease;
    Ok(config)
}

/// Runs the host's application sessions: a per-process prelude, then a loop of full
/// sessions on the main thread (KEL-53 §5 `run_app_direct`). A session ends by the
/// application's Quit, a fatal or CLI lease loss; `next_direct_session` decides whether
/// another follows. Today nothing but the debug-only evidence seam requests one.
#[cfg(any(target_os = "linux", windows))]
fn run_app_direct(
    boot: AppBootSelection,
    guard_snapshot: Option<&GuardSnapshot>,
) -> Result<(), HostAppError> {
    let AppBootSelection {
        root,
        name,
        entry_path,
        entry_file,
        initial_window,
        auth_configuration: _auth_configuration,
        #[cfg(windows)]
        mode,
    } = boot;
    // Held until the last session returns, so the installed version tree every
    // session runs from stays pinned through teardown.
    #[cfg(windows)]
    let (installed_identity, _installed_package) = match mode {
        WindowsBootMode::DevStage => (None, None),
        WindowsBootMode::Installed { identity, active } => (Some(identity), Some(active)),
    };
    let renderer_html = match initial_window {
        ValidatedInitialWindow::Renderer(html) => html,
        ValidatedInitialWindow::None => {
            return Err(HostAppError::new(
                "KELD-CORE-034",
                "facade boot platform",
                "schema 2 is supported on macOS only",
                "Remove initialWindow from keld.config.ts to use a renderer-declared boot on this platform.",
            ));
        }
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
    // Per-process state ends below; everything a session owns is created inside
    // `run_direct_app_session`. The first session takes the CLI dev-lease monitor,
    // whose standard-input reader lives for the process, so a dev-stage host runs one
    // session; the `PerUserDirect` handover and restore run without a lease.
    let shutdown = SessionShutdownState::new();
    #[cfg(windows)]
    let dev_lease = prepare_windows_dev_lease(&shutdown)?;
    #[cfg(windows)]
    let windows_profile_mode =
        select_windows_profile_mode(installed_identity, dev_lease.is_some())?;
    #[cfg(all(debug_assertions, windows))]
    let mut session_seam = TestSessionSeam::from_environment(dev_lease.is_some())?;
    #[cfg(target_os = "linux")]
    let mut dev_lease = DevHostLease::from_environment()?;
    #[cfg(target_os = "linux")]
    if let Some(lease) = dev_lease.as_mut()
        && lease.poll_lost()?
    {
        return Ok(());
    }
    let inputs = DirectSessionInputs {
        root: &root,
        name: &name,
        entry_path: &entry_path,
        html: &html,
        #[cfg(windows)]
        windows_profile_mode,
        guard_snapshot,
    };
    let mut session = (shutdown, dev_lease);
    loop {
        let (shutdown, dev_lease) = session;
        #[cfg(all(debug_assertions, windows))]
        let handover_at_ready = session_seam.take_handover_at_ready();
        let result = run_direct_app_session(
            &inputs,
            &shutdown,
            dev_lease,
            #[cfg(all(debug_assertions, windows))]
            handover_at_ready,
        );
        #[cfg(all(debug_assertions, windows))]
        let another_requested = session_seam.take_request();
        #[cfg(not(all(debug_assertions, windows)))]
        let another_requested = false;
        let ended_by_handover = matches!(result, Ok(DirectSessionEnd::Handover));
        match next_direct_session(
            result.is_ok(),
            shutdown.cause(),
            ended_by_handover,
            another_requested,
        ) {
            DirectSessionNext::Exit => return result.map(|_| ()),
            DirectSessionNext::RunAnother => {
                // The next session's guard preflight, before any of its resources.
                if let Some(snapshot) = inputs.guard_snapshot {
                    snapshot.prepare_next_session()?;
                }
                session = (SessionShutdownState::new(), None);
            }
        }
    }
}

/// One full application session: the Bun primary, the router, the window and its UI
/// loop, then the ordered teardown. Every session of one process, the first and each
/// one `run_app_direct` runs after it, is this same body.
#[cfg(any(target_os = "linux", windows))]
#[allow(clippy::too_many_lines)] // one shared direct-owner state machine keeps Linux/Windows lifecycle transitions identical
fn run_direct_app_session(
    inputs: &DirectSessionInputs<'_>,
    shutdown: &SessionShutdownState,
    #[cfg(windows)] dev_lease: Option<WindowsDevLeaseMonitor>,
    #[cfg(target_os = "linux")] dev_lease: Option<DevHostLease>,
    #[cfg(all(debug_assertions, windows))] handover_at_ready: bool,
) -> Result<DirectSessionEnd, HostAppError> {
    let prestart_lease_result = || {
        #[cfg(windows)]
        {
            take_direct_prestart_lease_result(dev_lease.as_ref())
                .map(|()| DirectSessionEnd::Ordinary)
        }
        #[cfg(target_os = "linux")]
        {
            Ok(DirectSessionEnd::Ordinary)
        }
    };
    #[cfg(target_os = "linux")]
    let direct_engine = WebKitGtkEngine::new()
        .map_err(|source| app_detail("Linux GPU safe mode", source.to_string()))?;
    #[cfg(target_os = "linux")]
    let config = linux_strict_primary_config(inputs.root, inputs.entry_path)?;
    #[cfg(windows)]
    let config = windows_primary_config(inputs.root, inputs.entry_path, dev_lease.as_ref())?;
    LISTENER_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    CHILD_ATTEMPTS.fetch_add(1, Ordering::AcqRel);
    let supervisor = run_direct_startup_if_session_running(shutdown, || {
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
        shutdown,
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
                return collapse_app_results([prestart_lease_result().map(|_| ()), cleanup])
                    .map(|()| DirectSessionEnd::Ordinary);
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
        return collapse_app_results([prestart_lease_result().map(|_| ()), cleanup])
            .map(|()| DirectSessionEnd::Ordinary);
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
        inputs.guard_snapshot,
    )?;
    primary_owner.attach_router(router.handle())?;
    let lease_errors = start_direct_dev_lease_tail(dev_lease, router.handle())?;
    let (window_events_tx, window_events_rx) = mpsc::channel();
    let router_handle = router.handle();
    let commands_for_events = window_commands_tx.clone();
    // The evidence seam's handover request, right after this session's `Ready` (the
    // recovery arm acknowledged): the shape of the coordinator's production request.
    #[cfg(all(debug_assertions, windows))]
    let on_ready = move |router: &PrimaryRouterHandle| -> Result<(), HostAppError> {
        if handover_at_ready {
            router.request_handover().map(|_| ())
        } else {
            Ok(())
        }
    };
    #[cfg(not(all(debug_assertions, windows)))]
    let on_ready = |_: &PrimaryRouterHandle| -> Result<(), HostAppError> { Ok(()) };
    let event_coordinator = thread::Builder::new()
        .name("keld-core-direct-app-window-events".to_owned())
        .spawn(move || {
            coordinate_window_events(
                &window_events_rx,
                &router_handle,
                &commands_for_events,
                on_ready,
            )
        })
        .map_err(|source| app_io("direct window event coordinator", &source))?;

    let spec = WebviewSpec {
        title: inputs.name.to_owned(),
        initial: NavTarget::Html(inputs.html.to_owned()),
        ..WebviewSpec::default()
    };
    let engine = run_direct_startup_if_session_running(shutdown, || {
        #[cfg(windows)]
        let mut direct_engine = match inputs.windows_profile_mode {
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
        let _retained_digest = inputs
            .guard_snapshot
            .map(|snapshot| snapshot.verified.verified_sha256());
        return collapse_app_results([lease_result, event_result, router_result, owner_result])
            .map(|()| DirectSessionEnd::Ordinary);
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
            let _retained_digest = inputs
                .guard_snapshot
                .map(|snapshot| snapshot.verified.verified_sha256());
            return Err(collapse_app_failures(
                &primary,
                [lease_result, event_result, router_result, owner_result],
            ));
        }
    };
    let window_result = engine.run_app_until_quit(window_commands_rx, window_events_tx);
    drop(window_commands_tx);
    // A handover ended the loop: the application session is still live and ends here
    // (KEL-53 §4 "Profile handover", steps 3a to 3c). A loop that returned an error
    // under a handover is a failed tail: no step follows, the session fails.
    let ended_by_handover = shutdown.handover_requested_guarded();
    let handover_result = match (&window_result, ended_by_handover) {
        (Ok(()), true) => {
            complete_profile_handover(&router.handle(), shutdown, HANDOVER_QUIT_DEADLINE)
        }
        _ => Ok(()),
    };
    #[cfg(all(debug_assertions, windows))]
    if handover_at_ready && window_result.is_ok() && handover_result.is_ok() {
        println!(
            "KELD_TEST_WINDOWS_HANDOVER {}",
            shutdown.handover_clock.evidence_json(shutdown.cause())
        );
    }
    let session_end = if ended_by_handover {
        DirectSessionEnd::Handover
    } else {
        DirectSessionEnd::Ordinary
    };
    let event_result = event_coordinator
        .join()
        .map_err(|_| app_detail("direct window event coordinator", "thread panicked"))
        .and_then(std::convert::identity);
    let lease_result = take_direct_lease_error(lease_errors.as_ref());
    let router_result = router.shutdown();
    let owner_result = primary_owner.shutdown();
    let _retained_digest = inputs
        .guard_snapshot
        .map(|snapshot| snapshot.verified.verified_sha256());

    let owner_result = match owner_result {
        Err(primary) if primary.code == "KELD-CORE-033" => {
            return Err(append_app_cleanup(
                primary,
                [
                    lease_result,
                    window_result
                        .map_err(|source| app_detail("direct app window", source.to_string())),
                    handover_result,
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
            handover_result,
            event_result,
            router_result,
            owner_result,
        ])
        .map(|()| session_end),
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
#[cfg(not(keld_core_native_renderer_main))]
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
    router.shutdown.mark_under_dev_lease();
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
    router.shutdown.mark_under_dev_lease();
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
                if request.channel() != keld_ipc::channel_table::ECHO.wire_id() {
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
    presentation: (
        PrimaryRouter,
        keld_native::auth::AsMainOwner,
        objc2::MainThreadMarker,
    ),
    guardian_owner: GuardianOwner,
) -> Result<(), HostAppError> {
    let (router, mut owner, main_thread) = presentation;
    if let Some(auth) = &router.handle.auth {
        auth.stop_tail();
    }
    router.handle.shutdown.stop_reader();
    // No main SDK owner has been created before this loop starts. Native closes
    // its port synchronously so queued starts and the worker cannot await a UI
    // loop that failed to start. This does not substitute for the worker tail.
    let presentation_result = match owner.drive(
        main_thread,
        &mut |_| None,
        keld_native::auth::AsMainStop::HostTail,
    ) {
        keld_native::auth::AsMainProgress::Drained => Ok(()),
        keld_native::auth::AsMainProgress::Unproved { fault, .. } => Err(app_detail(
            "auth presentation startup cleanup",
            format!("{fault:?}"),
        )),
        keld_native::auth::AsMainProgress::Pending { .. } => Err(app_detail(
            "auth presentation startup cleanup",
            "pre-loop owner remains pending",
        )),
    };
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
                presentation_result,
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
            // Lease loss is a host shutdown, not a Bun-accepted Quit. Before
            // the first generation exists the Guardian refuses Q; orderly S
            // is acknowledged at every startup stage and denies recovery
            // under the lock that orders it against initial PREPARE.
            guardian.shutdown().map_err(|source| {
                app_guardian_fatal(
                    "orderly shutdown after dev lease loss before Bound",
                    &source,
                )
            })?;
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

/// Forwards the UI loop's milestones to the router; `on_ready` runs once `Ready` has
/// been signalled and its recovery arm acknowledged (the evidence seam's handover
/// request on the direct path; a no-op elsewhere).
#[cfg(target_os = "macos")]
type InitTerminalWatch<'a> = Option<(&'a OnceLock<Instant>, fn(&HostAppError) -> !)>;

#[cfg(target_os = "macos")]
fn initial_milestone_poll(until: Option<Instant>, now: Instant) -> Duration {
    until.map_or(APP_LINK_READER_POLL, |deadline| {
        APP_LINK_READER_POLL.min(deadline.saturating_duration_since(now))
    })
}

/// The absolute first-Init deadline applies to successful queued receives as
/// well as an empty queue. A genuine on-time UI first turn keeps its native
/// timestamp even when this worker is descheduled before observing the event.
#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq, Eq)]
enum InitialMilestoneReceive {
    Event(AppWindowEvent),
    Again,
    Expired,
    Disconnected,
}

#[cfg(target_os = "macos")]
fn receive_initial_milestone(
    events: &Receiver<AppWindowEvent>,
    watch: Option<&OnceLock<Instant>>,
) -> InitialMilestoneReceive {
    let before_receive = watch.and_then(|clock| clock.get().copied());
    let poll = initial_milestone_poll(before_receive, Instant::now());
    let received = events.recv_timeout(poll);
    // The coordinator can begin waiting before the UI thread arms its first
    // loop-entry deadline. Re-read after receipt; real Init is sent only after
    // the native producer has armed this same OnceLock.
    let deadline = watch.and_then(|clock| clock.get().copied());
    match received {
        Ok(AppWindowEvent::HostInitialized { observed_at })
            if watch.is_some() && deadline.is_none_or(|at| observed_at >= at) =>
        {
            InitialMilestoneReceive::Expired
        }
        Ok(event)
            if deadline.is_some_and(|at| {
                !matches!(event, AppWindowEvent::HostInitialized { .. }) && Instant::now() >= at
            }) =>
        {
            // A busy queue of non-Init events must not starve the watchdog.
            InitialMilestoneReceive::Expired
        }
        Ok(event) => InitialMilestoneReceive::Event(event),
        Err(mpsc::RecvTimeoutError::Timeout) if deadline.is_some_and(|at| Instant::now() >= at) => {
            InitialMilestoneReceive::Expired
        }
        Err(mpsc::RecvTimeoutError::Timeout) => InitialMilestoneReceive::Again,
        Err(mpsc::RecvTimeoutError::Disconnected) => InitialMilestoneReceive::Disconnected,
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn coordinate_window_events(
    events: &Receiver<AppWindowEvent>,
    router: &PrimaryRouterHandle,
    commands: &Sender<AppWindowCommand>,
    #[cfg(target_os = "macos")] ready_trigger: AppReadyTrigger,
    mut on_ready: impl FnMut(&PrimaryRouterHandle) -> Result<(), HostAppError>,
    #[cfg(target_os = "macos")] init_watch: InitTerminalWatch<'_>,
) -> Result<(), HostAppError> {
    #[cfg(target_os = "macos")]
    let mut ready_signaled = false;
    #[cfg(target_os = "macos")]
    let init_seen = std::cell::Cell::new(false);
    #[cfg(target_os = "macos")]
    let mut signal_once = |observed| {
        accept_boot_ready_trigger(ready_trigger, observed, &mut ready_signaled)?;
        if observed == AppReadyTrigger::HostInitialized {
            init_seen.set(true);
        }
        router.signal_ready().and_then(|()| on_ready(router))
    };
    let mut process_event = |event| {
        let result = match event {
            AppWindowEvent::NavigationReady => {
                #[cfg(target_os = "macos")]
                {
                    signal_once(AppReadyTrigger::NavigationReady)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    if router.windows.is_none() {
                        router.signal_ready().and_then(|()| on_ready(router))
                    } else {
                        Ok(())
                    }
                }
            }
            #[cfg(target_os = "macos")]
            AppWindowEvent::HostInitialized { .. } => signal_once(AppReadyTrigger::HostInitialized),
            #[cfg(target_os = "macos")]
            AppWindowEvent::MacDialogFinished {
                attempt,
                nonce,
                outcome,
            } => router.dialog_native_finished(attempt, nonce, outcome),
            AppWindowEvent::LastWindowClosed if router.windows.is_none() => {
                router.signal_last_window_closed().map(drop)
            }
            // Registry windows own their count; never emit a legacy count.
            AppWindowEvent::LastWindowClosed => Ok(()),
            event => router.window_native_event(event),
        };
        if let Err(error) = result {
            let _ = commands.send(AppWindowCommand::Fatal);
            Err(error)
        } else {
            Ok(())
        }
    };
    #[cfg(target_os = "macos")]
    loop {
        if router.shutdown.reader_stop.load(Ordering::Acquire) && router.windows.is_none() {
            break;
        }
        let until_init = init_watch
            .filter(|_| ready_trigger == AppReadyTrigger::HostInitialized && !init_seen.get());
        let event = match receive_initial_milestone(events, until_init.map(|(clock, _)| clock)) {
            InitialMilestoneReceive::Event(event) => {
                if router.shutdown.reader_stop.load(Ordering::Acquire) && router.windows.is_none() {
                    break;
                }
                event
            }
            InitialMilestoneReceive::Expired => {
                if let Some((_, terminal)) = until_init {
                    terminate_stalled_init(router, terminal);
                }
                return Err(app_detail(
                    "initial UI milestone",
                    "expired first-Init observation without an armed deadline",
                ));
            }
            InitialMilestoneReceive::Again => continue,
            InitialMilestoneReceive::Disconnected if until_init.is_some() => {
                return Err(app_detail(
                    "initial UI milestone",
                    "window-module event source ended before the first Init turn",
                ));
            }
            InitialMilestoneReceive::Disconnected => break,
        };
        process_event(event)?;
    }
    #[cfg(not(target_os = "macos"))]
    while let Ok(event) = events.recv() {
        process_event(event)?;
    }
    router.finish_window_facts()
}

/// The UI loop cannot deliver Fatal before its initial Tao turn. Its blocked
/// main stack still owns `!Send` `AppKit`, profile and presentation resources.
/// Only thread-safe, bounded authority/child owners run on this worker.
#[cfg(target_os = "macos")]
fn terminate_stalled_init(router: &PrimaryRouterHandle, terminal: fn(&HostAppError) -> !) -> ! {
    let error = app_detail(
        "initial UI milestone",
        "the UI loop did not deliver its first turn within 15 s",
    );
    router.guardian.deny_recovery();
    router.shutdown.reader_stop.store(true, Ordering::Release);
    // Do not block on a reader/native handler holding one of these locks.
    if let Ok(readers) = router.shutdown.generation_reader_stops.try_lock() {
        for reader in readers.iter() {
            reader.store(true, Ordering::Release);
        }
    }
    if let Some(fs) = router.fs.as_ref().and_then(Weak::upgrade)
        && let Ok(mut state) = fs.state.try_lock()
    {
        state.accepting = false;
    }
    // The Runtime owner terminates/reaps the real supervised process group;
    // this reply has a separate 6s bound. Even a failed receipt must not let
    // Core unwind or drop the main-thread stack.
    if let Err(cleanup) = router.guardian.shutdown_and_wait() {
        eprintln!("{error}; pre-Init guardian cleanup unproved: {cleanup}");
    }
    terminal(&error)
}

#[cfg(target_os = "macos")]
fn accept_boot_ready_trigger(
    expected: AppReadyTrigger,
    observed: AppReadyTrigger,
    already_signaled: &mut bool,
) -> Result<(), HostAppError> {
    if expected != observed || *already_signaled {
        return Err(HostAppError::new(
            "KELD-CORE-044",
            "application Ready milestone",
            format!("unexpected or repeated {observed:?} for {expected:?} boot"),
            "Deliver exactly the selected boot kind's first Ready milestone.",
        ));
    }
    *already_signaled = true;
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
        Box<PrimaryRouterHandle>,
        std::sync::mpsc::SyncSender<Result<(), String>>,
    ),
    ArmRecovery(RecoveryArm, std::sync::mpsc::SyncSender<Result<(), String>>),
    DenyRecovery,
    FailGeneration(u32, std::sync::mpsc::SyncSender<Result<(), String>>),
    FailRetiredGeneration(u32, std::sync::mpsc::SyncSender<Result<Instant, String>>),
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
                let mut initial_arm = false;
                let mut fs_recovery: Option<FsRecoveryAttempt> = None;
                loop {
                    match command_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(GuardianOwnerCommand::AttachRouter(attached, reply)) => {
                            let attached = *attached;
                            let observed = if router.is_some() {
                                Err(String::from("router is already attached"))
                            } else {
                                router = Some(attached);
                                Ok(())
                            };
                            let _ = reply.send(observed);
                        }
                        Ok(GuardianOwnerCommand::PrepareAcceptedShutdown(reply)) => {
                            guardian.deny_recovery();
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
                        Ok(GuardianOwnerCommand::ArmRecovery(phase, reply)) => {
                            let result = apply_guardian_recovery_arm(
                                phase,
                                &mut guardian,
                                router.as_ref(),
                                &mut initial_arm,
                                fs_recovery.as_mut(),
                            );
                            let observed = result
                                .as_ref()
                                .copied()
                                .map_err(std::string::ToString::to_string);
                            let _ = reply.send(observed);
                            result?;
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
                                .map(|()| Instant::now())
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
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => {
                            return guardian.shutdown().map_err(|source| {
                                app_guardian_fatal("guardian owner disconnect", &source)
                            });
                        }
                    }
                    for _ in 0..64 {
                        let Some(update) = guardian.recv_update(Duration::ZERO) else {
                            break;
                        };
                        let Some(router) = &router else {
                            if matches!(&update, GuardedPrimaryUpdate::Role(_))
                                && !matches!(
                                    &update,
                                    GuardedPrimaryUpdate::Role(PrimaryRoleEvent::Revoked { .. })
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
                        let retired = match &update {
                            GuardedPrimaryUpdate::Role(PrimaryRoleEvent::Revoked {
                                attempt,
                                ..
                            }) => Some(*attempt),
                            _ => None,
                        };
                        let bound_attempt = match &update {
                            GuardedPrimaryUpdate::Bound(bound) => Some(bound.attempt()),
                            GuardedPrimaryUpdate::Role(_) => None,
                        };
                        let already_retired = router.last_revoked_attempt.load(Ordering::Acquire);
                        if let Err(error) = router.apply_generation_update(update.into()) {
                            guardian.deny_recovery();
                            let _ = window_commands.send(AppWindowCommand::Fatal);
                            return Err(error.into());
                        }
                        if let Some(attempt) = retired.filter(|attempt| *attempt > already_retired)
                            && router.shutdown.is_running()
                        {
                            if fs_recovery
                                .as_ref()
                                .is_some_and(|recovery| !recovery.released)
                            {
                                guardian.deny_recovery();
                                let _ = window_commands.send(AppWindowCommand::Fatal);
                                return Err(app_detail(
                                    "filesystem recovery",
                                    "overlapping native retirement attempts",
                                ));
                            }
                            fs_recovery = match FsRecoveryAttempt::retire(router, attempt) {
                                Ok(recovery) => Some(recovery),
                                Err(error) => {
                                    guardian.deny_recovery();
                                    let _ = window_commands.send(AppWindowCommand::Fatal);
                                    return Err(error);
                                }
                            };
                        }
                        if bound_attempt.is_some_and(|attempt| {
                            fs_recovery.as_ref().is_some_and(|recovery| {
                                recovery.prepared == attempt && recovery.released
                            })
                        }) {
                            fs_recovery = None;
                        }
                    }
                    if let (Some(router), Some(recovery)) = (router.as_ref(), fs_recovery.as_mut())
                    {
                        if let Err(error) = recovery.poll(router) {
                            guardian.deny_recovery();
                            let _ = window_commands.send(AppWindowCommand::Fatal);
                            return Err(error);
                        }
                        let phase = RecoveryArm::Retired {
                            retired: recovery.retired,
                            prepared: recovery.prepared,
                        };
                        if let Err(error) = apply_guardian_recovery_arm(
                            phase,
                            &mut guardian,
                            Some(router),
                            &mut initial_arm,
                            Some(recovery),
                        ) {
                            guardian.deny_recovery();
                            let _ = window_commands.send(AppWindowCommand::Fatal);
                            return Err(error);
                        }
                    }
                    if let Some(lease) = dev_lease.as_mut() {
                        match lease.poll_lost() {
                            Ok(true) => {
                                if shutdown.reserve_cli_lease_lost() {
                                    let Some(router) = &router else {
                                        let _ = window_commands.send(AppWindowCommand::Fatal);
                                        return Err(app_detail(
                                            "CLI lease loss",
                                            "primary router is not attached",
                                        ));
                                    };
                                    lease_tx.send(router.clone()).map_err(|_| {
                                        let _ = window_commands.send(AppWindowCommand::Fatal);
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
                    if let Err(source) = guardian.poll_fatal() {
                        let _ = window_commands.send(AppWindowCommand::Fatal);
                        return Err(app_guardian_fatal("guardian watcher", &source));
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
            .send(GuardianOwnerCommand::AttachRouter(
                Box::new(router),
                reply_tx,
            ))
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
fn apply_guardian_recovery_arm(
    phase: RecoveryArm,
    guardian: &mut GuardedPrimary,
    router: Option<&PrimaryRouterHandle>,
    initial_arm: &mut bool,
    recovery: Option<&mut FsRecoveryAttempt>,
) -> Result<(), HostAppError> {
    let Some(router) = router else {
        return Err(app_detail(
            "primary recovery arm",
            "primary router is not attached",
        ));
    };
    let _transition = router.shutdown.transition_guard();
    if !router.shutdown.is_running() {
        guardian.deny_recovery();
        return Ok(());
    }
    match phase {
        RecoveryArm::Initial(attempt) => {
            if attempt == 1 && !*initial_arm {
                guardian.arm_recovery();
                *initial_arm = true;
            }
        }
        RecoveryArm::Retired { retired, prepared } => {
            let Some(recovery) = recovery else {
                return Ok(());
            };
            if recovery.retired != retired
                || recovery.prepared != prepared
                || !recovery.ready
                || recovery.released
                || !*initial_arm
                || !router.recovery_armed.load(Ordering::Acquire)
                || router.last_revoked_attempt.load(Ordering::Acquire) != retired
                || guardian.group_pid().is_some()
            {
                return Ok(());
            }
            let current = router
                .current
                .lock()
                .map_err(|_| app_detail("primary recovery arm", "generation lock poisoned"))?;
            if current.is_some() {
                return Ok(());
            }
            if let Some(session) = router.fs.as_ref().and_then(Weak::upgrade) {
                let state = session.state.lock().map_err(|_| {
                    app_detail("primary recovery arm", "native metadata lock poisoned")
                })?;
                if state.failed_write_attempt.is_some() {
                    return Ok(());
                }
                if state.prepared_attempt != Some(prepared) || state.in_flight != 0 {
                    return Err(app_detail(
                        "primary recovery arm",
                        "native completion did not prepare the expected attempt",
                    ));
                }
            } else if router.fs.is_some() {
                return Err(app_detail(
                    "primary recovery arm",
                    "native metadata disappeared",
                ));
            }
            drop(current);
            guardian.arm_recovery();
            recovery.released = true;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
impl GuardianOwnerHandle {
    fn deny_recovery(&self) {
        let _ = self.command_tx.send(GuardianOwnerCommand::DenyRecovery);
    }

    fn request_recovery_arm(&self) -> Result<Receiver<Result<(), String>>, HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(GuardianOwnerCommand::ArmRecovery(
                RecoveryArm::Initial(1),
                reply_tx,
            ))
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
    fn request_fail_retired_generation(
        &self,
        attempt: u32,
    ) -> Result<Receiver<Result<Instant, String>>, HostAppError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.command_tx
            .send(GuardianOwnerCommand::FailRetiredGeneration(
                attempt, reply_tx,
            ))
            .map_err(|_| app_detail("filesystem reply recovery", "guardian owner stopped"))?;
        Ok(reply_rx)
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
    // Boxed like the macOS guardian's command: the handle dwarfs every other
    // variant (`clippy::large_enum_variant`), and attachment is cold.
    AttachRouter(
        Box<PrimaryRouterHandle>,
        mpsc::SyncSender<Result<(), String>>,
    ),
    ArmRecovery(RecoveryArm, mpsc::SyncSender<Result<(), String>>),
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
                                router = Some(*attached);
                                Ok(())
                            };
                            let _ = reply.send(result);
                        }
                        Ok(DirectPrimaryOwnerCommand::ArmRecovery(RecoveryArm::Initial(_attempt), reply)) => {
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
            .send(DirectPrimaryOwnerCommand::AttachRouter(
                Box::new(router),
                reply_tx,
            ))
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
            |reply| DirectPrimaryOwnerCommand::ArmRecovery(RecoveryArm::Initial(1), reply),
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
    /// None preserves legacy renderer composition. Validated facade boot and
    /// genuine Init remain GH-446's completion dependency.
    windows: Option<Arc<Mutex<window_router::WindowSession>>>,
    recovery_armed: Arc<AtomicBool>,
    last_revoked_attempt: Arc<AtomicU32>,
    shutdown: SessionShutdownState,
    fs: Option<Weak<FsDispatchSession>>,
    fs_worker_commands: Option<SyncSender<FsWorkerCommand>>,
    #[cfg(target_os = "macos")]
    auth: Option<Arc<AuthSession>>,
    #[cfg(target_os = "macos")]
    auth_commands: Option<SyncSender<AuthWork>>,
    /// One verified, immutable dialog policy and native effect custody owner.
    #[cfg(target_os = "macos")]
    dialog: Option<Arc<dialog_session::DialogSession>>,
    guardian: PlatformPrimaryOwnerHandle,
    window_commands: Sender<AppWindowCommand>,
    #[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
    quit_drain_hooks: Arc<Mutex<QuitDrainTestHooks>>,
}

/// Test hooks for the post-Quit drain: [`PrimaryRouterHandle::stall_next_quit_drain`]
/// and [`PrimaryRouterHandle::observe_next_quit_drain_end`].
#[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
#[derive(Default)]
struct QuitDrainTestHooks {
    stall: Option<(SyncSender<()>, Receiver<()>)>,
    end: Option<SyncSender<QuitDrainEnd>>,
    // Only the macOS CLI-publication fixture reads this slot.
    #[cfg(target_os = "macos")]
    cli_acceptance: Option<(SyncSender<()>, Receiver<()>)>,
}

/// How a post-Quit drain ended (GH-527 §4.9). Every end leads to the same
/// host-initiated close; tests observe which one it was.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum PrimaryLinkIdentity {
    Role(RolePrincipal),
    // Historical paired-stream component fixtures are not authenticated role
    // producers and cannot qualify KEL-97. Never a production admission path.
    #[cfg(test)]
    Fixture,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl PrimaryLinkIdentity {
    fn is_primary(self) -> bool {
        match self {
            Self::Role(principal) => principal.owner() == RoleOwner::Primary,
            #[cfg(test)]
            Self::Fixture => true,
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[derive(Clone)]
struct PrimaryCallerContext {
    attempt: u32,
    identity: PrimaryLinkIdentity,
    reader_stop: Arc<AtomicBool>,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct ActivePrimaryGeneration {
    attempt: u32,
    identity: PrimaryLinkIdentity,
    writer: BootstrapStream,
    reader_stop: Arc<AtomicBool>,
    /// The authenticated current primary role owns this sequence. Its value
    /// is protected by the same current-generation/transition lock as the
    /// caller fence, and is reset only by installing a new Runtime generation.
    /// The dialog effect owner must not copy or independently advance it.
    #[cfg(any(test, target_os = "macos"))]
    dialog_nonce_high_water: u64,
    /// Correlation id of this generation's accepted-but-unanswered `Quit`
    /// while it waits for its FS drain (GH-527 §4.9). Set and cleared under
    /// the generation lock, so retirement answers it at most once and never
    /// after its real REPLY.
    pending_quit: Option<CorrelationId>,
}

#[cfg(any(test, target_os = "macos"))]
impl ActivePrimaryGeneration {
    /// Admit each positive Message nonce at most once for this authenticated
    /// primary generation. The caller fence MUST run before invoking this.
    fn accept_dialog_nonce(&mut self, nonce: u64) -> bool {
        if nonce == 0 || nonce <= self.dialog_nonce_high_water {
            return false;
        }
        self.dialog_nonce_high_water = nonce;
        true
    }
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
            if !self.mark_window_registry_ready_guarded()? {
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

    #[cfg(target_os = "macos")]
    fn prepare_auth_presentation_drive(&self, ui_stopping: bool) -> keld_native::auth::AsMainStop {
        // Only the existing global host owner ends Auth admission. An anchor
        // close retires presentation authority, not the host's account session.
        let session_cut =
            self.shutdown.reader_stop.load(Ordering::Acquire) || !self.shutdown.is_running();
        if session_cut {
            if let Some(auth) = &self.auth {
                auth.stop_tail();
            }
            keld_native::auth::AsMainStop::HostTail
        } else if ui_stopping {
            keld_native::auth::AsMainStop::AnchorClosing
        } else {
            keld_native::auth::AsMainStop::Running
        }
    }

    /// Writes `LastWindowClosed` to the application and returns the instant the write
    /// returned. A window's own close ignores that instant; a profile handover counts
    /// its quit deadline D from the instant of its own write
    /// (`complete_profile_handover`), so an earlier close of the last window, after
    /// which the application kept running windowless, never starts D.
    fn signal_last_window_closed(&self) -> Result<Instant, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        self.last_window_closed.store(true, Ordering::Release);
        self.write_event_guarded(LifecycleEvent::LastWindowClosed)?;
        Ok(Instant::now())
    }

    /// Requests a profile handover (KEL-53 §4 "Profile handover", step 3a): the UI loop
    /// ends through the existing `Quit` command while the application keeps running;
    /// once the loop has returned, the main thread writes `LastWindowClosed`, waits up
    /// to [`HANDOVER_QUIT_DEADLINE`] for the application's `Quit` and otherwise ends the
    /// session under [`SESSION_HANDOVER`] (`complete_profile_handover`). This is the
    /// production entry the `PerUserDirect` coordinator calls; in S6c2a only the
    /// debug-only evidence seam calls it. The request is refused (`Ok(false)`) once the
    /// session is ending or a handover was already requested. A session under a CLI dev
    /// lease fails it typed (`KELD-CORE-037`), before anything reaches the loop: its
    /// lease tail would run beside the handover's teardown, which the main thread does
    /// not await (`await_tail_end`).
    #[cfg(any(target_os = "linux", windows))]
    #[cfg_attr(
        not(any(all(debug_assertions, windows), test)),
        expect(
            dead_code,
            reason = "the PerUserDirect coordinator (S6c2e) is the production caller; until then only the debug evidence seam calls it"
        )
    )]
    fn request_handover(&self) -> Result<bool, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        if self.shutdown.under_dev_lease.load(Ordering::Acquire) {
            return Err(HostAppError::new(
                "KELD-CORE-037",
                "profile handover",
                "a session under a CLI dev lease cannot hand over its profile",
                "Hand over only from an installed host launched without `keld dev`; a dev-stage session ends through its CLI lease.",
            ));
        }
        if !self.shutdown.is_running() || self.shutdown.handover_requested() {
            return Ok(false);
        }
        self.window_commands
            .send(AppWindowCommand::Quit)
            .map_err(|_| app_detail("profile handover", "UI event loop is unavailable"))?;
        self.shutdown.mark_handover_requested_guarded();
        Ok(true)
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

    fn privileged_calls(&self) -> &'static [&'static keld_ipc::channel_table::ChannelEntry] {
        #[cfg(target_os = "macos")]
        if self.auth.is_some() {
            return match (self.fs.is_some(), self.dialog.is_some()) {
                (true, true) => &[
                    &keld_ipc::channel_table::FS,
                    &keld_ipc::channel_table::AUTH,
                    &keld_ipc::channel_table::DIALOG,
                ],
                (true, false) => &[&keld_ipc::channel_table::FS, &keld_ipc::channel_table::AUTH],
                (false, true) => &[
                    &keld_ipc::channel_table::AUTH,
                    &keld_ipc::channel_table::DIALOG,
                ],
                (false, false) => &[&keld_ipc::channel_table::AUTH],
            };
        }
        #[cfg(target_os = "macos")]
        if self.dialog.is_some() {
            return if self.fs.is_some() {
                &[
                    &keld_ipc::channel_table::FS,
                    &keld_ipc::channel_table::DIALOG,
                ]
            } else {
                &[&keld_ipc::channel_table::DIALOG]
            };
        }
        if self.fs.is_some() {
            &[&keld_ipc::channel_table::FS]
        } else {
            &[]
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
        #[cfg(target_os = "macos")]
        if let Some(dialog) = &self.dialog {
            dialog.retire(active.identity, active.attempt);
        }
        let window_retirement = self.retire_window_owner(window_registry::WindowOwner::new(
            active.identity,
            active.attempt,
        ));
        // Every caller holds `transition`. A `Quit` of a retired generation can no
        // longer claim the session, so it no longer holds the handover's deadline.
        #[cfg(any(target_os = "linux", windows))]
        self.shutdown.clear_quit_admission_guarded(attempt);
        #[cfg(target_os = "macos")]
        let auth_retirement = self
            .auth
            .as_ref()
            .map_or(Ok(()), |auth| auth.retire(attempt));
        #[cfg(not(target_os = "macos"))]
        let auth_retirement = Ok(());
        collapse_app_results([
            window_retirement,
            auth_retirement,
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
        #[cfg(target_os = "macos")]
        let dialog_pending = self
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.pending_original_for(active.identity, attempt))
            .map(|corr| (keld_ipc::channel_table::DIALOG.id(), corr));
        #[cfg(target_os = "macos")]
        let auth_pending = match self
            .auth
            .as_ref()
            .map(|auth| auth.pending_for(attempt))
            .transpose()
        {
            Ok(pending) => pending.unwrap_or_default(),
            Err(error) => {
                eprintln!("{error}");
                if let Some(auth) = &self.auth {
                    auth.stop_tail();
                }
                let _ = self.window_commands.send(AppWindowCommand::Fatal);
                Vec::new()
            }
        };
        #[cfg(not(target_os = "macos"))]
        let auth_pending = std::iter::empty();
        write_best_effort_answers(
            &mut active.writer,
            pending.into_iter().flatten().chain(auth_pending).chain({
                #[cfg(target_os = "macos")]
                {
                    dialog_pending.into_iter()
                }
                #[cfg(not(target_os = "macos"))]
                {
                    None.into_iter()
                }
            }),
            error,
        );
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
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn answer_calls_after_quit(&self, attempt: u32, reader: &mut BootstrapStream) {
        let started = Instant::now();
        let (Some(idle_deadline), Some(hard_deadline)) = (
            started.checked_add(HOST_ANSWER_BUDGET),
            started.checked_add(APP_LINK_IO_DEADLINE),
        ) else {
            return;
        };
        #[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
        let idle_deadline = self
            .stall_quit_drain_for_test(started)
            .unwrap_or(idle_deadline);
        let end = self.drain_calls_after_quit(attempt, reader, idle_deadline, hard_deadline);
        #[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
        self.report_quit_drain_end(end);
        #[cfg(not(all(test, any(target_os = "macos", target_os = "linux", windows))))]
        let _ = end;
    }

    /// The drain loop of [`Self::answer_calls_after_quit`]. Every way it ends
    /// leads to the same host-initiated close, so none is a fault of the
    /// accepted shutdown: a role that closes right after its Quit REPLY ends
    /// it at that EOF, and an answer it can no longer receive (`EPIPE`, a
    /// reset, `NotConnected`) ends it quietly as a lost link.
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn drain_calls_after_quit(
        &self,
        attempt: u32,
        reader: &mut BootstrapStream,
        idle_deadline: Instant,
        hard_deadline: Instant,
    ) -> QuitDrainEnd {
        let error = CallError::quit_drained();
        let privileged_calls = self.privileged_calls();
        loop {
            let header = match read_quit_drain_frame_with_privileged_calls(
                reader,
                privileged_calls,
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
                || header.channel() == keld_ipc::channel_table::WINDOW.id()
                || privileged_calls
                    .iter()
                    .any(|entry| entry.id() == header.channel());
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
    #[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn observe_next_quit_drain_end(&self, end: SyncSender<QuitDrainEnd>) {
        self.quit_drain_hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .end = Some(end);
    }

    #[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
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
    #[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn stall_next_quit_drain(&self, stalled: SyncSender<()>, resume: Receiver<()>) {
        self.quit_drain_hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stall = Some((stalled, resume));
    }

    /// Returns the injected idle deadline (`started`, already passed) when
    /// the hook is armed.
    #[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
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

    #[cfg(all(test, target_os = "macos"))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn begin_fs_call(
        &self,
        attempt: u32,
        correlation: CorrelationId,
    ) -> Result<FsInFlight, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        self.begin_fs_call_guarded(attempt, correlation)
    }

    fn begin_fs_call_guarded(
        &self,
        attempt: u32,
        correlation: CorrelationId,
    ) -> Result<FsInFlight, HostAppError> {
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

    fn submit_fs_terminal_write(
        &self,
        admitted: FsInFlight,
        attempt: u32,
        write: impl FnOnce(&mut BootstrapStream) -> Result<(), HostAppError>,
    ) -> Result<Option<PendingFsWriteFailure>, HostAppError> {
        let transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("filesystem terminal reply", "generation lock poisoned"))?;
        admitted.retire_pending_call()?;
        if !self.shutdown.is_running() {
            return Ok(None);
        }
        let Some(active) = current.as_mut() else {
            return Ok(None);
        };
        if active.attempt != attempt {
            return Ok(None);
        }
        let Err(primary) = write(&mut active.writer) else {
            #[cfg(test)]
            {
                drop(current);
                drop(transition);
                admitted.session.hold_after_terminal_write();
            }
            return Ok(None);
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

        #[cfg(target_os = "macos")]
        {
            let deadline = Instant::now() + GUARDIAN_OWNER_REPLY_DEADLINE;
            let reply = match self.guardian.request_fail_retired_generation(attempt) {
                Ok(reply) => reply,
                Err(cleanup) => return Err(append_app_cleanup(primary, [Err(cleanup)])),
            };
            Ok(Some(PendingFsWriteFailure {
                attempt,
                session: fs,
                primary,
                reply,
                deadline,
            }))
        }
        #[cfg(any(target_os = "linux", windows))]
        {
            if let Err(cleanup) = self.guardian.fail_retired_generation(attempt) {
                return Err(append_app_cleanup(primary, [Err(cleanup)]));
            }
            if let Err(cleanup) = fs.clear_failed_write_attempt(attempt) {
                return Err(append_app_cleanup(primary, [Err(cleanup)]));
            }
            Ok(None)
        }
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

    #[cfg(target_os = "macos")]
    fn retire_auth_for_quit(&self, attempt: u32) -> Result<(), HostAppError> {
        self.auth.as_ref().map_or(Ok(()), |auth| {
            let retired = (|| {
                let mut current = self
                    .current
                    .lock()
                    .map_err(|_| app_detail("auth Quit", "generation lock poisoned"))?;
                self.answer_pending_calls_locked(
                    &mut current,
                    attempt,
                    &CallError::generation_retired(),
                );
                auth.retire(attempt)
            })();
            // Close physical admission even when the caller bookkeeping failed.
            // Native stop/poll remains the sole proof of physical drain.
            auth.stop_tail();
            retired
        })
    }

    fn lifecycle_quit(
        &self,
        attempt: u32,
        correlation: CorrelationId,
        reply: &[u8],
        reader: &mut BootstrapStream,
    ) -> Result<(), HostAppError> {
        let quit_frame = Instant::now();
        #[cfg(any(target_os = "linux", windows))]
        let admission;
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
            // Admitted while the session runs: a handover's deadline claim now waits
            // for this handler's claim or its release (`await_quit_or_claim_handover`).
            #[cfg(any(target_os = "linux", windows))]
            {
                admission = self.shutdown.admit_quit_guarded(attempt);
            }
        }

        // This reader is paused in Quit, so no later FS Call can enter. Keep
        // the session live until every already-admitted FS call has written its
        // terminal Reply/Err and released its lease; clients close immediately
        // after the correlated Quit Reply.
        if let Some(fs) = self.fs.as_ref().and_then(Weak::upgrade) {
            fs.drain()?;
        }

        let transition = self.shutdown.transition_guard();
        // This hold decides the admitted Quit's claim, and the handover's wait observes
        // neither the release nor the claim before the hold ends.
        #[cfg(any(target_os = "linux", windows))]
        admission.release_guarded();
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
        if !self.shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT) {
            drop(current_guard);
            drop(transition);
            return if self.shutdown.cause() == SESSION_CLI_LEASE_LOST {
                self.cli_lease_lost()
            } else {
                Ok(())
            };
        }
        // This Quit claims the session: its frame's entry instant ends the D sample at
        // a handover (from the `LastWindowClosed` write).
        #[cfg(any(target_os = "linux", windows))]
        let _ = self.shutdown.handover_clock.quit_frame.set(quit_frame);
        #[cfg(target_os = "macos")]
        let _ = quit_frame;
        drop(current_guard);
        drop(transition);
        // Ends the tail on every return below, so a main thread awaiting it after a
        // handover never waits past this claimant's failure.
        let _tail = self.shutdown.tail_run();
        self.fail_pending_echo_for_attempt(
            attempt,
            "application call retired by accepted lifecycle Quit",
        )?;
        self.guardian.prepare_accepted_shutdown()?;
        let (quit_reply, window_end) = {
            let _transition = self.shutdown.transition_guard();
            if !self
                .shutdown
                .accept_reserved_guarded(SESSION_LIFECYCLE_QUIT)
                || !self.shutdown.begin_tail()
            {
                return Ok(());
            }
            // Attempt the owed reply before native commands, then always end
            // managed windows. This hold prevents native receipts interleaving
            // between accepted Quit and Ending, including on reply failure.
            let quit_reply = self.write_quit_reply(correlation, reply);
            let window_end = self.end_window_session_guarded();
            (quit_reply, window_end)
        };
        // TailRun ends outside transition. Preserve the reply as the primary
        // error if teardown also failed; neither result skips the other attempt.
        if let Err(primary) = quit_reply {
            return Err(append_app_cleanup(primary, [window_end]));
        }
        window_end?;
        #[cfg(target_os = "macos")]
        let auth_retirement = self.retire_auth_for_quit(attempt);
        #[cfg(not(target_os = "macos"))]
        let auth_retirement = Ok(());
        if let Err(fs_error) = self.quiesce_and_drain_fs() {
            return collapse_app_results([auth_retirement, Err(fs_error)]);
        }
        #[cfg(any(target_os = "macos", target_os = "linux", windows))]
        self.answer_calls_after_quit(attempt, reader);
        self.finish_quit_link_and_tail(auth_retirement)
    }

    fn finish_quit_link_and_tail(
        &self,
        auth_retirement: Result<(), HostAppError>,
    ) -> Result<(), HostAppError> {
        let Ok(mut current_guard) = self.current.lock() else {
            return collapse_app_results([
                auth_retirement,
                Err(app_detail(
                    "primary session generation",
                    "generation lock poisoned",
                )),
            ]);
        };
        let Some(active) = current_guard.as_mut() else {
            return collapse_app_results([
                auth_retirement,
                Err(app_detail(
                    "lifecycle Quit link close",
                    "current primary generation disappeared after the reply",
                )),
            ]);
        };
        let window_retirement = self.retire_window_owner(window_registry::WindowOwner::new(
            active.identity,
            active.attempt,
        ));
        let link_close = collapse_app_results([
            window_retirement,
            finish_link_shutdown(
                active.writer.shutdown_app_link(),
                "lifecycle Quit link close",
            ),
        ]);
        current_guard.take();
        drop(current_guard);
        collapse_app_results([
            auth_retirement,
            link_close,
            self.finish_tail("lifecycle Quit"),
        ])
    }

    fn write_quit_reply(
        &self,
        correlation: CorrelationId,
        reply: &[u8],
    ) -> Result<(), HostAppError> {
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
        Ok(())
    }

    #[cfg(all(test, target_os = "macos"))]
    fn hold_cli_acceptance_for_test(&self) -> Result<(), HostAppError> {
        let hold = self
            .quit_drain_hooks
            .lock()
            .map_err(|_| app_detail("CLI publication fixture", "fixture slot poisoned"))?
            .cli_acceptance
            .take();
        if let Some((observed, resume)) = hold {
            observed.send(()).map_err(|error| {
                app_detail("CLI publication fixture witness", error.to_string())
            })?;
            resume
                .recv_timeout(Duration::from_secs(5))
                .map_err(|error| {
                    app_detail("CLI publication fixture release", error.to_string())
                })?;
        }
        Ok(())
    }

    /// Caller holds the shutdown transition and has won `begin_tail`: accept
    /// its reserved forced cause and end the managed window session atomically.
    fn accept_forced_tail_guarded(&self, reserved: u8) -> Result<(), HostAppError> {
        if self.shutdown.cause() == SESSION_RUNNING {
            self.shutdown.accept_reserved_guarded(reserved);
            #[cfg(all(test, target_os = "macos"))]
            self.hold_cli_acceptance_for_test()?;
        }
        self.end_window_session_guarded()
    }

    /// The forced tail: the host-initiated session end without the application's
    /// `Quit`, under CLI lease loss or, past the quit deadline of a profile handover,
    /// [`SESSION_HANDOVER`] (KEL-53 §4 "Profile handover", step 3c). One body for both
    /// causes; the gate admits either reserved cause and the tail runs once.
    fn cli_lease_lost(&self) -> Result<(), HostAppError> {
        let reserved = self.shutdown.reserved_cause.load(Ordering::Acquire);
        if !is_forced_cause(reserved) || self.shutdown.tail_started.load(Ordering::Acquire) {
            return Ok(());
        }
        let (retired_detail, link_phase, tail_phase) = if reserved == SESSION_HANDOVER {
            (
                "application call retired by the profile handover",
                "profile handover link close",
                "profile handover",
            )
        } else {
            (
                "application call retired by CLI lease loss",
                "CLI lease-loss link close",
                "CLI lease loss",
            )
        };
        self.guardian.prepare_accepted_shutdown()?;
        let window_ending = {
            let _transition = self.shutdown.transition_guard();
            if !self.shutdown.begin_tail() {
                return Ok(());
            }
            // Capture every fallible step after beginning this tail. Its
            // completion guard must be dropped only after releasing transition.
            self.accept_forced_tail_guarded(reserved)
        };
        let _tail = self.shutdown.tail_run();
        window_ending?;
        self.quiesce_and_drain_fs()?;
        let mut current_guard = self
            .current
            .lock()
            .map_err(|_| app_detail("primary session generation", "generation lock poisoned"))?;
        let pending = self.fail_any_pending_echo(retired_detail);
        #[cfg(target_os = "macos")]
        let auth_retirement = if let Some(active) = current_guard.as_ref() {
            let attempt = active.attempt;
            self.answer_pending_calls_locked(
                &mut current_guard,
                attempt,
                &CallError::generation_retired(),
            );
            self.auth.as_ref().map_or(Ok(()), |auth| {
                let retired = auth.retire(attempt);
                auth.stop_tail();
                retired
            })
        } else {
            if let Some(auth) = &self.auth {
                auth.stop_tail();
            }
            Ok(())
        };
        #[cfg(not(target_os = "macos"))]
        let auth_retirement = Ok(());
        let link = current_guard.as_mut().map_or(Ok(()), |active| {
            collapse_app_results([
                self.retire_window_owner(window_registry::WindowOwner::new(
                    active.identity,
                    active.attempt,
                )),
                finish_link_shutdown(active.writer.shutdown_app_link(), link_phase),
            ])
        });
        current_guard.take();
        drop(current_guard);
        collapse_app_results([pending, auth_retirement, link, self.finish_tail(tail_phase)])
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
                let identity = PrimaryLinkIdentity::Role(bound.principal());
                let attempt = bound.attempt();
                if attempt <= self.last_revoked_attempt.load(Ordering::Acquire) {
                    let stream = bound.into_stream();
                    let _ = stream.shutdown_app_link();
                    return Ok(());
                }
                #[cfg(target_os = "macos")]
                {
                    self.install_prepared_generation_with_identity(
                        attempt,
                        bound.into_stream(),
                        identity,
                    )
                }
                #[cfg(not(target_os = "macos"))]
                {
                    self.install_generation_with_identity(attempt, bound.into_stream(), identity)
                }
            }
        }
    }

    // Paired-stream regressions and historical non-macOS composition wait on
    // their calling thread. The macOS guardian actor never uses this adapter.
    #[cfg(any(test, not(target_os = "macos")))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn install_generation_with_identity(
        &self,
        attempt: u32,
        stream: BootstrapStream,
        identity: PrimaryLinkIdentity,
    ) -> Result<(), PrimaryGenerationFailure> {
        if let Some(session) = self.fs.as_ref().and_then(Weak::upgrade) {
            {
                let _transition = self.shutdown.transition_guard();
                if !self.shutdown.is_running() || self.shutdown.reader_stop.load(Ordering::Acquire)
                {
                    let _ = stream.shutdown_app_link();
                    return Ok(());
                }
                if self
                    .current
                    .lock()
                    .map_err(|_| {
                        app_detail("primary session generation", "generation lock poisoned")
                    })?
                    .is_some()
                {
                    return Err(app_detail(
                        "primary session generation",
                        "successor bound before the retired generation was revoked",
                    )
                    .into());
                }
            }
            session.drain()?;
            if !self.shutdown.is_running() {
                let _ = stream.shutdown_app_link();
                return Ok(());
            }
            #[cfg(target_os = "macos")]
            if session
                .state
                .lock()
                .map_err(|_| app_detail("filesystem recovery", "native metadata lock poisoned"))?
                .prepared_attempt
                != Some(attempt)
            {
                let retired = attempt.checked_sub(1).ok_or_else(|| {
                    app_detail("filesystem recovery", "invalid successor attempt")
                })?;
                let commands = self.fs_worker_commands.as_ref().ok_or_else(|| {
                    app_detail("filesystem recovery", "native worker is unavailable")
                })?;
                session.close_admission()?;
                let (reply, completion) = mpsc::sync_channel(1);
                commands
                    .send(FsWorkerCommand::Retire(retired, reply))
                    .map_err(|_| app_detail("filesystem recovery", "native worker stopped"))?;
                if !matches!(completion.recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE), Ok(FsWorkerCompletion::Retired(observed)) if observed == retired)
                {
                    return Err(app_detail(
                        "filesystem recovery",
                        "native retirement was not acknowledged",
                    )
                    .into());
                }
                if !self.shutdown.is_running() {
                    let _ = stream.shutdown_app_link();
                    return Ok(());
                }
                let (reply, completion) = mpsc::sync_channel(1);
                commands
                    .send(FsWorkerCommand::Prepare {
                        retired,
                        attempt,
                        reply,
                    })
                    .map_err(|_| app_detail("filesystem recovery", "native worker stopped"))?;
                match completion.recv_timeout(GUARDIAN_OWNER_REPLY_DEADLINE) {
                    Ok(FsWorkerCompletion::Prepared {
                        retired: old,
                        attempt: fresh,
                        result,
                    }) if old == retired && fresh == attempt => result?,
                    _ => {
                        return Err(app_detail(
                            "filesystem recovery",
                            "fresh native preparation was not acknowledged",
                        )
                        .into());
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                session
                    .state
                    .lock()
                    .map_err(|_| app_detail("filesystem generation", "in-flight lock poisoned"))?
                    .prepared_attempt = Some(attempt);
            }
        }
        self.install_prepared_generation_with_identity(attempt, stream, identity)
    }

    #[cfg(all(test, not(keld_core_native_renderer_main)))]
    fn install_generation(
        &self,
        attempt: u32,
        stream: BootstrapStream,
    ) -> Result<(), PrimaryGenerationFailure> {
        self.install_generation_with_identity(attempt, stream, PrimaryLinkIdentity::Fixture)
    }

    #[cfg(test)]
    fn install_prepared_generation(
        &self,
        attempt: u32,
        stream: BootstrapStream,
    ) -> Result<(), PrimaryGenerationFailure> {
        self.install_prepared_generation_with_identity(
            attempt,
            stream,
            PrimaryLinkIdentity::Fixture,
        )
    }

    fn install_prepared_generation_with_identity(
        &self,
        attempt: u32,
        mut stream: BootstrapStream,
        identity: PrimaryLinkIdentity,
    ) -> Result<(), PrimaryGenerationFailure> {
        if !identity.is_primary() {
            let primary = app_detail("primary role binding", "accepted role owner is not primary");
            let cleanup =
                finish_link_shutdown(stream.shutdown_app_link(), "wrong primary role link close");
            return Err(append_app_cleanup(primary, [cleanup]).into());
        }
        stream
            .set_app_link_read_deadline(Some(APP_LINK_READER_POLL))
            .map_err(|source| app_io("primary session reader deadline", &source))?;
        let writer_stream = stream
            .try_clone()
            .map_err(|source| app_io("primary session writer clone", &source))?;
        writer_stream
            .set_app_link_write_deadline(Some(APP_LINK_IO_DEADLINE))
            .map_err(|source| app_io("primary session writer deadline", &source))?;

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
            if let Some(session) = self.fs.as_ref().and_then(Weak::upgrade) {
                session.install_prepared_attempt(attempt)?;
            } else if self.fs.is_some() {
                return Err(app_detail(
                    "primary session generation",
                    "native preparation metadata disappeared",
                )
                .into());
            }
            reader_stop = self.shutdown.register_generation_reader();
            *current = Some(ActivePrimaryGeneration {
                attempt,
                identity,
                writer: writer_stream,
                reader_stop: Arc::clone(&reader_stop),
                #[cfg(any(test, target_os = "macos"))]
                dialog_nonce_high_water: 0,
                pending_quit: None,
            });
            drop(current);
            // Publication and retained replay are one transition with live
            // window delivery. Start the reader only after replay, so a call
            // cannot overtake Ready or observe the same Close via both paths.
            if self.window_ready.load(Ordering::Acquire) {
                self.write_event_guarded(LifecycleEvent::Ready)?;
            }
            if self.windows.is_none() && self.last_window_closed.load(Ordering::Acquire) {
                self.write_event_guarded(LifecycleEvent::LastWindowClosed)?;
            }
        }
        let handle = self.clone();
        let caller = PrimaryCallerContext {
            attempt,
            identity,
            reader_stop,
        };
        let reader = thread::Builder::new()
            .name(format!("keld-core-primary-router-{attempt}"))
            .spawn(move || {
                let mut result = read_primary_frames(&mut stream, &handle, &caller);
                if result.is_err()
                    && let Err(cleanup) = handle.fail_pending_echo_for_attempt(
                        attempt,
                        "application call ended with the primary reader",
                    )
                    && let Err(primary) = result
                {
                    result = Err(append_app_cleanup(primary, [Err(cleanup)]));
                }
                // A revoked reader during live recovery is intentionally inert.
                // Accepted terminal shutdown errors must survive current removal:
                // removing the link is one cleanup step, not proof all steps passed.
                if result.is_err() && !handle.is_current(attempt) && handle.shutdown.is_running() {
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
        // On every OS the primary owner (Guardian on macOS, `DirectPrimaryOwner` on
        // Linux and Windows) can fail this shutdown after an accepted lifecycle Quit.
        // That Quit already removed the primary, so its retired reader cannot send a
        // current-generation Fatal; wake the UI loop here. CLI lease loss and the
        // profile handover already have their own terminal paths.
        self.guardian.shutdown_and_wait().inspect_err(|_| {
            if self.shutdown.cause() == SESSION_LIFECYCLE_QUIT {
                let _ = self.window_commands.send(AppWindowCommand::Fatal);
            }
        })?;
        #[cfg(target_os = "macos")]
        if let Some(auth) = &self.auth {
            return auth.quit_after_drain(self);
        }
        // The UI step, read under the transition guard that also orders the handover's
        // own `Quit` send: once a handover has ended the UI loop, the loop needs nothing
        // from this tail (KEL-53 §4 "Profile handover", step 3b).
        let _transition = self.shutdown.transition_guard();
        if self.shutdown.handover_requested() {
            return Ok(());
        }
        self.window_commands
            .send(AppWindowCommand::Quit)
            .map_err(|_| app_detail(phase, "UI event loop is unavailable"))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct PrimaryRouter {
    handle: PrimaryRouterHandle,
    fs_worker: Option<FsWorker>,
    #[cfg(target_os = "macos")]
    auth_worker: Option<AuthWorker>,
}

/// The selected macOS boot's native window and authentication settings travel
/// together; neither is a separate authority source for the primary router.
#[cfg(target_os = "macos")]
struct MacBoundStart {
    auth_configuration: AuthConfiguration,
    presentation: Option<keld_native::auth::AsWorkerPort>,
    facade_windows: bool,
}

/// One optional runtime setup bundle keeps the primary admission boundary
/// independent of whether the session has a native registry or auth worker.
#[derive(Default)]
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
struct BoundSetup {
    window_native_support: Option<bool>,
    #[cfg(target_os = "macos")]
    auth_startup: Option<(
        VerifiedManifest,
        AuthConfiguration,
        Option<keld_native::auth::AsWorkerPort>,
    )>,
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
        broker: FsBroker,
        attempt: u32,
        #[cfg(test)] test_gate: Option<Arc<FsWorkerTestGate>>,
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
                    broker,
                    attempt,
                    #[cfg(test)]
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
        // This terminal owner has already drained native calls. A queued
        // retirement/preflight must finish before Stop, not lose the stop and
        // leave join waiting on a worker which can never receive it.
        let _ = self.commands.send(FsWorkerCommand::Stop);
        self.join.take().map_or(Ok(()), |join| {
            join.join()
                .map_err(|_| app_detail("filesystem worker", "thread panicked"))?
        })
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
impl PrimaryRouter {
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn start(
        stream: BootstrapStream,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
    ) -> Result<Self, HostAppError> {
        Self::start_with_fs(stream, window_commands, guardian, shutdown, None)
    }

    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    #[cfg(not(keld_core_native_renderer_main))]
    fn start_with_fs(
        stream: BootstrapStream,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
        fs: Option<&GuardSnapshot>,
    ) -> Result<Self, HostAppError> {
        Self::start_with_fs_test_gate(stream, window_commands, guardian, shutdown, fs, None)
    }

    #[cfg(test)]
    fn start_with_fs_test_gate(
        stream: BootstrapStream,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
        fs: Option<&GuardSnapshot>,
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
            windows: None,
            recovery_armed: Arc::new(AtomicBool::new(true)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown,
            fs: fs.map(GuardSnapshot::fs_weak),
            fs_worker_commands: None,
            #[cfg(target_os = "macos")]
            auth: None,
            #[cfg(target_os = "macos")]
            auth_commands: None,
            #[cfg(target_os = "macos")]
            dialog: fs.map(|guard| {
                Arc::new(dialog_session::DialogSession::new(
                    guard.verified.clone(),
                    window_commands.clone(),
                ))
            }),
            guardian,
            window_commands,
            #[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
            quit_drain_hooks: Arc::default(),
        };
        let router = Self::finish_start(
            handle,
            fs.map(GuardSnapshot::take_initial_broker).transpose()?,
            1,
            None,
            #[cfg(test)]
            fs_worker_test_gate,
            #[cfg(target_os = "macos")]
            None,
        )?;
        router.handle.install_prepared_generation(1, stream)?;
        Ok(router)
    }

    fn start_bound(
        bound: BoundPrimaryGeneration,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
        fs: Option<&GuardSnapshot>,
        #[cfg(target_os = "macos")] mac_start: MacBoundStart,
    ) -> Result<Self, HostAppError> {
        Self::start_bound_with_fs_setup(
            bound,
            window_commands,
            guardian,
            shutdown,
            fs,
            #[cfg(test)]
            None,
            #[cfg(target_os = "macos")]
            BoundSetup {
                window_native_support: mac_start.facade_windows.then_some(true),
                auth_startup: fs.map(|snapshot| {
                    (
                        snapshot.verified.clone(),
                        mac_start.auth_configuration,
                        mac_start.presentation,
                    )
                }),
            },
            // Linux/Windows have no owned macOS auth startup to move; borrow
            // the optional native-window policy instead of copying the bundle
            // by value (Clippy's needless_pass_by_value is deny-by-default).
            #[cfg(not(target_os = "macos"))]
            &BoundSetup {
                window_native_support: None,
            },
        )
    }

    #[cfg(feature = "fs-acceptance-observer")]
    keld_native::keld_native_fs_observer_items! {
    #[cfg(all(test, target_os = "macos", not(keld_core_native_renderer_main)))]
    fn start_bound_with_fs_test_gate(
        bound: BoundPrimaryGeneration,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
        fs: Option<&GuardSnapshot>,
        gate: Arc<FsWorkerTestGate>,
    ) -> Result<Self, HostAppError> {
        Self::start_bound_with_fs_setup(bound, window_commands, guardian, shutdown, fs, Some(gate), BoundSetup::default())
    }
    }

    fn start_bound_with_fs_setup(
        bound: BoundPrimaryGeneration,
        window_commands: Sender<AppWindowCommand>,
        guardian: PlatformPrimaryOwnerHandle,
        shutdown: SessionShutdownState,
        fs: Option<&GuardSnapshot>,
        #[cfg(test)] fs_worker_test_gate: Option<Arc<FsWorkerTestGate>>,
        // On macOS the verified auth startup is *consumed* exactly once by
        // AuthWorker; do not introduce a clone of its native presentation port.
        #[cfg(target_os = "macos")] options: BoundSetup,
        // On Linux/Windows BoundSetup contains only Copy policy values.
        #[cfg(not(target_os = "macos"))] options: &BoundSetup,
    ) -> Result<Self, HostAppError> {
        let identity = PrimaryLinkIdentity::Role(bound.principal());
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
            windows: None,
            recovery_armed: Arc::new(AtomicBool::new(false)),
            last_revoked_attempt: Arc::new(AtomicU32::new(0)),
            shutdown,
            fs: fs.map(GuardSnapshot::fs_weak),
            fs_worker_commands: None,
            #[cfg(target_os = "macos")]
            auth: None,
            #[cfg(target_os = "macos")]
            auth_commands: None,
            #[cfg(target_os = "macos")]
            dialog: fs.map(|guard| {
                Arc::new(dialog_session::DialogSession::new(
                    guard.verified.clone(),
                    window_commands.clone(),
                ))
            }),
            guardian,
            window_commands,
            #[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
            quit_drain_hooks: Arc::default(),
        };
        let router = Self::finish_start(
            handle,
            fs.map(GuardSnapshot::take_initial_broker).transpose()?,
            attempt,
            options.window_native_support,
            #[cfg(test)]
            fs_worker_test_gate,
            #[cfg(target_os = "macos")]
            options.auth_startup,
        )?;
        router
            .handle
            .install_prepared_generation_with_identity(attempt, stream, identity)?;
        Ok(router)
    }

    fn finish_start(
        mut handle: PrimaryRouterHandle,
        broker: Option<FsBroker>,
        attempt: u32,
        window_native_support: Option<bool>,
        #[cfg(test)] test_gate: Option<Arc<FsWorkerTestGate>>,
        #[cfg(target_os = "macos")] auth_startup: Option<(
            VerifiedManifest,
            AuthConfiguration,
            Option<keld_native::auth::AsWorkerPort>,
        )>,
    ) -> Result<Self, HostAppError> {
        handle.windows = window_native_support
            .map(|supported| Arc::new(Mutex::new(window_router::WindowSession::new(supported))));
        #[cfg(target_os = "macos")]
        let auth_worker = auth_startup
            .map(|(verified, configuration, presentation)| {
                AuthWorker::start(&mut handle, verified, configuration, presentation)
            })
            .transpose()?;
        let fs_worker = if let Some(broker) = broker {
            Some(FsWorker::start(
                &mut handle,
                broker,
                attempt,
                #[cfg(test)]
                test_gate,
            )?)
        } else {
            None
        };
        Ok(Self {
            handle,
            fs_worker,
            #[cfg(target_os = "macos")]
            auth_worker,
        })
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
            #[cfg(target_os = "macos")]
            if let Some(auth) = &self.handle.auth {
                auth.stop_tail();
            }
            self.handle.shutdown.stop_reader();
            let window_end = self.handle.end_window_session_guarded();
            let mut current = match self.handle.current.lock() {
                Ok(current) => current,
                Err(poisoned) => poisoned.into_inner(),
            };
            let mut pending = collapse_app_results([
                window_end,
                self.handle
                    .fail_any_pending_echo("application call retired by router shutdown"),
            ]);
            if let Some(active) = current.take() {
                pending = collapse_app_results([
                    pending,
                    self.handle
                        .retire_window_owner(window_registry::WindowOwner::new(
                            active.identity,
                            active.attempt,
                        )),
                ]);
                #[cfg(target_os = "macos")]
                if let Some(auth) = &self.handle.auth {
                    let _ = auth.retire(active.attempt);
                }
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
        #[cfg(target_os = "macos")]
        if let Some(worker) = &mut self.auth_worker {
            results.push(worker.stop_and_join());
        }
        collapse_app_results(results)
    }
}

#[cfg(all(test, target_os = "macos"))]
#[cfg(not(keld_core_native_renderer_main))]
fn prepare_fs_work(
    handle: &PrimaryRouterHandle,
    attempt: u32,
    correlation: CorrelationId,
    payload: &[u8],
    cancellation: &Arc<AtomicBool>,
) -> Result<FsWorkItem, HostAppError> {
    let _transition = handle.shutdown.transition_guard();
    prepare_fs_work_guarded(handle, attempt, correlation, payload, cancellation)
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn prepare_fs_work_guarded(
    handle: &PrimaryRouterHandle,
    attempt: u32,
    correlation: CorrelationId,
    payload: &[u8],
    cancellation: &Arc<AtomicBool>,
) -> Result<FsWorkItem, HostAppError> {
    let admitted = handle.begin_fs_call_guarded(attempt, correlation)?;
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
    initial_broker: FsBroker,
    initial_attempt: u32,
    #[cfg(test)] test_gate: Option<&FsWorkerTestGate>,
) -> Result<(), HostAppError> {
    let session = handle.fs.as_ref().and_then(Weak::upgrade).ok_or_else(|| {
        app_detail(
            "filesystem worker startup",
            "dispatch metadata is unavailable",
        )
    })?;
    let mut native = FsWorkerNativeState {
        broker: Some(initial_broker),
        attempt: initial_attempt,
    };
    let mut pending_failure: Option<PendingFsWriteFailure> = None;
    let mut stop_requested = false;
    #[cfg(target_os = "macos")]
    let mut retiring: Option<(u32, SyncSender<FsWorkerCompletion>)> = None;
    // Retain this exact worker's scope through its final call and loop exit.
    // A thread-affine erased value is never transferred back to the fixture.
    #[cfg(test)]
    let _worker_scope = test_gate
        .map(FsWorkerTestGate::initialize_worker)
        .transpose()?
        .flatten();
    loop {
        if let Some(pending) = pending_failure.take() {
            pending_failure = pending.poll()?;
        }
        if stop_requested && pending_failure.is_none() {
            drop(native.broker.take());
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        if retiring.is_some() {
            let state = session
                .state
                .lock()
                .map_err(|_| app_detail("filesystem retirement", "in-flight lock poisoned"))?;
            if state.in_flight == 0 {
                // The operation and its FsInFlight are gone. Destroy the actual
                // sole broker before emitting the root-free retirement receipt.
                drop(state);
                drop(native.broker.take());
                let mut state = session
                    .state
                    .lock()
                    .map_err(|_| app_detail("filesystem retirement", "in-flight lock poisoned"))?;
                state.prepared_attempt = None;
                drop(state);
                if let Some((attempt, reply)) = retiring.take() {
                    let _ = reply.send(FsWorkerCompletion::Retired(attempt));
                }
            }
        }
        #[cfg(target_os = "macos")]
        let native_pending = retiring.is_some();
        #[cfg(not(target_os = "macos"))]
        let native_pending = false;
        let incoming_command = if native_pending || pending_failure.is_some() {
            receiver.recv_timeout(Duration::from_millis(50))
        } else {
            receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        let command = match incoming_command {
            Ok(command) => command,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };
        let work = match command {
            FsWorkerCommand::Work(work) => work,
            FsWorkerCommand::Stop => {
                stop_requested = true;
                continue;
            }
            #[cfg(target_os = "macos")]
            generation_command => {
                native.handle_generation_command(&session, generation_command, &mut retiring)?;
                continue;
            }
        };
        let next_failure = run_fs_work_item(
            &native,
            &session,
            handle,
            work,
            #[cfg(test)]
            test_gate,
        )?;
        if let Some(next) = next_failure {
            if let Some(previous) = pending_failure.take() {
                pending_failure = previous.poll()?;
            }
            if pending_failure.is_some() {
                return Err(app_detail(
                    "filesystem reply recovery",
                    "another owner receipt is still pending",
                ));
            }
            pending_failure = Some(next);
        }
    }
}
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn run_fs_work_item(
    native: &FsWorkerNativeState,
    session: &FsDispatchSession,
    handle: &PrimaryRouterHandle,
    work: FsWorkItem,
    #[cfg(test)] test_gate: Option<&FsWorkerTestGate>,
) -> Result<Option<PendingFsWriteFailure>, HostAppError> {
    let FsWorkItem {
        admitted,
        request,
        attempt,
        correlation,
        cancellation,
    } = work;
    #[cfg(test)]
    if let Some(fence) = test_gate.and_then(|gate| gate.before_entry.as_deref()) {
        fence.wait_after_take()?;
    }
    let outcome = admitted.handle_with(|| {
        #[cfg(test)]
        if let Some(gate) = test_gate {
            gate.wait_after_take()?;
        }
        let broker = native
            .broker
            .as_ref()
            .filter(|_| cfg!(not(target_os = "macos")) || native.attempt == attempt)
            .ok_or_else(|| {
                app_detail(
                    "filesystem worker",
                    "request belongs to an unprepared generation",
                )
            })?;
        Ok::<_, HostAppError>(broker.handle_request(
            &session.verified,
            Principal::AppProcess,
            request,
            &cancellation,
        ))
    })?;
    let Some(outcome) = outcome else {
        return Ok(None);
    };
    match outcome? {
        Ok(response) => {
            let reply =
                encode(&response).map_err(|source| app_ipc("filesystem response", &source))?;
            handle.submit_fs_terminal_write(admitted, attempt, |writer| {
                write_frame(writer, FrameKind::Reply, 0, FS_CHANNEL, correlation, &reply)
                    .map_err(|source| app_ipc("primary session reply", &source))
            })
        }
        Err(error) => {
            let call_error = CallError::from(&error);
            handle.submit_fs_terminal_write(admitted, attempt, |writer| {
                keld_ipc::write_call_error(writer, FS_CHANNEL, correlation, &call_error)
                    .map_err(|source| app_ipc("primary session call error", &source))
            })
        }
    }
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
    caller: &PrimaryCallerContext,
) -> Result<(), HostAppError> {
    let attempt = caller.attempt;
    let reader_stop = &caller.reader_stop;
    loop {
        if handle.shutdown.cause() == SESSION_CLI_LEASE_LOST {
            handle.cli_lease_lost()?;
            return Ok(());
        }
        // KEL-133 owns every pre-payload semantic check. A guarded session
        // selects the existing privileged CALL policy for the `fs` channel
        // table entry; an unguarded session keeps the legacy
        // echo/lifecycle/PING policy.
        let frame = read_primary_app_frame_interruptible_with_admission(
            reader,
            reader_stop,
            || handle.pending_echo_corr_for(attempt),
            handle.privileged_calls(),
            |entry, corr| {
                #[cfg(not(target_os = "macos"))]
                let _ = corr; // Auth correlation admission is macOS-only.
                if entry.id() == FS_CHANNEL {
                    return if handle.has_outstanding_fs_call(attempt) {
                        GuardedCallAdmission::RejectSession
                    } else {
                        GuardedCallAdmission::Admit
                    };
                }
                #[cfg(target_os = "macos")]
                if entry.id() == keld_ipc::channel_table::AUTH.id() {
                    return handle
                        .auth
                        .as_ref()
                        .map_or(GuardedCallAdmission::RejectSession, |auth| {
                            auth.admission(attempt, corr)
                        });
                }
                #[cfg(target_os = "macos")]
                if entry.id() == keld_ipc::channel_table::DIALOG.id() {
                    // Unlike a new Message, Cancel must remain decodable when
                    // the original physically retained panel is at capacity.
                    // Core's guarded owner determines the typed Busy result
                    // after decoding the bounded, complete request.
                    return GuardedCallAdmission::Admit;
                }
                GuardedCallAdmission::RejectSession
            },
        );
        let frame = match frame {
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
        let keld_ipc::link::PrimaryAppFrame {
            header,
            payload,
            admission,
        } = frame;
        if admission == GuardedCallAdmission::AtCapacity {
            // The shared reader consumed the complete bounded frame under its
            // original clock. It admitted no call and the body is never decoded.
            with_primary_caller_writer(&handle.current, &handle.shutdown, caller, |writer| {
                keld_ipc::write_call_error(
                    writer,
                    header.channel(),
                    header.corr(),
                    &CallError::admission_capacity(),
                )
                .map_err(|source| app_ipc("guarded call capacity refusal", &source))
            })?;
            continue;
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
                dispatch_primary_echo(
                    &handle.current,
                    &handle.shutdown,
                    caller,
                    header.corr(),
                    &payload,
                )?;
            }
            (FrameKind::Call, FS_CHANNEL) if handle.shutdown.is_running() => {
                // Admission and enqueue share retirement's transition: an admitted
                // work item cannot be overtaken by its native retirement command.
                let _transition = handle.shutdown.transition_guard();
                let work =
                    prepare_fs_work_guarded(handle, attempt, header.corr(), &payload, reader_stop)?;
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
            #[cfg(target_os = "macos")]
            (FrameKind::Call, channel)
                if channel == keld_ipc::channel_table::AUTH.id()
                    && handle.shutdown.is_running() =>
            {
                handle.enqueue_auth(attempt, header.corr(), &payload, reader_stop)?;
            }
            #[cfg(target_os = "macos")]
            (FrameKind::Call, channel)
                if channel == keld_ipc::channel_table::DIALOG.id()
                    && handle.shutdown.is_running() =>
            {
                handle.dispatch_dialog(caller, header.corr(), &payload)?;
            }
            (FrameKind::Call, channel) if channel == keld_ipc::channel_table::WINDOW.id() => {
                handle.dispatch_window(caller, header.corr(), &payload)?;
            }
            (FrameKind::Call, LIFECYCLE_CHANNEL) => {
                let mut request = None;
                with_primary_caller_writer(&handle.current, &handle.shutdown, caller, |_| {
                    request = Some(
                        decode::<LifecycleRequest>(&payload)
                            .map_err(|source| app_ipc("lifecycle request", &source))?,
                    );
                    Ok(())
                })?;
                let Some(request) = request else {
                    continue;
                };
                match request {
                    LifecycleRequest::Subscribe { channel } => {
                        handle.subscribe_window(caller, header.corr(), channel)?;
                    }
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

// The write deadline each best-effort answer ran under, read from the
// writer itself on the writing thread once a test arms it. A Windows
// deadline is per handle, so no clone can read the writer's
// (`AppLinkDeadlines`); GH-528 T2 on Windows (gh527 §10).
#[cfg(all(test, windows))]
thread_local! {
    static ANSWER_WRITE_DEADLINES: std::cell::RefCell<Option<Vec<Option<Duration>>>> =
        const { std::cell::RefCell::new(None) };
}

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
    answers.into_iter().all(|(channel, corr)| {
        #[cfg(all(test, windows))]
        ANSWER_WRITE_DEADLINES.with_borrow_mut(|seen| {
            if let Some(seen) = seen {
                seen.push(writer.app_link_write_deadline().ok().flatten());
            }
        });
        keld_ipc::write_call_error(writer, channel, corr, error).is_ok()
    })
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn with_primary_caller_locked(
    current: &Mutex<Option<ActivePrimaryGeneration>>,
    shutdown: &SessionShutdownState,
    caller: &PrimaryCallerContext,
    dispatch: impl FnOnce(&mut Option<ActivePrimaryGeneration>) -> Result<(), HostAppError>,
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
    // Generation and owner are Runtime-minted. This existing reader-stop Arc
    // additionally scopes equal numeric generations to the actual Core link
    // instance; no Core counter or identity registry is minted or mirrored.
    if active.attempt != caller.attempt
        || active.identity != caller.identity
        || !Arc::ptr_eq(&active.reader_stop, &caller.reader_stop)
        || caller.reader_stop.load(Ordering::Acquire)
    {
        return Ok(());
    }
    dispatch(&mut current)
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn with_primary_caller_writer(
    current: &Mutex<Option<ActivePrimaryGeneration>>,
    shutdown: &SessionShutdownState,
    caller: &PrimaryCallerContext,
    write: impl FnOnce(&mut BootstrapStream) -> Result<(), HostAppError>,
) -> Result<(), HostAppError> {
    with_primary_caller_locked(current, shutdown, caller, |current| {
        let active = current.as_mut().ok_or_else(|| {
            app_detail("primary caller", "validated current generation disappeared")
        })?;
        write(&mut active.writer)
    })
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn dispatch_primary_echo(
    current: &Mutex<Option<ActivePrimaryGeneration>>,
    shutdown: &SessionShutdownState,
    caller: &PrimaryCallerContext,
    correlation: CorrelationId,
    payload: &[u8],
) -> Result<(), HostAppError> {
    with_primary_caller_writer(current, shutdown, caller, |writer| {
        let reply = keld_ipc::echo::handle_echo(payload)
            .map_err(|source| app_ipc("echo dispatch", &source))?;
        write_frame(
            writer,
            FrameKind::Reply,
            0,
            ECHO_CHANNEL,
            correlation,
            &reply,
        )
        .map_err(|source| app_ipc("primary session reply", &source))
    })
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn finish_link_shutdown(result: io::Result<()>, phase: &'static str) -> Result<(), HostAppError> {
    match result {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotConnected => Ok(()),
        Err(source) => Err(app_io(phase, &source)),
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
type GuardedFsTestSetup = Box<dyn FnOnce(&mut FsWorkerTestGate)>;

#[cfg(test)]
#[cfg(feature = "fs-acceptance-observer")]
keld_native::keld_native_fs_observer_items! {
    #[cfg(test)]
    #[path = "app_session/tests/native_observer_setup.rs"]
    mod native_observer_setup;
    #[cfg(test)]
    #[path = "app_session/tests/native_generation_fixture.rs"]
    mod native_generation_fixture;
}

#[cfg(test)]
#[cfg(all(keld_core_native_renderer_main, target_os = "macos", debug_assertions))]
#[path = "app_session/tests/native_observer_sensor.rs"]
mod native_observer_sensor;

#[cfg(test)]
#[cfg(all(keld_core_native_renderer_main, target_os = "macos", debug_assertions))]
#[path = "app_session/tests/native_renderer_main_entry.rs"]
mod native_renderer_main_entry;

#[cfg(test)]
#[cfg(all(keld_core_native_renderer_main, target_os = "macos", debug_assertions))]
pub(crate) fn native_renderer_main_thread_entry() {
    native_renderer_main_entry::run_main();
}

#[cfg(not(keld_core_native_renderer_main))]
#[cfg(test)]
mod tests;
