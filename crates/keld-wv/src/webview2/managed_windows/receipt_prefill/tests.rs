//! Actual `ManagedCustody` state with injected observations; no native execution or credit.
#![allow(clippy::unwrap_used)] // Test fixture setup and actual-state oracles.

use std::convert::Infallible;
use std::time::Instant;

use super::super::{CreateAdmission, DeferredCreate, RELEASING_DESTROYED_BOUND, StageResult};
use super::{ManagedCustody, ReceiptPrefill, Releasing, Wake};
use crate::WebviewId;
use crate::engine::BuildStage;

type Custody = ManagedCustody<u8, ()>;

fn create(build: u64) -> DeferredCreate<()> {
    DeferredCreate {
        build,
        width: 1,
        height: 1,
        x: None,
        y: None,
        original: None,
    }
}

fn armed(now: Instant) -> Custody {
    let mut custody = Custody::default();
    custody
        .park(1, 101, Releasing::Destroy(WebviewId(1)), now)
        .unwrap();
    custody.arm_destroyed_prefill(1).unwrap();
    custody
}

fn publish(custody: &mut Custody, native_alive: bool) -> usize {
    custody
        .publish_staged_with(|_| native_alive, |_, _, _| Ok::<(), Infallible>(()))
        .unwrap()
}

fn withheld(custody: &mut Custody) {
    assert_eq!(
        custody.stage_destroyed(1, || panic!(
            "withheld receipt cannot post normal stage wake"
        )),
        StageResult::Withheld,
    );
}

#[test]
fn actual_receipt_and_two_distinct_admissions_use_the_same_row_then_native_publication() {
    let now = Instant::now();
    let mut custody = armed(now);
    let original = std::ptr::from_ref(custody.releasing.get(&1).unwrap());
    assert_eq!(
        custody.stage_destroyed(9, || panic!("foreign event")),
        StageResult::Unknown
    );
    withheld(&mut custody);
    withheld(&mut custody);
    assert_eq!(
        custody.admit(create(2), false, false),
        CreateAdmission::Queued
    );
    custody
        .post_prefilled_destroyed(|| panic!("one admission is insufficient"))
        .unwrap();
    assert_eq!(
        custody.admit(create(3), false, false),
        CreateAdmission::Queued
    );
    assert_eq!(
        publish(&mut custody, false),
        0,
        "receipt capture is not staging"
    );
    let mut posts = 0;
    custody
        .post_prefilled_destroyed(|| {
            posts += 1;
            true
        })
        .unwrap();
    custody
        .post_prefilled_destroyed(|| panic!("one wake only"))
        .unwrap();
    assert_eq!(posts, 1);
    assert_eq!(custody.take_prefilled_destroyed(), Some(1));
    assert_eq!(custody.take_prefilled_destroyed(), None);
    assert_eq!(
        publish(&mut custody, false),
        0,
        "take is not native release credit"
    );
    assert!(matches!(
        custody.stage_destroyed(1, || true),
        StageResult::Staged { .. }
    ));
    assert_eq!(
        std::ptr::from_ref(custody.releasing.get(&1).unwrap()),
        original
    );
    assert_eq!(
        custody.releasing[&1].parked_at, now,
        "the original clock is retained"
    );
    assert_eq!(
        publish(&mut custody, true),
        0,
        "a live HWND retains the same row"
    );
    assert_eq!(publish(&mut custody, false), 1);
    assert!(custody.releasing.is_empty());
    assert_eq!(
        custody
            .deferred
            .iter()
            .map(|input| input.build)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(custody.post_deferred_wake(false, false, now, || true), None);
    assert_eq!(custody.take_deferred_wake(false, false), Some(create(2)));
    assert_eq!(custody.post_deferred_wake(false, false, now, || true), None);
    assert_eq!(custody.take_deferred_wake(false, false), Some(create(3)));
}

#[test]
fn missing_actual_destroyed_stays_missing_after_both_admissions_and_terminal() {
    let now = Instant::now();
    let mut custody = armed(now);
    for build in [2, 3] {
        assert_eq!(
            custody.admit(create(build), false, false),
            CreateAdmission::Queued
        );
    }
    custody.cancel_prefill_wait();
    custody
        .post_prefilled_destroyed(|| panic!("no actual Destroyed"))
        .unwrap();
    assert_eq!(custody.take_prefilled_destroyed(), None);
    assert_eq!(publish(&mut custody, false), 0);
    assert!(!custody.exit_ready(true, true));
    let mut expired = Vec::new();
    assert_eq!(
        custody.expire(now + RELEASING_DESTROYED_BOUND, |key, hwnd| expired
            .push((key, hwnd))),
        1
    );
    assert_eq!(
        expired,
        [(1, 101)],
        "the original expiry still reports the owed row"
    );
}

#[test]
fn duplicate_build_and_terminal_refusal_cannot_fill_the_second_admission() {
    let now = Instant::now();
    let mut custody = armed(now);
    withheld(&mut custody);
    assert_eq!(
        custody.admit(create(2), false, false),
        CreateAdmission::Queued
    );
    assert_eq!(
        custody.admit(create(2), false, false),
        CreateAdmission::Queued
    );
    assert_eq!(
        custody.admit(create(3), true, false),
        CreateAdmission::Refused(create(3))
    );
    let prefill = custody.releasing[&1].prefill.as_ref().unwrap();
    assert_eq!(prefill.admitted, 1);
    assert_eq!(prefill.builds[0], 2);
    custody
        .post_prefilled_destroyed(|| panic!("no distinct second admission"))
        .unwrap();
    assert_eq!(custody.take_prefilled_destroyed(), None);
}

#[test]
fn late_actual_receipt_is_still_required_and_consumed_on_a_later_turn() {
    let now = Instant::now();
    let mut custody = armed(now);
    for build in [2, 3] {
        let _ = custody.admit(create(build), false, false);
    }
    custody
        .post_prefilled_destroyed(|| panic!("commands are not receipts"))
        .unwrap();
    withheld(&mut custody);
    assert_eq!(custody.take_prefilled_destroyed(), None);
    custody.post_prefilled_destroyed(|| true).unwrap();
    assert_eq!(custody.take_prefilled_destroyed(), Some(1));
    assert!(!custody.releasing[&1].staged);
}

#[test]
fn terminal_cancels_the_wait_only_and_preserves_the_actual_held_receipt() {
    let now = Instant::now();
    let mut custody = armed(now);
    withheld(&mut custody);
    custody.cancel_prefill_wait();
    assert_eq!(custody.releasing[&1].prefill.as_ref().unwrap().admitted, 0);
    assert_eq!(custody.releasing[&1].parked_at, now);
    custody.post_prefilled_destroyed(|| true).unwrap();
    assert_eq!(custody.take_prefilled_destroyed(), Some(1));
    assert_eq!(publish(&mut custody, false), 0);
    assert!(matches!(
        custody.stage_destroyed(1, || true),
        StageResult::Staged { .. }
    ));
    assert_eq!(publish(&mut custody, false), 1);
}

#[test]
fn refused_receipt_wake_retains_same_fact_without_retry_or_fabricated_take() {
    let now = Instant::now();
    let mut custody = armed(now);
    withheld(&mut custody);
    for build in [2, 3] {
        let _ = custody.admit(create(build), false, false);
    }
    let error = custody.post_prefilled_destroyed(|| false).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("retained receipt wake post failed")
    );
    custody
        .post_prefilled_destroyed(|| panic!("post cannot be retried"))
        .unwrap();
    assert_eq!(custody.take_prefilled_destroyed(), None);
    let entry = &custody.releasing[&1];
    assert!(!entry.staged);
    assert_eq!(entry.parked_at, now);
    let prefill = entry.prefill.as_ref().unwrap();
    assert!(prefill.destroyed);
    assert_eq!(prefill.wake, Wake::Failed);
}

#[test]
fn selection_refuses_missing_failed_staged_or_second_rows_without_replacing_one() {
    let now = Instant::now();
    let mut custody = Custody::default();
    assert!(custody.arm_destroyed_prefill(1).is_err());
    custody
        .park(
            1,
            101,
            Releasing::FailedBuild {
                build: 7,
                stage: BuildStage::Webview,
            },
            now,
        )
        .unwrap();
    assert!(custody.arm_destroyed_prefill(1).is_err());
    custody
        .park(2, 202, Releasing::Destroy(WebviewId(2)), now)
        .unwrap();
    assert!(matches!(
        custody.stage_destroyed(2, || true),
        StageResult::Staged { .. }
    ));
    assert!(custody.arm_destroyed_prefill(2).is_err());
    custody
        .park(3, 303, Releasing::Destroy(WebviewId(3)), now)
        .unwrap();
    custody.arm_destroyed_prefill(3).unwrap();
    custody
        .park(4, 404, Releasing::Destroy(WebviewId(4)), now)
        .unwrap();
    assert!(custody.arm_destroyed_prefill(4).is_err());
    assert!(custody.arm_destroyed_prefill(3).is_err());
    assert!(custody.releasing[&3].prefill.is_some());
    assert!(custody.releasing[&4].prefill.is_none());
}

#[test]
fn receipt_handling_does_not_reset_the_real_parked_expiry() {
    let now = Instant::now();
    let mut custody = armed(now);
    withheld(&mut custody);
    for build in [2, 3] {
        let _ = custody.admit(create(build), false, false);
    }
    custody.post_prefilled_destroyed(|| true).unwrap();
    assert_eq!(custody.take_prefilled_destroyed(), Some(1));
    assert!(matches!(
        custody.stage_destroyed(1, || true),
        StageResult::Staged { .. }
    ));
    assert_eq!(
        custody.releasing[&1].expires_at(),
        now + RELEASING_DESTROYED_BOUND
    );
    let mut expired = Vec::new();
    assert_eq!(
        custody.expire(now + RELEASING_DESTROYED_BOUND, |key, hwnd| expired
            .push((key, hwnd))),
        1
    );
    assert_eq!(expired, [(1, 101)]);
}

#[test]
fn fixed_optional_metadata_fits_its_compiled_bound() {
    let bytes = std::mem::size_of::<Option<ReceiptPrefill>>();
    assert!(bytes <= 32);
    println!(
        "H4_PREFILL_LAYOUT optional_metadata_bytes={bytes} releasing_row_bytes={}",
        std::mem::size_of::<super::super::ReleasingEntry>()
    );
}

#[test]
fn a_different_owned_destroyed_row_cannot_supply_the_selected_receipt() {
    let now = Instant::now();
    let mut custody = armed(now);
    custody
        .park(2, 202, Releasing::Destroy(WebviewId(2)), now)
        .unwrap();
    assert!(matches!(
        custody.stage_destroyed(2, || true),
        StageResult::Staged { .. }
    ));
    for build in [3, 4] {
        let _ = custody.admit(create(build), false, false);
    }
    custody
        .post_prefilled_destroyed(|| panic!("a different actual row is not the receipt"))
        .unwrap();
    assert_eq!(custody.take_prefilled_destroyed(), None);
    assert!(!custody.releasing[&1].prefill.as_ref().unwrap().destroyed);
    assert_eq!(publish(&mut custody, false), 1);
    assert!(custody.releasing.contains_key(&1));
}
