//! Receipt ordering controls for the one production profile-release owner.

use super::{
    ProfileReleaseFault, ProfileReleaseWait, ReleaseSite, abnormal_receipt, normal_receipt,
    release_channel, test_binding,
};
use crate::webview2::browser_exit_is_relevant;
use std::time::{Duration, Instant};

fn waiter(
    receiver: std::sync::mpsc::Receiver<crate::webview2::BrowserExitReceipt>,
) -> ProfileReleaseWait {
    ProfileReleaseWait::with_bounds(
        Some(receiver),
        test_binding(),
        None,
        ReleaseSite::Host,
        Duration::from_secs(30),
        Duration::from_secs(30),
    )
}

#[test]
fn queued_failure_behind_normal_credit_wins_before_exit() {
    let (sender, receiver) = release_channel();
    sender
        .send(normal_receipt())
        .expect("real owner input: normal credit");
    sender
        .send(abnormal_receipt())
        .expect("real owner input: following failure");
    let release = waiter(receiver);
    release.poll();
    assert!(
        release.observed.get(),
        "valid normal observation remains recorded"
    );
    assert_eq!(
        release.fault(),
        Some(ProfileReleaseFault::AbnormalExit),
        "a queued failure must not disappear behind normal release credit"
    );
}

#[test]
fn later_poll_still_drains_failure_after_normal_credit() {
    let (sender, receiver) = release_channel();
    let release = waiter(receiver);
    sender
        .send(normal_receipt())
        .expect("first delivered receipt");
    release.poll();
    assert!(release.observed.get());
    assert_eq!(release.fault(), None);
    let first = release.normal_exit_observed_at.get();
    sender
        .send(abnormal_receipt())
        .expect("new fault delivered before the next exit decision");
    release.poll();
    assert_eq!(
        release.fault(),
        Some(ProfileReleaseFault::AbnormalExit),
        "an already observed receipt cannot disable later fault polling"
    );
    assert_eq!(release.normal_exit_observed_at.get(), first);
}

#[test]
fn duplicate_normal_receipts_keep_the_original_timestamp() {
    let (sender, receiver) = release_channel();
    let release = waiter(receiver);
    let original = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("test clock");
    release.observed.set(true);
    release.normal_exit_observed_at.set(Some(original));
    sender.send(normal_receipt()).expect("duplicate one");
    sender.send(normal_receipt()).expect("duplicate two");
    release.poll();
    assert_eq!(
        release.normal_exit_observed_at.get(),
        Some(original),
        "a duplicate cannot renew grace"
    );
    assert_eq!(release.fault(), None);
    assert!(
        release
            .receiver
            .as_ref()
            .expect("receiver")
            .try_recv()
            .is_err(),
        "duplicate receipts were not actually drained"
    );
}

#[test]
fn the_first_fault_is_never_erased_by_a_later_normal_receipt() {
    let (sender, receiver) = release_channel();
    let release = waiter(receiver);
    release.fault.set(Some(ProfileReleaseFault::GraceExpired));
    sender.send(normal_receipt()).expect("late normal");
    release.poll();
    assert_eq!(release.fault(), Some(ProfileReleaseFault::GraceExpired));
    assert!(!release.observed.get());
}

#[test]
fn observer_disconnect_before_credit_fails_but_does_not_erase_delivered_credit() {
    for normal in [false, true] {
        let (sender, receiver) = release_channel();
        if normal {
            sender
                .send(normal_receipt())
                .expect("last valid event before sender drop");
        }
        drop(sender);
        let release = waiter(receiver);
        release.poll();
        assert_eq!(release.observed.get(), normal);
        assert_eq!(
            release.fault(),
            if normal {
                None
            } else {
                Some(ProfileReleaseFault::ObserverDropped)
            }
        );
    }
}

#[test]
fn host_abnormal_events_are_faults_independent_of_normal_pid_attribution() {
    for (expected, pid) in [(20, 10), (0, 10), (20, 0), (20, 20)] {
        assert!(
            browser_exit_is_relevant(expected, pid, false, ReleaseSite::Host),
            "host abnormal event {expected}/{pid} was discarded"
        );
    }
    assert!(!browser_exit_is_relevant(20, 10, true, ReleaseSite::Host));
    assert!(!browser_exit_is_relevant(0, 0, true, ReleaseSite::Host));
    assert!(browser_exit_is_relevant(20, 20, true, ReleaseSite::Host));
    assert!(
        !browser_exit_is_relevant(20, 10, false, ReleaseSite::Probe),
        "exclusive probe attribution is unchanged"
    );
    assert!(!browser_exit_is_relevant(0, 10, false, ReleaseSite::Probe));
    assert!(browser_exit_is_relevant(20, 20, false, ReleaseSite::Probe));
}

#[test]
fn blocking_exit_wait_checks_fault_before_normal_credit() {
    for site in [ReleaseSite::Host, ReleaseSite::Probe] {
        let (sender, receiver) = release_channel();
        sender
            .send(normal_receipt())
            .expect("queued normal receipt");
        sender
            .send(abnormal_receipt())
            .expect("queued fault after normal receipt");
        let release = ProfileReleaseWait::with_bounds(
            Some(receiver),
            test_binding(),
            None,
            site,
            Duration::from_secs(30),
            Duration::from_secs(30),
        );
        let error = crate::webview2::wait_for_browser_exit(&release)
            .expect_err("the blocking release consumer must prefer fault over normal credit");
        assert!(error.to_string().contains("KELD-WV-009"));
        assert_eq!(release.fault(), Some(ProfileReleaseFault::AbnormalExit));
    }
}
