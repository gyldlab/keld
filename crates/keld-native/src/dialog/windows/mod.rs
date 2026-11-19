//! Windows presentation on the retained Host UI STA. This owns native objects;
//! Guard, parent identity, deadlines, and winning terminals remain in Core.
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use keld_ipc::IpcError;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    APTTYPE_CURRENT, APTTYPE_MAINSTA, APTTYPE_STA, APTTYPEQUALIFIER_NONE, CoGetApartmentType,
};
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindowVisible};
use windows::core::HSTRING;

use super::{
    DialogReplyV1, MessageOptions, OpenDialogOptions, SaveDialogOptions, invalid,
    validate_dialog_reply,
};

mod file;
mod file_timer;
mod message;

/// Borrowed, already admitted shared dialog options.
#[derive(Clone, Copy, Debug)]
pub enum WindowsDialogOptions<'a> {
    /// The existing ordered button/checkbox contract.
    Message(&'a MessageOptions),
    /// Real filesystem selection, without a filesystem grant.
    Open(&'a OpenDialogOptions),
    /// A save location; presentation never creates the application file.
    Save(&'a SaveDialogOptions),
}

/// Presentation failure, separate from a genuine user cancellation.
#[derive(Debug)]
pub enum WindowsDialogError {
    /// Missing, hidden, disabled, or non-UI-thread parent.
    UnsupportedParent,
    /// Native setup, callback, result or physical cleanup failed.
    Failed(IpcError),
}

impl From<IpcError> for WindowsDialogError {
    fn from(error: IpcError) -> Self {
        Self::Failed(error)
    }
}

/// Present one real dialog from the existing deferred UI dispatcher.
///
/// Core must retain its resolved W2 parent lease for this entire synchronous
/// call and invoke it after the originating `WebView2` callback has returned.
/// The predicate reads that operation's existing custody; it must not create a
/// second role, deadline, or cancellation policy. None means Host cancellation,
/// while native file Cancel is a Some reply with empty paths.
///
/// # Errors
///
/// Refuses an unsupported parent/STA, invalid options, native failure or an
/// invalid OS result. No successful value is manufactured on failure.
pub fn show(
    parent: isize,
    options: WindowsDialogOptions<'_>,
    must_cancel: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<Option<DialogReplyV1>, WindowsDialogError> {
    let parent = checked_parent(parent)?;
    let cancel = Rc::new(Cancellation {
        parent,
        predicate: must_cancel,
        requested: Cell::new(false),
        failed: Cell::new(false),
    });
    if cancel.check() {
        return cancel.finish(None).map_err(Into::into);
    }
    let result = match options {
        WindowsDialogOptions::Open(options) => file::open(parent, options, &cancel),
        WindowsDialogOptions::Save(options) => file::save(parent, options, &cancel),
        WindowsDialogOptions::Message(options) => message::show(parent, options, &cancel),
    }?;
    if let Some(reply) = &result {
        validate_dialog_reply(reply)?;
    }
    Ok(result)
}

#[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
fn checked_parent(raw: isize) -> Result<HWND, WindowsDialogError> {
    let parent = HWND(raw as *mut std::ffi::c_void);
    let mut process = 0;
    let mut apartment = APTTYPE_CURRENT;
    let mut qualifier = APTTYPEQUALIFIER_NONE;
    // SAFETY: these APIs query opaque HWND/thread identifiers only. They do not
    // adopt/dereference a caller pointer. Core supplies its retained W2 handle;
    // same-thread ownership prevents another thread destroying it during entry.
    let valid = unsafe {
        raw != 0
            && GetWindowThreadProcessId(parent, Some(&raw mut process)) == GetCurrentThreadId()
            && process == GetCurrentProcessId()
            && IsWindowVisible(parent).as_bool()
            && IsWindowEnabled(parent).as_bool()
            && CoGetApartmentType(&raw mut apartment, &raw mut qualifier).is_ok()
    };
    if !valid || !matches!(apartment, APTTYPE_STA | APTTYPE_MAINSTA) {
        return Err(WindowsDialogError::UnsupportedParent);
    }
    Ok(parent)
}

struct Cancellation {
    parent: HWND,
    predicate: Arc<dyn Fn() -> bool + Send + Sync>,
    requested: Cell<bool>,
    failed: Cell<bool>,
}

impl Cancellation {
    #[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
    fn check(&self) -> bool {
        if self.requested.get() {
            return true;
        }
        let requested = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (self.predicate)()
                // SAFETY: querying the retained parent is synchronous on its
                // owning STA. Modal disablement is expected; disappearance is not.
                || unsafe { !IsWindowVisible(self.parent).as_bool() }
        }))
        .unwrap_or_else(|_| {
            self.failed.set(true);
            true
        });
        self.requested.set(requested);
        requested
    }

    fn finish(&self, value: Option<DialogReplyV1>) -> Result<Option<DialogReplyV1>, IpcError> {
        let requested = self.check();
        if self.failed.get() {
            Err(invalid("Windows dialog callback or cleanup failed"))
        } else if requested {
            Ok(None)
        } else {
            Ok(value)
        }
    }
}

fn wide(value: &str) -> Result<HSTRING, IpcError> {
    if value.contains('\0') {
        return Err(invalid("Windows dialog text contains a NUL"));
    }
    Ok(HSTRING::from(value))
}

fn native<T>(result: windows::core::Result<T>, detail: &'static str) -> Result<T, IpcError> {
    result.map_err(|error| {
        // The HRESULT is diagnostic only; request options/paths are not logged.
        eprintln!("KELD-NATIVE-016: {detail}: {}", error.code());
        invalid(detail)
    })
}
