//! Fault-result injection after a real successful native wait cancellation.
//!
//! The kernel wait is already retired before this controller withholds its success.
//! Retained resources are escrowed only in that known-safe state and reclaimed by
//! this fixture after observing product refusal. This is not a spontaneous API fault.

use std::cell::{Cell, RefCell};
use std::os::windows::io::{FromRawHandle as _, OwnedHandle};
use tao::event_loop::EventLoopProxy;
use windows::Win32::Foundation::{E_UNEXPECTED, GetHandleInformation, HANDLE};

use super::failure;
use crate::WvError;
use crate::webview2::WindowsLoopEvent;

thread_local! {
    static ACTIVE: Cell<Option<bool>> = const { Cell::new(None) };
    static ATTEMPTS: Cell<u32> = const { Cell::new(0) };
    static NATIVE_SUCCESS: Cell<u32> = const { Cell::new(0) };
    static INJECTED: Cell<u32> = const { Cell::new(0) };
    static ESCROW: RefCell<Vec<RetiredResources>> = const { RefCell::new(Vec::new()) };
}

struct RetiredResources {
    proxy: *mut EventLoopProxy<WindowsLoopEvent>,
    process: HANDLE,
}
impl Drop for RetiredResources {
    fn drop(&mut self) {
        // SAFETY: escrow is created only after actual blocking UnregisterWaitEx
        // succeeded, so no callback can reach the original proxy or process.
        // The injected Err made the original owner retain, not free, these exact
        // resources. This one fixture owner reclaims them after product observation.
        // https://learn.microsoft.com/windows/win32/api/threadpoollegacyapiset/nf-threadpoollegacyapiset-unregisterwaitex
        unsafe {
            drop(Box::from_raw(self.proxy));
            drop(OwnedHandle::from_raw_handle(self.process.0));
        }
    }
}

#[derive(Debug)]
pub(crate) struct Observations {
    pub(crate) attempts: u32,
    pub(crate) native_successes: u32,
    pub(crate) injected_errors: u32,
    pub(crate) retained_live_handles: usize,
}

pub(crate) struct Scope;
impl Scope {
    pub(crate) fn enter(inject_error: bool) -> Self {
        assert!(
            ACTIVE.replace(Some(inject_error)).is_none(),
            "overlapping retirement fixture"
        );
        assert!(ESCROW.with_borrow(Vec::is_empty));
        ATTEMPTS.set(0);
        NATIVE_SUCCESS.set(0);
        INJECTED.set(0);
        Self
    }

    pub(crate) fn observations() -> Result<Observations, WvError> {
        let retained_live_handles = ESCROW.with_borrow(|resources| {
            for resource in resources {
                let mut flags = 0;
                // SAFETY: the known-unregistered fixture escrow retains this
                // exact process handle. Query only; never reconstruct from a PID.
                // https://learn.microsoft.com/windows/win32/api/handleapi/nf-handleapi-gethandleinformation
                unsafe { GetHandleInformation(resource.process, &raw mut flags) }
                    .map_err(failure)?;
            }
            Ok::<_, WvError>(resources.len())
        })?;
        Ok(Observations {
            attempts: ATTEMPTS.get(),
            native_successes: NATIVE_SUCCESS.get(),
            injected_errors: INJECTED.get(),
            retained_live_handles,
        })
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.set(None);
        ESCROW.with_borrow_mut(Vec::clear);
    }
}

/// Records the real cancellation and optionally transfers known-retired resources
/// into the fixture escrow while reporting an injected error to the product.
///
/// # Safety
///
/// `proxy` must be the one live Box allocation owned by this registration, and
/// `process` its still-owned handle. An Ok input must mean its actual blocking
/// `UnregisterWaitEx` completed. If this returns an injected error, the caller must
/// retain both resources rather than free them; the active Scope owns reclamation.
pub(in crate::webview2) unsafe fn after_native_unregister(
    result: windows::core::Result<()>,
    proxy: *mut EventLoopProxy<WindowsLoopEvent>,
    process: HANDLE,
) -> windows::core::Result<()> {
    let Some(inject) = ACTIVE.get() else {
        return result;
    };
    ATTEMPTS.set(ATTEMPTS.get() + 1);
    if result.is_ok() {
        NATIVE_SUCCESS.set(NATIVE_SUCCESS.get() + 1);
        if inject && INJECTED.get() == 0 {
            INJECTED.set(1);
            ESCROW.with_borrow_mut(|resources| resources.push(RetiredResources { proxy, process }));
            println!("KELD_WAKE_RETIRE_INJECT native_unregistered=true result_withheld=true");
            return Err(windows::core::Error::from(E_UNEXPECTED));
        }
    }
    result
}
