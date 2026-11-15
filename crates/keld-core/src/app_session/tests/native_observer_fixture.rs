//! Native observer caller/router/drain ownership for the ordinary AC7 scenarios.
use super::super::super::{FsDispatchSession, GuardedFsTestSetup, GuardedTestRouter};
use super::super::WorkerLinkRole;
use super::support;
use crate::app_session::native_observer_setup::OBSERVATION_BOUND;
pub(super) use crate::app_session::native_observer_setup::{NativeSignals, native_setup};
use keld_native::fs::acceptance_observer::Snapshot;
use std::fs;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

/// Owns release controls before router/caller teardown, including failed assertions.
pub(super) struct NativeFixture {
    pub(super) signals: NativeSignals,
    role: Option<WorkerLinkRole>,
    router: Option<GuardedTestRouter>,
    pub(super) terminal_resume: Option<Sender<()>>,
}

impl NativeFixture {
    pub(super) fn start(case: &str, scenario: &str, hold_native: bool, pre_entry: bool) -> Self {
        Self::start_with_caller(case, hold_native, pre_entry, |setup| {
            WorkerLinkRole::start_public_fs_with_test_setup(scenario, Some(setup))
        })
    }

    pub(super) fn start_with_caller(
        case: &str,
        hold_native: bool,
        pre_entry: bool,
        caller: impl FnOnce(GuardedFsTestSetup) -> (WorkerLinkRole, GuardedTestRouter),
    ) -> Self {
        let (setup, signals) = native_setup(hold_native, pre_entry);
        let (role, router) = caller(setup);
        signals
            .initialized
            .recv_timeout(OBSERVATION_BOUND)
            .expect("observer actually installed on native worker");
        support::record_owned_caller(case, role.child.as_ref().expect("owned public caller"));
        Self {
            signals,
            role: Some(role),
            router: Some(router),
            terminal_resume: None,
        }
    }

    pub(super) fn router(&self) -> &GuardedTestRouter {
        self.router.as_ref().expect("owned native router")
    }

    pub(super) fn supply(&mut self, target: &Path, late: &Path) {
        self.role
            .as_mut()
            .expect("public caller custody")
            .supply_public_fs_targets(target, late);
    }

    pub(super) fn release_native(&mut self) {
        self.signals
            .native_release
            .take()
            .expect("one native release")
            .send(())
            .expect("native hold still owned");
    }

    pub(super) fn finish_caller(&mut self) -> std::collections::BTreeMap<String, String> {
        let role = self.role.take().expect("one public caller");
        let (report, pid, status) = role.finish_with_status();
        support::record_owned_caller_exit(pid, status);
        report
    }

    pub(super) fn assert_held_drain(&self, probe: &DrainProbe) {
        assert_eq!(
            self.router()
                .snapshot
                .fs
                .state
                .lock()
                .expect("held accounting")
                .in_flight,
            1,
            "lease accounting retired while independently observed native handle remains held"
        );
        assert!(
            matches!(probe.completed.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "drain returned before native release"
        );
        assert!(matches!(
            self.signals.returned.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }

    pub(super) fn finish_router(
        &mut self,
        target: &Path,
        late: &Path,
        expected_entries: usize,
        expected_bytes: &[u8],
    ) -> Snapshot {
        let router = self.router.take().expect("one router cleanup");
        router
            .router
            .shutdown()
            .expect("native router worker/readers join");
        let final_snapshot = self
            .signals
            .final_snapshot
            .recv_timeout(OBSERVATION_BOUND)
            .expect("required final native scope fence after worker join");
        assert!(
            !final_snapshot.fixture_fault,
            "invalid native evidence: {final_snapshot:?}"
        );
        assert_eq!(
            final_snapshot.request_entries, expected_entries,
            "native invocation census"
        );
        assert_eq!(
            final_snapshot.request_returns, expected_entries,
            "native normal-return census"
        );
        assert_eq!(
            support::object_handle_count(target),
            0,
            "native target handle survived cleanup"
        );
        assert_eq!(
            router
                .snapshot
                .fs
                .state
                .lock()
                .expect("joined native state")
                .in_flight,
            0,
            "owner lease survives joined worker"
        );
        assert_eq!(
            fs::read(target).expect("post-join independent bytes"),
            expected_bytes
        );
        assert!(!late.exists(), "late target changed before scope teardown");
        // The temporary scope is still present for every file/handle oracle.
        drop(router.snapshot);
        final_snapshot
    }
}

impl Drop for NativeFixture {
    fn drop(&mut self) {
        if let Some(release) = self.signals.native_release.take() {
            let _ = release.send(());
        }
        if let Some(release) = self.signals.pre_release.take() {
            let _ = release.send(());
        }
        if let Some(resume) = self.terminal_resume.take() {
            let _ = resume.send(());
        }
        if let Some(router) = &self.router {
            let _ = router.release.send(());
        }
        drop(self.role.take());
        drop(self.router.take());
    }
}

/// The real drain thread releases the held native operation before fallback join.
pub(super) struct DrainProbe {
    pub(super) completed: Receiver<()>,
    join: Option<JoinHandle<()>>,
    release_for_cleanup: Sender<()>,
}

impl DrainProbe {
    pub(super) fn start(session: Arc<FsDispatchSession>, release_for_cleanup: Sender<()>) -> Self {
        let (done, completed) = mpsc::sync_channel(1);
        let join = thread::spawn(move || {
            session.drain().expect("owning native lease drain");
            done.send(()).expect("drain completion witness");
        });
        Self {
            completed,
            join: Some(join),
            release_for_cleanup,
        }
    }

    pub(super) fn finish(mut self) {
        self.completed
            .recv_timeout(OBSERVATION_BOUND)
            .expect("native drain completes after release");
        self.join
            .take()
            .expect("owned drain thread")
            .join()
            .expect("drain thread joins");
    }
}

impl Drop for DrainProbe {
    fn drop(&mut self) {
        let _ = self.release_for_cleanup.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub(super) fn held_snapshot(fixture: &NativeFixture, case: &str, target: &Path) -> Snapshot {
    let held = fixture
        .signals
        .held
        .recv_timeout(OBSERVATION_BOUND)
        .expect("actual retained native hold");
    assert!(!held.fixture_fault, "invalid held evidence: {held:?}");
    assert_eq!(held.request_entries, 1);
    assert_eq!(
        held.request_returns, 0,
        "native return fabricated while object held"
    );
    assert!(
        held.walk_entries > 0 && held.target_opens > 0,
        "native retained walk absent: {held:?}"
    );
    assert_eq!(
        held.truncate_calls + held.create_calls + held.content_writes,
        0,
        "effect before release"
    );
    assert_eq!(
        support::object_handle_count(target),
        1,
        "native did not retain actual target handle"
    );
    assert!(matches!(
        fixture.signals.returned.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    let pending = fixture
        .router()
        .snapshot
        .fs
        .pending_call_for(1)
        .expect("actual admitted correlation");
    println!(
        "KELD_AC7_NATIVE_HELD case={case} pid={} attempt={} corr={:?} entries={} returns={} target_handles=1",
        std::process::id(),
        pending.attempt,
        pending.correlation,
        held.request_entries,
        held.request_returns
    );
    held
}
