//! GH-478 state contract. Real reader, writer and `AppKit` acceptance are separate.

use super::dialog_custody::{CancelResult, DialogCustody, DialogOwner, ReserveError, TerminalKind};
use super::{CorrelationId, PrimaryLinkIdentity};

// The nonce represents authenticated per-operation identity, not a fixture
// sequence number. Mint it through the same OS-backed RNG as real IPC tokens.
fn owner(attempt: u32, correlation: u32) -> DialogOwner {
    let token = keld_ipc::SessionToken::random().expect("test dialog nonce entropy");
    let nonce = u64::from_le_bytes(token.as_bytes()[..8].try_into().expect("32-byte token"));
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
    let original = owner(1, 7);
    let successor = owner(1, 9);
    custody.reserve(original).expect("first panel");
    assert_eq!(
        custody.cancel(original.identity, 1, original.nonce),
        CancelResult::Requested
    );
    assert_eq!(
        custody.take_original_terminal(original),
        Some((CorrelationId(7), TerminalKind::Cancelled))
    );
    assert_eq!(custody.take_original_terminal(original), None);
    assert_eq!(custody.reserve(successor), Err(ReserveError::Busy));
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
        custody.cancel(original.identity, 1, original.nonce),
        CancelResult::AlreadyCompleted
    );
    custody
        .reserve(successor)
        .expect("capacity released only after drain");
}

#[test]
fn foreign_generation_cannot_cancel_active_effect() {
    let mut custody = DialogCustody::default();
    let original = owner(1, 7);
    custody.reserve(original).expect("first panel");
    assert_eq!(
        custody.cancel(original.identity, 2, original.nonce),
        CancelResult::NotFound
    );
    assert!(custody.may_present(original));
}

#[test]
fn committed_native_result_cannot_be_replaced_by_cancel_or_sent_twice() {
    let mut custody = DialogCustody::default();
    let original = owner(4, 19);
    let successor = owner(4, 20);
    custody.reserve(original).expect("first panel");
    assert!(custody.commit_native_result(original));
    assert!(!custody.commit_native_result(original));
    assert_eq!(
        custody.cancel(original.identity, 4, original.nonce),
        CancelResult::AlreadyCompleted
    );
    assert_eq!(
        custody.take_original_terminal(original),
        Some((CorrelationId(19), TerminalKind::NativeResult))
    );
    assert_eq!(custody.take_original_terminal(original), None);
    assert_eq!(custody.reserve(successor), Err(ReserveError::Busy));
    custody.quiesced(original).expect("native callback drained");
    custody.reserve(successor).expect("successor after drain");
}

#[test]
fn retired_generation_suppresses_wire_reply_but_keeps_native_custody() {
    let mut custody = DialogCustody::default();
    let original = owner(4, 21);
    // Reuse the same random nonce across generations to preserve the original
    // stale-generation boundary assertion without hard-coding a nonce.
    let successor = DialogOwner {
        attempt: 5,
        correlation: CorrelationId(22),
        ..original
    };
    custody.reserve(original).expect("first panel");
    assert!(custody.retire(original.identity, 4));
    assert!(!custody.may_present(original));
    assert_eq!(custody.take_original_terminal(original), None);
    assert_eq!(custody.reserve(successor), Err(ReserveError::Busy));
    custody
        .quiesced(original)
        .expect("retired native callback drained");
    custody
        .reserve(successor)
        .expect("new generation after drain");
    assert_eq!(
        custody.cancel(original.identity, 4, original.nonce),
        CancelResult::NotFound
    );
}

#[test]
fn quiescence_without_outcome_or_retirement_is_not_a_release() {
    let mut custody = DialogCustody::default();
    let original = owner(1, 7);
    custody.reserve(original).expect("first panel");
    assert_eq!(
        custody.quiesced(original),
        Err(ReserveError::InvalidTransition)
    );
    assert!(custody.may_present(original));
    assert_eq!(custody.reserve(owner(1, 9)), Err(ReserveError::Busy));
}

#[test]
fn expiration_is_not_a_user_cancel_and_retains_capacity_until_native_drains() {
    let mut custody = DialogCustody::default();
    let initial = owner(3, 7);
    let successor = owner(3, 8);
    custody.reserve(initial).expect("first operation");
    assert!(custody.expire(initial));
    assert!(!custody.commit_native_result(initial));
    assert!(!custody.expire(initial));
    assert_eq!(
        custody.cancel(initial.identity, 3, initial.nonce),
        CancelResult::AlreadyCompleted
    );
    assert_eq!(
        custody.take_original_terminal(initial),
        Some((CorrelationId(7), TerminalKind::Expired))
    );
    assert_eq!(custody.reserve(successor), Err(ReserveError::Busy));
    custody
        .quiesced(initial)
        .expect("actual native panel drained");
    custody.reserve(successor).expect("next operation");
}
