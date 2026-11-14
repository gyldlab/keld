//! Same-account native snapshot -> String primitive -> canonical app envelope.
//! No page value selects source, lease, role, native receiver or decoder policy.
use super::super::native_text::{read_task_text, read_task_text_admitted};
use super::super::original_callbacks::callback_credit;
use super::invocation::CallPayload;
use super::{OriginalBridge, RefCell, RetainedByteCredit, WvError, failure};
use crate::original_call::queue::OriginalInputCharge;
use crate::renderer_bridge::NativeMessageFacts;
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::resource_source::NativeNavigationUrl;
use crate::wv_link::decode_storage::{AppDecodeAdmission, initial_storage_bytes};
use crate::wv_link::{AppWvLinkEnvelope, MAX_APP_WV_LINK_ENVELOPE_LEN};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2WebMessageReceivedEventArgs;
use windows::Win32::Foundation::E_INVALIDARG;

pub(super) struct CapturedInput {
    pub(super) envelope: Option<AppWvLinkEnvelope>,
    pub(super) url: Option<NativeNavigationUrl>,
    pub(super) document: Option<CheckedNativeDocumentLease>,
    pub(super) scratch: RetainedByteCredit,
    pub(super) charge: Option<OriginalInputCharge<CallPayload>>,
    pub(super) value_bytes: usize,
}
impl CapturedInput {
    pub(super) fn facts(&mut self, bridge: &OriginalBridge) -> Result<NativeMessageFacts, WvError> {
        let lease = self
            .document
            .as_ref()
            .filter(|lease| lease.is_current() && lease.belongs_to_source(&bridge.source))
            .ok_or_else(|| failure("new input lacks its exact current source lease"))?;
        let url = self
            .url
            .take()
            .ok_or_else(|| failure("native input Source was already consumed"))?;
        NativeMessageFacts::from_windows_source_url(url, &bridge.source, lease, &self.scratch)
            .map_err(failure)
    }

    /// The explicit body-first cut is shared by successful controls and every
    /// refused/partially decoded input. A transferred charge is already absent.
    fn dispose(mut self, bridge: &OriginalBridge) -> Result<(), WvError> {
        drop(self.envelope.take());
        drop(self.url.take());
        drop(self.document.take());
        if self.charge.is_some() {
            bridge
                .state
                .try_borrow_mut()
                .map_err(|_| bridge.custody_failure("input release met retained state borrow"))?
                .calls
                .release_input(&mut self.charge)
                .map_err(|_| {
                    bridge.custody_failure("input charge release violated its retained account")
                })?;
        }
        Ok(())
    }
}

struct CaptureTurn<'a>(&'a OriginalBridge);
impl Drop for CaptureTurn<'_> {
    fn drop(&mut self) {
        self.0.capturing.set(false);
    }
}

/// UTF8 has at least as many bytes as UTF16 units for well-formed text. Search
/// the SAME canonical initial-byte formula plus the native snapshot and NUL;
/// wider UTF8 still gets its exact admission before copying. No new cap.
fn snapshot_units(remaining: usize) -> Option<usize> {
    let mut low = 0usize;
    let mut high = MAX_APP_WV_LINK_ENVELOPE_LEN;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        let bytes = mid
            .checked_add(1)?
            .checked_mul(2)?
            .checked_add(initial_storage_bytes(mid)?)?;
        if bytes <= remaining {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    (low != 0).then_some(low)
}

impl OriginalBridge {
    #[expect(
        clippy::single_match_else,
        reason = "Keep the caught panic payload in the original match scope through input disposal and fault latching."
    )]
    pub(super) fn receive_message_on_ui(
        &self,
        args: &ICoreWebView2WebMessageReceivedEventArgs,
    ) -> Result<(), WvError> {
        if self.capturing.replace(true) {
            self.fail("nested input capture cannot replace its charged predecessor");
            return Err(failure("nested native input capture"));
        }
        let _turn = CaptureTurn(self);
        let mut input = self.capture(args)?;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.intake(&mut input)));
        let disposed = input.dispose(self);
        match result {
            Ok(result) => result.and(disposed),
            Err(_) => {
                self.fail("native input intake panicked");
                disposed?;
                Err(failure("native input intake panicked"))
            }
        }
    }

    fn grow_capture(
        &self,
        charge: &RefCell<Option<OriginalInputCharge<CallPayload>>>,
        bytes: usize,
    ) -> Result<(), WvError> {
        let mut token = charge
            .try_borrow_mut()
            .map_err(|_| self.custody_failure("input token is borrowed"))?;
        let token = token
            .as_mut()
            .ok_or_else(|| self.custody_failure("input token was transferred during capture"))?;
        self.state
            .try_borrow_mut()
            .map_err(|_| self.custody_failure("input account is borrowed"))?
            .calls
            .grow_input(token, bytes)
    }
    fn return_disposed(
        &self,
        charge: &RefCell<Option<OriginalInputCharge<CallPayload>>>,
        bytes: usize,
    ) -> Result<(), WvError> {
        let mut token = charge
            .try_borrow_mut()
            .map_err(|_| self.custody_failure("input release token is borrowed"))?;
        let token = token
            .as_mut()
            .ok_or_else(|| self.custody_failure("input release token is absent"))?;
        self.state
            .try_borrow_mut()
            .map_err(|_| self.custody_failure("input release account is borrowed"))?
            .calls
            .release_disposed_input_storage(token, bytes)
            .map_err(|_| {
                self.custody_failure("disposed input storage lost its exact retained charge")
            })
    }
    fn captured_bytes(
        &self,
        charge: &RefCell<Option<OriginalInputCharge<CallPayload>>>,
    ) -> Result<usize, WvError> {
        charge
            .try_borrow()
            .map_err(|_| self.custody_failure("input count is borrowed"))?
            .as_ref()
            .map(OriginalInputCharge::retained_bytes)
            .ok_or_else(|| self.custody_failure("input count has no retained token"))
    }

    fn capture(
        &self,
        args: &ICoreWebView2WebMessageReceivedEventArgs,
    ) -> Result<CapturedInput, WvError> {
        let metadata = std::mem::size_of::<CapturedInput>()
            + std::mem::size_of::<crate::renderer_bridge::RendererBridgeRequest>()
            + std::mem::size_of::<super::state::RetainedSend>()
            + 1024;
        let charge = RefCell::new(Some(
            self.state
                .try_borrow_mut()
                .map_err(|_| self.custody_failure("native input account is borrowed"))?
                .calls
                .begin_input(metadata)?,
        ));
        // The token remains outside catch: every partial/raw/native temporary
        // unwinds before the SAME record can release or transfer its credit.
        let captured = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.capture_owned(args, &charge)
        }));
        match captured {
            Ok(Ok((envelope, url, document, scratch, value_bytes))) => Ok(CapturedInput {
                envelope: Some(envelope),
                url: Some(url),
                document,
                scratch,
                charge: charge.into_inner(),
                value_bytes,
            }),
            failure_result => {
                let mut token = charge.into_inner();
                self.state
                    .try_borrow_mut()
                    .map_err(|_| self.custody_failure("failed capture release is borrowed"))?
                    .calls
                    .release_input(&mut token)
                    .map_err(|_| {
                        self.custody_failure("failed capture charge lost its original account")
                    })?;
                match failure_result {
                    Ok(Err(error)) => Err(error),
                    _ => Err(self.custody_failure("bounded native input capture panicked")),
                }
            }
        }
    }

    #[allow(unsafe_code)] // Actual Source and WebMessageAsJson task-string getters only.
    fn capture_owned(
        &self,
        args: &ICoreWebView2WebMessageReceivedEventArgs,
        charge: &RefCell<Option<OriginalInputCharge<CallPayload>>>,
    ) -> Result<
        (
            AppWvLinkEnvelope,
            NativeNavigationUrl,
            Option<CheckedNativeDocumentLease>,
            RetainedByteCredit,
            usize,
        ),
        WvError,
    > {
        let metadata_floor = self.captured_bytes(charge)?;
        let scratch = callback_credit(&self.navigation)
            .map_err(|_| failure("native Source scratch credit is unavailable"))?;
        let document = self
            .navigation
            .try_borrow()
            .map_err(|_| self.custody_failure("native Source owner is borrowed"))?
            .current_document();
        let url = read_task_text(
            // SAFETY: actual same callback args; shared RAII owns the task allocation.
            |out| unsafe { args.Source(out) },
            crate::resource_intake::ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
            4 * crate::resource_intake::ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
        )
        .map_err(|_| failure("native message Source is malformed"))?;
        self.source
            .qualify_document_url(&url)
            .map_err(|_| failure("native message Source is not the selected document"))?;
        let url = NativeNavigationUrl::capture_bounded(
            &url,
            url.encode_utf16().count(),
            &self.source.retained_credit_account(),
        )
        .map_err(|_| failure("native Source retained credit unavailable"))?;

        let remaining = self
            .state
            .try_borrow()
            .map_err(|_| self.custody_failure("input bound account is borrowed"))?
            .calls
            .remaining_input_storage();
        let maximum = snapshot_units(remaining)
            .ok_or_else(|| failure("native input snapshot budget exhausted"))?;
        let native_bytes = maximum
            .checked_add(1)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| failure("native snapshot count overflow"))?;
        self.grow_capture(charge, native_bytes)?; // Includes the terminating unit, before COM.
        let mut reserve = |bytes| self.grow_capture(charge, bytes).is_ok();
        let reserve_ref = &mut reserve;
        let (outer, (outer_admission, outer_initial)) = read_task_text_admitted(
            // SAFETY: exact callback args. JSON snapshot escapes embedded NUL;
            // unlike TryGetString, a malicious suffix cannot hide behind NUL.
            |out| unsafe { args.WebMessageAsJson(out) },
            maximum,
            MAX_APP_WV_LINK_ENVELOPE_LEN,
            move |_, bytes| {
                let initial = initial_storage_bytes(bytes).ok_or(E_INVALIDARG)?;
                let admission =
                    AppDecodeAdmission::reserve(bytes, reserve_ref).map_err(|_| E_INVALIDARG)?;
                Ok((admission, initial))
            },
        )
        .map_err(|_| failure("native JSON snapshot/copy admission refused"))?;
        // The exact native TaskText owner dropped before the helper returned.
        self.return_disposed(charge, native_bytes)?;
        let before_value = self.captured_bytes(charge)?;
        let text = outer_admission
            .decode_native_string(outer.as_bytes())
            .map_err(|_| failure("native message must be one bounded JSON String"))?;
        let value_charge = self
            .captured_bytes(charge)?
            .checked_sub(before_value)
            .ok_or_else(|| self.custody_failure("native String accounting moved backwards"))?;
        drop(outer); // Decoder scratch already dropped inside the same canonical owner.
        self.return_disposed(charge, outer_initial)?;
        if value_charge < text.capacity() {
            return Err(self.custody_failure("native String exceeds its admitted value backing"));
        }
        // Intermediate Content/string copies are gone; retain the one actual
        // decoded envelope String capacity through its full canonical parse.
        self.return_disposed(charge, value_charge - text.capacity())?;
        let raw_capacity = text.capacity();
        let inner_initial = initial_storage_bytes(text.len())
            .ok_or_else(|| failure("canonical envelope initial charge overflow"))?;
        let mut reserve = |bytes| self.grow_capture(charge, bytes).is_ok();
        let parsed = AppDecodeAdmission::reserve(text.len(), &mut reserve)
            .and_then(|admission| admission.decode(text.as_bytes()));
        drop(text);
        // On reserve failure this initial charge never existed; leave the
        // entire record to the outer failure path instead of refunding it.
        let envelope = parsed.map_err(|_| failure("canonical native envelope refused"))?;
        self.return_disposed(
            charge,
            inner_initial
                .checked_add(raw_capacity)
                .ok_or_else(|| failure("disposed envelope count overflow"))?,
        )?;
        let value_bytes = self
            .captured_bytes(charge)?
            .checked_sub(metadata_floor)
            .ok_or_else(|| {
                self.custody_failure("decoded value storage crossed immutable metadata")
            })?;
        Ok((envelope, url, document, scratch, value_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::original_call::queue::OriginalCallQueue;

    #[test]
    fn snapshot_bound_prepays_terminator_and_uses_canonical_initial_owner() {
        for remaining in [16_384usize, 65_536, 200_000, 500_000] {
            let units = snapshot_units(remaining).expect("bounded snapshot");
            let paid = 2 * (units + 1) + initial_storage_bytes(units).unwrap();
            assert!(paid <= remaining);
            if units < MAX_APP_WV_LINK_ENVELOPE_LEN {
                let next = units + 1;
                assert!(2 * (next + 1) + initial_storage_bytes(next).unwrap() > remaining);
            }
        }
    }

    #[test]
    fn bounded_decode_refusal_releases_only_the_same_disposed_input_record() {
        let mut calls = OriginalCallQueue::<CallPayload>::new().unwrap();
        let baseline = calls.retained_bytes();
        for raw in [
            br#"{"not":"a String"}"#.as_slice(),
            br#""valid\u0000suffix""#.as_slice(),
        ] {
            let mut charge = Some(calls.begin_input(1024).unwrap());
            {
                let mut reserve = |bytes| calls.grow_input(charge.as_mut().unwrap(), bytes).is_ok();
                let decoded = AppDecodeAdmission::reserve(raw.len(), &mut reserve)
                    .and_then(|admission| admission.decode_native_string(raw));
                if let Ok(text) = decoded {
                    assert!(
                        text.contains('\0'),
                        "the complete String preserves embedded NUL"
                    );
                    let mut reserve =
                        |bytes| calls.grow_input(charge.as_mut().unwrap(), bytes).is_ok();
                    let parsed = AppDecodeAdmission::reserve(text.len(), &mut reserve)
                        .and_then(|admission| admission.decode(text.as_bytes()));
                    assert!(parsed.is_err());
                    drop(parsed);
                    drop(text);
                }
            }
            assert_eq!(calls.retained_records(), 1, "refusal itself is no refund");
            assert!(calls.retained_bytes() > baseline);
            calls.release_input(&mut charge).unwrap();
            assert!(charge.is_none());
            assert_eq!(calls.retained_records(), 0);
            assert_eq!(calls.retained_bytes(), baseline);
        }
    }
}
