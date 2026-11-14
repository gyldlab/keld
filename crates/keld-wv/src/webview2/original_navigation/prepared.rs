//! Prepared source locator and nonce; construction has no document effects.

use std::alloc::Layout;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crate::engine::{NativeNavigationRefusal, OriginalEntryQuery};
use crate::resource_intake::{ORIGINAL_WEBAPP_LIMITS, RetainedByteCredit, pending_metadata_charge};
use crate::resource_source::{AdmittedWebAppSource, NativeNavigationUrl};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeCall {
    Prepared,
    Admitted,
    InCall,
    ReturnedOk,
    ReturnedErr,
    Unproved,
}

/// The same preparation and its charge are retained by navigation and native input.
pub(in super::super) struct PreparedLoad {
    source: Arc<AdmittedWebAppSource>,
    query: OriginalEntryQuery,
    uri: NativeNavigationUrl,
    header: String,
    native_call: Cell<NativeCall>,
    _credit: RetainedByteCredit,
}

impl PreparedLoad {
    pub(super) fn new(
        source: &Arc<AdmittedWebAppSource>,
        query: &OriginalEntryQuery,
    ) -> Result<Rc<Self>, NativeNavigationRefusal> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let raw_query = query
            .as_optional_str()
            .map_err(|_| NativeNavigationRefusal::InvalidQuery)?;
        // Covers the temporary URI/UTF16 native-request fields, bounded query,
        // header and pending-record allocation. The URI's retained allocation
        // separately uses the shared NativeNavigationUrl owner.
        // Rc's strong/weak words and this exact payload are requested once.
        // This is a source-account charge, not a new queue or capacity limit.
        let (layout, _) = Layout::new::<[usize; 2]>()
            .extend(Layout::new::<Self>())
            .map_err(|_| NativeNavigationRefusal::RetainedByteLimit)?;
        let metadata = std::mem::size_of::<super::HostAttempt>()
            .checked_add(layout.pad_to_align().size())
            .ok_or(NativeNavigationRefusal::RetainedByteLimit)?;
        let pending_charge = pending_metadata_charge(
            2 * ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
            raw_query.map_or(0, str::len),
            metadata,
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
        for byte in nonce {
            header.push(char::from(HEX[usize::from(byte >> 4)]));
            header.push(char::from(HEX[usize::from(byte & 15)]));
        }
        Ok(Rc::new(Self {
            source: Arc::clone(source),
            query: query.clone(),
            uri,
            header,
            native_call: Cell::new(NativeCall::Prepared),
            _credit: credit,
        }))
    }

    pub(in super::super) fn is_prepared(&self) -> bool {
        self.native_call.get() == NativeCall::Prepared
    }

    /// This final local transition runs under the actual Core admission fence.
    pub(in super::super) fn admit_native_call(&self) -> bool {
        if !self.is_prepared() {
            return false;
        }
        self.native_call.set(NativeCall::Admitted);
        true
    }

    /// The actual retained invocation consumes admission once before native entry.
    pub(in super::super) fn begin_native_call(&self) -> bool {
        if self.native_call.get() != NativeCall::Admitted {
            return false;
        }
        self.native_call.set(NativeCall::InCall);
        true
    }

    /// Only the retained actual call reports its physical return, before cleanup.
    pub(in super::super) fn native_returned(&self, success: bool) -> bool {
        if self.native_call.get() != NativeCall::InCall {
            self.native_call.set(NativeCall::Unproved);
            return false;
        }
        self.native_call.set(if success {
            NativeCall::ReturnedOk
        } else {
            NativeCall::ReturnedErr
        });
        true
    }

    pub(in super::super) fn native_unwound(&self) {
        self.native_call.set(NativeCall::Unproved);
    }

    pub(in super::super) fn native_succeeded(&self) -> bool {
        self.native_call.get() == NativeCall::ReturnedOk
    }

    pub(in super::super) fn native_quiesced(&self) -> bool {
        matches!(
            self.native_call.get(),
            NativeCall::Prepared | NativeCall::ReturnedOk | NativeCall::ReturnedErr
        )
    }

    pub(in super::super) fn uri(&self) -> &str {
        self.uri.as_str()
    }

    pub(in super::super) fn header_value(&self) -> &str {
        &self.header
    }

    pub(in super::super) fn qualifies_uri(&self, uri: &str) -> bool {
        self.source
            .qualify_document_url(uri)
            .is_ok_and(|parsed| self.query_matches(parsed.raw_query.as_deref()))
    }

    pub(super) fn belongs_to(&self, source: &Arc<AdmittedWebAppSource>) -> bool {
        Arc::ptr_eq(&self.source, source)
    }

    pub(super) fn query_matches(&self, query: Option<&str>) -> bool {
        self.query.as_optional_str() == Ok(query)
    }
}
