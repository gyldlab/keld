//! Native Unix cut clocks for the authenticated Guardian test milestone.
//! These are independent of synthetic event values: actual accepted peers,
//! actual bytes and the original monotonic clock determine the result.
use super::{MilestoneReadStage, receive_held_milestone_with_hook};
use std::io::Write as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn panic_message(result: Result<(UnixStream, u32), Box<dyn std::any::Any + Send>>) -> String {
    let error = result.expect_err("late native byte must fail closed");
    if let Some(message) = error.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    error
        .downcast_ref::<String>()
        .cloned()
        .expect("exact deadline panic must carry a static diagnostic")
}

fn owned_pair() -> (tempfile::TempDir, UnixListener, UnixStream) {
    let root = tempfile::tempdir().expect("own private native auth-cut directory");
    let socket = root.path().join("held-auth.sock");
    let listener = UnixListener::bind(&socket).expect("own actual native listener");
    listener
        .set_nonblocking(true)
        .expect("bound original native accept");
    let client = UnixStream::connect(&socket).expect("own genuine connected control peer");
    (root, listener, client)
}

#[test]
fn native_lf_after_timely_pre_read_cannot_complete_authentication_milestone() {
    let (_root, listener, mut client) = owned_pair();
    let prefix = format!("AUTHENTICATED {}", std::process::id());
    client
        .write_all(prefix.as_bytes())
        .expect("queue actual native prefix before final LF");
    let expected = prefix.len();
    let deadline = Instant::now() + Duration::from_millis(250);
    let (pre_tx, pre_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (native_tx, native_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut paused = false;
        catch_unwind(AssertUnwindSafe(|| {
            receive_held_milestone_with_hook(&listener, "AUTHENTICATED", deadline, |phase| {
                match phase {
                    MilestoneReadStage::BeforeNativeRead(n) if n == expected && !paused => {
                        paused = true;
                        let _ = pre_tx.send(Instant::now());
                        let _ = release_rx.recv_timeout(Duration::from_secs(1));
                    }
                    MilestoneReadStage::AfterNativeByte(b'\n') => {
                        let _ = native_tx.send(Instant::now());
                    }
                    _ => {}
                }
            })
        }))
    });

    let prechecked = pre_rx.recv_timeout(Duration::from_millis(200));
    while Instant::now() < deadline {
        thread::yield_now();
    }
    let sent = client.write_all(b"\n");
    let _ = release_tx.send(());
    let lf = native_rx.recv_timeout(Duration::from_secs(1));
    drop(client);
    let outcome = reader
        .join()
        .expect("join original native acceptance owner");

    assert!(sent.is_ok(), "real control peer must deliver late LF");
    assert!(
        prechecked.is_ok_and(|when| when < deadline),
        "entire real prefix and pre-read clock gate must precede original deadline"
    );
    assert!(
        lf.is_ok_and(|when| when >= deadline),
        "independent native-byte receipt must prove LF was read AFTER original deadline"
    );
    assert!(
        panic_message(outcome)
            .contains("authentication notice byte arrived after original deadline"),
        "only the post-native-byte deadline fence can classify this genuinely late LF"
    );
}

#[test]
fn real_native_accept_that_crosses_deadline_cannot_enter_accepted_stage() {
    let (_root, listener, mut client) = owned_pair();
    let marker = format!("PROBED {}\n", std::process::id());
    client
        .write_all(marker.as_bytes())
        .expect("queue complete native milestone peer");
    let deadline = Instant::now() + Duration::from_millis(250);
    let (accepted_tx, accepted_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (on_time_tx, on_time_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        catch_unwind(AssertUnwindSafe(|| {
            receive_held_milestone_with_hook(&listener, "PROBED", deadline, |phase| match phase {
                MilestoneReadStage::AfterNativeAccept => {
                    let _ = accepted_tx.send(Instant::now());
                    let _ = release_rx.recv_timeout(Duration::from_secs(1));
                }
                MilestoneReadStage::AcceptedWithinDeadline => {
                    let _ = on_time_tx.send(());
                }
                _ => {}
            })
        }))
    });
    let accepted = accepted_rx.recv_timeout(Duration::from_millis(200));
    while Instant::now() < deadline {
        thread::yield_now();
    }
    let _ = release_tx.send(());
    drop(client);
    let result = reader
        .join()
        .expect("join native accepted-connection owner");

    assert!(
        accepted.is_ok_and(|when| when < deadline),
        "accept must complete in kernel BEFORE test deliberately crosses the clock"
    );
    assert!(
        !matches!(on_time_rx.try_recv(), Ok(())),
        "an expired native accept cannot reach the accepted-within-budget phase"
    );
    assert!(
        result.is_err(),
        "late post-accept milestone must be refused"
    );
}

#[test]
fn valid_timely_native_milestone_preserves_original_frame_and_peer() {
    let (_root, listener, mut client) = owned_pair();
    let real_role = std::process::id();
    client
        .write_all(format!("AUTHENTICATED {real_role}\n").as_bytes())
        .expect("write complete native Guardian test-role receipt");
    let (accepted, role) = receive_held_milestone_with_hook(
        &listener,
        "AUTHENTICATED",
        Instant::now() + Duration::from_secs(1),
        |_| {},
    );
    assert_eq!(role, real_role);
    drop(accepted);
    drop(client);
}
