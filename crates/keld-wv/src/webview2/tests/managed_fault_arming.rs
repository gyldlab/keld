//! Managed fault teardown must finish before the shared barrier arms.

use super::{
    ProfileReleaseWait, ReleaseSite, abnormal_receipt, alive_process_handle, release_channel,
    test_binding,
};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::time::Duration;

#[test]
fn a_fault_closes_every_owner_before_arming_and_repeated_poll_never_renews() {
    let (sender, receiver) = release_channel();
    let release = ProfileReleaseWait::with_bounds(
        Some(receiver),
        test_binding(),
        Some(alive_process_handle()),
        ReleaseSite::Host,
        Duration::from_hours(1),
        Duration::from_hours(1),
    );
    let mut legacy_views = BTreeMap::new();
    let legacy_retired = Cell::new(false);
    let managed_retired = Cell::new(false);
    let callback_count = Cell::new(0);

    // The callback boundary represents both native owners. Actual HWND/COM
    // teardown remains the facade-native fixture's independent obligation.
    sender
        .send(abnormal_receipt())
        .expect("native fault receipt");
    assert!(
        release.poll_host_receipts_with(&mut legacy_views, None, || {
            assert_eq!(
                release.armed_at.get(),
                None,
                "the barrier armed before managed Close"
            );
            assert!(!legacy_retired.replace(true));
            assert!(!managed_retired.replace(true));
            callback_count.set(callback_count.get() + 1);
        })
    );
    assert!(legacy_retired.get() && managed_retired.get());
    assert_eq!(callback_count.get(), 1, "the close owner must actually run");
    let first = release
        .armed_at
        .get()
        .expect("armed after every controller closed");

    assert!(
        release.poll_host_receipts_with(&mut legacy_views, None, || {
            assert!(
                legacy_retired.get() && managed_retired.get(),
                "no owner resurrected"
            );
            assert_eq!(
                release.armed_at.get(),
                Some(first),
                "a repeated fault cannot renew its bound"
            );
            callback_count.set(callback_count.get() + 1);
        })
    );
    assert_eq!(
        callback_count.get(),
        2,
        "the idempotent existing fault reaction remains live"
    );
    assert_eq!(release.armed_at.get(), Some(first));
}

#[test]
fn a_healthy_poll_does_not_close_managed_resources_or_arm_shutdown() {
    let (_sender, receiver) = release_channel();
    let release = ProfileReleaseWait::with_bounds(
        Some(receiver),
        test_binding(),
        Some(alive_process_handle()),
        ReleaseSite::Host,
        Duration::from_hours(1),
        Duration::from_hours(1),
    );
    let mut legacy_views = BTreeMap::new();
    let closed = Cell::new(false);
    assert!(!release.poll_host_receipts_with(&mut legacy_views, None, || closed.set(true)));
    assert!(!closed.get());
    assert_eq!(release.armed_at.get(), None);
}
