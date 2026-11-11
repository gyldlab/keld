use super::{TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT_ENV, checked_receipt_controls};

#[test]
fn receipt_controls_reject_two_simultaneous_withholding_owners() {
    let error = checked_receipt_controls(true, true).expect_err("two controls must be refused");
    let error = error.to_string();
    assert!(error.contains(TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT_ENV));
    assert!(error.contains(crate::webview2::TEST_WINDOWS_BARRIER_GRACE_EXPIRY_ENV));
}

#[test]
fn disabled_or_single_receipt_control_preserves_exact_selection() {
    assert!(!checked_receipt_controls(false, false).unwrap());
    assert!(!checked_receipt_controls(false, true).unwrap());
    assert!(checked_receipt_controls(true, false).unwrap());
}
