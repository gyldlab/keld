use super::super::PrimaryLinkIdentity;
use super::{
    BuildOutcome, CloseOutcome, CreateOutcome, FactOutcome, NativeSetter, OwnedWindowEvent,
    PendingWindowCall, Phase, SessionEffect, Teardown, WindowCounter, WindowError, WindowOwner,
    WindowRegistry,
};
use keld_ipc::CorrelationId;
use keld_ipc::window::{
    Rect, WindowEvent, WindowGeneration, WindowId, WindowRef, WindowResponse, WindowState,
};
use keld_wv::WebviewId;

mod close_policy;
mod close_state_table;

fn phase_name(phase: Phase) -> char {
    match phase {
        Phase::Opening(_) => 'O',
        Phase::Open(_) => 'P',
        Phase::ClosePending(_, _) => 'Q',
        Phase::Closing(_) => 'C',
        Phase::Destroyed(_) => 'D',
    }
}

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn owner(attempt: u32) -> WindowOwner {
    WindowOwner::new(PrimaryLinkIdentity::Fixture, attempt)
}
fn ready() -> WindowRegistry {
    let mut registry = WindowRegistry::new();
    registry.mark_ready();
    registry
}
fn begin(
    registry: &mut WindowRegistry,
    who: WindowOwner,
    correlation: u32,
) -> TestResult<(u64, WindowRef)> {
    match registry.create(who, CorrelationId(correlation), 640, 480, None, None)? {
        CreateOutcome::Build { build, window, .. } => Ok((build, window)),
        CreateOutcome::Adopted(_) => {
            Err(std::io::Error::other("fixture expected a fresh build").into())
        }
    }
}
fn snapshot() -> WindowState {
    WindowState {
        bounds: Rect {
            x: 37,
            y: 91,
            width: 800,
            height: 600,
        },
        focused: true,
        maximized: false,
        full_screen: false,
    }
}
fn complete(
    registry: &mut WindowRegistry,
    who: WindowOwner,
    correlation: u32,
    view: u32,
) -> TestResult<WindowRef> {
    let (build, window) = begin(registry, who, correlation)?;
    assert!(matches!(
        registry.native_built(build, WebviewId(view), snapshot()),
        BuildOutcome::Created { .. }
    ));
    Ok(window)
}

fn allow_native_close(registry: &mut WindowRegistry, view: WebviewId) -> TestResult<CloseOutcome> {
    let CloseOutcome::Requested {
        fact: Some(fact),
        close_seq,
    } = registry.native_close(view)
    else {
        return Err(std::io::Error::other("fixture expected a fresh native close request").into());
    };
    let WindowEvent::CloseRequested { window, .. } = fact.event else {
        return Err(std::io::Error::other("native close must produce its exact fact").into());
    };
    Ok(registry.close_reply(
        fact.owner,
        window,
        close_seq,
        keld_ipc::window::CloseVerdict::Allow,
    )?)
}

#[test]
fn native_close_holds_one_pending_sequence_instead_of_tearing_down() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    let first = registry.native_close(WebviewId(7));
    assert!(
        !matches!(first, CloseOutcome::Teardown(_)),
        "a native close must await the application verdict: {first:?}"
    );
    let pending = WindowEvent::Created {
        window,
        state: snapshot(),
        pending_close: Some(1),
    };
    assert_eq!(registry.replay(owner(1)).collect::<Vec<_>>(), vec![pending]);
    registry.native_close(WebviewId(7));
    assert_eq!(registry.replay(owner(1)).collect::<Vec<_>>(), vec![pending]);
    Ok(())
}

#[test]
fn registry_waits_for_ready_without_reserving_a_pair() -> TestResult {
    let mut registry = WindowRegistry::new();
    assert_eq!(
        registry.create(owner(1), CorrelationId(1), 1, 1, None, None),
        Err(WindowError::NotReady)
    );
    registry.mark_ready();
    assert_eq!(begin(&mut registry, owner(1), 2)?.1.id, WindowId(1));
    Ok(())
}

#[test]
fn native_snapshot_is_the_only_created_state_and_reply_follows_its_fact() -> TestResult {
    let mut registry = ready();
    let who = owner(1);
    let (build, window) = begin(&mut registry, who, 73)?;
    match registry.native_built(build, WebviewId(7), snapshot()) {
        BuildOutcome::Created { fact, reply } => {
            assert!(fact.owner == who && reply.owner == who);
            assert_eq!(
                fact.event,
                WindowEvent::Created {
                    window,
                    state: snapshot(),
                    pending_close: None
                }
            );
            assert_eq!(reply.correlation, CorrelationId(73));
            assert_eq!(reply.response, WindowResponse::Created { window });
        }
        other => panic!("unexpected native result: {other:?}"),
    }
    Ok(())
}

#[test]
fn native_view_numbers_do_not_mint_logical_window_ids() -> TestResult {
    let mut registry = ready();
    let first = complete(&mut registry, owner(1), 1, 7)?;
    let second = complete(&mut registry, owner(1), 2, 8)?;
    assert_eq!(first.id, WindowId(1));
    assert_eq!(second.id, WindowId(2));
    assert_ne!(first.incarnation, second.incarnation);
    Ok(())
}

#[test]
fn forged_reference_half_and_attempt_fence_have_no_state_effect() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    let forged = WindowRef {
        incarnation: WindowGeneration(window.incarnation.0 + 1),
        ..window
    };
    assert_eq!(
        registry.set_maximized(owner(1), forged, true),
        Err(WindowError::StaleWindow)
    );
    assert_eq!(
        registry.set_maximized(owner(2), window, true),
        Err(WindowError::WrongOwner)
    );
    assert_eq!(
        registry.replay(owner(1)).collect::<Vec<_>>(),
        vec![WindowEvent::Created {
            window,
            state: snapshot(),
            pending_close: None
        }]
    );
    assert!(matches!(
        registry.set_maximized(owner(1), window, true),
        Ok(NativeSetter::Maximized {
            view: WebviewId(7),
            maximized: true
        })
    ));
    // A setter accepts the request; only a later native fact changes the mirror.
    assert_eq!(
        registry.replay(owner(1)).next(),
        Some(WindowEvent::Created {
            window,
            state: snapshot(),
            pending_close: None
        })
    );
    Ok(())
}

#[test]
fn invalid_pair_precedes_wrong_owner_and_native_setter_is_not_optimistic() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    let forged = WindowRef {
        id: WindowId(999),
        ..window
    };
    assert_eq!(
        registry.set_full_screen(owner(2), forged, true),
        Err(WindowError::StaleWindow)
    );
    assert_eq!(
        registry.set_full_screen(owner(1), window, true),
        Ok(NativeSetter::FullScreen {
            view: WebviewId(7),
            full_screen: true
        })
    );
    assert!(!matches!(
        registry.replay(owner(1)).next(),
        Some(WindowEvent::Created {
            state: WindowState {
                full_screen: true,
                ..
            },
            ..
        })
    ));
    Ok(())
}

#[test]
fn failed_build_retires_both_identity_halves_and_retains_correlation() -> TestResult {
    let mut registry = ready();
    let (build, first) = begin(&mut registry, owner(1), 19)?;
    let failure = registry.native_failed(build);
    let pending = failure.call.expect("failed active create is answered once");
    assert_eq!(pending.owner, owner(1));
    assert_eq!(pending.correlation, CorrelationId(19));
    assert!(failure.last_window.is_none());
    let (_, second) = begin(&mut registry, owner(1), 20)?;
    assert_eq!(second.id, WindowId(2));
    assert_eq!(second.incarnation, WindowGeneration(2));
    assert_eq!(
        registry.set_maximized(owner(1), first, true),
        Err(WindowError::StaleWindow)
    );
    assert!(registry.native_failed(build).call.is_none());
    Ok(())
}

#[test]
fn identity_exhaustion_is_checked_before_either_counter_changes() {
    for (last_id, last_incarnation, expected) in [
        (u32::MAX, 9, WindowError::Exhausted(WindowCounter::Id)),
        (
            9,
            u64::MAX,
            WindowError::Exhausted(WindowCounter::Incarnation),
        ),
    ] {
        let mut registry = ready();
        registry.last_id = last_id;
        registry.last_incarnation = last_incarnation;
        assert_eq!(
            registry.create(owner(1), CorrelationId(1), 1, 1, None, None),
            Err(expected)
        );
        assert_eq!(registry.last_id, last_id);
        assert_eq!(registry.last_incarnation, last_incarnation);
        assert!(registry.entries.is_empty());
    }
}

#[test]
fn cumulative_window_churn_refuses_max_plus_one_without_minting_or_native_effect() -> TestResult {
    // Candidate GH1149 numeric policy, deliberately not yet accepted. A
    // destroyed window must still consume a session identity slot.
    const PROPOSED_SESSION_CAP: u32 = 1024;
    let mut registry = ready();
    for correlation in 1..=PROPOSED_SESSION_CAP {
        let (build, window) = begin(&mut registry, owner(1), correlation)?;
        assert_eq!(window.id.0, correlation);
        let failure = registry.native_failed(build);
        assert_eq!(
            failure
                .call
                .expect("admitted Create must have a terminal reply")
                .correlation,
            CorrelationId(correlation)
        );
    }
    assert_eq!(registry.entries.len(), PROPOSED_SESSION_CAP as usize);
    let before = (registry.last_id, registry.last_incarnation);
    assert_eq!(
        registry.create(owner(1), CorrelationId(2000), 320, 240, None, None),
        Err(WindowError::SessionWindowCapacity)
    );
    assert_eq!((registry.last_id, registry.last_incarnation), before);
    assert_eq!(registry.entries.len(), PROPOSED_SESSION_CAP as usize);
    // Both authenticated limits can be reached concurrently: original
    // counter-exhaustion diagnostics retain their precedence, and no pair
    // changes on a refused call.
    registry.last_id = u32::MAX;
    assert_eq!(
        registry.create(owner(1), CorrelationId(2002), 320, 240, None, None),
        Err(WindowError::Exhausted(WindowCounter::Id))
    );
    registry.last_id = before.0;
    registry.last_incarnation = u64::MAX;
    assert_eq!(
        registry.create(owner(1), CorrelationId(2003), 320, 240, None, None),
        Err(WindowError::Exhausted(WindowCounter::Incarnation))
    );
    registry.last_incarnation = before.1;
    // Retiring a Bun role does not reset the Host session's lifetime history.
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    assert_eq!(
        registry.create(owner(2), CorrelationId(2001), 320, 240, None, None),
        Err(WindowError::SessionWindowCapacity)
    );
    let mut fresh = WindowRegistry::new();
    assert!(fresh.mark_ready());
    assert!(matches!(
        fresh.create(owner(3), CorrelationId(1), 320, 240, None, None),
        Ok(CreateOutcome::Build { .. })
    ));
    Ok(())
}

#[test]
fn adopted_survivor_at_cumulative_capacity_does_not_mint_another_identity() -> TestResult {
    const PROPOSED_SESSION_CAP: u32 = 1024;
    let mut registry = ready();
    let survivor = complete(&mut registry, owner(1), 1, 7)?;
    for correlation in 2..=PROPOSED_SESSION_CAP {
        let (build, _) = begin(&mut registry, owner(1), correlation)?;
        assert!(registry.native_failed(build).call.is_some());
    }
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    let before = (registry.last_id, registry.last_incarnation);
    assert_eq!(
        registry.create(owner(2), CorrelationId(2000), 1, 1, None, None),
        Ok(CreateOutcome::Adopted(survivor)),
    );
    assert_eq!((registry.last_id, registry.last_incarnation), before);
    assert_eq!(
        registry.create(owner(2), CorrelationId(2001), 1, 1, None, None),
        Err(WindowError::SessionWindowCapacity)
    );
    Ok(())
}

#[test]
fn maximum_pair_is_usable_once_and_never_wraps() -> TestResult {
    let mut registry = ready();
    registry.last_id = u32::MAX - 1;
    registry.last_incarnation = u64::MAX - 1;
    let (_, maximum) = begin(&mut registry, owner(1), 1)?;
    assert_eq!(maximum.id, WindowId(u32::MAX));
    assert_eq!(maximum.incarnation, WindowGeneration(u64::MAX));
    assert_eq!(
        registry.create(owner(1), CorrelationId(2), 1, 1, None, None),
        Err(WindowError::Exhausted(WindowCounter::Id))
    );
    Ok(())
}

#[test]
fn opening_setters_fail_without_canceling_the_pending_create() -> TestResult {
    let mut registry = ready();
    let (build, window) = begin(&mut registry, owner(1), 1)?;
    assert_eq!(
        registry.set_maximized(owner(1), window, true),
        Err(WindowError::StaleWindow)
    );
    assert!(matches!(
        registry.native_built(build, WebviewId(7), snapshot()),
        BuildOutcome::Created { .. }
    ));
    Ok(())
}

#[test]
fn role_loss_keeps_open_native_window_but_fences_retired_caller() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    registry.role_lost(owner(1), |_| {});
    assert_eq!(
        registry.set_maximized(owner(1), window, true),
        Err(WindowError::WrongOwner)
    );
    assert_eq!(
        registry.native_focus(WebviewId(7), false),
        FactOutcome::Suppressed
    );
    registry.subscribe_successor(owner(2));
    let mut state = snapshot();
    state.focused = false;
    assert_eq!(
        registry.replay(owner(2)).collect::<Vec<_>>(),
        vec![WindowEvent::Created {
            window,
            state,
            pending_close: None
        }]
    );
    Ok(())
}

#[test]
fn adoption_uses_lowest_host_id_and_repeat_subscription_never_rearms_it() -> TestResult {
    let mut registry = ready();
    registry.last_id = 40;
    let first = complete(&mut registry, owner(1), 1, 7)?;
    let claimed = complete(&mut registry, owner(1), 2, 8)?;
    let third = complete(&mut registry, owner(1), 3, 9)?;
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    registry
        .set_maximized(owner(2), claimed, true)
        .expect("claim second survivor");
    assert_eq!(
        registry.create(owner(2), CorrelationId(4), 1, 1, None, None),
        Ok(CreateOutcome::Adopted(first))
    );
    registry.subscribe_successor(owner(2));
    assert_eq!(
        registry.create(owner(2), CorrelationId(5), 1, 1, None, None),
        Ok(CreateOutcome::Adopted(third))
    );
    let (_, next) = begin(&mut registry, owner(2), 6)?;
    assert_eq!(next.id, WindowId(44));
    assert_eq!(
        registry.replay(owner(2)).count(),
        3,
        "opening new window does not alter surviving views"
    );
    Ok(())
}

#[test]
fn two_intentional_creates_after_adoption_build_distinct_windows() -> TestResult {
    let mut registry = ready();
    let first = complete(&mut registry, owner(1), 1, 7)?;
    let second = complete(&mut registry, owner(1), 2, 8)?;
    assert_ne!(first, second);
    assert_eq!(registry.replay(owner(1)).count(), 2);
    Ok(())
}

#[test]
fn lost_opening_waits_for_late_build_then_tears_down_without_publishing() -> TestResult {
    let mut registry = ready();
    let (build, window) = begin(&mut registry, owner(1), 1)?;
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    assert_eq!(registry.replay(owner(2)).count(), 0);
    assert_eq!(
        registry.native_built(build, WebviewId(7), snapshot()),
        BuildOutcome::Cleanup(Teardown {
            window,
            view: WebviewId(7)
        })
    );
    let release = registry.native_released(WebviewId(7));
    assert!(release.fact.is_none() && release.last_window.is_none());
    Ok(())
}

#[test]
fn lost_opening_build_failure_has_no_reply_or_last_window_event() -> TestResult {
    let mut registry = ready();
    let (build, _) = begin(&mut registry, owner(1), 1)?;
    registry.role_lost(owner(1), |_| {});
    let failure = registry.native_failed(build);
    assert!(failure.call.is_none() && failure.last_window.is_none());
    Ok(())
}

#[test]
fn released_last_window_defers_while_an_admitted_create_is_opening() -> TestResult {
    let mut registry = ready();
    let first = complete(&mut registry, owner(1), 1, 7)?;
    let (build, _) = begin(&mut registry, owner(1), 2)?;
    assert_eq!(
        allow_native_close(&mut registry, WebviewId(7))?,
        CloseOutcome::Teardown(Teardown {
            window: first,
            view: WebviewId(7)
        })
    );
    let release = registry.native_released(WebviewId(7));
    assert!(matches!(
        release.fact,
        Some(OwnedWindowEvent {
            event: WindowEvent::Destroyed { .. },
            ..
        })
    ));
    assert!(release.last_window.is_none());
    let failure = registry.native_failed(build);
    assert!(failure.call.is_some());
    assert_eq!(failure.last_window, Some(owner(1)));
    Ok(())
}

#[test]
fn successful_build_clears_the_deferred_last_window_check() -> TestResult {
    let mut registry = ready();
    complete(&mut registry, owner(1), 1, 7)?;
    let (build, _) = begin(&mut registry, owner(1), 2)?;
    allow_native_close(&mut registry, WebviewId(7))?;
    registry.native_released(WebviewId(7));
    registry.native_built(build, WebviewId(8), snapshot());
    assert!(!registry.deferred_last_window);
    allow_native_close(&mut registry, WebviewId(8))?;
    assert_eq!(
        registry.native_released(WebviewId(8)).last_window,
        Some(owner(1))
    );
    Ok(())
}

#[test]
fn role_loss_and_session_end_clear_deferred_last_window_checks() -> TestResult {
    for end_session in [false, true] {
        let mut registry = ready();
        complete(&mut registry, owner(1), 1, 7)?;
        let (build, _) = begin(&mut registry, owner(1), 2)?;
        allow_native_close(&mut registry, WebviewId(7))?;
        registry.native_released(WebviewId(7));
        if end_session {
            registry.session_end(|_| {});
        } else {
            registry.role_lost(owner(1), |_| {});
        }
        assert!(!registry.deferred_last_window);
        assert!(registry.native_failed(build).last_window.is_none());
    }
    Ok(())
}

#[test]
fn session_end_tears_down_every_native_window_once_and_suppresses_last_window() -> TestResult {
    let mut registry = ready();
    let first = complete(&mut registry, owner(1), 1, 7)?;
    let second = complete(&mut registry, owner(1), 2, 8)?;
    let (build, late) = begin(&mut registry, owner(1), 3)?;
    let mut effects = Vec::new();
    registry.session_end(|teardown| effects.push(teardown));
    assert_eq!(
        effects,
        vec![
            SessionEffect::Teardown(Teardown {
                window: first,
                view: WebviewId(7)
            }),
            SessionEffect::Teardown(Teardown {
                window: second,
                view: WebviewId(8)
            }),
            SessionEffect::CreateInterrupted(PendingWindowCall {
                owner: owner(1),
                correlation: CorrelationId(3)
            }),
        ]
    );
    registry.session_end(|teardown| effects.push(teardown));
    assert_eq!(effects.len(), 3);
    assert!(registry.native_released(WebviewId(7)).last_window.is_none());
    assert!(registry.native_released(WebviewId(8)).last_window.is_none());
    assert_eq!(
        registry.native_built(build, WebviewId(9), snapshot()),
        BuildOutcome::Cleanup(Teardown {
            window: late,
            view: WebviewId(9)
        })
    );
    assert!(registry.native_released(WebviewId(9)).fact.is_none());
    assert_eq!(
        registry.create(owner(1), CorrelationId(9), 1, 1, None, None),
        Err(WindowError::NotReady)
    );
    Ok(())
}

#[test]
fn state_facts_deduplicate_but_recovery_snapshot_still_tracks_native_truth() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    assert_eq!(
        registry.native_bounds(WebviewId(7), snapshot().bounds),
        FactOutcome::Coalesced
    );
    assert_eq!(
        registry.native_maximized(WebviewId(7), true),
        FactOutcome::Event(OwnedWindowEvent {
            owner: owner(1),
            event: WindowEvent::Maximized {
                window,
                maximized: true
            }
        })
    );
    assert_eq!(
        registry.native_full_screen(WebviewId(7), true),
        FactOutcome::Event(OwnedWindowEvent {
            owner: owner(1),
            event: WindowEvent::FullScreen {
                window,
                full_screen: true
            }
        })
    );
    registry.role_lost(owner(1), |_| {});
    assert_eq!(
        registry.native_bounds(
            WebviewId(7),
            Rect {
                x: 99,
                ..snapshot().bounds
            }
        ),
        FactOutcome::Suppressed
    );
    registry.subscribe_successor(owner(2));
    let state = WindowState {
        bounds: Rect {
            x: 99,
            ..snapshot().bounds
        },
        maximized: true,
        full_screen: true,
        ..snapshot()
    };
    assert_eq!(
        registry.replay(owner(2)).next(),
        Some(WindowEvent::Created {
            window,
            state,
            pending_close: None
        })
    );
    Ok(())
}

#[test]
fn unmapped_and_teardown_facts_never_reach_the_live_writer() -> TestResult {
    let mut registry = ready();
    assert_eq!(
        registry.native_focus(WebviewId(77), true),
        FactOutcome::Unmapped
    );
    complete(&mut registry, owner(1), 1, 7)?;
    allow_native_close(&mut registry, WebviewId(7))?;
    assert_eq!(
        registry.native_focus(WebviewId(7), false),
        FactOutcome::Dropped
    );
    assert_eq!(registry.native_released(WebviewId(7)).dropped_facts, 1);
    assert_eq!(
        registry.native_focus(WebviewId(7), true),
        FactOutcome::Dropped
    );
    Ok(())
}

#[test]
fn retired_pair_is_stale_even_for_a_different_attempt() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    allow_native_close(&mut registry, WebviewId(7))?;
    registry.native_released(WebviewId(7));
    assert_eq!(
        registry.set_maximized(owner(2), window, true),
        Err(WindowError::StaleWindow)
    );
    Ok(())
}

#[test]
fn stale_role_loss_does_not_clear_a_successor_deferred_last_window_check() -> TestResult {
    let mut registry = ready();
    complete(&mut registry, owner(1), 1, 7)?;
    registry.role_lost(owner(1), |_| {});
    registry.subscribe_successor(owner(2));
    // Claim the survivor so the next Create genuinely starts a native build.
    let WindowEvent::Created { window, .. } =
        registry.replay(owner(2)).next().expect("replayed survivor")
    else {
        panic!("replay only creates")
    };
    registry
        .set_maximized(owner(2), window, true)
        .expect("claim survivor");
    let (build, _) = begin(&mut registry, owner(2), 2)?;
    allow_native_close(&mut registry, WebviewId(7))?;
    registry.native_released(WebviewId(7));
    assert!(registry.deferred_last_window);
    registry.role_lost(owner(1), |_| {});
    assert_eq!(registry.native_failed(build).last_window, Some(owner(2)));
    Ok(())
}

#[test]
fn retired_native_fact_counter_is_saturating_and_reported_once_after_drain() -> TestResult {
    let mut registry = ready();
    complete(&mut registry, owner(1), 1, 7)?;
    allow_native_close(&mut registry, WebviewId(7))?;
    registry.native_released(WebviewId(7));
    registry.destroyed_fact_drops = u64::MAX;
    assert_eq!(
        registry.native_focus(WebviewId(7), false),
        FactOutcome::Dropped
    );
    assert_eq!(registry.take_destroyed_fact_drops(), u64::MAX);
    assert_eq!(registry.take_destroyed_fact_drops(), 0);
    Ok(())
}

#[test]
fn premature_release_and_unknown_build_never_change_an_open_window() -> TestResult {
    let mut registry = ready();
    let window = complete(&mut registry, owner(1), 1, 7)?;
    assert!(registry.native_released(WebviewId(7)).diagnostic);
    assert!(registry.native_failed(999).diagnostic);
    assert_eq!(
        registry.native_built(999, WebviewId(8), snapshot()),
        BuildOutcome::Ignored
    );
    assert_eq!(
        registry.replay(owner(1)).next(),
        Some(WindowEvent::Created {
            window,
            state: snapshot(),
            pending_close: None
        })
    );
    Ok(())
}

#[test]
fn late_initialization_cannot_reopen_a_terminated_registry() {
    let mut registry = WindowRegistry::new();
    registry.session_end(|_| {});
    assert!(!registry.mark_ready());
    assert_eq!(
        registry.create(owner(1), CorrelationId(1), 1, 1, None, None),
        Err(WindowError::NotReady)
    );
}
