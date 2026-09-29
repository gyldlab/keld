//! Shared macOS AC9 child ownership; product assertions stay in the scenarios.

use std::fs::{self, File};
use std::io;
use std::os::unix::{fs::DirBuilderExt, process::ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CASE_ENV: &str = "KELD_AC9_CASE";
type Capture = (Vec<u8>, Vec<u8>, Vec<String>);

struct Owner {
    root: PathBuf,
    child: Option<Child>,
    finished: bool,
}

fn record(root: &Path) -> io::Result<Capture> {
    let stdout = fs::read(root.join("stdout.log"));
    let stderr = fs::read(root.join("stderr.log"));
    for (label, result) in [("stdout", &stdout), ("stderr", &stderr)] {
        match result {
            Ok(bytes) => println!("AC9_CAPTURE {label}\n{}", String::from_utf8_lossy(bytes)),
            Err(error) => eprintln!("AC9_CAPTURE {label} error={error}"),
        }
    }
    let remaining = fs::read_dir(root)
        .and_then(|entries| {
            entries
                .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
                .collect::<io::Result<Vec<_>>>()
        })
        .map(|mut names| {
            names.retain(|name| name != "stdout.log" && name != "stderr.log");
            names.sort();
            names
        });
    println!("AC9_ROOT before_cleanup={remaining:?}");
    Ok((stdout?, stderr?, remaining?))
}

fn remove(root: &Path) -> io::Result<()> {
    let cleanup = fs::remove_dir_all(root);
    let exists = root.try_exists();
    println!(
        "AC9_ROOT root={} cleanup={cleanup:?} exists={exists:?}",
        root.display()
    );
    cleanup?;
    if exists? {
        return Err(io::Error::other("AC9 owned root remains after cleanup"));
    }
    Ok(())
}

impl Drop for Owner {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let reaped = self.child.as_mut().is_none_or(|child| {
            let kill = child.kill();
            let wait = child.wait();
            eprintln!(
                "AC9_EMERGENCY pid={} kill={kill:?} wait={wait:?}",
                child.id()
            );
            wait.is_ok()
        });
        eprintln!("AC9_EMERGENCY reaped={reaped}");
        let capture = record(&self.root);
        if reaped && capture.is_ok() {
            if let Err(error) = remove(&self.root) {
                eprintln!("AC9_ROOT cleanup_error={error}");
            }
        } else {
            eprintln!(
                "AC9_ROOT cleanup_skipped root={} reaped={reaped} capture_ok={}",
                self.root.display(),
                capture.is_ok()
            );
        }
    }
}

pub(super) fn run_case_child(selector: &str, case: &str, limit: Duration) -> Output {
    let deadline = Instant::now()
        .checked_add(limit)
        .expect("finite AC9 child limit");
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).expect("private AC9 root nonce");
    let root = PathBuf::from(format!("/tmp/ka9-{:016x}", u64::from_le_bytes(nonce)));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .expect("exclusive AC9 root");
    let mut owner = Owner {
        root,
        child: None,
        finished: false,
    };
    owner.child = Some(
        Command::new(std::env::current_exe().expect("current test binary"))
            .args(["--exact", selector, "--ignored", "--nocapture"])
            .env(CASE_ENV, case)
            .env("TMPDIR", &owner.root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                File::create(owner.root.join("stdout.log")).expect("stdout capture"),
            ))
            .stderr(Stdio::from(
                File::create(owner.root.join("stderr.log")).expect("stderr capture"),
            ))
            .spawn()
            .expect("spawn exact AC9 child"),
    );
    let mut reason = "natural";
    let observed = loop {
        match owner.child.as_mut().expect("owned child").try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    reason = "timeout";
                    break None;
                }
                // Throttle exit polling; only try_wait observes completion.
                thread::park_timeout(remaining.min(Duration::from_millis(5)));
            }
            Err(error) => {
                eprintln!("AC9_OBSERVE_ERROR case={case} error={error}");
                reason = "observe-error";
                break None;
            }
        }
    };
    let status = observed.map_or_else(
        || {
            let child = owner.child.as_mut().expect("owned child");
            let kill = child.kill();
            let wait = child.wait();
            println!("AC9_TERMINATE case={case} kill={kill:?} wait={wait:?}");
            wait
        },
        Ok,
    );
    let code = status.as_ref().ok().and_then(ExitStatus::code);
    let signal = status.as_ref().ok().and_then(ExitStatusExt::signal);
    println!(
        "AC9_CHILD case={case} reason={reason} reaped={} code={code:?} signal={signal:?} status={status:?}",
        status.is_ok()
    );
    if status.is_ok() {
        owner.child = None;
    }
    // A reap or capture failure is incomplete evidence and retains the root.
    let capture = record(&owner.root);
    println!(
        "AC9_CAPTURE_COMPLETE case={case} complete={}",
        status.is_ok() && capture.is_ok()
    );
    let cleanup = if status.is_ok() && capture.is_ok() {
        remove(&owner.root)
    } else {
        Err(io::Error::other(
            "AC9 root retained: child reap or capture incomplete",
        ))
    };
    owner.finished = cleanup.is_ok();

    // Every post-spawn failure first records the available output and outcome.
    let status = status.expect("AC9 child reap failed");
    let (stdout, stderr, remaining) = capture.expect("record AC9 child evidence");
    cleanup.expect("remove AC9 owned root after reap and recording");
    assert_eq!(
        reason, "natural",
        "emergency termination is not product completion"
    );
    assert!(
        remaining.is_empty(),
        "product left resources before harness cleanup: {remaining:?}"
    );
    assert!(status.success(), "AC9 child product assertion failed");
    let marker = format!("AC9_ENTRY case={case}\n");
    assert_eq!(
        String::from_utf8_lossy(&stdout).matches(&marker).count(),
        1,
        "exact child entry must execute once; zero selected tests cannot pass"
    );
    Output {
        status,
        stdout,
        stderr,
    }
}

pub(super) fn case_id() -> String {
    let case = std::env::var(CASE_ENV).expect("AC9 child requires its explicit parent");
    println!("\nAC9_ENTRY case={case}");
    case
}

// The plain-session consumer uses an unnamed socket pair instead of a locator.
#[allow(dead_code)]
pub(super) fn capture_root() -> PathBuf {
    PathBuf::from(std::env::var_os("TMPDIR").expect("parent-owned AC9 TMPDIR"))
}
