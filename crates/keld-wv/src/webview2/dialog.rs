//! Retained parent and deferred driver boundary for native Windows dialogs.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tao::platform::windows::WindowExtWindows as _;
use tao::window::Window;

use crate::{WebviewId, WvError};

/// A live managed W2 window retained on its UI thread. Only W2 can construct it.
/// Core's registry identity still decides whether an admitted operation is live.
#[derive(Debug)]
pub struct WindowsDialogParent {
    window: Rc<Window>,
}

impl WindowsDialogParent {
    pub(super) fn new(window: Rc<Window>) -> Self {
        Self { window }
    }

    /// Borrow the real native handle while this same-thread lease is retained.
    #[must_use]
    pub fn hwnd(&self) -> isize {
        self.window.hwnd()
    }
}

/// Resolve only the current managed view, releasing its map borrow on return.
pub type WindowsDialogParentResolver<'a> = dyn FnMut(WebviewId) -> Option<WindowsDialogParent> + 'a;

/// Core's existing custody owner, driven after native event handlers return.
/// The atomic is the existing command bridge's terminal intent, not a new policy.
pub type WindowsDialogDriver<'a> =
    dyn FnMut(&mut WindowsDialogParentResolver<'_>, Arc<AtomicBool>) -> Result<(), WvError> + 'a;
