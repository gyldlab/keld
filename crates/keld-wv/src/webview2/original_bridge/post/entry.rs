//! The sole actual Post boundary; typed owners retain their own storage class.
use super::{PostAuthority, PostPhase, PostResult};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::webview2::guarded_navigation::canonical_webview_identity;
use crate::webview2::original_callbacks::{callback_credit, source_uri};
use crate::webview2::original_navigation::OriginalNavigation;
use std::cell::RefCell;
use std::rc::Rc;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use windows::core::PCWSTR;

/// Borrowed immutable backing/identity for one actual retained native target.
/// Creating this descriptor supplies no native admission or release receipt.
pub(in crate::webview2::original_bridge) struct PostTarget<'a> {
    pub(in crate::webview2::original_bridge) view: &'a ICoreWebView2,
    pub(in crate::webview2::original_bridge) expected_identity: usize,
    pub(in crate::webview2::original_bridge) navigation: &'a Rc<RefCell<OriginalNavigation>>,
    pub(in crate::webview2::original_bridge) wide: &'a [u16],
    pub(in crate::webview2::original_bridge) document: &'a CheckedNativeDocumentLease,
    pub(in crate::webview2::original_bridge) epoch: &'a [u8; 32],
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(in crate::webview2::original_bridge) event_storage:
        Option<crate::webview2::managed_test_control::navigation_post::EventStorage>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(in crate::webview2::original_bridge) carrier:
        crate::webview2::managed_test_control::navigation_post::PostCarrier,
}

/// All existing wrappers and typed carriers share this sole actual native API.
pub(in crate::webview2::original_bridge) fn post_prepared_with_entry(
    phase: &mut PostPhase,
    authority: &PostAuthority,
    target: PostTarget<'_>,
    enter: &mut dyn FnMut(
        &CheckedNativeDocumentLease,
        &[u8; 32],
        &crate::native_document::NativeDocumentOwner,
    ) -> bool,
) -> PostResult {
    post_prepared_inner(phase, authority, target, &mut |_, _| true, enter)
}

#[allow(unsafe_code)] // The sole retained creating-STA PostWebMessageAsString.
#[expect(
    clippy::needless_pass_by_value,
    clippy::single_match_else,
    reason = "Consume the one-call target descriptor and preserve caught-panic custody through the native-return disposition."
)]
#[expect(
    clippy::too_many_lines,
    reason = "The single native Post API keeps source admission, the final lease and role fence, and the immediate call auditable together across all build configurations."
)]
fn post_prepared_inner(
    phase: &mut PostPhase,
    authority: &PostAuthority,
    target: PostTarget<'_>,
    precheck: &mut dyn FnMut(&CheckedNativeDocumentLease, &[u8; 32]) -> bool,
    enter: &mut dyn FnMut(
        &CheckedNativeDocumentLease,
        &[u8; 32],
        &crate::native_document::NativeDocumentOwner,
    ) -> bool,
) -> PostResult {
    let PostTarget {
        view,
        expected_identity,
        navigation,
        wide,
        document,
        epoch,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        event_storage,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        carrier,
    } = target;
    if let Some(result) = phase.prior_attempt_result() {
        return result;
    }
    if !document.is_current() {
        return PostResult::NotAdmitted;
    }
    // Native getters and shared source parsing all precede the final fence.
    // Their temporary source metadata uses its existing owner. Each typed
    // carrier retains its own existing CONTROL/DATA/local-call storage charge.
    let source_ok = (|| {
        let _credit = callback_credit(navigation).ok()?;
        if canonical_webview_identity(view).ok()? != expected_identity {
            return None;
        }
        let uri = source_uri(view).ok()?;
        let source = navigation.try_borrow().ok()?.source().clone();
        // The shared parser performs Windows ordinal comparison. No
        // navigation interior borrow survives that native observation.
        if !document.belongs_to_source(&source) || source.qualify_document_url(&uri).is_err() {
            return None;
        }
        Some(())
    })();
    if source_ok.is_none() {
        return PostResult::NotAdmitted;
    }
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    let mut observation =
        crate::webview2::managed_test_control::navigation_post::PostObservation::prepare(
            navigation,
            document,
            event_storage,
            carrier,
            epoch,
            match authority {
                PostAuthority::Current(role) => Some(role.is_live()),
                PostAuthority::RetiredRoute(_) => None,
            },
        );
    let message = PCWSTR(wide.as_ptr());
    let mut admit = || {
        if *phase != PostPhase::Prepared || !precheck(document, epoch) {
            return false;
        }
        let Ok(mut state) = navigation.try_borrow_mut() else {
            return false;
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(observation) = &mut observation {
            observation.final_gate(state.post_observation());
        }
        if !state.begin_post_with(document, &mut |owner| enter(document, epoch, owner)) {
            return false;
        }
        *phase = PostPhase::Posting;
        true
    };
    let admitted = match authority {
        PostAuthority::Current(role) => role.try_admit_native(&mut admit),
        PostAuthority::RetiredRoute(permit) => {
            permit.try_admit_route_unavailable(document, &mut admit)
        }
    };
    if let Some(result) = phase.finish_admission(admitted) {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(observation) = observation {
            observation.complete(
                if matches!(result, PostResult::NotAdmitted) {
                    "not-admitted"
                } else {
                    "outcome-unknown"
                },
                None,
            );
        }
        return result;
    }
    // SAFETY: retained creating-STA view; bounded immutable NUL-terminated
    // UTF16 backing lives through return. All QI/getters/source parsing ran
    // first. The canonical fence just committed this exact document/epoch
    // and posting sentinel, and all Rust/Core guards are now released.
    // No native getter, callback, allocation, wait or pump intervenes here.
    // S_OK proves API acceptance only. Same-thread navigation reentry is a
    // qualification failure even if that native call eventually returns OK.
    // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2#postwebmessageasstring
    let returned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(observation) = &mut observation {
            observation.api_enter();
        }
        // SAFETY: the unchanged proof above applies to this same sole actual API.
        let result = unsafe { view.PostWebMessageAsString(message) };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(observation) = &mut observation {
            observation.api_return();
        }
        result
    }));
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    if returned.is_err()
        && let Some(observation) = &mut observation
    {
        observation.api_unwound();
    }
    let finished = navigation
        .try_borrow_mut()
        .is_ok_and(|mut state| state.finish_post());
    match (returned, finished) {
        (Ok(result), true) => {
            *phase = PostPhase::Returned;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if let Some(observation) = observation {
                observation.complete("returned", Some(result.is_ok()));
            }
            PostResult::Returned(result)
        }
        _ => {
            *phase = PostPhase::OutcomeUnknown;
            if let Ok(mut state) = navigation.try_borrow_mut() {
                state.destroy();
            }
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if let Some(observation) = observation {
                observation.complete("outcome-unknown", None);
            }
            PostResult::OutcomeUnknown
        }
    }
}
