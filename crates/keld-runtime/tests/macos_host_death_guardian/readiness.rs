//! Ready-state and group-ownership observation for real macOS Guardian tests.
use super::roles::spawn_link_peer;
use super::support::{
    NativeBirth, accept_line_before, parse_pid, process_group, process_rss_kib,
    retain_group_member_birth, retain_parented_birth,
};
use super::{
    CycleCleanup, Duration, EVENT_DEADLINE, FileTypeExt, Instant, Path, PathBuf, UnixListener, fs,
    parse_app_link,
};

pub(super) struct ReadyState {
    pub(super) guardian_pid: u32,
    pub(super) guardian_birth: NativeBirth,
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
    let mut guardian_birth = None;
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
        let event = fields.next();
        // Reject surplus fields BEFORE any role event can create a native
        // cleanup witness, spawn the app-link peer, or mark a barrier ready.
        // Missing required fields keep their existing field-specific errors.
        let maximum_args = match event {
            Some("GUARDIAN") => Some(2),
            Some("LEADER" | "DESCENDANT" | "APP_LINK" | "REGISTERED_LINK" | "LEAKER") => Some(1),
            Some("LINK_BOUND" | "PEER_BOUND") => Some(0),
            _ => None,
        };
        if let Some(maximum_args) = maximum_args {
            assert!(
                fields.clone().count() <= maximum_args,
                // `event` is one of the fixed whitelisted role names here.
                // Never print the untrusted full event: APP_LINK may contain
                // a private endpoint/token and a malformed line is attacker data.
                "unexpected trailing readiness event fields for {event:?}"
            );
        }
        match event {
            Some("GUARDIAN") => {
                let announced_guardian = parse_pid(fields.next(), "GUARDIAN");
                let group = parse_pid(fields.next(), "GUARDIAN");
                // A private control event is not signal authority. Require
                // both actual original kernel births and the exact host ->
                // Guardian -> group leader parent chain while they are live.
                guardian_birth = Some(retain_parented_birth(announced_guardian, host_pid));
                cleanup.group_birth = Some(retain_parented_birth(group, announced_guardian));
                guardian_pid = Some(announced_guardian);
                guardian_group_pid = Some(group);
                guardian_ready_after = Some(cycle_started.elapsed());
                cleanup.group = Some(group);
            }
            Some("LEADER") => leader_event_pid = Some(parse_pid(fields.next(), "LEADER")),
            Some("DESCENDANT") => descendant_pid = Some(parse_pid(fields.next(), "DESCENDANT")),
            Some("APP_LINK") => {
                let app_link_value = fields
                    .next()
                    .unwrap_or_else(|| panic!("APP_LINK event omitted value"))
                    .to_owned();
                cleanup.peer = Some(spawn_link_peer(control, &app_link_value, eof_witness));
                app_link = Some(app_link_value);
            }
            Some("REGISTERED_LINK") => {
                registered_link =
                    Some(PathBuf::from(fields.next().unwrap_or_else(|| {
                        panic!("REGISTERED_LINK event omitted path")
                    })));
            }
            Some("LINK_BOUND") => link_bound = true,
            Some("PEER_BOUND") => peer_bound = true,
            Some("LEAKER") => {
                let leaker = parse_pid(fields.next(), "LEAKER");
                cleanup.leaker_birth = Some(retain_parented_birth(leaker, host_pid));
                cleanup.leaker = Some(leaker);
            }
            _ => panic!("unexpected readiness event kind"),
        }
    }

    let leader_pid = guardian_group_pid.expect("guardian registered group");
    assert_eq!(
        leader_event_pid,
        Some(leader_pid),
        "guardian and group leader must identify the same process group"
    );
    let descendant_pid = arm_member_cleanup(cleanup, descendant_pid, leader_pid);
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
        guardian_birth: guardian_birth.expect("attested original Guardian birth"),
        leader_pid,
        descendant_pid,
        endpoint,
        session_dir,
        registered_link,
    };
    verify_ready_ownership(host_pid, &ready, guardian_ready_after);
    ready
}

// Arm member-based emergency group cleanup while the descendant is live and
// still the leader's direct child in the original group.
fn arm_member_cleanup(cleanup: &mut CycleCleanup, descendant: Option<u32>, leader: u32) -> u32 {
    let descendant = descendant.expect("descendant ready");
    cleanup.descendant_birth = Some(retain_group_member_birth(descendant, leader, leader));
    descendant
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

#[cfg(test)]
mod strict_event_shape_tests {
    use super::super::support::{Fixture, send_event};
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn trailing_fields_are_not_accepted_as_ready_control_events() {
        let fixture = Fixture::new();
        let listener = UnixListener::bind(&fixture.control).expect("own private control socket");
        listener
            .set_nonblocking(true)
            .expect("nonblocking fixture event accept");
        let host = std::process::Command::new("/usr/bin/true")
            .spawn()
            .expect("own harmless native host placeholder");
        let host_pid = host.id();
        let mut cleanup = CycleCleanup::new(host);

        // FIFO completed Unix connections make the first malformed record
        // observable without changing any shipping Guardian role. The second
        // poison record forces an immediate distinct error on the old parser,
        // avoiding an artificial 10-second fixture timeout as a false RED.
        // This extra field stands in for a sensitive private app-link token;
        // a fail-closed controller parser must never echo its raw contents.
        send_event(
            &fixture.control,
            "LINK_BOUND private-token-must-be-redacted",
        );
        send_event(&fixture.control, "POISON_AFTER_MALFORMED_EVENT");
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            await_ready(
                &listener,
                &fixture.control,
                host_pid,
                Instant::now(),
                &mut cleanup,
                None,
                false,
            )
        }));
        let panic = outcome
            .err()
            .expect("malformed readiness event must be rejected");
        let detail = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("non-string panic");
        assert!(
            detail.contains("unexpected trailing readiness event fields"),
            "malformed first record must fail on its extra token, not on a later poison event: {detail}"
        );
        assert!(
            !detail.contains("private-token-must-be-redacted"),
            "malformed event diagnostic revealed a private fixture field"
        );
    }

    #[test]
    fn invalid_pid_and_unknown_event_do_not_echo_private_fields() {
        const PRIVATE: &str = "private-token-must-not-appear";
        for record in [
            "LEADER private-token-must-not-appear",
            "UNKNOWN private-token-must-not-appear",
        ] {
            let fixture = Fixture::new();
            let listener = UnixListener::bind(&fixture.control).expect("own private event socket");
            listener
                .set_nonblocking(true)
                .expect("nonblocking event accept");
            let host = std::process::Command::new("/usr/bin/true")
                .spawn()
                .expect("own harmless host placeholder");
            let host_pid = host.id();
            let mut cleanup = CycleCleanup::new(host);
            send_event(&fixture.control, record);
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                await_ready(
                    &listener,
                    &fixture.control,
                    host_pid,
                    Instant::now(),
                    &mut cleanup,
                    None,
                    false,
                )
            }));
            let panic = outcome.err().expect("invalid role event must be refused");
            let detail = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic");
            assert!(
                !detail.contains(PRIVATE),
                "untrusted control field leaked into rejected-event diagnostic"
            );
        }
    }

    #[test]
    fn surplus_authority_bearing_records_cannot_create_cleanup_witnesses_or_peers() {
        for (record, event) in [
            ("GUARDIAN 4294967294 4294967293 surplus", "GUARDIAN"),
            ("LEAKER 4294967294 surplus", "LEAKER"),
            ("APP_LINK private-test-link surplus", "APP_LINK"),
        ] {
            let fixture = Fixture::new();
            let listener = UnixListener::bind(&fixture.control).expect("own readiness socket");
            listener
                .set_nonblocking(true)
                .expect("nonblocking role-event listener");
            let host = std::process::Command::new("/usr/bin/true")
                .spawn()
                .expect("own native host placeholder");
            let host_pid = host.id();
            let mut cleanup = CycleCleanup::new(host);
            send_event(&fixture.control, record);
            // Broken arity checks fail on a DIFFERENT poison record (or on
            // later birth validation), never on the original surplus bytes.
            send_event(&fixture.control, "POISON_AFTER_AUTHORITY_RECORD");
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                await_ready(
                    &listener,
                    &fixture.control,
                    host_pid,
                    Instant::now(),
                    &mut cleanup,
                    None,
                    false,
                )
            }));
            let panic = outcome.err().expect("surplus role record must be refused");
            let detail = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic");
            assert!(
                detail.contains("unexpected trailing readiness event fields"),
                "{event} was not rejected at the original field-count gate: {detail}"
            );
            assert!(
                cleanup.group_birth.is_none()
                    && cleanup.group.is_none()
                    && cleanup.leaker_birth.is_none()
                    && cleanup.leaker.is_none()
                    && cleanup.peer.is_none(),
                "{event} minted native cleanup or child authority before framing validation"
            );
        }
    }
}
