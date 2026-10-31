//! Owned direct-child custody tests. No actual browser/provider claim.
use super::*;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::thread;
use std::time::Duration;
const HELPER_ENV: &str = "KELD_AUTH_OPEN_OWNER_FIXTURE";
#[test]
fn launch_helper_child() {
    if std::env::var(HELPER_ENV).ok().as_deref() == Some("hold") {
        let marker = std::env::var_os("KELD_AUTH_OPEN_OWNER_MARKER").expect("marker path");
        std::fs::write(marker, b"OWNED_OPEN_HELPER_READY").expect("actual child readiness");
        loop {
            thread::yield_now();
        }
    }
}
fn command(kind: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command
        .args([
            "--exact",
            "auth::browser::tests::launch_helper_child",
            "--nocapture",
        ])
        .env(HELPER_ENV, kind)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);
struct FixtureDirectory(PathBuf);
impl FixtureDirectory {
    fn new() -> Self {
        let index = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("keld-auth-launch-{}-{index}", std::process::id()));
        std::fs::create_dir(&path).expect("new isolated fixture directory");
        Self(path)
    }
    fn await_ready(&self) {
        let bound = Instant::now() + Duration::from_secs(2);
        while std::fs::read(self.0.join("ready")).ok().as_deref()
            != Some(b"OWNED_OPEN_HELPER_READY".as_slice())
        {
            assert!(Instant::now() < bound, "actual helper case did not enter");
            thread::yield_now();
        }
    }
}
impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("fixture namespace cleanup");
    }
}
fn deadlines() -> BrowserDeadlines {
    let now = Instant::now();
    BrowserDeadlines {
        launch: now + Duration::from_secs(2),
        cleanup: now + Duration::from_secs(4),
    }
}
fn finish(owner: &mut BrowserLaunch) -> Result<ExitStatus, BrowserError> {
    let bound = Instant::now() + Duration::from_secs(5);
    loop {
        match owner.poll() {
            BrowserProgress::Complete(result) => return result,
            BrowserProgress::CleanupUnproven(error) => panic!("reap unproven: {error}"),
            BrowserProgress::Pending => {}
        }
        assert!(Instant::now() < bound, "child poll kill switch");
        thread::yield_now();
    }
}
#[test]
fn owned_helper_success_cancel_and_deadline_reap_before_terminal_result() {
    let mut healthy = BrowserLaunch::start_command(
        command("exit"),
        deadlines(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("actual child");
    assert!(finish(&mut healthy).expect("helper exit").success());
    assert!(matches!(healthy.child, ChildState::Reaped));
    for cancelled in [true, false] {
        let directory = FixtureDirectory::new();
        let mut held_command = command("hold");
        held_command.env("KELD_AUTH_OPEN_OWNER_MARKER", directory.0.join("ready"));
        let mut owner = BrowserLaunch::start_command(
            held_command,
            deadlines(),
            Arc::new(AtomicBool::new(false)),
        )
        .expect("owned child");
        directory.await_ready();
        if cancelled {
            owner.cancel();
        }
        let original_launch = owner.deadlines.launch;
        let error = finish(&mut owner).expect_err("stop result");
        assert_eq!(
            error.code(),
            if cancelled {
                "KELD-AUTH-007"
            } else {
                "KELD-AUTH-008"
            }
        );
        assert_eq!(
            owner.deadlines.launch, original_launch,
            "poll renewed the launch deadline"
        );
        assert!(
            error.exit_status.is_some(),
            "terminal error lacked kernel reap receipt"
        );
        assert!(
            matches!(owner.child, ChildState::Reaped),
            "terminal result retained unreaped Child"
        );
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(error.exit_status.expect("receipt").signal(), Some(9));
        }
    }
    let mut next = BrowserLaunch::start_command(
        command("exit"),
        deadlines(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("healthy follow-up child");
    assert!(finish(&mut next).expect("follow-up").success());
}
