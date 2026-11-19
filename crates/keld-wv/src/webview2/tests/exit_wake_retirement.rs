//! Pure ownership controls; actual wait cancellation/profile proofs are native.

use super::{
    ProfileReleaseFault, ProfileReleaseWait, ReleaseSite, normal_receipt, release_channel,
    test_binding,
};
use crate::webview2::BrowserBinding;
use std::rc::Rc;
use std::sync::Arc;

#[test]
fn zero_controller_owner_retires_without_inventing_native_credit() {
    let release = Rc::new(ProfileReleaseWait::new(
        None,
        Arc::new(BrowserBinding::new()),
        None,
        ReleaseSite::Host,
    ));
    let result = release
        .retire_loop_wait()
        .expect("no native registration to retire");
    assert!(result.process.borrow().is_none());
    assert!(!result.observed.get());
    assert_eq!(result.normal_exit_observed_at.get(), None);
    assert_eq!(result.fault(), None);
}

#[test]
fn successful_retirement_preserves_normal_credit_timestamp_and_existing_fault() {
    let (sender, receiver) = release_channel();
    let release = Rc::new(ProfileReleaseWait::new(
        Some(receiver),
        test_binding(),
        None,
        ReleaseSite::Host,
    ));
    sender.send(normal_receipt()).expect("owner input");
    release.poll();
    release.arm();
    let normal_at = release.normal_exit_observed_at.get();
    let armed_at = release.armed_at.get();
    assert!(normal_at.is_some());
    release.fault.set(Some(ProfileReleaseFault::GraceExpired));
    let result = release
        .retire_loop_wait()
        .expect("retire empty native owner");
    assert!(result.observed.get());
    assert_eq!(result.normal_exit_observed_at.get(), normal_at);
    assert_eq!(result.armed_at.get(), armed_at);
    assert_eq!(result.fault(), Some(ProfileReleaseFault::GraceExpired));
}

#[test]
fn retained_loop_reference_prevents_successful_retirement() {
    let release = Rc::new(ProfileReleaseWait::new(
        None,
        Arc::new(BrowserBinding::new()),
        None,
        ReleaseSite::Host,
    ));
    let unfinished_loop = Rc::clone(&release);
    let error = release
        .retire_loop_wait()
        .err()
        .expect("another loop reference is still live");
    assert!(
        error.to_string().contains("sole completed-loop ownership"),
        "{error}"
    );
    assert_eq!(Rc::strong_count(&unfinished_loop), 1);
    let result = unfinished_loop.retire_loop_wait();
    assert!(
        result.is_ok(),
        "once the other owner is gone retirement succeeds"
    );
}

#[test]
fn failed_wait_retirement_is_terminal_and_cannot_be_retried_or_promoted() {
    use crate::webview2::WaitRegistration;
    use windows::Win32::Foundation::HANDLE;
    let handle = HANDLE(std::ptr::dangling_mut::<core::ffi::c_void>());
    let mut phase = WaitRegistration::Live(handle);
    assert_eq!(phase.begin_retirement(), Ok(Some(handle)));
    assert!(matches!(phase, WaitRegistration::Unproved));
    assert!(phase.begin_retirement().is_err());
    assert!(phase.begin_retirement().is_err());
    assert!(matches!(phase, WaitRegistration::Unproved));
    let mut retired = WaitRegistration::Retired;
    assert_eq!(retired.begin_retirement(), Ok(None));
    assert_eq!(retired.begin_retirement(), Ok(None));
    assert!(matches!(retired, WaitRegistration::Retired));
}
