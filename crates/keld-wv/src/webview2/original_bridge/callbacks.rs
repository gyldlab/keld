//! Actual top-view subscriptions. Same attached Registration positions and
//! singleton failure sink; no navigation-specific hold is repurposed.
use super::super::original_callbacks::boundary::{FailureSink, boundary};
use super::{OriginalBridge, Rc, WvError, canonical_webview_identity, failure};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
use webview2_com::{DOMContentLoadedEventHandler, WebMessageReceivedEventHandler};
use windows::Win32::Foundation::{E_INVALIDARG, E_POINTER, E_UNEXPECTED};
use windows::core::Interface;

struct CallbackTurn(Rc<OriginalBridge>);
impl CallbackTurn {
    fn enter(owner: &Rc<OriginalBridge>) -> windows::core::Result<Self> {
        let next = owner
            .callback_depth
            .get()
            .and_then(|depth| depth.checked_add(1));
        owner.callback_depth.set(next);
        if next.is_none() {
            owner.fail("original callback depth lost its physical count");
            return Err(E_UNEXPECTED.into());
        }
        Ok(Self(Rc::clone(owner)))
    }
}
impl Drop for CallbackTurn {
    fn drop(&mut self) {
        let next = self
            .0
            .callback_depth
            .get()
            .and_then(|depth| depth.checked_sub(1));
        self.0.callback_depth.set(next);
        if next.is_none() {
            self.0.fail("original callback exit lost physical custody");
        }
        self.0.wake();
    }
}

impl OriginalBridge {
    /// Root has attached this Rc to the complete retained View before entry.
    /// Failed/partial registration remains here until actual same Close.
    pub(in crate::webview2) fn install_on_ui(self: &Rc<Self>) -> Result<(), WvError> {
        if self.install_attempted.replace(true) || self.retired.get() || self.close.actual_closed()
        {
            return Err(failure("original bridge subscription cannot be retried"));
        }
        self.installing.set(true);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            install_message(self)?;
            if self.retired.get() || self.close.actual_closed() {
                return Err(E_UNEXPECTED.into());
            }
            install_dom(self)
        }));
        self.installing.set(false);
        match result {
            Ok(Ok(())) if !self.retired.get() && !self.close.actual_closed() => Ok(()),
            _ => {
                self.fail("original bridge native subscription did not complete");
                Err(failure(
                    "original bridge native subscription did not complete",
                ))
            }
        }
    }
}

#[allow(unsafe_code)] // Only the exact retained top-view event subscription.
fn install_message(owner: &Rc<OriginalBridge>) -> windows::core::Result<()> {
    let weak = Rc::downgrade(owner);
    let sink = FailureSink::new(&owner.session, owner.id);
    let handler = WebMessageReceivedEventHandler::create(Box::new(move |sender, args| {
        let failed = |detail| sink.latch(detail);
        boundary(&failed, || {
            let retained = weak.upgrade(); // Kept before any native getter.
            let captured = (sender, args);
            let Some(owner) = retained.as_ref() else {
                failed("message callback lost its attached bridge owner");
                return Err(E_UNEXPECTED.into());
            };
            let turn = CallbackTurn::enter(owner)?;
            let result = (|| {
                let (sender, args) = captured; // Both released before turn and retained Rc.
                let sender = sender.as_ref().ok_or_else(|| {
                    owner.fail("message callback has no native sender");
                    windows::core::Error::from(E_POINTER)
                })?;
                let args = args.as_ref().ok_or_else(|| {
                    owner.fail("message callback has no native arguments");
                    windows::core::Error::from(E_POINTER)
                })?;
                let identity = canonical_webview_identity(sender).inspect_err(|_| {
                    owner.fail("message callback native sender identity is unavailable");
                })?;
                if identity != owner.identity {
                    owner.fail("message callback has a foreign native sender");
                    return Err(E_INVALIDARG.into());
                }
                // Canonical malformed/credential/capacity refusal is bounded
                // pre-entry rejection. Custody faults latch at their owner.
                owner
                    .receive_message_on_ui(args)
                    .map_err(|_| windows::core::Error::from(E_INVALIDARG))
            })();
            drop(turn);
            result
        })
    }));
    owner.message_registration.add(
        |token| {
            // SAFETY: attached token Cell and exact creating-STA view remain owned.
            unsafe { owner.view.add_WebMessageReceived(&handler, token.as_ptr()) }
        },
        || owner.fail("message subscription panicked"),
    )
}

#[allow(unsafe_code)] // Actual top-view DOMContentLoaded subscription and NavigationId getter.
fn install_dom(owner: &Rc<OriginalBridge>) -> windows::core::Result<()> {
    let view: ICoreWebView2_2 = owner.view.cast()?;
    let weak = Rc::downgrade(owner);
    let sink = FailureSink::new(&owner.session, owner.id);
    let handler = DOMContentLoadedEventHandler::create(Box::new(move |sender, args| {
        let failed = |detail| sink.latch(detail);
        boundary(&failed, || {
            let retained = weak.upgrade();
            let captured = (sender, args);
            let Some(owner) = retained.as_ref() else {
                failed("DOM callback lost its attached bridge owner");
                return Err(E_UNEXPECTED.into());
            };
            let turn = CallbackTurn::enter(owner)?;
            let result = (|| {
                let (sender, args) = captured;
                let sender = sender.as_ref().ok_or(E_POINTER)?;
                let args = args.as_ref().ok_or(E_POINTER)?;
                if canonical_webview_identity(sender)? != owner.identity {
                    return Err(E_INVALIDARG.into());
                }
                let mut id = 0;
                // SAFETY: actual callback args and writable stack output on same STA.
                unsafe { args.NavigationId(&raw mut id) }?;
                let id = std::num::NonZeroU64::new(id).ok_or(E_INVALIDARG)?;
                owner.synchronize_commit().map_err(|_| E_UNEXPECTED)?;
                let nav = owner.navigation.try_borrow().map_err(|_| E_UNEXPECTED)?;
                let mut state = owner.state.try_borrow_mut().map_err(|_| E_UNEXPECTED)?;
                if let Some(startup) = state.startup.as_mut() {
                    startup.observe_dom_content_loaded(nav.document(), id);
                }
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                let observed_startup = state.startup.is_some();
                drop(state);
                drop(nav);
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                if observed_startup {
                    owner.observe_startup("dom_callback", None, Some(id.get()));
                }
                Ok(())
            })();
            if result.is_err() {
                owner.fail("original native callback observation failed");
            }
            drop(turn);
            result
        })
    }));
    owner.dom_registration.add(
        |token| {
            // SAFETY: same attached token and retained view; no Rust guard survives.
            unsafe { view.add_DOMContentLoaded(&handler, token.as_ptr()) }
        },
        || owner.fail("DOM subscription panicked"),
    )
}
