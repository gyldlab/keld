//! Read-only Win32 observations for the one retained development facts record.
//! Calls run outside shutdown.transition. Outputs never grant filesystem access.

#![deny(unsafe_op_in_unsafe_fn)]

use std::ptr::null_mut;

use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Documents, FOLDERID_RoamingAppData, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath,
};
use windows_sys::core::GUID;

use super::{HostAppError, MAX_FIELD_BYTES, MAX_RECORD_BYTES, invalid, paths, valid_text};

/// Cold temporary transport bound; this is not an additional runtime account.
/// The complete native list must fit before its first item can be selected.
const MAX_LANGUAGE_UNITS: usize = MAX_RECORD_BYTES;

#[derive(Debug, Default)]
pub(super) struct Observation {
    pub(super) app_data: Option<String>,
    pub(super) documents: Option<String>,
    pub(super) locale: Option<String>,
}

pub(super) fn capture() -> Result<Observation, HostAppError> {
    Ok(Observation {
        app_data: known_folder(&FOLDERID_RoamingAppData)?,
        documents: known_folder(&FOLDERID_Documents)?,
        locale: preferred_language()?,
    })
}

#[allow(unsafe_code)]
fn preferred_language() -> Result<Option<String>, HostAppError> {
    let mut count = 0_u32;
    let mut units = 0_u32;
    // SAFETY: the size-query form has a NULL buffer and zero input size. Both
    // output pointers address live u32 locals for this synchronous call.
    if unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &raw mut count,
            null_mut(),
            &raw mut units,
        )
    } == 0
    {
        return Ok(None);
    }
    let length = usize::try_from(units).map_err(|_| invalid("native language size overflow"))?;
    if !(2..=MAX_LANGUAGE_UNITS).contains(&length) {
        return Err(invalid(
            "native preferred-language list exceeds its bounded capture",
        ));
    }
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(length)
        .map_err(|_| invalid("cannot retain native preferred-language list"))?;
    buffer.resize(length, 0_u16);
    count = 0;
    // SAFETY: the writable buffer has exactly units live u16 elements. The two
    // output scalars remain live; no Rust borrow escapes or callback is installed.
    // A changed list may refuse this call; it is never retried with a new capture.
    if unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &raw mut count,
            buffer.as_mut_ptr(),
            &raw mut units,
        )
    } == 0
    {
        return Ok(None);
    }
    let returned = usize::try_from(units).map_err(|_| invalid("native language size overflow"))?;
    let observed = buffer
        .get(..returned)
        .ok_or_else(|| invalid("native language length exceeds supplied storage"))?;
    first_language(observed, count)
}

fn first_language(buffer: &[u16], expected_count: u32) -> Result<Option<String>, HostAppError> {
    if buffer.len() < 2 || !buffer.ends_with(&[0, 0]) {
        return Err(invalid(
            "native preferred-language list is not double terminated",
        ));
    }
    if buffer == [0, 0] {
        return if expected_count == 0 {
            Ok(None)
        } else {
            Err(invalid(
                "native preferred-language count disagrees with its list",
            ))
        };
    }
    let mut first = None;
    let mut cursor = 0;
    let mut count = 0_u32;
    while cursor < buffer.len() - 1 {
        let length = buffer[cursor..]
            .iter()
            .position(|unit| *unit == 0)
            .ok_or_else(|| invalid("native preferred-language item is unterminated"))?;
        if length == 0 {
            return Err(invalid("native preferred-language item is empty"));
        }
        let item = &buffer[cursor..cursor + length];
        if char::decode_utf16(item.iter().copied()).any(|unit| unit.is_err()) {
            return Err(invalid("native preferred-language item is not UTF-16"));
        }
        if first.is_none() {
            let text = String::from_utf16(item)
                .map_err(|_| invalid("native preferred-language item is not UTF-16"))?;
            if !valid_text(&text) {
                return Err(invalid(
                    "native preferred language exceeds 4096 UTF-8 bytes",
                ));
            }
            first = Some(text);
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| invalid("native preferred-language count overflow"))?;
        cursor += length + 1;
    }
    if cursor != buffer.len() - 1 || count != expected_count {
        return Err(invalid(
            "native preferred-language count disagrees with its list",
        ));
    }
    Ok(first)
}

/// Installed before native entry: even a failed HRESULT can return task memory.
struct KnownFolderPath {
    pointer: *mut u16,
}

impl Drop for KnownFolderPath {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: only SHGetKnownFolderPath writes this field. Its documented
        // task allocation must be freed even on failure; NULL is harmless.
        // This unique non-clone owner makes the single release after all reads.
        unsafe { CoTaskMemFree(self.pointer.cast()) };
    }
}

#[allow(unsafe_code)]
fn known_folder(id: &GUID) -> Result<Option<String>, HostAppError> {
    let mut owned = KnownFolderPath {
        pointer: null_mut(),
    };
    // SAFETY: id is one fixed SDK KnownFolder GUID. NULL token selects the
    // current user. No CREATE flag is present; DONT_VERIFY avoids an existence
    // probe. The output address belongs to the already installed unique owner.
    let status = unsafe {
        SHGetKnownFolderPath(
            id,
            KF_FLAG_DONT_VERIFY.cast_unsigned(),
            null_mut(),
            &raw mut owned.pointer,
        )
    };
    if status != 0 {
        return Ok(None);
    }
    if owned.pointer.is_null() {
        return Err(invalid(
            "successful native known-folder lookup returned no string",
        ));
    }
    let mut length = None;
    for index in 0..=MAX_FIELD_BYTES {
        // SAFETY: successful SHGetKnownFolderPath promises a NUL-terminated
        // allocation retained by owned. Read sequentially only until NUL or
        // the existing field bound; no pointer outlives the owner.
        if unsafe { *owned.pointer.add(index) } == 0 {
            length = Some(index);
            break;
        }
    }
    let length =
        length.ok_or_else(|| invalid("native known-folder path exceeds its field bound"))?;
    // SAFETY: the scan established this many initialized non-NUL u16 elements
    // inside the still-owned native allocation. The borrowed slice is not retained.
    let wide = unsafe { std::slice::from_raw_parts(owned.pointer, length) };
    let path = String::from_utf16(wide)
        .map_err(|_| invalid("native known-folder path is not valid UTF-16"))?;
    if !valid_text(&path) {
        return Err(invalid("native known-folder path exceeds 4096 UTF-8 bytes"));
    }
    Ok(paths::observed_absolute(&path).then_some(path))
}

#[cfg(test)]
#[path = "windows_native_tests.rs"]
mod tests;
