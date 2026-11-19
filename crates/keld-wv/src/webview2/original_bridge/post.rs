//! Prepared CONTROL data and one actual creating-STA publication cut.
//! Core owns the four CONTROL slots. This adapter owns no quota or wire codec.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod entry;

use std::rc::Rc;

use super::super::original_resource::ResourceView;
use crate::native_document::NativeDocumentOwner;
use crate::renderer_bridge::{
    NativeControlReservation, RendererRoleWitness, RouteRetirementPermit,
};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::wv_link::HostRendererMessage;

type EntryCommit<'a> =
    dyn FnMut(&CheckedNativeDocumentLease, &[u8; 32], &NativeDocumentOwner) -> bool + 'a;

/// Only the fixed route-unavailable carrier may use the old Registry's permit.
pub(super) enum PostAuthority {
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
    authority: PostAuthority,
    phase: PostPhase,
    bound_view: Option<Rc<ResourceView>>,
    charge: NativeControlReservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PostPhase {
    Prepared,
    Cancelled,
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
    pub(super) fn prior_attempt_result(self) -> Option<PostResult> {
        match self {
            Self::Prepared => None,
            Self::Cancelled => Some(PostResult::NotAdmitted),
            Self::Posting | Self::OutcomeUnknown => Some(PostResult::OutcomeUnknown),
            Self::Returned => Some(PostResult::AlreadyReturned),
        }
    }

    /// Irreversible no-entry cancellation on this same operation. A later
    /// source/role change cannot turn it back into a prepared publication.
    pub(super) fn cancel_unentered(&mut self) -> bool {
        if matches!(self, Self::Prepared | Self::Cancelled) {
            *self = Self::Cancelled;
            true
        } else {
            false
        }
    }

    /// An actual same Close may settle unknown native custody only outside the
    /// in-call phase. API return and consumer/logical terminal remain separate.
    pub(super) fn physically_quiescent(self, actual_closed: bool) -> bool {
        match self {
            Self::Prepared | Self::Cancelled | Self::Returned => true,
            Self::Posting => false,
            Self::OutcomeUnknown => actual_closed,
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
        authority: PostAuthority,
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
                bound_view: None,
                charge,
            }),
            Err(detail) => Err(PrepareFailure { detail, charge }),
        }
    }

    /// Pure same-operation entry runs against the navigation owner's already
    /// borrowed living document inside the canonical Core retirement fence.
    pub(super) fn post_with_entry(
        &mut self,
        view: &Rc<ResourceView>,
        enter: &mut EntryCommit<'_>,
    ) -> PostResult {
        entry::post_control(self, view, enter)
    }

    /// Move only the SAME reservation after this operation's native completion
    /// or proved never-written cancellation. The original caller keeps any
    /// consumer/startup half live; this method does not acknowledge consumption.
    #[cfg(test)]
    #[expect(
        clippy::result_large_err,
        reason = "Test refusal returns the complete same charged native carrier without allocation or lost custody."
    )]
    pub(super) fn into_charge(self) -> Result<NativeControlReservation, Self> {
        if !self.phase.physically_quiescent(false) {
            return Err(self);
        }
        Ok(self.take_charge())
    }

    /// Actual same-controller Close can settle an outcome-unknown carrier, but
    /// never an in-call borrow. It grants no consumer or logical completion.
    #[expect(
        clippy::result_large_err,
        reason = "Refusal returns the complete charged native carrier without allocation or lost custody."
    )]
    pub(super) fn into_charge_after_close(self) -> Result<NativeControlReservation, Self> {
        let actual_closed = self
            .bound_view
            .as_ref()
            .is_some_and(|view| view.actual_post_controller_closed());
        if !self.phase.physically_quiescent(actual_closed) {
            return Err(self);
        }
        Ok(self.take_charge())
    }

    fn take_charge(self) -> NativeControlReservation {
        let Self {
            wide,
            epoch: _,
            document,
            authority,
            phase: _,
            bound_view,
            charge,
        } = self;
        drop(wide);
        drop(document);
        drop(authority);
        drop(bound_view);
        charge
    }
}

fn prepare_bytes(
    carrier: &HostRendererMessage<'_>,
    epoch: &str,
    authority: &PostAuthority,
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
    if matches!(authority, PostAuthority::RetiredRoute(_))
        && !matches!(carrier, HostRendererMessage::RouteState { ready: false })
    {
        return Err("retirement permit authorizes only route-state ready:false");
    }
    prepare_encoded_bytes(carrier, epoch, |encoded, backing| {
        charge
            .try_extend_storage(encoded, backing)
            .then_some(())
            .ok_or("existing CONTROL storage budget is unavailable")
    })
}

pub(super) fn prepare_encoded_bytes(
    carrier: &HostRendererMessage<'_>,
    epoch: &str,
    pay: impl FnOnce(usize, usize) -> Result<(), &'static str>,
) -> Result<(Vec<u16>, [u8; 32]), &'static str> {
    let encoded = carrier
        .encoded_len(epoch)
        .map_err(|_| "invalid closed native publication carrier")?;
    let epoch: [u8; 32] = epoch
        .as_bytes()
        .try_into()
        .map_err(|_| "invalid native epoch")?;
    let units = encoded
        .checked_add(1)
        .ok_or("native publication storage overflow")?;
    let backing = units
        .checked_mul(2)
        .and_then(|wide| wide.checked_add(encoded))
        .ok_or("native publication storage overflow")?;
    // Add JSON plus worst-case UTF16 capacity to the *actual* existing record.
    // The previous DTO/wire allocations remain charged; no guessed total and
    // no second slot. Every failure leaves the old lease intact.
    pay(encoded, backing)?;
    let bytes = carrier
        .encode(std::str::from_utf8(&epoch).map_err(|_| "invalid native epoch")?)
        .map_err(|_| "native publication serialization failed")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "native publication JSON is not UTF8")?;
    let mut wide = Vec::new();
    wide.try_reserve_exact(units)
        .map_err(|_| "native publication allocation failed")?;
    wide.extend(text.encode_utf16());
    wide.push(0);
    if bytes.capacity() > encoded || wide.capacity() > units || wide.len() > units {
        return Err("native publication backing exceeded its reserved capacity");
    }
    Ok((wide, epoch))
}

#[cfg(test)]
mod tests;
