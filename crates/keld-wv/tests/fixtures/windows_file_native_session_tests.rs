//! Synthetic cleanup decision checks. No native creation is submitted by these tests.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use super::Session;

fn check_pending_creation(pending_environment: bool) {
    let mut session = Session::new(
        Path::new("unused-pure-fixture-profile"),
        String::new(),
        Vec::new(),
    );
    session.cleanup_deadline = Instant::now();
    if pending_environment {
        session.pending_environment = Some(Rc::new(RefCell::new(None)));
    } else {
        session.pending_controller = Some(Rc::new(RefCell::new(None)));
    }
    let cleanup = session.shutdown();
    let pending = session.pending_environment.is_some() || session.pending_controller.is_some();
    // These phantom slots own no COM/window/profile resource and have no native
    // callback. Disarm them so this pure check cannot invoke the fatal Drop path.
    session.pending_environment = None;
    session.pending_controller = None;
    assert!(pending, "unresolved completion custody was discarded");
    assert_eq!(cleanup["fatal_process_exit_required"], true);
    for flag in [
        "controller_closed",
        "window_destroyed",
        "profile_removed",
        "com_uninitialized",
    ] {
        assert_eq!(
            cleanup[flag], false,
            "fatal pending callback was described as orderly cleanup: {flag}"
        );
    }
    assert!(
        cleanup["faults"]
            .as_array()
            .is_some_and(|faults| !faults.is_empty())
    );
}

#[test]
fn unresolved_environment_requires_process_exit_without_orderly_teardown() {
    check_pending_creation(true);
}

#[test]
fn unresolved_controller_requires_process_exit_without_orderly_teardown() {
    check_pending_creation(false);
}
