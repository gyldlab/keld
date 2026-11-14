//! Native WKNavigation witness for the shared document state owner.
use crate::WebviewId;
pub(crate) use crate::native_document::{
    DocumentFailure, NativeAttempt, NativeCommitIdentity, NativeCommittedStamp,
    NativeDocumentLease, NativeDocumentOwner, ResourceLease, VerifiedSourceId,
};
use objc2::Message;
use std::num::NonZeroU64;

/// The view keeps the actual Objective-C `WKNavigation` object retained from the
/// synchronous UI-owned loadRequest result through finish/fail. The state
/// owner stores only its opaque identity. Holding the object prevents pointer
/// reuse and proves that a later delegate callback matches this exact load.
#[derive(Debug)]
pub(super) struct RetainedNavigationWitness {
    pub(super) view: WebviewId,
    pub(super) attempt: NativeAttempt,
    navigation: objc2::rc::Retained<objc2_web_kit::WKNavigation>,
}

impl RetainedNavigationWitness {
    pub(super) fn from_host_load_result(
        view: WebviewId,
        attempt: NativeAttempt,
        navigation: &objc2_web_kit::WKNavigation,
    ) -> Option<Self> {
        (view == attempt.view()).then(|| Self {
            view,
            attempt,
            navigation: navigation.retain(),
        })
    }

    pub(super) fn matches(
        &self,
        view: WebviewId,
        navigation: &objc2_web_kit::WKNavigation,
    ) -> bool {
        self.view == view && std::ptr::eq(self.navigation.as_ref(), navigation)
    }

    pub(super) fn identity(&self) -> Option<NativeCommitIdentity> {
        // SAFETY: A non-null retained ObjC navigation object is kept alive
        // by this witness through each checked callback. The pointer alone
        // never admits a request without the same view/attempt/source stamp.
        let native = objc2::rc::Retained::as_ptr(&self.navigation) as usize as u64;
        NonZeroU64::new(native).map(NativeCommitIdentity)
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod recovery_fixture;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub use recovery_fixture::NativeDocumentFixture as MacNativeDocumentFixture;
