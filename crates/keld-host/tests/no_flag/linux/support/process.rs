//! Native PID/start-time census, exit observation and identity-checked fault injection.
#![allow(unsafe_code)] // only the existing identity-checked SIGKILL controller

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read as _,
    process::{Child, ExitStatus},
    thread,
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn kill(pid: std::os::raw::c_int, signal: std::os::raw::c_int) -> std::os::raw::c_int;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProcessIdentity {
    pub(crate) pid: u32,
    start_time: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StrictGeneration {
    pub(crate) bun: ProcessIdentity,
    pub(crate) descendant: ProcessIdentity,
}

pub(crate) fn wait_for_direct_host(root: u32, deadline: Instant) -> ProcessIdentity {
    loop {
        if let Some(host) = descendant_identities(root).into_iter().find(|process| {
            process_stat(process.pid).is_some_and(|(parent, _)| parent == root)
                && process_executable_name(process.pid).as_deref() == Some("keld-host")
        }) {
            return host;
        }
        assert!(Instant::now() < deadline, "host child did not appear");
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(crate) fn wait_for_strict_generation(host: u32, deadline: Instant) -> StrictGeneration {
    loop {
        let mut bun = None;
        let mut descendant = None;
        for process in descendant_identities(host) {
            let command = fs::read(format!("/proc/{}/cmdline", process.pid)).unwrap_or_default();
            if command.split(|byte| *byte == 0).next() != Some(b"/runtime/program".as_slice()) {
                continue;
            }
            if command
                .windows(b"/code/main.ts".len())
                .any(|part| part == b"/code/main.ts")
            {
                bun = Some(process);
            } else if command
                .windows(b"await new Promise".len())
                .any(|part| part == b"await new Promise")
            {
                descendant = Some(process);
            }
        }
        if let (Some(bun), Some(descendant)) = (bun, descendant) {
            return StrictGeneration { bun, descendant };
        }
        assert!(
            Instant::now() < deadline,
            "strict Bun generation did not become observable"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(crate) fn descendant_identities(root: u32) -> Vec<ProcessIdentity> {
    let mut parents = BTreeSet::from([root]);
    let mut found = BTreeMap::new();
    loop {
        let before = found.len();
        for entry in fs::read_dir("/proc").expect("process census") {
            let Ok(entry) = entry else { continue };
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Some((parent, start_time)) = process_stat(pid) else {
                continue;
            };
            if parents.contains(&parent) && pid != root {
                parents.insert(pid);
                found.insert(pid, ProcessIdentity { pid, start_time });
            }
        }
        if found.len() == before {
            return found.into_values().collect();
        }
    }
}

pub(crate) fn process_stat(pid: u32) -> Option<(u32, u64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, fields) = stat.rsplit_once(") ")?;
    let fields = fields.split_whitespace().collect::<Vec<_>>();
    let parent = fields.get(1)?.parse().ok()?;
    let start_time = fields.get(19)?.parse().ok()?;
    Some((parent, start_time))
}

fn process_executable_name(pid: u32) -> Option<String> {
    fs::read_link(format!("/proc/{pid}/exe"))
        .ok()?
        .file_name()?
        .to_str()
        .map(str::to_owned)
}

pub(crate) fn wait_process_identity_gone(process: &ProcessIdentity, deadline: Instant) {
    loop {
        if process_stat(process.pid).is_none_or(|(_, start)| start != process.start_time) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "process survived teardown: {process:?}"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(crate) fn sigkill_identity(process: &ProcessIdentity) {
    assert_eq!(
        process_stat(process.pid).map(|(_, start)| start),
        Some(process.start_time),
        "host identity changed before SIGKILL"
    );
    let pid = i32::try_from(process.pid).expect("host PID fits pid_t");
    // SAFETY: the PID and start time were revalidated immediately above. The
    // test controller owns this exact staged host and sends only SIGKILL.
    assert_eq!(unsafe { kill(pid, 9) }, 0, "SIGKILL staged host");
}

pub(crate) fn wait_child(child: &mut Child, deadline: Instant) -> ExitStatus {
    loop {
        if let Some(status) = child.try_wait().expect("observe host exit") {
            return status;
        }
        assert!(Instant::now() < deadline, "host exit timed out");
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(crate) fn wait_child_output(mut child: Child, deadline: Instant) -> std::process::Output {
    let status = wait_child(&mut child, deadline);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .expect("captured stdout")
        .read_to_end(&mut stdout)
        .expect("read stdout");
    child
        .stderr
        .take()
        .expect("captured stderr")
        .read_to_end(&mut stderr)
        .expect("read stderr");
    std::process::Output {
        status,
        stdout,
        stderr,
    }
}
