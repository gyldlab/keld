//! Per-STA fault controls compiled only into explicit native media test binaries.
#![forbid(unsafe_code)]
use std::cell::RefCell;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::webview2) enum Site {
    Read,
    Write,
}

#[derive(Default, Clone)]
pub(in crate::webview2) struct Trace {
    pub(in crate::webview2) build: Option<Instant>,
    pub(in crate::webview2) calls: Vec<(Site, Instant)>,
    pub(in crate::webview2) completed: Vec<(Site, bool)>,
}
#[derive(Default)]
struct State {
    active: bool,
    suppressed: Option<Site>,
    trace: Trace,
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

pub(in crate::webview2) struct Fault;
impl Fault {
    pub(in crate::webview2) fn arm(suppressed: Option<Site>) -> Self {
        STATE.with_borrow_mut(|state| {
            assert!(!state.active, "native fault must not nest");
            *state = State {
                active: true,
                suppressed,
                trace: Trace::default(),
            };
        });
        Self
    }
    pub(in crate::webview2) fn finish(self) -> Trace {
        let trace = STATE.with_borrow(|s| s.trace.clone());
        drop(self);
        trace
    }
}
impl Drop for Fault {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|s| *s = State::default());
    }
}
pub(in crate::webview2) fn record_build(deadline: Instant) {
    STATE.with_borrow_mut(|s| {
        if s.active {
            s.trace.build = Some(deadline);
        }
    });
}
pub(super) fn begin(site: Site, deadline: Instant) -> bool {
    STATE.with_borrow_mut(|s| {
        if s.active {
            s.trace.calls.push((site, deadline));
        }
        s.active && s.suppressed == Some(site)
    })
}
pub(super) fn completed(site: Site, success: bool) {
    STATE.with_borrow_mut(|s| {
        if s.active {
            s.trace.completed.push((site, success));
        }
    });
}
