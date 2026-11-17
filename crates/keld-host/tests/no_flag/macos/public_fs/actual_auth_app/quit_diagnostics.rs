//! Source-owned diagnostics for an uncompleted public auth Quit on the actual shipping CLI.
use super::{Case, Observation};
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output_observing;
use crate::support::process::{process_exists, process_exists_native};
use std::fs;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::Path;
use std::process::{Child, Output};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LiveQuitSnapshot {
    stage: u8,
    // Read-only test observations of the originally admitted process family,
    // sampled while the exact CLI Child is still owned/live. No PID is emitted.
    family_visible: [bool; 4],
}

/// Look for one exact, nonsecret marker in a bounded tail of the actual CLI
/// redirect. Never print the file's untrusted paths, credentials, or log text.
fn cli_tail_contains_marker(path: &Path, marker: &[u8]) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
        return false;
    };
    if file
        .seek(SeekFrom::Start(length.saturating_sub(64 * 1024)))
        .is_err()
    {
        return false;
    }
    let mut tail = Vec::new();
    if file.take(64 * 1024).read_to_end(&mut tail).is_err() {
        return false;
    }
    // The real line can appear in a quoted wrapper, but a longer name such
    // as KELD_KEL89_PUBLIC_QUIT_REQUEST_FORGED is not the same event. Count
    // only an independent token, never an attacker-controlled prefix match.
    let boundary = |byte: u8| byte.is_ascii_whitespace() || matches!(byte, b'"' | b'\'');
    tail.windows(marker.len()).enumerate().any(|(at, token)| {
        token == marker
            && (at == 0 || boundary(tail[at - 1]))
            && tail
                .get(at + marker.len())
                .is_none_or(|byte| boundary(*byte))
    })
}

fn marker_seen(project: &Path, marker: &[u8]) -> bool {
    cli_tail_contains_marker(&project.join("cli.stderr"), marker)
        || cli_tail_contains_marker(&project.join("cli.stdout"), marker)
}

/// Bind each static checkpoint to the expected, locally selected fixture case.
/// One prior case's output must not count as this case's correlated Quit.
fn marker_seen_for_case(project: &Path, marker: &[u8], case: &str) -> bool {
    let mut qualified = Vec::with_capacity(marker.len() + 1 + case.len());
    qualified.extend_from_slice(marker);
    qualified.push(b' ');
    qualified.extend_from_slice(case.as_bytes());
    marker_seen(project, &qualified)
}

/// A completed real shipping Quit must prove that the additional diagnostic
/// hooks actually reached the existing CLI output owner. Otherwise a later
/// timeout's missing hook would not distinguish Bun progress from log loss.
pub(super) fn completed_quit_checkpoints_visible(project: &Path, case: &str) -> bool {
    [
        b"KELD_KEL89_QUIT_HTTP_HEADERS".as_slice(),
        b"KELD_KEL89_QUIT_HTTP_BODY".as_slice(),
        b"KELD_KEL89_PUBLIC_QUIT_REQUEST".as_slice(),
        b"KELD_KEL89_QUIT_API_ACK".as_slice(),
    ]
    .into_iter()
    .all(|marker| marker_seen_for_case(project, marker, case))
}

/// Preserve the fatal child deadline while providing only fixed-name observer
/// receipts on a timeout. PID visibility is sampled AFTER the child wait's
/// forced CLI reap and is neither the deadline snapshot nor proof against a
/// zombie or recycled PID.
pub(super) fn wait_for_auth_cli_quit(
    cli_child: Child,
    observation: &Observation,
    project: &Path,
    case: &Case,
    family: [u32; 4],
) -> Output {
    wait_for_auth_cli_quit_with_deadline(
        cli_child,
        observation,
        project,
        case,
        family,
        EVENT_DEADLINE,
        |_| {},
    )
}

fn wait_for_auth_cli_quit_with_deadline(
    cli_child: Child,
    observation: &Observation,
    project: &Path,
    case: &Case,
    family: [u32; 4],
    timeout: Duration,
    mut observe_native: impl FnMut(LiveQuitSnapshot),
) -> Output {
    let began = Instant::now();
    let mut stage_while_cli_live = observation.quit_progress.load(Ordering::Acquire);
    let mut pre_cleanup = None;
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_child_output_observing(cli_child, timeout, || {
            stage_while_cli_live = observation.quit_progress.load(Ordering::Acquire);
            // At most one read-only in-process kernel census before the same
            // original fatal deadline and ReapOnDrop. Never spawn external
            // `/bin/kill -0` children inside this late observation callback:
            // subprocess waits have no deadline and can starve CLI cleanup.
            if pre_cleanup.is_none()
                && began.elapsed() >= timeout.saturating_sub(Duration::from_millis(200))
                && began.elapsed() < timeout
            {
                let snapshot = LiveQuitSnapshot {
                    stage: stage_while_cli_live,
                    family_visible: family.map(process_exists_native),
                };
                observe_native(snapshot);
                pre_cleanup = Some(snapshot);
            }
        })
    })) {
        Ok(output) => output,
        Err(original) => {
            // CLI stdout/stderr were redirected to project files. Thus its
            // captured stderr_bytes=0 is not a Bun/Host logging observation.
            // Never echo any contents, private paths, endpoints or tokens.
            let cli_quit_marker_seen =
                marker_seen_for_case(project, b"KELD_KEL89_PUBLIC_QUIT_REQUEST", case.name);
            let _ = writeln!(
                std::io::stderr().lock(),
                "KELD_KEL89_QUIT_BOUNDARY case={} http_quit_stage={} http_quit_stage_while_cli_live={} pre_cleanup_family_visible={:?} http_headers_seen={} http_body_seen={} http_failure_seen={} cli_quit_marker_seen={} app_quit_ack_seen={} post_cli_cleanup_host_pid_visible={} post_cli_cleanup_guardian_pid_visible={} post_cli_cleanup_bun_pid_visible={} post_cli_cleanup_descendant_pid_visible={}",
                case.name,
                observation.quit_progress.load(Ordering::Acquire),
                stage_while_cli_live,
                pre_cleanup.map(|snapshot| snapshot.family_visible),
                marker_seen_for_case(project, b"KELD_KEL89_QUIT_HTTP_HEADERS", case.name),
                marker_seen_for_case(project, b"KELD_KEL89_QUIT_HTTP_BODY", case.name),
                marker_seen_for_case(project, b"KELD_KEL89_QUIT_HTTP_FAILURE", case.name),
                cli_quit_marker_seen,
                marker_seen_for_case(project, b"KELD_KEL89_QUIT_API_ACK", case.name),
                process_exists(family[0]),
                process_exists(family[1]),
                process_exists(family[2]),
                process_exists(family[3]),
            );
            std::panic::resume_unwind(original);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;
    use std::os::unix::process::ExitStatusExt as _;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::process::{Command, Stdio};

    /// A real, individually owned process. The test owns each Child through
    /// unwinding, so no read-only PID census can become cleanup authority.
    struct OwnedCat(Child);

    impl OwnedCat {
        fn spawn() -> Self {
            Self(
                Command::new("/bin/cat")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("test-owned parked native child"),
            )
        }
    }

    impl Drop for OwnedCat {
        fn drop(&mut self) {
            if matches!(self.0.try_wait(), Ok(None)) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    #[test]
    fn bounded_cli_diagnostic_detects_only_literal_quit_marker() {
        let temp = tempfile::tempdir().expect("private redacted CLI marker fixture");
        let log = temp.path().join("cli.stderr");
        let mut bytes = vec![b'x'; 64 * 1024 + 128];
        bytes.extend_from_slice(
            b"\nUNTRUSTED_ACCOUNT_TOKEN=/private/secret\nKELD_KEL89_PUBLIC_QUIT_REQUEST_FORGED\n",
        );
        std::fs::write(&log, &bytes).expect("large private stderr with false prefix");
        assert!(!cli_tail_contains_marker(
            &log,
            b"KELD_KEL89_PUBLIC_QUIT_REQUEST"
        ));
        bytes.extend_from_slice(b"KELD_KEL89_PUBLIC_QUIT_REQUEST missing-client\n");
        std::fs::write(&log, &bytes).expect("complete exact marker at EOF");
        assert!(cli_tail_contains_marker(
            &log,
            b"KELD_KEL89_PUBLIC_QUIT_REQUEST"
        ));
        assert!(!cli_tail_contains_marker(
            &log,
            b"KELD_KEL89_PUBLIC_QUIT_REQUEST_NEVER_EMITTED"
        ));
        bytes.extend_from_slice(b"KELD_KEL89_QUIT_HTTP_HEADERS_FORGED\n");
        std::fs::write(&log, &bytes).expect("private diagnostic prefix control");
        assert!(!cli_tail_contains_marker(
            &log,
            b"KELD_KEL89_QUIT_HTTP_HEADERS"
        ));
        bytes.extend_from_slice(b"KELD_KEL89_QUIT_HTTP_HEADERS unknown-token\n");
        bytes.extend_from_slice(b"KELD_KEL89_QUIT_HTTP_BODY unknown-token\n");
        bytes.extend_from_slice(b"KELD_KEL89_QUIT_API_ACK unknown-token\n");
        bytes.extend_from_slice(b"KELD_KEL89_PUBLIC_QUIT_REQUEST unknown-token\n");
        std::fs::write(&log, &bytes).expect("fixed positive phase markers");
        for marker in [
            b"KELD_KEL89_QUIT_HTTP_HEADERS".as_slice(),
            b"KELD_KEL89_QUIT_HTTP_BODY".as_slice(),
            b"KELD_KEL89_QUIT_API_ACK".as_slice(),
        ] {
            assert!(cli_tail_contains_marker(&log, marker));
        }
        assert!(completed_quit_checkpoints_visible(
            temp.path(),
            "unknown-token"
        ));
        assert!(
            !completed_quit_checkpoints_visible(temp.path(), "missing-client"),
            "other-case marker suffix must not forge this case's Quit completion"
        );
    }

    fn assert_real_native_quit_response(observer: &Observation) {
        let mut peer = TcpStream::connect(("127.0.0.1", observer.port))
            .expect("own real native quit HTTP client");
        peer.set_read_timeout(Some(Duration::from_secs(2)))
            .expect("bound test client read");
        peer.write_all(b"GET /quit HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("send completed native quit request");
        observer.quit.send(()).expect("release real quit response");
        let mut response = Vec::new();
        peer.read_to_end(&mut response)
            .expect("read actual observer response");
        assert!(response.ends_with(b"\r\n\r\nquit"));
        assert_eq!(observer.quit_progress.load(Ordering::Acquire), 2);
    }

    #[test]
    fn timeout_captures_live_http_reply_and_os_family_before_cli_reap() {
        let observer = Observation::bind();
        assert_real_native_quit_response(&observer);

        // Four genuinely different family members; one has already exited
        // and been reaped before sampling. A bogus [family[0]; 4] probe must
        // fail this test even if its overall HTTP Quit witness remains green.
        let live_host = OwnedCat::spawn();
        let mut exited_guardian = Command::new("/usr/bin/true")
            .spawn()
            .expect("test-owned exited Guardian substitute");
        let exited_pid = exited_guardian.id();
        assert!(
            exited_guardian
                .wait()
                .expect("reap exited native role")
                .success()
        );
        let live_bun = OwnedCat::spawn();
        let live_descendant = OwnedCat::spawn();
        let family = [
            live_host.0.id(),
            exited_pid,
            live_bun.0.id(),
            live_descendant.0.id(),
        ];
        assert_eq!(
            family
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4
        );
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own the exact native CLI timeout child");
        let stdin = child.stdin.take().expect("retain native child stdin");
        let pid = child.id();
        assert!(!family.contains(&pid));
        let root = tempfile::tempdir().expect("owned private diagnostic fixture");
        let mut live = None;
        let wait_began = Instant::now();
        let timeout = Duration::from_millis(650);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            wait_for_auth_cli_quit_with_deadline(
                child,
                &observer,
                root.path(),
                &super::super::CASES[3],
                family,
                timeout,
                |snapshot| live = Some((snapshot, wait_began.elapsed())),
            )
        }));
        drop(stdin);
        assert!(outcome.is_err(), "original fatal timeout must propagate");
        let (snapshot, when) = live.expect("one native phase census before owned CLI reap");
        assert_eq!(
            snapshot,
            LiveQuitSnapshot {
                stage: 2,
                family_visible: [true, false, true, true],
            },
            "four distinct native family members must have different states"
        );
        assert!(
            when >= timeout.saturating_sub(Duration::from_millis(200)),
            "the phase census ran too early to falsify last-200ms behavior"
        );
        assert!(
            !process_exists(pid),
            "the original child must be killed/reaped on timeout, independent of diagnostic"
        );
        for owned in [&live_host, &live_bun, &live_descendant] {
            assert!(
                process_exists(owned.0.id()),
                "read-only probes must not kill family members"
            );
        }
        // The test retains each exact Child, observes its final signal status
        // and reaps it. The Owner's Drop is an independent panic-path rescue.
        for mut owned in [live_host, live_bun, live_descendant] {
            let original_pid = owned.0.id();
            owned
                .0
                .kill()
                .expect("terminate only the exact test-owned Child");
            let exit = owned.0.wait().expect("reap the exact test-owned Child");
            assert_eq!(
                exit.signal(),
                Some(9),
                "native child was reaped after SIGKILL"
            );
            assert!(
                !process_exists_native(original_pid),
                "reaped test-owned native role remained visible"
            );
        }
    }
}
