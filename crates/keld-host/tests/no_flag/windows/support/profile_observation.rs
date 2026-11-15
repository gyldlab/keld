//! Independent five-store observation and completeness oracle.

use super::profile_response::required_profile_state_field;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProfileStateObservation {
    pub(crate) case_name: String,
    pub(crate) nonce: String,
    pub(crate) before: ProfileStorageState,
    pub(crate) after: ProfileStorageState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProfileStorageState {
    cookie: String,
    local_storage: String,
    indexed_db: String,
    cache_storage: String,
    service_worker: String,
}

impl ProfileStorageState {
    pub(crate) fn from_fields(fields: &[(&str, &str)], phase: &str) -> Result<Self, String> {
        let value = |prefix: &str| {
            let name = if prefix.is_empty() {
                phase.to_owned()
            } else {
                format!("{prefix}_{phase}")
            };
            required_profile_state_field(fields, &name).map(str::to_owned)
        };
        Ok(Self {
            cookie: value("cookie")?,
            local_storage: value("")?,
            indexed_db: value("indexed_db")?,
            cache_storage: value("cache")?,
            service_worker: value("worker")?,
        })
    }

    pub(crate) fn values(&self) -> [(&str, &str); 5] {
        [
            ("cookie", &self.cookie),
            ("localStorage", &self.local_storage),
            ("IndexedDB", &self.indexed_db),
            ("CacheStorage", &self.cache_storage),
            ("serviceWorker", &self.service_worker),
        ]
    }

    pub(crate) fn assert_value(&self, expected: &str, case: &str, phase: &str) {
        for (store, value) in self.values() {
            assert_eq!(value, expected, "{case} {store} {phase}");
        }
    }

    pub(crate) fn is_uniform(&self, expected: &str) -> bool {
        self.values().iter().all(|(_, value)| *value == expected)
    }

    pub(crate) fn json(&self) -> Value {
        Value::Object(
            self.values()
                .into_iter()
                .map(|(store, value)| (store.to_owned(), Value::String(value.to_owned())))
                .collect(),
        )
    }
}
