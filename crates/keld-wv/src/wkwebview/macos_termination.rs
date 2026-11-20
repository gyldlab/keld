//! macOS native Quit cancellation until a renderer-owned veto path is qualified.
//!
//! Tao 0.35.3 installs `TaoAppDelegateParent` and observes will-terminate, but has
//! no should-terminate hook. Its dock API reads that same delegate's auxState.
//! Keep the delegate and ivars intact; add only the absent selector.
//! Source: <https://github.com/tauri-apps/tao/tree/5a14e624c81b7a799728129417e9218be25f17d9/src/platform_impl/macos>
//! Apple: <https://developer.apple.com/documentation/objectivec/class_addmethod(_:_:_:_:)>
//! Apple: <https://developer.apple.com/documentation/appkit/nsapplication/terminate(_:)>
//!
//! Verified on Darwin 25.5.0 arm64: native cancellation keeps the same delegate,
//! event loop and CoreGraphics window; duplicate registration preserves the IMP.
//! The owning GH-1404 handoff retains the baseline/bridge/conflict native probe.

#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use objc2::encode::Encode;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{MainThreadMarker, class, ffi, msg_send, sel};
use tao::event_loop::EventLoopProxy;

use super::WkUserEvent;
use crate::error::WvError;

struct Owner {
    application: usize,
    delegate: usize,
    proxy: EventLoopProxy<WkUserEvent>,
    source_attempt: Arc<AtomicU32>,
    pending: Cell<bool>,
    delivery_failed: Cell<bool>,
}

thread_local! {
    // No process-global native handle or strong sender survives a loop session.
    // The callback upgrades this weak owner only on AppKit's main thread.
    static OWNER: RefCell<Option<Weak<Owner>>> = const { RefCell::new(None) };
}

// Method registration is process lifetime. Session admission is separate.
static HOOK_CLASS: OnceLock<Result<usize, &'static str>> = OnceLock::new();

/// Owns the callback's admission for exactly this main-thread event-loop session.
/// Rc prevents moving this guard to an I/O thread. Drop retires the only strong
/// owner; later callbacks cancel without dispatch, even if the weak slot remains.
pub(super) struct NativeQuitBridge {
    owner: Rc<Owner>,
}

fn error(detail: &str) -> WvError {
    WvError::EventLoop(format!("native Quit bridge: {detail}"))
}

impl NativeQuitBridge {
    pub(super) fn bind(
        main_thread: MainThreadMarker,
        proxy: EventLoopProxy<WkUserEvent>,
        source_attempt: Arc<AtomicU32>,
    ) -> Result<Self, WvError> {
        let (application, delegate, class) = native_scope(main_thread)?;
        let registered = HOOK_CLASS.get_or_init(|| install_method(class));
        match registered {
            Ok(registered) if *registered == class => verify_method(class).map_err(error)?,
            Ok(_) => return Err(error("delegate class changed after native registration")),
            Err(detail) => return Err(error(detail)),
        }
        let owner = Rc::new(Owner {
            application,
            delegate,
            proxy,
            source_attempt,
            pending: Cell::new(false),
            delivery_failed: Cell::new(false),
        });
        OWNER
            .try_with(|slot| {
                let mut slot = slot
                    .try_borrow_mut()
                    .map_err(|_| error("callback owner is already borrowed"))?;
                if slot.as_ref().and_then(Weak::upgrade).is_some() {
                    return Err(error("another event-loop session owns native Quit"));
                }
                *slot = Some(Rc::downgrade(&owner));
                Ok(())
            })
            .map_err(|_| error("main-thread callback storage is unavailable"))??;
        Ok(Self { owner })
    }

    pub(super) fn failure(&self) -> Option<WvError> {
        self.owner
            .delivery_failed
            .get()
            .then(|| error("could not dispatch the native request to its live UI loop"))
    }

    /// Retire the posted Quit intent so later OS requests can be diagnosed.
    /// A native close request cannot prove that the legacy renderer accepted
    /// its close, so no window is touched until GH451's veto owner is qualified.
    pub(super) fn retain_unqualified_quit(&self) {
        self.owner.pending.set(false);
    }
}

#[allow(unsafe_code)]
fn native_scope(_: MainThreadMarker) -> Result<(usize, usize, usize), WvError> {
    // SAFETY: Tao has already constructed NSApplication on this proven main
    // thread and owns its live delegate for the surrounding EventLoop. Objective-C
    // getters use their exact signatures; no delegate or ivar is changed.
    unsafe {
        let application: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if application.is_null() {
            return Err(error("NSApplication is unavailable"));
        }
        let delegate: *mut AnyObject = msg_send![application, delegate];
        if delegate.is_null() {
            return Err(error("NSApplication has no delegate"));
        }
        let class = ffi::object_getClass(delegate);
        if class.is_null() || CStr::from_ptr(ffi::class_getName(class)) != c"TaoAppDelegateParent" {
            return Err(error("the current delegate is not the qualified Tao owner"));
        }
        Ok((application as usize, delegate as usize, class as usize))
    }
}

fn encoding() -> Result<CString, &'static str> {
    // Apple's NSApplicationTerminateReply is NSUInteger; objc2 maps it to usize.
    CString::new(format!("{}@:@", <usize as Encode>::ENCODING))
        .map_err(|_| "native callback encoding is invalid")
}

#[allow(unsafe_code)]
fn callback_imp() -> Imp {
    // SAFETY: The runtime stores an erased IMP, never called directly here.
    // class_addMethod receives the exact self/_cmd/NSApplication* -> NSUInteger
    // encoding of should_terminate, including its C-unwind calling convention.
    unsafe {
        std::mem::transmute::<
            unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
            Imp,
        >(should_terminate)
    }
}

#[allow(unsafe_code)]
fn install_method(class: usize) -> Result<usize, &'static str> {
    let types = encoding()?;
    // SAFETY: class comes only from native_scope on the main thread. The
    // registered class outlives this process. Never replace or shadow an existing
    // method, change its superclass, touch ivars, or replace the delegate object.
    unsafe {
        let class_pointer = class as *mut AnyClass;
        let selector = sel!(applicationShouldTerminate:);
        if !ffi::class_getInstanceMethod(class_pointer, selector).is_null() {
            return Err("the native Quit selector already has an owner");
        }
        if !ffi::class_addMethod(class_pointer, selector, callback_imp(), types.as_ptr()).as_bool()
        {
            return Err("the native Quit selector could not be added");
        }
    }
    verify_method(class)?;
    Ok(class)
}

#[allow(unsafe_code)]
fn verify_method(class: usize) -> Result<(), &'static str> {
    let types = encoding()?;
    // SAFETY: The exact process-lifetime class was obtained on the UI thread.
    // Runtime method metadata is read only; the erased implementation is compared,
    // never called. The runtime owns its non-null type-encoding C string.
    unsafe {
        let method = ffi::class_getInstanceMethod(
            class as *const AnyClass,
            sel!(applicationShouldTerminate:),
        );
        if method.is_null() {
            return Err("the native Quit selector disappeared");
        }
        let actual_types = ffi::method_getTypeEncoding(method);
        if actual_types.is_null() || CStr::from_ptr(actual_types) != types.as_c_str() {
            return Err("the native Quit selector encoding changed");
        }
        let implementation = ffi::method_getImplementation(method);
        if implementation.is_none_or(|imp| imp as usize != callback_imp() as usize) {
            return Err("the native Quit selector implementation changed");
        }
    }
    Ok(())
}

#[allow(unsafe_code)]
unsafe extern "C-unwind" fn should_terminate(
    delegate: *mut AnyObject,
    _: Sel,
    application: *mut AnyObject,
) -> usize {
    // SAFETY: These opaque callback arguments are compared only, never
    // dereferenced. No AppKit API is called. Fallible TLS/borrow/weak operations
    // keep the C callback non-panicking and reject absent or retired owners.
    let owner = OWNER
        .try_with(|slot| {
            slot.try_borrow()
                .ok()
                .and_then(|slot| slot.as_ref().and_then(Weak::upgrade))
        })
        .ok()
        .flatten();
    if let Some(owner) = owner
        && owner.application == application as usize
        && owner.delegate == delegate as usize
    {
        // Capture Core's original role on this AppKit callback, before Tao
        // can buffer the event across a recovery. Zero is a safe cancel.
        let source_attempt = owner.source_attempt.load(Ordering::Acquire);
        if source_attempt != 0
            && !owner.pending.replace(true)
            && owner
                .proxy
                .send_event(WkUserEvent::NativeQuitRequested { source_attempt })
                .is_err()
        {
            owner.delivery_failed.set(true);
        }
    }
    0 // NSTerminateCancel: only the existing application Quit call may terminate.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unowned_native_quit_is_canceled_without_appkit() {
        // SAFETY: The hook compares these opaque null pointers without using
        // them. No session was bound, so it must cancel without any AppKit call.
        #[allow(unsafe_code)]
        let reply = unsafe {
            should_terminate(
                std::ptr::null_mut(),
                sel!(applicationShouldTerminate:),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(reply, 0, "an unowned native Quit must cancel");
    }

    #[test]
    fn native_quit_has_no_unqualified_native_window_close_primitive() {
        // Source guard, not an installed-AppKit acceptance test. This checks
        // the high-risk primitive at the native Quit hook owner. The renderer
        // has no synchronous close/preventDefault receipt for this legacy path.
        let source = include_str!("macos_termination.rs");
        assert!(
            !source.contains(concat!("perform", "Close:")),
            "native Quit must not request an NSWindow close before a renderer-owned veto"
        );
    }
}
