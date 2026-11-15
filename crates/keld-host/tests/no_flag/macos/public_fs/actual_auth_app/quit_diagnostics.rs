//! Source-owned diagnostics for an uncompleted public auth Quit on the actual shipping CLI.
use super::{AuthReceipts, Case, Observation};
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output_observing;
use crate::support::process::ProcessExitObserver;
use crate::support::process::process_exists;
use std::fs;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::Path;
use std::process::{Child, Output};
use std::sync::atomic::Ordering;

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

// A prearmed kernel watcher observes only the exact originally registered
// process. Neither a missing event nor an error grants liveness or signal
// authority; all labels are fixed and contain no PIDs or private log bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExitPhase {
    NotSampled,
    NoEvent,
    Observed,
    Unproved,
}

impl ExitPhase {
    fn label(self) -> &'static str {
        match self {
            Self::NotSampled => "not_sampled",
            Self::NoEvent => "no_event",
            Self::Observed => "original_kernel_exit_observed",
            Self::Unproved => "kernel_read_unproved",
        }
    }
}

fn sample_original_kernel_exit(witness: &mut ProcessExitObserver) -> ExitPhase {
    match witness.try_poll_exit() {
        Ok(Some(_)) => ExitPhase::Observed,
        Ok(None) => ExitPhase::NoEvent,
        Err(_) => ExitPhase::Unproved,
    }
}

fn resample_original_kernel_exit(stage: ExitPhase) -> bool {
    matches!(stage, ExitPhase::NotSampled | ExitPhase::NoEvent)
}

fn safe_quit_http_label(stage: u8) -> &'static str {
    match stage {
        0 => "no_http_quit_observed",
        1 => "http_quit_received",
        2 => "observer_response_written",
        _ => "unproved_http_stage",
    }
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
    native_exits: &mut [ProcessExitObserver; 4],
    receipts: AuthReceipts<'_>,
) -> Output {
    let AuthReceipts {
        body: mut native_receipt,
        post_reply: mut post_reply_receipt,
    } = receipts;
    let mut last_live_http = 0_u8;
    let mut last_live_bun_body = "not_armed";
    let mut last_live_post_reply = "not_armed";
    let native_receipt_deadline = std::time::Instant::now() + EVENT_DEADLINE;
    let mut last_live_kernel = [ExitPhase::NotSampled; 4];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_child_output_observing(cli_child, EVENT_DEADLINE, || {
            // This callback runs while the exact CLI Child is still live and
            // before its original 20-second exit/output deadline is checked.
            last_live_http = observation.quit_progress.load(Ordering::Acquire);
            if let Some(receipt) = native_receipt.as_deref_mut() {
                receipt.poll(native_receipt_deadline);
                last_live_bun_body = receipt.stage();
            }
            if let Some(receipt) = post_reply_receipt.as_deref_mut() {
                receipt.poll(native_receipt_deadline);
                last_live_post_reply = match receipt.stage() {
                    "bun_body_verified" => "native_post_quit_ack_written",
                    "invalid_bun_receipt" => "invalid_post_quit_ack",
                    _ => "no_post_quit_ack",
                };
            }
            for (stage, witness) in last_live_kernel.iter_mut().zip(native_exits.iter_mut()) {
                // Refusal is sticky: a later empty read cannot erase a
                // failed native kernel observation. An observed original exit
                // is also retained without consuming a second event.
                if resample_original_kernel_exit(*stage) {
                    *stage = sample_original_kernel_exit(witness);
                }
            }
        })
    }));
    // Original CLI deadline/panic always wins: even failure to write this
    // optional redacted diagnostic must not replace the first source error.
    let _ = writeln!(
        std::io::stderr().lock(),
        "KELD_KEL89_ORIGINAL_KERNEL_QUIT_PHASE cli_outcome={} last_live_http_stage={} bun_body_stage={} bun_post_quit_stage={} host_exit={} guardian_exit={} bun_exit={} descendant_exit={}",
        if result.is_ok() {
            "cli_exited"
        } else {
            "cli_deadline_or_capture_failed"
        },
        safe_quit_http_label(last_live_http),
        last_live_bun_body,
        last_live_post_reply,
        last_live_kernel[0].label(),
        last_live_kernel[1].label(),
        last_live_kernel[2].label(),
        last_live_kernel[3].label(),
    );
    match result {
        Ok(output) => output,
        Err(original) => {
            // CLI stdout/stderr were redirected to project files. Thus its
            // captured stderr_bytes=0 is not a Bun/Host logging observation.
            // Never echo any contents, private paths, endpoints or tokens.
            let marker = b"KELD_KEL89_PUBLIC_QUIT_REQUEST";
            let cli_quit_marker_seen =
                cli_tail_contains_marker(&project.join("cli.stderr"), marker)
                    || cli_tail_contains_marker(&project.join("cli.stdout"), marker);
            let _ = writeln!(
                std::io::stderr().lock(),
                "KELD_KEL89_QUIT_BOUNDARY case={} http_quit_stage={} cli_quit_marker_seen={} post_cli_cleanup_host_pid_visible={} post_cli_cleanup_guardian_pid_visible={} post_cli_cleanup_bun_pid_visible={} post_cli_cleanup_descendant_pid_visible={}",
                case.name,
                observation.quit_progress.load(Ordering::Acquire),
                cli_quit_marker_seen,
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
    use super::{
        ExitPhase, cli_tail_contains_marker, resample_original_kernel_exit,
        sample_original_kernel_exit,
    };
    use crate::support::PROCESS_DEADLINE;
    use crate::support::process::ProcessExitObserver;
    use std::process::{Child, Command, Stdio};
    use std::time::Instant;

    struct OwnedNativeChild(Child);
    impl Drop for OwnedNativeChild {
        fn drop(&mut self) {
            if matches!(self.0.try_wait(), Ok(None)) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    #[test]
    fn native_observer_refusal_can_never_be_overwritten_by_later_no_event() {
        assert!(resample_original_kernel_exit(ExitPhase::NotSampled));
        assert!(resample_original_kernel_exit(ExitPhase::NoEvent));
        assert!(!resample_original_kernel_exit(ExitPhase::Unproved));
        assert!(!resample_original_kernel_exit(ExitPhase::Observed));
    }

    #[test]
    fn original_live_native_child_cannot_forge_kernel_exit_before_actual_status() {
        let mut child = OwnedNativeChild(
            Command::new("/bin/sh")
                .args(["-c", "IFS= read -r line; exit 23"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("owned native waiting shell"),
        );
        let mut witness = ProcessExitObserver::arm(child.0.id());
        assert!(
            child
                .0
                .try_wait()
                .expect("original child before release")
                .is_none(),
            "direct Child must remain live while its exact stdin owner is held"
        );
        assert_eq!(
            sample_original_kernel_exit(&mut witness),
            ExitPhase::NoEvent,
            "a live original Child cannot be reported as an observed kernel exit"
        );
        drop(child.0.stdin.take());
        let deadline = Instant::now() + PROCESS_DEADLINE;
        loop {
            match sample_original_kernel_exit(&mut witness) {
                ExitPhase::Observed => break,
                ExitPhase::NoEvent if Instant::now() < deadline => std::thread::yield_now(),
                _ => panic!("owned native process exit event missing before original deadline"),
            }
        }
        assert_eq!(
            child.0.wait().expect("reap exact owned shell").code(),
            Some(23)
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
            b"KELD_KEL89_PUBLIC_QUIT_REQUEST"
        ));
        assert!(!cli_tail_contains_marker(
            &log,
            b"KELD_KEL89_PUBLIC_QUIT_REQUEST_NEVER_EMITTED"
        ));
    }
}
