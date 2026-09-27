//! Renderer worker publication and join-order contracts.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::support::renderer::finish_renderer_beacon;

#[test]
fn renderer_beacon_prefers_worker_error_published_after_initial_receive_timeout() {
    let (observed_tx, observed) = mpsc::channel();
    let (publish_tx, publish_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        publish_rx
            .recv()
            .expect("release late renderer beacon publication");
        observed_tx
            .send(Err("precise worker failure".to_owned()))
            .expect("publish precise renderer beacon failure");
    });
    let initial_result = observed.recv_timeout(Duration::from_millis(1));
    assert_eq!(initial_result, Err(mpsc::RecvTimeoutError::Timeout));
    publish_tx
        .send(())
        .expect("release worker after initial receive timeout");

    let result = finish_renderer_beacon(&observed, worker, initial_result, "late publication");
    assert_eq!(
        result,
        Err("late publication: precise worker failure".to_owned())
    );
}

#[test]
fn renderer_beacon_rejects_worker_success_published_after_initial_receive_timeout() {
    let (observed_tx, observed) = mpsc::channel();
    let (publish_tx, publish_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        publish_rx
            .recv()
            .expect("release late renderer beacon success");
        observed_tx
            .send(Ok(()))
            .expect("publish late renderer beacon success");
    });
    let initial_result = observed.recv_timeout(Duration::from_millis(1));
    assert_eq!(initial_result, Err(mpsc::RecvTimeoutError::Timeout));
    publish_tx
        .send(())
        .expect("release success after initial receive timeout");

    let result = finish_renderer_beacon(&observed, worker, initial_result, "late success");
    assert_eq!(
        result,
        Err("late success: beacon worker did not report: timed out waiting on channel".to_owned())
    );
}

#[test]
fn renderer_beacon_preserves_initial_timeout_when_worker_publishes_nothing() {
    let (observed_tx, observed) = mpsc::channel::<Result<(), String>>();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        release_rx
            .recv()
            .expect("release renderer beacon worker without publication");
        drop(observed_tx);
    });
    let initial_result = observed.recv_timeout(Duration::from_millis(1));
    assert_eq!(initial_result, Err(mpsc::RecvTimeoutError::Timeout));
    release_tx
        .send(())
        .expect("release non-publishing worker after initial timeout");

    let result = finish_renderer_beacon(&observed, worker, initial_result, "no publication");
    assert_eq!(
        result,
        Err(
            "no publication: beacon worker did not report: timed out waiting on channel".to_owned()
        )
    );
}
