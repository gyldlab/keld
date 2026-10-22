//! Fixed, nonsecret native evidence for an incomplete real-pointer public Quit.
use super::observer::Observation;
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output_observing;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::process::{Child, Output};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// The observer's independent release makes `/quit` eligible; it cannot prove
/// the real Bun request was received. Snapshot its existing 0/1/2 receipt while
/// the host remains live, before the wait owner's forced-child cleanup on panic.
/// The original native host exit/output deadline and panic remain unchanged.
pub(super) fn wait(
    host: Child,
    observation: &Observation,
    launch: usize,
    independent_release_sent: bool,
) -> Output {
    wait_with_deadline(
        host,
        observation,
        launch,
        independent_release_sent,
        EVENT_DEADLINE,
    )
}

fn wait_with_deadline(
    host: Child,
    observation: &Observation,
    launch: usize,
    independent_release_sent: bool,
    deadline: Duration,
) -> Output {
    let mut stage_while_host_live = observation.quit_progress.load(Ordering::Acquire);
    match catch_unwind(AssertUnwindSafe(|| {
        wait_child_output_observing(host, deadline, || {
            stage_while_host_live = observation.quit_progress.load(Ordering::Acquire);
        })
    })) {
        Ok(output) => output,
        Err(original) => {
            // 0 = no completed HTTP /quit routed; 1 = routed but not replied;
            // 2 = observer write returned, not proof the Bun client read it.
            // Never emit HTTP bodies, paths, tokens, or raw host stderr.
            eprintln!(
                "KELD_KEL140_PUBLIC_QUIT_PHASE launch={launch} independent_release_sent={independent_release_sent} observer_stage_while_host_live={stage_while_host_live}"
            );
            resume_unwind(original);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Observation, wait_with_deadline};
    use crate::support::EVENT_DEADLINE;
    use crate::support::control::wait_child_output;
    use crate::support::process::process_exists;
    use std::io::{Read as _, Write as _};
    use std::net::TcpStream;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::process::{Command, Stdio};
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    const CHILD_ROLE: &str = "KELD_KEL140_DIAGNOSTIC_CHILD";
    const CHILD_STAGE: &str = "KELD_KEL140_DIAGNOSTIC_STAGE";
    const CHILD_SELECTOR: &str =
        "public_fs::quit_diagnostics::tests::native_parked_public_quit_child";

    /// An actual child test process owns the parked native child and TCP peer;
    /// its stdout/stderr bytes are independent of this test's expectations.
    #[test]
    fn native_parked_quit_reports_stage_before_owned_child_reap() {
        for stage in 0..=2 {
            let child = Command::new(std::env::current_exe().expect("running native test binary"))
                .args(["--ignored", "--exact", CHILD_SELECTOR, "--nocapture"])
                .env(CHILD_ROLE, "1")
                .env(CHILD_STAGE, stage.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("launch isolated native diagnostic child role");
            let output = wait_child_output(child, EVENT_DEADLINE);
            assert!(
                output.status.success(),
                "native diagnostic child failed at {stage}"
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            let name = "KELD_KEL140_PUBLIC_QUIT_PHASE";
            assert_eq!(
                stderr.matches(name).count(),
                1,
                "one phase witness at {stage}"
            );
            let expected = format!(
                "KELD_KEL140_PUBLIC_QUIT_PHASE launch=7 independent_release_sent={} observer_stage_while_host_live={stage}",
                stage == 2
            );
            assert!(
                stderr.contains(&expected),
                "the live observer receipt was lost or reported after cleanup at {stage}"
            );
        }
    }

    /// Only the parent above invokes this ignored real-process role. Its real
    /// complete GET remains held by the observer; no wall-clock sleep is used.
    #[test]
    #[ignore = "test-owned native subprocess invoked by the phase witness parent"]
    fn native_parked_public_quit_child() {
        assert_eq!(std::env::var(CHILD_ROLE).as_deref(), Ok("1"));
        let stage: u8 = std::env::var(CHILD_STAGE)
            .expect("expected child stage")
            .parse()
            .expect("bounded stage number");
        assert!(stage <= 2, "native observer has only three stages");
        let observer = Observation::bind();
        let mut client = if stage == 0 {
            None
        } else {
            let mut peer = TcpStream::connect(("127.0.0.1", observer.port))
                .expect("real public Quit HTTP client");
            peer.write_all(b"GET /quit HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .expect("complete independent HTTP Quit request");
            Some(peer)
        };
        if stage == 2 {
            observer
                .quit
                .send(())
                .expect("release test-owned public Quit");
            let peer = client.as_mut().expect("released Quit client");
            peer.set_read_timeout(Some(EVENT_DEADLINE))
                .expect("bounded native response");
            let mut response = Vec::new();
            peer.read_to_end(&mut response)
                .expect("actual native Quit response");
            assert!(response.ends_with(b"\r\n\r\nquit"));
        }
        let cutoff = Instant::now() + EVENT_DEADLINE;
        while observer.quit_progress.load(Ordering::Acquire) != stage {
            assert!(
                Instant::now() < cutoff,
                "native observer never accepted Quit"
            );
            std::thread::yield_now();
        }

        // The held stdin pipe parks a real child in a blocking read. Child
        // ownership is handed to ReapOnDrop in the production wait helper.
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("native parked child");
        let pid = child.id();
        let writer = child.stdin.take().expect("hold blocking native read");
        let original = catch_unwind(AssertUnwindSafe(|| {
            wait_with_deadline(child, &observer, 7, stage == 2, Duration::from_millis(100))
        }));
        assert!(
            original.is_err(),
            "the original child deadline must remain fatal"
        );
        assert_eq!(observer.quit_progress.load(Ordering::Acquire), stage);
        assert!(
            !process_exists(pid),
            "the owned child was not reaped on panic"
        );
        drop(writer);
        drop(client);
    }
}
