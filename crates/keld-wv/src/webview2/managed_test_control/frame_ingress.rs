//! Passive, fixed-shape initial-child observation for the real Host fixture.
//! It owns no dispatch path and borrows the existing document only for accounting.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::{Rc, Weak};
use tao::event_loop::EventLoopProxy;

use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2, ICoreWebView2Frame};
use webview2_com::{
    FrameCreatedEventHandler, FrameDestroyedEventHandler, FrameWebMessageReceivedEventHandler,
};
use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED};
use windows::core::{IUnknown, Interface as _};

use super::super::WindowsLoopEvent;
use super::super::controller_custody::AdoptedControllerClose;
use super::super::original_callbacks::registration::Registration;
use super::super::original_navigation::OriginalNavigation;
use crate::WebviewId;

mod native;
mod trace;

/// Extra fixed Rust metadata fits inside `OriginalView`'s existing 8 KiB payment.
/// The 1024 bytes cover the pinned SDK wrapper headers and three boxed `Weak`/id
/// captures; native COM/process storage is not claimed as measured Rust storage.
pub(in crate::webview2) const METADATA_BYTES: usize = std::mem::size_of::<FrameIngress>()
    + std::mem::size_of::<FrameCapture>()
    + 2 * std::mem::size_of::<usize>()
    + 1024;
const _: () = assert!(std::mem::align_of::<FrameIngress>() <= std::mem::align_of::<usize>());
const _: () = assert!(
    std::mem::size_of::<FrameCreatedEventHandler>()
        + std::mem::size_of::<FrameDestroyedEventHandler>()
        + std::mem::size_of::<FrameWebMessageReceivedEventHandler>()
        + 3 * std::mem::size_of::<(Weak<FrameIngress>, WebviewId)>()
        <= 1024
);

pub(in crate::webview2) struct FrameIngress {
    id: WebviewId,
    core: ICoreWebView2,
    navigation: Rc<RefCell<OriginalNavigation>>,
    close: Rc<AdoptedControllerClose>,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    top_identity: OnceCell<IUnknown>,
    created: Registration,
    frame: OnceCell<FrameCapture>,
    fault: Cell<Option<&'static str>>,
}

struct FrameCapture {
    core: ICoreWebView2Frame,
    identity: OnceCell<IUnknown>,
    destroyed_registration: Registration,
    message_registration: Registration,
    sender_matches_top: bool,
    destroyed: Cell<bool>,
    ready_emitted: Cell<bool>,
    message_seen: Cell<bool>,
}

impl FrameCapture {
    fn new(core: ICoreWebView2Frame, sender_matches_top: bool) -> Self {
        Self {
            core,
            identity: OnceCell::new(),
            destroyed_registration: Registration::new(),
            message_registration: Registration::new(),
            sender_matches_top,
            destroyed: Cell::new(false),
            ready_emitted: Cell::new(false),
            message_seen: Cell::new(false),
        }
    }
}

impl FrameIngress {
    /// Called only after the existing `OriginalView` fixed metadata reservation.
    /// The caller attaches this `Rc` before `install_on_ui` can enter native code.
    pub(in crate::webview2) fn prepare(
        id: WebviewId,
        core: ICoreWebView2,
        navigation: Rc<RefCell<OriginalNavigation>>,
        close: Rc<AdoptedControllerClose>,
        proxy: EventLoopProxy<WindowsLoopEvent>,
    ) -> Rc<Self> {
        Rc::new(Self {
            id,
            core,
            navigation,
            close,
            proxy,
            top_identity: OnceCell::new(),
            created: Registration::new(),
            frame: OnceCell::new(),
            fault: Cell::new(None),
        })
    }

    pub(in crate::webview2) fn install_on_ui(self: &Rc<Self>) -> windows::core::Result<()> {
        let retained = Rc::clone(self);
        native::install(&retained)
    }

    fn frame(&self) -> windows::core::Result<&FrameCapture> {
        self.frame.get().ok_or_else(|| E_UNEXPECTED.into())
    }

    fn same_frame(&self, sender: Option<&ICoreWebView2Frame>) -> windows::core::Result<bool> {
        let actual = sender.ok_or(E_POINTER)?.cast::<IUnknown>()?;
        let expected = self.frame()?.identity.get().ok_or(E_UNEXPECTED)?;
        Ok(actual == *expected)
    }

    fn latch_fault(&self, detail: &'static str) {
        if self.fault.get().is_none() {
            self.fault.set(Some(detail));
            let _ = trace::fault(self.id, detail);
        }
    }

    fn wake_after_close(&self) {
        // Only actual Close can leave the containing View waiting on this
        // callback's final Rc. Reuse the existing asynchronous service hint.
        if self.close.actual_closed()
            && self
                .proxy
                .send_event(WindowsLoopEvent::OriginalWake)
                .is_err()
        {
            self.latch_fault("closed observer service wake failed");
        }
    }

    fn ready(&self) -> windows::core::Result<()> {
        if self.fault.get().is_some() {
            return Err(E_UNEXPECTED.into());
        }
        let Some(frame) = self.frame.get() else {
            return Ok(());
        };
        if frame.ready_emitted.get()
            || !self.created.is_registered()
            || !frame.message_registration.is_registered()
            || !frame.destroyed_registration.is_registered()
        {
            return Ok(());
        }
        let already_destroyed = native::is_destroyed(&frame.core)?;
        if self.close.actual_closed() || frame.destroyed.get() || already_destroyed {
            return Err(E_UNEXPECTED.into());
        }
        // No native getter/entry follows these final observations before output.
        frame.ready_emitted.set(true);
        trace::ready(&trace::Ready {
            kind: "ready",
            view: self.id.0,
            sender_matches_top: frame.sender_matches_top,
            message_registration_returned: frame.message_registration.is_registered(),
            destroyed_registration_returned: frame.destroyed_registration.is_registered(),
            already_destroyed,
        })
    }

    /// The SAME Close owner is queried; this observer never calls Close/remove.
    /// Each callback upgrades this same Rc before native entry and keeps it
    /// through sender/args disposal, so sole ownership is actual quiescence.
    pub(in crate::webview2) fn is_drained(self: &Rc<Self>) -> bool {
        let closed = self.close.actual_closed();
        let callbacks_quiescent = Rc::strong_count(self) == 1;
        if !closed || !callbacks_quiescent || self.created.in_call() {
            return false;
        }
        if let Some(frame) = self.frame.get()
            && (frame.message_registration.in_call() || frame.destroyed_registration.in_call())
        {
            return false;
        }
        let created = self.created.clear_after_close(closed, callbacks_quiescent);
        let frame = self.frame.get().is_none_or(|frame| {
            frame
                .message_registration
                .clear_after_close(closed, callbacks_quiescent)
                && frame
                    .destroyed_registration
                    .clear_after_close(closed, callbacks_quiescent)
        });
        created && frame
    }
}

/// All actual callback inputs are owned inside the existing whole-handler
/// boundary. The upgraded Rc is declared first so it also outlives native input
/// disposal on unwind. The navigation-specific `CallbackTurn` is not reused.
fn callback<S, A>(
    weak: &Weak<FrameIngress>,
    id: WebviewId,
    sender: Option<S>,
    args: Option<A>,
    run: impl FnOnce(&Rc<FrameIngress>, Option<&S>, Option<&A>) -> windows::core::Result<()>,
) -> windows::core::Result<()> {
    let failed = |_: &'static str| {
        if let Some(owner) = weak.upgrade() {
            owner.latch_fault("native frame observation panicked");
            owner.wake_after_close();
        } else {
            let _ = trace::fault(id, "unwound callback lost its retained observer");
        }
    };
    crate::webview2::original_callbacks::boundary::boundary(&failed, move || {
        let owner = weak.upgrade();
        let inputs = (sender, args);
        let result = if let Some(owner) = &owner {
            run(owner, inputs.0.as_ref(), inputs.1.as_ref())
        } else {
            let _ = trace::fault(id, "callback has no retained View observer");
            Err(E_UNEXPECTED.into())
        };
        drop(inputs);
        if result.is_err()
            && let Some(owner) = &owner
        {
            owner.latch_fault("native frame observation refused");
        }
        if let Some(owner) = &owner {
            owner.wake_after_close();
        }
        drop(owner);
        result
    })
}
