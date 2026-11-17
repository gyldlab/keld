//! Owned temporary resources, process probes, and bounded test-control events.
use super::{
    Child, Command, Instant, NEXT_PATH_ID, Ordering, PROCESS_GONE_DEADLINE, Path, PathBuf,
    PermissionsExt, Read, Stdio, SystemTime, UNIX_EPOCH, UnixListener, UnixStream, Write, fs,
    thread,
};
use nix::libc;
use nix::sys::signal::{Signal, kill};
use nix::unistd::{Pid, getpgrp};

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
    // The *accepted* peer is not trusted to send a complete line. The original
    // absolute deadline covers accept, every byte and the final newline; a
    // successful accept or byte drip never grants a fresh read deadline.
    const MAX_EVENT_BYTES: usize = 4_096;
    loop {
        assert!(
            Instant::now() < deadline,
            "controller received no complete event before its original deadline"
        );
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_nonblocking(true)
                    .expect("read controller event without unbounded accepted-peer wait");
                let mut bytes = Vec::new();
                loop {
                    assert!(
                        Instant::now() < deadline,
                        "accepted controller event exceeded its original deadline"
                    );
                    let mut byte = [0_u8; 1];
                    match stream.read(&mut byte) {
                        Ok(1) if byte[0] == b'\n' => {
                            assert!(
                                Instant::now() < deadline,
                                "late event newline cannot extend controller deadline"
                            );
                            return String::from_utf8(bytes)
                                .expect("controller event is UTF-8")
                                .trim_end()
                                .to_owned();
                        }
                        Ok(1) => {
                            bytes.push(byte[0]);
                            assert!(
                                bytes.len() <= MAX_EVENT_BYTES,
                                "controller event exceeds bounded line size"
                            );
                        }
                        Ok(0) => panic!("controller event EOF before its final newline"),
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                            ) =>
                        {
                            thread::yield_now();
                        }
                        Err(error) => panic!("read controller event: {error}"),
                        Ok(_) => unreachable!("single-byte event reader"),
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
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

/// Original, live macOS kernel birth captured from the expected parent while
/// its process still exists. A controller wire PID alone is never signal
/// authority. Its parent may change after host death, but UID/birth and the
/// original group must remain identical before any later signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct NativeBirth {
    pub(super) pid: u32,
    group: u32,
    real_uid: libc::uid_t,
    started_sec: u64,
    started_usec: u64,
}

#[allow(unsafe_code)] // test-only exact-size read-only Darwin PROC_PIDTBSDINFO observation
fn observe_native_birth(pid: u32) -> Option<(NativeBirth, u32)> {
    let raw = i32::try_from(pid).ok()?;
    if raw <= 1 {
        return None;
    }
    // SAFETY: proc_bsdinfo contains only integer/fixed-array fields. The
    // kernel writes exactly the requested size and retains no Rust pointers.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: initialized exact-size writable C output buffer; pid fits c_int.
    let read =
        unsafe { libc::proc_pidinfo(raw, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    if read != size || info.pbi_pid != pid {
        return None;
    }
    Some((
        NativeBirth {
            pid,
            group: info.pbi_pgid,
            real_uid: info.pbi_ruid,
            started_sec: info.pbi_start_tvsec,
            started_usec: info.pbi_start_tvusec,
        },
        info.pbi_ppid,
    ))
}

pub(super) fn retain_parented_birth(pid: u32, expected_parent: u32) -> NativeBirth {
    let (birth, parent) =
        observe_native_birth(pid).expect("capture live original native process birth");
    assert_eq!(
        parent, expected_parent,
        "controller-reported PID must be the expected host/Guardian's actual direct child"
    );
    birth
}

/// Retain the live original birth of a same-group member at readiness: its
/// actual parent is the group leader and the kernel places it in that group.
pub(super) fn retain_group_member_birth(
    pid: u32,
    expected_parent: u32,
    expected_group: u32,
) -> NativeBirth {
    let birth = retain_parented_birth(pid, expected_parent);
    assert_eq!(
        birth.group, expected_group,
        "controller-reported member must belong to the original process group"
    );
    birth
}

fn require_original_birth(original: NativeBirth) -> std::io::Result<i32> {
    let raw = i32::try_from(original.pid).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "original PID exceeds c_int",
        )
    })?;
    if raw <= 1 || observe_native_birth(original.pid).map(|(birth, _)| birth) != Some(original) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "original fixture process birth disappeared or changed; no numeric PID signal",
        ));
    }
    Ok(raw)
}

pub(super) fn kill_attested_process(original: NativeBirth) -> std::io::Result<()> {
    let raw = require_original_birth(original)?;
    if raw == i32::try_from(std::process::id()).expect("test runner PID fits c_int") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "fixture must never signal its own test runner",
        ));
    }
    kill(Pid::from_raw(raw), Signal::SIGKILL).map_err(std::io::Error::from)
}

fn kill_attested_group(original: NativeBirth) -> std::io::Result<()> {
    let raw = require_original_birth(original)?;
    if original.group != original.pid || Pid::from_raw(raw) == getpgrp() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "original fixture leader has no exclusive group authority",
        ));
    }
    kill(Pid::from_raw(-raw), Signal::SIGKILL).map_err(std::io::Error::from)
}

// Production parity with `process_group::terminate_attested_group`: a reaped
// leader whose group survives through members keeps group cleanup, because
// Darwin reserves a PGID while it has members. Stricter than production, the
// fixture signals that group only while one retained same-group member birth
// still matches exactly (PID, UID, start time and original PGID).
fn kill_group_via_attested_member(member: NativeBirth, group: u32) -> std::io::Result<()> {
    let member_raw = require_original_birth(member)?;
    let raw = i32::try_from(group).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "original group exceeds c_int",
        )
    })?;
    if member.group != group || member_raw == raw || raw <= 1 || Pid::from_raw(raw) == getpgrp() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "retained member is not an exclusive original group member",
        ));
    }
    kill(Pid::from_raw(-raw), Signal::SIGKILL).map_err(std::io::Error::from)
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
    pub(super) group_birth: Option<NativeBirth>,
    pub(super) descendant_birth: Option<NativeBirth>,
    pub(super) leaker: Option<u32>,
    pub(super) leaker_birth: Option<NativeBirth>,
    pub(super) group_gone: bool,
}

impl CycleCleanup {
    pub(super) fn new(host: Child) -> Self {
        Self {
            host: Some(host),
            peer: None,
            group: None,
            group_birth: None,
            descendant_birth: None,
            leaker: None,
            leaker_birth: None,
            group_gone: false,
        }
    }
}

impl Drop for CycleCleanup {
    fn drop(&mut self) {
        if !self.group_gone
            && let (Some(group), Some(birth)) = (self.group, self.group_birth)
            && group == birth.pid
        {
            // A saved event is never authority. If the original leader was
            // reaped or its PGID reused, only a retained same-group member
            // birth that still matches exactly may keep group cleanup.
            if kill_attested_group(birth).is_err()
                && let Some(member) = self.descendant_birth
            {
                let _ = kill_group_via_attested_member(member, birth.pid);
            }
        }
        if let Some(host) = self.host.as_mut() {
            let _ = host.kill();
            let _ = host.wait();
        }
        if let Some(peer) = self.peer.as_mut() {
            let _ = peer.kill();
            let _ = peer.wait();
        }
        if let (Some(leaker), Some(birth)) = (self.leaker, self.leaker_birth)
            && leaker == birth.pid
        {
            let _ = kill_attested_process(birth);
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

#[cfg(test)]
#[path = "support/tests.rs"]
mod tests;
