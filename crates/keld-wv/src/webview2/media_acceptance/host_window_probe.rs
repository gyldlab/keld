//! Test-only observer/stimulus hook after a genuine application-loop dispatch.
//! It never alters the production event or supplies a native result to the backend.

use super::super::{View, WindowsLoopEvent};
use std::cell::RefCell;
use std::collections::BTreeMap;
use tao::event::{Event, WindowEvent};
use tao::event_loop::ControlFlow;
use tao::window::WindowId;

#[derive(Clone, Copy, Debug)]
pub(in crate::webview2) enum Point {
    Idle,
    Resized(WindowId),
    Moved(WindowId),
    Focused(WindowId, bool),
    Other,
}

impl Point {
    pub(in crate::webview2) fn capture(event: &Event<'_, WindowsLoopEvent>) -> Self {
        match event {
            Event::MainEventsCleared => Self::Idle,
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Resized(_),
                ..
            } => Self::Resized(*window_id),
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Moved(_),
                ..
            } => Self::Moved(*window_id),
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Focused(value),
                ..
            } => Self::Focused(*window_id, *value),
            _ => Self::Other,
        }
    }
}

type Callback = Box<dyn FnMut(Point, &BTreeMap<u32, View>, &mut ControlFlow)>;
thread_local! { static OBSERVER: RefCell<Option<Callback>> = const {RefCell::new(None)}; }

pub(super) struct Installed;
impl Drop for Installed {
    fn drop(&mut self) {
        OBSERVER.with_borrow_mut(|slot| {
            slot.take();
        });
    }
}

pub(super) fn install(callback: Callback) -> Installed {
    OBSERVER.with_borrow_mut(|slot| {
        assert!(slot.is_none(), "one UI observer at a time");
        *slot = Some(callback);
    });
    Installed
}

pub(in crate::webview2) fn after_dispatch(
    point: Point,
    views: &BTreeMap<u32, View>,
    flow: &mut ControlFlow,
) {
    // Take while calling native stimulus: any nested message dispatch cannot
    // alias a mutable closure. Tao delivers the queued event on a later turn.
    let callback = OBSERVER.with_borrow_mut(Option::take);
    if let Some(mut callback) = callback {
        callback(point, views, flow);
        OBSERVER.with_borrow_mut(|slot| {
            *slot = Some(callback);
        });
    }
}
