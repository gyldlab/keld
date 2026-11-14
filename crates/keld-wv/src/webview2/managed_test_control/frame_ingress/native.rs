//! Only actual retained STA frame subscriptions and bounded native getters.
use std::rc::Rc;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2_4, ICoreWebView2Frame, ICoreWebView2Frame2,
    ICoreWebView2FrameCreatedEventArgs, ICoreWebView2WebMessageReceivedEventArgs,
};
use webview2_com::{
    FrameCreatedEventHandler, FrameDestroyedEventHandler, FrameWebMessageReceivedEventHandler,
};
use windows::Win32::Foundation::{E_INVALIDARG, E_POINTER, E_UNEXPECTED};
use windows::core::{BOOL, IUnknown, Interface as _};

use super::{FrameCapture, FrameIngress, callback, trace};
use crate::webview2::native_text::read_task_text;
use crate::webview2::original_callbacks::callback_credit;

#[allow(unsafe_code)] // Exact creating-STA FrameCreated registration on the retained View.
pub(super) fn install(owner: &Rc<FrameIngress>) -> windows::core::Result<()> {
    owner
        .top_identity
        .set(owner.core.cast::<IUnknown>()?)
        .map_err(|_| E_UNEXPECTED)?;
    let view = owner.core.cast::<ICoreWebView2_4>()?;
    let weak = Rc::downgrade(owner);
    let id = owner.id;
    let handler = FrameCreatedEventHandler::create(Box::new(move |sender, args| {
        callback(&weak, id, sender, args, created)
    }));
    owner.created.add(
        |token| {
            // SAFETY: same retained STA view; attached out-token remains alive.
            unsafe { view.add_FrameCreated(&handler, token.as_ptr()) }
        },
        || owner.latch_fault("FrameCreated registration panicked"),
    )?;
    owner.ready()
}

#[allow(unsafe_code)] // Actual FrameCreated.Frame and two frame event Add operations only.
fn created(
    owner: &Rc<FrameIngress>,
    sender: Option<&ICoreWebView2>,
    args: Option<&ICoreWebView2FrameCreatedEventArgs>,
) -> windows::core::Result<()> {
    let actual = sender.ok_or(E_POINTER)?.cast::<IUnknown>()?;
    let expected = owner.top_identity.get().ok_or(E_UNEXPECTED)?;
    let sender_matches_top = actual == *expected;
    if !sender_matches_top || owner.frame.get().is_some() || owner.close.actual_closed() {
        return Err(E_INVALIDARG.into());
    }
    // SAFETY: actual retained FrameCreated args on their creating STA.
    let frame = unsafe { args.ok_or(E_POINTER)?.Frame() }?;
    // Attach the actual frame before QI or native Add can fail/reenter.
    owner
        .frame
        .set(FrameCapture::new(frame, sender_matches_top))
        .map_err(|_| E_UNEXPECTED)?;
    let frame = owner.frame()?;
    frame
        .identity
        .set(frame.core.cast::<IUnknown>()?)
        .map_err(|_| E_UNEXPECTED)?;
    let frame2 = frame.core.cast::<ICoreWebView2Frame2>()?;

    let weak = Rc::downgrade(owner);
    let id = owner.id;
    let destroyed = FrameDestroyedEventHandler::create(Box::new(move |sender, args| {
        callback(&weak, id, sender, args, |owner, sender, _args| {
            let same = owner.same_frame(sender)?;
            if !same {
                return Err(E_INVALIDARG.into());
            }
            owner.frame()?.destroyed.set(true);
            trace::destroyed(owner.id, same)
        })
    }));
    frame.destroyed_registration.add(
        |token| {
            // SAFETY: actual frame retained in the attached owner; stable token.
            unsafe { frame.core.add_Destroyed(&destroyed, token.as_ptr()) }
        },
        || owner.latch_fault("frame Destroyed registration panicked"),
    )?;

    let weak = Rc::downgrade(owner);
    let id = owner.id;
    let message = FrameWebMessageReceivedEventHandler::create(Box::new(move |sender, args| {
        callback(&weak, id, sender, args, received)
    }));
    frame.message_registration.add(
        |token| {
            // SAFETY: QI of that same retained frame, with attached token storage.
            unsafe { frame2.add_WebMessageReceived(&message, token.as_ptr()) }
        },
        || owner.latch_fault("Frame2 message registration panicked"),
    )?;
    owner.ready()
}

#[allow(unsafe_code)] // Actual same-frame IsDestroyed getter, with a fail-closed output.
pub(super) fn is_destroyed(frame: &ICoreWebView2Frame) -> windows::core::Result<bool> {
    let mut destroyed = BOOL(1);
    // SAFETY: same retained STA frame and writable initialized BOOL.
    unsafe { frame.IsDestroyed(&raw mut destroyed) }?;
    Ok(destroyed.as_bool())
}

#[allow(unsafe_code)] // Actual frame message Source/WebMessageAsJson getters only.
fn received(
    owner: &Rc<FrameIngress>,
    sender: Option<&ICoreWebView2Frame>,
    args: Option<&ICoreWebView2WebMessageReceivedEventArgs>,
) -> windows::core::Result<()> {
    let same = owner.same_frame(sender)?;
    let frame = owner.frame()?;
    if !same
        || !frame.ready_emitted.get()
        || frame.destroyed.get()
        || owner.close.actual_closed()
        || frame.message_seen.replace(true)
    {
        return Err(E_INVALIDARG.into());
    }
    let args = args.ok_or(E_POINTER)?;
    // The original source account pays BEFORE getters and remains through both
    // returned Strings and the borrowed streaming trace. No DATA credit minted.
    let _credit = callback_credit(&owner.navigation)?;
    // SAFETY: actual message args; the shared helper frees each COM task allocation.
    let source = read_task_text(|out| unsafe { args.Source(out) }, 256, 1024)?;
    // Complete outer JSON preserves embedded NUL and rejects non-string messages
    // at the separate observation oracle; never use the truncating string getter.
    // SAFETY: same actual args, bounded strict copy and one matching COM free.
    let outer = read_task_text(|out| unsafe { args.WebMessageAsJson(out) }, 1024, 4096)?;
    if owner.close.actual_closed() || frame.destroyed.get() {
        return Err(E_UNEXPECTED.into());
    }
    trace::message(owner.id, same, &source, &outer)
}
