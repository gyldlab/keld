//! GH480: exact main-thread `NSObject` target ABI and fixed native edit selectors.
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::{Retained, Weak};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSMenuItem, NSResponder};
use objc2_foundation::{NSObject, NSObjectProtocol};

use super::{
    ActivationState, MenuAction, MenuActivation, MenuFocusSnapshot, MenuRole, PendingActivation,
};

pub(super) struct TargetIvars {
    state: Rc<RefCell<ActivationState>>,
    generation: u64,
    command: u16,
    action: MenuAction,
    enabled: bool,
}

define_class!(
    // SAFETY: Final NSObject subclass initialized with owned Rust ivars. Both
    // AppKit target invocation and the retained owner stay on the main thread.
    #[allow(unsafe_code, reason = "GH480 exact MenuTarget NSObject class ABI")]
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KeldApplicationMenuTarget"]
    #[ivars = TargetIvars]
    pub(super) struct MenuTarget;

    // SAFETY: The initialized NSObject subclass adds no protocol requirements.
    #[allow(unsafe_code, reason = "GH480 exact MenuTarget NSObjectProtocol implementation")]
    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        // SAFETY: NSMenuItem action has void return and exactly one id sender.
        // The sender is an AppKit-owned NSMenuItem borrowed for this invocation;
        // the callback neither adopts nor retains any borrowed runtime pointer.
        #[allow(unsafe_code, reason = "GH480 exact main-thread activate:(NSMenuItem*) callback ABI")]
        #[unsafe(method(activate:))]
        fn activate(&self, _sender: &NSMenuItem) {
            let ivars = self.ivars();
            let mut state = ivars.state.borrow_mut();
            if !state.active || state.generation != ivars.generation || !ivars.enabled || state.pending.is_some() {
                return;
            }
            let Some(sequence) = state.sequence.checked_add(1) else { return; };
            state.sequence = sequence;
            let app = NSApplication::sharedApplication(self.mtm());
            let window = app.keyWindow();
            let responder = window.as_deref().and_then(objc2_app_kit::NSWindow::firstResponder);
            state.pending = Some(PendingActivation {
                stamp: MenuActivation { generation: ivars.generation, command: ivars.command, sequence },
                action: ivars.action,
                focus_at_click: MenuFocusSnapshot {
                    app_active: app.isActive(),
                    key_window_present: window.is_some(),
                    main_window_present: app.mainWindow().is_some(),
                },
                window: window.as_ref().map(Weak::from_retained),
                responder: responder.as_ref().map(Weak::from_retained),
            });
            let wake = Rc::clone(&state.wake);
            drop(state);
            // Only post the existing UI-loop wake. No mutable state borrow is
            // held across even a reentrant Host service notification.
            wake();
        }
    }
);

impl MenuTarget {
    #[allow(unsafe_code, reason = "GH480 exact NSObject superclass initialization")]
    pub(super) fn new(
        main: MainThreadMarker,
        state: Rc<RefCell<ActivationState>>,
        generation: u64,
        command: u16,
        action: MenuAction,
        enabled: bool,
    ) -> Option<Retained<Self>> {
        let allocated = Self::alloc(main).set_ivars(TargetIvars {
            state,
            generation,
            command,
            action,
            enabled,
        });
        // SAFETY: One allocated final NSObject with fully initialized owned
        // ivars; exact superclass init, with nil preserved as None.
        unsafe { msg_send![super(allocated), init] }
    }

    #[allow(
        unsafe_code,
        reason = "GH480 exact weak target and fixed activate: selector"
    )]
    pub(super) fn attach(&self, item: &NSMenuItem) {
        // SAFETY: This initialized main-thread target implements exactly the
        // void activate:(NSMenuItem*) ABI. InstalledMenu retains it until the
        // weak target and action are cleared; no caller supplies the selector.
        unsafe {
            item.setTarget(Some(self));
            item.setAction(Some(sel!(activate:)));
        }
    }

    #[allow(
        unsafe_code,
        reason = "GH480 clearing exact weak target and action before owner drop"
    )]
    pub(super) fn detach(item: &NSMenuItem) {
        // SAFETY: Both setters clear references on the AppKit main thread,
        // while the exact previously attached target is still retained.
        unsafe {
            item.setAction(None);
            item.setTarget(None);
        }
    }
}

#[allow(
    unsafe_code,
    reason = "GH480 fixed native edit selectors on the verified focused responder"
)]
pub(super) fn perform_edit(responder: &NSResponder, role: MenuRole) -> bool {
    let action = match role {
        MenuRole::Undo => sel!(undo:),
        MenuRole::Redo => sel!(redo:),
        MenuRole::Cut => sel!(cut:),
        MenuRole::Copy => sel!(copy:),
        MenuRole::Paste => sel!(paste:),
        MenuRole::PasteAndMatchStyle => sel!(pasteAndMatchStyle:),
        MenuRole::SelectAll => sel!(selectAll:),
        MenuRole::Close
        | MenuRole::Quit
        | MenuRole::Hide
        | MenuRole::HideOthers
        | MenuRole::Unhide => return false,
    };
    // SAFETY: Every selector is a fixed AppKit void action with one nullable
    // id sender. The retained click-time first responder was just rechecked
    // against the same key window; tryToPerform walks only its responder chain.
    // No raw caller selector, native close, terminate or JS evaluation exists.
    unsafe { responder.tryToPerform_with(action, None) }
}
