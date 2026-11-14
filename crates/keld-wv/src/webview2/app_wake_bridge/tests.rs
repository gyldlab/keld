//! Pure same-worker health observation; no native window or dequeue receipt.
use super::*;
use std::sync::mpsc;

#[test]
fn completed_post_failure_is_observable_without_any_delivered_tao_wake() {
    let (commands, command_rx) = mpsc::channel();
    commands.send(AppWindowCommand::Quit).unwrap();
    let (_wake_sender, wake_receiver) = mpsc::channel::<()>();
    let mut bridge = AppWakeBridge::prepare(
        command_rx,
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let input = Arc::clone(&bridge.input);
    let stop = Arc::clone(&bridge.stop);
    let terminal = Arc::clone(&bridge.terminal_intent);
    bridge.worker = Worker::Running(thread::spawn(move || {
        input::run(&input, &stop, &terminal, &mut || Err(()), &mut |_| {})
    }));
    let before = Instant::now();
    let next = bridge
        .next_failure_observation()
        .expect("same worker health deadline");
    assert!(next >= before);
    assert!(next <= Instant::now() + POLL_INTERVAL);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if bridge.poll_failure().is_err() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "unit-post failure must become visible without a wake"
        );
        thread::park_timeout(Duration::from_millis(1));
    }
    assert!(matches!(
        wake_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(bridge.input.lock().unwrap().has_current());
    assert!(
        bridge.try_finish().is_err(),
        "failure cannot become endpoint drain"
    );
}
