// Deterministic helper contract proof. Not native/Core acceptance.
// The generated report modules contain exact source-extracted helper bodies.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, SendError};

thread_local! { static CLOCK: Cell<u64> = const { Cell::new(0) }; }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Instant(u64);
impl Instant {
    fn now() -> Self {
        Self(CLOCK.with(Cell::get))
    }
}
fn clock(value: u64) {
    CLOCK.with(|c| c.set(value));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AppDesktopFacts(u32);

#[derive(Clone, Debug, PartialEq, Eq)]
enum AppWindowEvent {
    DesktopObserved { facts: Option<AppDesktopFacts> },
    HostInitialized { observed_at: Instant },
}

struct Sender<T> {
    inner: mpsc::Sender<T>,
    receiver: Rc<RefCell<Option<Receiver<T>>>>,
    attempts: Rc<RefCell<Vec<AppWindowEvent>>>,
    enqueued: Rc<RefCell<Vec<AppWindowEvent>>>,
    advance_after_desktop: Option<u64>,
    disconnect_after_desktop: bool,
}
impl Sender<AppWindowEvent> {
    fn send(&self, event: AppWindowEvent) -> Result<(), SendError<AppWindowEvent>> {
        self.attempts.borrow_mut().push(event.clone());
        let desktop = matches!(event, AppWindowEvent::DesktopObserved { .. });
        let observed = event.clone();
        let result = self.inner.send(event);
        if result.is_ok() {
            self.enqueued.borrow_mut().push(observed);
            if desktop {
                if let Some(after) = self.advance_after_desktop {
                    clock(after);
                }
                if self.disconnect_after_desktop {
                    self.receiver.borrow_mut().take();
                }
            }
        }
        result
    }
}

type Capture = Box<dyn FnOnce() -> Option<AppDesktopFacts>>;

fn pair(advance: Option<u64>, disconnect: bool) -> Sender<AppWindowEvent> {
    let (inner, receiver) = mpsc::channel();
    Sender {
        inner,
        receiver: Rc::new(RefCell::new(Some(receiver))),
        attempts: Rc::new(RefCell::new(Vec::new())),
        enqueued: Rc::new(RefCell::new(Vec::new())),
        advance_after_desktop: advance,
        disconnect_after_desktop: disconnect,
    }
}
fn receive(sender: &Sender<AppWindowEvent>) -> Vec<AppWindowEvent> {
    sender
        .receiver
        .borrow()
        .as_ref()
        .expect("receiver is present")
        .try_iter()
        .collect()
}
fn sample(at: u64, value: Option<AppDesktopFacts>, calls: Rc<Cell<u32>>) -> Capture {
    Box::new(move || {
        calls.set(calls.get() + 1);
        clock(at);
        value
    })
}
fn init(at: u64) -> AppWindowEvent {
    AppWindowEvent::HostInitialized {
        observed_at: Instant(at),
    }
}
fn desktop(value: Option<u32>) -> AppWindowEvent {
    AppWindowEvent::DesktopObserved {
        facts: value.map(AppDesktopFacts),
    }
}

// W4_EXTRACTED_HELPER_MODULES

fn report(events: &Sender<AppWindowEvent>, capture: Option<Capture>) -> bool {
    match std::env::var("W4_SEAM_VARIANT").as_deref() {
        Ok("v2") => v2::report_host_initialized(events, capture),
        Ok("v3") => v3::report_host_initialized(events, capture),
        Ok("before_sample") => before_sample::report_host_initialized(events, capture),
        Ok("noop") => noop::report_host_initialized(events, capture),
        other => panic!("unrecognized isolated test variant: {other:?}"),
    }
}

#[test]
fn timely_capture_keeps_timestamp_across_post_enqueue_preemption() {
    clock(14_900);
    let sender = pair(Some(15_001), false);
    let calls = Rc::new(Cell::new(0));
    assert!(report(
        &sender,
        Some(sample(14_999, Some(AppDesktopFacts(7)), calls.clone()))
    ));
    assert_eq!(receive(&sender), vec![desktop(Some(7)), init(14_999)]);
    assert_eq!(
        Instant::now(),
        Instant(15_001),
        "the injected interruption actually advanced time"
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn genuinely_slow_capture_is_not_backdated() {
    clock(14_999);
    let sender = pair(None, false);
    let calls = Rc::new(Cell::new(0));
    assert!(report(
        &sender,
        Some(sample(15_001, Some(AppDesktopFacts(8)), calls.clone()))
    ));
    assert_eq!(receive(&sender), vec![desktop(Some(8)), init(15_001)]);
    assert_eq!(calls.get(), 1);
}

#[test]
fn absent_callback_emits_only_init() {
    clock(31);
    let sender = pair(None, false);
    assert!(report(&sender, None));
    assert_eq!(receive(&sender), vec![init(31)]);
}

#[test]
fn unavailable_callback_still_emits_desktop_none_before_init() {
    clock(40);
    let sender = pair(None, false);
    let calls = Rc::new(Cell::new(0));
    assert!(report(&sender, Some(sample(41, None, calls.clone()))));
    assert_eq!(receive(&sender), vec![desktop(None), init(41)]);
    assert_eq!(calls.get(), 1);
}

#[test]
fn populated_callback_is_captured_once_in_desktop_init_order() {
    clock(50);
    let sender = pair(None, false);
    let calls = Rc::new(Cell::new(0));
    assert!(report(
        &sender,
        Some(sample(51, Some(AppDesktopFacts(9)), calls.clone()))
    ));
    assert_eq!(receive(&sender), vec![desktop(Some(9)), init(51)]);
    assert_eq!(calls.get(), 1);
    assert_eq!(sender.attempts.borrow().len(), 2);
}

#[test]
fn disconnected_receiver_refuses_before_attempting_init() {
    clock(60);
    let sender = pair(None, false);
    sender.receiver.borrow_mut().take();
    let calls = Rc::new(Cell::new(0));
    assert!(!report(
        &sender,
        Some(sample(61, Some(AppDesktopFacts(10)), calls.clone()))
    ));
    assert_eq!(*sender.attempts.borrow(), vec![desktop(Some(10))]);
    assert!(sender.enqueued.borrow().is_empty());
    assert_eq!(calls.get(), 1);
}

#[test]
fn disconnection_after_desktop_refuses_init() {
    clock(70);
    let sender = pair(None, true);
    let calls = Rc::new(Cell::new(0));
    assert!(!report(
        &sender,
        Some(sample(71, Some(AppDesktopFacts(11)), calls.clone()))
    ));
    assert_eq!(*sender.attempts.borrow(), vec![desktop(Some(11)), init(71)]);
    assert_eq!(*sender.enqueued.borrow(), vec![desktop(Some(11))]);
    assert!(sender.receiver.borrow().is_none());
    assert_eq!(calls.get(), 1);
}

#[test]
fn no_sampler_also_refuses_a_disconnected_receiver() {
    clock(80);
    let sender = pair(None, false);
    sender.receiver.borrow_mut().take();
    assert!(!report(&sender, None));
    assert_eq!(*sender.attempts.borrow(), vec![init(80)]);
    assert!(sender.enqueued.borrow().is_empty());
}
