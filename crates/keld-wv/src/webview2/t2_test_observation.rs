//! Debug-only counters at actual native creation owners (GH1188 D16).
#![forbid(unsafe_code)]

use std::cell::Cell;

use crate::engine::AppWindowCommand;

#[derive(Clone, Copy, Default)]
struct Counts {
    observing: bool,
    environments: u64,
    controllers: u64,
    releasing: usize,
    armed_by: Option<&'static str>,
}

thread_local! {
    static COUNTS: Cell<Counts> = const { Cell::new(Counts {
        observing: false, environments: 0, controllers: 0, releasing: 0, armed_by: None,
    }) };
}

pub(super) fn begin() {
    let observing = std::env::var_os(super::TEST_WINDOWS_T2_OBSERVE_ENV).as_deref()
        == Some(std::ffi::OsStr::new("1"));
    COUNTS.set(Counts {
        observing,
        ..Counts::default()
    });
    if observing {
        eprintln!("KELD_T2_ENGINE_BEGIN");
    }
}

pub(super) fn environment_requested() {
    COUNTS.update(|mut counts| {
        counts.environments += 1;
        counts
    });
}

pub(super) fn controller_requested() {
    COUNTS.update(|mut counts| {
        counts.controllers += 1;
        counts
    });
}

pub(super) fn enabled() -> bool {
    COUNTS.get().observing
}

pub(super) fn releasing(count: usize) {
    COUNTS.update(|mut counts| {
        counts.releasing = count;
        counts
    });
}

pub(super) fn terminal(command: &'static str) {
    let counts = COUNTS.get();
    if counts.observing {
        eprintln!("TERMINAL {command} releasing={}", counts.releasing);
    }
}

/// Only an observed transition of the actual barrier attributes arming to a
/// terminal command. A dequeue or a later fault cannot manufacture this credit.
pub(super) fn terminal_arming(command: &'static str, was_armed: bool, is_armed: bool) {
    if was_armed || !is_armed {
        return;
    }
    COUNTS.update(|mut counts| {
        if counts.armed_by.is_none() {
            counts.armed_by = Some(command);
        }
        counts
    });
}

pub(super) fn loop_ended(barrier_armed: bool) {
    let counts = COUNTS.get();
    if !counts.observing {
        return;
    }
    eprintln!(
        "KELD_T2_LOOP_EXIT environments={} controllers={} armed_by={} releasing={}",
        counts.environments,
        counts.controllers,
        if barrier_armed {
            counts.armed_by.unwrap_or("none")
        } else {
            "none"
        },
        counts.releasing,
    );
}

pub(super) fn browser_rebound(instance: super::browser_instance::BrowserInstance) {
    if COUNTS.get().observing {
        eprintln!("REBIND gen={} pid={}", instance.generation, instance.pid);
    }
}

pub(super) fn receipt_credited(instance: super::browser_instance::BrowserInstance) {
    if COUNTS.get().observing {
        eprintln!(
            "RECEIPT_CREDITED gen={} pid={}",
            instance.generation, instance.pid
        );
    }
}

pub(super) fn abnormal(
    instance: super::browser_instance::BrowserInstance,
    source: super::browser_instance::BrowserExitSource,
) {
    if COUNTS.get().observing {
        eprintln!(
            "ABNORMAL source={} gen={} pid={}",
            source.name(),
            instance.generation,
            instance.pid
        );
    }
}

/// Diagnostics at existing call boundaries only; no admission or release credit.
pub(super) fn native_release_step(step: &'static str, result: Option<i32>) {
    if enabled() {
        eprintln!("KELD_D16_RELEASE_STEP step={step} result={result:?}");
    }
}

pub(super) fn foreign_normal(current: super::browser_instance::BrowserInstance, pid: u32) {
    if COUNTS.get().observing {
        eprintln!(
            "FOREIGN_NORMAL gen={} expected={} pid={pid}",
            current.generation, current.pid
        );
    }
}

pub(super) fn bridge_posted(observing: bool, command: AppWindowCommand) {
    if !observing {
        return;
    }
    let name = match command {
        AppWindowCommand::CreateWindow { .. } => "CreateWindow",
        AppWindowCommand::DestroyWindow { .. } => "DestroyWindow",
        AppWindowCommand::SetMaximized { .. } => "SetMaximized",
        AppWindowCommand::SetFullScreen { .. } => "SetFullScreen",
        AppWindowCommand::Quit => "Quit",
        AppWindowCommand::Fatal => "Fatal",
    };
    eprintln!("BRIDGE_POSTED {name}");
}

pub(super) fn destroyed(hwnd: isize) {
    if enabled() {
        eprintln!("DESTROYED {hwnd}");
    }
}
pub(super) fn teardown_confirmed(hwnd: isize) {
    if enabled() {
        eprintln!("TEARDOWN_CONFIRMED {hwnd}");
    }
}
pub(super) fn released(view: u32) {
    if enabled() {
        eprintln!("RELEASED {view}");
    }
}
pub(super) fn close_dispatched(hwnd: isize) {
    if enabled() {
        eprintln!("CLOSE_DISPATCHED {hwnd}");
    }
}
pub(super) fn build_deferred(build: u64) {
    if enabled() {
        eprintln!("BUILD_DEFERRED {build}");
    }
}
pub(super) fn build_refused(build: u64) {
    if enabled() {
        eprintln!("BUILD_REFUSED {build}");
    }
}
pub(super) fn parent_moved(view: u32) {
    if enabled() {
        eprintln!("PARENT_MOVED {view}");
    }
}

pub(super) fn page_scheduled(deadline: std::time::Instant) {
    if enabled() {
        eprintln!(
            "KELD_D16_PAGE_SCHEDULE now={:?} deadline={deadline:?}",
            std::time::Instant::now()
        );
    }
}

#[cfg(test)]
#[path = "tests/t2_arming_observation.rs"]
mod tests;
