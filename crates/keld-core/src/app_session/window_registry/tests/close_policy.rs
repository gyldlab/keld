//! GH-531 close, loss, correlation and constructor-adoption invariants.
use super::super::{
    AbandonedClose, CloseOutcome, CreateOutcome, FactOutcome, MAX_PENDING_DESTROYS_PER_SESSION,
    MAX_PENDING_DESTROYS_PER_WINDOW, PendingWindowCall, Phase, SessionEffect, Teardown,
    WindowCounter, WindowError, WindowRegistry,
};
use super::{TestResult, begin, complete, owner, phase_name, ready, snapshot};
use keld_ipc::CorrelationId;
use keld_ipc::window::{CloseVerdict, WindowEvent, WindowRef};
use keld_wv::WebviewId;

#[test]
fn vetoes_advance_sequences_and_stale_allow_cannot_close_the_current_attempt() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    for close_seq in 1..=3 {
        assert!(
            matches!(registry.request_close(owner(1), window)?, CloseOutcome::Requested { close_seq: seq, .. } if seq == close_seq)
        );
        assert_eq!(
            registry.request_close(owner(1), window)?,
            CloseOutcome::Merged { close_seq }
        );
        assert_eq!(
            registry.close_reply(owner(1), window, close_seq - 1, CloseVerdict::Allow)?,
            CloseOutcome::Stale
        );
        assert_eq!(
            registry.replay(owner(1)).next(),
            Some(WindowEvent::Created {
                window,
                state: snapshot(),
                pending_close: Some(close_seq)
            })
        );
        assert_eq!(
            registry.close_reply(owner(1), window, close_seq, CloseVerdict::Veto)?,
            CloseOutcome::Vetoed
        );
        assert!(matches!(
            registry.entries[&window.id.0].phase,
            Phase::Open(_)
        ));
    }
    Ok(())
}

#[test]
fn exhausted_close_counter_never_wraps_and_explicit_destroy_still_releases() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    registry
        .entries
        .get_mut(&window.id.0)
        .expect("entry")
        .last_close_seq = u64::MAX;
    assert_eq!(
        registry.request_close(owner(1), window),
        Err(WindowError::Exhausted(WindowCounter::CloseSequence))
    );
    assert_eq!(registry.native_close(WebviewId(7)), CloseOutcome::Exhausted);
    assert!(matches!(
        registry.entries[&window.id.0].phase,
        Phase::Open(_)
    ));
    assert_eq!(
        registry.destroy(owner(1), window, CorrelationId(2), |_| panic!(
            "no request pending"
        ))?,
        CloseOutcome::Teardown(Teardown {
            window,
            view: WebviewId(7)
        })
    );
    let released = registry.native_released(WebviewId(7));
    assert_eq!(
        released.destroy_calls,
        vec![PendingWindowCall {
            owner: owner(1),
            correlation: CorrelationId(2)
        }]
    );
    assert!(!released.diagnostic);
    Ok(())
}

#[test]
fn repeated_destroy_has_one_teardown_and_all_replies_wait_for_release() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    registry.native_close(WebviewId(7));
    let mut abandoned = Vec::new();
    assert!(matches!(
        registry.destroy(owner(1), window, CorrelationId(2), |close| abandoned
            .push(close))?,
        CloseOutcome::Teardown(_)
    ));
    assert_eq!(
        abandoned,
        vec![AbandonedClose {
            window,
            close_seq: 1
        }]
    );
    assert_eq!(
        registry.destroy(owner(1), window, CorrelationId(3), |_| panic!(
            "cannot abandon twice"
        ))?,
        CloseOutcome::AlreadyClosing
    );
    assert!(registry.has_pending_call(owner(1), CorrelationId(2)));
    assert!(registry.has_pending_call(owner(1), CorrelationId(3)));
    let release = registry.native_released(WebviewId(7));
    assert_eq!(
        release.fact.expect("actual release fact").event,
        WindowEvent::Destroyed { window }
    );
    assert_eq!(release.last_window, Some(owner(1)));
    assert_eq!(
        release
            .destroy_calls
            .iter()
            .map(|call| call.correlation)
            .collect::<Vec<_>>(),
        vec![CorrelationId(2), CorrelationId(3)]
    );
    assert!(!registry.has_pending_call(owner(1), CorrelationId(2)));
    assert_eq!(registry.native_released(WebviewId(7)).destroy_calls, []);
    Ok(())
}

#[test]
fn delayed_native_release_caps_distinct_destroy_calls_without_losing_accepted_replies() -> TestResult
{
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    // One delayed native Released leaves the Host holding correlation custody.
    for corr in 2..=65 {
        let result = registry.destroy(owner(1), window, CorrelationId(corr), |_| {})?;
        if corr == 2 {
            assert!(matches!(result, CloseOutcome::Teardown(_)));
        } else {
            assert_eq!(result, CloseOutcome::AlreadyClosing);
        }
    }
    assert_eq!(registry.entries[&window.id.0].destroy_calls.len(), 64);
    let refusal = registry
        .destroy(owner(1), window, CorrelationId(66), |_| {})
        .unwrap_err();
    assert_eq!(
        refusal.code(),
        "KELD-CORE-046",
        "typed nonterminal admission refusal"
    );
    assert!(!registry.has_pending_call(owner(1), CorrelationId(66)));
    assert_eq!(registry.entries[&window.id.0].destroy_calls.len(), 64);
    assert_eq!(
        registry.request_close(owner(1), window)?,
        CloseOutcome::AlreadyClosing
    );

    let released = registry.native_released(WebviewId(7));
    assert_eq!(
        released.fact.expect("Host release").event,
        WindowEvent::Destroyed { window }
    );
    assert_eq!(released.last_window, Some(owner(1)));
    assert_eq!(released.destroy_calls.len(), 64);
    for (offset, call) in released.destroy_calls.iter().enumerate() {
        assert_eq!(
            (call.owner, call.correlation),
            (
                owner(1),
                CorrelationId(u32::try_from(offset).expect("at most 64 correlations") + 2)
            )
        );
    }
    assert!(!registry.has_pending_call(owner(1), CorrelationId(65)));
    assert_eq!(
        registry.native_released(WebviewId(7)).destroy_calls,
        Vec::<PendingWindowCall>::new()
    );
    Ok(())
}

#[test]
fn role_loss_at_destroy_capacity_retires_every_correlation_without_successor_replies() -> TestResult
{
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    for corr in 2..=65 {
        registry.destroy(owner(1), window, CorrelationId(corr), |_| {})?;
    }
    registry.role_lost(owner(1), |_| {});
    for corr in 2..=65 {
        assert!(!registry.has_pending_call(owner(1), CorrelationId(corr)));
    }
    registry.subscribe_successor(owner(2));
    assert_eq!(registry.replay(owner(2)).count(), 0);
    assert_eq!(
        registry.native_released(WebviewId(7)).destroy_calls,
        Vec::<PendingWindowCall>::new()
    );
    Ok(())
}

#[test]
fn role_loss_abandons_once_and_new_gap_close_is_held_until_terminal_end() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    registry.native_close(WebviewId(7));
    let mut abandoned = Vec::new();
    registry.role_lost(owner(1), |close| abandoned.push(close));
    registry.role_lost(owner(1), |_| panic!("loss cannot abandon twice"));
    assert_eq!(
        abandoned,
        vec![AbandonedClose {
            window,
            close_seq: 1
        }]
    );
    assert!(matches!(
        registry.entries[&window.id.0].phase,
        Phase::Open(_)
    ));
    assert_eq!(
        registry.close_reply(owner(1), window, 1, CloseVerdict::Allow),
        Err(WindowError::WrongOwner)
    );
    assert_eq!(
        registry.native_close(WebviewId(7)),
        CloseOutcome::Requested {
            close_seq: 2,
            fact: None
        }
    );
    assert_eq!(
        registry.native_close(WebviewId(7)),
        CloseOutcome::Merged { close_seq: 2 }
    );
    assert_eq!(
        registry.native_focus(WebviewId(7), false),
        FactOutcome::Suppressed
    );
    let mut effects = Vec::new();
    registry.session_end(|effect| effects.push(effect));
    assert_eq!(
        effects,
        vec![
            SessionEffect::CloseAbandoned(AbandonedClose {
                window,
                close_seq: 2
            }),
            SessionEffect::Teardown(Teardown {
                window,
                view: WebviewId(7)
            })
        ]
    );
    registry.session_end(|_| panic!("terminal end is idempotent"));
    let release = registry.native_released(WebviewId(7));
    assert!(release.fact.is_none() && release.last_window.is_none());
    Ok(())
}

#[test]
fn retired_destroy_correlations_cannot_reach_a_successor() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    registry.destroy(owner(1), window, CorrelationId(2), |_| {})?;
    registry.role_lost(owner(1), |_| {});
    assert!(!registry.has_pending_call(owner(1), CorrelationId(2)));
    registry.subscribe_successor(owner(2));
    assert_eq!(registry.replay(owner(2)).count(), 0);
    let release = registry.native_released(WebviewId(7));
    assert!(release.fact.is_none() && release.destroy_calls.is_empty());
    Ok(())
}

#[test]
fn one_window_veto_cannot_latch_another_windows_close() -> TestResult {
    let mut registry = ready();
    let first = complete(&mut registry, owner(1), 1, 7)?;
    let second = complete(&mut registry, owner(1), 2, 8)?;
    registry.request_close(owner(1), first)?;
    registry.close_reply(owner(1), first, 1, CloseVerdict::Veto)?;
    assert!(matches!(
        registry.request_close(owner(1), second)?,
        CloseOutcome::Requested { close_seq: 1, .. }
    ));
    assert!(matches!(
        registry.entries[&first.id.0].phase,
        Phase::Open(_)
    ));
    Ok(())
}

#[test]
fn gap_pending_transfers_and_replays_without_gaining_a_constructor_adoption_mark() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    registry.native_close(WebviewId(7));
    let mut abandoned = Vec::new();
    registry.role_lost(owner(1), |close| abandoned.push(close));
    assert_eq!(
        abandoned,
        vec![AbandonedClose {
            window,
            close_seq: 1
        }]
    );
    assert_eq!(
        registry.native_close(WebviewId(7)),
        CloseOutcome::Requested {
            close_seq: 2,
            fact: None
        }
    );
    registry.subscribe_successor(owner(2));
    assert_eq!(
        registry.replay(owner(2)).collect::<Vec<_>>(),
        vec![WindowEvent::Created {
            window,
            state: snapshot(),
            pending_close: Some(2)
        }]
    );
    assert!(!registry.entries[&window.id.0].adoptable);
    let (_, next) = begin(&mut registry, owner(2), 2)?;
    assert_ne!(
        next, window,
        "no eligible mark means a legitimate new native build"
    );
    assert!(
        matches!(registry.entries[&window.id.0].phase, Phase::ClosePending(native, 2) if native.view == WebviewId(7))
    );
    assert_eq!(
        registry.close_reply(owner(1), window, 2, CloseVerdict::Allow),
        Err(WindowError::WrongOwner)
    );
    assert_eq!(
        registry.close_reply(owner(2), window, 2, CloseVerdict::Veto)?,
        CloseOutcome::Vetoed
    );
    Ok(())
}

#[test]
fn native_close_after_subscribe_preserves_the_mark_and_adoption_changes_no_window_phase()
-> TestResult {
    let mut registry = ready();
    let first = complete(&mut registry, owner(1), 1, 7)?;
    let second = complete(&mut registry, owner(1), 2, 8)?;
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    registry.native_close(WebviewId(7));
    assert!(registry.entries[&first.id.0].adoptable);
    assert_eq!(
        registry.set_maximized(owner(1), first, true),
        Err(WindowError::WrongOwner)
    );
    assert!(
        registry.entries[&first.id.0].adoptable,
        "wrong-owner calls cannot clear the mark"
    );
    let forged = keld_ipc::window::WindowRef {
        incarnation: keld_ipc::window::WindowGeneration(first.incarnation.0 + 1),
        ..first
    };
    assert_eq!(
        registry.request_close(owner(2), forged),
        Err(WindowError::StaleWindow)
    );
    assert!(
        registry.entries[&first.id.0].adoptable,
        "a mismatched window incarnation cannot consume adoption"
    );
    let before = registry
        .entries
        .values()
        .map(|entry| (entry.window, phase_name(entry.phase)))
        .collect::<Vec<_>>();
    assert_eq!(
        registry.create(owner(2), CorrelationId(3), 1, 1, None, None)?,
        CreateOutcome::Adopted(first)
    );
    assert_eq!(
        registry
            .entries
            .values()
            .map(|entry| (entry.window, phase_name(entry.phase)))
            .collect::<Vec<_>>(),
        before
    );
    assert!(
        matches!(registry.entries[&first.id.0].phase, Phase::ClosePending(native, 1) if native.view == WebviewId(7))
    );
    assert_eq!(
        registry.create(owner(2), CorrelationId(4), 1, 1, None, None)?,
        CreateOutcome::Adopted(second)
    );
    assert_eq!(
        registry.close_reply(owner(2), first, 1, CloseVerdict::Veto)?,
        CloseOutcome::Vetoed
    );
    let (_, fresh) = begin(&mut registry, owner(2), 5)?;
    assert_ne!(fresh, first);
    Ok(())
}

fn fill_session_destroy_budget(registry: &mut WindowRegistry, windows: &[WindowRef]) -> TestResult {
    let per_window = u32::try_from(MAX_PENDING_DESTROYS_PER_WINDOW)?;
    for window_index in 0..16u32 {
        let window = windows[usize::try_from(window_index)?];
        for offset in 0..per_window {
            let correlation = 10_000 + window_index * per_window + offset;
            let mut abandoned = 0;
            let outcome = registry.destroy(owner(1), window, CorrelationId(correlation), |_| {
                abandoned += 1;
            })?;
            assert_eq!(abandoned, 0, "Open window cannot abandon a pending close");
            if offset == 0 {
                assert!(matches!(outcome, CloseOutcome::Teardown(_)));
            } else {
                assert_eq!(outcome, CloseOutcome::AlreadyClosing);
            }
        }
    }
    assert_eq!(
        registry.pending_destroy_calls,
        MAX_PENDING_DESTROYS_PER_SESSION
    );
    Ok(())
}

fn put_target_in_close_pending(registry: &mut WindowRegistry, target: WindowRef) -> TestResult {
    let entry = registry
        .entries
        .get_mut(&target.id.0)
        .ok_or_else(|| std::io::Error::other("target window must exist"))?;
    let Phase::Open(native) = entry.phase else {
        return Err(std::io::Error::other("target starts Open").into());
    };
    entry.phase = Phase::ClosePending(native, 1);
    entry.last_close_seq = 1;
    entry.adoptable = true;
    Ok(())
}

fn refuse_destroy_at_session_limit(registry: &mut WindowRegistry, target: WindowRef) -> TestResult {
    let mut abandoned = 0;
    let refusal = registry
        .destroy(owner(1), target, CorrelationId(50_000), |_| abandoned += 1)
        .err()
        .ok_or_else(|| std::io::Error::other("1,025th session call must be refused"))?;
    assert_eq!(refusal.code(), "KELD-CORE-048");
    assert_eq!(abandoned, 0, "refusal cannot abandon the pending close");
    assert!(!registry.has_pending_call(owner(1), CorrelationId(50_000)));
    let target_entry = &registry.entries[&target.id.0];
    assert!(matches!(target_entry.phase, Phase::ClosePending(_, 1)));
    assert!(target_entry.adoptable, "refusal cannot consume adoption");
    assert_eq!(target_entry.destroy_calls.len(), 0);
    assert_eq!(
        registry.pending_destroy_calls,
        MAX_PENDING_DESTROYS_PER_SESSION
    );
    Ok(())
}

fn release_first_window_and_check_fifo(registry: &mut WindowRegistry) -> TestResult {
    let released = registry.native_released(WebviewId(100));
    assert_eq!(
        released.destroy_calls.len(),
        MAX_PENDING_DESTROYS_PER_WINDOW
    );
    for (offset, call) in released.destroy_calls.iter().enumerate() {
        assert_eq!(
            call.correlation,
            CorrelationId(10_000 + u32::try_from(offset)?),
            "native Released preserves the accepted FIFO"
        );
    }
    assert_eq!(
        registry.pending_destroy_calls,
        MAX_PENDING_DESTROYS_PER_SESSION - MAX_PENDING_DESTROYS_PER_WINDOW
    );
    Ok(())
}

fn admit_target_after_release(registry: &mut WindowRegistry, target: WindowRef) -> TestResult {
    let mut abandoned = 0;
    let accepted = registry.destroy(owner(1), target, CorrelationId(50_001), |close| {
        assert_eq!(close.window, target);
        assert_eq!(close.close_seq, 1);
        abandoned += 1;
    })?;
    assert_eq!(
        accepted,
        CloseOutcome::Teardown(Teardown {
            window: target,
            view: WebviewId(116),
        })
    );
    assert_eq!(abandoned, 1, "the released slot admits the next Destroy");
    assert_eq!(
        registry.pending_destroy_calls,
        MAX_PENDING_DESTROYS_PER_SESSION - MAX_PENDING_DESTROYS_PER_WINDOW + 1
    );
    Ok(())
}

fn retire_old_owner_and_check_successor(registry: &mut WindowRegistry) -> TestResult {
    registry.role_lost(owner(1), |_| {});
    assert_eq!(registry.pending_destroy_calls, 0);
    assert!(!registry.has_pending_call(owner(1), CorrelationId(50_001)));
    for view in 101..=116 {
        assert_eq!(
            registry.native_released(WebviewId(view)).destroy_calls,
            Vec::<PendingWindowCall>::new(),
            "late native release cannot deliver or retire a correlation twice"
        );
        assert_eq!(registry.pending_destroy_calls, 0);
    }

    registry.subscribe_successor(owner(2));
    let successor_window = complete(registry, owner(2), 60_000, 200)?;
    assert_eq!(
        registry.destroy(owner(2), successor_window, CorrelationId(60_001), |_| {})?,
        CloseOutcome::Teardown(Teardown {
            window: successor_window,
            view: WebviewId(200),
        })
    );
    assert_eq!(registry.pending_destroy_calls, 1);
    let successor_release = registry.native_released(WebviewId(200));
    assert_eq!(
        successor_release.destroy_calls,
        vec![PendingWindowCall {
            owner: owner(2),
            correlation: CorrelationId(60_001),
        }]
    );
    assert_eq!(registry.pending_destroy_calls, 0);
    Ok(())
}

#[test]
fn session_destroy_budget_is_aggregate_and_released_exactly_once() -> TestResult {
    let mut registry = ready();
    let mut windows = Vec::with_capacity(17);
    for index in 0..17u32 {
        windows.push(complete(&mut registry, owner(1), index + 1, index + 100)?);
    }
    fill_session_destroy_budget(&mut registry, &windows)?;

    let target = windows[16];
    put_target_in_close_pending(&mut registry, target)?;
    refuse_destroy_at_session_limit(&mut registry, target)?;
    release_first_window_and_check_fifo(&mut registry)?;
    admit_target_after_release(&mut registry, target)?;
    retire_old_owner_and_check_successor(&mut registry)
}
