//! Actual barrier transitions, without a Window, COM object or process handle.
//! These units test observation provenance; native AC14 tests the real call site.

use super::super::browser_instance::{BrowserBinding, BrowserExitReceipt};
use super::super::{ProfileReleaseFault, ProfileReleaseWait, ReleaseSite};
use super::{COUNTS, Counts, terminal, terminal_arming};
use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

struct ObservationScope(Counts);

impl ObservationScope {
    fn enter() -> Self {
        let previous = COUNTS.get();
        COUNTS.set(Counts {
            observing: true,
            ..Counts::default()
        });
        Self(previous)
    }
}

impl Drop for ObservationScope {
    fn drop(&mut self) {
        COUNTS.set(self.0);
    }
}

fn unbound_release() -> ProfileReleaseWait {
    ProfileReleaseWait::new(
        None,
        Arc::new(BrowserBinding::new()),
        None,
        ReleaseSite::Host,
    )
}

#[test]
fn dequeue_without_actual_arm_never_records_terminal_provenance() {
    let _observation = ObservationScope::enter();
    let release = unbound_release();

    terminal("Quit");
    assert_eq!(
        COUNTS.get().armed_by,
        None,
        "TERMINAL is a dequeue receipt, not a barrier-arm receipt",
    );
    let was_armed = release.armed_at.get().is_some();
    // The omitted-operation control deliberately does not call release.arm().
    terminal_arming("Quit", was_armed, release.armed_at.get().is_some());
    assert_eq!(release.armed_at.get(), None);
    assert_eq!(
        COUNTS.get().armed_by,
        None,
        "omitting the actual arm must remain visible to the observer",
    );
}

#[test]
fn a_zero_controller_quit_records_only_its_real_arm() {
    let _observation = ObservationScope::enter();
    let release = unbound_release();

    terminal("Quit");
    let was_armed = release.armed_at.get().is_some();
    assert!(!was_armed);
    release.arm();
    terminal_arming("Quit", was_armed, release.armed_at.get().is_some());

    assert!(release.armed_at.get().is_some());
    assert_eq!(COUNTS.get().armed_by, Some("Quit"));
}

#[test]
fn a_last_browser_receipt_does_not_replace_the_listener_quit_arm() {
    let _observation = ObservationScope::enter();
    let binding = Arc::new(BrowserBinding::new());
    binding.bind_once(41).expect("pure nonzero instance stamp");
    let (sender, receiver) = mpsc::channel();
    let release = ProfileReleaseWait::new(
        Some(receiver),
        Arc::clone(&binding),
        None,
        ReleaseSite::Host,
    );
    sender
        .send(BrowserExitReceipt::Normal(binding.load()))
        .expect("current normal receipt");
    release.poll();
    assert!(
        release.observed.get(),
        "the listener can keep an exited browser's session open"
    );
    assert_eq!(release.armed_at.get(), None, "normal credit does not arm");

    terminal("Quit");
    let was_armed = release.armed_at.get().is_some();
    release.arm();
    terminal_arming("Quit", was_armed, release.armed_at.get().is_some());

    assert!(release.armed_at.get().is_some());
    assert_eq!(COUNTS.get().armed_by, Some("Quit"));
}

#[test]
fn a_prior_legacy_or_fault_arm_is_not_relabelled_by_terminal_dequeue() {
    for fault in [false, true] {
        let _observation = ObservationScope::enter();
        let release = unbound_release();
        if fault {
            release.latch_fault(ProfileReleaseFault::AbnormalExit);
            assert!(release.poll_host_receipts(&mut BTreeMap::new(), None));
        } else {
            // Legacy CloseRequested uses this same arm before the later Quit.
            release.arm();
        }
        let first = release.armed_at.get().expect("the earlier owner armed");
        terminal("Quit");
        let was_armed = release.armed_at.get().is_some();
        release.arm();
        terminal_arming("Quit", was_armed, release.armed_at.get().is_some());

        assert_eq!(release.armed_at.get(), Some(first));
        assert_eq!(
            COUNTS.get().armed_by,
            None,
            "a terminal cannot claim an already-armed barrier",
        );
    }
}

#[test]
fn a_later_fault_cannot_cover_an_omitted_terminal_arm() {
    let _observation = ObservationScope::enter();
    let release = unbound_release();

    terminal("Quit");
    let was_armed = release.armed_at.get().is_some();
    terminal_arming("Quit", was_armed, release.armed_at.get().is_some());
    assert_eq!(COUNTS.get().armed_by, None);

    release.latch_fault(ProfileReleaseFault::AbnormalExit);
    assert!(release.poll_host_receipts(&mut BTreeMap::new(), None));
    assert!(
        release.armed_at.get().is_some(),
        "the actual later fault owner armed"
    );
    assert_eq!(
        COUNTS.get().armed_by,
        None,
        "a later fault cannot manufacture the missing Quit provenance",
    );
}

#[test]
fn the_first_real_terminal_cause_survives_fault_and_repeated_terminal_calls() {
    for (first, later) in [("Quit", "Fatal"), ("Fatal", "Quit")] {
        let _observation = ObservationScope::enter();
        let release = unbound_release();

        terminal(first);
        let was_armed = release.armed_at.get().is_some();
        release.arm();
        terminal_arming(first, was_armed, release.armed_at.get().is_some());
        let armed = release.armed_at.get().expect("the first terminal armed");
        assert_eq!(COUNTS.get().armed_by, Some(first));

        release.latch_fault(ProfileReleaseFault::AbnormalExit);
        assert!(release.poll_host_receipts(&mut BTreeMap::new(), None));
        terminal(later);
        let was_armed = release.armed_at.get().is_some();
        release.arm();
        terminal_arming(later, was_armed, release.armed_at.get().is_some());

        assert_eq!(
            release.armed_at.get(),
            Some(armed),
            "the bound did not renew"
        );
        assert_eq!(COUNTS.get().armed_by, Some(first));
    }
}
