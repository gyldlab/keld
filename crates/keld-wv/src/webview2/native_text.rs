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
            #[cfg(test)]
            FREED_TASK_STRINGS.with(|count| count.set(count.get() + 1));
        }
    }
}

fn validate_limits(max_utf16_units: usize, max_utf8_bytes: usize) -> windows::core::Result<()> {
    if max_utf16_units > crate::wv_link::MAX_APP_WV_LINK_ENVELOPE_LEN
        || max_utf8_bytes > crate::wv_link::MAX_APP_WV_LINK_ENVELOPE_LEN
    {
        return Err(E_INVALIDARG.into());
    }
    Ok(())
}

impl TaskText {
    #[allow(unsafe_code)] // Same bounded scan/conversion for one or paired native outputs.
    fn copy_bounded(
        &self,
        max_utf16_units: usize,
        max_utf8_bytes: usize,
    ) -> windows::core::Result<String> {
        if self.0.is_null() {
            return Err(E_POINTER.into());
        }
        let mut length = 0;
        loop {
            // SAFETY: a successful native string getter returns a NUL-terminated
            // allocation. We read at most limit+1 units and stop at its first NUL,
            // so rejected overlong strings are never copied into an owned buffer.
            let unit = unsafe { *self.0.as_ptr().add(length) };
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
        let units = unsafe { std::slice::from_raw_parts(self.0.as_ptr(), length) };
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
}

pub(super) fn read_task_text(
    get: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>,
    max_utf16_units: usize,
    max_utf8_bytes: usize,
) -> windows::core::Result<String> {
    validate_limits(max_utf16_units, max_utf8_bytes)?;
    let mut text = TaskText(PWSTR::null());
    get(&raw mut text.0)?;
    text.copy_bounded(max_utf16_units, max_utf8_bytes)
}

/// `GetCurrentHeader` returns two independent task allocations in one call. Keep
/// both under the SAME RAII owner, including getter/first-conversion failures.
/// The second string's stricter field bound is selected only after the first
/// bounded string is known; a Host nonce therefore keeps its 32/32 bound before
/// owned copying. Neither field may exceed the remaining aggregate allowance.
pub(super) fn read_task_text_pair(
    get: impl FnOnce(*mut PWSTR, *mut PWSTR) -> windows::core::Result<()>,
    max_total_utf16_units: usize,
    max_total_utf8_bytes: usize,
    second_limits: impl FnOnce(&str) -> (usize, usize),
) -> windows::core::Result<(String, String)> {
    validate_limits(max_total_utf16_units, max_total_utf8_bytes)?;
    let mut first = TaskText(PWSTR::null());
    let mut second = TaskText(PWSTR::null());
    get(&raw mut first.0, &raw mut second.0)?;
    let first = first.copy_bounded(max_total_utf16_units, max_total_utf8_bytes)?;
    let remaining_units = max_total_utf16_units - first.encode_utf16().count();
    let remaining_bytes = max_total_utf8_bytes - first.len();
    let (units, bytes) = second_limits(&first);
    let second = second.copy_bounded(units.min(remaining_units), bytes.min(remaining_bytes))?;
    Ok((first, second))
}

#[cfg(test)]
thread_local! {
    static FREED_TASK_STRINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::CoTaskMemAlloc;

    #[allow(unsafe_code)] // Controlled COM allocation obeys the real getter's ownership contract.
    fn native_text(units: &[u16]) -> PWSTR {
        let bytes = (units.len() + 1) * std::mem::size_of::<u16>();
        // SAFETY: nonzero bounded fixture allocation, checked before writes.
        let allocation = unsafe { CoTaskMemAlloc(bytes) }.cast::<u16>();
        assert!(!allocation.is_null());
        // SAFETY: allocation has exactly units.len()+1 writable aligned u16s;
        // source is a live immutable slice and the final NUL is inside the buffer.
        unsafe {
            std::ptr::copy_nonoverlapping(units.as_ptr(), allocation, units.len());
            allocation.add(units.len()).write(0);
        }
        PWSTR(allocation)
    }

    #[allow(unsafe_code)] // Writes only the helper's two initialized getter out-pointers.
    fn pair_getter(first: *mut PWSTR, second: *mut PWSTR, a: &[u16], b: &[u16]) {
        // SAFETY: called only from read_task_text_pair's synchronous getter,
        // which provides both live writable out-pointers and then owns each result.
        unsafe {
            first.write(native_text(a));
            second.write(native_text(b));
        }
    }

    #[test]
    fn paired_getter_keeps_nonce_and_aggregate_bounds_and_frees_both_allocations() {
        FREED_TASK_STRINGS.set(0);
        let name = "X-Keld-Host-Attempt".encode_utf16().collect::<Vec<_>>();
        let oversized = vec![u16::from(b'a'); 33];
        let refused = read_task_text_pair(
            |a, b| {
                pair_getter(a, b, &name, &oversized);
                Ok(())
            },
            4096,
            16384,
            |_| (32, 32),
        );
        assert!(refused.is_err());
        assert_eq!(FREED_TASK_STRINGS.get(), 2);
        let valid = read_task_text_pair(
            |a, b| {
                pair_getter(a, b, &[u16::from(b'a')], &[u16::from(b'b')]);
                Ok(())
            },
            2,
            2,
            |_| (usize::MAX, usize::MAX),
        )
        .expect("exact combined bound");
        assert_eq!(valid, (String::from("a"), String::from("b")));
        assert_eq!(FREED_TASK_STRINGS.get(), 4);
        let over_total = read_task_text_pair(
            |a, b| {
                pair_getter(a, b, &[0x00e9], &[u16::from(b'b')]);
                Ok(())
            },
            2,
            2,
            |_| (usize::MAX, usize::MAX),
        );
        assert!(
            over_total.is_err(),
            "aggregate UTF8 bytes are distinct from UTF16 units"
        );
        assert_eq!(FREED_TASK_STRINGS.get(), 6);
    }

    #[test]
    fn paired_getter_failure_and_invalid_first_utf16_release_second_too() {
        FREED_TASK_STRINGS.set(0);
        let failed_getter = read_task_text_pair(
            |a, b| {
                pair_getter(a, b, &[u16::from(b'a')], &[u16::from(b'b')]);
                Err(E_INVALIDARG.into())
            },
            64,
            64,
            |_| (64, 64),
        );
        assert!(failed_getter.is_err());
        assert_eq!(FREED_TASK_STRINGS.get(), 2);
        let malformed = read_task_text_pair(
            |a, b| {
                pair_getter(a, b, &[0xd800], &[u16::from(b'b')]);
                Ok(())
            },
            64,
            64,
            |_| (64, 64),
        );
        assert!(malformed.is_err());
        assert_eq!(FREED_TASK_STRINGS.get(), 4);
    }
}
