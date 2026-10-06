//! AC2: the writer's output from the real unsigned release host is loadable (§7 row 2).

use super::{
    TRUST_E_NOSIGNATURE, WIN_VERIFY_TRUST_REJECTED, env_path, fixture_payload, sha256_hex,
};
use keld_pack::embed_host_identity;
use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, Read as _, Write as _};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// How long the embedded host may run before the row kills it and fails.
const HOST_EXIT_DEADLINE: Duration = Duration::from_mins(1);
/// How long a killed child's pipes may take to drain before its output is given up.
const DRAIN_AFTER_KILL: Duration = Duration::from_secs(5);

/// A child that closed both of its pipes and was reaped.
#[derive(Debug)]
struct Exited {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// A child that still held a pipe open at the deadline; it was killed and reaped.
#[derive(Debug)]
struct Hung {
    kill: io::Result<()>,
    reaped: io::Result<ExitStatus>,
    /// What the child wrote before it was killed, when its pipes closed in time.
    partial: Option<(Vec<u8>, Vec<u8>)>,
}

impl fmt::Display for Hung {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "kill={:?} reaped={:?}", self.kill, self.reaped)?;
        match &self.partial {
            Some((stdout, stderr)) => write!(
                f,
                " stdout={:?} stderr={:?}",
                String::from_utf8_lossy(stdout),
                String::from_utf8_lossy(stderr)
            ),
            None => write!(f, " output=unavailable"),
        }
    }
}

/// Drains `child`'s piped stdout and stderr on helper threads, then reaps it; a child that
/// still holds either pipe at `deadline` is killed and reported as hung instead.
///
/// The deadline is a kill switch, not synchronization: the pipes closing is the observed
/// condition. A child that closed both pipes and then kept running would still block the
/// final `wait`; the release host never closes its stdio before exiting.
fn exit_within(mut child: Child, deadline: Duration) -> Result<Exited, Hung> {
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let stderr = thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).map(|_| bytes)
        });
        let mut bytes = Vec::new();
        let stdout = stdout.read_to_end(&mut bytes).map(|_| bytes);
        let stderr = stderr.join().expect("the stderr reader finished");
        // A refused send means the caller already gave up on this child.
        let _ = sender.send((stdout, stderr));
    });
    match receiver.recv_timeout(deadline) {
        Ok((stdout, stderr)) => {
            let stdout = stdout.expect("read the child's stdout");
            let stderr = stderr.expect("read the child's stderr");
            // Both pipes closed, so the child has exited or is exiting.
            let status = child.wait().expect("reap the exited child");
            Ok(Exited {
                status,
                stdout,
                stderr,
            })
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let kill = child.kill();
            let reaped = child.wait();
            // The kill closes the pipes, so the drained bytes arrive unless another
            // process still holds them.
            let partial = receiver
                .recv_timeout(DRAIN_AFTER_KILL)
                .ok()
                .and_then(|(stdout, stderr)| Some((stdout.ok()?, stderr.ok()?)));
            Err(Hung {
                kill,
                reaped,
                partial,
            })
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("the output reader thread ended without reporting")
        }
    }
}

/// Negative control for the deadline: a child that never closes its pipes is killed,
/// reaped and reported as hung, and its pipes then drain.
///
/// The outcome does not depend on how far `cmd` got before the deadline, so whether its
/// prompt was written yet is not asserted.
#[test]
fn deadline_kills_a_child_that_never_closes_its_pipes() {
    // `pause` blocks until stdin yields a byte; the pipe's write end stays open in `child`.
    let child = Command::new("cmd")
        .args(["/c", "pause"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cmd /c pause");
    let hung = exit_within(child, Duration::from_millis(500))
        .expect_err("a child blocked on stdin counted as exited");
    hung.kill.as_ref().expect("the hung child was killed");
    let status = hung.reaped.as_ref().expect("the killed child was reaped");
    assert!(!status.success(), "{status}");
    assert!(
        hung.partial.is_some(),
        "the killed child's pipes did not drain: {hung}"
    );
}

/// Launched with no dev lease, the embedded release host exits with the KEL-135 identity
/// refusal before any listener, child or window.
#[test]
#[ignore = "needs KELD_PACK_REAL_HOST (release keld-host.exe) and a new KELD_PACK_EMBEDDED_HOST path"]
fn ac2_embedded_release_host_launches_to_the_identity_refusal() {
    let input_path = env_path("KELD_PACK_REAL_HOST");
    let embedded_path = env_path("KELD_PACK_EMBEDDED_HOST");
    let host = std::fs::read(&input_path).expect("read the release host");
    let embedded =
        embed_host_identity(&host, &fixture_payload()).expect("the release host is admissible");
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&embedded_path)
            .expect("KELD_PACK_EMBEDDED_HOST must be a new file in an existing directory");
        file.write_all(&embedded).expect("write the embedded host");
        file.sync_all().expect("flush the embedded host");
    } // The writer handle is closed before launch, so nothing else holds the image.

    // A plain lease-less launch: no dev lease and no launcher start gate.
    let child = Command::new(&embedded_path)
        .current_dir(
            embedded_path
                .parent()
                .expect("the embedded host has a parent"),
        )
        .env_remove("KELD_DEV_LEASE")
        .env_remove("KELD_WINDOWS_LAUNCH_GATE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("CreateProcess must accept the embedded image; a loader rejection fails AC2");
    let output = match exit_within(child, HOST_EXIT_DEADLINE) {
        Ok(exited) => exited,
        Err(hung) => panic!(
            "AC2: the embedded host did not exit within {} s: {hung}",
            HOST_EXIT_DEADLINE.as_secs()
        ),
    };
    let stderr = String::from_utf8(output.stderr).expect("host stderr is UTF-8");
    let stdout = String::from_utf8(output.stdout).expect("host stdout is UTF-8");
    println!(
        "KELD_PACK_AC2 input_sha256={} input_bytes={} embedded_sha256={} embedded_bytes={} \
         exit={:?}",
        sha256_hex(&host),
        host.len(),
        sha256_hex(&embedded),
        embedded.len(),
        output.status.code()
    );
    println!("KELD_PACK_AC2 stdout={stdout:?}");
    println!("KELD_PACK_AC2 stderr={stderr:?}");

    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    // WinVerifyTrust reports the embedded image as unsigned rather than as an unknown
    // subject, so the refusal is the KEL-135 identity check, reached with zero startup
    // resources.
    assert!(
        stderr.starts_with(
            "KELD-WV-009: no-flag host failed during Windows authenticated app identity — "
        ),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "{WIN_VERIFY_TRUST_REJECTED}{TRUST_E_NOSIGNATURE:08x}."
        )),
        "{stderr}"
    );
    assert!(
        stderr
            .trim_end()
            .ends_with("[startup-resource-attempts listener=0 child=0 window=0]"),
        "{stderr}"
    );
}
