//! Bounded, strict conversion of a COM task-allocated native text result.
//! The caller reserves its existing source/bridge metadata charge before the
//! getter. No native allocation is copied until the UTF16 bound is checked.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use windows::Win32::Foundation::{E_INVALIDARG, E_OUTOFMEMORY, E_POINTER};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::PWSTR;

struct TaskText(PWSTR);

impl Drop for TaskText {
    #[allow(unsafe_code)] // Only the COM task allocation returned by the getter.
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the getter returned a COM task-allocated PWSTR; this
            // private owner is its sole releaser, including conversion failure.
            unsafe { CoTaskMemFree(Some(self.0.as_ptr().cast())) };
        }
    }
}

#[allow(unsafe_code)] // Bounded reads of a native getter's documented PWSTR.
pub(super) fn read_task_text(
    get: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>,
    max_utf16_units: usize,
    max_utf8_bytes: usize,
) -> windows::core::Result<String> {
    if max_utf16_units > crate::wv_link::MAX_APP_WV_LINK_ENVELOPE_LEN
        || max_utf8_bytes > crate::wv_link::MAX_APP_WV_LINK_ENVELOPE_LEN
    {
        return Err(E_INVALIDARG.into());
    }
    let mut text = TaskText(PWSTR::null());
    get(&raw mut text.0)?;
    if text.0.is_null() {
        return Err(E_POINTER.into());
    }
    let mut length = 0;
    loop {
        // SAFETY: a successful native string getter returns a NUL-terminated
        // allocation. We read at most limit+1 units and stop at its first NUL,
        // so rejected overlong strings are never copied into an owned buffer.
        let unit = unsafe { *text.0.as_ptr().add(length) };
        if unit == 0 {
            break;
        }
        if length == max_utf16_units {
            return Err(E_INVALIDARG.into());
        }
        length += 1;
    }
    // SAFETY: the bounded scan observed these initialized units before NUL;
    // the task allocation remains alive through strict UTF16 conversion.
    let units = unsafe { std::slice::from_raw_parts(text.0.as_ptr(), length) };
    let mut bytes = 0_usize;
    for ch in char::decode_utf16(units.iter().copied()) {
        let ch = ch.map_err(|_| windows::core::Error::from(E_INVALIDARG))?;
        bytes = bytes.checked_add(ch.len_utf8()).ok_or(E_INVALIDARG)?;
        if bytes > max_utf8_bytes {
            return Err(E_INVALIDARG.into());
        }
    }
    let mut result = String::new();
    result.try_reserve_exact(bytes).map_err(|_| E_OUTOFMEMORY)?;
    for ch in char::decode_utf16(units.iter().copied()) {
        result.push(ch.map_err(|_| windows::core::Error::from(E_INVALIDARG))?);
    }
    Ok(result)
}
