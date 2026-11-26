//! Pure native-stamp contracts; these do not initialize `AppKit` on test workers.

use super::{
    ActivationState, MenuAction, MenuActivation, MenuFocusSnapshot, MenuRole, PendingActivation,
    captured_window, current_pending, modifiers,
};
use objc2_app_kit::NSEventModifierFlags;
use std::rc::Rc;

fn stamp() -> MenuActivation {
    MenuActivation {
        generation: 17,
        command: 3,
        sequence: 23,
    }
}

fn pending() -> ActivationState {
    ActivationState {
        generation: 17,
        sequence: 23,
        active: true,
        pending: Some(PendingActivation {
            stamp: stamp(),
            action: MenuAction::NativeRole(MenuRole::Close),
            focus_at_click: MenuFocusSnapshot {
                app_active: false,
                key_window_present: false,
                main_window_present: false,
            },
            window: None,
            responder: None,
        }),
        wake: Rc::new(|| {}),
    }
}

#[test]
fn native_stamp_requires_current_generation_and_exact_one_shot_invocation() {
    let mut state = pending();
    assert!(current_pending(&state, stamp()).is_ok());
    for stale in [
        MenuActivation {
            generation: 16,
            ..stamp()
        },
        MenuActivation {
            command: 4,
            ..stamp()
        },
        MenuActivation {
            sequence: 22,
            ..stamp()
        },
    ] {
        assert!(
            current_pending(&state, stale).is_err(),
            "stale stamp {stale:?}"
        );
    }
    state.pending = None;
    assert!(
        current_pending(&state, stamp()).is_err(),
        "a consumed click cannot replay"
    );
}

#[test]
fn role_retirement_and_replacement_each_refuse_an_otherwise_matching_pending_click() {
    let mut state = pending();
    state.active = false;
    assert!(
        current_pending(&state, stamp()).is_err(),
        "retirement alone must close the gate"
    );
    state.active = true;
    state.generation = 18;
    assert!(
        current_pending(&state, stamp()).is_err(),
        "replacement alone must close the gate"
    );
}

#[test]
fn role_modifiers_use_exact_native_command_option_shift_masks() {
    assert_eq!(modifiers(MenuRole::Close), NSEventModifierFlags::Command);
    assert_eq!(
        modifiers(MenuRole::HideOthers),
        NSEventModifierFlags::Command | NSEventModifierFlags::Option
    );
    assert_eq!(
        modifiers(MenuRole::Redo),
        NSEventModifierFlags::Command | NSEventModifierFlags::Shift
    );
    assert_eq!(
        modifiers(MenuRole::PasteAndMatchStyle),
        NSEventModifierFlags::Command | NSEventModifierFlags::Option | NSEventModifierFlags::Shift
    );
    assert_eq!(modifiers(MenuRole::Unhide), NSEventModifierFlags::empty());
}

#[test]
fn no_key_window_at_click_refuses_close_even_if_a_main_window_was_reported() {
    let mut state = pending();
    let pending = state.pending.as_mut().expect("synthetic original click");
    pending.focus_at_click.app_active = true;
    pending.focus_at_click.main_window_present = true;
    let pending = current_pending(&state, stamp()).expect("current native stamp");
    assert_eq!(pending.action, MenuAction::NativeRole(MenuRole::Close));
    assert!(!pending.focus_at_click.key_window_present);
    assert!(pending.focus_at_click.main_window_present);
    assert!(matches!(
        captured_window(pending),
        Err(keld_ipc::IpcError::Protocol {
            detail: "native menu action had no key window at click"
        })
    ));
    // The target never manufactures a selected NSWindow from a main window,
    // a last-focused window, or a current UI window observed after the click.
}
