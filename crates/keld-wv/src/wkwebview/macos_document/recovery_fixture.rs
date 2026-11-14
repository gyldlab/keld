//! Debug-only state fixture through the actual native lease owner.
//! The commit callback is modeled; this fixture makes no native OS proof claim.
//! Both platform modules include this single body under the existing test gate.

use std::num::NonZeroU64;
use std::sync::Arc;

use crate::WebviewId;
use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::resource_source::{AdmittedWebAppSource, NativeNavigationUrl};

/// Holds the real lease owner alive while Core exercises its role transitions.
/// Only the debug test-hook build exposes this modeled native callback source.
#[derive(Debug)]
pub struct NativeDocumentFixture {
    owner: NativeDocumentOwner,
    document: CheckedNativeDocumentLease,
}

impl NativeDocumentFixture {
    /// Bind an already-admitted source and model one matching native commit.
    ///
    /// # Errors
    /// Refuses any failed native state-machine transition or lease capture.
    pub fn new(view: WebviewId, source: &Arc<AdmittedWebAppSource>) -> Result<Self, &'static str> {
        let mut owner = NativeDocumentOwner::new(view);
        owner
            .bind_source(source)
            .map_err(|_| "fixture source binding")?;
        let attempt = owner
            .begin_host_load(source.source_id())
            .map_err(|_| "fixture load")?;
        let stamp = owner
            .commit_admitted(
                attempt,
                source.source_id(),
                NativeCommitIdentity(NonZeroU64::MIN),
            )
            .map_err(|_| "fixture commit")?;
        let document = CheckedNativeDocumentLease(owner.lease_for(stamp).ok_or("fixture lease")?);
        Ok(Self { owner, document })
    }

    /// Clone the same revocable stamp/source/liveness witness, not a new owner.
    #[must_use]
    pub fn document(&self) -> CheckedNativeDocumentLease {
        self.document.clone()
    }

    /// Model a matching callback URL with the real source-wide byte account.
    ///
    /// # Errors
    /// Refuses a missing source or exhausted native metadata credit.
    pub fn native_url(&self) -> Result<NativeNavigationUrl, &'static str> {
        let source = self
            .owner
            .source_binding()
            .ok_or("fixture source expired")?;
        #[cfg(target_os = "macos")]
        let text = source.admitted_entry_url("").ok_or("fixture entry URL")?;
        #[cfg(windows)]
        let text = source.admitted_entry_url(None).ok_or("fixture entry URL")?;
        NativeNavigationUrl::fixture_capture(&text, &source.retained_credit_account())
            .map_err(|_| "fixture native URL credit")
    }

    /// Model a Host completion from this SAME fixture lease and supplied actual
    /// role allocation. This debug/profile fixture is not native execution proof.
    #[cfg(windows)]
    #[must_use]
    pub fn event_quiescence_request(
        &self,
        sequence: u32,
        role: crate::renderer_bridge::RendererRoleWitness,
        completed: bool,
    ) -> crate::renderer_bridge::RendererBridgeRequest {
        let mut request = crate::original_call::request_for_lease(
            self.document(),
            sequence,
            crate::renderer_bridge::RendererBridgeOperation::EventQuiesced { completed },
            Vec::new(),
        );
        request.role_witness = Some(role);
        request
    }

    /// Model a consumer receipt against this same debug/profile fixture lease.
    #[cfg(windows)]
    #[must_use]
    pub fn event_consumer_request(
        &self,
        sequence: u32,
        role: crate::renderer_bridge::RendererRoleWitness,
        disposition: crate::wv_link::WvConsumerDisposition,
    ) -> crate::renderer_bridge::RendererBridgeRequest {
        let mut request = crate::original_call::request_for_lease(
            self.document(),
            sequence,
            crate::renderer_bridge::RendererBridgeOperation::Consumed { disposition },
            Vec::new(),
        );
        request.role_witness = Some(role);
        request
    }

    /// Model only an original preflight or Send from this same fixture lease.
    /// The actual commit/callback is modeled; this is not native execution proof.
    ///
    /// # Errors
    /// Refuses another operation, expired source, or failed same-source URL credit.
    #[cfg(windows)]
    pub fn original_call_request(
        &self,
        sequence: u32,
        role: crate::renderer_bridge::RendererRoleWitness,
        operation: crate::renderer_bridge::RendererBridgeOperation,
        payload: Vec<u8>,
    ) -> Result<crate::renderer_bridge::RendererBridgeRequest, &'static str> {
        use crate::renderer_bridge::{NativeMessageFacts, RendererBridgeOperation};
        use crate::resource_intake::{MAX_PENDING_METADATA_BYTES, RetainedByteCredit};

        if !matches!(
            &operation,
            RendererBridgeOperation::Preflight { .. } | RendererBridgeOperation::Send { .. }
        ) {
            return Err("fixture operation is not original preflight or Send");
        }
        let source = self
            .owner
            .source_binding()
            .ok_or("fixture source expired")?;
        let scratch = RetainedByteCredit::reserve(
            &source.retained_credit_account(),
            MAX_PENDING_METADATA_BYTES,
        )
        .map_err(|_| "fixture native metadata credit")?;
        let facts = NativeMessageFacts::from_windows_source_url(
            self.native_url()?,
            &source,
            &self.document,
            &scratch,
        )?;
        let mut request =
            crate::original_call::request_for_lease(self.document(), sequence, operation, payload);
        request.native_facts = Some(facts);
        request.role_witness = Some(role);
        Ok(request)
    }

    /// Model an independently observed native replacement through its real owner.
    pub fn revoke(&mut self) {
        self.owner.unexpected_mainframe_commit();
    }
}
