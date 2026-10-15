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
    APP_LINK_IO_DEADLINE, ActivePrimaryGeneration, AppLinkDeadlines, AppWindowCommand, Arc,
    AtomicBool, AtomicU32, AuthConfiguration, BOOT_FILE, BootstrapStream, BoundPrimaryGeneration,
    CallError, CorrelationId, DEV_LEASE_DRAIN_READS, Duration, ECHO_CHANNEL, FS_CHANNEL, FcntlArg,
    FdFlag, File, FrameKind, FsRequest, FsResponse, FsWorkItem, FsWorkerCommand, FsWorkerTestGate,
    GuardSnapshot, GuardedFsTestSetup, GuardianOwnerCommand, GuardianOwnerHandle,
    HOST_ANSWER_BUDGET, HashMap, HostAppError, Instant, IpcError, LIFECYCLE_CHANNEL,
    LifecycleEvent, LifecycleRequest, LifecycleResponse, MAX_BOOT_BYTES, MacValidatedAppIdentity,
    ManifestError, Mutex, OFlag, Ordering, PERMISSIONS_FILE, PathBuf, PendingFsCall,
    PendingFsWriteFailure, PlatformPrimaryOwnerHandle, PrimaryCallerContext, PrimaryEchoCall,
    PrimaryEchoReply, PrimaryLinkIdentity, PrimaryRouter, PrimaryRouterHandle, QuitDrainEnd,
    Receiver, RoleOwner, SESSION_CLI_LEASE_LOST, SESSION_LIFECYCLE_QUIT, SESSION_RUNNING, Sender,
    SessionShutdownState, SyncSender, ValidatedBootSelection, WebProfileSelection,
    admit_kipc_transport_sidecar, app_detail, collapse_app_results, configure_dev_lease_fd, decode,
    dispatch_primary_echo, encode, fcntl, finish_guarded_session, finish_link_shutdown, io,
    join_renderer_dispatch, load_verified_manifest, macos_identity_error, mpsc, parse_boot_bytes,
    poll_dev_lease_reader, prepare_fs_work,
    read_primary_app_frame_interruptible_with_privileged_call, run_fs_worker, run_guarded,
    select_macos_profile_mode_with, start_renderer_dispatch, startup_resource_snapshot, thread,
    validate_from_root, write_frame,
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
