//! Native COM apartment lifecycle, cleanup and refusal test family.
//!
//! These are tests of exact thread-local HRESULT behavior, not UI smoke tests.

use super::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize, ComApartment,
    S_FALSE, S_OK, WindowsUpdaterHelperLaunchError,
};

/// Enters COM's multithreaded apartment directly, as an independent probe of
/// the thread's COM state, and returns the raw `HRESULT`.
fn enter_multithreaded_apartment() -> i32 {
    // SAFETY: the reserved argument is null and the flag is a documented
    // COINIT value; the caller balances every success on this thread.
    unsafe {
        CoInitializeEx(
            std::ptr::null(),
            windows_sys::Win32::System::Com::COINIT_MULTITHREADED.cast_unsigned(),
        )
    }
}

#[test]
fn launch_apartment_is_left_on_its_own_thread_when_dropped() {
    std::thread::spawn(|| {
        let apartment = ComApartment::enter().expect("a fresh thread enters an STA");
        drop(apartment);
        // Microsoft: once COM is uninitialized on a thread it may be
        // reinitialized in any mode. While the STA were still entered, this
        // multithreaded request would fail with RPC_E_CHANGED_MODE.
        assert_eq!(enter_multithreaded_apartment(), S_OK);
        // SAFETY: balances the successful probe above on this thread.
        unsafe { CoUninitialize() };
    })
    .join()
    .expect("apartment thread");
}

#[test]
fn launch_apartment_reentered_on_an_sta_thread_balances_only_its_own_entry() {
    std::thread::spawn(|| {
        // SAFETY: the reserved argument is null and the flags are documented
        // COINIT values; the success is balanced below on this thread.
        let outer = unsafe {
            CoInitializeEx(
                std::ptr::null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).cast_unsigned(),
            )
        };
        assert_eq!(outer, S_OK);
        // Microsoft: a repeated compatible call returns S_FALSE and still
        // needs its own CoUninitialize.
        let apartment = ComApartment::enter().expect("S_FALSE is an entered apartment");
        drop(apartment);
        // The outer entry is still open, so the thread is still an STA.
        assert_eq!(
            enter_multithreaded_apartment(),
            windows_sys::Win32::Foundation::RPC_E_CHANGED_MODE
        );
        // SAFETY: balances the outer entry on this thread.
        unsafe { CoUninitialize() };
        assert_eq!(enter_multithreaded_apartment(), S_OK);
        // SAFETY: balances the successful probe above on this thread.
        unsafe { CoUninitialize() };
    })
    .join()
    .expect("apartment thread");
}

#[test]
fn launch_apartment_refuses_a_thread_in_another_apartment_and_leaves_it_entered() {
    std::thread::spawn(|| {
        assert_eq!(enter_multithreaded_apartment(), S_OK);
        match ComApartment::enter() {
            Err(WindowsUpdaterHelperLaunchError::ComInitialization { hresult }) => {
                assert_eq!(hresult, windows_sys::Win32::Foundation::RPC_E_CHANGED_MODE);
            }
            other => panic!("expected RPC_E_CHANGED_MODE, got {other:?}"),
        }
        // The refusal made no CoUninitialize: the thread's own MTA is still
        // entered, so a repeated request reports S_FALSE, not S_OK.
        assert_eq!(enter_multithreaded_apartment(), S_FALSE);
        // SAFETY: balances the two successful probes on this thread.
        unsafe {
            CoUninitialize();
            CoUninitialize();
        }
    })
    .join()
    .expect("apartment thread");
}
