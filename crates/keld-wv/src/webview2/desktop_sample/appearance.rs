//! Windows application appearance. Never substitute the system theme or a registry
//! approximation for the user's `WinRT` accessibility text scale.

use super::{os_error, unavailable};
use crate::WvError;
use std::marker::PhantomData;
use std::rc::Rc;
use windows::UI::ViewManagement::{AccessibilitySettings, UISettings};
use windows::Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

/// One successful initialization count on this existing UI thread only.
/// `S_OK` and `S_FALSE` both own one count; a failed call creates no guard.
struct Apartment(PhantomData<Rc<()>>);

impl Apartment {
    fn enter() -> Result<Self, WvError> {
        // SAFETY: this capture runs on the existing Host STA. The requested
        // model is unchanged; failure is propagated before any WinRT object.
        // Every successful call is balanced by this !Send/!Sync local guard.
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }
            .map_err(|error| os_error("RoInitialize(desktop STA)", error.code()))?;
        Ok(Self(PhantomData))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: this guard owns exactly one successful initialization on the
        // current thread. All local appearance interfaces have already dropped.
        unsafe { RoUninitialize() };
    }
}

/// Both approved appearance observations share one explicit apartment balance.
/// COM initialization alone does not establish `WinRT` activation permission.
pub(super) fn read() -> Result<(f64, bool), WvError> {
    let _apartment = Apartment::enter()?;
    Ok((text_scale()?, dark()?))
}

fn text_scale() -> Result<f64, WvError> {
    let value = UISettings::new()
        .and_then(|settings| settings.TextScaleFactor())
        .map_err(|error| os_error("UISettings.TextScaleFactor", error.code()))?;
    if !(1.0..=2.25).contains(&value) {
        return Err(unavailable("invalid WinRT text scale"));
    }
    // The original Electron native04 comparison exercised unit text scale.
    // No independent Windows physical run has qualified larger accessibility text.
    if value.to_bits() != 1.0_f64.to_bits() {
        return Err(unavailable(
            "non-unit accessibility text scale is not qualified",
        ));
    }
    Ok(value)
}

fn dark() -> Result<bool, WvError> {
    // Only the flag is needed. The projected property returns a boolean and
    // never gives this Host a scheme string to own or release.
    let high_contrast = AccessibilitySettings::new()
        .and_then(|settings| settings.HighContrast())
        .map_err(|error| os_error("AccessibilitySettings.HighContrast", error.code()))?;
    if high_contrast {
        return Err(unavailable("forced colors are not qualified"));
    }
    let mut value = 0_u32;
    let mut size = 4_u32;
    // SAFETY: read one DWORD into a four-byte buffer from the existing user key;
    // both static names are NUL-terminated. No registry or system setting is written.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
                .as_ptr(),
            windows::core::w!("AppsUseLightTheme").as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut value).cast(),
            &raw mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(os_error("RegGetValueW(AppsUseLightTheme)", status));
    }
    match (size, value) {
        (4, 1) => Ok(false),
        // Only the ordinary light setting has physical W4/Electron qualification.
        (4, 0) => Err(unavailable("dark appearance is not qualified")),
        _ => Err(unavailable("invalid application theme value")),
    }
}
