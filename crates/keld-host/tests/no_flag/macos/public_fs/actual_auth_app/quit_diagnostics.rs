//! Source-owned diagnostics for an uncompleted public auth Quit on the actual shipping CLI.
use super::{Case, Observation};
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output_observing;
use crate::support::process::process_exists;
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

/// A completed real shipping Quit must prove that the additional diagnostic
/// hooks actually reached the existing CLI output owner. Otherwise a later
/// timeout's missing hook would not distinguish Bun progress from log loss.
pub(super) fn completed_quit_checkpoints_visible(project: &Path) -> bool {
    [
        b"KELD_KEL89_QUIT_HTTP_HEADERS".as_slice(),
        b"KELD_KEL89_QUIT_HTTP_BODY".as_slice(),
        b"KELD_KEL89_PUBLIC_QUIT_REQUEST".as_slice(),
        b"KELD_KEL89_QUIT_API_ACK".as_slice(),
    ]
    .into_iter()
    .all(|marker| marker_seen(project, marker))
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
            // At most one census, late in the *original* wait, and before
            // ReapOnDrop can terminate the CLI. Sampling repeatedly would
            // launch many ps/kill probes and perturb the lifecycle being timed.
            if pre_cleanup.is_none()
                && began.elapsed() >= timeout.saturating_sub(Duration::from_millis(200))
            {
                let snapshot = LiveQuitSnapshot {
                    stage: stage_while_cli_live,
                    family_visible: family.map(process_exists),
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
            let cli_quit_marker_seen = marker_seen(project, b"KELD_KEL89_PUBLIC_QUIT_REQUEST");
            let _ = writeln!(
                std::io::stderr().lock(),
                "KELD_KEL89_QUIT_BOUNDARY case={} http_quit_stage={} http_quit_stage_while_cli_live={} pre_cleanup_family_visible={:?} http_headers_seen={} http_body_seen={} http_failure_seen={} cli_quit_marker_seen={} app_quit_ack_seen={} post_cli_cleanup_host_pid_visible={} post_cli_cleanup_guardian_pid_visible={} post_cli_cleanup_bun_pid_visible={} post_cli_cleanup_descendant_pid_visible={}",
                case.name,
                observation.quit_progress.load(Ordering::Acquire),
                stage_while_cli_live,
                pre_cleanup.map(|snapshot| snapshot.family_visible),
                marker_seen(project, b"KELD_KEL89_QUIT_HTTP_HEADERS"),
                marker_seen(project, b"KELD_KEL89_QUIT_HTTP_BODY"),
                marker_seen(project, b"KELD_KEL89_QUIT_HTTP_FAILURE"),
                cli_quit_marker_seen,
                marker_seen(project, b"KELD_KEL89_QUIT_API_ACK"),
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
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::process::{Command, Stdio};

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
        std::fs::write(&log, &bytes).expect("fixed positive phase markers");
        for marker in [
            b"KELD_KEL89_QUIT_HTTP_HEADERS".as_slice(),
            b"KELD_KEL89_QUIT_HTTP_BODY".as_slice(),
            b"KELD_KEL89_QUIT_API_ACK".as_slice(),
        ] {
            assert!(cli_tail_contains_marker(&log, marker));
        }
    }

    #[test]
    fn timeout_captures_live_http_reply_and_os_family_before_cli_reap() {
        let observer = Observation::bind();
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

        // The true retained stdin keeps /bin/cat alive until the exact test
        // helper owns/reaps it. Its numerical PID is only a read-only census.
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own native timeout child");
        let stdin = child.stdin.take().expect("retain native child stdin");
        let pid = child.id();
        let root = tempfile::tempdir().expect("owned private diagnostic fixture");
        let mut live = None;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            wait_for_auth_cli_quit_with_deadline(
                child,
                &observer,
                root.path(),
                &super::super::CASES[3],
                [pid; 4],
                Duration::from_millis(120),
                |snapshot| live = Some(snapshot),
            )
        }));
        drop(stdin);
        assert!(outcome.is_err(), "original fatal timeout must propagate");
        assert_eq!(
            live,
            Some(LiveQuitSnapshot {
                stage: 2,
                family_visible: [true; 4],
            }),
            "stage 2 and the real owned child must be observed BEFORE forced reap"
        );
        assert!(
            !process_exists(pid),
            "the original child must be killed/reaped on timeout, independent of diagnostic"
        );
    }
}
