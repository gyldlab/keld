//! Independent policy contract using the pre-existing native admission API.
use super::{ORIGINAL_WEBAPP_LIMITS, TaskLedger};

#[test]
fn production_policy_admits_five_parallel_resource_requests() {
    let mut ledger = TaskLedger::new(ORIGINAL_WEBAPP_LIMITS);
    for identity in 1..=5 {
        ledger
            .begin(identity)
            .expect("five native requests may wait for one bounded reader");
    }
}

#[cfg(windows)]
#[test]
fn windows_approved_limits_are_shared_across_retired_and_live_work() {
    use super::{IntakeReject, RetainedByteCredit, worst_case_charge};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    const MIB: usize = 1024 * 1024;
    let mut ledger = TaskLedger::new(ORIGINAL_WEBAPP_LIMITS);
    for identity in 1..=1024 {
        ledger
            .begin(identity)
            .expect("approved Host-wide task capacity");
    }
    assert_eq!(ledger.begin(1025), Err(IntakeReject::NoTaskCapacity));
    assert_eq!(
        worst_case_charge(64 * MIB as u64, 0, 0, ORIGINAL_WEBAPP_LIMITS),
        Ok(64 * MIB)
    );
    assert_eq!(
        worst_case_charge(64 * MIB as u64 + 1, 0, 0, ORIGINAL_WEBAPP_LIMITS),
        Err(IntakeReject::AssetLimit)
    );
    let account = Arc::new(AtomicUsize::new(0));
    let retired =
        RetainedByteCredit::reserve(&account, 70 * MIB).expect("still-running retired read");
    assert!(
        RetainedByteCredit::reserve(&account, 60 * MIB).is_err(),
        "new view cannot reset the Host-wide budget"
    );
    assert_eq!(account.load(Ordering::Acquire), 70 * MIB);
    drop(retired);
    let live =
        RetainedByteCredit::reserve(&account, 128 * MIB).expect("exact approved aggregate bound");
    assert!(RetainedByteCredit::reserve(&account, 1).is_err());
    drop(live);
    assert_eq!(account.load(Ordering::Acquire), 0);
}
