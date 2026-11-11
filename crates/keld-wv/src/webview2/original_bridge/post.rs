//! Prepared CONTROL data and one actual creating-STA publication cut.
//! Core owns the four CONTROL slots. This adapter owns no quota or wire codec.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::RefCell;
use std::rc::Rc;

use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use windows::core::PCWSTR;

use super::super::guarded_navigation::canonical_webview_identity;
use super::super::original_callbacks::{callback_credit, source_uri};
use super::super::original_navigation::OriginalNavigation;
use crate::renderer_bridge::{
    NativeControlReservation, RendererRoleWitness, RouteRetirementPermit,
};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::wv_link::HostRendererMessage;

/// Only the fixed route-unavailable carrier may use the old Registry's permit.
pub(super) enum ControlAuthority {
    Current(RendererRoleWitness),
    RetiredRoute(RouteRetirementPermit),
}

/// No native entry occurred on preparation failure; keep the original charge.
pub(super) struct PrepareFailure {
    pub(super) detail: &'static str,
    pub(super) charge: NativeControlReservation,
}

/// Field order releases all owned backing storage before its accounting lease.
/// The owning UI operation MUST retain this entire value after outcome-unknown.
#[must_use = "native CONTROL custody survives posting until its actual terminal receipt"]
pub(super) struct PreparedControl {
    wide: Vec<u16>,
    epoch: [u8; 32],
    document: CheckedNativeDocumentLease,
    authority: ControlAuthority,
    phase: PostPhase,
    charge: NativeControlReservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostPhase {
    Prepared,
    Posting,
    Returned,
    OutcomeUnknown,
}

#[derive(Debug)]
pub(super) enum PostResult {
    /// No API entry; keep the pending original operation or cancel it explicitly.
    NotAdmitted,
    /// The API returned; this never establishes the separate consumer receipt.
    Returned(windows::core::Result<()>),
    /// An earlier invocation returned; no API retry and no new result is implied.
    AlreadyReturned,
    /// Reentry/panic/owner failure: retain all custody and fail qualification.
    OutcomeUnknown,
}

impl PostPhase {
    fn prior_attempt_result(self) -> Option<PostResult> {
        match self {
            Self::Prepared => None,
            Self::Posting | Self::OutcomeUnknown => Some(PostResult::OutcomeUnknown),
            Self::Returned => Some(PostResult::AlreadyReturned),
        }
    }

    fn finish_admission(&mut self, admitted: bool) -> Option<PostResult> {
        match (admitted, *self) {
            (true, Self::Posting) => None,
            (false, Self::Prepared) => Some(PostResult::NotAdmitted),
            _ => {
                *self = Self::OutcomeUnknown;
                Some(PostResult::OutcomeUnknown)
            }
        }
    }
}

impl PreparedControl {
    pub(super) fn prepare(
        carrier: &HostRendererMessage<'_>,
        epoch: &str,
        document: CheckedNativeDocumentLease,
        authority: ControlAuthority,
        charge: NativeControlReservation,
    ) -> Result<Self, PrepareFailure> {
        let prepared = prepare_bytes(carrier, epoch, &authority, &charge);
        match prepared {
            Ok((wide, epoch)) => Ok(Self {
                wide,
                epoch,
                document,
                authority,
                phase: PostPhase::Prepared,
                charge,
            }),
            Err(detail) => Err(PrepareFailure { detail, charge }),
        }
    }

    /// The caller supplies its exact retained view/navigation binding, not a
    /// current-window lookup. The final predicate validates the live epoch in
    /// the UI owner's existing state; it may only inspect local state and must
    /// release every interior borrow before returning.
    #[allow(unsafe_code)] // One reviewed PostWebMessageAsString on the retained STA view.
    pub(super) fn post(
        &mut self,
        view: &ICoreWebView2,
        expected_identity: usize,
        navigation: &Rc<RefCell<OriginalNavigation>>,
        epoch_current: &mut dyn FnMut(&CheckedNativeDocumentLease, &[u8; 32]) -> bool,
    ) -> PostResult {
        if let Some(result) = self.phase.prior_attempt_result() {
            return result;
        }
        if !self.document.is_current() {
            return PostResult::NotAdmitted;
        }
        // Native getters and shared source parsing all precede the final fence.
        // Their temporary source metadata uses its existing owner. CONTROL
        // payload/UTF16 backing remains exclusively on the Core CONTROL slot.
        let source_ok = (|| {
            let _credit = callback_credit(navigation).ok()?;
            if canonical_webview_identity(view).ok()? != expected_identity {
                return None;
            }
            let uri = source_uri(view).ok()?;
            let source = navigation.try_borrow().ok()?.source().clone();
            // The shared parser performs Windows ordinal comparison. No
            // navigation interior borrow survives that native observation.
            if !self.document.belongs_to_source(&source)
                || source.qualify_document_url(&uri).is_err()
            {
                return None;
            }
            Some(())
        })();
        if source_ok.is_none() {
            return PostResult::NotAdmitted;
        }
        let message = PCWSTR(self.wide.as_ptr());
        let document = &self.document;
        let epoch = &self.epoch;
        let phase = &mut self.phase;
        let mut admit = || {
            if *phase != PostPhase::Prepared || !epoch_current(document, epoch) {
                return false;
            }
            let Ok(mut state) = navigation.try_borrow_mut() else {
                return false;
            };
            if !state.begin_post(document) {
                return false;
            }
            *phase = PostPhase::Posting;
            true
        };
        let admitted = match &self.authority {
            ControlAuthority::Current(role) => role.try_admit_native(&mut admit),
            ControlAuthority::RetiredRoute(permit) => {
                permit.try_admit_route_unavailable(document, &mut admit)
            }
        };
        if let Some(result) = self.phase.finish_admission(admitted) {
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
        let returned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            view.PostWebMessageAsString(message)
        }));
        let finished = navigation
            .try_borrow_mut()
            .is_ok_and(|mut state| state.finish_post());
        match (returned, finished) {
            (Ok(result), true) => {
                self.phase = PostPhase::Returned;
                PostResult::Returned(result)
            }
            _ => {
                self.phase = PostPhase::OutcomeUnknown;
                if let Ok(mut state) = navigation.try_borrow_mut() {
                    state.destroy();
                }
                PostResult::OutcomeUnknown
            }
        }
    }

    /// Move only the SAME reservation after this operation's native completion
    /// or proved never-written cancellation. The original caller keeps any
    /// consumer/startup half live; this method does not acknowledge consumption.
    pub(super) fn into_charge(self) -> Result<NativeControlReservation, Self> {
        if matches!(self.phase, PostPhase::Posting | PostPhase::OutcomeUnknown) {
            return Err(self);
        }
        let Self {
            wide,
            epoch: _,
            document,
            authority,
            phase: _,
            charge,
        } = self;
        drop(wide);
        drop(document);
        drop(authority);
        Ok(charge)
    }
}

fn prepare_bytes(
    carrier: &HostRendererMessage<'_>,
    epoch: &str,
    authority: &ControlAuthority,
    charge: &NativeControlReservation,
) -> Result<(Vec<u16>, [u8; 32]), &'static str> {
    match carrier {
        HostRendererMessage::Bound { .. }
        | HostRendererMessage::SendResult { .. }
        | HostRendererMessage::RouteState { .. }
        | HostRendererMessage::AppRegistered
        | HostRendererMessage::AppRefused { .. }
        | HostRendererMessage::Error { .. } => {}
        _ => return Err("DATA cannot consume a CONTROL reservation"),
    }
    if matches!(authority, ControlAuthority::RetiredRoute(_))
        && !matches!(carrier, HostRendererMessage::RouteState { ready: false })
    {
        return Err("retirement permit authorizes only route-state ready:false");
    }
    let encoded = carrier
        .encoded_len(epoch)
        .map_err(|_| "invalid closed CONTROL carrier")?;
    let epoch: [u8; 32] = epoch
        .as_bytes()
        .try_into()
        .map_err(|_| "invalid native epoch")?;
    let units = encoded
        .checked_add(1)
        .ok_or("native CONTROL storage overflow")?;
    let backing = units
        .checked_mul(2)
        .and_then(|wide| wide.checked_add(encoded))
        .ok_or("native CONTROL storage overflow")?;
    // Add JSON plus worst-case UTF16 capacity to the *actual* existing record.
    // The previous DTO/wire allocations remain charged; no guessed total and
    // no second slot. Every failure leaves the old lease intact.
    if !charge.try_extend_storage(encoded, backing) {
        return Err("existing CONTROL storage budget is unavailable");
    }
    let bytes = carrier
        .encode(std::str::from_utf8(&epoch).map_err(|_| "invalid native epoch")?)
        .map_err(|_| "CONTROL serialization failed")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "CONTROL JSON is not UTF8")?;
    let mut wide = Vec::new();
    wide.try_reserve_exact(units)
        .map_err(|_| "native CONTROL allocation failed")?;
    wide.extend(text.encode_utf16());
    wide.push(0);
    if bytes.capacity() > encoded || wide.capacity() > units || wide.len() > units {
        return Err("native CONTROL backing exceeded its reserved capacity");
    }
    Ok((wide, epoch))
}

#[cfg(test)]
mod tests;
