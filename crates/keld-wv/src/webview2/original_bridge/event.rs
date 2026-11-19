//! Sealed retirement observation for the original native DATA event.

use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::wv_link::WvDispatchCertainty;

/// Host-only proof of retirement of one original DATA event. No public
/// constructor, Clone, deserializer, page field or successor lookup can mint it.
/// This logical terminal never proves disposal of the native body or consumer
/// execution; the original operation retains that physical custody separately.
#[derive(Debug)]
pub struct NativeEventRetirement {
    delivery: u32,
    document: CheckedNativeDocumentLease,
    role: RendererRoleWitness,
    certainty: WvDispatchCertainty,
}
impl NativeEventRetirement {
    /// Original DATA correlation; never a replacement id.
    #[must_use]
    pub const fn delivery(&self) -> u32 {
        self.delivery
    }
    /// The exact lease captured by the already-admitted Core DATA record.
    #[must_use]
    pub const fn document(&self) -> &CheckedNativeDocumentLease {
        &self.document
    }
    /// Retained original caller. Cleanup compares `same_role`, not `is_live`.
    #[must_use]
    pub const fn role(&self) -> &RendererRoleWitness {
        &self.role
    }
    /// Existing `RouteRetired` wire certainty selected by actual native facts.
    #[must_use]
    pub const fn certainty(&self) -> WvDispatchCertainty {
        self.certainty
    }
}
