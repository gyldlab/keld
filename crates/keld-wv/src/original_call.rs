//! Shared original-preload local-call policy and bounded retention account.
//! Both native carriers use the same limits and exact action classification.
//! A derived channel is still data; only Core's current role and Guard grant admit it.

use crate::WvError;
use crate::wv_link::AppPageCall;

pub(crate) mod active;
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
    }
}

fn bridge_error(detail: impl Into<String>) -> WvError {
    WvError::RendererBridge {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod postbind_queue_limits {
    use super::{LocalCallBudget, MAX_LOCAL_CALL_BYTES, MAX_LOCAL_CALL_RECORDS};

    #[test]
    fn fixed_storage_reservation_uses_the_same_cap_without_a_call_record() {
        let mut budget = LocalCallBudget::with_fixed_storage(48).expect("one cold backing");
        assert_eq!(
            (budget.retained_records(), budget.retained_bytes()),
            (0, 48)
        );
        assert!(!budget.is_empty());
        assert!(LocalCallBudget::with_fixed_storage(MAX_LOCAL_CALL_BYTES + 1).is_err());
        assert!(LocalCallBudget::with_fixed_storage(usize::MAX).is_err());
        budget
            .reserve(MAX_LOCAL_CALL_BYTES - 48)
            .expect("exact shared cap");
        assert!(budget.reserve(1).is_err());
        assert_eq!(
            (budget.retained_records(), budget.retained_bytes()),
            (1, MAX_LOCAL_CALL_BYTES)
        );
        budget
            .release(MAX_LOCAL_CALL_BYTES - 48)
            .expect("actual call dropped");
        assert_eq!(
            (budget.retained_records(), budget.retained_bytes()),
            (0, 48)
        );
        assert!(!budget.is_empty());
        assert!(
            LocalCallBudget::default().is_empty(),
            "existing default semantics unchanged"
        );
    }

    #[test]
    fn observations_require_every_reserved_record_to_release() {
        let mut budget = LocalCallBudget::default();
        assert!(budget.is_empty());
        budget
            .reserve(0)
            .expect("zero bytes still retain one record");
        assert!(!budget.is_empty());
        assert_eq!((budget.retained_records(), budget.retained_bytes()), (1, 0));
        budget.reserve(7).expect("second retained record");
        budget.release(0).expect("first actual record release");
        assert!(!budget.is_empty());
        assert_eq!((budget.retained_records(), budget.retained_bytes()), (1, 7));
        assert!(budget.release(8).is_err());
        assert_eq!((budget.retained_records(), budget.retained_bytes()), (1, 7));
        budget.release(7).expect("last actual record release");
        assert!(budget.is_empty());
        assert_eq!((budget.retained_records(), budget.retained_bytes()), (0, 0));
        assert!(budget.release(0).is_err());
        assert!(budget.is_empty());
    }

    #[test]
    fn postbind_record_max_plus_one_refuses_without_changing_custody() {
        let mut budget = LocalCallBudget::default();
        for _ in 0..MAX_LOCAL_CALL_RECORDS {
            budget.reserve(1).expect("within bounded record census");
        }
        assert!(budget.reserve(1).is_err());
        assert_eq!((budget.records, budget.bytes), (64, 64));
        budget.release(1).expect("one actual record release");
        budget
            .reserve(1)
            .expect("released capacity permits successor");
        for _ in 0..MAX_LOCAL_CALL_RECORDS {
            budget
                .release(1)
                .expect("each original record released once");
        }
        assert_eq!((budget.records, budget.bytes), (0, 0));
        assert!(budget.release(1).is_err());
        assert_eq!((budget.records, budget.bytes), (0, 0));
    }

    #[test]
    fn postbind_byte_max_plus_one_and_overflow_refuse_atomically() {
        let mut budget = LocalCallBudget::default();
        assert!(budget.reserve(MAX_LOCAL_CALL_BYTES + 1).is_err());
        assert_eq!((budget.records, budget.bytes), (0, 0));
        budget
            .reserve(MAX_LOCAL_CALL_BYTES)
            .expect("exact byte maximum");
        assert!(budget.reserve(1).is_err());
        assert!(budget.reserve(usize::MAX).is_err());
        assert_eq!((budget.records, budget.bytes), (1, MAX_LOCAL_CALL_BYTES));
        assert!(budget.release(MAX_LOCAL_CALL_BYTES + 1).is_err());
        assert_eq!((budget.records, budget.bytes), (1, MAX_LOCAL_CALL_BYTES));
        budget
            .release(MAX_LOCAL_CALL_BYTES)
            .expect("actual charged bytes released");
        assert_eq!((budget.records, budget.bytes), (0, 0));
    }
}
