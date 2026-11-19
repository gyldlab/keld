//! D7 stop ordering through the production resource helper.
//! Drop witnesses replace native resources; no HWND, COM object or process is made.

use super::super::browser_instance::BrowserBinding;
use super::super::{ProfileReleaseFault, ProfileReleaseWait, ReleaseSite};
use super::{ManagedCustody, close_managed_resources};
use crate::WebviewId;
use crate::engine::{AppWindowEvent, BuildStage};
use crate::webview2::managed_windows::{CreateAdmission, DeferredCreate, Releasing};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Running,
    Stopping,
}

type Trace = Rc<RefCell<Vec<(&'static str, Phase, bool)>>>;

struct State {
    phase: Rc<Cell<Phase>>,
    release: Rc<ProfileReleaseWait>,
    trace: Trace,
}

impl State {
    fn new() -> Self {
        Self {
            phase: Rc::new(Cell::new(Phase::Running)),
            release: Rc::new(ProfileReleaseWait::new(
                None,
                Arc::new(BrowserBinding::new()),
                None,
                ReleaseSite::Host,
            )),
            trace: Rc::default(),
        }
    }

    fn record(&self, kind: &'static str) {
        self.trace.borrow_mut().push((
            kind,
            self.phase.get(),
            self.release.armed_at.get().is_some(),
        ));
    }

    fn commit(&self) {
        self.phase.set(Phase::Stopping);
        self.record("stop");
    }

    fn resource(&self, kind: &'static str) -> DropWitness {
        DropWitness {
            kind,
            phase: Rc::clone(&self.phase),
            release: Rc::clone(&self.release),
            trace: Rc::clone(&self.trace),
        }
    }
}

struct DropWitness {
    kind: &'static str,
    phase: Rc<Cell<Phase>>,
    release: Rc<ProfileReleaseWait>,
    trace: Trace,
}

impl Drop for DropWitness {
    fn drop(&mut self) {
        // Record instead of asserting in Drop: the old-order control must fail
        // one ordinary assertion after cleanup, never double-panic while unwinding.
        self.trace.borrow_mut().push((
            self.kind,
            self.phase.get(),
            self.release.armed_at.get().is_some(),
        ));
    }
}

fn pending(builds: &[u64]) -> ManagedCustody<u32> {
    let mut custody = ManagedCustody::default();
    custody
        .park(7, 77, Releasing::Destroy(WebviewId(9)), Instant::now())
        .expect("pure pending key, never read by a native API");
    for build in builds {
        assert!(matches!(
            custody.admit(
                DeferredCreate {
                    build: *build,
                    width: 1,
                    height: 1,
                    x: None,
                    y: None,
                },
                false,
            ),
            CreateAdmission::Queued,
        ));
    }
    custody
}

fn failed_build(event: AppWindowEvent) -> u64 {
    match event {
        AppWindowEvent::WindowBuildFailed {
            build,
            stage: BuildStage::Webview,
        } => build,
        other => panic!("unexpected deferred terminal answer: {other:?}"),
    }
}

#[test]
fn managed_drops_precede_stop_and_deferred_refusal() {
    let state = State::new();
    let mut views = BTreeMap::from([
        (1, state.resource("managed")),
        (2, state.resource("managed")),
    ]);
    let mut legacy = BTreeMap::from([(1, state.resource("legacy"))]);
    let mut custody = pending(&[41, 42]);
    let mut answered = Vec::new();
    close_managed_resources(
        &mut views,
        &mut custody,
        || {
            state.commit();
            legacy.clear();
        },
        |event| {
            answered.push(failed_build(event));
            state.record("refuse");
            Ok::<(), ()>(())
        },
    )
    .expect("healthy terminal drain");
    state.release.arm();
    state.record("arm");

    assert!(views.is_empty());
    assert!(legacy.is_empty());
    assert_eq!(answered, vec![41, 42]);
    assert!(
        custody.contains(7),
        "pending native retirement is still owed"
    );
    assert_eq!(
        *state.trace.borrow(),
        vec![
            ("managed", Phase::Running, false),
            ("managed", Phase::Running, false),
            ("stop", Phase::Stopping, false),
            ("legacy", Phase::Stopping, false),
            ("refuse", Phase::Stopping, false),
            ("refuse", Phase::Stopping, false),
            ("arm", Phase::Stopping, true),
        ],
        "D7 requires managed resource drops before Stopping and deferred answers before arming",
    );
}

#[test]
fn empty_managed_resources_leave_legacy_stop_before_drop() {
    let state = State::new();
    let mut managed: BTreeMap<u32, DropWitness> = BTreeMap::new();
    let mut legacy = BTreeMap::from([(1, state.resource("legacy"))]);
    let mut custody = ManagedCustody::<u32>::default();
    close_managed_resources(
        &mut managed,
        &mut custody,
        || {
            state.commit();
            legacy.clear();
        },
        |_| panic!("an empty facade has no deferred answers"),
    )
    .unwrap_or_else(|never: std::convert::Infallible| match never {});
    state.release.arm();
    state.record("arm");

    assert!(legacy.is_empty());
    assert_eq!(
        *state.trace.borrow(),
        vec![
            ("stop", Phase::Stopping, false),
            ("legacy", Phase::Stopping, false),
            ("arm", Phase::Stopping, true),
        ],
        "the renderer callback retains its existing commit, drop, arm order",
    );
}

#[test]
fn refusal_error_drains_once_after_close_and_preserves_native_custody() {
    let state = State::new();
    let mut views = BTreeMap::from([(1, state.resource("managed"))]);
    let mut custody = pending(&[51, 52]);
    let mut attempted = Vec::new();
    let result = close_managed_resources(
        &mut views,
        &mut custody,
        || state.commit(),
        |event| {
            let build = failed_build(event);
            attempted.push(build);
            state.record("refuse");
            Err(if build == 51 {
                "first owner error"
            } else {
                "later owner error"
            })
        },
    );
    state.release.arm();
    state.record("arm");

    assert_eq!(result, Err("first owner error"));
    assert_eq!(
        attempted,
        vec![51, 52],
        "owner failure cannot leave a queued answer"
    );
    assert!(views.is_empty());
    assert!(custody.contains(7), "a send error is not native retirement");
    assert!(!custody.exit_ready(true, true));
    let again: Result<(), ()> = custody.refuse_deferred(|_| panic!("a refusal was replayed"));
    assert_eq!(again, Ok(()));
    assert_eq!(
        *state.trace.borrow(),
        vec![
            ("managed", Phase::Running, false),
            ("stop", Phase::Stopping, false),
            ("refuse", Phase::Stopping, false),
            ("refuse", Phase::Stopping, false),
            ("arm", Phase::Stopping, true),
        ],
    );
}

#[test]
fn only_a_latched_fault_runs_the_shared_stop_reaction() {
    let state = State::new();
    assert!(!state.release.poll_host_receipts_with(|| {
        panic!("healthy polling must not stop native resources");
    }));
    assert!(state.release.armed_at.get().is_none());
    state.release.latch_fault(ProfileReleaseFault::AbnormalExit);
    let mut views = BTreeMap::from([(1, state.resource("managed"))]);
    let mut custody = ManagedCustody::<u32>::default();
    assert!(state.release.poll_host_receipts_with(|| {
        close_managed_resources(
            &mut views,
            &mut custody,
            || state.commit(),
            |_| Ok::<(), ()>(()),
        )
        .expect("empty deferred fault drain");
        state.release.arm();
        state.record("arm");
    }));

    assert_eq!(
        state.release.fault(),
        Some(ProfileReleaseFault::AbnormalExit)
    );
    assert!(views.is_empty());
    assert_eq!(
        *state.trace.borrow(),
        vec![
            ("managed", Phase::Running, false),
            ("stop", Phase::Stopping, false),
            ("arm", Phase::Stopping, true),
        ],
        "the existing first-fault owner invokes the same ordered stop reaction",
    );
}
