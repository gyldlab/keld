//! Renderer terminal acceptance against a real supervised Bun primary.

use crate::app_session::{
    AppWindowCommand, DirectPrimaryOwner, PrimaryRouter, SessionShutdownState,
    stop_failed_renderer_dispatch,
};
use keld_runtime::primary::{PrimaryRoleConfig, PrimaryRoleSupervisor};
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{atomic::Ordering, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const DEADLINE: Duration = Duration::from_secs(15);

#[test]
fn held_ui_fatal_denies_runtime_successor_and_preserves_admitted_work() -> TestResult {
    exercise_renderer_failure(false)
}

#[test]
fn disconnected_ui_still_closes_reader_and_runtime_recovery() -> TestResult {
    exercise_renderer_failure(true)
}

fn exercise_renderer_failure(disconnect_ui: bool) -> TestResult {
    let _custody = keld_runtime::windows_job::install_host_death_job()?;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/app_session/tests/fixtures/accepted_primary_identity_role.ts");
    let (supervisor, recovery) = PrimaryRoleSupervisor::start_with_bound_generations_gated(
        PrimaryRoleConfig::primary("bun")
            .arg(fixture)
            .arg(listener.local_addr()?.port().to_string()),
    )?;
    let bound = supervisor
        .recv_bound_generation(DEADLINE)
        .ok_or_else(|| io::Error::other("real primary did not bind"))?;
    assert_eq!(bound.attempt(), 1);
    // The actor opens its control socket before its authenticated HELLO.
    let (stream, _) = listener.accept()?;
    stream.set_read_timeout(Some(DEADLINE))?;
    let mut control = BufReader::new(stream);
    let mut line = String::new();
    control.read_line(&mut line)?;
    assert_eq!(line.trim_end(), "BOUND");
    let shutdown = SessionShutdownState::new();
    let (ui, events) = mpsc::channel();
    let mut owner = DirectPrimaryOwner::start(supervisor, recovery, ui.clone(), shutdown.clone())?;
    let router = PrimaryRouter::start_bound(
        bound,
        ui.clone(),
        owner.handle(),
        shutdown.clone(),
        None,
        Some(true),
        None,
    )?;
    let handle = router.handle();
    owner.attach_router(handle.clone())?;
    handle.signal_ready()?;
    assert!(handle.recovery_armed.load(Ordering::Acquire));
    let original = handle
        .current_primary_writer_target()?
        .ok_or_else(|| io::Error::other("original writer absent after Ready"))?;

    let (dequeued, resume) = handle.primary_writer_owner()?.hold_next_dequeue_for_test();
    writeln!(control.get_mut(), "ECHO")?;
    dequeued.recv_timeout(DEADLINE)?;
    let retained_events = if disconnect_ui {
        drop(events);
        None
    } else {
        Some(events)
    };
    let terminal = stop_failed_renderer_dispatch(&handle, &ui);
    resume.send(())?;
    line.clear();
    control.read_line(&mut line)?;
    assert_eq!(
        line.trim_end(),
        "ECHO-OK",
        "the admitted original write completes"
    );
    let readers_stopped = original.caller().reader_stop.load(Ordering::Acquire)
        && shutdown.reader_stop.load(Ordering::Acquire);

    // The UI has not consumed Fatal. Crash the actual child and observe its
    // independent control listener, which a successor connects before HELLO.
    writeln!(control.get_mut(), "CRASH")?;
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        match listener.accept() {
            Ok(_) => panic!("accepted renderer failure allowed a Runtime successor to connect"),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        if owner.handle.as_ref().is_some_and(JoinHandle::is_finished) {
            break;
        }
        if Instant::now() >= deadline {
            return Err(
                io::Error::other("original Runtime did not reach its terminal outcome").into(),
            );
        }
        thread::yield_now();
    }
    owner.join()?;
    assert!(
        readers_stopped,
        "accepted renderer failure left an original reader open"
    );
    if let Some(events) = retained_events {
        terminal?;
        assert_eq!(
            events
                .try_iter()
                .filter(|event| matches!(event, AppWindowCommand::Fatal))
                .count(),
            1,
            "the renderer cut and later Runtime exit queue one Fatal"
        );
    } else {
        assert!(
            terminal
                .expect_err("UI disconnection stays observable")
                .to_string()
                .contains("window command owner ended")
        );
    }
    router.shutdown()?;
    Ok(())
}
