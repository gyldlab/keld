//! Actual owner-loop proof; Native/FSM census and window/document proof remain distinct.
use super::super::super::super::{
    AppWindowCommand, GUARDIAN_OWNER_REPLY_DEADLINE, GuardSnapshot, GuardianOwner,
    GuardianOwnerHandle, PrimaryCallerContext, PrimaryEchoCall, PrimaryLinkIdentity, PrimaryRouter,
    PrimaryRouterHandle, SessionShutdownState, dispatch_primary_echo,
};
use super::super::{OBSERVATION_BOUND, sensor, support};
use super::{
    Inputs, NativeWorkerSetup, SENTINEL, assert_owned_groups, native_worker_setup, next_bound,
    public_call, public_outcome, public_outcome_count, roundtrip,
};
use crate::app_session::HostRunFailure;
use crate::app_session::native_generation_fixture::ActorTurns;
use nix::errno::Errno;
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::{Pid, getpgid};
use serde_json::json;
use std::fs;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Instant;

const CASE: &str = "guardian-native-actor";
const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::actor_owner::actual_guardian_owner_services_commands_and_fs_completions_while_native_is_held";

/// Test owns release/receipt custody; the actual owner alone owns the guardian and FSM.
struct ActorFixture {
    inputs: Inputs,
    snapshot: GuardSnapshot,
    native: NativeWorkerSetup,
    router: Option<PrimaryRouter>,
    owner: Option<GuardianOwner>,
    window: Receiver<AppWindowCommand>,
    guardian_group: u32,
    initial_bun_group: u32,
}

impl ActorFixture {
    fn start() -> Self {
        Self::start_with_admission(false).0
    }

    fn start_identity() -> (Self, keld_runtime::registry::RolePrincipal) {
        Self::start_with_admission(true)
    }

    fn start_with_admission(identity: bool) -> (Self, keld_runtime::registry::RolePrincipal) {
        let inputs = Inputs::supplied();
        assert_eq!(
            fs::read(&inputs.target).expect("Host-seeded target"),
            SENTINEL
        );
        let snapshot = inputs.prepare_policy();
        let native = native_worker_setup();
        let mut guardian = inputs.spawn_guardian();
        let bound = next_bound(&mut guardian, 1);
        let principal = bound.principal();
        let guardian_group = guardian.guardian_pid();
        let initial_bun_group = guardian.group_pid().expect("registered real G1");
        assert_owned_groups(&guardian, initial_bun_group);
        let (window, window_rx) = mpsc::channel();
        let shutdown = SessionShutdownState::new();
        let owner = GuardianOwner::start(guardian, window.clone(), None, shutdown.clone())
            .expect("actual guardian owner loop");
        let router = if identity {
            PrimaryRouter::start_bound_with_fs_test_gate(
                bound,
                window,
                owner.handle(),
                shutdown,
                Some(&snapshot),
                Arc::clone(&native.gate),
            )
        } else {
            PrimaryRouter::start_with_fs_test_gate(
                bound.into_stream(),
                window,
                owner.handle(),
                shutdown,
                Some(&snapshot),
                Some(Arc::clone(&native.gate)),
            )
        }
        .expect("actual native router");
        if !identity {
            router
                .handle()
                .recovery_armed
                .store(false, Ordering::Release);
        }
        owner
            .attach_router(router.handle())
            .expect("actual owner attachment");
        native
            .signals
            .initialized
            .recv_timeout(OBSERVATION_BOUND)
            .expect("existing Native scope installed on the real worker");
        router
            .handle()
            .signal_ready()
            .expect("actual initial arm and acknowledgment");
        (
            Self {
                inputs,
                snapshot,
                native,
                router: Some(router),
                owner: Some(owner),
                window: window_rx,
                guardian_group,
                initial_bun_group,
            },
            principal,
        )
    }

    fn handle(&self) -> PrimaryRouterHandle {
        self.router.as_ref().expect("owned router").handle()
    }

    fn owner_handle(&self) -> GuardianOwnerHandle {
        self.owner.as_ref().expect("actual owner custody").handle()
    }

    fn assert_held(&self) {
        assert!(
            matches!(
                self.native.signals.returned.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ),
            "Native returned before explicit release"
        );
        let state = self
            .snapshot
            .fs
            .state
            .lock()
            .expect("real Native accounting");
        assert_eq!(state.in_flight, 1);
        assert_eq!(
            state.prepared_attempt,
            Some(1),
            "fresh broker appeared while Native was held"
        );
        drop(state);
        assert_eq!(sensor::object_handle_count(&self.inputs.target), 1);
        assert!(sensor::directory_handle_count(&self.inputs.allowed_root) > 0);
        assert_eq!(
            fs::read(&self.inputs.target).expect("held independent bytes"),
            SENTINEL
        );
        assert!(
            matches!(self.window.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "actual actor reported a window shutdown/fatal command"
        );
    }

    fn finish(&mut self) {
        let handle = self.handle();
        assert!(handle.shutdown.reserve_cli_lease_lost());
        handle
            .cli_lease_lost()
            .expect("existing ordered attributed shutdown tail");
        assert_eq!(
            self.window
                .recv_timeout(OBSERVATION_BOUND)
                .expect("existing tail UI wake"),
            AppWindowCommand::Quit
        );
        self.router
            .take()
            .expect("one router owner")
            .shutdown()
            .expect("actual worker/readers join");
        let fence = self
            .native
            .signals
            .final_snapshot
            .recv_timeout(OBSERVATION_BOUND)
            .expect("actual Native final fence");
        assert!(!fence.fixture_fault);
        assert_eq!((fence.request_entries, fence.request_returns), (4, 4));
        assert_eq!(sensor::directory_handle_count(&self.inputs.allowed_root), 0);
        assert_eq!(sensor::object_handle_count(&self.inputs.target), 0);
        self.owner
            .take()
            .expect("one guardian actor owner")
            .shutdown()
            .expect("actual guardian EOF, successful exit, owned reap and owner join");
        for group in [self.initial_bun_group, self.guardian_group] {
            let group = Pid::from_raw(i32::try_from(group).expect("diagnostic group fits"));
            assert_eq!(
                killpg(group, None::<Signal>),
                Err(Errno::ESRCH),
                "post-reap group remains (diagnostic only)"
            );
        }
        assert!(
            !self.inputs.stage_root.exists(),
            "normal guardian stage cleanup absent"
        );
        println!(
            "KELD_KEL143_ACTOR_CLEANUP entries=4 returns=4 roots=0 target_handles=0 owner_joined=true stage_absent=true"
        );
    }
}

impl Drop for ActorFixture {
    fn drop(&mut self) {
        if let Some(owner) = &self.owner {
            owner.handle().deny_recovery();
        }
        if let Some(release) = self.native.signals.native_release.take() {
            let _ = release.send(());
        }
        for _ in 0..4 {
            let _ = self.native.gate_release.send(());
        }
        if let Some(router) = self.router.take() {
            let _ = router.shutdown();
        }
        if let Some(owner) = self.owner.take() {
            let _ = owner.shutdown();
        }
    }
}

/// A real test-owned member of the authenticated G1 group. Only this test
/// parent may reap it; failure cleanup never acts on an observed foreign PID.
struct HeldGuardianMember(Option<Child>);

impl HeldGuardianMember {
    fn new(group_pid: u32) -> Self {
        let raw = i32::try_from(group_pid).expect("authenticated Bun group PID fits i32");
        let member = Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(raw)
            .spawn()
            .expect("test-owned child joined authenticated G1 group");
        assert_eq!(
            getpgid(Some(Pid::from_raw(
                i32::try_from(member.id()).expect("member PID fits")
            ))),
            Ok(Pid::from_raw(raw)),
            "member is in the authentic, registered generation group"
        );
        Self(Some(member))
    }

    fn reap(&mut self) {
        let mut member = self.0.take().expect("one owned child reap");
        let deadline = Instant::now() + OBSERVATION_BOUND;
        let status = loop {
            if let Some(status) = member.try_wait().expect("reap exact test child") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "original group signal did not kill held member"
            );
            thread::yield_now();
        };
        assert_eq!(status.signal(), Some(nix::libc::SIGKILL));
    }
}

impl Drop for HeldGuardianMember {
    fn drop(&mut self) {
        if let Some(mut member) = self.0.take() {
            let _ = member.kill();
            let _ = member.wait();
        }
    }
}

#[test]
fn disconnected_actor_command_cannot_acknowledge_clean_quit() {
    let (command_tx, commands) = mpsc::channel();
    drop(commands);
    let error = GuardianOwnerHandle { command_tx }
        .shutdown_and_wait()
        .expect_err("a disconnected Guardian owner must refuse clean Quit");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert!(
        error
            .to_string()
            .contains("stopped before accepting shutdown"),
        "the original owner-disconnection cause must be explicit"
    );
}

/// The aborted actor's reader must fail closed; only after all native
/// children are absent may the exact source-bound fixture stage be released.
fn finish_retained_actor(mut fixture: ActorFixture, failure: HostRunFailure) {
    let router_tail = fixture.router.take().expect("original router").shutdown();
    assert!(
        matches!(&router_tail, Err(error)
            if error.code() == "KELD-CORE-037"
            && error.phase == "primary app-link failure"
            && matches!(error.detail.as_str(),
                "guardian owner stopped"
                | "guardian owner ended before acknowledging link failure")),
        "the retired reader must fail closed when its Guardian owner is gone: {router_tail:?}"
    );
    let stage = fixture.inputs.stage_root.clone();
    drop(failure);
    drop(fixture);
    let deleted = fs::remove_dir_all(&stage);
    assert!(
        deleted.is_ok()
            || matches!(&deleted, Err(source) if source.kind() == std::io::ErrorKind::NotFound),
        "exact native fixture stage cleanup after group retirement: {deleted:?}"
    );
    assert!(
        !stage.exists(),
        "fixture stage deletion after native custody proof"
    );
}

/// Core must retain Runtime's native group birth after its actor has ended.
/// The Runtime-only ECHILD regression keeps `GuardedPrimary` alive manually and
/// therefore cannot prove this Host failure handoff.
#[test]
#[ignore = "explicit physical macOS Guardian actor fatal-custody lane"]
fn fatal_actor_retains_held_native_group_in_host_failure() {
    const CASE: &str = "guardian-actor-retained-birth";
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::actor_owner::fatal_actor_retains_held_native_group_in_host_failure";
    if !support::is_native_child(CASE) {
        support::isolated_guardian_case(CASE, SELECTOR);
        return;
    }

    let mut fixture = ActorFixture::start();
    let group_pid = fixture.initial_bun_group;
    let group = Pid::from_raw(i32::try_from(group_pid).expect("registered group PID fits"));
    let mut held = HeldGuardianMember::new(group_pid);
    // This is the original test-spawned Guardian Child, still owned by the
    // actor. The actor performs its own Child::try_wait/owned reap.
    kill(
        Pid::from_raw(i32::try_from(fixture.guardian_group).expect("Guardian PID fits")),
        Some(Signal::SIGKILL),
    )
    .expect("kill exact original test Guardian");
    assert_eq!(
        fixture
            .window
            .recv_timeout(OBSERVATION_BOUND + OBSERVATION_BOUND)
            .expect("fatal actor must wake original UI"),
        AppWindowCommand::Fatal
    );
    let mut failure = fixture
        .owner
        .take()
        .expect("actual actor owns original group")
        .shutdown()
        .expect_err("fatal actor cannot report completed shutdown");
    assert_eq!(failure.code(), "KELD-CORE-033");
    assert!(failure.to_string().contains("guardian watcher"));
    let original_runtime = std::error::Error::source(&failure)
        .and_then(|source| source.downcast_ref::<keld_runtime::RuntimeError>())
        .expect("original typed native Guardian failure remains the Core error source");
    assert!(
        matches!(original_runtime,
            keld_runtime::RuntimeError::GuardianExited { group_pid: original, .. }
            if *original == group_pid),
        "nested GuardianExit retains exact original native group"
    );
    assert!(
        failure.retains_main_custody(),
        "Host's existing fatal stage-cleanup gate needs retained Guardian custody"
    );
    assert_eq!(
        failure
            .retained_guardian
            .as_ref()
            .expect("original Guardian birth returned out of joined actor")
            .group_pid(),
        Some(group_pid)
    );
    assert!(
        fixture.inputs.stage_root.exists(),
        "failed Host must not delete the owner-private stage while custody is held"
    );
    assert!(
        !fixture.handle().is_current(2),
        "fatal actor provisioned a successor"
    );
    assert_ne!(
        killpg(group, None::<Signal>),
        Err(Errno::ESRCH),
        "a held unreaped member must prevent a false group-absence receipt"
    );

    // The test-only parent now reaps its exact child. The already-spent
    // original signal may not be repeated or given a new kill budget.
    held.reap();
    let deadline = Instant::now() + OBSERVATION_BOUND;
    while killpg(group, None::<Signal>) != Err(Errno::ESRCH) {
        assert!(Instant::now() < deadline, "owned group never fully retired");
        thread::yield_now();
    }
    let guardian = failure
        .retained_guardian
        .as_mut()
        .expect("Core still holds exact original Runtime owner");
    guardian
        .poll_fatal()
        .expect_err("a dead Guardian never becomes healthy after group absence");
    assert_eq!(
        guardian.group_pid(),
        None,
        "dual ESRCH released exact group birth"
    );
    assert_eq!(
        failure.code(),
        "KELD-CORE-033",
        "first fatal outcome remains sticky"
    );
    finish_retained_actor(fixture, failure);
    println!(
        "KELD_GH824_CORE_CUSTODY actor_joined=true held_member_denied=true failure_retained=true dual_esrch_released=true"
    );
    println!("KELD_AC7_CASE_DONE case={CASE} actor_joined=true fatal_custody=true");
}

/// A non-release Rust panic must preserve the same original, registered G1
/// and source-bound Host stage when the actor unwinds while the group is held.
fn inject_actual_actor_unwind() {
    panic!("test-only GuardianOwner actor unwind");
}

#[test]
#[ignore = "explicit physical macOS actor unwind with real enrolled group"]
fn actor_unwind_retains_original_guardian_and_denies_successor() {
    const CASE: &str = "guardian-actor-unwind-custody";
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::actor_owner::actor_unwind_retains_original_guardian_and_denies_successor";
    if !support::is_native_child(CASE) {
        support::isolated_guardian_case(CASE, SELECTOR);
        return;
    }

    let mut fixture = ActorFixture::start();
    let group_pid = fixture.initial_bun_group;
    let group = Pid::from_raw(i32::try_from(group_pid).expect("enrolled G1 PID fits"));
    let mut held = HeldGuardianMember::new(group_pid);
    fixture
        .owner
        .as_ref()
        .expect("actual original Guardian actor")
        .trigger_panic_for_test(inject_actual_actor_unwind);
    assert_eq!(
        fixture
            .window
            .recv_timeout(OBSERVATION_BOUND * 2)
            .expect("fatal UI wake"),
        AppWindowCommand::Fatal
    );
    let mut failure = fixture
        .owner
        .take()
        .expect("original Guardian actor")
        .shutdown()
        .expect_err("actor unwind must not complete shutdown");
    assert_eq!(failure.code(), "KELD-CORE-037");
    assert!(
        failure
            .to_string()
            .contains("actor panicked before proving"),
        "the original panic must remain the first terminal cause: {failure}"
    );
    assert!(
        failure.retains_main_custody(),
        "Host must retain the original Guardian"
    );
    assert_eq!(
        failure
            .retained_guardian
            .as_ref()
            .expect("retained native owner")
            .group_pid(),
        Some(group_pid),
        "actor unwind must retain the original registered birth"
    );
    assert!(
        fixture.inputs.stage_root.exists(),
        "Host must retain unproved stage"
    );
    assert!(!fixture.handle().is_current(2), "no successor after panic");
    assert_ne!(
        killpg(group, None::<Signal>),
        Err(Errno::ESRCH),
        "the unreaped same-group member keeps group absence unproved"
    );

    // The synthetic panic occurs before any native signal. Only in this
    // controlled test can the retained original owner safely perform its
    // first shutdown; an arbitrary actor panic never retries it automatically.
    let guardian = failure
        .retained_guardian
        .as_mut()
        .expect("original Guardian remains available after unwind");
    guardian
        .shutdown()
        .expect_err("an unreaped same-group child must deny clean shutdown");
    assert_eq!(
        guardian.group_pid(),
        Some(group_pid),
        "failed native absence retains the original group birth"
    );

    // After the test parent reaps its exact child, the original group may
    // clear only through the runtime's read-only absence probe.
    held.reap();
    let deadline = Instant::now() + OBSERVATION_BOUND;
    while killpg(group, None::<Signal>) != Err(Errno::ESRCH) {
        assert!(
            Instant::now() < deadline,
            "original process group still held"
        );
        thread::yield_now();
    }
    // The same retained owner now performs only its read-only absence poll.
    guardian
        .poll_fatal()
        .expect_err("panicked Guardian owner remains terminal");
    assert_eq!(
        guardian.group_pid(),
        None,
        "dual ESRCH releases enrolled birth"
    );
    assert_eq!(
        failure.code(),
        "KELD-CORE-037",
        "panic failure stays sticky"
    );
    finish_retained_actor(fixture, failure);
    println!(
        "KELD_GH824_ACTOR_PANIC_CUSTODY retained=true held_member_denied=true successor=false dual_esrch_released=true"
    );
    println!("KELD_AC7_CASE_DONE case={CASE} panic_custody=true");
}

fn assert_old_return_and_broker_baseline(fixture: &mut ActorFixture) {
    fixture
        .native
        .signals
        .native_release
        .take()
        .expect("one Native release")
        .send(())
        .expect("existing Native Hold owner");
    let returned = fixture
        .native
        .signals
        .returned
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual Native normal return");
    assert!(!returned.fixture_fault);
    assert_eq!((returned.request_entries, returned.request_returns), (1, 1));
    // An existing command receipt remains unconsumed: at most one normal turn
    // can run here. A Retired receipt only queues Prepare; the next turn sends it.
    // Observe the worker's actual old drop before resuming that next actor turn.
    let deadline = Instant::now() + OBSERVATION_BOUND;
    loop {
        let state = fixture
            .snapshot
            .fs
            .state
            .lock()
            .expect("real retirement metadata");
        let retired = state.prepared_attempt.is_none() && state.in_flight == 0;
        drop(state);
        if retired {
            break;
        }
        assert!(Instant::now() < deadline, "actual worker retirement absent");
        thread::yield_now();
    }
    assert_eq!(sensor::object_handle_count(&fixture.inputs.target), 0);
    assert_eq!(
        sensor::directory_handle_count(&fixture.inputs.allowed_root),
        0,
        "actual old broker roots survive worker retirement"
    );
    assert_eq!(
        fs::read(&fixture.inputs.target).expect("retired independent bytes"),
        SENTINEL
    );
    println!("KELD_KEL143_ACTOR_OLD_BASELINE entries=1 returns=1 target_handles=0 root_handles=0");
}

fn observe_held_actor_retirement(
    fixture: &ActorFixture,
    turns: &mut ActorTurns,
    old: &PrimaryEchoCall,
) {
    let handle = fixture.handle();
    let revoked_by = Instant::now() + OBSERVATION_BOUND;
    while handle.last_revoked_attempt.load(Ordering::Acquire) != 1 || handle.is_current(1) {
        assert!(
            Instant::now() < revoked_by,
            "actual owner did not apply G1 Revoked"
        );
        turns.advance();
    }
    fixture.assert_held();
    let replied_by = Instant::now() + GUARDIAN_OWNER_REPLY_DEADLINE;
    let receipt = fixture
        .owner_handle()
        .request_fail_retired_generation(1)
        .expect("existing retired-failure command");
    let acknowledged = loop {
        match receipt.try_recv() {
            Ok(actual) => break actual.expect("actual held-state owner acknowledgment"),
            Err(mpsc::TryRecvError::Empty) => turns.advance(),
            Err(mpsc::TryRecvError::Disconnected) => panic!("actual actor lost failure receipt"),
        }
        assert!(
            Instant::now() < replied_by,
            "actor waited for Native before acknowledging"
        );
    };
    assert!(acknowledged <= replied_by);
    fixture.assert_held();
    let retired_echo = old
        .reply
        .recv_timeout(OBSERVATION_BOUND)
        .expect("one retired public-call outcome")
        .expect_err("G1 pending echo must retire");
    assert_eq!(retired_echo.code(), "KELD-CORE-037");
    assert!(
        matches!(old.reply.try_recv(), Err(mpsc::TryRecvError::Disconnected)),
        "old public call received another outcome"
    );
    println!(
        "KELD_KEL143_ACTOR_HELD_ACK attempt=1 native_entries=1 native_returns=0 actual_actor=true"
    );
}

#[test]
#[ignore = "explicit physical macOS source-bound actual guardian actor/native lane"]
fn actual_guardian_owner_services_commands_and_fs_completions_while_native_is_held() {
    if !support::is_native_child(CASE) {
        support::isolated_guardian_case(CASE, SELECTOR);
        return;
    }
    let mut fixture = ActorFixture::start();
    let old = roundtrip(&fixture.handle(), "00000000000000000000000000003143");
    fixture
        .native
        .gate_taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual G1 work taken");
    let held = fixture
        .native
        .signals
        .held
        .recv_timeout(OBSERVATION_BOUND)
        .expect("real Native after-walk hold");
    assert!(!held.fixture_fault);
    assert_eq!((held.request_entries, held.request_returns), (1, 0));
    assert!(held.walk_entries > 0 && held.target_opens > 0);
    fixture.assert_held();
    fixture
        .owner_handle()
        .fail_generation(1)
        .expect("actual owner requests G1 fault");
    let mut turns = ActorTurns::start(fixture.owner_handle());
    let handle = fixture.handle();
    observe_held_actor_retirement(&fixture, &mut turns, &old);
    assert_old_return_and_broker_baseline(&mut fixture);
    let bound_by = Instant::now() + OBSERVATION_BOUND;
    while !handle.is_current(2) {
        assert!(
            Instant::now() < bound_by,
            "normal actor turns did not complete fresh recovery"
        );
        turns.advance();
    }
    let state = fixture
        .snapshot
        .fs
        .state
        .lock()
        .expect("actual G2 preparation");
    assert_eq!(state.prepared_attempt, Some(2));
    assert_eq!(state.in_flight, 0);
    drop(state);
    assert_eq!(handle.last_revoked_attempt.load(Ordering::Acquire), 1);
    assert!(sensor::directory_handle_count(&fixture.inputs.allowed_root) > 0);
    let successor = roundtrip(&handle, "00000000000000000000000000003243");
    assert_eq!(
        public_outcome(&successor),
        json!({"ok":true,"bytes":fixture.inputs.expected_content})
    );
    assert_eq!(
        fs::read(&fixture.inputs.target).expect("independent G2 effect"),
        fixture.inputs.expected_content
    );
    let denied = public_call(&handle, "00000000000000000000000000003244", "deny", 2);
    let denied_outcome = public_outcome_count(&denied, 2);
    assert_eq!(denied_outcome["ok"], false);
    assert_eq!(denied_outcome["code"], "KELD-GUARD002");
    assert_eq!(
        fs::read(&fixture.inputs.outside).expect("independent denied sentinel"),
        fixture.inputs.outside_content
    );
    println!(
        "KELD_KEL143_ACTOR_G2 current=2 queued_command_turns={} allowed_exact_bytes=true denied_no_effect=true",
        turns.completed
    );
    drop(turns); // Disconnect pending rendezvous before requesting/joining shutdown.
    fixture.finish();
    println!(
        "KELD_AC7_CASE_DONE case={CASE} actual_actor=true normal_turn_completion=true window_document_claim=false"
    );
}

/// The actual loop owns error/denial/fatal cleanup; the direct case owns event history.
#[test]
#[ignore = "explicit physical macOS source-bound actual owner absent-anchor fatal lane"]
fn actual_owner_preserves_absent_anchor_error_and_fatal_cleanup() {
    use super::anchor_failure::AnchorAbsence;
    const CASE: &str = "guardian-actor-anchor-absent";
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::actor_owner::actual_owner_preserves_absent_anchor_error_and_fatal_cleanup";
    if !support::is_native_child(CASE) {
        support::isolated_guardian_case(CASE, SELECTOR);
        return;
    }

    let mut fixture = ActorFixture::start();
    let old = roundtrip(&fixture.handle(), "00000000000000000000000000004143");
    fixture
        .native
        .gate_taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual G1 work taken");
    let held = fixture
        .native
        .signals
        .held
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual Native after-walk hold");
    assert!(!held.fixture_fault);
    assert_eq!((held.request_entries, held.request_returns), (1, 0));
    assert!(held.walk_entries > 0 && held.target_opens > 0);
    fixture.assert_held();
    fixture
        .owner_handle()
        .fail_generation(1)
        .expect("actual owner requests G1 retirement");
    let mut turns = ActorTurns::start(fixture.owner_handle());
    observe_held_actor_retirement(&fixture, &mut turns, &old);
    let absence = AnchorAbsence::remove(&fixture.inputs.allowed_root, &fixture.inputs.target);
    assert!(sensor::directory_handle_count(absence.renamed_root()) > 0);
    assert_eq!(sensor::object_handle_count(absence.renamed_target()), 1);
    // Disconnect the existing bounded baton before the real worker returns.
    drop(turns);
    fixture
        .native
        .signals
        .native_release
        .take()
        .expect("one Native release")
        .send(())
        .expect("existing Native Hold owner");
    let returned = fixture
        .native
        .signals
        .returned
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual Native return");
    assert!(!returned.fixture_fault);
    assert_eq!((returned.request_entries, returned.request_returns), (1, 1));
    assert_eq!(
        fixture
            .window
            .recv_timeout(OBSERVATION_BOUND)
            .expect("actual preflight Fatal wake"),
        AppWindowCommand::Fatal
    );
    let error = fixture
        .owner
        .take()
        .expect("one actor owner")
        .shutdown()
        .expect_err("actual actor must preserve failed fresh preparation");
    assert_eq!(error.code(), "KELD-NATIVE-008");
    fixture
        .router
        .take()
        .expect("one router owner")
        .shutdown()
        .expect("fatal-path Native worker and reader joins");
    let fence = fixture
        .native
        .signals
        .final_snapshot
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual final Native fence");
    assert!(!fence.fixture_fault);
    assert_eq!((fence.request_entries, fence.request_returns), (1, 1));
    assert_eq!(sensor::directory_handle_count(absence.renamed_root()), 0);
    assert_eq!(sensor::object_handle_count(absence.renamed_target()), 0);
    for group in [fixture.initial_bun_group, fixture.guardian_group] {
        let group = Pid::from_raw(i32::try_from(group).expect("diagnostic group fits"));
        assert_eq!(
            killpg(group, None::<Signal>),
            Err(Errno::ESRCH),
            "fatal cleanup left an owned process group"
        );
    }
    assert!(
        !fixture.inputs.stage_root.exists(),
        "fatal cleanup retained the exact stage"
    );
    println!(
        "KELD_KEL143_AC4_ACTOR_FAILURE code={} fatal=true entries=1 returns=1 roots=0 target_handles=0 owner_joined=true stage_absent=true provision_trace_owned_by=direct",
        error.code()
    );
    absence.restore();
    println!("KELD_AC7_CASE_DONE case={CASE} actual_actor=true provision_trace_owned_by=direct");
}

/// Actual accepted identities share the existing actor/native physical owner lane.
#[test]
#[ignore = "explicit physical macOS source-bound KEL-97 guardian identity lane"]
fn actual_guardian_retains_primary_identity_through_retirement_and_g2() {
    use keld_ipc::CorrelationId;
    const CASE: &str = "guardian-primary-identity";
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::actor_owner::actual_guardian_retains_primary_identity_through_retirement_and_g2";
    if !support::is_native_child(CASE) {
        support::isolated_guardian_case(CASE, SELECTOR);
        return;
    }
    let (mut fixture, initial_principal) = ActorFixture::start_identity();
    let handle = fixture.handle();
    let caller1 = {
        let current = handle.current.lock().expect("actual accepted row");
        let active = current.as_ref().expect("real G1 row");
        assert!(active.identity == PrimaryLinkIdentity::Role(initial_principal));
        PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    let old = roundtrip(&handle, "00000000000000000000000000000097");
    fixture
        .native
        .gate_taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual G1 work taken");
    let held = fixture
        .native
        .signals
        .held
        .recv_timeout(OBSERVATION_BOUND)
        .expect("real native hold");
    assert!(!held.fixture_fault);
    assert_eq!((held.request_entries, held.request_returns), (1, 0));
    fixture.assert_held();
    fixture
        .owner_handle()
        .fail_generation(1)
        .expect("actual owner fault request");
    let mut turns = ActorTurns::start(fixture.owner_handle());
    observe_held_actor_retirement(&fixture, &mut turns, &old);
    assert!(caller1.reader_stop.load(Ordering::Acquire));
    dispatch_primary_echo(
        &handle.current,
        &handle.shutdown,
        &caller1,
        CorrelationId(97),
        &[0xff],
    )
    .expect("real retired caller inert before decode");
    assert_old_return_and_broker_baseline(&mut fixture);
    let deadline = Instant::now() + OBSERVATION_BOUND;
    while !handle.is_current(2) {
        assert!(Instant::now() < deadline, "actual owner G2 absent");
        turns.advance();
    }
    let caller2 = {
        let current = handle.current.lock().expect("real G2 row");
        let active = current.as_ref().expect("actual G2 installed");
        let PrimaryLinkIdentity::Role(principal) = active.identity else {
            panic!("authenticated G2 lost actual role identity");
        };
        assert_eq!(principal.owner(), initial_principal.owner());
        assert_ne!(principal.generation(), initial_principal.generation());
        PrimaryCallerContext {
            attempt: active.attempt,
            identity: active.identity,
            reader_stop: Arc::clone(&active.reader_stop),
        }
    };
    let mismatched = PrimaryCallerContext {
        identity: caller1.identity,
        ..caller2.clone()
    };
    dispatch_primary_echo(
        &handle.current,
        &handle.shutdown,
        &mismatched,
        CorrelationId(98),
        &[0xff],
    )
    .expect("only old accepted principal refuses before decode");
    dispatch_primary_echo(
        &handle.current,
        &handle.shutdown,
        &caller1,
        CorrelationId(99),
        &[0xff],
    )
    .expect("actual G1 stays inert while G2 current");
    assert_eq!(caller2.attempt, 2);
    let successor = roundtrip(&handle, "00000000000000000000000000000197");
    assert_eq!(
        public_outcome(&successor),
        json!({"ok":true,"bytes":fixture.inputs.expected_content})
    );
    // Positive is the approved successor channel: actual G2 public FS write/read
    // carried by this one accepted link; the outer Host Echo is only control.
    {
        let current = handle
            .current
            .lock()
            .expect("current after actual G2 reply");
        let active = current
            .as_ref()
            .expect("G2 survived its successor-channel reply");
        assert_eq!(active.attempt, caller2.attempt);
        assert!(active.identity == caller2.identity);
        assert!(Arc::ptr_eq(&active.reader_stop, &caller2.reader_stop));
        assert!(!caller2.reader_stop.load(Ordering::Acquire));
        let state = fixture
            .snapshot
            .fs
            .state
            .lock()
            .expect("actual G2 worker metadata");
        assert_eq!(state.prepared_attempt, Some(2));
        assert_eq!(state.in_flight, 0);
    }
    assert_eq!(
        fs::read(&fixture.inputs.target).expect("independent exact G2 effect"),
        fixture.inputs.expected_content
    );
    let denied = public_call(&handle, "00000000000000000000000000000297", "deny", 2);
    assert_eq!(public_outcome_count(&denied, 2)["code"], "KELD-GUARD002");
    assert_eq!(
        fs::read(&fixture.inputs.outside).expect("denied sentinel"),
        fixture.inputs.outside_content
    );
    drop(turns);
    fixture.finish();
    println!(
        "KELD_AC7_CASE_DONE case={CASE} actual_actor=true bound_principal_retained=true window_document_claim=false"
    );
}
