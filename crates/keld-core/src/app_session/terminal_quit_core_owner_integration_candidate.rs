//! Local draft test module. Registration is deliberately excluded from shipping Core.
//! Intended parent: window_registry::tests::terminal_quit_candidate (the unchanged prototype).
//! The real registry and shutdown state are used; the fixture substitutes only authenticated
//! caller delivery, FS completion, and native event delivery. It does not claim native/IPC proof.

use super::super::super::super::{
    SESSION_CLI_LEASE_LOST, SESSION_LIFECYCLE_QUIT, SessionShutdownState,
};
use super::super::super::{
    BuildOutcome, CloseOutcome, CreateOutcome, WindowError, WindowOwner, WindowRegistry,
};
use super::super::{owner, snapshot};
use super::{HostFacts, Outcome, State, TerminalQuit, Token};
use keld_ipc::CorrelationId;
use keld_ipc::window::WindowEvent;
use keld_wv::WebviewId;
use std::num::NonZeroU32;
use std::sync::{Mutex, atomic::Ordering};

struct WindowCut {
    registry: WindowRegistry,
    terminal: TerminalQuit,
}

// Test-only owner seam. The shipping PrimaryRouterHandle must use its own
// transition -> authenticated current -> WindowSession lock ordering.
struct OwnerFixture {
    shutdown: SessionShutdownState,
    current: Mutex<Option<WindowOwner>>,
    windows: Mutex<WindowCut>,
}

impl OwnerFixture {
    fn new() -> Self {
        let mut registry = WindowRegistry::new();
        assert!(registry.mark_ready());
        Self {
            shutdown: SessionShutdownState::new(),
            current: Mutex::new(Some(owner(1))),
            windows: Mutex::new(WindowCut {
                registry,
                terminal: TerminalQuit::new(),
            }),
        }
    }

    fn cut<R>(
        &self,
        run: impl FnOnce(Option<WindowOwner>, &mut WindowCut, &SessionShutdownState) -> R,
    ) -> R {
        let _transition = self.shutdown.transition_guard();
        let current = self.current.lock().expect("fixture current");
        let mut windows = self.windows.lock().expect("fixture registry");
        run(*current, &mut windows, &self.shutdown)
    }

    // Physical-empty fact is derived from authoritative registry phases, not
    // user-facing close or a count of Created events. This relies on truthful
    // native WindowReleased/WindowBuildFailed delivery; the fixture cannot prove it.
    fn facts(
        current: Option<WindowOwner>,
        window: &WindowCut,
        shutdown: &SessionShutdownState,
        drained: bool,
    ) -> HostFacts {
        let empty = !window.registry.has_live_windows();
        HostFacts {
            generation: current.and_then(|current| NonZeroU32::new(current.attempt)),
            qualified: current == Some(owner(1))
                && window.registry.ready
                && !window.registry.ending,
            running: shutdown.is_running(),
            registry_empty: empty,
            destruction_complete: empty,
            fs_terminal_drained: drained,
        }
    }

    fn prepare(&self) -> Outcome {
        self.cut(|current, window, shutdown| {
            let facts = Self::facts(current, window, shutdown, false);
            window.terminal.prepare(facts)
        })
    }

    fn begin_commit(&self, token: Token) -> Outcome {
        self.cut(|current, window, shutdown| {
            let facts = Self::facts(current, window, shutdown, false);
            window.terminal.begin_commit(token, facts)
        })
    }

    fn finish_commit(&self, token: Token, drained: bool) -> Outcome {
        // The fixture's caller supplies drain completion only to model the seam.
        // Shipping Core must obtain it from the existing paused-reader FS owner.
        self.cut(|current, window, shutdown| {
            let facts = Self::facts(current, window, shutdown, drained);
            window.terminal.finish_commit(token, facts, || {
                shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT)
            })
        })
    }

    fn create(&self, correlation: u32) -> Option<Result<CreateOutcome, WindowError>> {
        self.cut(|current, window, shutdown| {
            let owner = current?;
            if !shutdown.is_running()
                || matches!(window.terminal.state, State::Committed(_) | State::Unknown)
            {
                return None;
            }
            let result =
                window
                    .registry
                    .create(owner, CorrelationId(correlation), 640, 480, None, None);
            if result.is_ok() {
                // The existing registry admission succeeded. Both Build and
                // Adopted must invalidate before the lock is released.
                assert!(window.terminal.admit_create());
            }
            Some(result)
        })
    }
}

fn token(fixture: &OwnerFixture) -> Token {
    match fixture.prepare() {
        Outcome::Prepared(token) => token,
        other => panic!("expected Prepared, received {other:?}"),
    }
}

fn create_build(fixture: &OwnerFixture, correlation: u32) -> u64 {
    match fixture.create(correlation) {
        Some(Ok(CreateOutcome::Build { build, .. })) => build,
        other => panic!("expected admitted Build, received {other:?}"),
    }
}

#[test]
fn real_registry_close_pending_and_closing_cannot_authorize_terminal() {
    let fixture = OwnerFixture::new();
    let build = create_build(&fixture, 1);
    assert_eq!(fixture.prepare(), Outcome::Retained); // Opening
    fixture.cut(|_, window, _| {
        assert!(matches!(
            window
                .registry
                .native_built(build, WebviewId(7), snapshot()),
            BuildOutcome::Created { .. }
        ));
    });
    assert_eq!(fixture.prepare(), Outcome::Retained); // Open
    fixture.cut(|_, window, _| {
        assert!(matches!(
            window.registry.native_close(WebviewId(7)),
            CloseOutcome::Requested { .. }
        ));
        assert!(window.registry.native_released(WebviewId(7)).diagnostic);
    });
    assert_eq!(fixture.prepare(), Outcome::Retained); // ClosePending
    fixture.cut(|_, window, _| {
        // Reuse actual sequence from registry, including the merged close.
        let CloseOutcome::Merged { close_seq } = window.registry.native_close(WebviewId(7)) else {
            panic!("expected the same pending close sequence");
        };
        let current = window
            .registry
            .replay(owner(1))
            .find_map(|event| match event {
                WindowEvent::Created { window, .. } => Some(window),
                _ => None,
            })
            .expect("live replay entry");
        assert!(matches!(
            window
                .registry
                .close_reply(
                    owner(1),
                    current,
                    close_seq,
                    keld_ipc::window::CloseVerdict::Allow
                )
                .expect("Allow"),
            CloseOutcome::Teardown(_)
        ));
        assert!(window.registry.native_released(WebviewId(99)).diagnostic);
    });
    assert_eq!(fixture.prepare(), Outcome::Retained); // Closing
    fixture.cut(|_, window, _| {
        let release = window.registry.native_released(WebviewId(7));
        assert!(!release.diagnostic);
        assert!(matches!(
            release.fact.expect("physical Destroyed").event,
            WindowEvent::Destroyed { .. }
        ));
        assert_eq!(release.last_window, Some(owner(1)));
        assert!(window.registry.native_released(WebviewId(7)).diagnostic);
    });
    let prepared = token(&fixture);
    assert_eq!(fixture.begin_commit(prepared), Outcome::DrainRequired);
    assert_eq!(
        fixture.finish_commit(prepared, false),
        Outcome::AwaitingDrain
    );
    assert!(fixture.shutdown.is_running());
    assert!(!fixture.shutdown.reader_stop.load(Ordering::Acquire));
    assert_eq!(fixture.finish_commit(prepared, true), Outcome::Committed);
    assert!(!fixture.shutdown.is_running());
    assert!(fixture.shutdown.reader_stop.load(Ordering::Acquire));
    assert!(fixture.create(2).is_none());
    assert_eq!(
        fixture.finish_commit(prepared, true),
        Outcome::AlreadyTerminal
    );
    fixture.cut(|_, _, shutdown| {
        assert!(shutdown.accept_reserved_guarded(SESSION_LIFECYCLE_QUIT));
        assert!(shutdown.begin_tail());
        assert!(!shutdown.accept_reserved_guarded(SESSION_LIFECYCLE_QUIT));
        assert!(!shutdown.begin_tail());
    });
}

#[test]
fn actual_opening_then_failure_invalidates_old_prepare_even_after_empty() {
    let fixture = OwnerFixture::new();
    let prepared = token(&fixture);
    let build = create_build(&fixture, 1);
    fixture.cut(|_, window, _| {
        let failed = window.registry.native_failed(build);
        assert!(!failed.diagnostic);
        assert!(failed.call.is_some());
        assert!(!window.registry.has_live_windows());
    });
    assert_eq!(fixture.begin_commit(prepared), Outcome::Retained);
    assert!(fixture.shutdown.is_running());
    assert_ne!(token(&fixture), prepared);
}

#[test]
fn actual_create_during_reversible_drain_cannot_be_hidden_by_build_failure() {
    let fixture = OwnerFixture::new();
    let prepared = token(&fixture);
    assert_eq!(fixture.begin_commit(prepared), Outcome::DrainRequired);
    // This can acquire the owner cut because begin_commit returned without
    // stopping the reader or reserving shutdown. No FS wait owns the mutexes.
    let build = create_build(&fixture, 1);
    fixture.cut(|_, window, _| {
        assert!(!window.registry.native_failed(build).diagnostic);
    });
    assert_eq!(fixture.finish_commit(prepared, true), Outcome::Retained);
    assert!(fixture.shutdown.is_running());
    assert!(!fixture.shutdown.reader_stop.load(Ordering::Acquire));
    assert_ne!(token(&fixture), prepared);
}

#[test]
fn refused_registry_create_has_no_invalidation_side_effect() {
    let fixture = OwnerFixture::new();
    let prepared = token(&fixture);
    fixture.cut(|_, window, _| {
        // Force the existing counter exhaustion refusal, before allocation.
        window.registry.last_id = u32::MAX;
    });
    assert!(matches!(
        fixture.create(1),
        Some(Err(WindowError::Exhausted(_)))
    ));
    assert_eq!(fixture.begin_commit(prepared), Outcome::DrainRequired);
    assert_eq!(fixture.finish_commit(prepared, true), Outcome::Committed);
}

#[test]
fn competing_existing_shutdown_reservation_is_unknown_and_no_create() {
    let fixture = OwnerFixture::new();
    let prepared = token(&fixture);
    assert_eq!(fixture.begin_commit(prepared), Outcome::DrainRequired);
    fixture.cut(|_, _, shutdown| {
        assert!(shutdown.reserve_guarded(SESSION_CLI_LEASE_LOST));
    });
    assert_eq!(fixture.finish_commit(prepared, true), Outcome::Unknown);
    assert!(fixture.create(1).is_none());
    fixture.cut(|_, _, shutdown| {
        assert!(!shutdown.reserve_guarded(SESSION_LIFECYCLE_QUIT));
    });
}
