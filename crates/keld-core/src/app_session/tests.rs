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

use super::{
    ActivePrimaryGeneration, AppLinkDeadlines, AppWindowCommand, Arc, AtomicBool, AtomicU32,
    BOOT_FILE, BootstrapStream, CorrelationId, Duration, FS_CHANNEL, File, FrameKind,
    GuardSnapshot, HashMap, HostAppError, Instant, LIFECYCLE_CHANNEL, LifecycleEvent,
    MAX_BOOT_BYTES, ManifestError, Mutex, Ordering, PERMISSIONS_FILE, PendingFsCall,
    PlatformPrimaryOwnerHandle, PrimaryLinkIdentity, PrimaryRouter, PrimaryRouterHandle, Receiver,
    SESSION_CLI_LEASE_LOST, SESSION_LIFECYCLE_QUIT, SESSION_RUNNING, SessionShutdownState,
    admit_kipc_transport_sidecar, app_detail, io, load_verified_manifest, mpsc, parse_boot_bytes,
    thread, validate_from_root, write_frame,
};

#[cfg(any(target_os = "macos", windows))]
use super::{APP_LINK_IO_DEADLINE, PathBuf, WebProfileSelection, startup_resource_snapshot};

#[cfg(any(target_os = "linux", windows))]
use super::{
    DirectSessionNext, FsDispatchSession, FsInFlight, HandoverClock, HandoverWait,
    PrimaryGenerationFailure, SESSION_HANDOVER, acknowledge_direct_recovery_arm, app_ipc,
    complete_profile_handover, is_forced_cause, next_direct_session,
    restart_failed_bound_generation,
};

#[cfg(any(target_os = "macos", target_os = "linux"))]
use super::{ECHO_CHANNEL, LifecycleRequest, LifecycleResponse, decode, encode};

#[cfg(target_os = "macos")]
use super::{
    AuthConfiguration, BoundPrimaryGeneration, CallError, FsRequest, FsWorkItem, FsWorkerCommand,
    HOST_ANSWER_BUDGET, IpcError, PendingFsWriteFailure, PrimaryCallerContext, RoleOwner, Sender,
    SyncSender, ValidatedBootSelection, collapse_app_results, dispatch_primary_echo,
    finish_link_shutdown, run_fs_worker, run_guarded,
};

#[cfg(target_os = "macos")]
use super::{DEV_LEASE_DRAIN_READS, FcntlArg, FdFlag, OFlag, fcntl};

#[cfg(target_os = "macos")]
use super::{
    FsResponse, FsWorkerTestGate, GuardedFsTestSetup, GuardianOwnerCommand, GuardianOwnerHandle,
    MacValidatedAppIdentity, PrimaryEchoCall, PrimaryEchoReply, QuitDrainEnd,
    configure_dev_lease_fd, finish_guarded_session, join_renderer_dispatch, macos_identity_error,
    poll_dev_lease_reader, prepare_fs_work,
    read_primary_app_frame_interruptible_with_privileged_call, select_macos_profile_mode_with,
    start_renderer_dispatch,
};

// Match the native owner's observer gate: ordinary builds omit these
// imports, while admitted debug/macOS fixtures keep their existing paths.
#[cfg(feature = "fs-acceptance-observer")]
keld_native::keld_native_fs_observer_items! {
    use super::{
        FsDispatchSession, FsRecoveryAttempt, GUARDIAN_OWNER_REPLY_DEADLINE, GuardianOwner,
        RecoveryArm, apply_guardian_recovery_arm, read_primary_frames,
    };
}

// Defined under `cfg(all(debug_assertions, windows))`, so the import follows it;
// a release-profile Windows test build has no such item to import.
#[cfg(all(debug_assertions, windows))]
use super::TestSessionSeam;

#[cfg(windows)]
use super::{
    GetHandleInformation, HANDLE_FLAG_INHERIT, SetHandleInformation, ValidatedAppIdentity,
    WINDOWS_SHARING_VIOLATION, WebView2Engine, WindowsBootMode, WindowsDevLeaseMonitor,
    WindowsProfileMode, await_windows_quit_peer_close_until, clear_windows_handle_inheritance,
    current_windows_executable, decode_digest, installed_package_error,
    publish_windows_lease_result, receive_direct_owner_reply, remove_windows_dev_stage_with,
    run_direct_startup_if_session_running, select_windows_boot_route_with,
    select_windows_profile_mode, start_direct_dev_lease_tail, target_error,
    validate_installed_current_exe, validate_installed_from_verified, validate_windows_boot_files,
    verified_windows_image, windows_identity_error, windows_lease_pipe_is_live,
};

include!("tests/app_identity_and_shared_probes.rs");
include!("tests/session_handover.rs");
include!("tests/windows_profile_and_boot.rs");
include!("tests/windows_signature_and_lease.rs");
include!("tests/boot_schema_and_preflight.rs");
include!("tests/filesystem_guard_and_recovery.rs");
include!("tests/guarded_router_and_quit.rs");
include!("tests/role_shutdown_and_worker.rs");
include!("tests/successor_single_flight.rs");
include!("tests/generation_correlation_and_auth.rs");
include!("tests/filesystem_quiesce.rs");
include!("tests/router_lifecycle.rs");
include!("tests/recovered_events_and_echo.rs");
include!("tests/ready_recovery.rs");
include!("tests/failed_ready_and_cli_lease.rs");
include!("tests/terminal_cleanup_and_windows_shutdown.rs");
