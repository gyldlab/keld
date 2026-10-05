//! Real Windows process-tree proof for the KEL-78/T3 host-death Job.

#![cfg(windows)]
#![allow(unsafe_code)] // isolated test-only process-handle observation with local ABI proofs
#![allow(clippy::expect_used, clippy::panic)] // process fixture invariants must abort loudly
#![deny(unsafe_op_in_unsafe_fn)]

use std::env;
use std::fs::OpenOptions;
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::{FromRawHandle as _, OwnedHandle};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use keld_ipc::{
    WindowsLifecycleBinding, WindowsLifecycleExpectation, WindowsLifecyclePurpose,
    WindowsLifecycleRendezvousListener, WindowsLifecycleRendezvousPeer,
};
use keld_runtime::windows_job::{
    WINDOWS_DEV_STAGE_CLEANUP_RELEASE_V1, WINDOWS_LAUNCH_GATE_ATTEMPT_JOB_V1,
    WINDOWS_LAUNCH_GATE_ENV, WindowsLifecycleKeeperHandoff, WindowsLifecycleRetirementPending,
    WindowsProcessJob, WindowsProcessPeer, accept_host_start_v1, install_host_death_job,
    release_host_start_v1,
};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

const HELPER_ENV: &str = "KELD_WINDOWS_JOB_HELPER";
const HELPER_TEST: &str = "windows_job_process_helper";
const PROCESS_WAIT_MS: u32 = 10_000;

#[test]
fn abnormal_host_death_reaps_direct_child_and_descendant_then_relaunches() {
    let mut host = spawn_helper("host");
    let stdout = host.stdout.take().expect("host stdout pipe");
    let mut lines = BufReader::new(stdout).lines();

    let observation = next_prefixed_line(&mut lines, "JOB ");
    assert!(
        observation.contains("limits=0x00002000"),
        "Job must report only KILL_ON_JOB_CLOSE: {observation}"
    );
    assert!(
        observation.contains("assigned=true") && observation.contains("inheritable=false"),
        "Job assignment and non-inheritance must be observed: {observation}"
    );

    let direct_pid = parse_pid(&next_prefixed_line(&mut lines, "DIRECT "), "DIRECT");
    let descendant_pid = parse_pid(&next_prefixed_line(&mut lines, "DESCENDANT "), "DESCENDANT");
    let direct = open_process_for_wait(direct_pid);
    let descendant = open_process_for_wait(descendant_pid);

    // Child::kill terminates this one host process. It does not request tree
    // termination, so the two retained process handles independently observe
    // whether the Job kernel contract reaped both enrolled descendants.
    host.kill()
        .expect("terminate only the host fixture process");
    let _ = host.wait().expect("wait for terminated host fixture");
    assert_process_exited(&direct, "direct child");
    assert_process_exited(&descendant, "descendant");

    let relaunch = spawn_helper("relaunch")
        .wait_with_output()
        .expect("run post-cleanup launch");
    assert!(
        relaunch.status.success(),
        "post-cleanup launch failed: status={} stderr={}",
        relaunch.status,
        String::from_utf8_lossy(&relaunch.stderr)
    );
    assert!(
        String::from_utf8_lossy(&relaunch.stdout).contains("RELAUNCH_OK"),
        "post-cleanup launch did not report success: {}",
        String::from_utf8_lossy(&relaunch.stdout)
    );
}

#[test]
fn launcher_job_assignment_gates_the_host_and_reaps_the_full_attempt() {
    let mut attempt = WindowsProcessJob::create().expect("create launcher attempt Job");
    let mut host = spawn_host_attempt_gate();
    let stdout = host.stdout.take().expect("host stdout pipe");
    let mut lines = BufReader::new(stdout).lines();
    if let Err(error) = attempt.assign_child(&host) {
        drop(host.stdin.take());
        let _ = host.wait();
        panic!("assign blocked host before releasing application startup: {error}");
    }
    assert!(
        attempt.contains_child(&host).expect("exact Job membership"),
        "the host must be in the launcher-owned attempt Job"
    );
    let mut start_writer = host.stdin.take().expect("host start writer");
    release_host_start_v1(&mut start_writer).expect("release exact host startup gate");

    let ready = next_prefixed_line(&mut lines, "ATTEMPT ");
    assert!(ready.contains("inner_job=true"), "{ready}");
    assert_eq!(
        next_prefixed_line(&mut lines, "APP_RESOURCE_STARTED"),
        "APP_RESOURCE_STARTED"
    );
    let direct_pid = parse_pid(&next_prefixed_line(&mut lines, "DIRECT "), "DIRECT");
    let descendant_pid = parse_pid(&next_prefixed_line(&mut lines, "DESCENDANT "), "DESCENDANT");
    let direct = open_process_for_wait(direct_pid);
    let descendant = open_process_for_wait(descendant_pid);
    assert!(
        attempt.active_processes().expect("attempt Job census") >= 3,
        "host, direct Bun stand-in, and descendant must all be enrolled"
    );

    attempt
        .terminate_and_wait(&host, std::time::Duration::from_secs(10))
        .expect("terminate attempt and prove zero active members");
    drop(start_writer);
    let _ = host.wait().expect("wait for direct host");
    assert_process_exited(&direct, "attempt direct child");
    assert_process_exited(&descendant, "attempt descendant");

    let relaunch = spawn_helper("relaunch")
        .wait_with_output()
        .expect("run next attempt after old Job is empty");
    assert!(
        relaunch.status.success(),
        "next launch failed: status={} stderr={}",
        relaunch.status,
        String::from_utf8_lossy(&relaunch.stderr)
    );
}

#[test]
fn unnamed_attempt_job_transfers_to_exact_keeper_process_without_a_name_lookup() {
    let mut attempt = WindowsProcessJob::create().expect("create exact unnamed attempt Job");
    let mut host = spawn_host_attempt_gate();
    attempt
        .assign_child(&host)
        .expect("assign exact host before keeper transfer");

    let mut keeper_command = Command::new(env::current_exe().expect("test executable"));
    keeper_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "lifecycle-receiver")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut keeper = keeper_command
        .spawn()
        .expect("spawn keeper outside attempt Job");
    let keeper_stdout = keeper.stdout.take().expect("keeper stdout");
    let mut keeper_lines = BufReader::new(keeper_stdout).lines();
    let mut keeper_session = 0_u32;
    // SAFETY: the keeper child is live and its PID comes from Child.
    assert_ne!(
        unsafe { ProcessIdToSessionId(keeper.id(), &raw mut keeper_session) },
        0
    );
    let keeper_peer =
        keld_runtime::windows_job::WindowsProcessPeer::open(keeper.id(), keeper_session)
            .expect("pin exact keeper process");
    let remote_handle = attempt
        .transfer_lifecycle_handle_to(&keeper_peer)
        .expect("duplicate only QUERY|TERMINATE into exact keeper");

    let mut coordinator_session = 0_u32;
    // SAFETY: this test process is the live coordinator and the output is writable.
    assert_ne!(
        unsafe { ProcessIdToSessionId(std::process::id(), &raw mut coordinator_session) },
        0
    );
    writeln!(
        keeper.stdin.as_mut().expect("keeper stdin"),
        "HANDLE {remote_handle} COORDINATOR {} {coordinator_session}",
        std::process::id()
    )
    .expect("send target-process handle value over private test pipe");
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "LIFECYCLE_KEEPER_READY"),
        "LIFECYCLE_KEEPER_READY active=1"
    );
    assert_eq!(
        attempt.active_processes().expect("coordinator Job query"),
        1
    );
    writeln!(keeper.stdin.as_mut().expect("keeper stdin"), "EXIT")
        .expect("stop keeper test fixture");
    drop(keeper.stdin.take());
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "LIFECYCLE_KEEPER_EXIT"),
        "LIFECYCLE_KEEPER_EXIT"
    );
    let status = keeper.wait().expect("wait keeper process");
    assert!(status.success(), "keeper helper failed: {status}");
    attempt
        .terminate_and_wait(&host, std::time::Duration::from_secs(10))
        .expect("coordinator terminates and proves exact Job zero");
    let _ = host.wait().expect("wait terminated Job host");
}

#[test]
fn activation_lease_retention_transfers_to_keeper_and_survives_coordinator_drop() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt as _;
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Foundation::DuplicateHandle;
    use windows_sys::Win32::Storage::FileSystem::{FILE_READ_ATTRIBUTES, SYNCHRONIZE};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let fixture = tempfile::tempdir().expect("lease transfer fixture");
    let path = fixture.path().join("activation.lock");
    let writer = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(&path)
        .expect("open stable share-zero lease");

    let mut attempt = WindowsProcessJob::create().expect("create exact attempt Job");
    let mut member = spawn_host_attempt_gate();
    attempt
        .assign_child(&member)
        .expect("assign exact attempt member");
    let mut keeper_command = Command::new(env::current_exe().expect("test executable"));
    keeper_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "lease-receiver")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut keeper = keeper_command
        .spawn()
        .expect("spawn keeper outside attempt Job");
    let keeper_stdout = keeper.stdout.take().expect("keeper stdout");
    let mut keeper_lines = BufReader::new(keeper_stdout).lines();
    let mut keeper_session = 0_u32;
    // SAFETY: keeper is live and the PID came from Child.
    assert_ne!(
        unsafe { ProcessIdToSessionId(keeper.id(), &raw mut keeper_session) },
        0
    );
    let keeper_peer =
        keld_runtime::windows_job::WindowsProcessPeer::open(keeper.id(), keeper_session)
            .expect("pin exact keeper process");

    let mut reduced = std::ptr::null_mut();
    // SAFETY: source is the exact share-zero activation.lock object; the local
    // duplicate receives only FILE_READ_ATTRIBUTES and SYNCHRONIZE.
    assert_ne!(
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                writer.as_raw_handle().cast(),
                GetCurrentProcess(),
                &raw mut reduced,
                FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                0,
                0,
            )
        },
        0
    );
    // SAFETY: successful DuplicateHandle returned one fresh owned handle.
    let reduced = unsafe { OwnedHandle::from_raw_handle(reduced.cast()) };
    let remote_handle = attempt
        .transfer_activation_lease_handle_to(&keeper_peer, &reduced)
        .expect("transfer only read-attributes/synchronize lease rights");
    drop(writer);
    drop(reduced);
    assert!(
        OpenOptions::new().read(true).open(&path).is_err(),
        "keeper's remote same-object handle must retain share-zero exclusion"
    );

    writeln!(
        keeper.stdin.as_mut().expect("keeper stdin"),
        "LEASE {remote_handle}"
    )
    .expect("deliver exact target-process lease handle");
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "LIFECYCLE_LEASE_KEEPER_READY"),
        "LIFECYCLE_LEASE_KEEPER_READY"
    );
    assert!(
        OpenOptions::new().read(true).open(&path).is_err(),
        "keeper retention must continue excluding readers after coordinator handles drop"
    );
    writeln!(keeper.stdin.as_mut().expect("keeper stdin"), "EXIT")
        .expect("release keeper lease retention");
    drop(keeper.stdin.take());
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "LIFECYCLE_LEASE_KEEPER_EXIT"),
        "LIFECYCLE_LEASE_KEEPER_EXIT"
    );
    assert!(
        keeper.wait().expect("wait keeper").success(),
        "keeper should close its exact remote handle"
    );
    assert!(
        OpenOptions::new().read(true).open(&path).is_ok(),
        "closing the final keeper duplicate releases the share-zero lease"
    );
    attempt
        .terminate_and_wait(&member, std::time::Duration::from_secs(10))
        .expect("retire attempt Job after the lock-retention test");
    let _ = member.wait().expect("wait attempt member");
}

#[expect(
    clippy::too_many_lines,
    reason = "real Windows subprocess proves authenticated Job+lease bundle transfer as one gate"
)]
#[test]
fn authenticated_one_shot_handoff_couples_attempt_job_and_writer_lease() {
    let fixture = tempfile::tempdir().expect("authenticated lifecycle handoff fixture");
    let lock_path = fixture.path().join("activation.lock");
    let writer = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&lock_path)
        .expect("open exact share-zero activation lock");

    let mut attempt = WindowsProcessJob::create().expect("create unnamed attempt Job");
    let mut member = spawn_host_attempt_gate();
    attempt
        .assign_child(&member)
        .expect("assign exact attempt family member");

    let binding = lifecycle_binding();
    let listener = WindowsLifecycleRendezvousListener::bind([0x79; 32], binding)
        .expect("bind dedicated one-shot lifecycle endpoint");
    let endpoint = listener.endpoint().to_owned();
    let mut server_session = 0_u32;
    // SAFETY: this live coordinator PID and writable session output are valid.
    assert_ne!(
        unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
        0
    );
    let expected_image = env::current_exe()
        .expect("test executable")
        .canonicalize()
        .expect("canonical coordinator image");
    let mut keeper_command = Command::new(env::current_exe().expect("test executable"));
    keeper_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "authenticated-attempt-keeper")
        .env("KELD_TEST_LIFECYCLE_ENDPOINT", &endpoint)
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_PID",
            std::process::id().to_string(),
        )
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_SESSION",
            server_session.to_string(),
        )
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_IMAGE",
            expected_image.as_os_str(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut keeper = keeper_command
        .spawn()
        .expect("spawn bounded keeper outside attempt Job");
    let keeper_stdout = keeper.stdout.take().expect("keeper stdout");
    let mut keeper_lines = BufReader::new(keeper_stdout).lines();
    let mut keeper_session = 0_u32;
    // SAFETY: keeper is live and the session output is writable.
    assert_ne!(
        unsafe { ProcessIdToSessionId(keeper.id(), &raw mut keeper_session) },
        0
    );
    let accepted = listener
        .accept_until(
            std::time::Instant::now() + std::time::Duration::from_secs(5),
            |pid, session, facts| {
                if session != keeper_session
                    || session != server_session
                    || facts.session_id != session
                {
                    return None;
                }
                let peer = WindowsProcessPeer::open(pid, session).ok()?;
                if peer.image_path().canonicalize().ok()? != expected_image
                    || peer.token_facts() != facts
                {
                    return None;
                }
                Some(peer)
            },
        )
        .expect("authenticate keeper and exact LC1/LA1/LR1 transcript")
        .expect("authenticated keeper connected before deadline");
    assert_eq!(accepted.process_id(), keeper.id());
    assert_eq!(accepted.binding(), binding);

    let retention_source: OwnedHandle = writer
        .try_clone()
        .expect("clone exact activation.lock object")
        .into();
    attempt
        .transfer_lifecycle_attempt_handoff(
            accepted,
            &retention_source,
            std::time::Instant::now() + std::time::Duration::from_secs(5),
        )
        .expect("transfer Job+lease bundle and receive keeper adoption ACK");
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "ATTEMPT_KEEPER_READY "),
        "ATTEMPT_KEEPER_READY active=1"
    );

    let member_process = open_process_for_wait(member.id());
    assert_process_live(
        &member_process,
        "attempt member while coordinator and keeper own the exact Job",
    );
    assert!(
        OpenOptions::new().read(true).open(&lock_path).is_err(),
        "the authenticated Job+lease handoff must preserve share-zero exclusion"
    );

    writeln!(keeper.stdin.as_mut().expect("keeper stdin"), "CRASH")
        .expect("crash adopted keeper before retirement proof");
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "ATTEMPT_KEEPER_CRASH_ARMED"),
        "ATTEMPT_KEEPER_CRASH_ARMED"
    );
    drop(keeper.stdin.take());
    assert!(
        !keeper.wait().expect("reap crashed keeper").success(),
        "the adopted keeper must terminate abnormally after its crash marker"
    );
    assert_process_live(&member_process, "attempt member after keeper crash");
    assert_eq!(
        attempt.active_processes().expect("coordinator Job census"),
        1,
        "keeper death alone is not proof that the exact Job family reached zero"
    );
    assert!(
        OpenOptions::new().read(true).open(&lock_path).is_err(),
        "keeper death must not release the coordinator's still-owned writer lease"
    );
    attempt
        .terminate_and_wait(&member, std::time::Duration::from_secs(10))
        .expect("coordinator observes retirement of its exact Job after keeper death");
    drop(writer);
    drop(retention_source);
    assert_process_exited(
        &member_process,
        "attempt member after coordinator retirement",
    );
    assert!(
        OpenOptions::new().read(true).open(&lock_path).is_ok(),
        "only coordinator lease closure releases share-zero after keeper death"
    );
    let _ = member
        .wait()
        .expect("reap attempt member after coordinator retirement");
}

#[test]
fn keeper_releases_writer_only_after_authenticated_successor_zero_ack() {
    run_keeper_retirement_case(true);
}

#[test]
fn keeper_retains_writer_when_successor_disconnects_before_qa1() {
    run_keeper_retirement_case(false);
}

#[expect(
    clippy::too_many_lines,
    reason = "success and pre-QA1 disconnect share one real three-process retirement fixture"
)]
fn run_keeper_retirement_case(successor_acknowledged: bool) {
    let fixture = tempfile::tempdir().expect("zero-ack lifecycle fixture");
    let lock_path = fixture.path().join("activation.lock");
    std::fs::write(&lock_path, []).expect("create empty activation lock");
    let expected_image = env::current_exe()
        .expect("test executable")
        .canonicalize()
        .expect("canonical test image");
    let locator_byte = if successor_acknowledged { 0x78 } else { 0x7b };
    let locator = [locator_byte; 32];
    let endpoint =
        keld_ipc::WindowsNamedPipeBootstrapStream::endpoint_for_lifecycle_install(&locator);

    let mut coordinator_command = Command::new(env::current_exe().expect("test executable"));
    coordinator_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "authenticated-attempt-coordinator")
        .env(
            "KELD_TEST_LIFECYCLE_LOCATOR_BYTE",
            format!("{locator_byte:02x}"),
        )
        .env("KELD_TEST_ACTIVATION_LOCK", &lock_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut coordinator = coordinator_command
        .spawn()
        .expect("spawn real coordinator owner");
    let coordinator_stdout = coordinator.stdout.take().expect("coordinator stdout");
    let mut coordinator_lines = BufReader::new(coordinator_stdout).lines();
    let coordinator_ready = next_prefixed_line(&mut coordinator_lines, "COORDINATOR_LISTENING ");
    let target_pid = parse_pid(&coordinator_ready, "COORDINATOR_LISTENING");
    let target = open_process_for_wait(target_pid);
    let mut coordinator_session = 0_u32;
    // SAFETY: coordinator is live and the session output is writable.
    assert_ne!(
        unsafe { ProcessIdToSessionId(coordinator.id(), &raw mut coordinator_session) },
        0
    );

    let mut keeper_command = Command::new(env::current_exe().expect("test executable"));
    keeper_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "authenticated-attempt-keeper")
        .env(
            "KELD_TEST_LIFECYCLE_LOCATOR_BYTE",
            format!("{locator_byte:02x}"),
        )
        .env("KELD_TEST_LIFECYCLE_ENDPOINT", &endpoint)
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_PID",
            coordinator.id().to_string(),
        )
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_SESSION",
            coordinator_session.to_string(),
        )
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_IMAGE",
            expected_image.as_os_str(),
        )
        .env("KELD_TEST_ACTIVATION_LOCK", &lock_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut keeper = keeper_command.spawn().expect("spawn exact keeper");
    let mut keeper_session = 0_u32;
    // SAFETY: keeper is live and the session output is writable.
    assert_ne!(
        unsafe { ProcessIdToSessionId(keeper.id(), &raw mut keeper_session) },
        0
    );
    writeln!(
        coordinator
            .stdin
            .as_mut()
            .expect("coordinator control pipe"),
        "KEEPER {} {}",
        keeper.id(),
        keeper_session
    )
    .expect("authorize exact keeper fixture PID/session");

    let keeper_stdout = keeper.stdout.take().expect("keeper stdout");
    let mut keeper_lines = BufReader::new(keeper_stdout).lines();
    assert_eq!(
        next_prefixed_line(&mut coordinator_lines, "COORDINATOR_HANDOFF_READY "),
        format!("COORDINATOR_HANDOFF_READY target={target_pid}")
    );
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "ATTEMPT_KEEPER_READY "),
        "ATTEMPT_KEEPER_READY active=1"
    );
    assert_process_live(&target, "candidate family before coordinator death");
    assert!(
        OpenOptions::new().read(true).open(&lock_path).is_err(),
        "authenticated keeper retains the exact writer exclusion"
    );

    coordinator
        .kill()
        .expect("abruptly terminate coordinator that held full Job/lease owners");
    assert!(
        !coordinator
            .wait()
            .expect("wait coordinator death")
            .success(),
        "coordinator must die abnormally for the keeper-death proof"
    );
    assert_process_live(
        &target,
        "keeper's exact Job reference survives coordinator death",
    );
    writeln!(
        keeper.stdin.as_mut().expect("keeper control pipe"),
        "RETIRE"
    )
    .expect("authorize successor retirement phase");
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "KEEPER_RETIRE_LISTENER_READY"),
        "KEEPER_RETIRE_LISTENER_READY"
    );
    assert!(
        OpenOptions::new().read(true).open(&lock_path).is_err(),
        "keeper retains the writer lease while no zero-witness successor has acknowledged"
    );

    let mut successor_command = Command::new(env::current_exe().expect("test executable"));
    successor_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "authenticated-attempt-successor")
        .env("KELD_TEST_LIFECYCLE_ENDPOINT", &endpoint)
        .env("KELD_TEST_LIFECYCLE_SERVER_PID", keeper.id().to_string())
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_SESSION",
            keeper_session.to_string(),
        )
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_IMAGE",
            expected_image.as_os_str(),
        )
        .env("KELD_TEST_ACTIVATION_LOCK", &lock_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut successor = successor_command
        .spawn()
        .expect("spawn independent successor");
    let mut successor_lines =
        BufReader::new(successor.stdout.take().expect("successor stdout")).lines();
    assert_eq!(
        next_prefixed_line(&mut successor_lines, "SUCCESSOR_ZERO_BEFORE_ACK "),
        "SUCCESSOR_ZERO_BEFORE_ACK active=0"
    );
    assert!(
        OpenOptions::new().read(true).open(&lock_path).is_err(),
        "keeper must retain the writer lease after successor observes zero but before QA1"
    );
    assert_process_exited(
        &target,
        "exact attempt family after successor zero observation",
    );
    if !successor_acknowledged {
        writeln!(
            successor.stdin.as_mut().expect("successor QA1 gate"),
            "ABORT"
        )
        .expect("disconnect successor before sending QA1");
        assert_eq!(
            next_prefixed_line(&mut successor_lines, "SUCCESSOR_ABORTED_BEFORE_QA1"),
            "SUCCESSOR_ABORTED_BEFORE_QA1"
        );
        drop(successor.stdin.take());
        assert!(
            successor
                .wait()
                .expect("wait disconnected successor")
                .success(),
            "the test successor exits normally before acknowledging"
        );
        assert!(
            OpenOptions::new().read(true).open(&lock_path).is_err(),
            "a successor disconnect before QA1 must leave writer exclusion with the keeper"
        );
        let keeper_failure = next_prefixed_line(&mut keeper_lines, "KEEPER_RETIRE_FAILED ");
        assert!(
            keeper_failure.starts_with("KEEPER_RETIRE_FAILED "),
            "keeper must report the unacknowledged retirement cut: {keeper_failure}"
        );
        assert!(
            keeper_failure.contains("lifecycle successor zero acknowledgement"),
            "keeper must fail specifically while awaiting QA1: {keeper_failure}"
        );
        assert!(
            OpenOptions::new().read(true).open(&lock_path).is_err(),
            "keeper must still retain writer exclusion after reporting the pre-QA1 failure"
        );
        writeln!(keeper.stdin.as_mut().expect("keeper control pipe"), "EXIT")
            .expect("release keeper after failed retirement proof");
        drop(keeper.stdin.take());
        assert_eq!(
            next_prefixed_line(&mut keeper_lines, "KEEPER_FAILED_RETIRE_EXIT"),
            "KEEPER_FAILED_RETIRE_EXIT"
        );
        assert!(
            keeper
                .wait()
                .expect("wait keeper after failed retirement")
                .success(),
            "keeper failure cleanup exits cleanly"
        );
        assert!(
            OpenOptions::new().read(true).open(&lock_path).is_ok(),
            "writer exclusion releases only when the final keeper owner exits"
        );
        return;
    }
    writeln!(successor.stdin.as_mut().expect("successor QA1 gate"), "ACK")
        .expect("release exact zero-ack barrier");
    assert_eq!(
        next_prefixed_line(&mut successor_lines, "SUCCESSOR_WRITER_ACQUIRED "),
        "SUCCESSOR_WRITER_ACQUIRED active=0 pending-journal-revalidation"
    );
    assert!(
        OpenOptions::new().read(true).open(&lock_path).is_err(),
        "successor's acquired share-zero writer excludes a second writer"
    );
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "KEEPER_RETIRED"),
        "KEEPER_RETIRED"
    );
    assert!(keeper.wait().expect("wait retired keeper").success());

    writeln!(
        successor.stdin.as_mut().expect("successor release gate"),
        "EXIT"
    )
    .expect("release successor writer fixture");
    drop(successor.stdin.take());
    assert!(successor.wait().expect("wait successor").success());
    assert!(
        OpenOptions::new().read(true).open(&lock_path).is_ok(),
        "writer exclusion releases only after successor closes its lease"
    );
}

#[expect(
    clippy::too_many_lines,
    reason = "subprocess negative proves truncated bundles terminate the receiver before unknown handles leak"
)]
#[test]
fn partial_authenticated_bundle_terminates_keeper_and_closes_unknown_handles() {
    let fixture = tempfile::tempdir().expect("partial handle-bundle fixture");
    let lock_path = fixture.path().join("activation.lock");
    let writer = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&lock_path)
        .expect("open exact share-zero activation lock");
    let retention_source: OwnedHandle = writer
        .try_clone()
        .expect("clone activation lock handle")
        .into();
    let mut attempt = WindowsProcessJob::create().expect("create exact attempt Job");
    let mut member = spawn_host_attempt_gate();
    attempt
        .assign_child(&member)
        .expect("assign candidate family member");
    let member_process = open_process_for_wait(member.id());
    let binding = lifecycle_binding();
    let listener = WindowsLifecycleRendezvousListener::bind([0x7a; 32], binding)
        .expect("bind malformed-bundle endpoint");
    let endpoint = listener.endpoint().to_owned();
    let mut server_session = 0_u32;
    // SAFETY: this live coordinator PID and writable session output are valid.
    assert_ne!(
        unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
        0
    );
    let expected_image = env::current_exe()
        .expect("test executable")
        .canonicalize()
        .expect("canonical coordinator image");
    let mut keeper_command = Command::new(env::current_exe().expect("test executable"));
    keeper_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "malformed-attempt-keeper")
        .env("KELD_TEST_LIFECYCLE_ENDPOINT", &endpoint)
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_PID",
            std::process::id().to_string(),
        )
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_SESSION",
            server_session.to_string(),
        )
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_IMAGE",
            expected_image.as_os_str(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let keeper = keeper_command
        .spawn()
        .expect("spawn dedicated malformed-bundle receiver");
    let mut keeper_session = 0_u32;
    // SAFETY: keeper is live and the session output is writable.
    assert_ne!(
        unsafe { ProcessIdToSessionId(keeper.id(), &raw mut keeper_session) },
        0
    );
    let mut peer = listener
        .accept_until(
            std::time::Instant::now() + std::time::Duration::from_secs(5),
            |pid, session, facts| {
                if pid != keeper.id()
                    || session != keeper_session
                    || session != server_session
                    || facts.session_id != session
                {
                    return None;
                }
                let pin = WindowsProcessPeer::open(pid, session).ok()?;
                if pin.image_path().canonicalize().ok()? != expected_image
                    || pin.token_facts() != facts
                {
                    return None;
                }
                Some(pin)
            },
        )
        .expect("authenticate malformed-bundle receiver")
        .expect("receiver completed the one-shot context handshake");
    let _remote_job = attempt
        .transfer_lifecycle_handle_to(peer.process_pin())
        .expect("duplicate exact Job before the incomplete bundle record");
    let _remote_lease = attempt
        .transfer_activation_lease_handle_to(peer.process_pin(), &retention_source)
        .expect("duplicate reduced lock before the incomplete bundle record");
    peer.stream_mut()
        .write_all(b"KELD-HO1")
        .expect("send only the fixed record magic before coordinator failure");
    drop(peer);
    let output = keeper
        .wait_with_output()
        .expect("wait fail-stop keeper after truncated offer");
    assert!(
        !output.status.success(),
        "partial HO1 must terminate the dedicated keeper to close unknown handles"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("KELD-RUNTIME-014"),
        "fail-stop cleanup must identify the runtime boundary failure"
    );
    drop(retention_source);
    drop(writer);
    drop(attempt);
    assert_process_exited(
        &member_process,
        "Job family after malformed keeper process closes unknown Job handle",
    );
    assert!(
        OpenOptions::new().read(true).open(&lock_path).is_ok(),
        "keeper termination closes the undisclosed activation-lock handle"
    );
    let _ = member.wait().expect("reap member after keeper fail-stop");
}

#[test]
fn keeper_transfer_refuses_a_target_inside_the_attempt_job() {
    let mut attempt = WindowsProcessJob::create().expect("create exact unnamed attempt Job");
    let mut member = spawn_host_attempt_gate();
    attempt
        .assign_child(&member)
        .expect("assign process before keeper-transfer negative control");
    let mut session_id = 0_u32;
    // SAFETY: the attempt member is live and its PID came from Child.
    assert_ne!(
        unsafe { ProcessIdToSessionId(member.id(), &raw mut session_id) },
        0
    );
    let peer = keld_runtime::windows_job::WindowsProcessPeer::open(member.id(), session_id)
        .expect("pin attempt member");
    let error = attempt
        .transfer_lifecycle_handle_to(&peer)
        .expect_err("keeper must remain outside the attempt Job");
    assert!(
        error.to_string().contains("inside the attempt Job"),
        "unexpected transfer refusal: {error}"
    );
    attempt
        .terminate_and_wait(&member, std::time::Duration::from_secs(10))
        .expect("clean up attempt after target-membership refusal");
    let _ = member.wait().expect("wait refused keeper target");
}

#[test]
fn lifecycle_rendezvous_pins_exact_image_token_and_nonce_peer() {
    let locator = [0x71; 32];
    let binding = lifecycle_binding();
    let listener = WindowsLifecycleRendezvousListener::bind(locator, binding)
        .expect("bind stable install/user lifecycle endpoint");
    let endpoint = listener.endpoint().to_owned();
    let expected_image = env::current_exe()
        .expect("test executable")
        .canonicalize()
        .expect("canonical test executable");
    let mut server_session = 0_u32;
    // SAFETY: this live coordinator PID and writable session output are valid.
    assert_ne!(
        unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
        0
    );

    let mut command = Command::new(env::current_exe().expect("test executable"));
    command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "lifecycle-client")
        .env("KELD_TEST_LIFECYCLE_ENDPOINT", &endpoint)
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_PID",
            std::process::id().to_string(),
        )
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_SESSION",
            server_session.to_string(),
        )
        .env(
            "KELD_TEST_LIFECYCLE_SERVER_IMAGE",
            expected_image.as_os_str(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut client = command.spawn().expect("spawn exact lifecycle client");
    let client_stdout = client.stdout.take().expect("client stdout");
    let mut client_lines = BufReader::new(client_stdout).lines();
    let mut server_peer = listener
        .accept_until(
            std::time::Instant::now() + std::time::Duration::from_secs(5),
            |pid, session, facts| {
                if session != server_session || facts.session_id != session {
                    return None;
                }
                let peer =
                    keld_runtime::windows_job::WindowsProcessPeer::open(pid, session).ok()?;
                if peer.image_path().canonicalize().ok()? != expected_image
                    || peer.token_facts() != facts
                {
                    return None;
                }
                Some(peer)
            },
        )
        .expect("accept and authenticate connected process/token")
        .expect("valid same-install client connects before deadline");
    assert_eq!(server_peer.binding(), binding);

    assert_authenticated_lifecycle_peer(
        &mut server_peer,
        client.id(),
        server_session,
        &expected_image,
        &mut client_lines,
    );

    writeln!(client.stdin.as_mut().expect("client stdin"), "EXIT")
        .expect("release authenticated client fixture");
    drop(client.stdin.take());
    let status = client.wait().expect("wait lifecycle client");
    let stderr = client
        .stderr
        .take()
        .map(io::read_to_string)
        .transpose()
        .expect("read lifecycle client stderr")
        .unwrap_or_default();
    assert!(
        status.success(),
        "lifecycle client failed: {status}; {stderr}"
    );
    assert!(
        server_peer
            .process_pin()
            .has_exited()
            .expect("client exit pin")
    );
}

fn assert_authenticated_lifecycle_peer(
    peer: &mut WindowsLifecycleRendezvousPeer<WindowsProcessPeer>,
    client_pid: u32,
    session_id: u32,
    expected_image: &std::path::Path,
    client_lines: &mut impl Iterator<Item = io::Result<String>>,
) {
    assert_eq!(peer.process_id(), client_pid);
    assert_eq!(peer.session_id(), session_id);
    assert_eq!(
        peer.process_pin()
            .image_path()
            .canonicalize()
            .expect("peer image path"),
        expected_image
    );
    assert_eq!(
        peer.process_pin().token_facts(),
        peer.token_facts(),
        "impersonated last-writer token must match the retained process token"
    );
    assert_ne!(peer.client_nonce(), peer.server_nonce());
    assert!(
        !peer.process_pin().has_exited().expect("peer liveness"),
        "the retained peer process object must still be live"
    );
    assert_eq!(
        next_prefixed_line(client_lines, "LIFECYCLE_CLIENT_PINNED"),
        format!(
            "LIFECYCLE_CLIENT_PINNED server={} client={}",
            std::process::id(),
            client_pid
        )
    );
    assert!(
        !peer
            .stream_mut()
            .is_handle_inheritable()
            .expect("server pipe flags")
    );
}

#[test]
fn keeper_reaps_exact_job_after_coordinator_death_and_reports_zero() {
    let mut coordinator = spawn_helper("lifecycle-coordinator");
    let stdout = coordinator.stdout.take().expect("coordinator stdout");
    let mut coordinator_lines = BufReader::new(stdout).lines();
    let ready = next_prefixed_line(&mut coordinator_lines, "LIFECYCLE_COORDINATOR_READY ");
    let mut fields = ready.split_ascii_whitespace();
    assert_eq!(fields.next(), Some("LIFECYCLE_COORDINATOR_READY"));
    let target_pid = fields
        .next()
        .expect("target PID field")
        .strip_prefix("target=")
        .expect("target field prefix")
        .parse::<u32>()
        .expect("valid target PID");
    let keeper_pid = fields
        .next()
        .expect("keeper PID field")
        .strip_prefix("keeper=")
        .expect("keeper field prefix")
        .parse::<u32>()
        .expect("valid keeper PID");
    assert_eq!(fields.next(), None, "coordinator readiness record is exact");
    let target = open_process_for_wait(target_pid);
    let keeper_process = open_process_for_wait(keeper_pid);
    assert_process_live(&target, "Job member while coordinator holds its Job");
    assert_process_live(&keeper_process, "keeper before coordinator death");
    coordinator
        .kill()
        .expect("abruptly terminate coordinator holding the full Job handle");
    let status = coordinator.wait().expect("wait coordinator death");
    assert!(!status.success(), "coordinator fixture must die abruptly");
    let post_death = next_prefixed_line(
        &mut coordinator_lines,
        "LIFECYCLE_KEEPER_COORDINATOR_EXITED ",
    );
    assert_eq!(post_death, "LIFECYCLE_KEEPER_COORDINATOR_EXITED active=1");
    assert_eq!(
        next_prefixed_line(&mut coordinator_lines, "LIFECYCLE_KEEPER_ZERO"),
        "LIFECYCLE_KEEPER_ZERO active=0"
    );
    assert_process_exited(&target, "Job member after coordinator death");
    assert_process_exited(&keeper_process, "keeper after exact Job retirement");
}

#[test]
fn keeper_transfers_query_only_zero_witness_to_successor_process() {
    let mut attempt = WindowsProcessJob::create().expect("create unnamed attempt Job");
    let mut member = spawn_host_attempt_gate();
    attempt
        .assign_child(&member)
        .expect("assign exact attempt member");
    let mut coordinator = spawn_helper("descendant");
    let mut coordinator_session = 0_u32;
    // SAFETY: coordinator is live and its PID came from Child.
    assert_ne!(
        unsafe { ProcessIdToSessionId(coordinator.id(), &raw mut coordinator_session) },
        0
    );
    let coordinator_peer =
        keld_runtime::windows_job::WindowsProcessPeer::open(coordinator.id(), coordinator_session)
            .expect("pin exact coordinator");
    let keeper_handle = attempt
        .duplicate_lifecycle_keeper_handle()
        .expect("duplicate bounded keeper Job rights");
    let keeper = keld_runtime::windows_job::WindowsLifecycleJobWitness::adopt_transferred(
        keeper_handle,
        coordinator_peer,
    )
    .expect("adopt exact attempt as keeper");
    coordinator
        .kill()
        .expect("terminate coordinator before keeper takeover");
    let _ = coordinator.wait().expect("wait coordinator exit");
    assert!(
        keeper
            .wait_for_coordinator_exit(std::time::Duration::from_secs(1))
            .expect("wait retained coordinator process"),
        "keeper must prove the exact coordinator exited"
    );
    keeper
        .terminate_and_wait_zero(std::time::Duration::from_secs(10))
        .expect("retire exact process family before successor admission");
    assert_eq!(keeper.active_processes().expect("keeper zero query"), 0);

    let mut successor_command = Command::new(env::current_exe().expect("test executable"));
    successor_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "query-witness-receiver")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut successor = successor_command
        .spawn()
        .expect("spawn exact successor process");
    let successor_stdout = successor.stdout.take().expect("successor stdout");
    let mut successor_lines = BufReader::new(successor_stdout).lines();
    let mut successor_session = 0_u32;
    // SAFETY: successor is live and its PID came from Child.
    assert_ne!(
        unsafe { ProcessIdToSessionId(successor.id(), &raw mut successor_session) },
        0
    );
    let successor_peer =
        keld_runtime::windows_job::WindowsProcessPeer::open(successor.id(), successor_session)
            .expect("pin exact successor process");
    let query_handle = keeper
        .transfer_query_witness_to(&successor_peer)
        .expect("duplicate QUERY-only exact Job witness to successor");
    writeln!(
        successor.stdin.as_mut().expect("successor stdin"),
        "JOB_QUERY {query_handle}"
    )
    .expect("send target-process query handle over private test pipe");
    assert_eq!(
        next_prefixed_line(&mut successor_lines, "SUCCESSOR_QUERY_ACTIVE"),
        "SUCCESSOR_QUERY_ACTIVE 0"
    );
    drop(successor.stdin.take());
    assert!(
        successor.wait().expect("wait successor process").success(),
        "successor query helper must exit cleanly"
    );
    let _ = member
        .wait()
        .expect("wait attempt member killed at retirement");
}

#[test]
fn undelivered_remote_job_handle_is_closed_when_one_shot_keeper_exits() {
    let mut coordinator = spawn_helper("lifecycle-undelivered-coordinator");
    let stdout = coordinator.stdout.take().expect("coordinator stdout");
    let mut lines = BufReader::new(stdout).lines();
    let ready = next_prefixed_line(&mut lines, "LIFECYCLE_UNDELIVERED ");
    let mut fields = ready.split_ascii_whitespace();
    assert_eq!(fields.next(), Some("LIFECYCLE_UNDELIVERED"));
    let target_pid = fields
        .next()
        .expect("target PID field")
        .strip_prefix("target=")
        .expect("target field prefix")
        .parse::<u32>()
        .expect("valid target PID");
    let keeper_pid = fields
        .next()
        .expect("keeper PID field")
        .strip_prefix("keeper=")
        .expect("keeper field prefix")
        .parse::<u32>()
        .expect("valid keeper PID");
    assert_eq!(fields.next(), None, "undelivered record must be exact");
    let target = open_process_for_wait(target_pid);
    let keeper = open_process_for_wait(keeper_pid);
    assert_process_live(&target, "Job member before failed delivery");
    assert_process_live(&keeper, "keeper before failed delivery");

    coordinator
        .kill()
        .expect("abruptly stop coordinator before handle delivery");
    let _ = coordinator.wait().expect("reap crashed coordinator");
    assert_process_exited(&keeper, "one-shot keeper after setup-pipe EOF");
    assert_process_exited(
        &target,
        "Job member after the orphaned remote handle closed with its keeper",
    );
}

#[test]
fn launcher_job_rejects_a_different_direct_host_handle() {
    let mut attempt = WindowsProcessJob::create().expect("create launcher attempt Job");
    let mut admitted_host = spawn_host_attempt_gate();
    attempt
        .assign_child(&admitted_host)
        .expect("admit exact gated host");
    let mut unrelated_host = spawn_helper("identity");
    let error = attempt
        .terminate_and_wait(&unrelated_host, std::time::Duration::from_secs(10))
        .expect_err("a different host must not be supplied for this Job");
    assert!(
        error.to_string().contains("exact process admitted"),
        "wrong host was rejected for an unexpected reason: {error}"
    );
    let _ = admitted_host.wait().expect("Job close reaps gated host");
    let _ = unrelated_host.wait().expect("wait unrelated host");
}

#[test]
fn launcher_loss_before_job_assignment_closes_the_host_start_gate() {
    let mut host = spawn_host_attempt_gate();
    let stdout = host.stdout.take().expect("host stdout pipe");
    let writer = host.stdin.take().expect("host start writer");
    drop(writer);
    let status = host.wait().expect("wait for host after launcher pipe loss");
    assert!(
        !status.success(),
        "host continued after losing its launcher"
    );
    let output = io::read_to_string(stdout).expect("read gated host output");
    assert!(
        !output.contains("APP_RESOURCE_STARTED"),
        "application resources started without Job assignment: {output}"
    );
}

#[test]
fn startup_gate_rejects_token_with_trailing_byte_before_app_resources() {
    let mut host = spawn_host_attempt_gate();
    let stdout = host.stdout.take().expect("host stdout pipe");
    host.stdin
        .take()
        .expect("host start writer")
        .write_all(b"KELDHOST/1\x01")
        .expect("send token and hostile suffix in one write");
    let status = host.wait().expect("wait for malformed gated host");
    assert!(!status.success(), "host accepted a startup token suffix");
    let output = io::read_to_string(stdout).expect("read refused host output");
    assert!(output.contains("START_REFUSED"), "{output}");
    assert!(
        !output.contains("APP_RESOURCE_STARTED"),
        "application resources started after malformed token: {output}"
    );
}

#[test]
fn cleanup_control_pipe_distinguishes_exact_release_loss_and_bad_records() {
    for (payload, expected) in [
        (&[][..], "LauncherLost"),
        (&WINDOWS_DEV_STAGE_CLEANUP_RELEASE_V1[..], "Released"),
        (b"KELD-CLEANUP/1\x01", "Malformed"),
        (b"KELD-CLEANUP/0", "Malformed"),
        (b"KELD-CLEAN", "Malformed"),
    ] {
        let mut child = spawn_cleanup_control_helper();
        child
            .stdin
            .take()
            .expect("cleanup control input")
            .write_all(payload)
            .expect("write cleanup control payload");
        let output = child
            .wait_with_output()
            .expect("wait for cleanup control helper");
        assert!(
            output.status.success(),
            "cleanup helper failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!("CLEANUP_CONTROL {expected}")),
            "expected {expected}, got {stdout:?}"
        );
    }
}

#[test]
fn reduced_cleanup_handle_reaps_exact_job_on_malformed_control_without_set_rights() {
    let mut attempt = WindowsProcessJob::create().expect("create launcher attempt Job");
    let mut host = spawn_host_attempt_gate();
    attempt.assign_child(&host).expect("admit exact gated host");
    let mut host_stdout = BufReader::new(host.stdout.take().expect("host stdout pipe")).lines();
    let job_handle = attempt
        .duplicate_cleanup_observer_handle()
        .expect("duplicate only query/terminate cleanup rights");
    let mut cleanup = Command::new(env::current_exe().expect("current test executable"));
    cleanup
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "cleanup-observer")
        .env("KELD_WINDOWS_CLEANUP_HOST_PID", host.id().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(job_handle));
    let mut cleanup = cleanup.spawn().expect("spawn reduced-rights observer");
    let mut cleanup_stdin = cleanup.stdin.take().expect("cleanup control pipe");
    let cleanup_stdout = cleanup.stdout.take().expect("cleanup result pipe");
    let mut cleanup_lines = BufReader::new(cleanup_stdout).lines();
    assert_eq!(
        next_prefixed_line(&mut cleanup_lines, "CLEANUP_OBSERVER_READY"),
        "CLEANUP_OBSERVER_READY"
    );
    cleanup_stdin
        .write_all(b"KELD-CLEANUP/1\x01")
        .expect("send malformed cleanup record");
    drop(cleanup_stdin);
    assert_eq!(
        next_prefixed_line(&mut cleanup_lines, "CLEANUP_OBSERVER_DONE"),
        "CLEANUP_OBSERVER_DONE Malformed"
    );
    let output_status = cleanup.wait().expect("wait reduced-rights observer");
    assert!(output_status.success(), "observer exited {output_status}");
    let _ = host.wait().expect("wait host killed by exact attempt Job");
    assert_eq!(attempt.active_processes().expect("launcher Job census"), 0);
    drop(attempt);
    let _ = host_stdout.next();
}

#[test]
fn launcher_process_death_closes_the_attempt_job_and_reaps_its_tree() {
    let mut launcher = spawn_helper("launcher");
    let stdout = launcher.stdout.take().expect("launcher stdout pipe");
    let mut lines = BufReader::new(stdout).lines();
    let host_pid = parse_pid(
        &next_prefixed_line(&mut lines, "LAUNCHER_HOST "),
        "LAUNCHER_HOST",
    );
    let direct_pid = parse_pid(&next_prefixed_line(&mut lines, "DIRECT "), "DIRECT");
    let descendant_pid = parse_pid(&next_prefixed_line(&mut lines, "DESCENDANT "), "DESCENDANT");
    let host = open_process_for_wait(host_pid);
    let direct = open_process_for_wait(direct_pid);
    let descendant = open_process_for_wait(descendant_pid);

    launcher
        .kill()
        .expect("kill only the launcher process holding the Job handle");
    let _ = launcher.wait().expect("wait for killed launcher");
    assert_process_exited(&host, "host after launcher death");
    assert_process_exited(&direct, "direct child after launcher death");
    assert_process_exited(&descendant, "descendant after launcher death");
}

#[test]
#[ignore = "private subprocess entry point"]
fn windows_job_process_helper() {
    match env::var(HELPER_ENV).as_deref() {
        Ok("host") => run_host_helper(),
        Ok("attempt-host") => run_host_attempt_helper(),
        Ok("launcher") => run_launcher_helper(),
        Ok("direct") => run_direct_helper(),
        Ok("descendant") => run_descendant_helper(),
        Ok("relaunch") => run_relaunch_helper(),
        Ok("identity") => println!("IDENTITY_ONLY {}", std::process::id()),
        Ok("cleanup-control") => match WindowsProcessJob::await_cleanup_release_v1() {
            Ok(signal) => println!("CLEANUP_CONTROL {signal:?}"),
            Err(error) => {
                println!("CLEANUP_CONTROL_ERROR {error}");
                std::process::exit(74);
            }
        },
        Ok("cleanup-observer") => run_cleanup_observer_helper(),
        Ok("lifecycle-receiver") => run_lifecycle_receiver_helper(),
        Ok("lifecycle-client") => run_lifecycle_client_helper(),
        Ok("query-witness-receiver") => run_query_witness_receiver_helper(),
        Ok("lease-receiver") => run_lease_receiver_helper(),
        Ok("lifecycle-coordinator") => run_lifecycle_coordinator_helper(),
        Ok("lifecycle-undelivered-coordinator") => run_lifecycle_undelivered_coordinator_helper(),
        Ok("authenticated-attempt-coordinator") => run_authenticated_attempt_coordinator_helper(),
        Ok("authenticated-attempt-keeper") => run_authenticated_attempt_keeper_helper(),
        Ok("malformed-attempt-keeper") => run_malformed_attempt_keeper_helper(),
        Ok("authenticated-attempt-successor") => run_authenticated_attempt_successor_helper(),
        other => panic!("unexpected {HELPER_ENV} value: {other:?}"),
    }
}

fn run_lifecycle_client_helper() {
    let endpoint = env::var("KELD_TEST_LIFECYCLE_ENDPOINT").expect("lifecycle endpoint");
    let expected_server_pid = env::var("KELD_TEST_LIFECYCLE_SERVER_PID")
        .expect("expected server PID")
        .parse::<u32>()
        .expect("valid expected server PID");
    let expected_server_session = env::var("KELD_TEST_LIFECYCLE_SERVER_SESSION")
        .expect("expected server session")
        .parse::<u32>()
        .expect("valid expected server session");
    let expected_server_image =
        PathBuf::from(env::var("KELD_TEST_LIFECYCLE_SERVER_IMAGE").expect("expected server image"))
            .canonicalize()
            .expect("canonical expected server image");
    let mut connection = keld_ipc::connect_windows_lifecycle_rendezvous_until(
        &endpoint,
        WindowsLifecycleExpectation::exact(lifecycle_binding()),
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        |pid, session| {
            if pid != expected_server_pid || session != expected_server_session {
                return None;
            }
            let peer = keld_runtime::windows_job::WindowsProcessPeer::open(pid, session).ok()?;
            if peer.image_path().canonicalize().ok()? != expected_server_image
                || peer.token_facts().session_id != session
            {
                return None;
            }
            Some(peer)
        },
    )
    .expect("authenticate exact lifecycle server process and complete nonce exchange");
    assert!(
        !connection
            .stream_mut()
            .is_handle_inheritable()
            .expect("client pipe inheritance query"),
        "lifecycle client pipe handle must be non-inheritable"
    );
    println!(
        "LIFECYCLE_CLIENT_PINNED server={} client={}",
        connection.process_id(),
        std::process::id()
    );
    io::stdout()
        .flush()
        .expect("flush lifecycle client pin result");
    let mut command = String::new();
    let stdin = io::stdin();
    let mut input = BufReader::new(stdin);
    assert!(
        input.read_line(&mut command).expect("read client release") > 0,
        "parent must release the retained client process"
    );
    assert_eq!(command.trim_end(), "EXIT");
}

fn lifecycle_binding() -> WindowsLifecycleBinding {
    WindowsLifecycleBinding::new(
        [0x41; 32],
        [0x52; 32],
        [0x63; 32],
        WindowsLifecyclePurpose::CoordinatorToKeeper,
    )
    .expect("distinct lifecycle test context")
}

fn lifecycle_test_locator() -> [u8; 32] {
    let byte = env::var("KELD_TEST_LIFECYCLE_LOCATOR_BYTE")
        .ok()
        .map_or(0x78, |value| {
            u8::from_str_radix(&value, 16).expect("valid lifecycle locator byte")
        });
    [byte; 32]
}

fn run_authenticated_attempt_coordinator_helper() {
    let lock_path = env::var_os("KELD_TEST_ACTIVATION_LOCK")
        .map(PathBuf::from)
        .expect("activation lock path");
    let writer = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&lock_path)
        .expect("open coordinator share-zero activation lock");
    let retention_source: OwnedHandle = writer
        .try_clone()
        .expect("duplicate local handle to the same activation.lock object")
        .into();
    let mut attempt = WindowsProcessJob::create().expect("create coordinator attempt Job");
    let member = spawn_parked_descendant();
    let target_pid = member.id();
    attempt
        .assign_child(&member)
        .expect("assign exact candidate-family member");
    let binding = lifecycle_binding();
    let listener = WindowsLifecycleRendezvousListener::bind(lifecycle_test_locator(), binding)
        .expect("bind coordinator one-shot endpoint");
    let mut server_session = 0_u32;
    // SAFETY: this live coordinator PID and writable session output are valid.
    assert_ne!(
        unsafe { ProcessIdToSessionId(std::process::id(), &raw mut server_session) },
        0
    );
    println!("COORDINATOR_LISTENING {target_pid}");
    io::stdout()
        .flush()
        .expect("flush coordinator endpoint readiness");

    let keeper_control = BufReader::new(io::stdin())
        .lines()
        .next()
        .expect("keeper identity record")
        .expect("read keeper identity record");
    let mut fields = keeper_control.split_ascii_whitespace();
    assert_eq!(fields.next(), Some("KEEPER"));
    let keeper_pid = fields
        .next()
        .expect("keeper PID")
        .parse::<u32>()
        .expect("valid keeper PID");
    let keeper_session = fields
        .next()
        .expect("keeper session")
        .parse::<u32>()
        .expect("valid keeper session");
    assert_eq!(fields.next(), None, "keeper identity record must be exact");
    let expected_image = env::current_exe()
        .expect("test executable")
        .canonicalize()
        .expect("canonical keeper image");
    let accepted = listener
        .accept_until(
            std::time::Instant::now() + std::time::Duration::from_secs(5),
            |pid, session, facts| {
                if pid != keeper_pid
                    || session != keeper_session
                    || session != server_session
                    || facts.session_id != session
                {
                    return None;
                }
                let peer = WindowsProcessPeer::open(pid, session).ok()?;
                if peer.image_path().canonicalize().ok()? != expected_image
                    || peer.token_facts() != facts
                {
                    return None;
                }
                Some(peer)
            },
        )
        .expect("authenticate exact keeper and complete LC1/LA1/LR1")
        .expect("keeper connects before deadline");
    attempt
        .transfer_lifecycle_attempt_handoff(
            accepted,
            &retention_source,
            std::time::Instant::now() + std::time::Duration::from_secs(5),
        )
        .expect("transfer exact Job+lease and receive keeper adoption ACK");
    drop(retention_source);
    drop(writer);
    drop(attempt);
    drop(member);
    println!("COORDINATOR_HANDOFF_READY target={target_pid}");
    io::stdout()
        .flush()
        .expect("flush coordinator handoff readiness");
    std::thread::park();
}

#[expect(
    clippy::too_many_lines,
    reason = "private keeper subprocess covers accepted bundle, retirement ACK and lease release"
)]
fn run_authenticated_attempt_keeper_helper() {
    let endpoint = env::var("KELD_TEST_LIFECYCLE_ENDPOINT").expect("lifecycle endpoint");
    let expected_server_pid = env::var("KELD_TEST_LIFECYCLE_SERVER_PID")
        .expect("expected server PID")
        .parse::<u32>()
        .expect("valid expected server PID");
    let expected_server_session = env::var("KELD_TEST_LIFECYCLE_SERVER_SESSION")
        .expect("expected server session")
        .parse::<u32>()
        .expect("valid expected server session");
    let expected_server_image =
        PathBuf::from(env::var("KELD_TEST_LIFECYCLE_SERVER_IMAGE").expect("server image"))
            .canonicalize()
            .expect("canonical expected server image");
    let connection = keld_ipc::connect_windows_lifecycle_rendezvous_until(
        &endpoint,
        WindowsLifecycleExpectation::exact(lifecycle_binding()),
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        |pid, session| {
            if pid != expected_server_pid || session != expected_server_session {
                return None;
            }
            let peer = WindowsProcessPeer::open(pid, session).ok()?;
            if peer.image_path().canonicalize().ok()? != expected_server_image
                || peer.token_facts().session_id != session
            {
                return None;
            }
            Some(peer)
        },
    )
    .expect("authenticate exact coordinator and receive final lifecycle receipt");
    let mut handoff = WindowsLifecycleKeeperHandoff::receive_attempt_bundle(
        connection,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
    )
    .expect("adopt exact attempt Job and reduced writer lease");
    assert_eq!(handoff.active_processes().expect("keeper Job query"), 1);
    let mut lease_file = std::fs::File::from(
        handoff
            .activation_lease_retention()
            .expect("active keeper retains the writer lease")
            .try_clone()
            .expect("clone reduced read-only keeper lease for negative control"),
    );
    assert!(
        lease_file.write_all(b"forbidden").is_err(),
        "keeper's activation lease must have no write right"
    );
    println!("ATTEMPT_KEEPER_READY active=1");
    io::stdout()
        .flush()
        .expect("flush keeper attempt readiness");
    let command = BufReader::new(io::stdin())
        .lines()
        .next()
        .expect("keeper command")
        .expect("read keeper command");
    match command.as_str() {
        "EXIT" => {
            drop(lease_file);
            drop(handoff);
            println!("ATTEMPT_KEEPER_EXIT");
            io::stdout().flush().expect("flush keeper exit");
        }
        "CRASH" => {
            println!("ATTEMPT_KEEPER_CRASH_ARMED");
            io::stdout().flush().expect("flush keeper crash-arm marker");
            std::process::abort();
        }
        "RETIRE" => {
            drop(lease_file);
            let retirement_binding =
                lifecycle_binding().with_purpose(WindowsLifecyclePurpose::KeeperToSuccessor);
            let listener = WindowsLifecycleRendezvousListener::bind(
                lifecycle_test_locator(),
                retirement_binding,
            )
            .expect("bind one-shot successor endpoint");
            println!("KEEPER_RETIRE_LISTENER_READY");
            io::stdout()
                .flush()
                .expect("flush successor listener readiness");
            let expected_image = env::current_exe()
                .expect("test executable")
                .canonicalize()
                .expect("canonical successor image");
            let mut keeper_session = 0_u32;
            // SAFETY: this live keeper PID and writable session output are valid.
            assert_ne!(
                unsafe { ProcessIdToSessionId(std::process::id(), &raw mut keeper_session) },
                0
            );
            let successor = listener
                .accept_until(
                    std::time::Instant::now() + std::time::Duration::from_secs(10),
                    |pid, session, facts| {
                        if session != keeper_session || facts.session_id != session {
                            return None;
                        }
                        let peer = WindowsProcessPeer::open(pid, session).ok()?;
                        if peer.image_path().canonicalize().ok()? != expected_image
                            || peer.token_facts() != facts
                        {
                            return None;
                        }
                        Some(peer)
                    },
                )
                .expect("accept authenticated successor")
                .expect("successor connects before deadline");
            match handoff.retire_to_successor(
                successor,
                std::time::Duration::from_secs(10),
                std::time::Instant::now() + std::time::Duration::from_secs(10),
            ) {
                Ok(()) => {
                    println!("KEEPER_RETIRED");
                    io::stdout().flush().expect("flush keeper retirement");
                }
                Err(error) => {
                    println!("KEEPER_RETIRE_FAILED {error}");
                    io::stdout().flush().expect("flush failed retirement proof");
                    assert_eq!(
                        BufReader::new(io::stdin())
                            .lines()
                            .next()
                            .expect("failed-retirement keeper release command")
                            .expect("read failed-retirement keeper release command"),
                        "EXIT"
                    );
                    println!("KEEPER_FAILED_RETIRE_EXIT");
                    io::stdout()
                        .flush()
                        .expect("flush failed-retirement keeper exit");
                }
            }
        }
        other => panic!("unexpected authenticated keeper command {other:?}"),
    }
}

fn run_malformed_attempt_keeper_helper() {
    let endpoint = env::var("KELD_TEST_LIFECYCLE_ENDPOINT").expect("lifecycle endpoint");
    let expected_server_pid = env::var("KELD_TEST_LIFECYCLE_SERVER_PID")
        .expect("expected coordinator PID")
        .parse::<u32>()
        .expect("valid coordinator PID");
    let expected_server_session = env::var("KELD_TEST_LIFECYCLE_SERVER_SESSION")
        .expect("expected coordinator session")
        .parse::<u32>()
        .expect("valid coordinator session");
    let expected_server_image =
        PathBuf::from(env::var("KELD_TEST_LIFECYCLE_SERVER_IMAGE").expect("coordinator image"))
            .canonicalize()
            .expect("canonical expected coordinator image");
    let connection = keld_ipc::connect_windows_lifecycle_rendezvous_until(
        &endpoint,
        WindowsLifecycleExpectation::exact(lifecycle_binding()),
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        |pid, session| {
            if pid != expected_server_pid || session != expected_server_session {
                return None;
            }
            let peer = WindowsProcessPeer::open(pid, session).ok()?;
            if peer.image_path().canonicalize().ok()? != expected_server_image
                || peer.token_facts().session_id != session
            {
                return None;
            }
            Some(peer)
        },
    )
    .expect("authenticate coordinator before malformed handle record");
    let result = WindowsLifecycleKeeperHandoff::receive_attempt_bundle(
        connection,
        std::time::Instant::now() + std::time::Duration::from_millis(500),
    );
    println!("MALFORMED_RECEIVER_RETURNED={result:?}");
    std::process::exit(0);
}

fn run_authenticated_attempt_successor_helper() {
    let endpoint = env::var("KELD_TEST_LIFECYCLE_ENDPOINT").expect("lifecycle endpoint");
    let expected_server_pid = env::var("KELD_TEST_LIFECYCLE_SERVER_PID")
        .expect("expected keeper PID")
        .parse::<u32>()
        .expect("valid keeper PID");
    let expected_server_session = env::var("KELD_TEST_LIFECYCLE_SERVER_SESSION")
        .expect("expected keeper session")
        .parse::<u32>()
        .expect("valid keeper session");
    let expected_server_image =
        PathBuf::from(env::var("KELD_TEST_LIFECYCLE_SERVER_IMAGE").expect("keeper image"))
            .canonicalize()
            .expect("canonical keeper image");
    let expectation = WindowsLifecycleExpectation::from_keeper([0x41; 32])
        .expect("independently known installation identity");
    let connection = keld_ipc::connect_windows_lifecycle_rendezvous_until(
        &endpoint,
        expectation,
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        |pid, session| {
            if pid != expected_server_pid || session != expected_server_session {
                return None;
            }
            let peer = WindowsProcessPeer::open(pid, session).ok()?;
            if peer.image_path().canonicalize().ok()? != expected_server_image
                || peer.token_facts().session_id != session
            {
                return None;
            }
            Some(peer)
        },
    )
    .expect("authenticate exact keeper and receive LR1");
    let pending = WindowsLifecycleRetirementPending::receive(
        connection,
        std::time::Instant::now() + std::time::Duration::from_secs(10),
    )
    .expect("independently query exact attempt Job zero before QA1");
    assert_eq!(
        pending
            .active_processes()
            .expect("pending successor census"),
        0
    );
    assert_eq!(
        pending.binding_knowledge(),
        keld_ipc::WindowsLifecycleBindingKnowledge::KeeperSuppliedAwaitingJournalRevalidation
    );
    println!("SUCCESSOR_ZERO_BEFORE_ACK active=0");
    io::stdout()
        .flush()
        .expect("flush zero observation before QA1");
    let qa1_command = BufReader::new(io::stdin())
        .lines()
        .next()
        .expect("successor QA1 gate")
        .expect("read successor QA1 gate");
    if qa1_command == "ABORT" {
        println!("SUCCESSOR_ABORTED_BEFORE_QA1");
        io::stdout().flush().expect("flush successor pre-QA1 exit");
        return;
    }
    assert_eq!(qa1_command, "ACK");
    let retirement = pending
        .acknowledge()
        .expect("send QA1 and wait for QF1 after keeper releases the lease");
    assert_eq!(
        retirement.active_processes().expect("successor Job census"),
        0
    );
    let lock_path = env::var_os("KELD_TEST_ACTIVATION_LOCK")
        .map(PathBuf::from)
        .expect("activation lock path");
    let writer = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&lock_path)
        .expect("keeper released the lock only after the exact zero acknowledgement");
    println!("SUCCESSOR_WRITER_ACQUIRED active=0 pending-journal-revalidation");
    io::stdout()
        .flush()
        .expect("flush successor retirement proof");
    assert_eq!(
        BufReader::new(io::stdin())
            .lines()
            .next()
            .expect("successor release command")
            .expect("read successor release command"),
        "EXIT"
    );
    drop(writer);
    drop(retirement);
}

fn run_lease_receiver_helper() {
    let mut lines = BufReader::new(io::stdin()).lines();
    let record = lines
        .next()
        .expect("lease handle record")
        .expect("read lease handle record");
    let mut fields = record.split_ascii_whitespace();
    assert_eq!(fields.next(), Some("LEASE"));
    let remote_handle = fields
        .next()
        .expect("remote lease handle")
        .parse::<usize>()
        .expect("valid remote lease handle");
    assert_eq!(fields.next(), None, "lease record must be exact");

    // SAFETY: the integer was duplicated into this exact helper process by the
    // coordinator immediately before this private test record.
    let handle =
        unsafe { OwnedHandle::from_raw_handle((remote_handle as *mut std::ffi::c_void).cast()) };
    let mut lease = std::fs::File::from(handle);
    assert!(
        lease.write_all(b"forbidden").is_err(),
        "reduced keeper lease must not grant file writes"
    );
    println!("LIFECYCLE_LEASE_KEEPER_READY");
    io::stdout()
        .flush()
        .expect("flush retained-lease readiness");
    assert_eq!(
        lines
            .next()
            .expect("lease keeper exit command")
            .expect("read exit"),
        "EXIT"
    );
    println!("LIFECYCLE_LEASE_KEEPER_EXIT");
    io::stdout().flush().expect("flush lease keeper exit");
}

fn run_lifecycle_receiver_helper() {
    println!("LIFECYCLE_KEEPER_BOOT_READY");
    io::stdout().flush().expect("flush keeper boot readiness");
    let mut lines = BufReader::new(io::stdin()).lines();
    let Some(transfer) = lines.next() else {
        println!("LIFECYCLE_KEEPER_SETUP_EOF");
        io::stdout()
            .flush()
            .expect("flush keeper setup-EOF refusal");
        return;
    };
    let transfer = transfer.expect("read transfer record");
    let mut fields = transfer.split_ascii_whitespace();
    assert_eq!(fields.next(), Some("HANDLE"));
    let remote_handle = fields
        .next()
        .expect("remote handle value")
        .parse::<usize>()
        .expect("valid target handle value");
    assert_eq!(fields.next(), Some("COORDINATOR"));
    let coordinator_pid = fields
        .next()
        .expect("coordinator PID")
        .parse::<u32>()
        .expect("valid coordinator PID");
    let coordinator_session = fields
        .next()
        .expect("coordinator session")
        .parse::<u32>()
        .expect("valid coordinator session");
    assert_eq!(fields.next(), None, "transfer record must be exact");

    // SAFETY: the integer was created by DuplicateHandle into this exact helper
    // process immediately before the parent sent this private test record.
    let transferred =
        unsafe { OwnedHandle::from_raw_handle((remote_handle as *mut std::ffi::c_void).cast()) };
    let coordinator =
        keld_runtime::windows_job::WindowsProcessPeer::open(coordinator_pid, coordinator_session)
            .expect("pin exact coordinator process");
    let witness = keld_runtime::windows_job::WindowsLifecycleJobWitness::adopt_transferred(
        transferred,
        coordinator,
    )
    .expect("adopt and attenuate transferred exact Job");
    let active = witness.active_processes().expect("query transferred Job");
    println!("LIFECYCLE_KEEPER_READY active={active}");
    eprintln!("LIFECYCLE_KEEPER_READY active={active}");
    io::stdout().flush().expect("flush keeper readiness");
    io::stderr().flush().expect("flush keeper stderr readiness");
    match lines
        .next()
        .expect("keeper control command")
        .expect("read keeper control")
        .as_str()
    {
        "EXIT" => println!("LIFECYCLE_KEEPER_EXIT"),
        "WAIT_DEATH" => {
            assert!(
                witness
                    .wait_for_coordinator_exit(std::time::Duration::from_secs(10))
                    .expect("wait for exact coordinator process"),
                "coordinator did not exit before the bounded deadline"
            );
            let active = witness
                .active_processes()
                .expect("query exact Job after coordinator death");
            assert_eq!(
                active, 1,
                "the exact Job member must survive the coordinator"
            );
            println!("LIFECYCLE_KEEPER_COORDINATOR_EXITED active={active}");
            io::stdout().flush().expect("flush post-death Job census");
            witness
                .terminate_and_wait_zero(std::time::Duration::from_secs(10))
                .expect("terminate exact Job after coordinator death");
            println!(
                "LIFECYCLE_KEEPER_ZERO active={}",
                witness.active_processes().expect("final Job query")
            );
        }
        other => panic!("unexpected keeper control command {other:?}"),
    }
    io::stdout().flush().expect("flush keeper exit");
}

fn run_query_witness_receiver_helper() {
    let mut lines = BufReader::new(io::stdin()).lines();
    let record = lines
        .next()
        .expect("successor Job handle record")
        .expect("read successor Job handle record");
    let mut fields = record.split_ascii_whitespace();
    assert_eq!(fields.next(), Some("JOB_QUERY"));
    let remote_handle = fields
        .next()
        .expect("successor Job handle value")
        .parse::<usize>()
        .expect("valid successor Job handle");
    assert_eq!(fields.next(), None, "successor record must be exact");
    // SAFETY: the value was duplicated into this exact subprocess before it was
    // written to the private test-control pipe.
    let handle =
        unsafe { OwnedHandle::from_raw_handle((remote_handle as *mut std::ffi::c_void).cast()) };
    let witness =
        keld_runtime::windows_job::WindowsLifecycleQueryWitness::adopt_transferred(handle)
            .expect("adopt query-only successor witness");
    println!(
        "SUCCESSOR_QUERY_ACTIVE {}",
        witness
            .active_processes()
            .expect("independently query exact Job")
    );
    io::stdout().flush().expect("flush successor Job census");
}

#[allow(clippy::zombie_processes)] // The parent kills this fixture at the coordinator crash cut.
fn run_lifecycle_coordinator_helper() {
    let mut attempt = WindowsProcessJob::create().expect("create unnamed attempt Job");
    let target = spawn_parked_descendant();
    attempt
        .assign_child(&target)
        .expect("assign exact parked member before keeper launch");

    let mut keeper_command = Command::new(env::current_exe().expect("test executable"));
    keeper_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "lifecycle-receiver")
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped());
    let mut keeper = keeper_command
        .spawn()
        .expect("spawn lifecycle keeper outside attempt Job");
    let mut keeper_lines =
        BufReader::new(keeper.stderr.take().expect("keeper readiness pipe")).lines();
    let mut keeper_session = 0_u32;
    // SAFETY: the keeper child is live and its PID comes from Child.
    assert_ne!(
        unsafe { ProcessIdToSessionId(keeper.id(), &raw mut keeper_session) },
        0
    );
    let keeper_peer =
        keld_runtime::windows_job::WindowsProcessPeer::open(keeper.id(), keeper_session)
            .expect("pin exact child keeper process");
    let remote_handle = attempt
        .transfer_lifecycle_handle_to(&keeper_peer)
        .expect("transfer reduced exact Job handle to keeper");

    let mut coordinator_session = 0_u32;
    // SAFETY: this helper process is the live coordinator and output is writable.
    assert_ne!(
        unsafe { ProcessIdToSessionId(std::process::id(), &raw mut coordinator_session) },
        0
    );
    writeln!(
        keeper.stdin.as_mut().expect("keeper setup pipe"),
        "HANDLE {remote_handle} COORDINATOR {} {coordinator_session}",
        std::process::id()
    )
    .expect("deliver exact remote Job handle to keeper");
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "LIFECYCLE_KEEPER_READY"),
        "LIFECYCLE_KEEPER_READY active=1"
    );
    writeln!(
        keeper.stdin.as_mut().expect("keeper control pipe"),
        "WAIT_DEATH"
    )
    .expect("arm exact coordinator-death wait");
    drop(keeper.stdin.take());
    println!(
        "LIFECYCLE_COORDINATOR_READY target={} keeper={}",
        target.id(),
        keeper.id()
    );
    io::stdout()
        .flush()
        .expect("flush registered lifecycle owners");
    // The parent kills this coordinator abruptly. Its full Job handle and Child
    // handles close in the OS; the keeper's reduced duplicate remains.
    std::thread::park();
}

#[allow(clippy::zombie_processes)] // The parent kills this fixture before remote-handle delivery.
fn run_lifecycle_undelivered_coordinator_helper() {
    let mut attempt = WindowsProcessJob::create().expect("create unnamed attempt Job");
    let target = spawn_parked_descendant();
    attempt
        .assign_child(&target)
        .expect("assign exact parked member before keeper launch");

    let mut keeper_command = Command::new(env::current_exe().expect("test executable"));
    keeper_command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "lifecycle-receiver")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut keeper = keeper_command
        .spawn()
        .expect("spawn one-shot keeper outside attempt Job");
    let keeper_stdout = keeper.stdout.take().expect("keeper stdout");
    let mut keeper_lines = BufReader::new(keeper_stdout).lines();
    assert_eq!(
        next_prefixed_line(&mut keeper_lines, "LIFECYCLE_KEEPER_BOOT_READY"),
        "LIFECYCLE_KEEPER_BOOT_READY"
    );
    let mut keeper_session = 0_u32;
    // SAFETY: the keeper child is live and its PID comes from Child.
    assert_ne!(
        unsafe { ProcessIdToSessionId(keeper.id(), &raw mut keeper_session) },
        0
    );
    let keeper_peer =
        keld_runtime::windows_job::WindowsProcessPeer::open(keeper.id(), keeper_session)
            .expect("pin exact keeper process");
    let _remote_handle = attempt
        .transfer_lifecycle_handle_to(&keeper_peer)
        .expect("duplicate exact Job before the deliberately missing delivery");
    println!(
        "LIFECYCLE_UNDELIVERED target={} keeper={}",
        target.id(),
        keeper.id()
    );
    io::stdout()
        .flush()
        .expect("flush process IDs before coordinator death");
    // No handle record is written. The remote duplicate is unknowable to the
    // keeper; coordinator death closes the setup pipe, so the one-shot keeper
    // exits and the OS closes that otherwise-orphaned remote handle.
    std::thread::park();
}

fn run_cleanup_observer_helper() {
    let host_pid = env::var("KELD_WINDOWS_CLEANUP_HOST_PID")
        .expect("cleanup host PID")
        .parse::<u32>()
        .expect("valid cleanup host PID");
    let observer = WindowsProcessJob::adopt_cleanup_observer_from_stderr(host_pid)
        .expect("adopt exact query/terminate Job handle");
    println!("CLEANUP_OBSERVER_READY");
    io::stdout()
        .flush()
        .expect("flush cleanup observer readiness");
    let signal =
        WindowsProcessJob::await_cleanup_release_v1().expect("cleanup observer control pipe");
    observer
        .terminate_and_wait_attached(std::time::Duration::from_secs(10))
        .expect("reap exact Job through reduced rights");
    assert_eq!(
        observer
            .active_processes()
            .expect("query retained exact Job after family reap"),
        0,
        "the cleanup owner must retain its exact Job witness through the deletion boundary"
    );
    println!("CLEANUP_OBSERVER_DONE {signal:?}");
    io::stdout().flush().expect("flush cleanup observer result");
}

fn run_launcher_helper() {
    let mut attempt = WindowsProcessJob::create().expect("create launcher-owned attempt Job");
    let mut host = spawn_host_attempt_gate();
    if let Err(error) = attempt.assign_child(&host) {
        drop(host.stdin.take());
        let _ = host.wait();
        panic!("assign host before app start: {error}");
    }
    let mut host_stdout = BufReader::new(host.stdout.take().expect("host stdout pipe")).lines();
    let mut start_writer = host.stdin.take().expect("host start writer");
    release_host_start_v1(&mut start_writer).expect("release host start gate");

    let ready = next_prefixed_line(&mut host_stdout, "ATTEMPT ");
    assert!(ready.contains("inner_job=true"), "{ready}");
    assert_eq!(
        next_prefixed_line(&mut host_stdout, "APP_RESOURCE_STARTED"),
        "APP_RESOURCE_STARTED"
    );
    println!("LAUNCHER_HOST {}", host.id());
    println!("{}", next_prefixed_line(&mut host_stdout, "DIRECT "));
    println!("{}", next_prefixed_line(&mut host_stdout, "DESCENDANT "));
    io::stdout()
        .flush()
        .expect("flush process-family observation");
    let _ = host.wait().expect("wait for host owned by this launcher");
    drop(start_writer);
}

fn run_host_attempt_helper() {
    if !keld_runtime::windows_job::launcher_start_gate_requested() {
        println!("START_REFUSED selector");
        std::process::exit(72);
    }
    if let Err(error) = accept_host_start_v1() {
        println!("START_REFUSED {error}");
        std::process::exit(72);
    }
    let inner = match install_host_death_job() {
        Ok(inner) => inner,
        Err(error) => {
            println!("ATTEMPT_REFUSED {error}");
            std::process::exit(73);
        }
    };
    println!("ATTEMPT inner_job={}", inner.current_process_assigned);
    io::stdout().flush().expect("flush inner Job observation");

    println!("APP_RESOURCE_STARTED");
    io::stdout()
        .flush()
        .expect("flush first app-resource witness");

    let mut direct = spawn_helper("direct");
    let direct_stdout = direct.stdout.take().expect("direct stdout pipe");
    for line in BufReader::new(direct_stdout).lines() {
        println!("{}", line.expect("read direct process observation"));
        io::stdout().flush().expect("flush process observation");
    }
    let status = direct.wait().expect("wait for direct process");
    assert!(status.success(), "direct process failed: {status}");
    std::thread::park();
}

fn run_host_helper() {
    let observation = install_host_death_job().expect("install host-death Job");
    println!(
        "JOB limits=0x{:08x} nested={} assigned={} inheritable={}",
        observation.limit_flags,
        observation.nested_under_existing_job,
        observation.current_process_assigned,
        observation.handle_inheritable
    );
    io::stdout().flush().expect("flush Job observation");

    let mut direct = spawn_helper("direct");
    let direct_stdout = direct.stdout.take().expect("direct stdout pipe");
    for line in BufReader::new(direct_stdout).lines() {
        println!("{}", line.expect("read direct process observation"));
        io::stdout().flush().expect("flush process observation");
    }
    let status = direct.wait().expect("wait for direct child");
    assert!(status.success(), "direct child failed: {status}");
}

fn run_direct_helper() {
    println!("DIRECT {}", std::process::id());
    io::stdout().flush().expect("flush direct PID");

    let mut descendant = spawn_parked_descendant();
    println!("DESCENDANT {}", descendant.id());
    io::stdout().flush().expect("flush descendant PID");
    let status = descendant.wait().expect("wait for descendant");
    assert!(status.success(), "descendant failed: {status}");
}

fn run_descendant_helper() {
    println!("DESCENDANT {}", std::process::id());
    io::stdout().flush().expect("flush descendant PID");
    std::thread::park();
}

fn run_relaunch_helper() {
    let observation = install_host_death_job().expect("install Job after prior host death");
    assert!(observation.current_process_assigned);

    let status = Command::new("cmd.exe")
        .args(["/d", "/c", "exit 0"])
        .status()
        .expect("spawn child after prior Job cleanup");
    assert!(status.success(), "post-cleanup child failed: {status}");
    println!("RELAUNCH_OK");
}

fn spawn_helper(role: &str) -> Child {
    let mut command = Command::new(env::current_exe().expect("current test executable"));
    command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, role)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.spawn().unwrap_or_else(|error| {
        panic!("spawn {role} process fixture: {error}");
    })
}

/// Spawns the parked `descendant` fixture and consumes its final stdout record.
///
/// The returned `Child` holds the member's only stdout reader. Waiting for the
/// `DESCENDANT` record means every write the member makes happens before that reader
/// can close (by `drop` or coordinator death); a member still booting at that close
/// would fail its write with `ERROR_NO_DATA` and exit on its own, not by retirement.
fn spawn_parked_descendant() -> Child {
    let mut descendant = spawn_helper("descendant");
    let stdout = descendant.stdout.as_mut().expect("descendant stdout pipe");
    let ready = next_prefixed_line(&mut BufReader::new(stdout).lines(), "DESCENDANT ");
    assert_eq!(
        parse_pid(&ready, "DESCENDANT"),
        descendant.id(),
        "the exact descendant must be parked before its reader can close"
    );
    descendant
}

fn spawn_host_attempt_gate() -> Child {
    let mut command = Command::new(env::current_exe().expect("current test executable"));
    command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "attempt-host")
        .env(WINDOWS_LAUNCH_GATE_ENV, WINDOWS_LAUNCH_GATE_ATTEMPT_JOB_V1)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.spawn().expect("spawn host behind start gate")
}

fn spawn_cleanup_control_helper() -> Child {
    let mut command = Command::new(env::current_exe().expect("current test executable"));
    command
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "cleanup-control")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.spawn().expect("spawn cleanup control helper")
}

fn next_prefixed_line(
    lines: &mut impl Iterator<Item = io::Result<String>>,
    prefix: &str,
) -> String {
    lines
        .find_map(|line| match line {
            Ok(line) if line.starts_with(prefix) => Some(Ok(line)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .unwrap_or_else(|| panic!("line starting with {prefix:?} missing"))
        .unwrap_or_else(|error| panic!("read line starting with {prefix:?}: {error}"))
}

fn parse_pid(line: &str, label: &str) -> u32 {
    let prefix = format!("{label} ");
    line.strip_prefix(&prefix)
        .unwrap_or_else(|| panic!("expected {label} PID line, got {line:?}"))
        .parse()
        .unwrap_or_else(|error| panic!("parse {label} PID from {line:?}: {error}"))
}

struct ObservedProcess {
    handle: OwnedHandle,
    pid: u32,
}

impl Drop for ObservedProcess {
    fn drop(&mut self) {
        use std::os::windows::io::AsRawHandle as _;

        let raw = self.handle.as_raw_handle().cast();
        // SAFETY: `raw` is live for both calls. A zero-time wait only checks
        // state. If a failed assertion or negative control left the fixture
        // alive, PROCESS_TERMINATE was requested specifically for cleanup.
        if unsafe { WaitForSingleObject(raw, 0) } != WAIT_OBJECT_0 {
            // SAFETY: same live process handle, with PROCESS_TERMINATE access;
            // this test-owned fixture has no state outside the test.
            let _ = unsafe { TerminateProcess(raw, 1) };
        }
    }
}

fn open_process_for_wait(pid: u32) -> ObservedProcess {
    // SAFETY: OpenProcess receives a numeric PID observed from the live test
    // child and requests observation plus test-fixture cleanup rights. A
    // non-null result is one fresh owning handle, converted exactly once.
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
    assert!(
        !raw.is_null(),
        "open process {pid}: {}",
        io::Error::last_os_error()
    );
    ObservedProcess {
        // SAFETY: `raw` is the fresh non-null owning handle returned above.
        handle: unsafe { OwnedHandle::from_raw_handle(raw.cast()) },
        pid,
    }
}

fn assert_process_exited(process: &ObservedProcess, description: &str) {
    use std::os::windows::io::AsRawHandle as _;

    // SAFETY: the borrowed raw process handle remains live for this call. A
    // signaled process handle is the kernel's termination oracle; the timeout
    // only bounds a broken fixture and is not synchronization by sleeping.
    let result =
        unsafe { WaitForSingleObject(process.handle.as_raw_handle().cast(), PROCESS_WAIT_MS) };
    assert_eq!(
        result, WAIT_OBJECT_0,
        "{description} PID {} survived host death",
        process.pid
    );
}

fn assert_process_live(process: &ObservedProcess, description: &str) {
    use std::os::windows::io::AsRawHandle as _;

    // SAFETY: the borrowed raw process handle remains live for this read-only
    // zero-time state query.
    let result = unsafe { WaitForSingleObject(process.handle.as_raw_handle().cast(), 0) };
    assert_eq!(
        result, WAIT_TIMEOUT,
        "{description} PID {} exited before retirement was released",
        process.pid
    );
}
