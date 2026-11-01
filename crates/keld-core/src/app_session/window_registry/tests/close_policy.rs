//! GH-531 close, loss, correlation and constructor-adoption invariants.
use super::super::{
    AbandonedClose, CloseOutcome, CreateOutcome, FactOutcome, PendingWindowCall, Phase,
    SessionEffect, Teardown, WindowCounter, WindowError,
};
use super::{TestResult, begin, complete, owner, phase_name, ready, snapshot};
use keld_ipc::CorrelationId;
use keld_ipc::window::{CloseVerdict, WindowEvent};
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
