//! Pure existing-channel schedules; unit wake is not native completion.
use super::*;
use std::sync::mpsc;

#[test]
fn terminal_keeps_same_worker_delivering_existing_completion_wakes() {
    let (commands, command_rx) = mpsc::channel();
    let (outcomes, outcome_rx) = mpsc::channel();
    commands.send(AppWindowCommand::Quit).unwrap();
    commands.send(AppWindowCommand::HostWriterProgress).unwrap();
    outcomes
        .send(RendererBridgeOutcome::ControlCapacity)
        .unwrap();
    let input = Arc::new(Mutex::new(BridgeInput::new(command_rx, Some(outcome_rx))));
    let stop = AtomicBool::new(false);
    let terminal = AtomicBool::new(false);
    let mut seen = Vec::new();
    run(
        &input,
        &stop,
        &terminal,
        &mut || {
            let event = input
                .try_lock()
                .expect("no mutex across unit post")
                .take_current()
                .unwrap();
            match event {
                AppWakeDelivery::Command(Some(AppWindowCommand::Quit)) => {
                    assert!(terminal.load(Ordering::Acquire));
                    seen.push("quit");
                }
                AppWakeDelivery::Renderer(Some(RendererBridgeOutcome::ControlCapacity)) => {
                    seen.push("capacity");
                }
                AppWakeDelivery::Command(Some(AppWindowCommand::HostWriterProgress)) => {
                    seen.push("writer");
                    stop.store(true, Ordering::Release);
                }
                _ => panic!("unexpected source event"),
            }
            Ok(())
        },
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(seen, ["quit", "capacity", "writer"]);
    assert!(!input.lock().unwrap().has_current());
}

#[test]
fn occupied_current_keeps_next_event_in_original_receiver() {
    let (commands, command_rx) = mpsc::channel();
    let (outcomes, outcome_rx) = mpsc::channel();
    commands.send(AppWindowCommand::Quit).unwrap();
    outcomes
        .send(RendererBridgeOutcome::ControlCapacity)
        .unwrap();
    let input = Arc::new(Mutex::new(BridgeInput::new(command_rx, Some(outcome_rx))));
    let stop = AtomicBool::new(false);
    run(
        &input,
        &stop,
        &AtomicBool::new(false),
        &mut || {
            // Post success leaves actual payload in its same owner until UI takes it.
            stop.store(true, Ordering::Release);
            Ok(())
        },
        &mut |_| {},
    )
    .unwrap();
    let input = input.lock().unwrap();
    assert!(matches!(
        input.current,
        Some(AppWakeDelivery::Command(Some(AppWindowCommand::Quit)))
    ));
    assert!(matches!(
        input.outcomes.as_ref().unwrap().try_recv(),
        Ok(RendererBridgeOutcome::ControlCapacity)
    ));
}

#[test]
fn stop_keeps_original_receivers_and_their_unread_event() {
    let (_commands, command_rx) = mpsc::channel();
    let (outcomes, outcome_rx) = mpsc::channel();
    outcomes
        .send(RendererBridgeOutcome::ControlCapacity)
        .unwrap();
    let input = Arc::new(Mutex::new(BridgeInput::new(command_rx, Some(outcome_rx))));
    run(
        &input,
        &AtomicBool::new(true),
        &AtomicBool::new(false),
        &mut || panic!("stopped source may not post"),
        &mut |_| {},
    )
    .unwrap();
    assert!(matches!(
        input.lock().unwrap().outcomes.as_ref().unwrap().try_recv(),
        Ok(RendererBridgeOutcome::ControlCapacity)
    ));
}

#[test]
fn actual_input_keeps_the_same_reply_allocation_on_unit_error_and_unwind() {
    for unwind in [false, true] {
        let (_commands, command_rx) = mpsc::channel();
        let (outcomes, outcome_rx) = mpsc::channel();
        let payload = vec![17_u8; 4096];
        let address = payload.as_ptr();
        outcomes
            .send(RendererBridgeOutcome::Reply {
                webview: crate::WebviewId(7),
                navigation: 3,
                request: 5,
                payload,
                numeric_result: None,
            })
            .unwrap();
        let input = Arc::new(Mutex::new(BridgeInput::new(command_rx, Some(outcome_rx))));
        let result = run(
            &input,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            &mut || {
                assert!(!unwind, "unit post unwound before any delivery");
                Err(())
            },
            &mut |_| panic!("failed post must not be reported successful"),
        );
        assert!(matches!(
            result,
            Err(Failure::PostRejected | Failure::PostUnwound)
        ));
        let guard = input.lock().unwrap();
        let Some(AppWakeDelivery::Renderer(Some(RendererBridgeOutcome::Reply { payload, .. }))) =
            &guard.current
        else {
            panic!("same original payload must stay attached after failed unit post");
        };
        assert_eq!(payload.as_ptr(), address);
        assert_eq!(payload.len(), 4096);
    }
}

#[test]
fn final_queue_check_preserves_actual_reply_after_pure_hints() {
    let (commands, command_rx) = mpsc::channel();
    let (outcomes, outcome_rx) = mpsc::channel();
    commands.send(AppWindowCommand::DialogWake).unwrap();
    commands.send(AppWindowCommand::HostWriterProgress).unwrap();
    outcomes
        .send(RendererBridgeOutcome::ControlCapacity)
        .unwrap();
    let payload = vec![29_u8; 4096];
    let address = payload.as_ptr();
    outcomes
        .send(RendererBridgeOutcome::Reply {
            webview: crate::WebviewId(8),
            navigation: 9,
            request: 10,
            payload,
            numeric_result: None,
        })
        .unwrap();
    let mut input = BridgeInput::new(command_rx, Some(outcome_rx));
    assert!(input.check_drained().is_err());
    let Some(AppWakeDelivery::Renderer(Some(RendererBridgeOutcome::Reply { payload, .. }))) =
        input.current.as_ref()
    else {
        panic!("actual unread reply must be attached, never disposed as a wake");
    };
    assert_eq!(payload.as_ptr(), address);
    assert_eq!(payload.len(), 4096);
}

#[test]
fn final_queue_check_does_not_discard_a_real_command() {
    let (commands, command_rx) = mpsc::channel();
    commands
        .send(AppWindowCommand::SetMaximized {
            view: crate::WebviewId(11),
            maximized: true,
        })
        .unwrap();
    let mut input = BridgeInput::new(command_rx, None);
    assert!(input.check_drained().is_err());
    assert!(matches!(
        input.current,
        Some(AppWakeDelivery::Command(Some(
            AppWindowCommand::SetMaximized {
                view: crate::WebviewId(11),
                maximized: true,
            }
        )))
    ));
}

#[test]
fn pure_hint_disposal_is_only_an_additional_queue_check() {
    let (commands, command_rx) = mpsc::channel();
    let (outcomes, outcome_rx) = mpsc::channel();
    commands.send(AppWindowCommand::DialogWake).unwrap();
    commands.send(AppWindowCommand::HostWriterProgress).unwrap();
    outcomes
        .send(RendererBridgeOutcome::ControlCapacity)
        .unwrap();
    let mut input = BridgeInput::new(command_rx, Some(outcome_rx));
    // This helper deliberately supplies no producer/protocol/native proof.
    // Production calls it only after those distinct actual owner gates.
    input.check_drained().unwrap();
    assert!(!input.has_current());
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[test]
fn outcome_hold_keeps_real_allocation_unread_while_commands_continue() {
    let (commands, command_rx) = mpsc::channel();
    let (outcomes, outcome_rx) = mpsc::channel();
    commands.send(AppWindowCommand::Quit).unwrap();
    let payload = vec![41_u8; 4096];
    let address = payload.as_ptr();
    outcomes
        .send(RendererBridgeOutcome::Reply {
            webview: crate::WebviewId(17),
            navigation: 4,
            request: 9,
            payload,
            numeric_result: None,
        })
        .unwrap();
    let mut input = BridgeInput::new(command_rx, Some(outcome_rx));
    input.test_outcomes_held = true;
    let terminal = AtomicBool::new(false);
    assert!(matches!(input.next(&terminal), Ok(Step::Post(_))));
    assert!(matches!(
        input.take_current(),
        Some(AppWakeDelivery::Command(Some(AppWindowCommand::Quit)))
    ));
    assert!(terminal.load(Ordering::Acquire));
    assert!(matches!(input.next(&terminal), Ok(Step::Wait)));
    assert!(input.current.is_none());
    input.test_outcomes_held = false;
    assert!(matches!(input.next(&terminal), Ok(Step::Post(_))));
    let Some(AppWakeDelivery::Renderer(Some(RendererBridgeOutcome::Reply { payload, .. }))) =
        input.current.as_ref()
    else {
        panic!("same real reply must remain");
    };
    assert_eq!(payload.as_ptr(), address);
    assert_eq!(payload.len(), 4096);
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[test]
fn final_queue_check_cannot_mistake_active_outcome_hold_for_empty() {
    let (_commands, command_rx) = mpsc::channel();
    let (outcomes, outcome_rx) = mpsc::channel();
    let payload = vec![43_u8; 4096];
    let address = payload.as_ptr();
    outcomes
        .send(RendererBridgeOutcome::Reply {
            webview: crate::WebviewId(18),
            navigation: 5,
            request: 10,
            payload,
            numeric_result: None,
        })
        .unwrap();
    let mut input = BridgeInput::new(command_rx, Some(outcome_rx));
    input.test_outcomes_held = true;
    assert!(input.check_drained().is_err());
    let Some(AppWakeDelivery::Renderer(Some(RendererBridgeOutcome::Reply { payload, .. }))) =
        input.current.as_ref()
    else {
        panic!("queued payload must remain retained");
    };
    assert_eq!(payload.as_ptr(), address);
    assert_eq!(payload.len(), 4096);
}
