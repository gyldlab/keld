//! Controller ownership at completion, channel loss and the shared deadline.
//! Pure controls only: fake interface counts never stand in for native acceptance.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{finish_wait, wrap_completion};
use crate::webview2::wait_for_launch_with_message_pump_until;

#[derive(Default)]
struct Trace {
    events: RefCell<Vec<&'static str>>,
    closes: Cell<usize>,
}

struct FakeController(Rc<Trace>);

impl Drop for FakeController {
    fn drop(&mut self) {
        self.0.events.borrow_mut().push("interface-drop");
    }
}

fn retire_unclaimed(controller: &FakeController) {
    controller.0.events.borrow_mut().push("unidentified");
    controller.0.events.borrow_mut().push("close");
    controller.0.closes.set(controller.0.closes.get() + 1);
}

fn builder_close(controller: &FakeController) {
    controller.0.events.borrow_mut().push("builder-close");
    controller.0.closes.set(controller.0.closes.get() + 1);
}

#[test]
fn failed_hresult_with_a_controller_closes_it_and_keeps_the_original_error() {
    let trace = Rc::new(Trace::default());
    let completion = wrap_completion(
        Err("original HRESULT"),
        Some(FakeController(Rc::clone(&trace))),
        retire_unclaimed,
        || "missing pointer",
    );
    assert!(matches!(completion, Err("original HRESULT")));
    assert_eq!(trace.closes.get(), 1);
    assert_eq!(
        *trace.events.borrow(),
        ["unidentified", "close", "interface-drop"]
    );
}

#[test]
fn a_queued_controller_is_closed_before_the_unresolved_deadline_fallback() {
    let trace = Rc::new(Trace::default());
    let (sender, receiver) = mpsc::channel();
    assert!(
        sender
            .send(wrap_completion(
                Ok::<(), &'static str>(()),
                Some(FakeController(Rc::clone(&trace))),
                retire_unclaimed,
                || "missing pointer",
            ))
            .is_ok()
    );
    // The real shared wait checks this expired deadline before receiving. No
    // native pump can run here; this is the previously unclaimed queue path.
    let waited = wait_for_launch_with_message_pump_until(&receiver, Instant::now());
    let result = finish_wait(waited, receiver, || {
        trace.events.borrow_mut().push("unresolved");
    });
    assert!(matches!(result, Err(ref error) if error.to_string().contains("KELD-WV-009")));
    assert_eq!(trace.closes.get(), 1);
    assert_eq!(
        *trace.events.borrow(),
        ["unidentified", "close", "interface-drop", "unresolved"],
        "the real first-fault cell must see the known controller before the fallback",
    );
}

#[test]
fn a_late_callback_after_timeout_still_closes_its_controller_once() {
    let trace = Rc::new(Trace::default());
    let (sender, receiver) = mpsc::channel();
    let waited = wait_for_launch_with_message_pump_until(&receiver, Instant::now());
    let result = finish_wait(waited, receiver, || {
        trace.events.borrow_mut().push("unresolved");
    });
    assert!(result.is_err());
    assert_eq!(*trace.events.borrow(), ["unresolved"]);
    let sent = sender
        .send(wrap_completion(
            Ok::<(), &'static str>(()),
            Some(FakeController(Rc::clone(&trace))),
            retire_unclaimed,
            || "missing pointer",
        ))
        .map_err(|_| "receiver ended");
    assert_eq!(sent, Err("receiver ended"));
    assert_eq!(trace.closes.get(), 1);
    assert_eq!(
        *trace.events.borrow(),
        ["unresolved", "unidentified", "close", "interface-drop"],
        "late cleanup cannot remove the earlier unresolved-request fault",
    );
}

#[test]
fn a_successful_delivery_transfers_close_responsibility_only_on_adoption() {
    let trace = Rc::new(Trace::default());
    let (sender, receiver) = mpsc::channel();
    assert!(
        sender
            .send(wrap_completion(
                Ok::<(), &'static str>(()),
                Some(FakeController(Rc::clone(&trace))),
                retire_unclaimed,
                || "missing pointer",
            ))
            .is_ok()
    );
    let waited =
        wait_for_launch_with_message_pump_until(&receiver, Instant::now() + Duration::from_secs(1));
    let result = finish_wait(waited, receiver, || panic!("completed request is resolved"));
    let guard = match result {
        Ok(Ok(guard)) => guard,
        _ => panic!("successful controller completion"),
    };
    assert!(
        trace.events.borrow().is_empty(),
        "receiving does not abandon the controller"
    );
    let controller = guard.take().expect("one adoption");
    assert!(
        trace.events.borrow().is_empty(),
        "adoption must not close or fault"
    );
    builder_close(&controller);
    drop(controller);
    assert_eq!(
        trace.closes.get(),
        1,
        "the later View/Controller custodian closes once"
    );
    assert_eq!(*trace.events.borrow(), ["builder-close", "interface-drop"]);
}

#[test]
fn missing_completion_pointer_and_failure_hresult_keep_distinct_errors() {
    let trace = Rc::new(Trace::default());
    for (status, expected) in [
        (Ok(()), "missing pointer"),
        (Err("original HRESULT"), "original HRESULT"),
    ] {
        let completion = wrap_completion::<FakeController, _>(
            status,
            None,
            retire_unclaimed,
            || "missing pointer",
        );
        assert!(matches!(completion, Err(error) if error == expected));
    }
    assert_eq!(trace.closes.get(), 0);
}
