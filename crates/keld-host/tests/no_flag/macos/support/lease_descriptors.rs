use std::process::Command;

pub(crate) fn lsof_stdin(pid: u32) -> String {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "0", "-FDifnat"])
        .output()
        .expect("inspect stdin with lsof");
    assert!(output.status.success(), "lsof stdin failed: {output:?}");
    String::from_utf8(output.stdout).expect("lsof stdin UTF-8")
}

pub(crate) fn lsof_all(pid: u32) -> String {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-a", "-p", &pid.to_string(), "-FDifnat"])
        .output()
        .expect("inspect process descriptors with lsof");
    assert!(output.status.success(), "lsof process failed: {output:?}");
    String::from_utf8(output.stdout).expect("lsof process UTF-8")
}

pub(crate) fn assert_lease_descriptor_ownership(
    cli_pid: u32,
    host_pid: u32,
    guardian_pid: u32,
    bun_pid: u32,
) {
    let cli_fds = lsof_all(cli_pid);
    let host_lease = pipe_identity(host_pid, "0");
    let lease_writers: Vec<&str> = pipe_descriptors(&cli_fds)
        .into_iter()
        .filter(|fd| {
            let candidate = pipe_identity(cli_pid, fd);
            candidate.0 == host_lease.1 && candidate.1 == host_lease.0
        })
        .collect();
    assert_eq!(
        lease_writers.len(),
        1,
        "CLI must own exactly one writer reciprocal to host fd 0: {cli_fds}"
    );
    let host_stdin = lsof_stdin(host_pid);
    assert!(host_stdin.contains("tPIPE"), "host stdin: {host_stdin}");
    let lease_peer = host_stdin
        .lines()
        .find(|line| line.starts_with("n->"))
        .expect("host lease pipe peer");
    let guardian_fds = lsof_all(guardian_pid);
    let bun_fds = lsof_all(bun_pid);
    for (pid, snapshot) in [(guardian_pid, &guardian_fds), (bun_pid, &bun_fds)] {
        for fd in pipe_descriptors(snapshot) {
            let candidate = pipe_identity(pid, fd);
            assert!(
                candidate != host_lease
                    && (candidate.0, candidate.1) != (host_lease.1, host_lease.0),
                "process {pid} inherited a dev-lease endpoint on fd {fd}: {snapshot}"
            );
        }
    }
    assert!(
        !guardian_fds.lines().any(|line| line == lease_peer),
        "guardian inherited the host lease reader: {guardian_fds}"
    );
    assert!(
        !bun_fds.lines().any(|line| line == lease_peer),
        "Bun inherited the host lease reader: {bun_fds}"
    );
    let guardian_stdin = lsof_stdin(guardian_pid);
    assert!(
        guardian_stdin.contains("tPIPE") && !guardian_stdin.contains(lease_peer),
        "guardian stdin must be its distinct authenticated bootstrap pipe: {guardian_stdin}"
    );
    let bun_stdin = lsof_stdin(bun_pid);
    assert!(
        bun_stdin.contains("tCHR") && bun_stdin.contains("n/dev/null"),
        "Bun stdin is not null: {bun_stdin}"
    );
}

pub(crate) fn pipe_descriptors(snapshot: &str) -> Vec<&str> {
    let mut current = None;
    let mut pipes = Vec::new();
    for line in snapshot.lines() {
        if let Some(fd) = line.strip_prefix('f') {
            current = Some(fd);
        } else if line == "tPIPE"
            && let Some(fd) = current
        {
            pipes.push(fd);
        }
    }
    pipes
}

pub(crate) fn pipe_identity(pid: u32, fd: &str) -> (u64, u64) {
    // macOS `proc_pidfdinfo(PROC_PIDFDPIPEINFO)` exposes the kernel pipe
    // handle/peer-handle pair, so the oracle can match opposite endpoints
    // without inferring identity from descriptor numbers or `lsof` names.
    const SCRIPT: &str = r#"
import Darwin
let pid = Int32(CommandLine.arguments[1])!
let fd = Int32(CommandLine.arguments[2])!
var info = pipe_fdinfo()
let size = proc_pidfdinfo(pid, fd, PROC_PIDFDPIPEINFO, &info, Int32(MemoryLayout<pipe_fdinfo>.size))
guard size == MemoryLayout<pipe_fdinfo>.size else { exit(2) }
print("\(info.pipeinfo.pipe_handle) \(info.pipeinfo.pipe_peerhandle)")
"#;
    let output = Command::new("/usr/bin/xcrun")
        .args(["swift", "-e", SCRIPT, &pid.to_string(), fd])
        .output()
        .expect("inspect macOS pipe identity");
    assert!(output.status.success(), "pipe identity failed: {output:?}");
    let rendered = String::from_utf8(output.stdout).expect("pipe identity UTF-8");
    let mut fields = rendered.split_whitespace();
    let handle = fields
        .next()
        .expect("pipe handle")
        .parse()
        .expect("numeric pipe handle");
    let peer = fields
        .next()
        .expect("pipe peer handle")
        .parse()
        .expect("numeric pipe peer handle");
    assert!(
        fields.next().is_none(),
        "unexpected pipe identity: {rendered}"
    );
    (handle, peer)
}
