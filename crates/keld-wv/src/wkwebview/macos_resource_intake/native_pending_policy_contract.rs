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
