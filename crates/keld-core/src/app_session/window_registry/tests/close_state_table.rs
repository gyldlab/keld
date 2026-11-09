//! Literal approved GH-531 §4.e row oracle and direct outcome observer.
use super::super::{
    AbandonedClose, BuildOutcome, CloseOutcome, FactOutcome, NativeWindow, Phase, SessionEffect,
    WindowError, WindowRegistry,
};
use super::{TestResult, begin, owner, phase_name, ready, snapshot};
use keld_ipc::CorrelationId;
use keld_ipc::window::{CloseVerdict, Rect, WindowEvent, WindowRef};
use keld_wv::WebviewId;

#[derive(Clone, Copy)]
enum Input {
    Built,
    Failed,
    Native,
    Request,
    Allow,
    Veto,
    Stale,
    Setter,
    Destroy,
    Released,
    Lost,
    End,
    Bounds,
    Focus,
    Maximized,
    FullScreen,
}

// Observable effects are classified from actual return values. Expectations below
// are transcribed from approved §4.e, never computed by a transition function.
use Input::{
    Allow, Bounds, Built, Destroy, End, Failed, Focus, FullScreen, Lost, Maximized, Native,
    Released, Request, Setter, Stale, Veto,
};

const ROWS: [(&str, char, Input, char, &str); 76] = [
    ("O1", 'O', Built, 'P', "created"),
    ("O2", 'O', Failed, 'D', "build-error"),
    ("O3", 'O', Native, 'O', "diagnostic"),
    ("O4", 'O', Request, 'O', "stale-error"),
    ("O5", 'O', Allow, 'O', "stale-reply"),
    ("O6", 'O', Stale, 'O', "stale-reply"),
    ("O7", 'O', Setter, 'O', "stale-error"),
    ("O8", 'O', Destroy, 'O', "stale-error"),
    ("O9", 'O', Released, 'O', "diagnostic"),
    ("O10", 'O', Lost, 'C', "retired"),
    ("O11", 'O', End, 'C', "interrupted"),
    ("O12", 'O', Bounds, 'O', "diagnostic"),
    ("O13", 'O', Focus, 'O', "diagnostic"),
    ("O14", 'O', Maximized, 'O', "diagnostic"),
    ("O15", 'O', FullScreen, 'O', "diagnostic"),
    ("P1", 'P', Built, 'P', "diagnostic"),
    ("P2", 'P', Failed, 'P', "diagnostic"),
    ("P3", 'P', Native, 'Q', "requested"),
    ("P4", 'P', Request, 'Q', "requested"),
    ("P5", 'P', Allow, 'P', "stale-reply"),
    ("P6", 'P', Stale, 'P', "stale-reply"),
    ("P7", 'P', Setter, 'P', "setter"),
    ("P8", 'P', Destroy, 'C', "teardown"),
    ("P9", 'P', Released, 'P', "diagnostic"),
    ("P10", 'P', Lost, 'P', "retired"),
    ("P11", 'P', End, 'C', "teardown"),
    ("P12", 'P', Bounds, 'P', "fact"),
    ("P13", 'P', Focus, 'P', "fact"),
    ("P14", 'P', Maximized, 'P', "fact"),
    ("P15", 'P', FullScreen, 'P', "fact"),
    ("Q1", 'Q', Built, 'Q', "diagnostic"),
    ("Q2", 'Q', Failed, 'Q', "diagnostic"),
    ("Q3", 'Q', Native, 'Q', "merged"),
    ("Q4", 'Q', Request, 'Q', "merged"),
    ("Q5", 'Q', Veto, 'P', "veto"),
    ("Q6", 'Q', Allow, 'C', "teardown"),
    ("Q7", 'Q', Stale, 'Q', "stale-reply"),
    ("Q8", 'Q', Setter, 'Q', "setter"),
    ("Q9", 'Q', Destroy, 'C', "abandoned+teardown"),
    ("Q10", 'Q', Released, 'Q', "diagnostic"),
    ("Q11", 'Q', Lost, 'P', "abandoned"),
    ("Q12", 'Q', End, 'C', "abandoned+teardown"),
    ("Q13", 'Q', Bounds, 'Q', "fact"),
    ("Q14", 'Q', Focus, 'Q', "fact"),
    ("Q15", 'Q', Maximized, 'Q', "fact"),
    ("Q16", 'Q', FullScreen, 'Q', "fact"),
    ("C1", 'C', Built, 'C', "late-cleanup"),
    ("C2", 'C', Failed, 'D', "quiet"),
    ("C3", 'C', Native, 'C', "already-closing"),
    ("C4", 'C', Request, 'C', "already-closing"),
    ("C5", 'C', Allow, 'C', "stale-reply"),
    ("C6", 'C', Stale, 'C', "stale-reply"),
    ("C7", 'C', Setter, 'C', "stale-error"),
    ("C8", 'C', Destroy, 'C', "already-closing"),
    ("C9", 'C', Released, 'D', "released"),
    ("C10", 'C', Lost, 'C', "retired"),
    ("C11", 'C', End, 'C', "quiet"),
    ("C12", 'C', Bounds, 'C', "dropped"),
    ("C13", 'C', Focus, 'C', "dropped"),
    ("C14", 'C', Maximized, 'C', "dropped"),
    ("C15", 'C', FullScreen, 'C', "dropped"),
    ("D1", 'D', Built, 'D', "diagnostic"),
    ("D2", 'D', Failed, 'D', "diagnostic"),
    ("D3", 'D', Native, 'D', "already-closing"),
    ("D4", 'D', Request, 'D', "stale-error"),
    ("D5", 'D', Allow, 'D', "stale-error"),
    ("D6", 'D', Stale, 'D', "stale-error"),
    ("D7", 'D', Setter, 'D', "stale-error"),
    ("D8", 'D', Destroy, 'D', "stale-error"),
    ("D9", 'D', Released, 'D', "diagnostic"),
    ("D10", 'D', Lost, 'D', "retired"),
    ("D11", 'D', End, 'D', "quiet"),
    ("D12", 'D', Bounds, 'D', "dropped"),
    ("D13", 'D', Focus, 'D', "dropped"),
    ("D14", 'D', Maximized, 'D', "dropped"),
    ("D15", 'D', FullScreen, 'D', "dropped"),
];

#[test]
fn every_approved_state_input_pair_has_its_exact_transition_and_effect() -> TestResult {
    assert_eq!(
        ROWS.len(),
        76,
        "all canonical rows, including impossible classifications"
    );
    for (row, initial, input, expected_phase, expected_effect) in ROWS {
        let mut registry = ready();
        let (build, window) = begin(&mut registry, owner(1), 1)?;
        let native = NativeWindow {
            view: WebviewId(7),
            state: snapshot(),
        };
        let entry = registry.entries.get_mut(&window.id.0).expect("seed entry");
        entry.created = initial != 'O';
        entry.last_close_seq = 7;
        entry.phase = match initial {
            'O' => entry.phase,
            'P' => Phase::Open(native),
            'Q' => Phase::ClosePending(native, 7),
            'C' if matches!(input, Built | Failed) => {
                entry.created = false;
                Phase::Closing(None)
            }
            'C' => Phase::Closing(Some(native)),
            'D' => Phase::Destroyed(Some(native.view)),
            _ => unreachable!("literal table state"),
        };
        let mut abandoned = Vec::new();
        let effect = observe_input(&mut registry, build, window, input, &mut abandoned)?;
        assert_eq!(
            phase_name(registry.entries[&window.id.0].phase),
            expected_phase,
            "{row}"
        );
        assert_eq!(effect, expected_effect, "{row}");
        if !abandoned.is_empty() {
            assert_eq!(
                abandoned,
                vec![AbandonedClose {
                    window,
                    close_seq: 7
                }],
                "{row}"
            );
        }
        if matches!(input, Destroy) && expected_phase == 'C' {
            assert!(
                registry.has_pending_call(owner(1), CorrelationId(2)),
                "{row}"
            );
        }
        if expected_effect == "dropped" {
            assert_eq!(
                if initial == 'D' {
                    registry.destroyed_fact_drops
                } else {
                    registry.entries[&window.id.0].dropped
                },
                1,
                "{row}"
            );
        }
    }
    Ok(())
}

fn observe_input(
    registry: &mut WindowRegistry,
    build: u64,
    window: WindowRef,
    input: Input,
    abandoned: &mut Vec<AbandonedClose>,
) -> TestResult<&'static str> {
    Ok(match input {
        Built | Failed => observe_build(registry, build, window, input),
        Native => observe_close(Ok(registry.native_close(WebviewId(7))))?,
        Request => observe_close(registry.request_close(owner(1), window))?,
        Allow | Veto | Stale => observe_close(registry.close_reply(
            owner(1),
            window,
            if matches!(input, Stale) { 6 } else { 7 },
            if matches!(input, Veto) {
                CloseVerdict::Veto
            } else {
                CloseVerdict::Allow
            },
        ))?,
        Setter => match registry.set_maximized(owner(1), window, true) {
            Ok(_) => "setter",
            Err(WindowError::StaleWindow) => "stale-error",
            other => {
                return Err(std::io::Error::other(format!("unexpected setter: {other:?}")).into());
            }
        },
        Destroy => {
            let out = registry.destroy(owner(1), window, CorrelationId(2), |close| {
                abandoned.push(close);
            });
            let tag = observe_close(out)?;
            if tag == "teardown" && !abandoned.is_empty() {
                "abandoned+teardown"
            } else {
                tag
            }
        }
        Released => observe_release(registry, window),
        Lost => {
            registry.role_lost(owner(1), |close| {
                abandoned.push(close);
            });
            if abandoned.is_empty() {
                "retired"
            } else {
                "abandoned"
            }
        }
        End => observe_end(registry, abandoned)?,
        Bounds | Focus | Maximized | FullScreen => observe_fact(registry, input)?,
    })
}

fn observe_build(
    registry: &mut WindowRegistry,
    build: u64,
    window: WindowRef,
    input: Input,
) -> &'static str {
    if matches!(input, Failed) {
        let out = registry.native_failed(build);
        return if out.diagnostic {
            "diagnostic"
        } else if out.call.is_some() {
            "build-error"
        } else {
            "quiet"
        };
    }
    match registry.native_built(build, WebviewId(7), snapshot()) {
        BuildOutcome::Created { fact, reply } => {
            assert_eq!(
                fact.event,
                WindowEvent::Created {
                    window,
                    state: snapshot(),
                    pending_close: None
                }
            );
            assert_eq!(
                reply.response,
                keld_ipc::window::WindowResponse::Created { window }
            );
            "created"
        }
        BuildOutcome::Cleanup(teardown) => {
            assert_eq!(teardown.view, WebviewId(7));
            "late-cleanup"
        }
        BuildOutcome::Ignored => "diagnostic",
    }
}

fn observe_release(registry: &mut WindowRegistry, window: WindowRef) -> &'static str {
    let out = registry.native_released(WebviewId(7));
    if out.diagnostic {
        "diagnostic"
    } else {
        assert_eq!(
            out.fact.map(|fact| fact.event),
            Some(WindowEvent::Destroyed { window })
        );
        assert_eq!(out.last_window, Some(owner(1)));
        "released"
    }
}

fn observe_end(
    registry: &mut WindowRegistry,
    abandoned: &mut Vec<AbandonedClose>,
) -> TestResult<&'static str> {
    let mut effects = Vec::new();
    registry.session_end(|effect| effects.push(effect));
    Ok(match effects.as_slice() {
        [] => "quiet",
        [SessionEffect::CreateInterrupted(_)] => "interrupted",
        [SessionEffect::Teardown(_)] => "teardown",
        [
            SessionEffect::CloseAbandoned(close),
            SessionEffect::Teardown(_),
        ] => {
            abandoned.push(*close);
            "abandoned+teardown"
        }
        other => {
            return Err(
                std::io::Error::other(format!("unexpected terminal effects: {other:?}")).into(),
            );
        }
    })
}

fn observe_fact(registry: &mut WindowRegistry, input: Input) -> TestResult<&'static str> {
    let out = match input {
        Bounds => registry.native_bounds(
            WebviewId(7),
            Rect {
                x: 38,
                ..snapshot().bounds
            },
        ),
        Focus => registry.native_focus(WebviewId(7), false),
        Maximized => registry.native_maximized(WebviewId(7), true),
        FullScreen => registry.native_full_screen(WebviewId(7), true),
        _ => return Err(std::io::Error::other("non-fact observer input").into()),
    };
    Ok(match out {
        FactOutcome::Event(_) => "fact",
        FactOutcome::Dropped => "dropped",
        FactOutcome::Unmapped => "diagnostic",
        other => {
            return Err(std::io::Error::other(format!("unexpected native fact: {other:?}")).into());
        }
    })
}

fn observe_close(out: Result<CloseOutcome, WindowError>) -> TestResult<&'static str> {
    Ok(match out {
        Ok(CloseOutcome::Requested {
            close_seq,
            fact: Some(fact),
        }) => {
            assert_eq!(close_seq, 8);
            assert!(matches!(
                fact.event,
                WindowEvent::CloseRequested { close_seq: 8, .. }
            ));
            "requested"
        }
        Ok(CloseOutcome::Merged { close_seq }) => {
            assert_eq!(close_seq, 7);
            "merged"
        }
        Ok(CloseOutcome::Teardown(teardown)) => {
            assert_eq!(teardown.view, WebviewId(7));
            "teardown"
        }
        Ok(CloseOutcome::Vetoed) => "veto",
        Ok(CloseOutcome::Stale) => "stale-reply",
        Ok(CloseOutcome::AlreadyClosing) => "already-closing",
        Ok(CloseOutcome::Unmapped) => "diagnostic",
        Err(WindowError::StaleWindow) => "stale-error",
        other => {
            return Err(
                std::io::Error::other(format!("unexpected table outcome: {other:?}")).into(),
            );
        }
    })
}
