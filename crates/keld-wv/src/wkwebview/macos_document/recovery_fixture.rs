//! Debug-only state fixture through the actual native lease owner.
//! The commit callback is modeled; this fixture makes no OS/WebKit proof claim.

use std::num::NonZeroU64;
use std::sync::Arc;

use super::{NativeCommitIdentity, NativeDocumentOwner};
use crate::WebviewId;
use crate::wkwebview::{AdmittedWebAppSource, MacNativeDocumentLease, MacNavigationUrl};

/// Holds the real lease owner alive while Core exercises its role transitions.
/// Only the debug test-hook build exposes this modeled native callback source.
#[derive(Debug)]
pub struct MacNativeDocumentFixture {
    owner: NativeDocumentOwner,
    document: MacNativeDocumentLease,
}

impl MacNativeDocumentFixture {
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
        let document = MacNativeDocumentLease(owner.lease_for(stamp).ok_or("fixture lease")?);
        Ok(Self { owner, document })
    }

    /// Clone the same revocable stamp/source/liveness witness, not a new owner.
    #[must_use]
    pub fn document(&self) -> MacNativeDocumentLease {
        self.document.clone()
    }

    /// Model a matching callback URL with the real source-wide byte account.
    ///
    /// # Errors
    /// Refuses a missing source or exhausted native metadata credit.
    pub fn native_url(&self) -> Result<MacNavigationUrl, &'static str> {
        let source = self
            .owner
            .source_binding()
            .ok_or("fixture source expired")?;
        let text = source.admitted_entry_url("").ok_or("fixture entry URL")?;
        MacNavigationUrl::fixture_capture(&text, &source.retained_credit_account())
            .map_err(|_| "fixture native URL credit")
    }

    /// Model an independently observed native replacement through its real owner.
    pub fn revoke(&mut self) {
        self.owner.unexpected_mainframe_commit();
    }
}
