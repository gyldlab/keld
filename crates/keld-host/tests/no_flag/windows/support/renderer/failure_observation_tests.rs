use super::{BeaconFailureObservation, RendererBeacon, spawn_renderer_beacon};
use std::cell::Cell;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn failed_beacon() -> RendererBeacon {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind beacon");
    let address = listener.local_addr().expect("beacon address");
    let beacon = spawn_renderer_beacon(listener);
    let mut client = TcpStream::connect(address).expect("connect request");
    client
        .write_all(b"GET /private-sentinel HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .expect("send wrong request");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !beacon.worker.is_finished() {
        assert!(Instant::now() < deadline, "worker did not publish error");
        thread::yield_now();
    }
    beacon
}

#[test]
fn already_published_socket_error_is_reported_without_request_bytes() {
    let beacon = failed_beacon();
    let observation = beacon.observe_on_failure();
    assert_eq!(observation, BeaconFailureObservation::Failed);
    assert_eq!(
        observation.message(),
        "renderer beacon worker reported an error (details withheld)"
    );
    assert!(!observation.message().contains("private-sentinel"));
}

#[test]
fn failure_observation_never_waits_for_a_live_worker() {
    let (published_tx, beacon_rx) = mpsc::channel();
    let (activate_deadline, deadline_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (stopped_tx, stopped_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        release_rx.recv().expect("release held worker");
        drop(published_tx);
        stopped_tx.send(()).expect("report held worker exit");
    });
    let beacon = RendererBeacon {
        observed: beacon_rx,
        worker,
        activate_deadline,
    };
    let (result_tx, result_rx) = mpsc::channel();
    let observer = thread::spawn(move || result_tx.send(beacon.observe_on_failure()));
    let snapshot = result_rx.recv_timeout(Duration::from_secs(1));
    release_tx
        .send(())
        .expect("release worker after observing result");
    stopped_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("worker stopped");
    observer
        .join()
        .expect("observation thread")
        .expect("send observation");
    assert_eq!(
        snapshot.expect("failure observation must not wait"),
        BeaconFailureObservation::Pending
    );
    assert_eq!(
        deadline_rx.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    );
}

#[test]
fn console_failure_reports_published_error_and_preserves_original_panic() {
    const CHILD: &str = "KELD_TEST_GH814_CONSOLE_FAILURE_CHILD";
    const SELECTOR: &str = "support::renderer::failure_observation_tests::console_failure_reports_published_error_and_preserves_original_panic";
    if std::env::var_os(CHILD).is_some() {
        let mut beacon = Some(failed_beacon());
        let (result, beacon_failure) = super::catch_beacon_failure(&mut beacon, |_| {
            panic!("original pre-READY failure sentinel");
        });
        match result {
            Err(failure) => super::finish_beacon_failure(
                failure,
                beacon_failure,
                || {},
                std::io::stderr,
            ),
            Ok(()) => panic!("the pre-READY failure disappeared"),
        }
    }
    let output = Command::new(std::env::current_exe().expect("test binary"))
        .args([SELECTOR, "--exact", "--nocapture"])
        .env(CHILD, "1")
        .output()
        .expect("run isolated console failure");
    assert!(!output.status.success(), "the original failure disappeared");
    let stderr = String::from_utf8(output.stderr).expect("failure stderr is UTF-8");
    assert!(
        stderr.contains("original pre-READY failure sentinel"),
        "{stderr}"
    );
    assert!(
        stderr.contains(
            "KELD-BEACON-ON-FAILURE: renderer beacon worker reported an error (details withheld)"
        ),
        "{stderr}"
    );
    assert!(
        !stderr.contains("private-sentinel"),
        "request bytes leaked: {stderr}"
    );
}

#[test]
fn failed_diagnostic_sink_keeps_cleanup_and_original_panic() {
    struct BrokenWriter<'a> {
        cleanup_finished: &'a Cell<bool>,
    }

    impl Write for BrokenWriter<'_> {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            assert!(self.cleanup_finished.get(), "diagnostic ran before cleanup");
            Err(std::io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let cleaned = Cell::new(true);
    let mut direct_sink = BrokenWriter {
        cleanup_finished: &cleaned,
    };
    let error = super::write_beacon_failure(
        &mut direct_sink,
        BeaconFailureObservation::Failed,
    )
    .expect_err("writer error must remain observable");
    assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);

    cleaned.set(false);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        super::finish_beacon_failure(
            Box::new(42_u32),
            Some(BeaconFailureObservation::Failed),
            || cleaned.set(true),
            || {
                assert!(cleaned.get(), "sink opened before cleanup");
                BrokenWriter {
                    cleanup_finished: &cleaned,
                }
            },
        )
    }));
    assert!(cleaned.get(), "cleanup did not run");
    let original = result.expect_err("original failure must be resumed");
    assert_eq!(original.downcast_ref::<u32>(), Some(&42));
}

#[test]
fn locked_stderr_does_not_precede_console_cleanup() {
    let (locked_tx, locked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let holder = thread::spawn(move || {
        let _lock = std::io::stderr().lock();
        locked_tx.send(()).expect("stderr lock acquired");
        release_rx.recv().expect("release stderr lock");
    });
    locked_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("stderr lock holder ready");
    let (cleaned_tx, cleaned_rx) = mpsc::channel();
    let finisher = thread::spawn(move || {
        std::panic::catch_unwind(|| {
            super::finish_beacon_failure(
                Box::new(42_u32),
                Some(BeaconFailureObservation::Failed),
                || cleaned_tx.send(()).expect("cleanup reached"),
                || std::io::stderr().lock(),
            )
        })
    });
    let cleanup = cleaned_rx.recv_timeout(Duration::from_secs(1));
    release_tx.send(()).expect("release stderr holder");
    holder.join().expect("stderr holder exited");
    let failure = finisher
        .join()
        .expect("finisher exited")
        .expect_err("panic resumed");
    cleanup.expect("cleanup blocked behind stderr lock");
    assert_eq!(failure.downcast_ref::<u32>(), Some(&42));
}
