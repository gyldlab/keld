//! Owned temporary resources, process probes, and bounded test-control events.
use super::{
    BufRead, BufReader, Child, Command, Instant, NEXT_PATH_ID, Ordering, PROCESS_GONE_DEADLINE,
    Path, PathBuf, PermissionsExt, Stdio, SystemTime, UNIX_EPOCH, UnixListener, UnixStream, Write,
    fs, thread,
};

pub(super) fn send_event(control: &Path, event: &str) {
    try_send_event(control, event).expect("send controller event");
}

pub(super) fn try_send_event(control: &Path, event: &str) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(control)?;
    stream.write_all(event.as_bytes())?;
    stream.write_all(b"\n")
}

pub(super) fn revoke_registered_link(path: &Path) -> std::io::Result<()> {
    fs::remove_file(path)?;
    let owner_dir = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "registered link has no owner directory",
        )
    })?;
    fs::remove_dir(owner_dir)
}

pub(super) fn accept_line_before(listener: &UnixListener, deadline: Instant) -> String {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("normalize accepted controller stream");
                let mut line = String::new();
                BufReader::new(stream)
                    .read_line(&mut line)
                    .expect("read controller event");
                return line.trim_end().to_owned();
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "controller received no event before its deadline"
                );
                thread::yield_now();
            }
            Err(error) => panic!("accept controller event: {error}"),
        }
    }
}

pub(super) fn parse_pid(field: Option<&str>, line: &str) -> u32 {
    field
        .unwrap_or_else(|| panic!("event omitted pid: {line}"))
        .parse()
        .unwrap_or_else(|error| panic!("event pid is invalid ({error}): {line}"))
}

fn process_exists(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(super) fn kill_pid(pid: u32) {
    let status = Command::new("/bin/kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .expect("invoke kill for one process");
    assert!(status.success(), "kill process {pid}: {status:?}");
}

pub(super) fn process_group(pid: u32) -> u32 {
    let output = Command::new("/bin/ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .expect("inspect process group");
    assert!(output.status.success(), "inspect process group for {pid}");
    String::from_utf8(output.stdout)
        .expect("process group output is UTF-8")
        .trim()
        .parse()
        .expect("process group output is numeric")
}

pub(super) fn parent_process(pid: u32) -> u32 {
    let output = Command::new("/bin/ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .expect("inspect parent process");
    assert!(output.status.success(), "inspect parent process for {pid}");
    String::from_utf8(output.stdout)
        .expect("parent process output is UTF-8")
        .trim()
        .parse()
        .expect("parent process output is numeric")
}

pub(super) fn process_rss_kib(pid: u32) -> u64 {
    let output = Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .expect("inspect process RSS");
    assert!(output.status.success(), "inspect process RSS for {pid}");
    String::from_utf8(output.stdout)
        .expect("process RSS output is UTF-8")
        .trim()
        .parse()
        .expect("process RSS output is numeric")
}

pub(super) fn await_process_gone(pid: u32) {
    let deadline = Instant::now() + PROCESS_GONE_DEADLINE;
    while process_exists(pid) {
        assert!(
            Instant::now() < deadline,
            "process {pid} survived the guardian's bounded reap deadline"
        );
        thread::yield_now();
    }
}

pub(super) struct CycleCleanup {
    pub(super) host: Option<Child>,
    pub(super) peer: Option<Child>,
    pub(super) group: Option<u32>,
    pub(super) leaker: Option<u32>,
    pub(super) group_gone: bool,
}

impl CycleCleanup {
    pub(super) fn new(host: Child) -> Self {
        Self {
            host: Some(host),
            peer: None,
            group: None,
            leaker: None,
            group_gone: false,
        }
    }
}

impl Drop for CycleCleanup {
    fn drop(&mut self) {
        if !self.group_gone
            && let Some(group) = self.group
        {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{group}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if let Some(host) = self.host.as_mut() {
            let _ = host.kill();
            let _ = host.wait();
        }
        if let Some(peer) = self.peer.as_mut() {
            let _ = peer.kill();
            let _ = peer.wait();
        }
        if let Some(leaker) = self.leaker {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &leaker.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

pub(super) struct Fixture {
    root: PathBuf,
    pub(super) control: PathBuf,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let id = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        let epoch_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after Unix epoch")
            .as_nanos();
        let root = PathBuf::from(format!(
            "/tmp/k78-{}-{epoch_nanos:x}-{id}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create owner fixture directory");
        let mut permissions = fs::metadata(&root).expect("fixture metadata").permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&root, permissions).expect("set fixture permissions");
        let control = root.join("control.sock");
        Self { root, control }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.control);
        let _ = fs::remove_dir(&self.root);
    }
}
