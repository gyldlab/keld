//! Prepared source locator and nonce; construction has no document effects.

use std::sync::Arc;

use crate::engine::{NativeNavigationRefusal, OriginalEntryQuery};
use crate::resource_intake::{ORIGINAL_WEBAPP_LIMITS, RetainedByteCredit, pending_metadata_charge};
use crate::resource_source::{AdmittedWebAppSource, NativeNavigationUrl};

pub(in super::super) struct PreparedLoad {
    source: Arc<AdmittedWebAppSource>,
    query: OriginalEntryQuery,
    uri: NativeNavigationUrl,
    header: String,
    _credit: RetainedByteCredit,
}

impl PreparedLoad {
    pub(super) fn new(
        source: &Arc<AdmittedWebAppSource>,
        query: &OriginalEntryQuery,
    ) -> Result<Self, NativeNavigationRefusal> {
        let raw_query = query
            .as_optional_str()
            .map_err(|_| NativeNavigationRefusal::InvalidQuery)?;
        // Covers the temporary URI/UTF16 native-request fields, bounded query,
        // header and pending-record allocation. The URI's retained allocation
        // separately uses the shared NativeNavigationUrl owner.
        let pending_charge = pending_metadata_charge(
            2 * ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
            raw_query.map_or(0, str::len),
            std::mem::size_of::<super::HostAttempt>(),
            128,
            ORIGINAL_WEBAPP_LIMITS,
        )
        .map_err(|_| NativeNavigationRefusal::RetainedByteLimit)?;
        let credit = RetainedByteCredit::reserve(&source.retained_credit_account(), pending_charge)
            .map_err(|_| NativeNavigationRefusal::RetainedByteLimit)?;
        let uri = source
            .admitted_entry_url(raw_query)
            .ok_or(NativeNavigationRefusal::NativeUrlRejected)?;
        let uri = NativeNavigationUrl::capture_bounded(
            &uri,
            uri.encode_utf16().count(),
            &source.retained_credit_account(),
        )
        .map_err(|_| NativeNavigationRefusal::NativeUrlRejected)?;
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| NativeNavigationRefusal::Unavailable)?;
        let mut header = String::with_capacity(32);
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in nonce {
            header.push(char::from(HEX[usize::from(byte >> 4)]));
            header.push(char::from(HEX[usize::from(byte & 15)]));
        }
        Ok(Self {
            source: Arc::clone(source),
            query: query.clone(),
            uri,
            header,
            _credit: credit,
        })
    }

    pub(in super::super) fn uri(&self) -> &str {
        self.uri.as_str()
    }

    pub(in super::super) fn header_value(&self) -> &str {
        &self.header
    }

    pub(super) fn belongs_to(&self, source: &Arc<AdmittedWebAppSource>) -> bool {
        Arc::ptr_eq(&self.source, source)
    }

    pub(super) fn query_matches(&self, query: Option<&str>) -> bool {
        self.query.as_optional_str() == Ok(query)
    }
}
