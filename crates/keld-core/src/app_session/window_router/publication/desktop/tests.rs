//! Core publication tests use real Windows pipes and supplied native observations.

use super::*;
use crate::app_session::{
    AppWindowCommand, BootstrapStream, DirectPrimaryOwnerCommand, LifecycleEvent,
    LifecycleResponse, primary_writer::PrimaryWriterWorker,
    tests::initial_ready_router_with_window_loop,
};
use keld_ipc::{
    codec,
    link::{AppLinkDeadlines as _, read_frame},
};
use keld_wv::{AppDesktopDisplay, AppDesktopRect};
use std::io::{self, Read as _};
use std::sync::{Mutex, atomic::AtomicBool, mpsc::Receiver};
use std::time::Duration;

type TestError = Box<dyn std::error::Error>;

struct DesktopFixture {
    router: PrimaryRouterHandle,
    caller: PrimaryCallerContext,
    peer: BootstrapStream,
    commands: Receiver<AppWindowCommand>,
    owner: Receiver<DirectPrimaryOwnerCommand>,
    worker: PrimaryWriterWorker,
}

impl DesktopFixture {
    fn new() -> Result<Self, TestError> {
        let (mut router, peer, owner, commands) = initial_ready_router_with_window_loop();
        let mut session = WindowSession::new(true);
        session.desktop = Some(SelectedDesktop::default());
        router.windows = Some(Arc::new(Mutex::new(session)));
        let (writer, worker) = PrimaryWriterWorker::start(router.clone())?;
        router.primary_writer = Some(writer);
        let caller = {
            let _transition = router.shutdown.transition_guard();
            let mut current = router.current.lock().expect("current fixture");
            let active = current.as_mut().expect("original fixture generation");
            let caller = PrimaryCallerContext {
                identity: active.identity,
                attempt: active.attempt,
                reader_stop: Arc::clone(&active.reader_stop),
            };
            let stream = active.take_legacy_test_writer().expect("real Windows pipe");
            active.writer_link = Some(WriterLink::new(stream, caller.clone())?);
            router.bind_selected_desktop(&caller)?;
            caller
        };
        assert!(matches!(
            commands.try_recv()?,
            AppWindowCommand::WindowsDesktopProducer { .. }
        ));
        Ok(Self {
            router,
            caller,
            peer,
            commands,
            owner,
            worker,
        })
    }

    fn subscribe(&self, correlation: u32) -> Result<(), HostAppError> {
        self.router.subscribe_window_ordered(
            &self.caller,
            CorrelationId(correlation),
            keld_ipc::channel_table::WINDOW.wire_id(),
        )
    }

    fn pending_deadline(&self) -> Instant {
        self.router
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("registry")
            .desktop
            .as_ref()
            .expect("selected")
            .pending
            .as_ref()
            .expect("original pending")
            .clock
            .caller_deadline()
    }

    fn no_bytes(&mut self) -> Result<(), TestError> {
        self.peer
            .set_app_link_read_deadline(Some(Duration::from_millis(60)))?;
        let error = self
            .peer
            .read(&mut [0])
            .expect_err("no frame before native Init");
        assert!(matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ));
        self.peer
            .set_app_link_read_deadline(Some(Duration::from_secs(4)))?;
        Ok(())
    }
}

fn sample() -> AppDesktopFacts {
    let bounds = AppDesktopRect {
        x: 0,
        y: 0,
        width: 1440,
        height: 900,
    };
    AppDesktopFacts {
        primary_id: 17,
        displays: vec![AppDesktopDisplay {
            id: 17,
            bounds,
            work_area: AppDesktopRect {
                height: 856,
                ..bounds
            },
            scale_factor: 1.25,
        }],
        should_use_dark_colors: false,
    }
}

#[test]
fn first_exact_sample_and_init_settle_desktop_before_original_subscribe() -> Result<(), TestError> {
    let mut fixture = DesktopFixture::new()?;
    fixture.subscribe(41)?;
    let caller_cut = fixture.pending_deadline();
    let entry = Instant::now();
    let foreign =
        AppDesktopCapture::from_current(fixture.caller.attempt, Arc::new(AtomicBool::new(false)));
    fixture
        .router
        .observe_selected_desktop(Some(foreign), None)?;
    assert!(matches!(
        fixture.router.service_selected_desktop()?,
        DesktopStartup::Waiting
    ));
    fixture
        .router
        .observe_selected_desktop(None, Some(sample()))?;
    assert!(matches!(
        fixture.router.service_selected_desktop()?,
        DesktopStartup::Waiting
    ));
    fixture.no_bytes()?;
    fixture
        .router
        .mark_selected_desktop_initialized(entry, Instant::now())?;
    let DesktopStartup::Ready { target, deadline } = fixture.router.service_selected_desktop()?
    else {
        panic!("exact snapshot and actual Init must settle the original correlation")
    };
    assert!(target.matches(&fixture.caller));
    assert_eq!(
        deadline, caller_cut,
        "Ready keeps the original caller budget"
    );
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel),
        (FrameKind::Event, keld_ipc::channel_table::WINDOW.id())
    );
    let WindowEvent::DesktopSnapshot {
        epoch: 1,
        snapshot: Some(facts),
    } = codec::decode::<WindowEvent>(&bytes)?
    else {
        panic!("actual valid desktop event expected")
    };
    assert_eq!(facts.primary_id, 17);
    assert_eq!(facts.displays[0].work_area.height, 856);
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(41))
    );
    assert_eq!(
        codec::decode::<LifecycleResponse>(&bytes)?,
        LifecycleResponse::Subscribed
    );
    assert_eq!(
        fixture
            .router
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("registry")
            .subscribed,
        Some(WindowOwner::new(
            fixture.caller.identity,
            fixture.caller.attempt
        ))
    );
    let publisher = fixture.router.clone();
    let ready =
        std::thread::spawn(move || publisher.signal_ready_with_writer_for(&target, deadline));
    let DirectPrimaryOwnerCommand::ArmRecovery(_, reply) =
        fixture.owner.recv_timeout(Duration::from_secs(4))?
    else {
        panic!("actual Ready writer must ask the existing recovery owner")
    };
    reply.send(Ok(()))?;
    ready.join().expect("Ready owner joins")?;
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.channel),
        (FrameKind::Event, LIFECYCLE_CHANNEL)
    );
    assert_eq!(
        codec::decode::<LifecycleEvent>(&bytes)?,
        LifecycleEvent::Ready
    );
    fixture.worker.stop_and_join()?;
    Ok(())
}

#[test]
fn unavailable_init_preserves_the_first_correlation_and_never_replays() -> Result<(), TestError> {
    let mut fixture = DesktopFixture::new()?;
    fixture.subscribe(41)?;
    let cut = fixture.pending_deadline();
    fixture.subscribe(42)?;
    assert_eq!(fixture.pending_deadline(), cut);
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Err, CorrelationId(42))
    );
    assert_eq!(codec::decode::<CallError>(&bytes)?.code, "KELD-IPC-005");
    let entry = Instant::now();
    let mut malformed = sample();
    malformed.displays[0].scale_factor = f64::NAN;
    fixture
        .router
        .observe_selected_desktop(None, Some(malformed))?;
    assert!(matches!(
        fixture.router.service_selected_desktop()?,
        DesktopStartup::Waiting
    ));
    fixture.no_bytes()?;
    fixture
        .router
        .mark_selected_desktop_initialized(entry, Instant::now())?;
    let DesktopStartup::FatalQueued(error) = fixture.router.service_selected_desktop()? else {
        panic!("one fixed original 049 must terminate unavailable startup")
    };
    assert_eq!(error.code(), "KELD-CORE-049");
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Err, CorrelationId(41))
    );
    assert_eq!(codec::decode::<CallError>(&bytes)?.code, "KELD-CORE-049");
    assert!(
        fixture
            .router
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("registry")
            .subscribed
            .is_none()
    );
    assert!(!fixture.router.window_ready.load(Ordering::Acquire));
    assert_eq!(
        fixture
            .commands
            .try_iter()
            .filter(|command| matches!(command, AppWindowCommand::Fatal))
            .count(),
        1
    );
    fixture.worker.stop_and_join()?;
    Ok(())
}

#[test]
fn pre_loop_watchdog_observes_the_ten_second_cut_while_startup_owns_transition()
-> Result<(), TestError> {
    let mut fixture = DesktopFixture::new()?;
    fixture.subscribe(41)?;
    let entry_cut = fixture.pending_deadline() - Duration::from_secs(20);
    let inspect = fixture.router.clone();
    let (seen, result) = std::sync::mpsc::sync_channel(1);
    let transition = fixture.router.shutdown.transition_guard();
    let watchdog = std::thread::spawn(move || {
        let _ = seen.send(inspect.selected_desktop_startup_deadline(None, entry_cut));
    });
    let observed = result.recv_timeout(Duration::from_secs(2));
    drop(transition);
    watchdog.join().expect("watchdog retires");
    let (target, error) = observed??.expect("absolute entry cutoff");
    assert!(target.matches(&fixture.caller));
    assert_eq!(error.code(), "KELD-CORE-037");
    assert!(error.to_string().contains("10 s"));
    fixture.no_bytes()?;
    fixture.worker.stop_and_join()?;
    Ok(())
}

#[test]
fn expired_first_sample_budget_emits_no_partial_replay_or_subscriber_state() -> Result<(), TestError>
{
    let mut fixture = DesktopFixture::new()?;
    fixture.subscribe(41)?;
    let entered = Instant::now();
    fixture
        .router
        .observe_selected_desktop(None, Some(sample()))?;
    fixture
        .router
        .mark_selected_desktop_initialized(entered, Instant::now())?;
    {
        let mut session = fixture
            .router
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("Registry");
        let selected = session.desktop.as_mut().expect("selected desktop");
        selected
            .sample
            .as_mut()
            .expect("validated sample")
            .validated_at = Instant::now() - Duration::from_secs(5);
    }
    let DesktopStartup::FatalQueued(error) = fixture.router.service_selected_desktop()? else {
        panic!("an elapsed aggregate writer budget cannot admit the first frame")
    };
    assert!(error.to_string().contains("KELD-IPC-006"), "{error}");
    fixture.no_bytes()?;
    assert!(
        fixture
            .router
            .windows
            .as_ref()
            .expect("Window owner")
            .lock()
            .expect("Registry")
            .subscribed
            .is_none()
    );
    assert!(!fixture.router.window_ready.load(Ordering::Acquire));
    fixture.worker.stop_and_join()?;
    Ok(())
}
