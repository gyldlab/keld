//! KEL-143 T1: real guardian recovery waits for actual native return and root drop.
//! Test-only composition of existing owners; no window/document product claim.
use super::super::super::{
    AppWindowCommand, FsRecoveryAttempt, FsWorkerCommand, GuardSnapshot, GuardianOwnerCommand,
    GuardianOwnerHandle, PERMISSIONS_FILE, PrimaryCallerContext, PrimaryRouter,
    PrimaryRouterHandle, RecoveryArm, SessionShutdownState, apply_guardian_recovery_arm,
};
use super::{OBSERVATION_BOUND, sensor, support};
use crate::app_session::native_generation_fixture::{
    Inputs, NativeWorkerSetup, SENTINEL, assert_owned_groups, captured_digest, file_digest,
    native_worker_setup, next_bound, public_call, public_outcome, public_outcome_count, roundtrip,
};
use crate::app_session::native_observer_setup::NativeSignals;
use keld_runtime::macos_guardian::{GuardedPrimary, GuardedPrimaryUpdate};
use keld_runtime::primary::{PrimaryRoleEvent as RoleEvent, RoleGeneration};
use nix::errno::Errno;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use serde_json::json;
use std::fs;

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const CASE: &str = "guardian-native-retirement";
const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::real_guardian_waits_for_native_return_and_old_broker_directory_baseline";

#[path = "native_generation_actor.rs"]
mod actor_owner;

#[path = "native_generation_anchor_failure.rs"]
mod anchor_failure;

#[path = "native_generation_root_identity.rs"]
mod root_identity;

/// Releases test waits before router join and guardian EOF, also on an assertion unwind.
struct Fixture {
    inputs: Inputs,
    snapshot: GuardSnapshot,
    signals: NativeSignals,
    gate_release: Sender<()>,
    gate_taken: Receiver<()>,
    owner_commands: Receiver<GuardianOwnerCommand>,
    window: Receiver<AppWindowCommand>,
    router: Option<PrimaryRouter>,
    guardian: GuardedPrimary,
    initial_arm: bool,
    groups: Vec<u32>,
    initial_generation: RoleGeneration,
}

impl Fixture {
    fn start() -> Self {
        let inputs = Inputs::supplied();
        assert_eq!(
            fs::read(&inputs.target).expect("Host-seeded target"),
            SENTINEL
        );
        let snapshot = inputs.prepare_policy();
        let NativeWorkerSetup {
            signals,
            gate,
            gate_release,
            gate_taken,
        } = native_worker_setup();
        let mut guardian = inputs.spawn_guardian();
        let bound = next_bound(&mut guardian, 1);
        let initial_generation = bound.generation();
        let group = guardian.group_pid().expect("registered G1 group");
        assert_owned_groups(&guardian, group);
        let (owner, owner_commands) = mpsc::channel();
        let (window, window_rx) = mpsc::channel();
        let router = PrimaryRouter::start_with_fs_test_gate(
            bound.into_stream(),
            window,
            GuardianOwnerHandle { command_tx: owner },
            SessionShutdownState::new(),
            Some(&snapshot),
            Some(gate),
        )
        .expect("real authenticated native router");
        // The paired test adapter defaults true; exercise the real initial arm/ACK seam.
        router
            .handle()
            .recovery_armed
            .store(false, Ordering::Release);
        signals
            .initialized
            .recv_timeout(OBSERVATION_BOUND)
            .expect("actual native worker scope");
        Self {
            inputs,
            snapshot,
            signals,
            gate_release,
            gate_taken,
            owner_commands,
            window: window_rx,
            router: Some(router),
            guardian,
            initial_arm: false,
            groups: vec![group],
            initial_generation,
        }
    }

    fn handle(&self) -> PrimaryRouterHandle {
        self.router.as_ref().expect("owned router").handle()
    }

    fn ready(&mut self) {
        let handle = self.handle();
        let ready = thread::spawn(move || handle.signal_ready());
        let GuardianOwnerCommand::ArmRecovery(phase, receipt) = self
            .owner_commands
            .recv_timeout(OBSERVATION_BOUND)
            .expect("actual initial Ready arm")
        else {
            panic!("Ready did not request the existing owner arm");
        };
        let handle = self.handle();
        apply_guardian_recovery_arm(
            phase,
            &mut self.guardian,
            Some(&handle),
            &mut self.initial_arm,
            None,
        )
        .expect("actual Runtime eligibility");
        receipt.send(Ok(())).expect("owner ACK receiver");
        ready
            .join()
            .expect("Ready sender join")
            .expect("actual Ready receipt");
        assert!(self.initial_arm && self.handle().recovery_armed.load(Ordering::Acquire));
    }

    fn edit_staged_policy_after_ready(&self) {
        use std::io::Write as _;
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
        assert!(self.initial_arm && self.handle().recovery_armed.load(Ordering::Acquire));
        let path = self.inputs.stage_root.join(PERMISSIONS_FILE);
        let original_metadata =
            fs::symlink_metadata(&path).expect("owned original policy identity");
        assert!(
            original_metadata.is_file(),
            "staged policy must remain a regular object"
        );
        let original_mode = original_metadata.permissions().mode() & 0o7777;
        assert_eq!(
            original_mode & 0o222,
            0,
            "producer input is deliberately read-only"
        );
        let original = file_digest(&path);
        assert_eq!(original, self.inputs.permissions_sha256);
        let retained = captured_digest(&self.inputs.permissions_sha256);
        assert_eq!(self.snapshot.verified.verified_sha256(), retained);
        assert_eq!(self.snapshot.fs.verified.verified_sha256(), retained);
        let parent = self
            .inputs
            .outside
            .parent()
            .expect("outside sentinel directory");
        assert!(self.inputs.allowed_root.starts_with(parent));
        let scope = format!("{}/**", parent.display());
        let edited = serde_json::to_string(&json!({
            "app": { "fs": { "read": [&scope], "write": [&scope] } }
        }))
        .expect("valid widened fixture policy JSON");
        let _ = keld_guard::parse_manifest(&edited)
            .expect("existing guard parser accepts widened fixture scope");
        let mut replacement = tempfile::Builder::new()
            .prefix("kel143-policy-")
            .permissions(original_metadata.permissions())
            .tempfile_in(&self.inputs.stage_root)
            .expect("existing owned staging parent permits fresh policy control");
        replacement
            .write_all(edited.as_bytes())
            .expect("complete replacement policy bytes");
        replacement
            .as_file()
            .sync_all()
            .expect("complete replacement input before publish");
        assert_eq!(
            replacement
                .as_file()
                .metadata()
                .expect("new candidate mode")
                .permissions()
                .mode()
                & 0o7777,
            original_mode,
            "candidate must retain original narrow mode"
        );
        let published = replacement
            .persist(&path)
            .expect("atomic owned policy-leaf replacement");
        let replacement_metadata = published
            .metadata()
            .expect("published policy object identity");
        assert_ne!(
            (replacement_metadata.dev(), replacement_metadata.ino()),
            (original_metadata.dev(), original_metadata.ino()),
            "policy namespace did not select a fresh object"
        );
        assert_eq!(
            replacement_metadata.permissions().mode() & 0o7777,
            original_mode
        );
        let selected =
            fs::symlink_metadata(&path).expect("canonical staged policy after replacement");
        assert_eq!(
            (selected.dev(), selected.ino()),
            (replacement_metadata.dev(), replacement_metadata.ino())
        );
        drop(published);
        let changed = file_digest(&path);
        assert_ne!(changed, original, "actual disk policy did not change");
        println!(
            "KELD_KEL143_POLICY_DISK_EDIT original_sha256={original} edited_sha256={changed} original_dev={} original_ino={} edited_dev={} edited_ino={} mode=0o{original_mode:o} original_verified_policy_retained=true",
            original_metadata.dev(),
            original_metadata.ino(),
            replacement_metadata.dev(),
            replacement_metadata.ino()
        );
    }

    fn pump_failure_commands(&mut self) {
        while let Ok(command) = self.owner_commands.try_recv() {
            match command {
                GuardianOwnerCommand::FailGeneration(attempt, reply) => {
                    let result = self
                        .guardian
                        .fail_current_generation(attempt)
                        .map_err(|error| error.to_string());
                    reply.send(result).expect("reader retains failure receipt");
                }
                GuardianOwnerCommand::FailRetiredGeneration(attempt, reply) => {
                    let result = self
                        .guardian
                        .fail_current_generation(attempt)
                        .map(|()| Instant::now())
                        .map_err(|error| error.to_string());
                    reply
                        .send(result)
                        .expect("actual original failure receipt receiver");
                }
                _ => panic!("unexpected owner command at native-retirement cut"),
            }
        }
    }

    fn assert_native_held(&self) {
        assert!(
            matches!(
                self.signals.returned.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ),
            "native operation returned before the explicit release"
        );
        assert_eq!(
            self.snapshot
                .fs
                .state
                .lock()
                .expect("native accounting")
                .in_flight,
            1
        );
        assert_eq!(sensor::object_handle_count(&self.inputs.target), 1);
        assert!(sensor::directory_handle_count(&self.inputs.allowed_root) > 0);
        assert_eq!(fs::read(&self.inputs.target).expect("held bytes"), SENTINEL);
    }

    fn retire_while_held(&mut self) -> FsRecoveryAttempt {
        self.guardian
            .fail_current_generation(1)
            .expect("sole Runtime fault owner");
        let deadline = Instant::now() + OBSERVATION_BOUND;
        let mut recovery = None;
        loop {
            assert!(Instant::now() < deadline, "actual Revoked/Clear absent");
            self.pump_failure_commands();
            self.guardian
                .poll_fatal()
                .expect("guardian lives during native hold");
            while let Some(update) = self.guardian.recv_update(Duration::ZERO) {
                assert_no_successor(&update);
                let revoked = matches!(
                    &update,
                    GuardedPrimaryUpdate::Role(RoleEvent::Revoked { attempt: 1, .. })
                );
                self.handle()
                    .apply_generation_update(update.into())
                    .expect("actual old authority retirement");
                if revoked {
                    assert!(recovery.is_none());
                    recovery = Some(
                        FsRecoveryAttempt::retire(&self.handle(), 1)
                            .expect("retire old worker broker"),
                    );
                }
            }
            if let Some(recovery) = recovery.as_mut() {
                let handle = self.handle();
                recovery.poll(&handle).expect("nonblocking retirement poll");
                apply_guardian_recovery_arm(
                    RecoveryArm::Retired {
                        retired: 1,
                        prepared: 2,
                    },
                    &mut self.guardian,
                    Some(&handle),
                    &mut self.initial_arm,
                    Some(&mut *recovery),
                )
                .expect("existing owner arm remains closed during native work");
                assert!(
                    !recovery.released,
                    "Core released Runtime before native completion"
                );
            }
            if recovery.is_some() && self.guardian.group_pid().is_none() {
                break;
            }
            thread::yield_now();
        }
        self.assert_native_held();
        let recovery = recovery.expect("exact retired attempt");
        assert!(!recovery.ready && !recovery.preparing && !recovery.released);
        assert_eq!(
            self.handle().last_revoked_attempt.load(Ordering::Acquire),
            1
        );
        assert!(!self.handle().is_current(1));
        println!(
            "KELD_KEL143_HELD_RETIRED entries=1 returns=0 root_handles_retained=true successor_absent=true"
        );
        recovery
    }

    fn return_and_observe_old_drop(&mut self, recovery: &mut FsRecoveryAttempt) {
        self.signals
            .native_release
            .take()
            .expect("one native release")
            .send(())
            .expect("held worker");
        let returned = self
            .signals
            .returned
            .recv_timeout(OBSERVATION_BOUND)
            .expect("actual native normal return");
        assert!(!returned.fixture_fault);
        assert_eq!((returned.request_entries, returned.request_returns), (1, 1));
        let deadline = Instant::now() + OBSERVATION_BOUND;
        while !recovery.preparing {
            assert!(
                Instant::now() < deadline,
                "actual old-broker retirement receipt absent"
            );
            self.pump_failure_commands();
            while let Some(update) = self.guardian.recv_update(Duration::ZERO) {
                assert_no_successor(&update);
            }
            recovery
                .poll(&self.handle())
                .expect("natural Retired-before-Prepare cut");
            thread::yield_now();
        }
        assert!(!recovery.ready && !recovery.released);
        assert!(matches!(
            recovery.pending.as_ref(),
            Some(FsWorkerCommand::Prepare {
                retired: 1,
                attempt: 2,
                ..
            })
        ));
        assert_eq!(
            sensor::object_handle_count(&self.inputs.target),
            0,
            "returned call File survives"
        );
        assert_eq!(
            sensor::directory_handle_count(&self.inputs.allowed_root),
            0,
            "actual old broker roots survive Retired receipt"
        );
        assert_eq!(
            fs::read(&self.inputs.target).expect("retired independent effect"),
            SENTINEL
        );
        while let Some(update) = self.guardian.recv_update(Duration::ZERO) {
            assert_no_successor(&update);
        }
        println!(
            "KELD_KEL143_OLD_BROKER_BASELINE entries=1 returns=1 target_handles=0 root_handles=0 before_prepare=true"
        );
    }

    fn prepare_release_and_bind(&mut self, recovery: &mut FsRecoveryAttempt) {
        let deadline = Instant::now() + OBSERVATION_BOUND;
        while !recovery.released {
            assert!(Instant::now() < deadline, "fresh preflight/release absent");
            self.pump_failure_commands();
            while let Some(update) = self.guardian.recv_update(Duration::ZERO) {
                assert_no_successor(&update);
            }
            recovery
                .poll(&self.handle())
                .expect("same-worker fresh preparation");
            let handle = self.handle();
            apply_guardian_recovery_arm(
                RecoveryArm::Retired {
                    retired: 1,
                    prepared: 2,
                },
                &mut self.guardian,
                Some(&handle),
                &mut self.initial_arm,
                Some(&mut *recovery),
            )
            .expect("existing attempt-bound arm seam");
            thread::yield_now();
        }
        assert!(recovery.ready && sensor::directory_handle_count(&self.inputs.allowed_root) > 0);
        let digest = captured_digest(&self.inputs.permissions_sha256);
        assert_eq!(self.snapshot.verified.verified_sha256(), digest);
        assert_eq!(self.snapshot.fs.verified.verified_sha256(), digest);
        let bound = next_bound(&mut self.guardian, 2);
        assert_ne!(
            bound.generation(),
            self.initial_generation,
            "successor reused G1 identity"
        );
        let group = self.guardian.group_pid().expect("registered G2 group");
        assert_ne!(group, self.groups[0]);
        assert_owned_groups(&self.guardian, group);
        self.groups.push(group);
        self.handle()
            .apply_generation_update(GuardedPrimaryUpdate::Bound(bound).into())
            .expect("fresh authenticated successor installed");
        self.handle()
            .signal_ready()
            .expect("real G2 Ready on existing initial eligibility");
    }

    fn finish(&mut self) {
        self.guardian
            .accept_shutdown()
            .expect("actual owner Q acknowledgment");
        self.router
            .take()
            .expect("one router owner")
            .shutdown()
            .expect("worker/readers join");
        let fence = self
            .signals
            .final_snapshot
            .recv_timeout(OBSERVATION_BOUND)
            .expect("native scope final fence");
        assert!(!fence.fixture_fault);
        assert_eq!((fence.request_entries, fence.request_returns), (4, 4));
        assert_eq!(sensor::directory_handle_count(&self.inputs.allowed_root), 0);
        assert_eq!(sensor::object_handle_count(&self.inputs.target), 0);
        let status = self
            .guardian
            .shutdown()
            .expect("guardian EOF, owned wait and control join");
        assert!(status.success(), "actual guardian exit {status:?}");
        assert!(self.guardian.group_pid().is_none());
        for &group in &self.groups {
            let group = Pid::from_raw(i32::try_from(group).expect("diagnostic group fits"));
            assert_eq!(
                killpg(group, None::<Signal>),
                Err(Errno::ESRCH),
                "post-reap group remains (diagnostic only)"
            );
        }
        assert!(
            !self.inputs.stage_root.exists(),
            "guardian-owned normal stage cleanup absent"
        );
        println!(
            "KELD_KEL143_GUARDIAN_NATIVE_DONE status={status:?} entries=4 returns=4 roots=0 stage_absent=true"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.guardian.deny_recovery();
        if let Some(release) = self.signals.native_release.take() {
            let _ = release.send(());
        }
        for _ in 0..3 {
            let _ = self.gate_release.send(());
        }
        if let Some(router) = self.router.take() {
            let _ = router.shutdown();
        }
        // GuardedPrimary remains the only retained guardian Child/EOF/fail-safe owner.
        let _ = self.guardian.shutdown();
    }
}

fn assert_no_successor(update: &GuardedPrimaryUpdate) {
    println!("KELD_KEL143_REAL_EVENT {update:?}");
    match update {
        GuardedPrimaryUpdate::Bound(bound) => panic!("early Bound attempt {}", bound.attempt()),
        GuardedPrimaryUpdate::Role(
            RoleEvent::Provisioned { attempt, .. }
            | RoleEvent::Spawned { attempt, .. }
            | RoleEvent::LinkBound { attempt, .. },
        ) => assert_eq!(
            *attempt, 1,
            "successor authority appeared before old destruction/preflight"
        ),
        GuardedPrimaryUpdate::Role(_) => {}
    }
}

#[test]
#[ignore = "explicit physical macOS source-bound real guardian/native/root-census lane"]
fn real_guardian_waits_for_native_return_and_old_broker_directory_baseline() {
    if !support::is_native_child(CASE) {
        support::isolated_guardian_case(CASE, SELECTOR);
        return;
    }
    let mut fixture = Fixture::start();
    fixture.ready();
    fixture.edit_staged_policy_after_ready();
    let began = Instant::now();
    let old = roundtrip(&fixture.handle(), "00000000000000000000000000000143");
    fixture
        .gate_taken
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual G1 work taken");
    let held = fixture
        .signals
        .held
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual native after-walk hold");
    assert!(!held.fixture_fault);
    assert_eq!((held.request_entries, held.request_returns), (1, 0));
    assert!(held.walk_entries > 0 && held.target_opens > 0);
    let expired = public_outcome(&old);
    assert_eq!(expired["ok"], false);
    assert_eq!(
        expired["code"], "KELD-IPC-006",
        "actual public call outer expiry"
    );
    assert!(began.elapsed() >= Duration::from_secs(5));
    fixture.assert_native_held();
    let mut recovery = fixture.retire_while_held();
    fixture.return_and_observe_old_drop(&mut recovery);
    fixture.prepare_release_and_bind(&mut recovery);
    let successor = roundtrip(&fixture.handle(), "00000000000000000000000000000243");
    assert_eq!(
        public_outcome(&successor),
        json!({"ok":true, "bytes":fixture.inputs.expected_content})
    );
    assert_eq!(
        fs::read(&fixture.inputs.target).expect("independent actual G2 bytes"),
        fixture.inputs.expected_content
    );
    let denied = public_call(
        &fixture.handle(),
        "00000000000000000000000000000244",
        "deny",
        2,
    );
    let outcome = public_outcome_count(&denied, 2);
    assert_eq!(outcome["ok"], false);
    assert_eq!(outcome["code"], "KELD-GUARD002");
    assert_eq!(
        fs::read(&fixture.inputs.outside).expect("independent G2 outside sentinel"),
        fixture.inputs.outside_content,
        "successor performed an unauthorized effect"
    );
    println!("KELD_KEL143_G2_DENIED code=KELD-GUARD002 outside_unchanged=true");
    fixture.finish();
    println!(
        "KELD_AC7_CASE_DONE case={CASE} actual_guardian=true native_return_before_roots_baseline=true"
    );
}

/// Captures actual owner results; it neither selects shutdown nor owns a process.
#[derive(Default)]
struct CliOwnerFacts {
    attribution: Vec<Result<(), String>>,
    shutdown: Option<Result<(), String>>,
    shutdown_requests: usize,
    events: Vec<String>,
    delivery_faults: Vec<String>,
    unsupported: Vec<&'static str>,
}

impl CliOwnerFacts {
    fn release(&mut self, resume: &mut Option<Sender<()>>) {
        if let Some(resume) = resume.take()
            && let Err(error) = resume.send(())
        {
            self.delivery_faults.push(error.to_string());
        }
    }

    fn drive(
        &mut self,
        fixture: &mut Fixture,
        command: GuardianOwnerCommand,
        resume: &mut Option<Sender<()>>,
    ) {
        match command {
            GuardianOwnerCommand::PrepareAcceptedShutdown(reply) => {
                self.events.push(String::from("PrepareAcceptedShutdown"));
                self.release(resume);
                let actual = fixture
                    .guardian
                    .accept_shutdown()
                    .map_err(|error| error.to_string());
                println!("KELD_KEL143_DUPLICATE_Q_ACTUAL result={actual:?}");
                if let Err(error) = reply.send(actual.clone()) {
                    self.delivery_faults.push(error.to_string());
                }
                self.attribution.push(actual);
            }
            GuardianOwnerCommand::DenyRecovery => {
                self.events.push(String::from("DenyRecovery"));
                fixture.guardian.deny_recovery();
            }
            GuardianOwnerCommand::FailGeneration(attempt, reply) => {
                let actual = fixture
                    .guardian
                    .fail_current_generation(attempt)
                    .map_err(|error| error.to_string());
                self.events
                    .push(format!("FailGeneration({attempt})={actual:?}"));
                if let Err(error) = reply.send(actual) {
                    self.delivery_faults.push(error.to_string());
                }
            }
            GuardianOwnerCommand::FailRetiredGeneration(attempt, reply) => {
                let actual = fixture
                    .guardian
                    .fail_current_generation(attempt)
                    .map(|()| Instant::now())
                    .map_err(|error| error.to_string());
                self.events
                    .push(format!("FailRetiredGeneration({attempt})={actual:?}"));
                if let Err(error) = reply.send(actual) {
                    self.delivery_faults.push(error.to_string());
                }
            }
            GuardianOwnerCommand::Shutdown(reply) => {
                self.shutdown_requests += 1;
                self.release(resume);
                let actual = self.shutdown.get_or_insert_with(|| {
                    let actual = fixture.guardian.shutdown();
                    println!("KELD_KEL143_CLI_OWNER_SHUTDOWN_ACTUAL result={actual:?}");
                    actual.map(|_| ()).map_err(|error| error.to_string())
                });
                self.events.push(format!("Shutdown={actual:?}"));
                if let Err(error) = reply.send(actual.clone()) {
                    self.delivery_faults.push(error.to_string());
                }
            }
            GuardianOwnerCommand::ArmRecovery(phase, reply) => {
                let handle = fixture.handle();
                let actual = apply_guardian_recovery_arm(
                    phase,
                    &mut fixture.guardian,
                    Some(&handle),
                    &mut fixture.initial_arm,
                    None,
                )
                .map_err(|error| error.to_string());
                self.events.push(format!("ArmRecovery={actual:?}"));
                if let Err(error) = reply.send(actual) {
                    self.delivery_faults.push(error.to_string());
                }
            }
            GuardianOwnerCommand::AttachRouter(_, reply) => {
                // This fixture already has a router. Report unsupported input;
                // dropping its receipt wakes the requester without fabricating an ACK.
                self.unsupported.push("AttachRouter");
                self.release(resume);
                drop(reply);
            }
        }
    }
}

#[test]
#[ignore = "explicit physical macOS source-bound guardian CLI attribution race"]
fn cli_acceptance_publication_preserves_acknowledged_shutdown() {
    const CASE: &str = "cli-attribution-publication";
    const SELECTOR: &str = "app_session::tests::worker_link_e2e::native_observer_e2e::native_generation_recovery::cli_acceptance_publication_preserves_acknowledged_shutdown";
    if !support::is_native_child(CASE) {
        support::isolated_guardian_case(CASE, SELECTOR);
        return;
    }
    let mut fixture = Fixture::start();
    fixture.ready();
    let handle = fixture.handle();
    let (mut reader, caller) = {
        let current = handle.current.lock().expect("retained current generation");
        let generation = current.as_ref().expect("real G1");
        (
            generation
                .writer
                .try_clone()
                .expect("actual authenticated reader clone"),
            PrimaryCallerContext {
                attempt: generation.attempt,
                identity: generation.identity,
                reader_stop: Arc::clone(&generation.reader_stop),
            },
        )
    };
    let (published, publication) = mpsc::sync_channel(1);
    let (resume, release) = mpsc::channel();
    let mut resume = Some(resume);
    handle
        .quit_drain_hooks
        .lock()
        .expect("existing test-owned publication slot")
        .cli_acceptance = Some((published, release));
    assert!(handle.shutdown.reserve_cli_lease_lost());
    // Retain the real router's reader owner and let its reserved-stop poll finish.
    // The second thread below executes that exact reader routine at the post-Q cut.
    let stopped_by = Instant::now() + OBSERVATION_BOUND;
    while handle
        .readers
        .lock()
        .expect("retained reader table")
        .values()
        .any(|reader| !reader.is_finished())
    {
        assert!(
            Instant::now() < stopped_by,
            "reserved reader stop did not finish"
        );
        thread::yield_now();
    }
    let executor_handle = handle.clone();
    let executor = thread::spawn(move || executor_handle.cli_lease_lost());
    let GuardianOwnerCommand::PrepareAcceptedShutdown(first) = fixture
        .owner_commands
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual lease executor Q1 request")
    else {
        panic!("lease executor skipped owner attribution");
    };
    fixture
        .guardian
        .accept_shutdown()
        .expect("real Q1 acknowledgment");
    first.send(Ok(())).expect("actual Q1 receipt receiver");
    publication
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual accepted CLI publication cut");
    let primary_handle = handle.clone();
    let primary = thread::spawn(move || {
        super::super::super::read_primary_frames(&mut reader, &primary_handle, &caller)
    });
    let deadline = Instant::now() + OBSERVATION_BOUND;
    let mut facts = CliOwnerFacts::default();
    while !primary.is_finished() && Instant::now() < deadline {
        if let Ok(command) = fixture.owner_commands.try_recv() {
            facts.drive(&mut fixture, command, &mut resume);
        }
        thread::yield_now();
    }
    let reader_expired = !primary.is_finished();
    facts.release(&mut resume);
    // Shutdown can already be queued while the primary caller consumes its
    // actual Q2 result. Handle every existing owner command through this same
    // retained guardian before joining either caller or asserting the oracle.
    let terminal_by = Instant::now() + OBSERVATION_BOUND;
    while !executor.is_finished() && Instant::now() < terminal_by {
        if let Ok(command) = fixture.owner_commands.try_recv() {
            facts.drive(&mut fixture, command, &mut resume);
        }
        thread::yield_now();
    }
    let executor_expired = !executor.is_finished();
    let primary_result = primary.join().expect("actual primary reader joins");
    let executor_result = executor.join().expect("lease executor joins");
    while let Ok(command) = fixture.owner_commands.try_recv() {
        facts.drive(&mut fixture, command, &mut resume);
    }
    println!(
        "KELD_KEL143_CLI_PUBLICATION_RESULT attribution={:?} reader={primary_result:?} executor={executor_result:?} shutdown={:?} events={:?} delivery_faults={:?} unsupported={:?} reader_expired={reader_expired} executor_expired={executor_expired}",
        facts.attribution, facts.shutdown, facts.events, facts.delivery_faults, facts.unsupported
    );
    assert_cli_terminal(
        &fixture,
        &facts,
        primary_result,
        executor_result,
        (reader_expired, executor_expired),
    );
}

fn assert_cli_terminal(
    fixture: &Fixture,
    facts: &CliOwnerFacts,
    primary_result: Result<(), super::super::super::HostAppError>,
    executor_result: Result<(), super::super::super::HostAppError>,
    expired: (bool, bool),
) {
    let (reader_expired, executor_expired) = expired;
    assert!(
        facts.attribution.iter().all(Result::is_ok),
        "one accepted CLI stop caused an actual second-attribution failure: {:?}",
        facts.attribution
    );
    primary_result.expect("one attributed CLI stop cannot fail its primary reader");
    executor_result.expect("one attributed CLI stop cannot fail its executor");
    assert!(
        !reader_expired && !executor_expired,
        "caller exceeded its existing fixture phase bound"
    );
    assert!(
        facts.delivery_faults.is_empty() && facts.unsupported.is_empty(),
        "owner driver receipt fault"
    );
    assert_eq!(facts.shutdown_requests, 1, "one terminal tail");
    facts
        .shutdown
        .as_ref()
        .expect("actual terminal owner result")
        .as_ref()
        .expect("actual orderly guardian cleanup");
    assert_eq!(
        fixture
            .window
            .recv_timeout(OBSERVATION_BOUND)
            .expect("CLI UI wake"),
        AppWindowCommand::Quit
    );
    println!(
        "KELD_AC7_CASE_DONE case=cli-attribution-publication owner_receipt_outcomes={:?}",
        facts.attribution
    );
}
