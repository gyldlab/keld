//! Admit bounded actual Source and one complete `WebView2` JSON String.
use std::cell::Cell;

use super::super::native_text::{read_task_text, read_task_text_admitted};
use super::super::original_callbacks::callback_credit;
use super::{OriginalBridge, WvError, failure};
use crate::resource_intake::ORIGINAL_WEBAPP_LIMITS;
use crate::wv_link::decode_storage::{AppDecodeAdmission, initial_storage_bytes};
use crate::wv_link::{AppWvLinkEnvelope, MAX_APP_WV_LINK_ENVELOPE_LEN, WvLinkEnvelope};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2WebMessageReceivedEventArgs;
use windows::Win32::Foundation::E_INVALIDARG;

struct CaptureTurn<'a>(&'a OriginalBridge);
impl Drop for CaptureTurn<'_> {
    fn drop(&mut self) {
        self.0.capturing.set(false);
    }
}

/// One existing local-call input record. The actual captured strings and
/// decoded body are disposed before the same record gives any credit back.
struct CaptureCredit<'a> {
    bridge: &'a OriginalBridge,
    bytes: Cell<usize>,
    metadata_floor: usize,
}

impl CaptureCredit<'_> {
    fn grow(&self, additional: usize) -> Result<(), WvError> {
        let total = self
            .bytes
            .get()
            .checked_add(additional)
            .ok_or_else(|| failure("selected input storage overflow"))?;
        self.bridge
            .budget
            .try_borrow_mut()
            .map_err(|_| failure("selected input account borrowed"))?
            .grow_record_storage(additional)?;
        self.bytes.set(total);
        Ok(())
    }

    fn refund_disposed(&self, bytes: usize) -> Result<(), WvError> {
        let retained = self
            .bytes
            .get()
            .checked_sub(bytes)
            .filter(|remaining| *remaining >= self.metadata_floor)
            .ok_or_else(|| failure("selected input refund crosses live metadata"))?;
        self.bridge
            .budget
            .try_borrow_mut()
            .map_err(|_| failure("selected input disposal account borrowed"))?
            .release_record_storage(bytes)?;
        self.bytes.set(retained);
        Ok(())
    }

    fn finish(self) -> Result<(), WvError> {
        self.bridge
            .budget
            .try_borrow_mut()
            .map_err(|_| failure("selected input release account borrowed"))?
            .release(self.bytes.get())
    }
}

/// Prepay one worst-case UTF-16 task snapshot including NUL and the initial
/// UTF-8/canonical decoder storage. The canonical decoder then charges growth.
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
    pub(super) fn receive_message_on_ui(
        &self,
        args: &ICoreWebView2WebMessageReceivedEventArgs,
    ) -> Result<(), WvError> {
        if self.retired.get() || self.faulted.get() {
            return Err(failure("retired selected bridge refuses native input"));
        }
        if self.capturing.replace(true) {
            self.fail("nested selected capture cannot replace its active input");
            return Err(failure("nested selected native input capture"));
        }
        let _turn = CaptureTurn(self);
        let metadata_floor = std::mem::size_of::<CaptureCredit<'_>>()
            + std::mem::size_of::<AppWvLinkEnvelope>()
            + 1024;
        self.budget
            .try_borrow_mut()
            .map_err(|_| failure("selected input account borrowed at admission"))?
            .reserve(metadata_floor)?;
        let credit = CaptureCredit {
            bridge: self,
            bytes: Cell::new(metadata_floor),
            metadata_floor,
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.capture_startup(args, &credit)
        }));
        if let Err(error) = credit.finish() {
            self.fail("selected input release lost its retained account");
            return Err(error);
        }
        if let Ok(result) = result {
            result
        } else {
            self.fail("selected input capture unwound");
            Err(failure(
                "selected input capture unwound with storage disposed",
            ))
        }
    }

    #[allow(unsafe_code)] // Exact attached callback Source and WebMessageAsJson task strings.
    fn capture_startup(
        &self,
        args: &ICoreWebView2WebMessageReceivedEventArgs,
        credit: &CaptureCredit<'_>,
    ) -> Result<(), WvError> {
        let source_scratch = callback_credit(&self.navigation)
            .map_err(|_| failure("selected Source scratch credit unavailable"))?;
        let captured = self
            .navigation
            .try_borrow()
            .map_err(|_| failure("selected Source document owner borrowed"))?
            .current_document();
        let source = read_task_text(
            // SAFETY: the actual retained callback args own the call and the
            // existing task-string helper owns the result from this STA getter.
            |out| unsafe { args.Source(out) },
            ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
            4 * ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
        )
        .map_err(|_| failure("selected native message Source malformed"))?;
        if let Some(document) = &captured {
            crate::renderer_bridge::NativeMessageFacts::check_windows_source_url(
                &source,
                &self.source,
                document,
                &source_scratch,
            )
            .map_err(failure)?;
        } else {
            // The tokenless bind can precede the first native document commit.
            self.source.qualify_document_url(&source).map_err(|_| {
                failure("selected native message Source is outside its admitted entry")
            })?;
        }
        drop(source);
        let remaining = self
            .budget
            .try_borrow()
            .map_err(|_| failure("selected input budget borrowed"))?
            .remaining_storage();
        let maximum = snapshot_units(remaining)
            .ok_or_else(|| failure("selected native snapshot budget exhausted"))?;
        let native_bytes = maximum
            .checked_add(1)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| failure("selected native snapshot overflow"))?;
        credit.grow(native_bytes)?;
        let mut reserve = |bytes| credit.grow(bytes).is_ok();
        let reserve_ref = &mut reserve;
        let (outer, (outer_admission, outer_initial)) = read_task_text_admitted(
            // SAFETY: same actual callback args. The task string owner keeps
            // the native buffer and captures JSON with NUL suffixes escaped.
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
        .map_err(|_| failure("selected native JSON snapshot admission refused"))?;
        credit.refund_disposed(native_bytes)?;
        let before_value = credit.bytes.get();
        let text = outer_admission
            .decode_native_string(outer.as_bytes())
            .map_err(|_| failure("selected native message requires one complete JSON String"))?;
        let value_charge = credit
            .bytes
            .get()
            .checked_sub(before_value)
            .ok_or_else(|| failure("selected decoded String charge underflow"))?;
        drop(outer);
        credit.refund_disposed(outer_initial)?;
        let intermediate = value_charge
            .checked_sub(text.capacity())
            .ok_or_else(|| failure("selected decoded String exceeded its admitted backing"))?;
        credit.refund_disposed(intermediate)?;
        let raw_capacity = text.capacity();
        let initial = initial_storage_bytes(text.len())
            .ok_or_else(|| failure("selected envelope initial charge overflow"))?;
        let mut reserve = |bytes| credit.grow(bytes).is_ok();
        let envelope = AppDecodeAdmission::reserve(text.len(), &mut reserve)
            .and_then(|admission| admission.decode(text.as_bytes()));
        drop(text);
        let envelope =
            envelope.map_err(|_| failure("selected native canonical envelope refused"))?;
        credit.refund_disposed(
            initial
                .checked_add(raw_capacity)
                .ok_or_else(|| failure("selected disposed envelope count overflow"))?,
        )?;
        match envelope {
            AppWvLinkEnvelope::Transport(WvLinkEnvelope::Bind) => self.wake(),
            AppWvLinkEnvelope::Registered {
                document,
                methods: _,
            } => {
                let captured = captured.as_ref().ok_or_else(|| {
                    failure("registration did not capture a qualified native document")
                })?;
                self.register_app(captured, &document)?;
                self.wake();
            }
            _ => {
                return Err(failure(
                    "selected startup accepts only binding and app registration",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_APP_WV_LINK_ENVELOPE_LEN, initial_storage_bytes, snapshot_units};

    #[test]
    fn selected_capture_prepays_the_native_terminator_and_canonical_decoder() {
        assert_eq!(snapshot_units(0), None);
        for remaining in [16_384usize, 65_536, 200_000, 500_000] {
            let units = snapshot_units(remaining).unwrap();
            let paid = 2 * (units + 1) + initial_storage_bytes(units).unwrap();
            assert!(paid <= remaining);
            if units < MAX_APP_WV_LINK_ENVELOPE_LEN {
                let next = units + 1;
                assert!(2 * (next + 1) + initial_storage_bytes(next).unwrap() > remaining);
            }
        }
    }
}
