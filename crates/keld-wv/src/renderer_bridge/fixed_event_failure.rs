//! Lossless compact custody of one already-existing fixed Host failure.
//! This representation grants no role, completion, retirement or dispatch right.

use super::{RendererBridgeOperation, RendererBridgeRequest, RendererRoleWitness};
use crate::WebviewId;
use crate::renderer_contract::CheckedNativeDocumentLease;

/// The same owned Host EventQuiesced(false) request after exact shape validation.
/// Fixed fields are represented by this closed type: channel zero, no payload
/// allocation, native facts or numeric token. All original identifiers and both
/// actual authority observations are moved, never cloned or recaptured.
#[derive(Debug)]
pub struct RetainedRendererEventFailure {
    webview: WebviewId,
    navigation: u64,
    request: u32,
    document_lease: CheckedNativeDocumentLease,
    role_witness: RendererRoleWitness,
}

impl RetainedRendererEventFailure {
    /// Original physical view identifier; not a fresh route lookup.
    #[must_use]
    pub const fn webview(&self) -> WebviewId {
        self.webview
    }

    /// Original navigation observation.
    #[must_use]
    pub const fn navigation(&self) -> u64 {
        self.navigation
    }

    /// Original delivery correlation.
    #[must_use]
    pub const fn request(&self) -> u32 {
        self.request
    }

    /// The same retained checked lease, even after its actual revocation.
    #[must_use]
    pub const fn document_lease(&self) -> &CheckedNativeDocumentLease {
        &self.document_lease
    }

    /// The same role allocation, even after its actual stop.
    #[must_use]
    pub const fn role_witness(&self) -> &RendererRoleWitness {
        &self.role_witness
    }
}

impl RendererBridgeRequest {
    /// Move exactly one fixed Host failure into compact, lossless custody.
    ///
    /// Refusal restores the identical request to `input`, including all owned
    /// buffers, observations and tokens. No allocation or source/role lookup is
    /// performed. The caller still owns every final admission and release check.
    #[must_use]
    pub fn take_fixed_event_failure(
        input: &mut Option<Self>,
    ) -> Option<RetainedRendererEventFailure> {
        match input.take()? {
            // Deliberately exhaustive: adding a request field must force this
            // ownership conversion to be reviewed instead of silently dropping it.
            Self {
                webview,
                navigation,
                request,
                channel: 0,
                payload,
                operation: RendererBridgeOperation::EventQuiesced { completed: false },
                document_lease: Some(document_lease),
                native_facts: None,
                role_witness: Some(role_witness),
                numeric_result: None,
            } if payload.capacity() == 0 => Some(RetainedRendererEventFailure {
                webview,
                navigation,
                request,
                document_lease,
                role_witness,
            }),
            request => {
                *input = Some(request);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests;
