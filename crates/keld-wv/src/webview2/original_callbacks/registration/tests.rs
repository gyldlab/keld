//! Same production token/phase owner; no native subscription is synthesized.

use super::*;
use windows::Win32::Foundation::E_FAIL;

#[test]
fn partial_install_failure_retains_prior_success_and_the_failed_out_token() {
    let first = Registration::new();
    let second = Registration::new();
    first
        .add(
            |token| {
                token.set(11);
                Ok(())
            },
            || panic!("no unwind"),
        )
        .unwrap();
    assert!(
        second
            .add(
                |token| {
                    token.set(22);
                    Err(E_FAIL.into())
                },
                || panic!("no unwind")
            )
            .is_err()
    );
    assert_eq!(first.phase.get(), Phase::Registered);
    assert_eq!(first.token.get(), 11);
    assert_eq!(second.phase.get(), Phase::Unproved);
    assert_eq!(second.token.get(), 22);
    assert!(!first.clear_after_close(false, true));
    assert!(!second.clear_after_close(false, true));
    assert_eq!(first.token.get(), 11);
    assert_eq!(second.token.get(), 22);
}

#[test]
fn reentrant_close_cannot_clear_the_actual_in_call_token_position() {
    let slot = Registration::new();
    slot.add(
        |token| {
            token.set(91);
            assert!(slot.in_call());
            assert!(!slot.clear_after_close(true, true));
            assert_eq!(slot.token.get(), 91);
            Ok(())
        },
        || panic!("no unwind"),
    )
    .unwrap();
    assert!(!slot.in_call());
    assert_eq!(slot.token.get(), 91);
    assert!(slot.clear_after_close(true, true));
    assert_eq!(slot.phase.get(), Phase::Closed);
    assert_eq!(slot.token.get(), 0);
}

#[test]
fn a_panicking_add_retains_native_output_and_reports_unproved_before_cleanup() {
    let slot = Registration::new();
    let failures = Cell::new(0);
    assert!(
        slot.add(
            |token| {
                token.set(17);
                panic!("native adapter call did not return");
            },
            || {
                assert_eq!(slot.phase.get(), Phase::Unproved);
                assert_eq!(slot.token.get(), 17);
                failures.set(failures.get() + 1);
            }
        )
        .is_err()
    );
    assert_eq!(failures.get(), 1);
    assert!(!slot.clear_after_close(false, true));
    assert!(!slot.clear_after_close(true, false));
    assert_eq!(slot.token.get(), 17);
    assert!(slot.clear_after_close(true, true));
}

#[test]
fn duplicate_add_cannot_overwrite_or_reenter_the_same_native_position() {
    let slot = Registration::new();
    slot.add(
        |token| {
            token.set(31);
            assert!(slot.add(|_| panic!("nested add entered"), || {}).is_err());
            Ok(())
        },
        || panic!("no unwind"),
    )
    .unwrap();
    assert!(
        slot.add(|_| panic!("duplicate add entered"), || {})
            .is_err()
    );
    assert_eq!(slot.token.get(), 31);
    assert!(slot.clear_after_close(true, true));
    assert!(
        slot.add(|_| panic!("closed position restarted"), || {})
            .is_err()
    );
}

#[test]
fn callback_stack_must_quiesce_after_close_before_token_is_released() {
    let slot = Registration::new();
    slot.add(
        |token| {
            token.set(44);
            Ok(())
        },
        || {},
    )
    .unwrap();
    assert!(!slot.clear_after_close(true, false));
    assert_eq!(slot.token.get(), 44);
    assert!(slot.clear_after_close(true, true));
}
