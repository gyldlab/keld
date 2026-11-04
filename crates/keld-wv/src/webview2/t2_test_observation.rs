//! Debug-only counters at actual native creation owners (GH1188 D16).
#![forbid(unsafe_code)]

use std::cell::Cell;

#[derive(Clone, Copy, Default)]
struct Counts {
    observing: bool,
    environments: u64,
    controllers: u64,
    armed_by: Option<&'static str>,
}

thread_local! {
    static COUNTS: Cell<Counts> = const { Cell::new(Counts {
        observing: false, environments: 0, controllers: 0, armed_by: None,
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

pub(super) fn terminal(command: &'static str) {
    COUNTS.update(|mut counts| {
        if counts.armed_by.is_none() {
            counts.armed_by = Some(command);
        }
        counts
    });
}

pub(super) fn loop_ended() {
    let counts = COUNTS.get();
    if !counts.observing {
        return;
    }
    eprintln!(
        "KELD_T2_LOOP_EXIT environments={} controllers={} armed_by={}",
        counts.environments,
        counts.controllers,
        counts.armed_by.unwrap_or("none")
    );
}
