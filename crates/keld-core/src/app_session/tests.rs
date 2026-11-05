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
            configure_guardian_boot_mode(&mut command, trigger, false)
                .expect("validated guardian boot mode");
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
        receive_initial_milestone(&events, Some(&clock), true, true),
        InitialMilestoneReceive::Expired,
        "queued late Init must not publish Ready or disarm the terminal"
    );
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock), true, true),
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
    let (send, events) = mpsc::channel();
    send.send(AppWindowEvent::HostInitialized {
        observed_at: on_time,
    })
    .expect("queue genuine timely UI event");
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock), true, true),
        InitialMilestoneReceive::Event(AppWindowEvent::HostInitialized {
            observed_at: on_time,
        }),
        "coordinator descheduling must not transform timely native Init into timeout"
    );
    send.send(AppWindowEvent::LastWindowClosed)
        .expect("healthy windowless session may continue");
    assert_eq!(
        receive_initial_milestone(&events, None, true, true),
        InitialMilestoneReceive::Event(AppWindowEvent::LastWindowClosed),
        "once Init is accepted the expired clock is no longer armed"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn desktop_sample_before_timestamp_preserves_on_time_init_under_delayed_receive() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    use std::sync::OnceLock;

    // An on-time native Init may be queued until after the worker-side
    // watchdog deadline. Its desktop sample necessarily precedes the
    // timestamped HostInitialized on the same FIFO: sample must not eclipse
    // that native timestamp. Do not weaken the non-Init/late-Init negatives.
    let deadline = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("monotonic predecessor");
    let on_time = deadline
        .checked_sub(Duration::from_millis(1))
        .expect("native before deadline");
    let clock = OnceLock::new();
    clock.set(deadline).expect("arm deadline");
    let (sender, events) = mpsc::channel();
    sender
        .send(AppWindowEvent::DesktopObserved { facts: None })
        .expect("queue first native desktop sample");
    sender
        .send(AppWindowEvent::HostInitialized {
            observed_at: on_time,
        })
        .expect("queue genuine on-time Init after desktop");
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock), true, true),
        InitialMilestoneReceive::Event(AppWindowEvent::DesktopObserved { facts: None }),
        "desktop event is allowed through for subsequent native timestamp review"
    );
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock), false, true),
        InitialMilestoneReceive::Event(AppWindowEvent::HostInitialized {
            observed_at: on_time
        }),
        "the native on-time marker survives coordinator scheduling"
    );

    let (sender, events) = mpsc::channel();
    sender
        .send(AppWindowEvent::DesktopObserved { facts: None })
        .expect("queue desktop sample for late Init");
    sender
        .send(AppWindowEvent::HostInitialized {
            observed_at: deadline + Duration::from_millis(1),
        })
        .expect("queue late Init");
    assert!(matches!(
        receive_initial_milestone(&events, Some(&clock), true, true),
        InitialMilestoneReceive::Event(AppWindowEvent::DesktopObserved { .. })
    ));
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock), false, true),
        InitialMilestoneReceive::Expired,
        "desktop bypass cannot make a late native Init valid"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn repeated_desktop_samples_cannot_starve_or_replace_the_one_first_init_watchdog() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    use std::sync::OnceLock;

    let clock = OnceLock::new();
    clock
        .set(Instant::now() + Duration::from_secs(15))
        .expect("arm actual first Init's unchanged 15-second bound");
    let (sender, events) = mpsc::channel();
    for _ in 0..100 {
        sender
            .send(AppWindowEvent::DesktopObserved { facts: None })
            .expect("queue deliberately repeated native samples");
    }
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock), true, true),
        InitialMilestoneReceive::Event(AppWindowEvent::DesktopObserved { facts: None }),
        "the sole native sample is allowed to precede the timestamped Init"
    );
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock), false, true),
        InitialMilestoneReceive::UnexpectedDesktop,
        "the second sample must fail immediately even with time remaining"
    );

    // A solitary desktop sample is also not permission to wait indefinitely
    // when the producer drops/omits HostInitialized. This tests the expired
    // boundary directly, with no sleep and no invented native Init timestamp.
    let expired_clock = OnceLock::new();
    expired_clock
        .set(
            Instant::now()
                .checked_sub(Duration::from_secs(1))
                .expect("monotonic predecessor"),
        )
        .expect("arm a deadline that has actually elapsed");
    let (sender, events) = mpsc::channel();
    sender
        .send(AppWindowEvent::DesktopObserved { facts: None })
        .expect("queue one pre-Init desktop observation");
    assert_eq!(
        receive_initial_milestone(&events, Some(&expired_clock), true, true),
        InitialMilestoneReceive::Event(AppWindowEvent::DesktopObserved { facts: None }),
    );
    assert_eq!(
        receive_initial_milestone(&events, Some(&expired_clock), false, true),
        InitialMilestoneReceive::Expired,
        "a missing HostInitialized may not retain the desktop exception"
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
        receive_initial_milestone(&events, Some(&clock), true, true),
        InitialMilestoneReceive::Expired,
        "busy queue cannot perpetually avoid a timeout"
    );
    assert_eq!(
        receive_initial_milestone(&events, Some(&clock), true, true),
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
        receive_initial_milestone(&events, Some(&clock), true, true),
        InitialMilestoneReceive::Expired,
        "unarmed native clock must not authorize Ready"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn native_event_receive_facade_waits_for_event_then_disconnect_without_idle_returns() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    let (send, events) = mpsc::channel();
    let (armed_send, armed) = mpsc::sync_channel(0);
    let (result_send, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        armed_send.send(()).expect("announce receiver");
        for _ in 0..2 {
            let observed = receive_initial_milestone(&events, None, false, false);
            if result_send.send(observed).is_err() {
                return;
            }
        }
    });
    armed.recv().expect("receiver started");
    let early = result.recv_timeout(Duration::from_millis(500));
    let stayed_waiting = matches!(&early, Err(mpsc::RecvTimeoutError::Timeout));
    let delivered = send.send(AppWindowEvent::LastWindowClosed);
    drop(send);
    let first = early.or_else(|_| result.recv_timeout(Duration::from_secs(2)));
    let second = result.recv_timeout(Duration::from_secs(2));
    let joined = worker.join();
    // Assertions follow shutdown/join, so the negative control leaves no worker.
    assert!(
        stayed_waiting,
        "an established facade must not return Again while no native event exists"
    );
    assert!(delivered.is_ok(), "real event must be accepted after idle");
    assert_eq!(
        first.expect("first event"),
        InitialMilestoneReceive::Event(AppWindowEvent::LastWindowClosed)
    );
    assert_eq!(
        second.expect("channel close"),
        InitialMilestoneReceive::Disconnected
    );
    assert!(
        joined.is_ok(),
        "receiver joins after its real sender closes"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn native_event_receive_legacy_retains_stop_poll() {
    use super::{InitialMilestoneReceive, receive_initial_milestone};
    let (send, events) = mpsc::channel();
    let (result_send, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let value = receive_initial_milestone(&events, None, false, true);
        let _ = result_send.send(value);
    });
    let observed = result.recv_timeout(Duration::from_secs(2));
    drop(send);
    let joined = worker.join();
    assert_eq!(
        observed.expect("bounded legacy polling"),
        InitialMilestoneReceive::Again
    );
    assert!(joined.is_ok());
}

#[cfg(target_os = "macos")]
#[test]
fn native_event_receive_facade_keeps_armed_empty_init_deadline() {
    use super::{InitialMilestoneReceive, receive_initial_milestone};
    use std::sync::OnceLock;
    let (send, events) = mpsc::channel();
    let (result_send, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let clock = OnceLock::new();
        clock
            .set(
                Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .expect("monotonic clock has one-second history"),
            )
            .expect("expired absolute clock");
        let value = receive_initial_milestone(&events, Some(&clock), true, false);
        let _ = result_send.send(value);
    });
    let observed = result.recv_timeout(Duration::from_secs(2));
    drop(send);
    let joined = worker.join();
    assert_eq!(
        observed.expect("armed Init still has deadline"),
        InitialMilestoneReceive::Expired
    );
    assert!(joined.is_ok());
}

#[cfg(target_os = "macos")]
#[test]
fn native_event_receive_facade_keeps_unarmed_init_poll_and_refusal() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    use std::sync::OnceLock;
    let (send, events) = mpsc::channel();
    let (result_send, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let clock = OnceLock::new();
        for _ in 0..2 {
            let value = receive_initial_milestone(&events, Some(&clock), true, false);
            if result_send.send(value).is_err() {
                return;
            }
        }
    });
    let first = result.recv_timeout(Duration::from_secs(2));
    let queued = send.send(AppWindowEvent::HostInitialized {
        observed_at: Instant::now(),
    });
    drop(send);
    let second = result.recv_timeout(Duration::from_secs(2));
    let joined = worker.join();
    assert_eq!(
        first.expect("unarmed clock still polls"),
        InitialMilestoneReceive::Again
    );
    assert!(queued.is_ok());
    assert_eq!(
        second.expect("unarmed Init refused"),
        InitialMilestoneReceive::Expired
    );
    assert!(joined.is_ok());
}

#[cfg(target_os = "macos")]
#[test]
fn native_event_receive_facade_preserves_fifo_before_sender_close() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    let (send, events) = mpsc::channel();
    send.send(AppWindowEvent::NavigationReady)
        .expect("first queued event");
    send.send(AppWindowEvent::LastWindowClosed)
        .expect("second queued event");
    drop(send);
    assert_eq!(
        receive_initial_milestone(&events, None, false, false),
        InitialMilestoneReceive::Event(AppWindowEvent::NavigationReady)
    );
    assert_eq!(
        receive_initial_milestone(&events, None, false, false),
        InitialMilestoneReceive::Event(AppWindowEvent::LastWindowClosed)
    );
    assert_eq!(
        receive_initial_milestone(&events, None, false, false),
        InitialMilestoneReceive::Disconnected
    );
}

#[cfg(target_os = "macos")]
#[test]
fn native_event_receive_facade_retains_post_init_duplicate_desktop_refusal() {
    use super::{AppWindowEvent, InitialMilestoneReceive, receive_initial_milestone};
    let (send, events) = mpsc::channel();
    send.send(AppWindowEvent::DesktopObserved { facts: None })
        .expect("duplicate native sample");
    drop(send);
    assert_eq!(
        receive_initial_milestone(&events, None, false, false),
        InitialMilestoneReceive::UnexpectedDesktop
    );
    assert_eq!(
        receive_initial_milestone(&events, None, false, false),
        InitialMilestoneReceive::Disconnected
    );
}

#[cfg(target_os = "macos")]
#[test]
fn native_event_caller_facade_waits_for_last_sender_despite_reader_stop() {
    use super::{AppWindowEvent, next_native_window_event, window_router};
    use keld_wv::wkwebview::MacDialogUiOutcome;

    let (mut router, _peer, _owner_commands, _window_commands) =
        initial_ready_router_with_window_loop();
    router.windows = Some(Arc::new(Mutex::new(window_router::WindowSession::new(
        true,
    ))));
    router.shutdown.reader_stop.store(true, Ordering::Release);
    let (send, events) = mpsc::channel();
    let last_sender = send.clone();
    let (armed_send, armed) = mpsc::sync_channel(0);
    let (result_send, result) = mpsc::channel();
    let worker = thread::spawn(move || {
        armed_send.send(()).expect("announce caller");
        for _ in 0..2 {
            let observed =
                next_native_window_event(&events, &router, &router.window_commands, None, false);
            if result_send.send(observed).is_err() {
                return;
            }
        }
    });
    armed.recv().expect("caller started");
    let early = result.recv_timeout(Duration::from_millis(500));
    let waited_for_event = matches!(&early, Err(mpsc::RecvTimeoutError::Timeout));
    let delivery = send.send(AppWindowEvent::MacDialogFinished {
        attempt: 1,
        nonce: 1,
        outcome: MacDialogUiOutcome::Dismissed,
    });
    drop(send);
    let first = early.or_else(|_| result.recv_timeout(Duration::from_secs(2)));
    let before_last_drop = result.recv_timeout(Duration::from_millis(500));
    let waited_for_last_sender = matches!(&before_last_drop, Err(mpsc::RecvTimeoutError::Timeout));
    drop(last_sender);
    let final_result = before_last_drop.or_else(|_| result.recv_timeout(Duration::from_secs(2)));
    let joined = worker.join();

    // Even a broken caller must release owned senders and join before asserting.
    assert!(
        waited_for_event,
        "facade reader_stop cannot end native event custody"
    );
    assert!(delivery.is_ok(), "late native event remains accepted");
    assert_eq!(
        first
            .expect("native event response")
            .expect("valid native event"),
        Some(AppWindowEvent::MacDialogFinished {
            attempt: 1,
            nonce: 1,
            outcome: MacDialogUiOutcome::Dismissed,
        })
    );
    assert!(
        waited_for_last_sender,
        "one retained native producer keeps the established facade alive"
    );
    assert_eq!(
        final_result
            .expect("final sender close")
            .expect("clean channel close"),
        None
    );
    assert!(joined.is_ok());
}

#[cfg(target_os = "macos")]
#[test]
fn native_event_caller_legacy_observes_reader_stop_with_live_sender() {
    use super::next_native_window_event;

    let (router, _peer, _owner_commands, _window_commands) =
        initial_ready_router_with_window_loop();
    assert!(
        router.windows.is_none(),
        "fixture selects legacy caller policy"
    );
    let stop = Arc::clone(&router.shutdown.reader_stop);
    let (send, events) = mpsc::channel();
    let (armed_send, armed) = mpsc::sync_channel(0);
    let (result_send, result) = mpsc::channel();
    let worker = thread::spawn(move || {
        armed_send.send(()).expect("announce caller");
        let observed =
            next_native_window_event(&events, &router, &router.window_commands, None, false);
        let _ = result_send.send(observed);
    });
    armed.recv().expect("caller started");
    let early = result.recv_timeout(Duration::from_millis(500));
    let waiting_before_stop = matches!(&early, Err(mpsc::RecvTimeoutError::Timeout));
    stop.store(true, Ordering::Release);
    let stopped = early.or_else(|_| result.recv_timeout(Duration::from_secs(2)));
    // Keep the producer live until after the bounded stop observation. Dropping
    // it earlier would let a broken blocking legacy receive pass by disconnection.
    drop(send);
    let joined = worker.join();

    assert!(
        waiting_before_stop,
        "legacy source stays live before its stop"
    );
    assert_eq!(
        stopped
            .expect("reader stop remains bounded")
            .expect("clean legacy stop"),
        None
    );
    assert!(joined.is_ok());
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

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
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
    WindowsDevLeaseMonitor, WindowsProfileMode, clear_windows_handle_inheritance,
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
mod application_name_for_user_agent;
#[cfg(target_os = "macos")]
include!("tests/gh499_stage_graph_admission.rs");
include!("tests/profile_admission.rs");
include!("tests/filesystem_guard_and_recovery.rs");
include!("tests/guarded_router_and_quit.rs");
// The existing macOS declaration stays with its historical selector. Linux
// reuses the same real Runtime/HELLO identity proof without any GUI claim.
#[cfg(target_os = "linux")]
#[path = "tests/accepted_primary_identity.rs"]
mod accepted_primary_identity_linux;
include!("tests/linux_strict_test_flags.rs");
include!("tests/role_shutdown_and_worker.rs");
include!("tests/successor_single_flight.rs");
include!("tests/generation_correlation_and_auth.rs");
include!("tests/filesystem_quiesce.rs");
include!("tests/router_lifecycle.rs");
include!("tests/recovered_events_and_echo.rs");
include!("tests/ready_recovery.rs");
include!("tests/failed_ready_and_cli_lease.rs");
include!("tests/terminal_cleanup_and_windows_shutdown.rs");
