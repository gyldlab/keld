//! Source-owned diagnostics for an uncompleted public auth Quit on the actual shipping CLI.
use super::{Case, Observation};
use crate::support::EVENT_DEADLINE;
use crate::support::control::wait_child_output;
use crate::support::process::process_exists;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::Path;
use std::process::{Child, Output};
use std::sync::atomic::Ordering;

/// Read at most 64 KiB of the redirected CLI output. The bytes are untrusted
/// and must never be printed or persisted in a diagnostic summary.
fn cli_tail(path: &Path) -> Option<Vec<u8>> {
    let Ok(mut file) = fs::File::open(path) else {
        return None;
    };
    let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
        return None;
    };
    if file
        .seek(SeekFrom::Start(length.saturating_sub(64 * 1024)))
        .is_err()
    {
        return None;
    }
    let mut tail = Vec::new();
    if file.take(64 * 1024).read_to_end(&mut tail).is_err() {
        return None;
    }
    Some(tail)
}

/// Look for one exact, nonsecret marker in a bounded tail of the actual CLI
/// redirect. Never print the file's untrusted paths, credentials, or log text.
fn cli_tail_contains_marker(path: &Path, marker: &[u8]) -> bool {
    let Some(tail) = cli_tail(path) else {
        return false;
    };
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

/// Return only syntactically complete, recognized diagnostic codes, never
/// adjacent untrusted error messages, paths, tokens or application output.
/// These are diagnostic *observations*, not authenticated product receipts.
fn cli_tail_known_codes(path: &Path) -> Vec<String> {
    let Some(tail) = cli_tail(path) else {
        return Vec::new();
    };
    let mut codes = BTreeSet::new();
    for prefix in [
        b"KELD-CLI-".as_slice(),
        b"KELD-CORE-",
        b"KELD-IPC-",
        b"KELD-RUNTIME-",
        b"KELD-AUTH-",
        b"KELD-GUARD-",
        b"KELD-GUARD",
        b"KELD-COMPAT-",
        b"KELD-WV-",
    ] {
        for (index, window) in tail.windows(prefix.len() + 3).enumerate() {
            if !window.starts_with(prefix)
                || !window[prefix.len()..].iter().all(u8::is_ascii_digit)
                || tail
                    .get(index + prefix.len() + 3)
                    .is_some_and(|next| next.is_ascii_alphanumeric() || *next == b'_')
            {
                continue;
            }
            // Only ASCII fixed-shape identifiers are reconstructed, never
            // log text supplied by the child or the provider.
            if let Ok(code) = std::str::from_utf8(window) {
                codes.insert(code.to_owned());
            }
        }
    }
    codes.into_iter().take(12).collect()
}

/// This observation is after the CLI wait/reap. Never infer a deadline-time
/// process state or that a sampled PID retained its birth from this output.
fn emit_quit_boundary(
    project: &Path,
    case: &Case,
    observation: &Observation,
    family: [u32; 4],
    phase: &'static str,
    cli_exit_code: Option<i32>,
) {
    let marker = b"KELD_KEL89_PUBLIC_QUIT_REQUEST";
    let marker_seen = cli_tail_contains_marker(&project.join("cli.stderr"), marker)
        || cli_tail_contains_marker(&project.join("cli.stdout"), marker);
    let codes = cli_tail_known_codes(&project.join("cli.stderr"));
    let _ = writeln!(
        std::io::stderr().lock(),
        "KELD_KEL89_QUIT_BOUNDARY case={} phase={} cli_exit_code={:?} http_quit_stage={} cli_quit_marker_seen={} cli_stderr_known_codes={:?} post_cli_cleanup_host_pid_visible={} post_cli_cleanup_guardian_pid_visible={} post_cli_cleanup_bun_pid_visible={} post_cli_cleanup_descendant_pid_visible={}",
        case.name,
        phase,
        cli_exit_code,
        observation.quit_progress.load(Ordering::Acquire),
        marker_seen,
        codes,
        process_exists(family[0]),
        process_exists(family[1]),
        process_exists(family[2]),
        process_exists(family[3]),
    );
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
        Ok(output) => {
            if !output.status.success() {
                emit_quit_boundary(
                    project,
                    case,
                    observation,
                    family,
                    "cli-nonzero",
                    output.status.code(),
                );
            }
            output
        }
        Err(original) => {
            // CLI stdout/stderr were redirected to project files. Thus its
            // captured stderr_bytes=0 is not a Bun/Host logging observation.
            // Never echo any contents, private paths, endpoints or tokens.
            emit_quit_boundary(project, case, observation, family, "cli-timeout", None);
            std::panic::resume_unwind(original);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{cli_tail_contains_marker, cli_tail_known_codes};

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

    #[test]
    fn bounded_cli_codes_exclude_untrusted_messages_and_partial_identifiers() {
        let temp = tempfile::tempdir().expect("private bounded error code test");
        let log = temp.path().join("cli.stderr");
        std::fs::write(
            &log,
            b"UNTRUSTED_ACCOUNT_TOKEN=/private/secret\nKELD-CORE-033: private\nKELD-IPC-022 unknown\nKELD-CORE-033X\nKELD-CLI-031_UNTRUSTED\n",
        )
        .expect("fixture-only private log");
        assert_eq!(
            cli_tail_known_codes(&log),
            ["KELD-CORE-033".to_owned(), "KELD-IPC-022".to_owned()]
        );
    }
}
