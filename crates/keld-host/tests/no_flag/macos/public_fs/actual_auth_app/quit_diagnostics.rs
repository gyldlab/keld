//! Source-owned diagnostics for an uncompleted public auth Quit on the actual shipping CLI.
use super::{Case, Observation};
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output_observing;
use crate::support::process::process_exists_native;
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

/// Test-owned native observations and a deterministic post-kernel-probe seam.
struct QuitProbeHooks<OnSample, AfterProbe> {
    on_sample: OnSample,
    after_probe: AfterProbe,
}

/// Match one complete `console.error("<checkpoint> <case>")` line from the
/// test-owned Bun fixture in a bounded CLI redirect. A quoted log message or
/// partial prefix is not the emitted checkpoint; do not echo private log bytes.
fn cli_tail_contains_marker(path: &Path, marker: &[u8]) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
        return false;
    };
    let start = length.saturating_sub(64 * 1024);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return false;
    }
    let mut tail = Vec::new();
    if file.take(64 * 1024).read_to_end(&mut tail).is_err() {
        return false;
    }
    // The first record may be truncated when the file exceeds its tail limit.
    // Refuse it rather than treating a partial line as an independent emission.
    let tail = if start == 0 {
        tail.as_slice()
    } else if let Some(end) = tail.iter().position(|&byte| byte == b'\n') {
        &tail[end + 1..]
    } else {
        return false;
    };
    tail.split_inclusive(|&byte| byte == b'\n').any(|line| {
        line.strip_suffix(b"\n")
            .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
            == Some(marker)
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
        QuitProbeHooks {
            on_sample: |_| {},
            after_probe: |_| {},
        },
    )
}

fn wait_for_auth_cli_quit_with_deadline(
    cli_child: Child,
    observation: &Observation,
    project: &Path,
    case: &Case,
    family: [u32; 4],
    timeout: Duration,
    mut hooks: QuitProbeHooks<impl FnMut(LiveQuitSnapshot), impl FnMut(Instant)>,
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
                // Test-only scheduling seam after the *real* kernel probes.
                // It can deterministically expose a missing post-probe fence.
                (hooks.after_probe)(began);
                // Four kernel probes and a possible scheduling pause consumed
                // clock time. An expired sample must not become a predeadline
                // witness even though the exact CLI owner is still retained.
                if began.elapsed() < timeout {
                    (hooks.on_sample)(snapshot);
                    if began.elapsed() < timeout {
                        pre_cleanup = Some(snapshot);
                    }
                }
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
                process_exists_native(family[0]),
                process_exists_native(family[1]),
                process_exists_native(family[2]),
                process_exists_native(family[3]),
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

    /// Retain each real native Child across assertions and unwind until the
    /// exact CLI Child is handed to the waiter's `ReapOnDrop` owner.
    struct OwnedChild(Option<Child>);

    impl OwnedChild {
        fn new(child: Child) -> Self {
            Self(Some(child))
        }

        fn parked_cat() -> Self {
            Self::new(
                Command::new("/bin/cat")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("test-owned parked native child"),
            )
        }

        fn child(&self) -> &Child {
            self.0.as_ref().expect("retained native Child")
        }

        fn child_mut(&mut self) -> &mut Child {
            self.0.as_mut().expect("retained native Child")
        }

        fn into_wait_owner(mut self) -> Child {
            self.0
                .take()
                .expect("transfer retained Child to ReapOnDrop")
        }
    }

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let Some(child) = self.0.as_mut() else {
                return;
            };
            // /bin/cat owns no inherited writer. EOF is an independent exit
            // route if killing this exact Child is denied during panic cleanup.
            drop(child.stdin.take());
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }

    #[test]
    fn parked_native_child_is_reaped_during_assertion_unwind() {
        let mut pid = None;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let child = OwnedChild::parked_cat();
            pid = Some(child.child().id());
            panic!("exercise panic rescue while the test-owned native Child is live");
        }));
        assert!(result.is_err(), "test must actually exercise panic cleanup");
        assert!(
            !process_exists_native(pid.expect("test-owned native Child existed")),
            "the exact Child must be reaped before the unwind returns"
        );
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
            b"KELD_KEL89_PUBLIC_QUIT_REQUEST missing-client"
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
            b"KELD_KEL89_QUIT_HTTP_HEADERS unknown-token".as_slice(),
            b"KELD_KEL89_QUIT_HTTP_BODY unknown-token".as_slice(),
            b"KELD_KEL89_QUIT_API_ACK unknown-token".as_slice(),
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

    #[test]
    fn quoted_same_case_four_markers_cannot_certify_bun_quit() {
        let root = tempfile::tempdir().expect("quoted marker fixture");
        let mut stderr =
            fs::File::create(root.path().join("cli.stderr")).expect("test-owned redirected stderr");
        for marker in [
            "KELD_KEL89_QUIT_HTTP_HEADERS",
            "KELD_KEL89_QUIT_HTTP_BODY",
            "KELD_KEL89_PUBLIC_QUIT_REQUEST",
            "KELD_KEL89_QUIT_API_ACK",
        ] {
            writeln!(stderr, "\"{marker} unknown-token\"").expect("write untrusted quoted event");
        }
        drop(stderr);
        assert!(
            !completed_quit_checkpoints_visible(root.path(), "unknown-token"),
            "quoted untrusted same-case names must not pass as raw Bun console.error lines"
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
        let live_host = OwnedChild::parked_cat();
        let mut exited_guardian = OwnedChild::new(
            Command::new("/usr/bin/true")
                .spawn()
                .expect("test-owned exited Guardian substitute"),
        );
        let exited_pid = exited_guardian.child().id();
        assert!(
            exited_guardian
                .child_mut()
                .wait()
                .expect("reap exited native role")
                .success()
        );
        let live_bun = OwnedChild::parked_cat();
        let live_descendant = OwnedChild::parked_cat();
        let family = [
            live_host.child().id(),
            exited_pid,
            live_bun.child().id(),
            live_descendant.child().id(),
        ];
        assert_eq!(
            family
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4
        );
        let mut child = OwnedChild::parked_cat();
        let stdin = child
            .child_mut()
            .stdin
            .take()
            .expect("retain native child stdin");
        let pid = child.child().id();
        assert!(!family.contains(&pid));
        let root = tempfile::tempdir().expect("owned private diagnostic fixture");
        let mut live = None;
        let wait_began = Instant::now();
        let timeout = Duration::from_millis(650);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            wait_for_auth_cli_quit_with_deadline(
                child.into_wait_owner(),
                &observer,
                root.path(),
                &super::super::CASES[3],
                family,
                timeout,
                QuitProbeHooks {
                    on_sample: |snapshot| live = Some((snapshot, wait_began.elapsed())),
                    after_probe: |_| {},
                },
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
            when < timeout,
            "a native snapshot reported after the original deadline is not pre-cleanup evidence"
        );
        assert!(
            !process_exists_native(pid),
            "the original child must be killed/reaped on timeout, independent of diagnostic"
        );
        for owned in [&live_host, &live_bun, &live_descendant] {
            assert!(
                process_exists_native(owned.child().id()),
                "read-only probes must not kill family members"
            );
        }
        // The test retains each exact Child, observes its final signal status
        // and reaps it. The Owner's Drop is an independent panic-path rescue.
        for mut owned in [live_host, live_bun, live_descendant] {
            let original_pid = owned.child().id();
            owned.child_mut().kill().expect("kill exact native Child");
            let exit = owned.child_mut().wait().expect("reap exact native Child");
            assert_eq!(exit.signal(), Some(9), "native child reaped after SIGKILL");
            assert!(
                !process_exists_native(original_pid),
                "reaped test-owned native role remained visible"
            );
        }
    }

    #[test]
    fn late_native_census_is_discarded_before_cli_reap() {
        let observer = Observation::bind();
        let live_host = OwnedChild::parked_cat();
        let live_guardian = OwnedChild::parked_cat();
        let live_bun = OwnedChild::parked_cat();
        let live_descendant = OwnedChild::parked_cat();
        let family = [
            live_host.child().id(),
            live_guardian.child().id(),
            live_bun.child().id(),
            live_descendant.child().id(),
        ];
        let child = OwnedChild::parked_cat();
        let cli_pid = child.child().id();
        let root = tempfile::tempdir().expect("bounded native probe fixture");
        let timeout = Duration::from_millis(650);
        let mut reported = None;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            wait_for_auth_cli_quit_with_deadline(
                child.into_wait_owner(),
                &observer,
                root.path(),
                &super::super::CASES[3],
                family,
                timeout,
                QuitProbeHooks {
                    on_sample: |snapshot| reported = Some(snapshot),
                    after_probe: |wait_started: Instant| {
                        while wait_started.elapsed() < timeout {
                            std::thread::yield_now();
                        }
                    },
                },
            )
        }));
        assert!(
            outcome.is_err(),
            "original fatal CLI deadline must propagate"
        );
        assert!(
            reported.is_none(),
            "post-deadline native probes must never be reported as live pre-cleanup evidence"
        );
        assert!(
            !process_exists_native(cli_pid),
            "original CLI Child must be reaped even when native census finishes late"
        );
    }
}
