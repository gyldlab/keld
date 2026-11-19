//! Native storage and request construction for original renderer calls.

use crate::WvError;

pub(crate) mod queue;

/// Data-only control construction from the same already-checked retained lease.
/// No parser, native origin or current-role authority is created here.
pub(crate) fn request_for_lease(
    lease: crate::renderer_contract::CheckedNativeDocumentLease,
    sequence: u32,
    operation: crate::renderer_bridge::RendererBridgeOperation,
    payload: Vec<u8>,
) -> crate::renderer_bridge::RendererBridgeRequest {
    let stamp = lease.stamp();
    crate::renderer_bridge::RendererBridgeRequest {
        webview: stamp.view(),
        navigation: u64::from(stamp.observed_generation()),
        request: sequence,
        channel: 0,
        payload,
        operation,
        document_lease: Some(lease),
        native_facts: None,
        role_witness: None,
        #[cfg(windows)]
        numeric_result: None,
        #[cfg(windows)]
        send_input: None,
    }
}

fn bridge_error(detail: impl Into<String>) -> WvError {
    WvError::RendererBridge {
        detail: detail.into(),
    }
}
