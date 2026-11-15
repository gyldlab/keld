//! Exact-self custody for the risky held schedule; no native operation in the parent.
use super::Fixture;
use std::io::{Read as _, Seek as _};
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SELECTOR: &str = "fs::acceptance_observer::tests::child::retained_hold_child";
const OUTER_BUDGET: Duration = Duration::from_secs(15);

struct ChildCustody {
    child: Child,
    reaped: bool,
}

impl Drop for ChildCustody {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

struct Evidence {
    output: Output,
    pid: u32,
    token: String,
    executable: PathBuf,
    timed_out: bool,
    fixture: Fixture,
}

fn run_child(selector: &str) -> Evidence {
    // Reuse native's existing owned-root and exclusive capture-file pattern.
    // File captures avoid pipe drain/descendant EOF as a prerequisite to custody.
    let fixture = Fixture::new();
    let resource_root = fixture.0.join("child-files");
    std::fs::create_dir(&resource_root).expect("owned child-only resource root");
    let capture = |name| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(fixture.0.join(name))
            .expect("owned child capture")
    };
    let mut stdout = capture("stdout");
    let mut stderr = capture("stderr");
    let executable = std::env::current_exe().expect("exact native libtest executable");
    let token = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("token clock")
            .as_nanos()
    );
    let child = Command::new(&executable)
        .args([
            "--exact",
            selector,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("KELD_NATIVE_OBSERVER_CHILD_TOKEN", &token)
        .env("KELD_NATIVE_OBSERVER_CHILD_ROOT", &resource_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            stdout.try_clone().expect("capture stdout clone"),
        ))
        .stderr(Stdio::from(
            stderr.try_clone().expect("capture stderr clone"),
        ))
        .spawn()
        .expect("spawn owned native self-child");
    let mut custody = ChildCustody {
        child,
        reaped: false,
    };
    let pid = custody.child.id();
    let deadline = Instant::now() + OUTER_BUDGET;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = custody.child.try_wait().expect("observe owned child exit") {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            custody
                .child
                .kill()
                .expect("external watchdog terminates only owned child");
            break custody
                .child
                .wait()
                .expect("reap watchdog-terminated child");
        }
        // Exit status, not elapsed delay, ends the successful wait.
        std::thread::yield_now();
    };
    custody.reaped = true;
    let read_capture = |file: &mut std::fs::File| {
        assert!(
            file.metadata().expect("capture length").len() <= 64 * 1024,
            "native child output exceeds bounded capture"
        );
        file.rewind().expect("rewind child capture");
        let mut bytes = Vec::new();
        file.take(64 * 1024 + 1)
            .read_to_end(&mut bytes)
            .expect("bounded child output");
        assert!(bytes.len() <= 64 * 1024);
        bytes
    };
    let output = Output {
        status,
        stdout: read_capture(&mut stdout),
        stderr: read_capture(&mut stderr),
    };
    eprintln!(
        "KELD_NATIVE_OBSERVER_CHILD_CAPTURE pid={pid} timed_out={timed_out} output={output:?}"
    );
    Evidence {
        output,
        pid,
        token,
        executable,
        timed_out,
        fixture,
    }
}

fn accepted(evidence: &Evidence) -> bool {
    let stdout = String::from_utf8_lossy(&evidence.output.stdout);
    let acknowledgement = format!(
        "KELD_NATIVE_OBSERVER_CHILD_ACK token={} pid={} executable={:?}",
        evidence.token, evidence.pid, evidence.executable
    );
    let completion = format!(
        "KELD_NATIVE_OBSERVER_CHILD_COMPLETE token={} pid={}",
        evidence.token, evidence.pid
    );
    !evidence.timed_out
        && evidence.output.status.success()
        && evidence.output.status.signal().is_none()
        && stdout
            .lines()
            .filter(|line| *line == "running 1 test")
            .count()
            == 1
        && stdout
            .lines()
            .filter(|line| *line == acknowledgement)
            .count()
            == 1
        && stdout.lines().filter(|line| *line == completion).count() == 1
        && stdout.lines().filter(|line|
            *line == "KELD_NATIVE_OBSERVER_NEXT_OPERATION native_read=success bytes=6368616e676564"
        ).count() == 1
}

pub(super) fn run_and_assert_hold() {
    let evidence = run_child(SELECTOR);
    assert!(
        accepted(&evidence),
        "exact isolated held-native case did not complete: {:?}",
        evidence.output
    );
    assert_eq!(
        std::fs::read(evidence.fixture.0.join("child-files/target"))
            .expect("independent parent after-child file bytes"),
        b"changed"
    );
    // The exact child is already reaped before file cleanup. Fixture Drop owns
    // its only files; this scenario creates no descendants or other processes.
}

#[test]
fn missing_self_child_selector_cannot_be_accepted() {
    let evidence = run_child("fs::acceptance_observer::tests::child::nonexistent_selector");
    assert!(
        !accepted(&evidence),
        "zero selected must fail the parent's exact acceptance"
    );
    assert!(String::from_utf8_lossy(&evidence.output.stdout).contains("running 0 tests"));
    assert!(!evidence.fixture.0.join("child-files/target").exists());
}

#[test]
#[ignore = "exact helper selected only by the bounded native parent custodian"]
fn retained_hold_child() {
    let token = std::env::var("KELD_NATIVE_OBSERVER_CHILD_TOKEN").expect("explicit parent token");
    assert!(!token.is_empty() && token.len() <= 128);
    let root = PathBuf::from(
        std::env::var_os("KELD_NATIVE_OBSERVER_CHILD_ROOT")
            .expect("parent-owned child-only resources"),
    );
    assert!(root.is_absolute() && root.is_dir());
    println!(
        "\nKELD_NATIVE_OBSERVER_CHILD_ACK token={token} pid={} executable={:?}",
        std::process::id(),
        std::env::current_exe().expect("actual native child executable")
    );
    super::hold_schedule(&root);
    println!(
        "KELD_NATIVE_OBSERVER_CHILD_COMPLETE token={token} pid={}",
        std::process::id()
    );
}
