//! The three existing native event positions retain their exact call/token facts.
#![forbid(unsafe_code)]

use std::cell::Cell;
use windows::Win32::Foundation::E_UNEXPECTED;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Fresh,
    Adding,
    Registered,
    Unproved,
    Closed,
}

pub(in crate::webview2) struct Registration {
    phase: Cell<Phase>,
    token: Cell<i64>,
}

impl Registration {
    pub(in crate::webview2) fn new() -> Self {
        Self {
            phase: Cell::new(Phase::Fresh),
            token: Cell::new(0),
        }
    }

    /// The attached slot is installed before entry, including its actual out-token.
    /// No local handler may Release before this method records the call's return.
    #[expect(
        clippy::single_match_else,
        reason = "Preserve the explicit native-return and unwind arms with their original payload drop scopes"
    )]
    pub(in crate::webview2) fn add(
        &self,
        run: impl FnOnce(&Cell<i64>) -> windows::core::Result<()>,
        on_unwind: impl FnOnce(),
    ) -> windows::core::Result<()> {
        if self.phase.get() != Phase::Fresh {
            return Err(E_UNEXPECTED.into());
        }
        self.phase.set(Phase::Adding);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&self.token)));
        self.phase.set(if matches!(&result, Ok(Ok(()))) {
            Phase::Registered
        } else {
            Phase::Unproved
        });
        match result {
            Ok(result) => result,
            Err(_) => {
                on_unwind();
                Err(E_UNEXPECTED.into())
            }
        }
    }

    pub(in crate::webview2) fn in_call(&self) -> bool {
        self.phase.get() == Phase::Adding
    }

    /// Test observer reads the same actual Add return, with no new receipt.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(in crate::webview2) fn is_registered(&self) -> bool {
        self.phase.get() == Phase::Registered
    }

    /// Pure observation of the same actual-Close-and-callback retirement.
    pub(in crate::webview2) fn is_cleared(&self) -> bool {
        self.phase.get() == Phase::Closed
    }

    /// Native Close is the sole removal receipt. A live native call or callback
    /// must retain its position even if Close was observed reentrantly.
    pub(in crate::webview2) fn clear_after_close(
        &self,
        closed: bool,
        callbacks_quiescent: bool,
    ) -> bool {
        if !closed || !callbacks_quiescent || self.in_call() {
            return false;
        }
        self.phase.set(Phase::Closed);
        self.token.set(0);
        true
    }
}

#[cfg(test)]
mod tests;
