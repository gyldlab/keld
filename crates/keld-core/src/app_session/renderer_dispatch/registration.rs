//! Identity-only renderer registration on the same current physical Window.

use super::super::{
    HostAppError, PrimaryCallerContext, PrimaryLinkIdentity, PrimaryRouterHandle, app_detail,
    with_primary_caller_locked,
};
use keld_runtime::primary::RoleOwner;
use keld_wv::renderer_bridge::{RendererBridgeOperation, RendererBridgeRequest};
use std::sync::Arc;

impl PrimaryRouterHandle {
    /// Return once the original registration is stored or its exact owner no
    /// longer qualifies. No page or Bun result follows an identity notice.
    pub(super) fn observe_registered_renderer(
        &self,
        request: &RendererBridgeRequest,
    ) -> Result<(), HostAppError> {
        if !matches!(request.operation(), RendererBridgeOperation::Registered) {
            return Err(app_detail(
                "renderer dispatch",
                "application renderer delivery is unavailable",
            ));
        }
        let document = request.document_lease().ok_or_else(|| {
            app_detail("renderer registration", "checked document lease is absent")
        })?;
        let role = request.role_witness().ok_or_else(|| {
            app_detail(
                "renderer registration",
                "original Runtime witness is absent",
            )
        })?;
        let stamp = document.stamp();
        if request.request() != 0
            || request.channel() != 0
            || !request.payload().is_empty()
            || request.url().is_some()
            || request.webview() != stamp.view()
            || request.navigation() != u64::from(stamp.observed_generation())
        {
            return Err(app_detail(
                "renderer registration",
                "invalid identity-only notice",
            ));
        }
        let caller = self
            .current
            .lock()
            .map_err(|_| {
                app_detail(
                    "renderer registration",
                    "current Runtime owner lock poisoned",
                )
            })?
            .as_ref()
            .filter(|active| {
                matches!(active.identity, PrimaryLinkIdentity::Role(principal)
                if principal.owner() == RoleOwner::Primary)
                    && role.matches(&active.reader_stop)
            })
            .map(|active| PrimaryCallerContext {
                attempt: active.attempt,
                identity: active.identity,
                reader_stop: Arc::clone(&active.reader_stop),
            });
        let Some(caller) = caller else {
            return Ok(()); // the original Runtime owner is already retired
        };
        with_primary_caller_locked(&self.current, &self.shutdown, &caller, |_| {
            let Some(windows) = &self.windows else {
                return Err(app_detail(
                    "renderer registration",
                    "Window owner is unavailable",
                ));
            };
            let mut windows = windows.lock().map_err(|_| {
                app_detail("renderer registration", "Window Registry lock poisoned")
            })?;
            windows
                .note_native_registration(&caller, request.webview(), document)
                .map_err(|error| app_detail("renderer registration provenance", error.to_string()))
        })
    }
}
