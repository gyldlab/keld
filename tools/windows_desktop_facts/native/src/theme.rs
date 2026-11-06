use crate::error::{ProbeError, ProbeResult, wide};
use serde_json::{Value, json};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, GetLastError, LocalFree, SetLastError};
use windows_sys::Win32::Graphics::Gdi::{COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW};

struct RegistryFact {
    status: u32,
    value: Option<u32>,
}

impl RegistryFact {
    fn json(&self) -> Value {
        json!({"status": self.status, "value": self.value})
    }
}

fn preference(name: &str) -> ProbeResult<RegistryFact> {
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = wide(name);
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: borrowed predefined key, NUL-terminated strings, and a writable
    // DWORD-sized buffer. This read neither creates nor changes registry values.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    if status == ERROR_SUCCESS && size != std::mem::size_of::<u32>() as u32 {
        return Err(ProbeError::invalid(
            "RegGetValueW",
            "Successful DWORD query returned an unexpected size",
        ));
    }
    Ok(RegistryFact {
        status,
        value: if status == ERROR_SUCCESS {
            Some(value)
        } else {
            None
        },
    })
}

pub(crate) fn high_contrast(invalid_size: bool) -> ProbeResult<Value> {
    let size = std::mem::size_of::<HIGHCONTRASTW>() as u32;
    let mut value = HIGHCONTRASTW {
        cbSize: if invalid_size { 0 } else { size },
        ..Default::default()
    };
    // SAFETY: GET is read-only, value is writable for size bytes. The negative
    // control deliberately supplies cbSize=0 without an undersized allocation.
    let ok = unsafe {
        SetLastError(0);
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            size,
            (&mut value as *mut HIGHCONTRASTW).cast(),
            0,
        )
    };
    // SAFETY: no pointer arguments, captured before cleanup can overwrite it.
    let error = if ok == 0 {
        Some(unsafe { GetLastError() })
    } else {
        None
    };
    let allocated = !value.lpszDefaultScheme.is_null();
    if allocated {
        // SAFETY: HIGHCONTRASTW documentation gives ownership of this system
        // allocated scheme buffer to the caller, to be released with LocalFree.
        // It is never read or retained, and this is its only release.
        let remaining = unsafe { LocalFree(value.lpszDefaultScheme.cast()) };
        if !remaining.is_null() {
            // SAFETY: no arguments, immediately follows the failed cleanup.
            return Err(ProbeError::native(
                "LocalFree(HIGHCONTRASTW scheme)",
                i64::from(unsafe { GetLastError() }),
            ));
        }
    }
    if let Some(error) = error {
        return Err(ProbeError::native(
            "SystemParametersInfoW(SPI_GETHIGHCONTRAST)",
            i64::from(error),
        ));
    }
    Ok(
        json!({"active": value.dwFlags & HCF_HIGHCONTRASTON != 0, "flags": value.dwFlags,
        "schemeBufferReturned": allocated, "schemeBufferFreed": allocated}),
    )
}

fn color(value: u32) -> Value {
    // COLORREF is 0x00bbggrr. Zero is valid black, never a failure sentinel.
    json!({"colorRef": value, "r": value & 0xff, "g": (value >> 8) & 0xff, "b": (value >> 16) & 0xff})
}

pub(crate) fn sample() -> ProbeResult<Value> {
    let contrast = high_contrast(false)?;
    let apps = preference("AppsUseLightTheme")?;
    let system = preference("SystemUsesLightTheme")?;
    // SAFETY: both indices are documented system colors; GetSysColor borrows
    // no resources and may legitimately return zero (black).
    let (window, text) = unsafe { (GetSysColor(COLOR_WINDOW), GetSysColor(COLOR_WINDOWTEXT)) };
    let scheme = if contrast["active"] == false {
        apps.value
            .map(|value| if value == 0 { "dark" } else { "light" })
    } else {
        None
    };
    Ok(json!({
        "appsUseLightTheme": apps.json(), "systemUsesLightTheme": system.json(),
        "highContrast": contrast, "systemColors": {"window": color(window), "windowText": color(text)},
        "nonForcedColorScheme": scheme,
    }))
}
