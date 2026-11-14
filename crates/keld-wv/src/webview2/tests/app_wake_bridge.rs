//! Real bridge ownership tests; no `WebView2` controller or COM is created.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use tao::event_loop::EventLoopBuilder;
use tao::platform::windows::EventLoopBuilderExtWindows;

use super::{AppWakeBridge, Worker};
use crate::WvError;
use crate::engine::AppWindowCommand;
use crate::webview2::WindowsLoopEvent;

#[test]
fn setup_error_retires_bridge_with_a_retained_silent_sender() {
    // Reuse the existing tests' Tao proxy facility. This loop never runs and
    // creates no browser/controller; its lifetime encloses the bridge probe.
    let mut builder = EventLoopBuilder::<WindowsLoopEvent>::with_user_event();
    builder.with_any_thread(true).with_dpi_aware(false);
    let event_loop = builder.build();
    let (commands, receiver) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let result = (|| -> Result<(), WvError> {
        let mut bridge = AppWakeBridge::prepare(
            receiver,
            None,
            Arc::clone(&stop),
            Arc::new(AtomicBool::new(false)),
        );
        bridge.start(event_loop.create_proxy())?;
        // When this actual owner is released after a setup error, Drop must
        // retire its exact worker even while the caller retains Sender. The
        // production fatal path retains the whole owner until that release.
        Err(WvError::EventLoop(String::from(
            "injected later setup error",
        )))
    })();
    assert!(
        matches!(&result, Err(WvError::EventLoop(message))
            if message == "injected later setup error"),
        "the intended post-spawn setup error must be reached: {result:?}"
    );

    // Silence ends only after the return being tested. Disconnection is the
    // independent ownership oracle: no bridge Receiver may survive that return.
    // On the failing baseline this terminal probe also lets the detached worker
    // finish; record its result before any fallback stop, so cleanup cannot pass it.
    let receiver_retired = commands.send(AppWindowCommand::Fatal).is_err();
    stop.store(true, Ordering::Release);
    // Keep the Tao target alive until even the RED worker is demonstrably gone.
    // Arc release is a positive capture-lifetime receipt, not a thread count.
    // The first sample above is frozen: fallback cleanup cannot turn it green.
    let cleanup_deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut cleanup_confirmed = false;
    while std::time::Instant::now() < cleanup_deadline {
        if Arc::strong_count(&stop) == 1 && commands.send(AppWindowCommand::Fatal).is_err() {
            cleanup_confirmed = true;
            break;
        }
        thread::yield_now();
    }
    assert!(
        cleanup_confirmed,
        "bridge cleanup did not positively confirm capture release and receiver disconnection"
    );
    drop(event_loop);
    assert!(
        receiver_retired,
        "setup error returned while the retained silent Sender still had a bridge Receiver"
    );
}

#[test]
fn stop_bridge_does_not_return_before_its_held_worker_retires() {
    let (release_worker, hold_worker) = mpsc::channel();
    let (worker_started, started) = mpsc::channel();
    let (worker_ended, ended) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _ = worker_started.send(());
        // Cleanup also opens the gate when setup failed or the receiver went.
        let _ = hold_worker.recv();
        let _ = worker_ended.send(());
        Ok(())
    });
    if let Err(error) = started.recv_timeout(Duration::from_secs(2)) {
        let _ = release_worker.send(());
        ended
            .recv_timeout(Duration::from_secs(2))
            .expect("setup failure must release the held worker before failing");
        worker
            .join()
            .expect("join worker after failed setup")
            .expect("held worker returned normally");
        panic!("worker did not reach the held boundary: {error}");
    }

    let stop = Arc::new(AtomicBool::new(false));
    let (_commands, receiver) = mpsc::channel();
    let mut bridge = AppWakeBridge::prepare(
        receiver,
        None,
        Arc::clone(&stop),
        Arc::new(AtomicBool::new(false)),
    );
    // The existing owner carries the SAME held JoinHandle. No substitute join
    // helper or second worker supplies the release being observed.
    bridge.worker = Worker::Running(worker);
    let (finished, completion) = mpsc::channel();
    let joiner = thread::spawn(move || {
        bridge
            .finish_without_renderer()
            .expect("actual standalone wake owner retires its held worker");
        finished.send(()).expect("report completed retirement");
    });

    // This is a bounded receive while the worker is deliberately held, not a
    // sleep used to guess whether the worker has finished. A missing join
    // produces an actual premature completion receipt.
    let before_release = completion.recv_timeout(Duration::from_millis(100));
    release_worker.send(()).expect("allow worker retirement");
    // The omit-join control can already have reported completion. Consume the
    // worker's own final acknowledgement before the intended semantic failure.
    ended
        .recv_timeout(Duration::from_secs(2))
        .expect("released worker must acknowledge cleanup before assertions");
    let after_release = if before_release.is_ok() {
        Ok(())
    } else {
        completion.recv_timeout(Duration::from_secs(2))
    };
    joiner.join().expect("join retirement caller");
    assert!(stop.load(Ordering::Acquire), "retirement requests stop");
    assert!(
        matches!(before_release, Err(RecvTimeoutError::Timeout)),
        "retirement completed while its worker was still held: {before_release:?}"
    );
    assert!(
        after_release.is_ok(),
        "retirement must finish after worker release: {after_release:?}"
    );
}

#[test]
fn pre_stopped_bridge_retires_without_dispatching_queued_terminal() {
    let mut builder = EventLoopBuilder::<WindowsLoopEvent>::with_user_event();
    builder.with_any_thread(true).with_dpi_aware(false);
    let event_loop = builder.build();
    let (commands, receiver) = mpsc::channel();
    commands
        .send(AppWindowCommand::Fatal)
        .expect("queue terminal before the worker exists");
    let stop = Arc::new(AtomicBool::new(true));
    let terminal_intent = Arc::new(AtomicBool::new(false));
    let mut bridge = AppWakeBridge::prepare(
        receiver,
        None,
        Arc::clone(&stop),
        Arc::clone(&terminal_intent),
    );
    bridge
        .start(event_loop.create_proxy())
        .expect("start pre-stopped worker");
    drop(bridge);

    // A queued terminal lets even the old loop terminate; no stuck-worker
    // timeout is the negative oracle. Keep Sender and Tao alive until cleanup.
    let cleanup_deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut cleanup_confirmed = false;
    while std::time::Instant::now() < cleanup_deadline {
        if Arc::strong_count(&stop) == 1 && commands.send(AppWindowCommand::Fatal).is_err() {
            cleanup_confirmed = true;
            break;
        }
        thread::yield_now();
    }
    assert!(
        cleanup_confirmed,
        "pre-stopped worker must positively retire its captures and receiver"
    );
    drop(event_loop);
    assert!(
        !terminal_intent.load(Ordering::Acquire),
        "a pre-stopped bridge dispatched a queued terminal instead of retiring"
    );
}
