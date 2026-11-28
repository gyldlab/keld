//! GH-451 draft transaction candidate; registered only in Core's registry tests.
//! No IPC, native termination, window authority or production behavior is
//! changed. The existing Host owner must supply these facts under its
//! transition/current/registry ordering before this can be integrated.

use std::num::{NonZeroU32, NonZeroU64};

#[cfg(test)]
#[path = "terminal_quit_core_owner_integration_candidate.rs"]
mod core_owner_integration;

#[cfg(all(test, not(keld_core_native_renderer_main)))]
#[path = "terminal_quit_core_real_fs_candidate.rs"]
mod real_fs;

#[cfg(all(test, not(keld_core_native_renderer_main)))]
#[path = "terminal_quit_core_dispatch_candidate.rs"]
mod real_dispatch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Token {
    generation: NonZeroU32,
    serial: NonZeroU64,
}

// Review fixture for the joint GH-451/GH-523 terminal cut. This is an
// already-validated owner input, not an Exit API or wire allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ValidatedExit {
    generation: NonZeroU32,
    request: NonZeroU64,
    code: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalWinner {
    Quit(Token),
    Exit(ValidatedExit),
}

impl TerminalWinner {
    fn fixture_code(self) -> i32 {
        match self {
            // A clean-exit fixture value, not an approved public quit event.
            Self::Quit(_) => 0,
            Self::Exit(exit) => exit.code,
        }
    }
}

/// Facts supplied by the existing Host owner, never by the calling app.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent test inputs, not a boolean encoding of transaction phases"
)]
#[derive(Clone, Copy)]
struct HostFacts {
    generation: Option<NonZeroU32>,
    qualified: bool,
    running: bool,
    registry_empty: bool,
    destruction_complete: bool,
    fs_terminal_drained: bool,
}

impl HostFacts {
    fn owns(self, token: Token) -> bool {
        self.qualified && self.generation == Some(token.generation)
    }

    fn can_prepare(self) -> bool {
        self.registry_empty && self.destruction_complete
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Prepared,
    Draining,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pending {
    token: Token,
    phase: Phase,
    invalidated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle { cancelled: Option<Token> },
    Pending(Pending),
    Committed(TerminalWinner),
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Prepared(Token),
    DrainRequired,
    AwaitingDrain,
    Cancelled,
    Retained,
    Committed,
    Refused,
    Busy,
    AlreadyTerminal,
    Unknown,
}

/// Draft state owned by the existing lifecycle coordinator, not a second
/// registry. The real owner must serialize every operation with Create,
/// including preload/adoption, and share the existing terminal reservation.
/// No new request IDs or wire discriminants are allocated here.
struct TerminalQuit {
    serial: u64,
    state: State,
}

impl TerminalQuit {
    fn new() -> Self {
        Self {
            serial: 0,
            state: State::Idle { cancelled: None },
        }
    }

    fn prepare(&mut self, facts: HostFacts) -> Outcome {
        if !facts.qualified || !facts.running {
            return Outcome::Refused;
        }
        let Some(generation) = facts.generation else {
            return Outcome::Refused;
        };
        match self.state {
            State::Idle { .. } => {}
            State::Pending(_) => return Outcome::Busy,
            State::Committed(_) => return Outcome::AlreadyTerminal,
            State::Unknown => return Outcome::Unknown,
        }
        if !facts.can_prepare() {
            return Outcome::Retained;
        }
        let Some(serial) = self.serial.checked_add(1).and_then(NonZeroU64::new) else {
            return Outcome::Refused;
        };
        self.serial = serial.get();
        let token = Token { generation, serial };
        self.state = State::Pending(Pending {
            token,
            phase: Phase::Prepared,
            invalidated: false,
        });
        Outcome::Prepared(token)
    }

    /// Call only at the successful existing Create admission cut. The real
    /// registry remains the sole source of window authority. A failed build
    /// cannot undo this invalidation; an unadmitted request does not call it.
    fn admit_create(&mut self) -> bool {
        match &mut self.state {
            State::Pending(pending) => pending.invalidated = true,
            State::Idle { .. } => {}
            State::Committed(_) | State::Unknown => return false,
        }
        true
    }

    fn cancel(&mut self, token: Token, facts: HostFacts) -> Outcome {
        if !facts.owns(token) || !facts.running {
            return Outcome::Refused;
        }
        match self.state {
            State::Idle {
                cancelled: Some(previous),
            } if previous == token => Outcome::Cancelled,
            State::Pending(pending)
                if pending.token == token && pending.phase == Phase::Prepared =>
            {
                self.state = State::Idle {
                    cancelled: Some(token),
                };
                Outcome::Cancelled
            }
            State::Committed(TerminalWinner::Quit(committed)) if committed == token => {
                Outcome::AlreadyTerminal
            }
            State::Unknown => Outcome::Unknown,
            _ => Outcome::Refused,
        }
    }

    /// Begin the reversible reader-dispatch pause. No terminal reservation
    /// occurs here. The real owner must drain existing FS terminal writes
    /// while the session is running and without holding the transition locks.
    fn begin_commit(&mut self, token: Token, facts: HostFacts) -> Outcome {
        if !facts.owns(token) {
            return Outcome::Refused;
        }
        let State::Pending(mut pending) = self.state else {
            return self.non_pending_outcome(token);
        };
        if pending.token != token {
            return Outcome::Refused;
        }
        if !facts.running {
            self.state = State::Unknown;
            return Outcome::Unknown;
        }
        if pending.phase != Phase::Prepared {
            return Outcome::Busy;
        }
        if pending.invalidated {
            self.state = State::Idle { cancelled: None };
            return Outcome::Retained;
        }
        if !facts.can_prepare() {
            self.state = State::Unknown;
            return Outcome::Unknown;
        }
        pending.phase = Phase::Draining;
        self.state = State::Pending(pending);
        Outcome::DrainRequired
    }

    /// Model the final guarded cut: recheck all facts, then execute the
    /// existing terminal reserve exactly once, inside that same cut.
    /// The closure must not perform cleanup or wait for native/FS work.
    /// The real reservation result is not inferred from a zero-window count.
    fn finish_commit(
        &mut self,
        token: Token,
        facts: HostFacts,
        reserve: impl FnOnce() -> bool,
    ) -> Outcome {
        if !facts.owns(token) {
            return Outcome::Refused;
        }
        let State::Pending(pending) = self.state else {
            return self.non_pending_outcome(token);
        };
        if pending.token != token || pending.phase != Phase::Draining {
            return Outcome::Refused;
        }
        if !facts.running {
            self.state = State::Unknown;
            return Outcome::Unknown;
        }
        if !facts.fs_terminal_drained {
            return Outcome::AwaitingDrain;
        }
        if pending.invalidated {
            self.state = State::Idle { cancelled: None };
            return Outcome::Retained;
        }
        if !facts.can_prepare() {
            self.state = State::Unknown;
            return Outcome::Unknown;
        }
        if !reserve() {
            self.state = State::Unknown;
            return Outcome::Unknown;
        }
        self.state = State::Committed(TerminalWinner::Quit(token));
        Outcome::Committed
    }

    // Test seam for GH-523's separately qualified Exit owner. The caller
    // must already satisfy that owner's admission and cleanup prerequisites.
    fn admit_validated_exit(
        &mut self,
        exit: ValidatedExit,
        facts: HostFacts,
        reserve: impl FnOnce() -> bool,
    ) -> Outcome {
        if !facts.qualified || facts.generation != Some(exit.generation) {
            return Outcome::Refused;
        }
        match self.state {
            State::Committed(TerminalWinner::Exit(committed)) if committed == exit => {
                return Outcome::AlreadyTerminal;
            }
            State::Committed(_) => return Outcome::Refused,
            State::Unknown => return Outcome::Unknown,
            State::Idle { .. } | State::Pending(_) => {}
        }
        if !facts.running {
            self.state = State::Unknown;
            return Outcome::Unknown;
        }
        // Either terminal contender must honor the same already-admitted
        // FS reply and lease obligations before the single final reserve.
        if !facts.fs_terminal_drained {
            return Outcome::AwaitingDrain;
        }
        if !reserve() {
            self.state = State::Unknown;
            return Outcome::Unknown;
        }
        // The existing owner's successful irreversible reservation wins.
        // Invalidate the reversible Quit and freeze the winning cause/code
        // in the same serialized decision; no second reservation is made.
        self.state = State::Committed(TerminalWinner::Exit(exit));
        Outcome::Committed
    }

    fn non_pending_outcome(&self, token: Token) -> Outcome {
        match self.state {
            // This is a status for the exact committed attempt, not a
            // successful receipt for an unrelated or retired token.
            State::Committed(TerminalWinner::Quit(committed)) if committed == token => {
                Outcome::AlreadyTerminal
            }
            State::Unknown => Outcome::Unknown,
            _ => Outcome::Refused,
        }
    }

    /// Only the existing lifecycle owner may report authoritative retirement.
    /// A late notification cannot cancel a successor or reset terminal state.
    fn retire_generation(&mut self, generation: NonZeroU32) {
        if matches!(self.state, State::Pending(p) if p.token.generation == generation)
            || matches!(self.state, State::Idle { cancelled: Some(t) } if t.generation == generation)
        {
            self.state = State::Idle { cancelled: None };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn generation(value: u32) -> NonZeroU32 {
        NonZeroU32::new(value).expect("fixture generation is nonzero")
    }

    fn ready(value: u32) -> HostFacts {
        HostFacts {
            generation: Some(generation(value)),
            qualified: true,
            running: true,
            registry_empty: true,
            destruction_complete: true,
            fs_terminal_drained: true,
        }
    }

    fn prepared(quit: &mut TerminalQuit, facts: HostFacts) -> Token {
        match quit.prepare(facts) {
            Outcome::Prepared(token) => token,
            other => panic!("expected prepared token, got {other:?}"),
        }
    }

    #[test]
    fn physical_destroyed_and_authoritative_empty_are_both_required() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        // CloseRequested or user-facing closed is not physical destruction.
        assert_eq!(
            quit.prepare(HostFacts {
                destruction_complete: false,
                ..facts
            }),
            Outcome::Retained
        );
        // Includes Opening, Closing and replacement windows.
        assert_eq!(
            quit.prepare(HostFacts {
                registry_empty: false,
                ..facts
            }),
            Outcome::Retained
        );
        assert!(matches!(quit.prepare(facts), Outcome::Prepared(_)));
    }

    #[test]
    fn unqualified_or_retired_owner_cannot_prepare() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        for denied in [
            HostFacts {
                qualified: false,
                ..facts
            },
            HostFacts {
                generation: None,
                ..facts
            },
            HostFacts {
                running: false,
                ..facts
            },
        ] {
            assert_eq!(quit.prepare(denied), Outcome::Refused);
        }
    }

    #[test]
    fn veto_is_idempotent_and_retry_mints_a_new_token() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let first = prepared(&mut quit, facts);
        assert_eq!(quit.cancel(first, facts), Outcome::Cancelled);
        assert_eq!(quit.cancel(first, facts), Outcome::Cancelled);
        let second = prepared(&mut quit, facts);
        assert_ne!(first, second);
        assert_eq!(quit.cancel(first, facts), Outcome::Refused);
        assert_eq!(quit.begin_commit(first, facts), Outcome::Refused);
        assert_eq!(quit.begin_commit(second, facts), Outcome::DrainRequired);
    }

    #[test]
    fn create_invalidates_even_after_transient_build_failure() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let token = prepared(&mut quit, facts);
        assert!(quit.admit_create());
        // The Create (normal/preload/adoption share this cut) failed and
        // current registry is empty again. The old token stays invalid.
        assert_eq!(quit.begin_commit(token, facts), Outcome::Retained);
        assert_ne!(token, prepared(&mut quit, facts));
    }

    #[test]
    fn create_during_drain_blocks_commit_even_after_window_disappears() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let token = prepared(&mut quit, facts);
        assert_eq!(quit.begin_commit(token, facts), Outcome::DrainRequired);
        assert!(quit.admit_create());
        let called = Cell::new(false);
        assert_eq!(
            quit.finish_commit(token, facts, || {
                called.set(true);
                true
            }),
            Outcome::Retained
        );
        assert!(!called.get());
        assert_ne!(token, prepared(&mut quit, facts));
    }

    #[test]
    fn fs_terminal_replies_must_finish_before_reservation() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let token = prepared(&mut quit, facts);
        let count = Cell::new(0);
        let reserve = || {
            count.set(count.get() + 1);
            true
        };
        assert_eq!(quit.begin_commit(token, facts), Outcome::DrainRequired);
        assert_eq!(
            quit.finish_commit(
                token,
                HostFacts {
                    fs_terminal_drained: false,
                    ..facts
                },
                reserve
            ),
            Outcome::AwaitingDrain
        );
        assert_eq!(count.get(), 0);
        assert_eq!(
            quit.finish_commit(token, facts, reserve),
            Outcome::Committed
        );
        assert_eq!(count.get(), 1);
        assert_eq!(
            quit.finish_commit(token, facts, reserve),
            Outcome::AlreadyTerminal
        );
        assert_eq!(count.get(), 1);
        assert!(!quit.admit_create());
    }

    #[test]
    fn unconfirmed_registry_change_preserves_unknown_disposition() {
        let facts = ready(1);
        for changed in [
            HostFacts {
                registry_empty: false,
                ..facts
            },
            HostFacts {
                destruction_complete: false,
                ..facts
            },
        ] {
            let mut before_drain = TerminalQuit::new();
            let before_token = prepared(&mut before_drain, facts);
            assert_eq!(
                before_drain.begin_commit(before_token, changed),
                Outcome::Unknown
            );
            assert_eq!(before_drain.prepare(facts), Outcome::Unknown);

            let mut quit = TerminalQuit::new();
            let token = prepared(&mut quit, facts);
            assert_eq!(quit.begin_commit(token, facts), Outcome::DrainRequired);
            assert_eq!(
                quit.finish_commit(token, changed, || panic!("must not reserve")),
                Outcome::Unknown
            );
            assert_eq!(quit.prepare(facts), Outcome::Unknown);
        }
    }

    #[test]
    fn stale_generation_and_late_retirement_cannot_spend_successor_token() {
        let mut quit = TerminalQuit::new();
        let old = ready(1);
        let current = ready(2);
        let first = prepared(&mut quit, old);
        quit.retire_generation(generation(1));
        let second = prepared(&mut quit, current);
        quit.retire_generation(generation(1));
        assert_eq!(quit.begin_commit(first, current), Outcome::Refused);
        assert_eq!(quit.begin_commit(second, old), Outcome::Refused);
        assert_eq!(quit.begin_commit(second, current), Outcome::DrainRequired);
        assert_eq!(
            quit.finish_commit(second, current, || true),
            Outcome::Committed
        );
    }

    #[test]
    fn final_cut_rejects_stale_generation_before_reservation() {
        let mut quit = TerminalQuit::new();
        let current = ready(2);
        let token = prepared(&mut quit, current);
        assert_eq!(quit.begin_commit(token, current), Outcome::DrainRequired);

        // Keep the exact token and draining phase. Change only the generation
        // observed at the final cut; all other admission facts stay valid.
        let stale = HostFacts {
            generation: Some(generation(1)),
            ..current
        };
        let reservations = Cell::new(0);
        let reserve = || {
            reservations.set(reservations.get() + 1);
            true
        };
        assert_eq!(
            quit.finish_commit(token, stale, reserve),
            Outcome::Refused,
            "stale generation must not reserve at the final cut"
        );
        assert_eq!(reservations.get(), 0);

        assert_eq!(
            quit.finish_commit(token, current, reserve),
            Outcome::Committed
        );
        assert_eq!(reservations.get(), 1);
    }

    #[test]
    fn competing_shutdown_is_not_a_retryable_retained_outcome() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let token = prepared(&mut quit, facts);
        assert_eq!(quit.begin_commit(token, facts), Outcome::DrainRequired);
        assert_eq!(
            quit.finish_commit(
                token,
                HostFacts {
                    running: false,
                    ..facts
                },
                || panic!("must not reserve")
            ),
            Outcome::Unknown
        );
        assert_eq!(quit.prepare(facts), Outcome::Unknown);
        assert!(!quit.admit_create());
    }

    #[test]
    fn failed_terminal_reservation_cannot_reopen_admission() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let token = prepared(&mut quit, facts);
        assert_eq!(quit.begin_commit(token, facts), Outcome::DrainRequired);
        assert_eq!(quit.finish_commit(token, facts, || false), Outcome::Unknown);
        assert_eq!(
            quit.finish_commit(token, facts, || panic!("must not reserve again")),
            Outcome::Unknown
        );
        assert_eq!(quit.prepare(facts), Outcome::Unknown);
        assert!(!quit.admit_create());
    }

    #[test]
    fn duplicate_prepare_and_early_commit_cannot_mint_or_reserve() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let token = prepared(&mut quit, facts);
        assert_eq!(quit.prepare(facts), Outcome::Busy);
        assert_eq!(
            quit.finish_commit(token, facts, || panic!("must not reserve")),
            Outcome::Refused
        );
        assert_eq!(quit.begin_commit(token, facts), Outcome::DrainRequired);
        assert_eq!(quit.begin_commit(token, facts), Outcome::Busy);
    }

    #[test]
    fn retired_attempt_does_not_claim_newer_terminal_receipt() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let old = prepared(&mut quit, facts);
        assert_eq!(quit.cancel(old, facts), Outcome::Cancelled);
        let winner = prepared(&mut quit, facts);
        assert_ne!(old, winner);
        let reserves = Cell::new(0);
        let reserve = || {
            reserves.set(reserves.get() + 1);
            true
        };
        assert_eq!(quit.begin_commit(winner, facts), Outcome::DrainRequired);
        assert_eq!(
            quit.finish_commit(winner, facts, reserve),
            Outcome::Committed
        );
        assert_eq!(quit.begin_commit(old, facts), Outcome::Refused);
        assert_eq!(quit.finish_commit(old, facts, reserve), Outcome::Refused);
        assert_eq!(quit.cancel(old, facts), Outcome::Refused);
        assert_eq!(quit.begin_commit(winner, facts), Outcome::AlreadyTerminal);
        assert_eq!(
            quit.finish_commit(winner, facts, reserve),
            Outcome::AlreadyTerminal
        );
        assert_eq!(quit.cancel(winner, facts), Outcome::AlreadyTerminal);
        assert_eq!(reserves.get(), 1);
    }

    #[test]
    fn shared_terminal_exit_first_revokes_prepared_quit_even_during_drain() {
        for draining in [false, true] {
            let mut quit = TerminalQuit::new();
            let facts = ready(1);
            let token = prepared(&mut quit, facts);
            if draining {
                assert_eq!(quit.begin_commit(token, facts), Outcome::DrainRequired);
            }
            let exit = ValidatedExit {
                generation: generation(1),
                request: NonZeroU64::new(42).expect("fixture request"),
                code: 7,
            };
            let reserves = Cell::new(0);
            let reserve = || {
                reserves.set(reserves.get() + 1);
                true
            };
            assert_eq!(
                quit.admit_validated_exit(exit, facts, reserve),
                Outcome::Committed
            );
            let State::Committed(winner) = quit.state else {
                panic!("missing single terminal winner");
            };
            assert_eq!(winner, TerminalWinner::Exit(exit));
            assert_eq!(winner.fixture_code(), 7);
            assert_eq!(quit.begin_commit(token, facts), Outcome::Refused);
            assert_eq!(quit.finish_commit(token, facts, reserve), Outcome::Refused);
            assert_eq!(quit.cancel(token, facts), Outcome::Refused);
            assert_eq!(
                quit.admit_validated_exit(exit, facts, reserve),
                Outcome::AlreadyTerminal
            );
            assert_eq!(reserves.get(), 1);
            assert!(!quit.admit_create());
        }
    }

    #[test]
    fn shared_terminal_quit_first_preserves_winner_and_clean_fixture_code() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let token = prepared(&mut quit, facts);
        let reserves = Cell::new(0);
        let reserve = || {
            reserves.set(reserves.get() + 1);
            true
        };
        assert_eq!(quit.begin_commit(token, facts), Outcome::DrainRequired);
        assert_eq!(
            quit.finish_commit(token, facts, reserve),
            Outcome::Committed
        );
        let exit = ValidatedExit {
            generation: generation(1),
            request: NonZeroU64::new(42).expect("fixture request"),
            code: 7,
        };
        assert_eq!(
            quit.admit_validated_exit(exit, facts, reserve),
            Outcome::Refused
        );
        let State::Committed(winner) = quit.state else {
            panic!("missing single terminal winner");
        };
        assert_eq!(winner, TerminalWinner::Quit(token));
        assert_eq!(winner.fixture_code(), 0);
        assert_eq!(reserves.get(), 1);
    }

    #[test]
    fn shared_terminal_exit_reservation_unknown_never_reopens_quit() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let token = prepared(&mut quit, facts);
        let exit = ValidatedExit {
            generation: generation(1),
            request: NonZeroU64::new(42).expect("fixture request"),
            code: 7,
        };
        assert_eq!(
            quit.admit_validated_exit(exit, facts, || false),
            Outcome::Unknown
        );
        assert_eq!(quit.begin_commit(token, facts), Outcome::Unknown);
        assert_eq!(
            quit.finish_commit(token, facts, || panic!("second reserve")),
            Outcome::Unknown
        );
        assert!(!quit.admit_create());
    }

    #[test]
    fn unqualified_exit_cannot_invalidate_a_prepared_quit_or_change_winner() {
        let mut quit = TerminalQuit::new();
        let facts = ready(1);
        let token = prepared(&mut quit, facts);
        let stale = ValidatedExit {
            generation: generation(2),
            request: NonZeroU64::new(42).expect("fixture request"),
            code: 7,
        };
        assert_eq!(
            quit.admit_validated_exit(stale, facts, || panic!("stale reserve")),
            Outcome::Refused
        );
        let current = ValidatedExit {
            generation: generation(1),
            ..stale
        };
        assert_eq!(
            quit.admit_validated_exit(
                current,
                HostFacts {
                    qualified: false,
                    ..facts
                },
                || panic!("unqualified reserve"),
            ),
            Outcome::Refused
        );
        assert_eq!(quit.begin_commit(token, facts), Outcome::DrainRequired);
        assert_eq!(
            quit.finish_commit(token, facts, || true),
            Outcome::Committed
        );
        assert_eq!(quit.state, State::Committed(TerminalWinner::Quit(token)));
    }

    // Model witness only: the fixture records reply delivery and lease
    // retirement separately. The real FS owner must supply both facts.
    #[derive(Default)]
    struct OwedFs {
        reply_written: Cell<bool>,
        lease_retired: Cell<bool>,
        reserves: Cell<u32>,
    }

    impl OwedFs {
        fn facts(&self) -> HostFacts {
            HostFacts {
                fs_terminal_drained: self.reply_written.get() && self.lease_retired.get(),
                ..ready(1)
            }
        }

        fn reserve(&self) -> bool {
            // The invocation count, rather than a simulated successful
            // outcome, is the independent observable of premature reserve.
            self.reserves.set(self.reserves.get() + 1);
            true
        }
    }

    fn exit_request(request: u64, code: i32) -> ValidatedExit {
        ValidatedExit {
            generation: generation(1),
            request: NonZeroU64::new(request).expect("fixture request"),
            code,
        }
    }

    #[test]
    fn shared_exit_final_cut_waits_for_owed_reply_and_lease() {
        for draining in [false, true] {
            let fs = OwedFs::default();
            let mut quit = TerminalQuit::new();
            let token = prepared(&mut quit, fs.facts());
            if draining {
                assert_eq!(quit.begin_commit(token, fs.facts()), Outcome::DrainRequired);
            }
            let pending = quit.state;
            let exit = exit_request(42, 7);
            // Exercise the final reservation seam directly, not a caller that
            // silently pre-filters the undrained input. This is a deliberately
            // stronger model oracle for the jointly selected Q6 precondition.
            for reply_written in [false, true] {
                fs.reply_written.set(reply_written);
                let outcome = quit.admit_validated_exit(exit, fs.facts(), || fs.reserve());
                assert_eq!(
                    fs.reserves.get(),
                    0,
                    "Exit reserved before both FS obligations settled; draining={draining}, reply_written={reply_written}"
                );
                assert_eq!(outcome, Outcome::AwaitingDrain);
                assert_eq!(
                    quit.state, pending,
                    "waiting must preserve the reversible attempt"
                );
            }
            fs.lease_retired.set(true);
            assert_eq!(
                quit.admit_validated_exit(exit, fs.facts(), || fs.reserve()),
                Outcome::Committed
            );
            assert_eq!(quit.state, State::Committed(TerminalWinner::Exit(exit)));
            let terminal = HostFacts {
                running: false,
                ..fs.facts()
            };
            assert_eq!(
                quit.finish_commit(token, terminal, || fs.reserve()),
                Outcome::Refused
            );
            assert_eq!(fs.reserves.get(), 1);
        }
    }

    #[test]
    fn invalidated_quit_keeps_draining_until_reply_and_lease_settle() {
        for reply_first in [false, true] {
            let fs = OwedFs::default();
            let mut quit = TerminalQuit::new();
            let token = prepared(&mut quit, fs.facts());
            assert_eq!(quit.begin_commit(token, fs.facts()), Outcome::DrainRequired);
            assert!(quit.admit_create());
            // The admitted Create has failed by the final cut; the registry is
            // empty, but the original invalidation and drain custody both remain.
            let pending = State::Pending(Pending {
                token,
                phase: Phase::Draining,
                invalidated: true,
            });
            assert_eq!(quit.state, pending);
            for first_settled in [false, true] {
                fs.reply_written.set(reply_first && first_settled);
                fs.lease_retired.set(!reply_first && first_settled);
                assert_eq!(
                    quit.finish_commit(token, fs.facts(), || fs.reserve()),
                    Outcome::AwaitingDrain,
                    "reply_first={reply_first}, first_settled={first_settled}"
                );
                assert_eq!(quit.state, pending);
                assert_eq!(fs.reserves.get(), 0);
            }

            fs.reply_written.set(true);
            fs.lease_retired.set(true);
            assert_eq!(
                quit.finish_commit(token, fs.facts(), || fs.reserve()),
                Outcome::Retained
            );
            assert_eq!(quit.state, State::Idle { cancelled: None });
            assert_eq!(fs.reserves.get(), 0);
            assert_ne!(prepared(&mut quit, fs.facts()), token);
        }
    }

    #[test]
    fn shared_quit_final_cut_waits_for_owed_reply_and_lease() {
        let fs = OwedFs::default();
        let mut quit = TerminalQuit::new();
        let token = prepared(&mut quit, fs.facts());
        assert_eq!(quit.begin_commit(token, fs.facts()), Outcome::DrainRequired);
        let pending = quit.state;
        for reply_written in [false, true] {
            fs.reply_written.set(reply_written);
            assert_eq!(
                quit.finish_commit(token, fs.facts(), || fs.reserve()),
                Outcome::AwaitingDrain
            );
            assert_eq!(fs.reserves.get(), 0);
            assert_eq!(quit.state, pending);
        }
        fs.lease_retired.set(true);
        assert_eq!(
            quit.finish_commit(token, fs.facts(), || fs.reserve()),
            Outcome::Committed
        );
        let expected = State::Committed(TerminalWinner::Quit(token));
        quit.retire_generation(token.generation);
        let terminal = HostFacts {
            running: false,
            ..fs.facts()
        };
        assert_eq!(
            quit.admit_validated_exit(exit_request(42, 7), terminal, || fs.reserve()),
            Outcome::Refused
        );
        assert_eq!(
            quit.finish_commit(token, terminal, || fs.reserve()),
            Outcome::AlreadyTerminal
        );
        assert_eq!(quit.state, expected);
        assert_eq!(fs.reserves.get(), 1);
        assert!(!quit.admit_create());
    }

    #[test]
    fn shared_exit_winner_cannot_change_code_identity_or_retirement() {
        let facts = ready(1);
        let mut quit = TerminalQuit::new();
        let token = prepared(&mut quit, facts);
        let exit = exit_request(42, 7);
        let reserves = Cell::new(0);
        let reserve = || {
            reserves.set(reserves.get() + 1);
            true
        };
        assert_eq!(
            quit.admit_validated_exit(exit, facts, reserve),
            Outcome::Committed
        );
        let expected = State::Committed(TerminalWinner::Exit(exit));
        quit.retire_generation(token.generation);
        let terminal = HostFacts {
            running: false,
            ..facts
        };
        // Keep identity and code as separate discriminants: changing either
        // must not obtain the winner's exact receipt or another reserve.
        for contender in [exit_request(42, 8), exit_request(43, 7)] {
            assert_eq!(
                quit.admit_validated_exit(contender, terminal, reserve),
                Outcome::Refused
            );
            assert_eq!(quit.state, expected);
        }
        assert_eq!(
            quit.admit_validated_exit(exit, terminal, reserve),
            Outcome::AlreadyTerminal
        );
        assert_eq!(
            quit.finish_commit(token, terminal, reserve),
            Outcome::Refused
        );
        assert_eq!(quit.state, expected);
        assert_eq!(reserves.get(), 1);
        assert!(!quit.admit_create());
    }

    #[test]
    fn serial_exhaustion_refuses_without_wrapping() {
        let mut quit = TerminalQuit::new();
        quit.serial = u64::MAX;
        assert_eq!(quit.prepare(ready(1)), Outcome::Refused);
        assert_eq!(quit.serial, u64::MAX);
    }
}
