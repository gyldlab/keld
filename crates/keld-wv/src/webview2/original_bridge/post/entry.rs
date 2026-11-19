//! The sole WebView2 post entry. The typed carrier retains its storage charge.
#![deny(unsafe_op_in_unsafe_fn)]

use super::{EntryCommit, PostAuthority, PostPhase, PostResult, PreparedControl};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::webview2::guarded_navigation::canonical_webview_identity;
use crate::webview2::original_callbacks::{callback_credit, source_uri};
use crate::webview2::original_resource::ResourceView;
use std::rc::Rc;
use windows::core::PCWSTR;

/// Recheck the native source before entering the Core posting fence.
fn source_is_qualified(
    resource: &ResourceView,
    document: &CheckedNativeDocumentLease,
) -> Option<()> {
    let (view, expected_identity, navigation) = resource.original_post_target();
    let _credit = callback_credit(navigation).ok()?;
    if canonical_webview_identity(view).ok()? != expected_identity {
        return None;
    }
    let uri = source_uri(view).ok()?;
    let source = navigation.try_borrow().ok()?.source().clone();
    // The same navigation owner is rechecked after this native source comparison.
    if !document.belongs_to_source(&source) {
        return None;
    }
    source.qualify_document_url(&uri).ok()?;
    Some(())
}

#[allow(unsafe_code)] // The sole retained creating-STA PostWebMessageAsString.
pub(super) fn post_control(
    prepared: &mut PreparedControl,
    resource: &Rc<ResourceView>,
    enter: &mut EntryCommit<'_>,
) -> PostResult {
    let PreparedControl {
        wide,
        epoch,
        document,
        authority,
        phase,
        bound_view,
        ..
    } = prepared;
    if let Some(result) = phase.prior_attempt_result() {
        return result;
    }
    let (view, _, navigation) = resource.original_post_target();
    if bound_view.is_some()
        || !document.is_current()
        || source_is_qualified(resource, document).is_none()
    {
        return PostResult::NotAdmitted;
    }
    let message = PCWSTR(wide.as_ptr());
    let mut admit = || {
        if *phase != PostPhase::Prepared {
            return false;
        }
        let Ok(mut owner) = navigation.try_borrow_mut() else {
            return false;
        };
        if !resource.holds_live_load_owner(&owner)
            || !owner.begin_post_with(document, &mut |owner| enter(document, epoch, owner))
        {
            return false;
        }
        // The only permitted storage move is a clone of the same already retained view.
        *bound_view = Some(Rc::clone(resource));
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
        return result;
    }

    let returned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: this is the COM View and navigation from one retained binding.
        // PreparedControl owns immutable charged NUL UTF-16 through native return.
        // The same document and epoch just entered Posting under the Core fence.
        // Its Rust guards are gone. This is the first native operation after admission.
        // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2#postwebmessageasstring
        unsafe { view.PostWebMessageAsString(message) }
    }));
    let finished = navigation
        .try_borrow_mut()
        .is_ok_and(|mut owner| owner.finish_post());
    match (returned, finished) {
        (Ok(result), true) => {
            *phase = PostPhase::Returned;
            PostResult::Returned(result)
        }
        _ => {
            *phase = PostPhase::OutcomeUnknown;
            if let Ok(mut owner) = navigation.try_borrow_mut() {
                owner.destroy();
            }
            PostResult::OutcomeUnknown
        }
    }
}
