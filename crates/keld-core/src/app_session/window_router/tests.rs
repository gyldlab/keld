//! Component proof over real platform sockets and the actual Core handler/writer.
//! Fixture identities and fake UI commands do not prove Runtime admission or native
//! windows. Actual app-bound Subscribe admission and GH-446 Init remain separate.
use super::super::window_registry::WindowOwner;
use super::super::{BootstrapStream, PrimaryCallerContext, PrimaryRouterHandle};
use super::{WindowSession, event, native_state, subscription_refusal};
use keld_ipc::channel_table::{Authority, ReceiveClass};
use keld_ipc::codec::{decode, encode};
use keld_ipc::frame::{CorrelationId, FrameKind};
use keld_ipc::link::{AppLinkDeadlines as _, read_frame, write_frame};
use keld_ipc::window::{Rect, WindowEvent, WindowId, WindowRequest, WindowResponse, WindowState};
use keld_ipc::{CallError, LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleResponse, SubscribeRefusal};
use keld_wv::{AppWindowCommand, AppWindowEvent, WebviewId, WindowSnapshot};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

#[cfg(any(target_os = "linux", windows))]
use super::super::DirectPrimaryOwnerCommand as OwnerCommand;
#[cfg(target_os = "macos")]
use super::super::GuardianOwnerCommand as OwnerCommand;

struct Fixture {
    handle: PrimaryRouterHandle,
    peer: BootstrapStream,
    commands: mpsc::Receiver<AppWindowCommand>,
    owner_commands: mpsc::Receiver<OwnerCommand>,
}
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

impl Fixture {
    fn new(support: Option<bool>) -> TestResult<Self> {
        let (mut handle, peer, owner_commands, commands) =
            super::super::tests::initial_ready_router_with_window_loop();
        handle.windows = support.map(|native| Arc::new(Mutex::new(WindowSession::new(native))));
        handle.recovery_armed.store(true, Ordering::Release);
        peer.set_app_link_deadlines(Some(Duration::from_secs(3)))?;
        Ok(Self {
            handle,
            peer,
            commands,
            owner_commands,
        })
    }
    fn caller(&self) -> TestResult<PrimaryCallerContext> {
        let guard = self
            .handle
            .current
            .lock()
            .map_err(|_| std::io::Error::other("current fixture lock poisoned"))?;
        let active = guard
            .as_ref()
            .ok_or_else(|| std::io::Error::other("active fixture is absent"))?;
        Ok(PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        })
    }
    fn reader(&self) -> TestResult<BootstrapStream> {
        let current = self
            .handle
            .current
            .lock()
            .map_err(|_| std::io::Error::other("current fixture lock poisoned"))?;
        let active = current
            .as_ref()
            .ok_or_else(|| std::io::Error::other("active fixture is absent"))?;
        let reader = active.writer.try_clone()?;
        reader.set_app_link_read_deadline(Some(super::super::APP_LINK_READER_POLL))?;
        Ok(reader)
    }
    fn retire_reader(&self, attempt: u32) -> TestResult {
        self.handle.retire_generation(attempt)?;
        let reader = self
            .handle
            .readers
            .lock()
            .map_err(|_| std::io::Error::other("reader fixture lock poisoned"))?
            .remove(&attempt);
        if let Some(reader) = reader {
            reader
                .join()
                .map_err(|_| std::io::Error::other("component reader panicked"))??;
        }
        Ok(())
    }
    fn subscribe(&mut self) -> TestResult {
        self.handle.subscribe_window(
            &self.caller()?,
            CorrelationId(11),
            keld_ipc::channel_table::WINDOW.wire_id(),
        )?;
        let (header, bytes) = read_frame(&mut self.peer)?;
        assert_eq!(
            (header.kind, header.channel, header.corr),
            (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(11))
        );
        assert_eq!(
            decode::<LifecycleResponse>(&bytes)?,
            LifecycleResponse::Subscribed
        );
        Ok(())
    }
    fn ready(&mut self) -> TestResult {
        self.handle.signal_ready()?;
        let (header, bytes) = read_frame(&mut self.peer)?;
        assert_eq!(
            (header.kind, header.channel, header.corr),
            (FrameKind::Event, LIFECYCLE_CHANNEL, CorrelationId(0))
        );
        assert_eq!(decode::<LifecycleEvent>(&bytes)?, LifecycleEvent::Ready);
        Ok(())
    }
    fn call(&self, request: &WindowRequest, correlation: u32) -> TestResult {
        self.handle.dispatch_window(
            &self.caller()?,
            CorrelationId(correlation),
            &encode(request)?,
        )?;
        Ok(())
    }
    fn error(&mut self, correlation: u32, code: &str) -> TestResult {
        let (header, bytes) = read_frame(&mut self.peer)?;
        assert_eq!(
            (header.kind, header.channel, header.corr),
            (
                FrameKind::Err,
                keld_ipc::channel_table::WINDOW.id(),
                CorrelationId(correlation)
            )
        );
        assert_eq!(decode::<CallError>(&bytes)?.code, code);
        Ok(())
    }
    fn build(&mut self, correlation: u32, view: u32) -> TestResult<keld_ipc::window::WindowRef> {
        self.call(
            &WindowRequest::Create {
                width: 640,
                height: 480,
                x: None,
                y: None,
            },
            correlation,
        )?;
        let AppWindowCommand::CreateWindow {
            build,
            width,
            height,
            x,
            y,
        } = self.commands.recv_timeout(Duration::from_secs(3))?
        else {
            return Err(std::io::Error::other("Create command expected").into());
        };
        assert_eq!((width, height, x, y), (640, 480, None, None));
        self.handle
            .window_native_event(AppWindowEvent::WindowBuilt {
                build,
                view: WebviewId(view),
                snapshot: snapshot(),
            })?;
        let (event_header, bytes) = read_frame(&mut self.peer)?;
        assert_eq!(
            (event_header.kind, event_header.channel, event_header.corr),
            (
                FrameKind::Event,
                keld_ipc::channel_table::WINDOW.id(),
                CorrelationId(0)
            )
        );
        let WindowEvent::Created {
            window,
            state,
            pending_close,
        } = decode(&bytes)?
        else {
            return Err(std::io::Error::other("Created event required").into());
        };
        assert_eq!(state, native_state(snapshot()));
        assert_eq!(pending_close, None);
        let (reply_header, bytes) = read_frame(&mut self.peer)?;
        assert_eq!(
            (reply_header.kind, reply_header.channel, reply_header.corr),
            (
                FrameKind::Reply,
                keld_ipc::channel_table::WINDOW.id(),
                CorrelationId(correlation)
            )
        );
        assert_eq!(
            decode::<WindowResponse>(&bytes)?,
            WindowResponse::Created { window }
        );
        Ok(window)
    }
}
fn snapshot() -> WindowSnapshot {
    WindowSnapshot {
        x: 37,
        y: 91,
        width: 800,
        height: 600,
        focused: true,
        maximized: false,
        full_screen: false,
    }
}
#[test]
fn window_router_subscription_precedes_ready_and_body_decoding() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture
        .handle
        .dispatch_window(&fixture.caller()?, CorrelationId(1), &[0xff])
        .expect("unsubscribed malformed body is not decoded");
    fixture.error(1, "KELD-CORE-045")?;
    fixture.subscribe()?;
    fixture
        .handle
        .dispatch_window(&fixture.caller()?, CorrelationId(2), &[0xff])
        .expect("not-ready malformed body is not decoded");
    fixture.error(2, "KELD-CORE-040")?;
    assert!(fixture.commands.try_recv().is_err());
    Ok(())
}
#[test]
fn window_router_native_snapshot_identity_and_setter_wire_are_actual() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let first = fixture.build(21, 7)?;
    let second = fixture.build(22, 8)?;
    assert_eq!((first.id, second.id), (WindowId(1), WindowId(2)));
    fixture.call(
        &WindowRequest::SetMaximized {
            window: first,
            maximized: true,
        },
        23,
    )?;
    assert_eq!(
        fixture
            .commands
            .recv_timeout(Duration::from_secs(3))
            .expect("setter command"),
        AppWindowCommand::SetMaximized {
            view: WebviewId(7),
            maximized: true
        }
    );
    let (header, bytes) = read_frame(&mut fixture.peer).expect("accepted response");
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(23))
    );
    assert_eq!(
        decode::<WindowResponse>(&bytes).expect("setter response"),
        WindowResponse::Accepted
    );
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowMaximized {
            view: WebviewId(7),
            maximized: true,
        })
        .expect("native truth");
    let (header, bytes) = read_frame(&mut fixture.peer).expect("native-confirmed event");
    assert_eq!(header.kind, FrameKind::Event);
    assert_eq!(
        decode::<WindowEvent>(&bytes).expect("Maximized fact"),
        WindowEvent::Maximized {
            window: first,
            maximized: true
        }
    );
    Ok(())
}
#[test]
fn window_router_refuses_t2_calls_without_implementation_or_effects() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    for (corr, request) in [
        (2, WindowRequest::RequestClose { window }),
        (
            3,
            WindowRequest::CloseReply {
                window,
                close_seq: 1,
                verdict: keld_ipc::window::CloseVerdict::Allow,
            },
        ),
        (4, WindowRequest::Destroy { window }),
    ] {
        fixture.call(&request, corr)?;
        fixture.error(corr, "KELD-CORE-038")?;
        assert!(
            fixture.commands.try_recv().is_err(),
            "T2 stage refusal has no native effects"
        );
    }
    assert_eq!(
        fixture
            .handle
            .windows
            .as_ref()
            .expect("registry")
            .lock()
            .expect("registry lock")
            .registry
            .replay(owner_from(&fixture)?)
            .count(),
        1
    );
    Ok(())
}
fn owner_from(fixture: &Fixture) -> TestResult<WindowOwner> {
    let caller = fixture.caller()?;
    Ok(WindowOwner::new(caller.identity, caller.attempt))
}
#[test]
fn window_router_unsupported_platform_is_typed_after_subscription_and_ready() -> TestResult {
    let mut fixture = Fixture::new(Some(false))?;
    fixture.subscribe()?;
    fixture.ready()?;
    fixture.call(
        &WindowRequest::Create {
            width: 1,
            height: 1,
            x: None,
            y: None,
        },
        42,
    )?;
    fixture.error(42, "KELD-CORE-042")?;
    assert!(fixture.commands.try_recv().is_err());
    Ok(())
}
#[test]
fn window_router_renderer_mode_remains_not_subscribable() -> TestResult {
    let mut fixture = Fixture::new(None)?;
    fixture
        .handle
        .subscribe_window(
            &fixture.caller()?,
            CorrelationId(1),
            keld_ipc::channel_table::WINDOW.wire_id(),
        )
        .expect("legacy refusal");
    let (_, bytes) = read_frame(&mut fixture.peer).expect("refusal");
    assert_eq!(
        decode::<LifecycleResponse>(&bytes).expect("response"),
        LifecycleResponse::SubscribeRefused {
            reason: SubscribeRefusal::NotSubscribable
        }
    );
    fixture.ready()?;
    assert!(fixture.handle.windows.is_none());
    Ok(())
}
#[test]
fn window_router_subscription_reasons_are_closed_and_do_not_mutate_state() -> TestResult {
    let guarded = Authority::Guarded(&[keld_guard::capability::FS_READ]);
    assert_eq!(
        subscription_refusal(
            ReceiveClass::HostCall,
            Authority::HostInternal,
            false,
            false
        ),
        Some(SubscribeRefusal::NotSubscribable)
    );
    assert_eq!(
        subscription_refusal(ReceiveClass::HostCallWithEvents, guarded, false, false),
        Some(SubscribeRefusal::GuardedNotSupported)
    );
    assert_eq!(
        subscription_refusal(
            ReceiveClass::HostCallWithEvents,
            Authority::HostInternal,
            false,
            false
        ),
        Some(SubscribeRefusal::RoleNotAdmitted)
    );
    assert_eq!(
        subscription_refusal(
            ReceiveClass::HostCallWithEvents,
            Authority::HostInternal,
            true,
            true
        ),
        Some(SubscribeRefusal::NotSubscribable)
    );
    let mut fixture = Fixture::new(Some(true))?;
    fixture
        .handle
        .subscribe_window(&fixture.caller()?, CorrelationId(3), u16::MAX)
        .expect("unknown refusal");
    let (_, bytes) = read_frame(&mut fixture.peer).expect("unknown response");
    assert_eq!(
        decode::<LifecycleResponse>(&bytes).expect("response"),
        LifecycleResponse::SubscribeRefused {
            reason: SubscribeRefusal::UnknownChannel
        }
    );
    assert!(
        fixture
            .handle
            .windows
            .as_ref()
            .expect("service")
            .lock()
            .expect("lock")
            .subscribed
            .is_none()
    );
    Ok(())
}
#[test]
fn window_router_stale_attempt_foreign_arc_and_stopped_callers_are_inert() -> TestResult {
    let fixture = Fixture::new(Some(true))?;
    let current = fixture.caller()?;
    for stale in [
        PrimaryCallerContext {
            attempt: current.attempt + 1,
            ..current.clone()
        },
        PrimaryCallerContext {
            reader_stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            ..current.clone()
        },
    ] {
        fixture
            .handle
            .dispatch_window(&stale, CorrelationId(1), &[0xff])
            .expect("stale body never decoded");
        fixture
            .handle
            .subscribe_window(
                &stale,
                CorrelationId(2),
                keld_ipc::channel_table::WINDOW.wire_id(),
            )
            .expect("foreign callback inert");
    }
    current.reader_stop.store(true, Ordering::Release);
    fixture
        .handle
        .dispatch_window(&current, CorrelationId(3), &[0xff])
        .expect("stopped callback inert");
    assert!(
        fixture
            .handle
            .windows
            .as_ref()
            .expect("service")
            .lock()
            .expect("lock")
            .subscribed
            .is_none()
    );
    assert!(fixture.commands.try_recv().is_err());
    Ok(())
}
#[test]
fn window_router_terminal_cut_precedes_release_and_is_idempotent() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    {
        let _transition = fixture.handle.shutdown.transition_guard();
        fixture
            .handle
            .end_window_session_guarded()
            .expect("terminal owner cut");
        fixture
            .handle
            .end_window_session_guarded()
            .expect("idempotent second service drive");
    }
    assert_eq!(
        fixture
            .commands
            .recv_timeout(Duration::from_secs(3))
            .expect("sole teardown"),
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    assert!(fixture.commands.try_recv().is_err());
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowReleased { view: WebviewId(7) })
        .expect("physical release after terminal state");
    let (_, bytes) =
        read_frame(&mut fixture.peer).expect("Destroyed, no last-window on terminal cut");
    assert_eq!(
        decode::<WindowEvent>(&bytes).expect("Destroyed fact"),
        WindowEvent::Destroyed { window }
    );
    assert!(
        fixture
            .handle
            .windows
            .as_ref()
            .expect("service")
            .lock()
            .expect("lock")
            .registry
            .replay(owner_from(&fixture)?)
            .next()
            .is_none()
    );
    Ok(())
}
#[test]
fn window_router_role_loss_replays_before_subscribed_and_adopts_without_build() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    let old = owner_from(&fixture)?;
    fixture
        .handle
        .retire_generation(1)
        .expect("actual retirement");
    assert!(fixture.handle.current.lock().expect("current").is_none());
    assert_ne!(
        fixture
            .handle
            .windows
            .as_ref()
            .expect("service")
            .lock()
            .expect("lock")
            .subscribed,
        Some(old)
    );
    // Replace the component socket with a fresh correctly fenced Core link. This
    // is a port/state fixture, not an authenticated Runtime principal claim.
    let successor = Fixture::new(None)?;
    let mut active = successor
        .handle
        .current
        .lock()
        .expect("successor current")
        .take()
        .expect("successor fixture generation");
    active.attempt = 2;
    fixture.peer = successor.peer;
    *fixture.handle.current.lock().expect("current") = Some(active);
    fixture
        .handle
        .subscribe_window(
            &fixture.caller()?,
            CorrelationId(2),
            keld_ipc::channel_table::WINDOW.wire_id(),
        )
        .expect("successor subscription");
    let (header, bytes) = read_frame(&mut fixture.peer).expect("complete replay before reply");
    assert_eq!(header.kind, FrameKind::Event);
    assert_eq!(
        decode::<WindowEvent>(&bytes).expect("replayed Created"),
        WindowEvent::Created {
            window,
            state: native_state(snapshot()),
            pending_close: None
        }
    );
    let (header, bytes) =
        read_frame(&mut fixture.peer).expect("subscription acceptance after replay");
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(2))
    );
    assert_eq!(
        decode::<LifecycleResponse>(&bytes).expect("acceptance"),
        LifecycleResponse::Subscribed
    );
    fixture.call(
        &WindowRequest::Create {
            width: 99,
            height: 99,
            x: None,
            y: None,
        },
        3,
    )?;
    let (_, bytes) = read_frame(&mut fixture.peer).expect("adoption response");
    assert_eq!(
        decode::<WindowResponse>(&bytes).expect("Adopted"),
        WindowResponse::Adopted { window }
    );
    assert!(
        fixture.commands.try_recv().is_err(),
        "adoption never rebuilds native window"
    );
    fixture.subscribe()?;
    fixture.call(
        &WindowRequest::Create {
            width: 99,
            height: 99,
            x: None,
            y: None,
        },
        4,
    )?;
    assert!(matches!(
        fixture
            .commands
            .recv_timeout(Duration::from_secs(3))
            .expect("next intentional build"),
        AppWindowCommand::CreateWindow { .. }
    ));
    Ok(())
}

#[test]
fn window_router_duplicate_live_correlation_closes_link_before_second_effect() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let request = WindowRequest::Create {
        width: 9,
        height: 9,
        x: None,
        y: None,
    };
    fixture.call(&request, 91)?;
    let AppWindowCommand::CreateWindow { build, .. } = fixture
        .commands
        .recv_timeout(Duration::from_secs(3))
        .expect("first build held")
    else {
        panic!("Create expected")
    };
    assert_eq!(build, 1);
    // A missing owner still cannot prevent synchronous socket retirement. Its
    // cleanup error stays attached to the primary IPC005, without a timed wait.
    let caller = fixture.caller()?;
    drop(fixture.owner_commands);
    let error = fixture
        .handle
        .dispatch_window(&caller, CorrelationId(91), &[0xff])
        .expect_err("duplicate rejected before second decode");
    assert!(error.to_string().contains("KELD-IPC-005"), "{error}");
    assert!(fixture.handle.current.lock().expect("current").is_none());
    assert!(
        fixture.commands.try_recv().is_err(),
        "only the first native command was admitted"
    );
    assert!(
        read_frame(&mut fixture.peer).is_err(),
        "unanswered first Create observes actual link loss, not a fabricated no-effect reply"
    );
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowBuilt {
            build,
            view: WebviewId(7),
            snapshot: snapshot(),
        })
        .expect("late build still physically cleaned up");
    assert_eq!(
        fixture
            .commands
            .recv_timeout(Duration::from_secs(3))
            .expect("late cleanup"),
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    Ok(())
}

#[test]
fn window_router_existing_primary_reader_admits_the_named_window_policy() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    let caller = fixture.caller()?;
    let mut reader = fixture.reader()?;
    let handle = fixture.handle.clone();
    let reader_caller = caller.clone();
    let join = std::thread::spawn(move || {
        super::super::read_primary_frames(&mut reader, &handle, &reader_caller)
    });
    let request = encode(&keld_ipc::LifecycleRequest::Subscribe {
        channel: keld_ipc::channel_table::WINDOW.wire_id(),
    })
    .expect("Subscribe codec");
    write_frame(
        &mut fixture.peer,
        FrameKind::Call,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(61),
        &request,
    )
    .expect("actual ingress Subscribe");
    let (header, bytes) = read_frame(&mut fixture.peer).expect("actual ingress acceptance");
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(61))
    );
    assert_eq!(
        decode::<LifecycleResponse>(&bytes).expect("accepted"),
        LifecycleResponse::Subscribed
    );
    fixture.ready()?;
    let create = encode(&WindowRequest::Create {
        width: 11,
        height: 12,
        x: None,
        y: None,
    })
    .expect("Create codec");
    write_frame(
        &mut fixture.peer,
        FrameKind::Call,
        0,
        keld_ipc::channel_table::WINDOW.id(),
        CorrelationId(62),
        &create,
    )
    .expect("actual ingress window frame");
    let AppWindowCommand::CreateWindow {
        build,
        width,
        height,
        ..
    } = fixture
        .commands
        .recv_timeout(Duration::from_secs(3))
        .expect("actual ingress reached UI command owner")
    else {
        panic!("native build command")
    };
    assert_eq!((width, height), (11, 12));
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowBuilt {
            build,
            view: WebviewId(17),
            snapshot: snapshot(),
        })
        .expect("native result");
    let (event_header, bytes) = read_frame(&mut fixture.peer).expect("Created event");
    let WindowEvent::Created { window, .. } = decode(&bytes).expect("Created payload") else {
        panic!("Created")
    };
    assert_eq!(
        (event_header.kind, window.id),
        (FrameKind::Event, WindowId(1))
    );
    let (header, bytes) = read_frame(&mut fixture.peer).expect("correlated Create reply");
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(62))
    );
    assert_eq!(
        decode::<WindowResponse>(&bytes).expect("Create response"),
        WindowResponse::Created { window }
    );
    caller.reader_stop.store(true, Ordering::Release);
    join.join()
        .expect("join actual reader")
        .expect("bounded poll exits on existing reader-stop owner");
    Ok(())
}

#[test]
fn window_router_maximum_created_fact_fits_the_fixed_writer_storage() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    let fact = WindowEvent::Created {
        window: keld_ipc::window::WindowRef {
            id: WindowId(u32::MAX),
            incarnation: keld_ipc::window::WindowGeneration(u64::MAX),
        },
        state: WindowState {
            bounds: Rect {
                x: i32::MIN,
                y: i32::MAX,
                width: u32::MAX,
                height: u32::MAX,
            },
            focused: true,
            maximized: true,
            full_screen: true,
        },
        pending_close: Some(u64::MAX),
    };
    let mut failed = None;
    {
        let mut guard = fixture.handle.current.lock().expect("current writer");
        event(guard.as_mut().expect("active writer"), fact, &mut failed)
            .expect("maximum fact encodes into caller storage");
    }
    let (header, bytes) = read_frame(&mut fixture.peer).expect("actual maximum fact frame");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0)
        )
    );
    assert_eq!(
        bytes.len(),
        50,
        "pinned maximum postcard field widths, not a heap/RSS budget"
    );
    assert_eq!(
        decode::<WindowEvent>(&bytes).expect("exact maximum payload"),
        fact
    );
    assert!(failed.is_none());
    Ok(())
}

#[test]
fn window_router_successor_has_no_stale_legacy_last_window_replay() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let first = fixture.build(1, 7)?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowCloseRequested { view: WebviewId(7) })
        .expect("native close request");
    assert_eq!(
        fixture
            .commands
            .recv_timeout(Duration::from_secs(3))
            .expect("sole Destroy"),
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowReleased { view: WebviewId(7) })
        .expect("actual release receipt");
    let (header, bytes) = read_frame(&mut fixture.peer).expect("Destroyed before LastWindowClosed");
    assert_eq!(header.kind, FrameKind::Event);
    assert_eq!(
        decode::<WindowEvent>(&bytes).expect("Destroyed"),
        WindowEvent::Destroyed { window: first }
    );
    let (header, bytes) =
        read_frame(&mut fixture.peer).expect("real registry last-window transition");
    assert_eq!(header.channel, LIFECYCLE_CHANNEL);
    assert_eq!(
        decode::<LifecycleEvent>(&bytes).expect("last-window"),
        LifecycleEvent::LastWindowClosed
    );
    let second = fixture.build(2, 8)?;
    fixture
        .handle
        .retire_generation(1)
        .expect("retire old link");
    let successor = Fixture::new(None)?;
    let server = successor
        .handle
        .current
        .lock()
        .expect("successor fixture")
        .take()
        .expect("server")
        .writer;
    fixture.peer = successor.peer;
    fixture
        .handle
        .install_generation(2, server)
        .expect("install actual successor component stream");
    let (_, bytes) = read_frame(&mut fixture.peer).expect("retained Ready");
    assert_eq!(
        decode::<LifecycleEvent>(&bytes).expect("Ready"),
        LifecycleEvent::Ready
    );
    write_frame(
        &mut fixture.peer,
        FrameKind::Call,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(71),
        &encode(&keld_ipc::LifecycleRequest::Subscribe {
            channel: keld_ipc::channel_table::WINDOW.wire_id(),
        })
        .expect("Subscribe body"),
    )
    .expect("successor Subscribe ingress");
    let first_after_ready = read_frame(&mut fixture.peer).expect("next actual frame");
    let second_after_ready = read_frame(&mut fixture.peer).expect("correlated acceptance");
    // Release the real reader before any assertion can fail; the original
    // regression frame is retained independently in these owned observations.
    fixture.retire_reader(2)?;
    assert_eq!(
        (first_after_ready.0.kind, first_after_ready.0.channel),
        (FrameKind::Event, keld_ipc::channel_table::WINDOW.id()),
        "stale legacy LastWindowClosed must not precede current Created replay"
    );
    assert_eq!(
        decode::<WindowEvent>(&first_after_ready.1).expect("current survivor replay"),
        WindowEvent::Created {
            window: second,
            state: native_state(snapshot()),
            pending_close: None
        }
    );
    assert_eq!(
        (
            second_after_ready.0.kind,
            second_after_ready.0.channel,
            second_after_ready.0.corr
        ),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(71))
    );
    assert_eq!(
        decode::<LifecycleResponse>(&second_after_ready.1).expect("accepted"),
        LifecycleResponse::Subscribed
    );
    Ok(())
}

#[test]
fn window_router_ui_terminal_cut_fences_successor_installation() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    fixture.build(1, 7)?;
    fixture
        .handle
        .window_main_owner_stopping(true)
        .expect("existing main-owner terminal cut");
    let stopped = fixture.handle.shutdown.reader_stop.load(Ordering::Acquire);
    let denied_recovery = matches!(
        fixture.owner_commands.try_recv(),
        Ok(OwnerCommand::DenyRecovery)
    );
    fixture
        .handle
        .retire_generation(1)
        .expect("retire old generation");
    let successor = Fixture::new(None)?;
    let server = successor
        .handle
        .current
        .lock()
        .expect("successor fixture")
        .take()
        .expect("server")
        .writer;
    let mut peer = successor.peer;
    fixture
        .handle
        .install_generation(2, server)
        .expect("terminal installer is a closed inert path");
    let installed = fixture.handle.current.lock().expect("current").is_some();
    let leaked_ready = if installed {
        Some(read_frame(&mut peer).expect("baseline leak is an actual Ready frame"))
    } else {
        None
    };
    fixture.retire_reader(2)?;
    assert!(
        stopped,
        "UI terminal intent must stop existing Core reader/admission before physical native release"
    );
    assert!(
        denied_recovery,
        "the existing Runtime owner must be denied recovery"
    );
    assert!(
        !installed && leaked_ready.is_none(),
        "terminated registry cannot install a successor or publish stale Ready"
    );
    Ok(())
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn window_router_accepted_quit_reply_precedes_teardown_failure_with_pending_create() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    fixture.build(1, 7)?;
    fixture.call(
        &WindowRequest::Create {
            width: 9,
            height: 9,
            x: None,
            y: None,
        },
        2,
    )?;
    assert!(matches!(
        fixture
            .commands
            .recv_timeout(Duration::from_secs(3))
            .expect("second Create held before native result"),
        AppWindowCommand::CreateWindow { .. }
    ));
    // Make the existing UI queue unavailable. This must not erase the owed
    // correlated Quit reply on a healthy socket while Create remains unanswered.
    drop(fixture.commands);
    let mut reader = fixture
        .handle
        .current
        .lock()
        .expect("current")
        .as_ref()
        .expect("active")
        .writer
        .try_clone()
        .expect("actual quit reader");
    let handle = fixture.handle.clone();
    let worker = std::thread::spawn(move || {
        handle.lifecycle_quit(
            1,
            CorrelationId(77),
            &encode(&LifecycleResponse::Quit).expect("Quit reply"),
            &mut reader,
        )
    });
    let OwnerCommand::PrepareAcceptedShutdown(reply) = fixture
        .owner_commands
        .recv_timeout(Duration::from_secs(3))
        .expect("existing prepare owner cut")
    else {
        panic!("only existing prepare is expected")
    };
    reply.send(Ok(())).expect("owned prepare acknowledgement");
    let failure = worker
        .join()
        .expect("Quit handler joins")
        .expect_err("UI queue failure stays observable");
    assert!(
        failure
            .to_string()
            .contains("UI command owner is unavailable"),
        "{failure}"
    );
    // The real host caller closes this failed session. A Unix close retains its
    // earlier written bytes; the oracle is a correlated frame or immediate EOF,
    // never a timeout used as proof that no frame was written.
    if let Some(active) = fixture.handle.current.lock().expect("current").as_ref() {
        active
            .writer
            .shutdown_app_link()
            .expect("failed session close");
    }
    let (header, bytes) = read_frame(&mut fixture.peer)
        .expect("accepted Quit must have been replied before native teardown failed");
    assert_eq!(
        (header.kind, header.channel, header.corr),
        (FrameKind::Reply, LIFECYCLE_CHANNEL, CorrelationId(77))
    );
    assert_eq!(
        decode::<LifecycleResponse>(&bytes).expect("Quit reply"),
        LifecycleResponse::Quit
    );
    assert!(
        read_frame(&mut fixture.peer).is_err(),
        "original admitted Create receives link loss, not a fabricated024 or027 answer"
    );
    Ok(())
}

#[test]
fn window_router_forced_tail_marks_completion_after_managed_teardown_error() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    fixture.build(1, 7)?;
    // The window is Open, so the real forced tail must attempt its existing
    // Destroy command. Closing this receiver makes that enqueue fail directly.
    drop(fixture.commands);
    assert!(fixture.handle.shutdown.claim_cli_lease_lost());
    assert!(!fixture.handle.shutdown.tail_started.load(Ordering::Acquire));
    assert!(!fixture.handle.shutdown.tail_ended.load(Ordering::Acquire));
    let handle = fixture.handle.clone();
    let worker = std::thread::spawn(move || handle.cli_lease_lost());
    let OwnerCommand::PrepareAcceptedShutdown(reply) = fixture
        .owner_commands
        .recv_timeout(Duration::from_secs(3))
        .expect("existing forced-tail preparation")
    else {
        panic!("forced tail must prepare its existing Runtime owner")
    };
    reply
        .send(Ok(()))
        .expect("owned shutdown preparation acknowledgement");
    let failure = worker
        .join()
        .expect("forced-tail handler joins")
        .expect_err("unavailable UI command owner remains a typed failure");
    assert_eq!(failure.code(), "KELD-CORE-037");
    assert_eq!(failure.phase, "window session teardown");
    assert_eq!(failure.detail, "UI command owner is unavailable");
    assert!(fixture.handle.shutdown.tail_started.load(Ordering::Acquire));
    assert!(
        fixture.handle.shutdown.tail_ended.load(Ordering::Acquire),
        "a forced tail that returned an error must publish completion"
    );
    Ok(())
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn window_router_failed_quit_reply_still_ends_managed_windows() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    fixture.build(1, 7)?;
    let caller = fixture.caller()?;
    let mut reader = fixture.reader()?;
    let handle = fixture.handle.clone();
    let worker = std::thread::spawn(move || {
        super::super::read_primary_frames(&mut reader, &handle, &caller)
    });
    write_frame(
        &mut fixture.peer,
        FrameKind::Call,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(78),
        &encode(&keld_ipc::LifecycleRequest::Quit)?,
    )?;
    let OwnerCommand::PrepareAcceptedShutdown(reply) = fixture
        .owner_commands
        .recv_timeout(Duration::from_secs(3))
        .expect("actual Quit reached existing preparation owner")
    else {
        panic!("actual Quit must prepare its existing Runtime owner")
    };
    // No peer reference survives the acknowledgement: the actual reader must
    // encounter a failed Quit reply before any UI stop hook or router fallback.
    fixture.peer.shutdown_app_link()?;
    drop(fixture.peer);
    reply
        .send(Ok(()))
        .expect("owned preparation acknowledgement");
    let failure = worker
        .join()
        .expect("actual primary reader joins")
        .expect_err("closed peer must fail the Quit reply write");
    assert_eq!(failure.code(), "KELD-CORE-037");
    assert_eq!(failure.phase, "lifecycle Quit reply");
    let serviceable = fixture
        .handle
        .windows
        .as_ref()
        .expect("managed service")
        .lock()
        .expect("managed registry lock")
        .registry
        .mark_ready();
    let teardown = fixture.commands.try_recv();
    assert!(
        !serviceable,
        "an accepted Quit reply failure must end managed windows before reader return"
    );
    assert_eq!(
        teardown.expect("managed teardown was enqueued before reader return"),
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    Ok(())
}

// This component uses actual Runtime/Bun principals and a controlled native port.
// Other OSes retain their existing tests; this adds no cross-platform acceptance claim.
#[cfg(target_os = "linux")]
#[path = "authenticated_tests.rs"]
mod authenticated_tests;
