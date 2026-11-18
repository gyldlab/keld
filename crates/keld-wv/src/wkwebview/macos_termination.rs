//! macOS native Quit admission for the existing renderer-window close owner.
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
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::rc::{Rc, Weak};
use std::sync::OnceLock;

use objc2::encode::Encode;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{MainThreadMarker, class, ffi, msg_send, sel};
use tao::event_loop::EventLoopProxy;
use wry::WebViewExtMacOS;

use super::{View, WkUserEvent};
use crate::engine::AppReadyTrigger;
use crate::error::WvError;

struct Owner {
    application: usize,
    delegate: usize,
    proxy: EventLoopProxy<WkUserEvent>,
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
    main_thread: MainThreadMarker,
}

fn error(detail: &str) -> WvError {
    WvError::EventLoop(format!("native Quit bridge: {detail}"))
}

impl NativeQuitBridge {
    pub(super) fn bind(
        main_thread: MainThreadMarker,
        proxy: EventLoopProxy<WkUserEvent>,
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
        Ok(Self { owner, main_thread })
    }

    pub(super) fn failure(&self) -> Option<WvError> {
        self.owner
            .delivery_failed
            .get()
            .then(|| error("could not dispatch the native request to its live UI loop"))
    }

    /// Request the actual native close path only for the admitted renderer slice.
    /// The existing `CloseRequested` handler still owns release and anchor draining.
    pub(super) fn request_renderer_close(
        &self,
        ready_trigger: AppReadyTrigger,
        navigation_ready: bool,
        views: &BTreeMap<u32, View>,
        closing: &[u32],
    ) -> Result<bool, WvError> {
        self.owner.pending.set(false);
        if ready_trigger != AppReadyTrigger::NavigationReady
            || !navigation_ready
            || views.len() != 1
        {
            return Ok(false);
        }
        let Some((id, view)) = views.first_key_value() else {
            return Ok(false);
        };
        // Registry/original/mixed apps require their existing app Quit intent and
        // veto owner. Never broaden this renderer close path to those modes.
        if view.registry_managed || closing.contains(id) {
            return Ok(false);
        }
        let window = view
            .webview
            .webview()
            .window()
            .ok_or_else(|| error("the admitted renderer no longer owns its native window"))?;
        request_close(&window, self.main_thread);
        Ok(true)
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
        && !owner.pending.replace(true)
        && owner
            .proxy
            .send_event(WkUserEvent::NativeQuitRequested)
            .is_err()
    {
        owner.delivery_failed.set(true);
    }
    0 // NSTerminateCancel: only the existing application Quit call may terminate.
}

#[allow(unsafe_code)]
fn request_close(window: &objc2_app_kit::NSWindow, _: MainThreadMarker) {
    // SAFETY: This retained, currently owned NSWindow is used on the UI thread.
    // performClose: takes a nullable object sender and returns void. Apple's
    // delegate veto remains intact. Tao 0.35.3 window_should_close enqueues a real
    // CloseRequested and returns NO; no recursive Keld callback or direct drop.
    // <https://developer.apple.com/documentation/appkit/nswindow/performclose(_:)>
    // Pinned Tao window_delegate.rs and app_state.rs: emit_event -> queue_event.
    unsafe {
        let _: () = msg_send![window, performClose: std::ptr::null_mut::<AnyObject>()];
    }
}
