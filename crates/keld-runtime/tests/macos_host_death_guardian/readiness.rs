//! Ready-state and group-ownership observation for real macOS Guardian tests.
use super::roles::spawn_link_peer;
use super::support::{accept_line_before, parse_pid, process_group, process_rss_kib};
use super::{
    CycleCleanup, Duration, EVENT_DEADLINE, FileTypeExt, Instant, Path, PathBuf, UnixListener, fs,
    parse_app_link,
};

pub(super) struct ReadyState {
    pub(super) guardian_pid: u32,
    pub(super) leader_pid: u32,
    pub(super) descendant_pid: u32,
    pub(super) endpoint: PathBuf,
    pub(super) session_dir: PathBuf,
    pub(super) registered_link: PathBuf,
}

pub(super) fn await_ready(
    listener: &UnixListener,
    control: &Path,
    host_pid: u32,
    cycle_started: Instant,
    cleanup: &mut CycleCleanup,
    eof_witness: Option<&Path>,
    expect_leaker: bool,
) -> ReadyState {
    let mut guardian_pid = None;
    let mut guardian_group_pid = None;
    let mut leader_event_pid = None;
    let mut descendant_pid = None;
    let mut app_link = None;
    let mut registered_link = None;
    let mut link_bound = false;
    let mut peer_bound = false;
    let mut guardian_ready_after = None;

    let ready_deadline = Instant::now() + EVENT_DEADLINE;
    while guardian_pid.is_none()
        || guardian_group_pid.is_none()
        || leader_event_pid.is_none()
        || descendant_pid.is_none()
        || app_link.is_none()
        || registered_link.is_none()
        || !link_bound
        || !peer_bound
        || (expect_leaker && cleanup.leaker.is_none())
    {
        let line = accept_line_before(listener, ready_deadline);
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("GUARDIAN") => {
                guardian_pid = Some(parse_pid(fields.next(), &line));
                let group = parse_pid(fields.next(), &line);
                guardian_group_pid = Some(group);
                guardian_ready_after = Some(cycle_started.elapsed());
                cleanup.group = Some(group);
            }
            Some("LEADER") => leader_event_pid = Some(parse_pid(fields.next(), &line)),
            Some("DESCENDANT") => descendant_pid = Some(parse_pid(fields.next(), &line)),
            Some("APP_LINK") => {
                let app_link_value = fields
                    .next()
                    .unwrap_or_else(|| panic!("APP_LINK event omitted value: {line}"))
                    .to_owned();
                cleanup.peer = Some(spawn_link_peer(control, &app_link_value, eof_witness));
                app_link = Some(app_link_value);
            }
            Some("REGISTERED_LINK") => {
                registered_link =
                    Some(PathBuf::from(fields.next().unwrap_or_else(|| {
                        panic!("REGISTERED_LINK event omitted path: {line}")
                    })));
            }
            Some("LINK_BOUND") => link_bound = true,
            Some("PEER_BOUND") => peer_bound = true,
            Some("LEAKER") => cleanup.leaker = Some(parse_pid(fields.next(), &line)),
            event => panic!("unexpected readiness event {event:?}: {line}"),
        }
    }

    let leader_pid = guardian_group_pid.expect("guardian registered group");
    assert_eq!(
        leader_event_pid,
        Some(leader_pid),
        "guardian and group leader must identify the same process group"
    );
    let descendant_pid = descendant_pid.expect("descendant ready");
    let guardian_pid = guardian_pid.expect("guardian ready");
    let app_link = app_link.expect("app link ready");
    let registered_link = registered_link.expect("registered link ready");
    let (endpoint, _) = parse_app_link(&app_link).expect("parse controller app link");
    let endpoint = PathBuf::from(endpoint);
    let session_dir = endpoint
        .parent()
        .expect("app-link endpoint has session directory")
        .to_path_buf();
    let ready = ReadyState {
        guardian_pid,
        leader_pid,
        descendant_pid,
        endpoint,
        session_dir,
        registered_link,
    };
    verify_ready_ownership(host_pid, &ready, guardian_ready_after);
    ready
}

fn verify_ready_ownership(
    host_pid: u32,
    ready: &ReadyState,
    guardian_ready_after: Option<Duration>,
) {
    assert!(
        fs::metadata(&ready.registered_link)
            .expect("registered link metadata")
            .file_type()
            .is_socket(),
        "registered link must be a real Unix socket before host death"
    );
    assert!(
        ready.guardian_pid != host_pid && ready.guardian_pid != ready.leader_pid,
        "guardian, host, and Bun group leader must be different processes"
    );
    assert_eq!(
        process_group(ready.leader_pid),
        ready.leader_pid,
        "Bun leader must own its isolated process group"
    );
    assert_eq!(
        process_group(ready.descendant_pid),
        ready.leader_pid,
        "Bun descendants must remain enrolled in the leader's process group"
    );
    eprintln!(
        "KELD_GUARDIAN_EVIDENCE registration_us={} guardian_rss_kib={} leader_rss_kib={}",
        guardian_ready_after
            .expect("guardian registration timing")
            .as_micros(),
        process_rss_kib(ready.guardian_pid),
        process_rss_kib(ready.leader_pid)
    );
}
