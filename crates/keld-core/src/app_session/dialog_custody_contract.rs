//! GH-478 state contract. Real reader, writer and `AppKit` acceptance are separate.

use super::dialog_custody::{CancelResult, DialogCustody, DialogOwner, ReserveError, TerminalKind};
use super::{CorrelationId, PrimaryLinkIdentity};

fn owner(attempt: u32, correlation: u32, nonce: u64) -> DialogOwner {
    DialogOwner {
        identity: PrimaryLinkIdentity::Fixture,
        attempt,
        correlation: CorrelationId(correlation),
        nonce,
        parent: None,
    }
}

#[test]
fn cancelled_original_gets_one_terminal_and_retains_capacity_until_quiescence() {
    let mut custody = DialogCustody::default();
    let original = owner(1, 7, 1);
    custody.reserve(original).expect("first panel");
    assert_eq!(
        custody.cancel(original.identity, 1, 1),
        CancelResult::Requested
    );
    assert_eq!(
        custody.take_original_terminal(original),
        Some((CorrelationId(7), TerminalKind::Cancelled))
    );
    assert_eq!(custody.take_original_terminal(original), None);
    assert_eq!(custody.reserve(owner(1, 9, 2)), Err(ReserveError::Busy));
    assert!(
        !custody.may_present(original),
        "tombstone suppresses queued UI"
    );
    assert!(
        !custody.commit_native_result(original),
        "late native result cannot defeat cancellation"
    );
    custody.quiesced(original).expect("native callback drained");
    assert_eq!(
        custody.cancel(original.identity, 1, 1),
        CancelResult::AlreadyCompleted
    );
    custody
        .reserve(owner(1, 9, 2))
        .expect("capacity released only after drain");
}

#[test]
fn foreign_generation_cannot_cancel_active_effect() {
    let mut custody = DialogCustody::default();
    let original = owner(1, 7, 1);
    custody.reserve(original).expect("first panel");
    assert_eq!(
        custody.cancel(original.identity, 2, 1),
        CancelResult::NotFound
    );
    assert!(custody.may_present(original));
}

#[test]
fn committed_native_result_cannot_be_replaced_by_cancel_or_sent_twice() {
    let mut custody = DialogCustody::default();
    let original = owner(4, 19, 7);
    custody.reserve(original).expect("first panel");
    assert!(custody.commit_native_result(original));
    assert!(!custody.commit_native_result(original));
    assert_eq!(
        custody.cancel(original.identity, 4, 7),
        CancelResult::AlreadyCompleted
    );
    assert_eq!(
        custody.take_original_terminal(original),
        Some((CorrelationId(19), TerminalKind::NativeResult))
    );
    assert_eq!(custody.take_original_terminal(original), None);
    assert_eq!(custody.reserve(owner(4, 20, 8)), Err(ReserveError::Busy));
    custody.quiesced(original).expect("native callback drained");
    custody
        .reserve(owner(4, 20, 8))
        .expect("successor after drain");
}

#[test]
fn retired_generation_suppresses_wire_reply_but_keeps_native_custody() {
    let mut custody = DialogCustody::default();
    let original = owner(4, 21, 1);
    custody.reserve(original).expect("first panel");
    assert!(custody.retire(original.identity, 4));
    assert!(!custody.may_present(original));
    assert_eq!(custody.take_original_terminal(original), None);
    assert_eq!(custody.reserve(owner(5, 22, 1)), Err(ReserveError::Busy));
    custody
        .quiesced(original)
        .expect("retired native callback drained");
    custody
        .reserve(owner(5, 22, 1))
        .expect("new generation after drain");
    assert_eq!(
        custody.cancel(original.identity, 4, 1),
        CancelResult::NotFound
    );
}

#[test]
fn quiescence_without_outcome_or_retirement_is_not_a_release() {
    let mut custody = DialogCustody::default();
    let original = owner(1, 7, 1);
    custody.reserve(original).expect("first panel");
    assert_eq!(
        custody.quiesced(original),
        Err(ReserveError::InvalidTransition)
    );
    assert!(custody.may_present(original));
    assert_eq!(custody.reserve(owner(1, 9, 2)), Err(ReserveError::Busy));
}
