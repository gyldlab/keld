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
    GuardSnapshot, HashMap, HostAppError, InitialWindow, Instant, LIFECYCLE_CHANNEL,
    LifecycleEvent, MAX_BOOT_BYTES, ManifestError, Mutex, Ordering, PERMISSIONS_FILE,
    PendingFsCall, PlatformPrimaryOwnerHandle, PrimaryLinkIdentity, PrimaryRouter,
    PrimaryRouterHandle, Receiver, SESSION_CLI_LEASE_LOST, SESSION_LIFECYCLE_QUIT, SESSION_RUNNING,
    SessionShutdownState, admit_kipc_transport_sidecar, app_detail, io, load_verified_manifest,
    mpsc, parse_boot_bytes, thread, validate_from_root, write_frame,
};

#[cfg(any(target_os = "macos", windows))]
use super::{ValidatedInitialWindow, WebProfileSelection, startup_resource_snapshot};

// The every-OS `finish_tail` per-cause test also runs on macOS.
#[cfg(target_os = "macos")]
use super::SESSION_HANDOVER;

#[cfg(target_os = "macos")]
#[test]
fn guardian_mode_argument_and_environment_share_the_validated_boot_decision() {
    use super::{AppReadyTrigger, NATIVE_WINDOW_FACADE_ENV, configure_guardian_boot_mode};
    for (trigger, expected) in [
        (AppReadyTrigger::HostInitialized, "1"),
        (AppReadyTrigger::NavigationReady, "0"),
    ] {
        for ambient in ["0", "1", "unknown"] {
            let mut command = std::process::Command::new("unused-guardian");
            command
                .arg("entry-inode")
                .env(NATIVE_WINDOW_FACADE_ENV, ambient);
            configure_guardian_boot_mode(&mut command, trigger);
            assert_eq!(
                command.get_args().collect::<Vec<_>>(),
                ["entry-inode", expected]
            );
            assert_eq!(
                command
                    .get_envs()
                    .find(|(name, _)| *name == NATIVE_WINDOW_FACADE_ENV)
                    .and_then(|(_, value)| value),
                Some(std::ffi::OsStr::new(expected))
            );
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn facade_and_renderer_ready_milestones_cannot_cross_or_repeat() {
    use super::{AppReadyTrigger, accept_boot_ready_trigger};

    for expected in [
        AppReadyTrigger::NavigationReady,
        AppReadyTrigger::HostInitialized,
    ] {
        let wrong = match expected {
            AppReadyTrigger::NavigationReady => AppReadyTrigger::HostInitialized,
            AppReadyTrigger::HostInitialized => AppReadyTrigger::NavigationReady,
        };
        let mut accepted = false;
        let foreign = accept_boot_ready_trigger(expected, wrong, &mut accepted)
            .expect_err("foreign Ready event must fail before publication");
        assert_eq!(foreign.code(), "KELD-CORE-044");
        assert!(!accepted);
        accept_boot_ready_trigger(expected, expected, &mut accepted)
            .expect("one correct Ready event");
        let repeated = accept_boot_ready_trigger(expected, expected, &mut accepted)
            .expect_err("duplicate Ready is fatal, not silently replayed");
        assert_eq!(repeated.code(), "KELD-CORE-044");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn facade_init_deadline_arms_only_on_loop_entry_and_ends_at_first_init() {
    use super::{AppReadyTrigger, initial_milestone_poll};
    use std::sync::OnceLock;

    let entered = OnceLock::new();
    assert!(
        entered.get().is_none(),
        "engine install has no active Init deadline"
    );
    let now = Instant::now();
    entered
        .set(now + keld_wv::INITIAL_MILESTONE_DEADLINE)
        .expect("first native loop entry");
    let deadline = *entered.get().expect("deadline armed once");
    assert!(deadline > now);
    assert_eq!(
        initial_milestone_poll(None, now),
        super::APP_LINK_READER_POLL
    );
    assert_eq!(
        initial_milestone_poll(
            Some(deadline),
            deadline
                .checked_sub(Duration::from_millis(20))
                .expect("valid before deadline"),
        ),
        Duration::from_millis(20)
    );
    assert_eq!(
        initial_milestone_poll(Some(deadline), deadline),
        Duration::ZERO
    );
    assert_eq!(
        initial_milestone_poll(Some(deadline), deadline + Duration::from_secs(1)),
        Duration::ZERO
    );
    let mut signaled = false;
    super::accept_boot_ready_trigger(
        AppReadyTrigger::HostInitialized,
        AppReadyTrigger::HostInitialized,
        &mut signaled,
    )
    .expect("first Init is accepted");
    assert!(signaled, "post-Init sessions disarm the milestone bound");
    assert_eq!(
        initial_milestone_poll(None, deadline + Duration::from_secs(30)),
        super::APP_LINK_READER_POLL,
        "a healthy Init must disarm the deadline even when still windowless"
    );
    assert!(
        now + keld_wv::INITIAL_MILESTONE_DEADLINE >= deadline,
        "a healthy windowless session must not time out after Init"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn queued_late_first_init_cannot_bypass_absolute_deadline() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    use std::sync::OnceLock;

    // No sleeps or scheduler assumptions: even a queued event wins a zero
    // timeout, so the receiver must decide using the genuine native timestamp.
    let deadline = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("monotonic clock supports one-second predecessor");
    let clock = OnceLock::new();
    clock.set(deadline).expect("arm first-Init absolute bound");
    let (send, events) = mpsc::channel();
    send.send(AppWindowEvent::HostInitialized {
        observed_at: deadline + Duration::from_millis(1),
    })
    .expect("queue one late UI first turn");
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock)),
        InitialMilestoneReceive::Expired,
        "queued late Init must not publish Ready or disarm the terminal"
    );
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock)),
        InitialMilestoneReceive::Expired,
        "a non-terminal successful receive must not consume an invalid Init"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn on_time_native_first_init_survives_delayed_coordinator_and_disarms_deadline() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    use std::sync::OnceLock;

    let deadline = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("monotonic clock supports one-second predecessor");
    let clock = OnceLock::new();
    clock.set(deadline).expect("arm first-Init absolute bound");
    let on_time = deadline
        .checked_sub(Duration::from_millis(1))
        .expect("monotonic clock supports preceding native event");
    let first = AppWindowEvent::HostInitialized {
        observed_at: on_time,
    };
    let (send, events) = mpsc::channel();
    send.send(first).expect("queue genuine timely UI event");
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock)),
        InitialMilestoneReceive::Event(first),
        "coordinator descheduling must not transform timely native Init into timeout"
    );
    send.send(AppWindowEvent::LastWindowClosed)
        .expect("healthy windowless session may continue");
    assert_eq!(
        receive_initial_milestone(&events, None),
        InitialMilestoneReceive::Event(AppWindowEvent::LastWindowClosed),
        "once Init is accepted the expired clock is no longer armed"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn busy_non_init_queue_and_empty_deadline_both_expire_without_ready() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    use std::sync::OnceLock;

    let deadline = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("monotonic clock supports one-second predecessor");
    let clock = OnceLock::new();
    clock.set(deadline).expect("arm first-Init absolute bound");
    let (send, events) = mpsc::channel();
    send.send(AppWindowEvent::LastWindowClosed)
        .expect("queue non-Init event after absolute expiry");
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock)),
        InitialMilestoneReceive::Expired,
        "busy queue cannot perpetually avoid a timeout"
    );
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock)),
        InitialMilestoneReceive::Expired,
        "empty deadline expiry still takes bounded terminal path"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn unarmed_first_init_timestamp_cannot_mint_ready() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    use std::sync::OnceLock;

    // The native producer must arm the shared loop-entry clock before sending
    // a genuine first Init. An unarmed / untrusted milestone cannot be accepted.
    let clock = OnceLock::new();
    let (send, events) = mpsc::channel();
    send.send(AppWindowEvent::HostInitialized {
        observed_at: Instant::now(),
    })
    .expect("queue a milestone with no loop-entry clock");
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock)),
        InitialMilestoneReceive::Expired,
        "unarmed native clock must not authorize Ready"
    );
}

#[cfg(any(target_os = "linux", windows))]
use super::{
    DirectSessionNext, FsDispatchSession, FsInFlight, HandoverClock, HandoverWait,
    PrimaryGenerationFailure, SESSION_HANDOVER, acknowledge_direct_recovery_arm, app_ipc,
    complete_profile_handover, is_forced_cause, next_direct_session,
    restart_failed_bound_generation,
};

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use super::{
    APP_LINK_IO_DEADLINE, CallError, ECHO_CHANNEL, FsRequest, FsWorkerTestGate, GuardedFsTestSetup,
    LifecycleRequest, LifecycleResponse, PathBuf, Sender, SyncSender, decode, encode,
};

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use super::HOST_ANSWER_BUDGET;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use super::QuitDrainEnd;

#[cfg(target_os = "macos")]
use super::{
    AuthConfiguration, FsWorkItem, FsWorkerCommand, IpcError, PendingFsWriteFailure,
    PrimaryCallerContext, ValidatedBootSelection, collapse_app_results, dispatch_primary_echo,
    finish_link_shutdown, run_fs_worker, run_guarded,
};

#[cfg(target_os = "macos")]
use super::{DEV_LEASE_DRAIN_READS, FcntlArg, FdFlag, OFlag, fcntl};

#[cfg(target_os = "macos")]
use super::{
    FsResponse, GuardianOwnerCommand, GuardianOwnerHandle, MacValidatedAppIdentity,
    PrimaryEchoCall, PrimaryEchoReply, configure_dev_lease_fd, finish_guarded_session,
    join_renderer_dispatch, macos_identity_error, poll_dev_lease_reader, prepare_fs_work,
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
    ANSWER_WRITE_DEADLINES, GetHandleInformation, HANDLE_FLAG_INHERIT, SetHandleInformation,
    ValidatedAppIdentity, WINDOWS_SHARING_VIOLATION, WebView2Engine, WindowsBootMode,
    WindowsDevLeaseMonitor, WindowsProfileMode, await_windows_quit_peer_close_until,
    clear_windows_handle_inheritance, current_windows_executable, decode_digest,
    installed_package_error, publish_windows_lease_result, receive_direct_owner_reply,
    remove_windows_dev_stage_with, run_direct_startup_if_session_running,
    select_windows_boot_route_with, select_windows_profile_mode, start_direct_dev_lease_tail,
    target_error, validate_installed_current_exe, validate_installed_from_verified,
    validate_windows_boot_files, verified_windows_image, windows_identity_error,
    windows_lease_pipe_is_live,
};

include!("tests/app_identity_and_shared_probes.rs");
include!("tests/session_handover.rs");
include!("tests/windows_profile_and_boot.rs");
include!("tests/windows_signature_and_lease.rs");
include!("tests/boot_schema_and_preflight.rs");
include!("tests/filesystem_guard_and_recovery.rs");
include!("tests/guarded_router_and_quit.rs");
// The existing macOS declaration stays with its historical selector. Linux
// reuses the same real Runtime/HELLO identity proof without any GUI claim.
#[cfg(target_os = "linux")]
#[path = "tests/accepted_primary_identity.rs"]
mod accepted_primary_identity_linux;
include!("tests/role_shutdown_and_worker.rs");
include!("tests/successor_single_flight.rs");
include!("tests/generation_correlation_and_auth.rs");
include!("tests/filesystem_quiesce.rs");
include!("tests/router_lifecycle.rs");
include!("tests/recovered_events_and_echo.rs");
include!("tests/ready_recovery.rs");
include!("tests/failed_ready_and_cli_lease.rs");
include!("tests/terminal_cleanup_and_windows_shutdown.rs");
