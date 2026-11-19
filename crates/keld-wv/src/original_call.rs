//! Native storage and request construction for original renderer calls.

use crate::WvError;

pub(crate) mod queue;

const MAX_LOCAL_CALL_RECORDS: usize = 64;
const MAX_LOCAL_CALL_BYTES: usize = 524_288;

pub(crate) fn original_call_channel(call: &AppPageCall) -> Result<Option<String>, WvError> {
    let channel = match call.method.as_str() {
        "request" => Some(String::from("rendererReq")),
        "sendMessage" => {
            let topic = call
                .args
                .first()
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| bridge_error("sendMessage needs a copied literal action"))?;
            if topic.len() > 256 {
                return Err(bridge_error("named action exceeds 256 bytes"));
            }
            Some(topic.to_owned())
        }
        _ => None,
    };
    Ok(channel)
}

#[derive(Default)]
pub(crate) struct LocalCallBudget {
    records: usize,
    bytes: usize,
}
impl LocalCallBudget {
    /// Reserve the one cold backing in this same byte counter without taking a
    /// call record. `Default/is_empty` and ordinary reserve/release stay unchanged.
    pub(crate) fn with_fixed_storage(bytes: usize) -> Result<Self, WvError> {
        if bytes > MAX_LOCAL_CALL_BYTES {
            return Err(bridge_error(
                "original call fixed storage exceeds its existing byte limit",
            ));
        }
        Ok(Self { records: 0, bytes })
    }

    /// Cold retained allocation in this same byte account. No active record
    /// may use this operation, and it never takes or releases a call slot.
    pub(crate) fn reserve_fixed_storage(&mut self, bytes: usize) -> Result<(), WvError> {
        if self.records != 0 {
            return Err(bridge_error(
                "fixed call storage cannot change with active records",
            ));
        }
        let total = self
            .bytes
            .checked_add(bytes)
            .filter(|total| *total <= MAX_LOCAL_CALL_BYTES)
            .ok_or_else(|| {
                bridge_error("original fixed storage exceeds the existing byte limit")
            })?;
        self.bytes = total;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) const fn is_empty(&self) -> bool {
        self.records == 0 && self.bytes == 0
    }
    pub(crate) const fn retained_records(&self) -> usize {
        self.records
    }
    pub(crate) const fn retained_bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn reserve(&mut self, bytes: usize) -> Result<(), WvError> {
        let records = self
            .records
            .checked_add(1)
            .filter(|value| *value <= MAX_LOCAL_CALL_RECORDS)
            .ok_or_else(|| bridge_error("post-bind local call record limit (64) exhausted"))?;
        let total = self
            .bytes
            .checked_add(bytes)
            .filter(|value| *value <= MAX_LOCAL_CALL_BYTES)
            .ok_or_else(|| bridge_error("post-bind local call byte limit (524288) exhausted"))?;
        self.records = records;
        self.bytes = total;
        Ok(())
    }
    /// Extend a retained input record before its next allocation. Only its
    /// private token owner selects this operation; no new record is created.
    pub(crate) fn grow_record_storage(&mut self, additional: usize) -> Result<(), WvError> {
        if self.records == 0 {
            return Err(bridge_error("input storage growth lacks a retained record"));
        }
        let bytes = self
            .bytes
            .checked_add(additional)
            .filter(|bytes| *bytes <= MAX_LOCAL_CALL_BYTES)
            .ok_or_else(|| bridge_error("post-bind local call byte limit (524288) exhausted"))?;
        self.bytes = bytes;
        Ok(())
    }

    pub(crate) fn remaining_storage(&self) -> usize {
        MAX_LOCAL_CALL_BYTES.saturating_sub(self.bytes)
    }

    pub(crate) fn release_record_storage(&mut self, bytes: usize) -> Result<(), WvError> {
        if self.records == 0 {
            return Err(bridge_error(
                "input storage release lacks its retained record",
            ));
        }
        let retained = self
            .bytes
            .checked_sub(bytes)
            .ok_or_else(|| bridge_error("input storage release underflow"))?;
        self.bytes = retained;
        Ok(())
    }

    pub(crate) fn release(&mut self, bytes: usize) -> Result<(), WvError> {
        let records = self
            .records
            .checked_sub(1)
            .ok_or_else(|| bridge_error("local call record release underflow"))?;
        let total = self
            .bytes
            .checked_sub(bytes)
            .ok_or_else(|| bridge_error("local call byte release underflow"))?;
        self.records = records;
        self.bytes = total;
        Ok(())
    }
}
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
