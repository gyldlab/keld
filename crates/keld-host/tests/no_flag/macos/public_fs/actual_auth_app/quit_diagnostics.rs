//! Source-owned diagnostics for an uncompleted public auth Quit on the actual shipping CLI.
use super::{Case, Observation};
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
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
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_child_output(cli_child, EVENT_DEADLINE)
    })) {
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
    use super::cli_tail_contains_marker;

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
