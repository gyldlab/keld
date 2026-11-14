//! Allocation admission around the existing app-envelope deserializer.
//! `RawCompatEnvelope`, `LocalValueSeed` and their validators remain the grammar.
//! This adapter reserves before an owned allocation; it never returns credit.
#[path = "decode_storage/access.rs"]
mod access;
#[path = "decode_storage/error.rs"]
mod error;
#[path = "decode_storage/layout.rs"]
mod layout;
#[path = "decode_storage/visitor.rs"]
mod visitor;

use super::{
    AppWvLinkEnvelope, MAX_APP_WV_LINK_ENVELOPE_LEN, RawCompatEnvelope, WvLinkRejectReason,
    decode_raw_app,
};
use error::DecodeError;

/// A scoped admission to the caller's SAME local-call record. Construct this
/// before copying native text. The callback must grow that record's existing
/// `LocalCallBudget` charge atomically; it must not allocate or enter native code.
/// The caller keeps the full charge through raw/partial/decoded value disposal.
pub(crate) struct AppDecodeAdmission<'a, F: FnMut(usize) -> bool> {
    input_len: usize,
    state: State<'a, F>,
}

struct State<'a, F: FnMut(usize) -> bool> {
    reserve: &'a mut F,
    failure: Option<DecodeError>,
}

/// Exact raw-copy/parser-only prepayment from the canonical allocation owner.
/// It may be returned only after both the raw input and decoder have disposed;
/// allocations admitted by the value visitors remain charged separately.
pub(crate) fn initial_storage_bytes(input_len: usize) -> Option<usize> {
    (input_len <= MAX_APP_WV_LINK_ENVELOPE_LEN)
        .then(|| layout::initial_bytes(input_len))
        .flatten()
}

impl<'a, F: FnMut(usize) -> bool> AppDecodeAdmission<'a, F> {
    /// Charge the copied raw bytes, the pinned `serde_json` scratch growth peak,
    /// and fixed decoder/error metadata BEFORE the native-to-Rust copy.
    /// The native UTF16 allocation, if retained, is charged by its existing
    /// getter owner in addition to this amount.
    pub(crate) fn reserve(
        input_len: usize,
        reserve: &'a mut F,
    ) -> Result<Self, WvLinkRejectReason> {
        if input_len > MAX_APP_WV_LINK_ENVELOPE_LEN {
            return Err(WvLinkRejectReason::MalformedEnvelope);
        }
        let mut state = State {
            reserve,
            failure: None,
        };
        let initial =
            layout::initial_bytes(input_len).ok_or(WvLinkRejectReason::MalformedEnvelope)?;
        state.admit(initial).map_err(DecodeError::reason)?;
        Ok(Self { input_len, state })
    }

    /// Run the one canonical schema once, then its unchanged validation.
    /// No credit is released here, even after a syntax/allocation failure.
    pub(crate) fn decode(mut self, input: &[u8]) -> Result<AppWvLinkEnvelope, WvLinkRejectReason> {
        let raw = self.decode_owned::<RawCompatEnvelope>(input)?;
        decode_raw_app(raw, input.len())
    }

    /// `WebView2`'s JSON transport snapshot preserves embedded NUL through JSON
    /// escaping. Admit only a String primitive with this SAME bounded visitor;
    /// the decoded text still goes through the unchanged app-envelope grammar.
    pub(crate) fn decode_native_string(
        mut self,
        input: &[u8],
    ) -> Result<String, WvLinkRejectReason> {
        self.decode_owned::<String>(input)
    }

    fn decode_owned<T: serde::de::DeserializeOwned>(
        &mut self,
        input: &[u8],
    ) -> Result<T, WvLinkRejectReason> {
        if input.len() != self.input_len {
            return Err(WvLinkRejectReason::MalformedEnvelope);
        }
        let mut json = serde_json::Deserializer::from_slice(input);
        let raw = T::deserialize(access::Admitted {
            inner: &mut json,
            state: &mut self.state,
        })
        .map_err(|error| self.state.failure.unwrap_or(error).reason())?;
        if json.end().is_err() {
            drop(raw);
            return Err(WvLinkRejectReason::MalformedEnvelope);
        }
        drop(json);
        Ok(raw)
    }
}

impl<F: FnMut(usize) -> bool> State<'_, F> {
    fn admit(&mut self, additional: usize) -> Result<(), DecodeError> {
        if (self.reserve)(additional) {
            Ok(())
        } else {
            self.failure.get_or_insert(DecodeError::Storage);
            Err(DecodeError::Storage)
        }
    }

    fn outer<E: serde::de::Error, T>(&mut self, result: Result<T, DecodeError>) -> Result<T, E> {
        result.map_err(|error| {
            self.failure.get_or_insert(error);
            // Only constant diagnostic text reaches serde_json's allocating
            // Error::custom. The untrusted value is formatted only into our
            // fixed error classifier, never into a retained String.
            E::custom("bounded renderer decode refused")
        })
    }

    fn inner<T, E>(&self, result: Result<T, E>) -> Result<T, DecodeError> {
        result.map_err(|_| self.failure.unwrap_or(DecodeError::Malformed))
    }
}

#[cfg(test)]
#[path = "decode_storage/tests.rs"]
mod tests;
