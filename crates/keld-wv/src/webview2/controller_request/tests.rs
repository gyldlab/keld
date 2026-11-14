//! Same request core/port controls. Native call/Close are modeled, not OS proof.
use super::state::{RequestCore, Retirement};
use super::*;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use windows::Win32::Foundation::E_ABORT;

struct ControllerWitness {
    allocation: Rc<Cell<u32>>,
    drops: Arc<AtomicUsize>,
}
impl Drop for ControllerWitness {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::AcqRel);
    }
}
fn controller() -> (ControllerWitness, Rc<Cell<u32>>, Arc<AtomicUsize>) {
    let allocation = Rc::new(Cell::new(9));
    let drops = Arc::new(AtomicUsize::new(0));
    (
        ControllerWitness {
            allocation: Rc::clone(&allocation),
            drops: Arc::clone(&drops),
        },
        allocation,
        drops,
    )
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn held(core: &RequestCore<ControllerWitness>, expected: &Rc<Cell<u32>>) {
    let Retirement::Resolved(Some(actual)) = core.retirement() else {
        panic!("actual resolved request expected")
    };
    assert!(Rc::ptr_eq(&actual.allocation, expected));
}
fn assert_unknown(core: &RequestCore<ControllerWitness>) {
    assert!(matches!(core.retirement(), Retirement::Unproved));
    assert!(core.completed_controller().is_none());
}

#[test]
fn callback_before_api_return_cannot_turn_an_unwound_call_into_success() {
    let (mut core, port) = RequestCore::new(deadline());
    let (controller, allocation, drops) = controller();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = core.begin_with(
            || {
                port.complete(Ok(()), Some(controller), |completion| {
                    assert!(completion.status().is_ok());
                    assert!(Rc::ptr_eq(
                        &completion.controller().unwrap().allocation,
                        &allocation
                    ));
                })
                .unwrap_or_else(|error| panic!("{error}"));
                panic!("modeled unwind before native API return");
            },
            || {},
        );
    }));
    assert!(outcome.is_err());
    assert_unknown(&core);
    assert!(core.entered_native());
    assert_eq!(drops.load(Ordering::Acquire), 0);
    drop(core); // Pure owner-lifetime witness; production retains whole engine.
    assert_eq!(drops.load(Ordering::Acquire), 1);
}

#[test]
fn original_deadline_before_entry_refuses_without_a_second_native_attempt() {
    let (mut core, _port) = RequestCore::<ControllerWitness>::new(Instant::now());
    let calls = Cell::new(0);
    assert!(
        core.begin_with(
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
            || {}
        )
        .is_err()
    );
    assert!(matches!(core.retirement(), Retirement::NeverEntered));
    assert!(!core.entered_native());
    assert!(
        core.begin_with(
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
            || {}
        )
        .is_err()
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn successful_request_wait_uses_original_clock_and_retains_same_interface() {
    let clock = deadline();
    let (mut core, port) = RequestCore::new(clock);
    let (controller, allocation, drops) = controller();
    let calls = Cell::new(0);
    core.begin_with(
        || {
            calls.set(calls.get() + 1);
            Ok(())
        },
        || {},
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_unknown(&core);
    core.wait_with(|receiver, observed_clock| {
        assert_eq!(observed_clock, clock);
        port.complete(Ok(()), Some(controller), |_| {}).unwrap();
        receiver.try_recv().unwrap();
        Ok(())
    })
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(Rc::ptr_eq(
        &core.completed_controller().unwrap().allocation,
        &allocation
    ));
    held(&core, &allocation);
    assert!(
        core.begin_with(
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
            || {}
        )
        .is_err()
    );
    assert!(core.wait_with(|_, _| panic!("no second pump")).is_err());
    assert_eq!(calls.get(), 1);
    assert_eq!(drops.load(Ordering::Acquire), 0);
}

#[test]
fn immediate_api_error_keeps_receiver_for_late_callback_and_never_builds() {
    let (mut core, port) = RequestCore::new(deadline());
    let (controller, allocation, drops) = controller();
    assert!(
        core.begin_with(|| Err(windows::core::Error::from(E_ABORT)), || {})
            .is_err()
    );
    assert_unknown(&core);
    assert!(
        core.wait_with(|_, _| panic!("an API error cannot begin another wait"))
            .is_err()
    );
    assert_eq!(drops.load(Ordering::Acquire), 0);
    // Success proves the ORIGINAL receiver still exists after the immediate Err.
    port.complete(
        Err(windows::core::Error::from(E_ABORT)),
        Some(controller),
        |_| {},
    )
    .unwrap();
    held(&core, &allocation);
    assert!(core.completed_controller().is_none());
    assert_eq!(drops.load(Ordering::Acquire), 0);
}

#[test]
fn timeout_then_late_callback_preserves_custody_without_success_or_second_pump() {
    let clock = deadline();
    let (mut core, port) = RequestCore::new(clock);
    let (controller, allocation, drops) = controller();
    core.begin_with(|| Ok(()), || {})
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        core.wait_with(|_, original_clock| {
            assert_eq!(original_clock, clock);
            Err(super::super::profile_failure_with(
                super::super::ProfileErrorKind::LifecycleUnproven,
                "modeled original deadline/pump refusal",
            ))
        })
        .is_err()
    );
    assert!(core.unproved_wait());
    assert_unknown(&core);
    port.complete(Ok(()), Some(controller), |_| {}).unwrap();
    // Actual callback storage allows only checked physical retirement, not build
    // success, implicit Close or another wait with a fresh deadline.
    held(&core, &allocation);
    assert!(core.completed_controller().is_none());
    assert!(
        core.wait_with(|_, _| panic!("a late callback cannot restart the pump"))
            .is_err()
    );
    assert!(core.unproved_wait());
    assert_eq!(drops.load(Ordering::Acquire), 0);
}

#[test]
fn error_status_interface_is_retained_before_status_interpretation() {
    let (mut core, port) = RequestCore::new(deadline());
    let (controller, allocation, drops) = controller();
    core.begin_with(
        || {
            port.complete(
                Err(windows::core::Error::from(E_ABORT)),
                Some(controller),
                |completion| {
                    assert_eq!(completion.status().as_ref().unwrap_err().code(), E_ABORT);
                    assert!(Rc::ptr_eq(
                        &completion.controller().unwrap().allocation,
                        &allocation
                    ));
                    assert_eq!(drops.load(Ordering::Acquire), 0);
                },
            )
            .unwrap_or_else(|error| panic!("{error}"));
            Ok(())
        },
        || {},
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        core.wait_with(|_, _| panic!("completion already retained"))
            .is_err()
    );
    held(&core, &allocation);
    assert!(core.completed_controller().is_none());
    assert_eq!(drops.load(Ordering::Acquire), 0);
}

#[test]
fn callback_observer_panic_is_contained_after_retaining_actual_interface() {
    let (mut core, port) = RequestCore::new(deadline());
    let (controller, _allocation, drops) = controller();
    core.begin_with(
        || {
            assert!(
                port.complete(Ok(()), Some(controller), |_| panic!("observer fault"))
                    .is_err()
            );
            Ok(())
        },
        || {},
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        core.wait_with(|_, _| panic!("retained callback must be inspected"))
            .is_err()
    );
    assert_unknown(&core);
    assert!(core.unproved_wait());
    assert_eq!(drops.load(Ordering::Acquire), 0);
}

#[test]
fn pump_unwind_keeps_request_unknown_even_when_callback_was_stored() {
    let (mut core, port) = RequestCore::new(deadline());
    let (controller, _allocation, drops) = controller();
    core.begin_with(|| Ok(()), || {})
        .unwrap_or_else(|error| panic!("{error}"));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = core.wait_with(|receiver, _| {
            port.complete(Ok(()), Some(controller), |_| {}).unwrap();
            receiver.try_recv().unwrap();
            panic!("modeled existing pump unwind");
        });
    }));
    assert!(outcome.is_err());
    assert_unknown(&core);
    assert!(core.unproved_wait());
    assert_eq!(drops.load(Ordering::Acquire), 0);
}

#[test]
fn missing_interface_refuses_build_but_actual_callback_resolves_request() {
    let (mut core, port) = RequestCore::<ControllerWitness>::new(deadline());
    core.begin_with(
        || {
            port.complete(Ok(()), None, |_| {}).unwrap();
            Ok(())
        },
        || {},
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        core.wait_with(|_, _| panic!("completion already retained"))
            .is_err()
    );
    assert!(matches!(core.retirement(), Retirement::Resolved(None)));
    assert!(core.completed_controller().is_none());
}
