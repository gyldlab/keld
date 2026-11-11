//! Shared original-preload local-call policy and bounded retention account.
//! Both native carriers use the same limits and exact action classification.
//! A derived channel is still data; only Core's current role and Guard grant admit it.

use crate::WvError;
use crate::wv_link::AppPageCall;

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
fn bridge_error(detail: impl Into<String>) -> WvError {
    WvError::RendererBridge {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod postbind_queue_limits {
    use super::{LocalCallBudget, MAX_LOCAL_CALL_BYTES, MAX_LOCAL_CALL_RECORDS};

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
