// Copyright 2014-2021 The winit contributors
// Copyright 2021-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0

use std::{
  any::Any,
  cell::{Cell, RefCell},
  collections::{HashSet, VecDeque},
  mem, panic,
  rc::Rc,
  time::Instant,
};

use windows::Win32::{
  Foundation::HWND,
  Graphics::Gdi::{RedrawWindow, RDW_INTERNALPAINT},
};

use crate::{
  dpi::PhysicalSize,
  event::{Event, StartCause, WindowEvent},
  event_loop::ControlFlow,
  platform_impl::platform::util,
  window::WindowId,
};

pub(crate) type EventLoopRunnerShared<T> = Rc<EventLoopRunner<T>>;
pub(crate) struct EventLoopRunner<T: 'static> {
  // The event loop's win32 handles
  thread_msg_target: HWND,
  wait_thread_id: u32,

  control_flow: Cell<ControlFlow>,
  runner_state: Cell<RunnerState>,
  last_events_cleared: Cell<Instant>,

  event_handler: Cell<Option<Box<dyn FnMut(Event<'_, T>, &mut ControlFlow)>>>,
  event_buffer: RefCell<VecDeque<BufferedEvent<T>>>,

  owned_windows: Cell<HashSet<isize>>,

  panic_error: Cell<Option<PanicError>>,
}

pub type PanicError = Box<dyn Any + Send + 'static>;

/// See `move_state_to` function for details on how the state loop works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RunnerState {
  /// The event loop has just been created, and an `Init` event must be sent.
  Uninitialized,
  /// The event loop is idling.
  Idle,
  /// The event loop is handling the OS's events and sending them to the user's callback.
  /// `NewEvents` has been sent, and `MainEventsCleared` hasn't.
  HandlingMainEvents,
  /// The event loop is handling the redraw events and sending them to the user's callback.
  /// `MainEventsCleared` has been sent, and `RedrawEventsCleared` hasn't.
  HandlingRedrawEvents,
  /// The event loop has been destroyed. No other events will be emitted.
  Destroyed,
}

enum BufferedEvent<T: 'static> {
  Event(Event<'static, T>),
  ScaleFactorChanged(WindowId, f64, PhysicalSize<u32>),
}

impl<T> EventLoopRunner<T> {
  pub(crate) fn new(thread_msg_target: HWND, wait_thread_id: u32) -> EventLoopRunner<T> {
    EventLoopRunner {
      thread_msg_target,
      wait_thread_id,
      runner_state: Cell::new(RunnerState::Uninitialized),
      control_flow: Cell::new(ControlFlow::Poll),
      panic_error: Cell::new(None),
      last_events_cleared: Cell::new(Instant::now()),
      event_handler: Cell::new(None),
      event_buffer: RefCell::new(VecDeque::new()),
      owned_windows: Cell::new(HashSet::new()),
    }
  }

  pub(crate) unsafe fn set_event_handler<F>(&self, f: F)
  where
    F: FnMut(Event<'_, T>, &mut ControlFlow),
  {
    let old_event_handler = self.event_handler.replace(mem::transmute::<
      Option<Box<dyn FnMut(Event<'_, T>, &mut ControlFlow)>>,
      Option<Box<dyn FnMut(Event<'_, T>, &mut ControlFlow)>>,
    >(Some(Box::new(f))));
    assert!(old_event_handler.is_none());
  }

  pub(crate) fn reset_runner(&self) {
    let EventLoopRunner {
      thread_msg_target: _,
      wait_thread_id: _,
      runner_state,
      panic_error,
      control_flow,
      last_events_cleared: _,
      event_handler,
      event_buffer: _,
      owned_windows: _,
    } = self;
    runner_state.set(RunnerState::Uninitialized);
    panic_error.set(None);
    control_flow.set(ControlFlow::Poll);
    event_handler.set(None);
  }
}

/// State retrieval functions.
impl<T> EventLoopRunner<T> {
  pub fn thread_msg_target(&self) -> HWND {
    self.thread_msg_target
  }

  pub fn wait_thread_id(&self) -> u32 {
    self.wait_thread_id
  }

  pub fn redrawing(&self) -> bool {
    self.runner_state.get() == RunnerState::HandlingRedrawEvents
  }

  pub fn take_panic_error(&self) -> Result<(), PanicError> {
    match self.panic_error.take() {
      Some(err) => Err(err),
      None => Ok(()),
    }
  }

  pub fn control_flow(&self) -> ControlFlow {
    self.control_flow.get()
  }

  pub fn handling_events(&self) -> bool {
    self.runner_state.get() != RunnerState::Idle
  }

  pub fn should_buffer(&self) -> bool {
    let handler = self.event_handler.take();
    let should_buffer = handler.is_none();
    self.event_handler.set(handler);
    should_buffer
  }
}

/// Misc. functions
impl<T> EventLoopRunner<T> {
  pub fn catch_unwind<R>(&self, f: impl FnOnce() -> R) -> Option<R> {
    let panic_error = self.panic_error.take();
    if panic_error.is_none() {
      let result = panic::catch_unwind(panic::AssertUnwindSafe(f));

      // Check to see if the panic error was set in a re-entrant call to catch_unwind inside
      // of `f`. If it was, that error takes priority. If it wasn't, check if our call to
      // catch_unwind caught any panics and set panic_error appropriately.
      match self.panic_error.take() {
        None => match result {
          Ok(r) => Some(r),
          Err(e) => {
            self.panic_error.set(Some(e));
            None
          }
        },
        Some(e) => {
          self.panic_error.set(Some(e));
          None
        }
      }
    } else {
      self.panic_error.set(panic_error);
      None
    }
  }
  pub fn register_window(&self, window: HWND) {
    let mut owned_windows = self.owned_windows.take();
    owned_windows.insert(window.0 as _);
    self.owned_windows.set(owned_windows);
  }

  pub fn remove_window(&self, window: HWND) {
    let mut owned_windows = self.owned_windows.take();
    owned_windows.remove(&(window.0 as _));
    self.owned_windows.set(owned_windows);
  }

  pub fn owned_windows(&self, mut f: impl FnMut(HWND)) {
    let mut owned_windows = self.owned_windows.take();
    for hwnd in &owned_windows {
      f(HWND(*hwnd as _));
    }
    let new_owned_windows = self.owned_windows.take();
    owned_windows.extend(&new_owned_windows);
    self.owned_windows.set(owned_windows);
  }
}

/// Event dispatch functions.
impl<T> EventLoopRunner<T> {
  pub(crate) unsafe fn poll(&self) {
    self.move_state_to(RunnerState::HandlingMainEvents);
  }

  pub(crate) unsafe fn send_event(&self, event: Event<'_, T>) {
    // Buffer nested emissions while the callback owns event_handler. Native
    // pumps can queue ordinary events during redraw callbacks; those wait for
    // the next main phase. Direct redraw emissions use the same borrow barrier.
    if self.should_buffer() {
      self
        .event_buffer
        .borrow_mut()
        .push_back(BufferedEvent::from_event(event));
      return;
    }
    if let Event::RedrawRequested(_) = event {
      if self.runner_state.get() != RunnerState::HandlingRedrawEvents {
        // TODO: Consider removing log once https://github.com/rust-windowing/winit/pull/2767 gets adopted.
        debug!("RedrawRequested dispatched without explicit MainEventsCleared");
        self.move_state_to(RunnerState::HandlingRedrawEvents);
      }
      self.call_event_handler(event);
    } else {
      self.move_state_to(RunnerState::HandlingMainEvents);
      if self.redrawing() {
        // NewEvents can queue a redraw which starts a cluster while entering
        // the main phase. Keep this older ordinary event ahead of emissions
        // from that cluster, but do not deliver it until the cluster ends.
        self
          .event_buffer
          .borrow_mut()
          .push_front(BufferedEvent::from_event(event));
        return;
      }
      self.call_event_handler(event);
      self.dispatch_buffered_events();
    }
  }

  pub(crate) unsafe fn main_events_cleared(&self) {
    self.move_state_to(RunnerState::HandlingRedrawEvents);
  }

  pub(crate) unsafe fn redraw_events_cleared(&self) {
    self.move_state_to(RunnerState::Idle);
    // Nested pumps may have consumed ordinary events during the redraw phase.
    // Finish the whole redraw cluster before starting their main phase, even
    // under Wait where no further external wake is guaranteed. Keep Idle on
    // ExitWithCode so the outer loop can terminate at this boundary.
    let has_buffered_events = !self.event_buffer.borrow().is_empty();
    if has_buffered_events && !matches!(self.control_flow(), ControlFlow::ExitWithCode(_)) {
      self.move_state_to(RunnerState::HandlingMainEvents);
    }
  }

  pub(crate) unsafe fn loop_destroyed(&self) {
    self.move_state_to(RunnerState::Destroyed);
  }

  unsafe fn call_event_handler(&self, event: Event<'_, T>) {
    self.catch_unwind(|| {
            let mut control_flow = self.control_flow.take();
            let mut event_handler = self.event_handler.take()
                .expect("either event handler is re-entrant (likely), or no event handler is registered (very unlikely)");

            if let ControlFlow::ExitWithCode(code) = control_flow  {
                event_handler(event, &mut ControlFlow::ExitWithCode(code));
            } else {
                event_handler(event, &mut control_flow);
            }

            assert!(self.event_handler.replace(Some(event_handler)).is_none());
            self.control_flow.set(control_flow);
        });
  }

  unsafe fn dispatch_buffered_events(&self) {
    while !self.redrawing() {
      // We do this instead of using a `while let` loop because if we use a `while let`
      // loop the reference returned `borrow_mut()` doesn't get dropped until the end
      // of the loop's body and attempts to add events to the event buffer while in
      // `process_event` will fail.
      let buffered_event_opt = self.event_buffer.borrow_mut().pop_front();
      match buffered_event_opt {
        Some(e) => e.dispatch_event(|e| {
          if matches!(e, Event::RedrawRequested(_)) {
            self.move_state_to(RunnerState::HandlingRedrawEvents);
          }
          self.call_event_handler(e);
        }),
        None => break,
      }
    }
  }

  /// Dispatch control flow events (`NewEvents`, `MainEventsCleared`, `RedrawEventsCleared`, and
  /// `LoopDestroyed`) as necessary to bring the internal `RunnerState` to the new runner state.
  ///
  /// The state transitions are defined as follows:
  ///
  /// ```text
  ///    Uninitialized
  ///          |
  ///          V
  ///  HandlingMainEvents
  ///   ^            |
  ///   |            V
  /// Idle <--- HandlingRedrawEvents
  ///   |
  ///   V
  /// Destroyed
  /// ```
  ///
  /// Attempting to transition back to `Uninitialized` will result in a panic. Attempting to
  /// transition *from* `Destroyed` will also reuslt in a panic. Transitioning to the current
  /// state is a no-op. Even if the `new_runner_state` isn't the immediate next state in the
  /// runner state machine (e.g. `self.runner_state == HandlingMainEvents` and
  /// `new_runner_state == Idle`), the intermediate state transitions will still be executed.
  unsafe fn move_state_to(&self, new_runner_state: RunnerState) {
    use RunnerState::{Destroyed, HandlingMainEvents, HandlingRedrawEvents, Idle, Uninitialized};

    match (
      self.runner_state.replace(new_runner_state),
      new_runner_state,
    ) {
      (Uninitialized, Uninitialized)
      | (Idle, Idle)
      | (HandlingMainEvents, HandlingMainEvents)
      | (HandlingRedrawEvents, HandlingRedrawEvents)
      | (Destroyed, Destroyed) => (),

      // State transitions that initialize the event loop.
      (Uninitialized, HandlingMainEvents) => {
        self.call_new_events(true);
      }
      (Uninitialized, HandlingRedrawEvents) => {
        self.call_new_events(true);
        self.call_event_handler(Event::MainEventsCleared);
      }
      (Uninitialized, Idle) => {
        self.call_new_events(true);
        self.call_event_handler(Event::MainEventsCleared);
        self.call_redraw_events_cleared();
      }
      (Uninitialized, Destroyed) => {
        self.call_new_events(true);
        self.call_event_handler(Event::MainEventsCleared);
        self.call_redraw_events_cleared();
        self.call_event_handler(Event::LoopDestroyed);
      }
      (_, Uninitialized) => panic!("cannot move state to Uninitialized"),

      // State transitions that start the event handling process.
      (Idle, HandlingMainEvents) => {
        self.call_new_events(false);
      }
      (Idle, HandlingRedrawEvents) => {
        self.call_new_events(false);
        self.call_event_handler(Event::MainEventsCleared);
      }
      (Idle, Destroyed) => {
        self.call_event_handler(Event::LoopDestroyed);
      }

      (HandlingMainEvents, HandlingRedrawEvents) => {
        self.call_event_handler(Event::MainEventsCleared);
      }
      (HandlingMainEvents, Idle) => {
        // TODO: Consider removing log once https://github.com/rust-windowing/winit/pull/2767 gets adopted.
        debug!("RedrawEventsCleared emitted without explicit MainEventsCleared");
        self.call_event_handler(Event::MainEventsCleared);
        self.call_redraw_events_cleared();
      }
      (HandlingMainEvents, Destroyed) => {
        self.call_event_handler(Event::MainEventsCleared);
        self.call_redraw_events_cleared();
        self.call_event_handler(Event::LoopDestroyed);
      }

      (HandlingRedrawEvents, Idle) => {
        self.call_redraw_events_cleared();
      }
      (HandlingRedrawEvents, HandlingMainEvents) => {
        // TODO: Consider removing log once https://github.com/rust-windowing/winit/pull/2767 gets adopted.
        debug!("NewEvents emitted without explicit RedrawEventsCleared");
        self.call_redraw_events_cleared();
        self.call_new_events(false);
      }
      (HandlingRedrawEvents, Destroyed) => {
        self.call_redraw_events_cleared();
        self.call_event_handler(Event::LoopDestroyed);
      }

      (Destroyed, _) => panic!("cannot move state from Destroyed"),
    }
  }

  unsafe fn call_new_events(&self, init: bool) {
    let start_cause = match (init, self.control_flow()) {
      (true, _) => StartCause::Init,
      (false, ControlFlow::Poll) => StartCause::Poll,
      (false, ControlFlow::ExitWithCode(_)) | (false, ControlFlow::Wait) => {
        StartCause::WaitCancelled {
          requested_resume: None,
          start: self.last_events_cleared.get(),
        }
      }
      (false, ControlFlow::WaitUntil(requested_resume)) => {
        if Instant::now() < requested_resume {
          StartCause::WaitCancelled {
            requested_resume: Some(requested_resume),
            start: self.last_events_cleared.get(),
          }
        } else {
          StartCause::ResumeTimeReached {
            requested_resume,
            start: self.last_events_cleared.get(),
          }
        }
      }
    };
    self.call_event_handler(Event::NewEvents(start_cause));
    self.dispatch_buffered_events();
    let _ = RedrawWindow(Some(self.thread_msg_target), None, None, RDW_INTERNALPAINT);
  }

  unsafe fn call_redraw_events_cleared(&self) {
    self.call_event_handler(Event::RedrawEventsCleared);
    self.last_events_cleared.set(Instant::now());
  }
}

impl<T> BufferedEvent<T> {
  pub fn from_event(event: Event<'_, T>) -> BufferedEvent<T> {
    match event {
      Event::WindowEvent {
        event:
          WindowEvent::ScaleFactorChanged {
            scale_factor,
            new_inner_size,
          },
        window_id,
      } => BufferedEvent::ScaleFactorChanged(window_id, scale_factor, *new_inner_size),
      event => BufferedEvent::Event(event.to_static().unwrap()),
    }
  }

  pub fn dispatch_event(self, dispatch: impl FnOnce(Event<'_, T>)) {
    match self {
      Self::Event(event) => dispatch(event),
      Self::ScaleFactorChanged(window_id, scale_factor, mut new_inner_size) => {
        let os_inner_size = new_inner_size;

        dispatch(Event::WindowEvent {
          window_id,
          event: WindowEvent::ScaleFactorChanged {
            scale_factor,
            new_inner_size: &mut new_inner_size,
          },
        });

        if new_inner_size != os_inner_size {
          util::set_inner_size_physical(
            HWND(window_id.0 .0 as _),
            new_inner_size.width as _,
            new_inner_size.height as _,
            true,
          );
        }
      }
    }
  }
}

// These tests drive the runner directly to check event phases and borrowed-
// handler exclusion. Native modal/message causality requires separate OS proof.
#[cfg(all(test, target_os = "windows"))]
mod gh1188_nested_redraw_tests {
  use super::*;
  use std::cell::RefCell;

  #[test]
  fn ordinary_event_buffered_during_main_cleared_waits_for_main_phase() {
    for inject_at in ["main-cleared", "redraw-first", "redraw-cleared"] {
      // Use an invalid non-null target so call_new_events cannot request a
      // desktop-wide redraw; this runner does not own a native window.
      let runner = Rc::new(EventLoopRunner::<u8>::new(HWND(usize::MAX as _), 0));
      runner.runner_state.set(RunnerState::HandlingMainEvents);
      runner.control_flow.set(ControlFlow::Wait);
      let seen = Rc::new(RefCell::new(Vec::new()));
      let seen_for_handler = Rc::clone(&seen);
      let weak = Rc::downgrade(&runner);
      let first = WindowId(crate::platform_impl::platform::WindowId(1));
      let second = WindowId(crate::platform_impl::platform::WindowId(2));
      // SAFETY: all runner calls and owned callback state stay on this test
      // thread. This follows a two-window WM_PAINT flush sequence without
      // creating a window or dispatching a native message.
      unsafe {
        runner.set_event_handler(move |event, _| {
          let current = weak.upgrade().expect("runner retained");
          let label = match event {
            Event::MainEventsCleared => "main-cleared",
            Event::RedrawRequested(id) if id == first => "redraw-first",
            Event::RedrawRequested(id) if id == second => "redraw-second",
            Event::RedrawEventsCleared => "redraw-cleared",
            Event::NewEvents(_) => "new-events",
            Event::UserEvent(1) => "user-first",
            Event::UserEvent(2) => "user-second",
            Event::UserEvent(3) => "user-third",
            _ => return,
          };
          seen_for_handler
            .borrow_mut()
            .push((label, current.redrawing()));
          if label == inject_at {
            current.send_event(Event::UserEvent(1));
            current.send_event(Event::UserEvent(2));
          } else if label == "new-events" {
            current.send_event(Event::UserEvent(3));
          }
        });
        runner.main_events_cleared();
        runner.send_event(Event::RedrawRequested(first));
        runner.send_event(Event::RedrawRequested(second));
        assert_eq!(
          &*seen.borrow(),
          &[
            ("main-cleared", true),
            ("redraw-first", true),
            ("redraw-second", true)
          ],
          "an ordinary event buffered at {inject_at} must not interrupt the redraw phase",
        );
        runner.redraw_events_cleared();
      }
      assert!(runner.take_panic_error().is_ok());
      assert_eq!(
      &*seen.borrow(),
      &[
        ("main-cleared", true),
        ("redraw-first", true),
        ("redraw-second", true),
        ("redraw-cleared", false),
        ("new-events", false),
        ("user-first", false),
        ("user-second", false),
        ("user-third", false),
      ],
      "events buffered at {inject_at} must be delivered once in FIFO order without another external wake",
    );
    }
  }

  #[test]
  fn redraw_end_with_no_buffered_events_stays_idle() {
    let runner = Rc::new(EventLoopRunner::<()>::new(HWND(usize::MAX as _), 0));
    runner.runner_state.set(RunnerState::HandlingRedrawEvents);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen_for_handler = Rc::clone(&seen);
    // SAFETY: this test-thread runner owns its callback and no native window.
    unsafe {
      runner.set_event_handler(move |event, _| match event {
        Event::RedrawEventsCleared => seen_for_handler.borrow_mut().push("redraw-cleared"),
        Event::NewEvents(_) => seen_for_handler.borrow_mut().push("new-events"),
        _ => {}
      });
      runner.redraw_events_cleared();
    }
    assert!(runner.take_panic_error().is_ok());
    assert_eq!(&*seen.borrow(), &["redraw-cleared"]);
    assert_eq!(runner.runner_state.get(), RunnerState::Idle);
  }

  #[test]
  fn redraw_end_with_exit_does_not_start_another_main_phase() {
    for exit_at in ["redraw", "redraw-cleared"] {
      let runner = Rc::new(EventLoopRunner::<()>::new(HWND(usize::MAX as _), 0));
      runner.runner_state.set(RunnerState::HandlingMainEvents);
      let seen = Rc::new(RefCell::new(Vec::new()));
      let seen_for_handler = Rc::clone(&seen);
      let weak = Rc::downgrade(&runner);
      // SAFETY: all runner calls and owned callback state stay on this test
      // thread, without creating a native window or starting an event loop.
      unsafe {
        runner.set_event_handler(move |event, control_flow| {
          let label = match event {
            Event::MainEventsCleared => {
              weak
                .upgrade()
                .expect("runner retained")
                .send_event(Event::UserEvent(()));
              "main-cleared"
            }
            Event::RedrawRequested(_) => "redraw",
            Event::RedrawEventsCleared => "redraw-cleared",
            Event::NewEvents(_) => "new-events",
            Event::UserEvent(()) => "user-event",
            Event::LoopDestroyed => "loop-destroyed",
            _ => return,
          };
          seen_for_handler.borrow_mut().push(label);
          if label == exit_at {
            *control_flow = ControlFlow::ExitWithCode(42);
          } else if label == "redraw-cleared" {
            // ExitWithCode is sticky even if a later callback attempts to undo it.
            *control_flow = ControlFlow::Wait;
          }
        });
        runner.main_events_cleared();
        runner.send_event(Event::RedrawRequested(WindowId(
          crate::platform_impl::platform::WindowId(1),
        )));
        runner.redraw_events_cleared();
        assert_eq!(runner.runner_state.get(), RunnerState::Idle);
        assert_eq!(runner.control_flow(), ControlFlow::ExitWithCode(42));
        runner.loop_destroyed();
      }
      assert!(runner.take_panic_error().is_ok());
      assert_eq!(
        &*seen.borrow(),
        &["main-cleared", "redraw", "redraw-cleared", "loop-destroyed"]
      );
    }
  }

  #[test]
  fn redraw_end_retains_callback_panic_without_more_callbacks() {
    let runner = Rc::new(EventLoopRunner::<()>::new(HWND(usize::MAX as _), 0));
    runner.runner_state.set(RunnerState::HandlingRedrawEvents);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen_for_handler = Rc::clone(&seen);
    let weak = Rc::downgrade(&runner);
    // SAFETY: the test owns all state on one thread. The intentional callback
    // panic is caught by the runner; no native window or message loop is used.
    unsafe {
      runner.set_event_handler(move |event, _| match event {
        Event::RedrawEventsCleared => {
          seen_for_handler.borrow_mut().push("redraw-cleared");
          weak
            .upgrade()
            .expect("runner retained")
            .send_event(Event::UserEvent(()));
          panic!("redraw-end-callback-panic");
        }
        Event::NewEvents(_) => seen_for_handler.borrow_mut().push("new-events"),
        Event::UserEvent(()) => seen_for_handler.borrow_mut().push("user-event"),
        _ => {}
      });
      runner.redraw_events_cleared();
    }
    let error = runner
      .take_panic_error()
      .expect_err("callback panic must remain observable");
    assert_eq!(
      error.downcast_ref::<&str>(),
      Some(&"redraw-end-callback-panic")
    );
    assert_eq!(&*seen.borrow(), &["redraw-cleared"]);
  }

  #[test]
  fn nested_redraw_is_fifo_buffered_while_event_handler_is_active() {
    for nested in [false, true] {
      let runner = Rc::new(EventLoopRunner::<u8>::new(HWND(usize::MAX as _), 0));
      runner.runner_state.set(RunnerState::HandlingMainEvents);
      let seen = Rc::new(RefCell::new(Vec::new()));
      let seen_for_handler = Rc::clone(&seen);
      let weak = Rc::downgrade(&runner);
      let id = WindowId(crate::platform_impl::platform::WindowId(1));
      // SAFETY: the test owns the runner and callback on one thread. The
      // invalid non-null target cannot request a desktop-wide redraw.
      unsafe {
        runner.set_event_handler(move |event, _| {
          let current = weak.upgrade().expect("runner retained");
          match event {
            Event::UserEvent(1) => {
              current.send_event(Event::RedrawRequested(id));
              assert!(
                !current.redrawing(),
                "nested redraw must wait until the borrowed handler returns"
              );
              assert!(seen_for_handler.borrow().is_empty());
            }
            Event::MainEventsCleared => {
              seen_for_handler
                .borrow_mut()
                .push(("main-cleared", current.redrawing()));
            }
            Event::RedrawRequested(_) => {
              seen_for_handler
                .borrow_mut()
                .push(("redraw", current.redrawing()));
              current.send_event(Event::UserEvent(2));
            }
            Event::RedrawEventsCleared => {
              seen_for_handler
                .borrow_mut()
                .push(("redraw-cleared", current.redrawing()));
            }
            Event::NewEvents(_) => {
              seen_for_handler
                .borrow_mut()
                .push(("new-events", current.redrawing()));
            }
            Event::UserEvent(2) => {
              seen_for_handler
                .borrow_mut()
                .push(("ordinary", current.redrawing()));
            }
            _ => {}
          }
        });
        if nested {
          runner.send_event(Event::UserEvent(1));
        } else {
          runner.send_event(Event::RedrawRequested(id));
        }
        assert_eq!(
          &*seen.borrow(),
          &[("main-cleared", true), ("redraw", true)],
          "ordinary events must not interrupt either redraw delivery path"
        );
        runner.redraw_events_cleared();
      }
      assert!(runner.take_panic_error().is_ok());
      assert_eq!(
        &*seen.borrow(),
        &[
          ("main-cleared", true),
          ("redraw", true),
          ("redraw-cleared", false),
          ("new-events", false),
          ("ordinary", false),
        ]
      );
    }
  }

  #[test]
  fn new_events_nested_redraw_defers_the_outer_ordinary_event() {
    let runner = Rc::new(EventLoopRunner::<u8>::new(HWND(usize::MAX as _), 0));
    runner.runner_state.set(RunnerState::Idle);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen_for_handler = Rc::clone(&seen);
    let weak = Rc::downgrade(&runner);
    let id = WindowId(crate::platform_impl::platform::WindowId(1));
    let mut first = true;
    // SAFETY: one test thread owns the callback and invalid non-null target.
    unsafe {
      runner.set_event_handler(move |event, _| {
        let current = weak.upgrade().expect("runner retained");
        let label = match event {
          Event::NewEvents(_) => "new-events",
          Event::MainEventsCleared => "main-cleared",
          Event::RedrawRequested(_) => "redraw",
          Event::RedrawEventsCleared => "redraw-cleared",
          Event::UserEvent(1) => "outer",
          Event::UserEvent(2) => "nested",
          _ => return,
        };
        seen_for_handler
          .borrow_mut()
          .push((label, current.redrawing()));
        if label == "new-events" && first {
          first = false;
          current.send_event(Event::RedrawRequested(id));
        } else if label == "redraw" {
          current.send_event(Event::UserEvent(2));
        }
      });
      runner.send_event(Event::UserEvent(1));
      assert_eq!(
        &*seen.borrow(),
        &[("new-events", false), ("main-cleared", true), ("redraw", true)]
      );
      runner.redraw_events_cleared();
    }
    assert!(runner.take_panic_error().is_ok());
    assert_eq!(
      &*seen.borrow(),
      &[
        ("new-events", false),
        ("main-cleared", true),
        ("redraw", true),
        ("redraw-cleared", false),
        ("new-events", false),
        ("outer", false),
        ("nested", false),
      ]
    );
  }
}
