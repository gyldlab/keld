//! Passive bounded metadata for actual original-document navigation/Post ordering.
#![forbid(unsafe_code)]
use super::super::original_navigation::OriginalNavigation;
use crate::WvError;
use std::cell::{Cell, OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
mod post;
pub(in crate::webview2) use post::{EventStorage, PostCarrier, PostObservation};

const ARM_PATH_CAPACITY: usize = 1024;
const MAX_RECORDS: u32 = 128;
/// The Rc headers, fixed observer and bounded arm pathname are paid by the
/// existing `OriginalView` metadata reservation before this observer is created.
pub(in crate::webview2) const METADATA_BYTES: usize =
    std::mem::size_of::<Probe>() + 2 * std::mem::size_of::<usize>() + ARM_PATH_CAPACITY;
const _: () = assert!(std::mem::align_of::<Probe>() <= std::mem::align_of::<usize>());

#[derive(Clone, Copy)]
pub(in crate::webview2) struct NavigationState {
    pub(in crate::webview2) view: u32,
    pub(in crate::webview2) generation: Option<u32>,
    pub(in crate::webview2) native_commit: Option<u64>,
    pub(in crate::webview2) callback_depth: u32,
    pub(in crate::webview2) posting: bool,
    pub(in crate::webview2) reentry_failed: bool,
}
impl NavigationState {
    fn value(self) -> serde_json::Value {
        serde_json::json!({
            "view":self.view,"generation":self.generation,"native_commit":self.native_commit,
            "callback_depth":self.callback_depth,"posting":self.posting,
            "reentry_failed":self.reentry_failed,
        })
    }
}

/// No payload, queue, timer, COM pointer, lease or release authority is stored.
pub(in crate::webview2) struct Probe {
    sequence: Cell<u64>,
    calls: Cell<u64>,
    active: Cell<bool>,
    in_call_records: Cell<u64>,
    in_call_max_depth: Cell<u32>,
    first_in_call_sequence: Cell<Option<u64>>,
    last_in_call_sequence: Cell<Option<u64>>,
    invalid: Cell<bool>,
    emitted: Cell<u32>,
    synthetic_arm: OnceCell<PathBuf>,
    synthetic_used: Cell<bool>,
}
impl Probe {
    pub(in crate::webview2) const fn new() -> Self {
        Self {
            sequence: Cell::new(0),
            calls: Cell::new(0),
            active: Cell::new(false),
            in_call_records: Cell::new(0),
            in_call_max_depth: Cell::new(0),
            first_in_call_sequence: Cell::new(None),
            last_in_call_sequence: Cell::new(None),
            invalid: Cell::new(false),
            emitted: Cell::new(0),
            synthetic_arm: OnceCell::new(),
            synthetic_used: Cell::new(false),
        }
    }
    pub(in crate::webview2) fn configure_synthetic(&self, directory: &Path) -> Result<(), WvError> {
        if !directory.is_absolute() || directory.as_os_str().len() > ARM_PATH_CAPACITY - 8 {
            return Err(WvError::Webview(
                "bounded absolute synthetic arm directory required".into(),
            ));
        }
        let mut arm = PathBuf::with_capacity(ARM_PATH_CAPACITY);
        arm.push(directory);
        arm.push("arm");
        if arm.capacity() > ARM_PATH_CAPACITY {
            return Err(WvError::Webview(
                "synthetic arm allocation exceeds fixed metadata".into(),
            ));
        }
        self.synthetic_arm
            .set(arm)
            .map_err(|_| WvError::Webview("synthetic arm already attached".into()))
    }
    fn next(&self) -> u64 {
        let previous = self.sequence.get();
        let next = previous.saturating_add(1);
        if previous == next {
            self.invalid.set(true);
        }
        self.sequence.set(next);
        next
    }
    /// Only infallible scalar reads/writes run at the sole native API boundary.
    fn enter_call(&self) -> u64 {
        if self.active.replace(true) {
            self.invalid.set(true);
        }
        let count = self.calls.get();
        if count == u64::MAX {
            self.invalid.set(true);
        }
        self.calls.set(count.saturating_add(1));
        self.in_call_records.set(0);
        self.in_call_max_depth.set(0);
        self.first_in_call_sequence.set(None);
        self.last_in_call_sequence.set(None);
        self.next()
    }
    fn return_call(&self) -> u64 {
        if !self.active.replace(false) {
            self.invalid.set(true);
        }
        self.next()
    }
    fn emit(&self, value: &serde_json::Value) {
        if !super::super::t2_test_observation::enabled() {
            return;
        }
        let count = self.emitted.get();
        if count > MAX_RECORDS {
            return;
        }
        self.emitted.set(count + 1);
        if count == MAX_RECORDS {
            eprintln!("KELD_X10_NAVIGATION_POST {{\"phase\":\"observation-overflow\"}}");
        } else {
            eprintln!("KELD_X10_NAVIGATION_POST {value}");
        }
    }
}

/// Capture scalars under the existing navigation borrow; output after it drops.
/// Actual in-call callbacks update only fixed aggregate metadata. Their presence
/// fails qualification and is flushed by the actual returning Post observation.
pub(in crate::webview2) fn navigation(
    state: &Rc<RefCell<OriginalNavigation>>,
    phase: &'static str,
    native_id: Option<u64>,
    admitted: Option<bool>,
) {
    observe(state, phase, native_id, admitted, None);
}

/// The existing event protocol has already accepted this exact consumer record.
/// Copy only its checked native stamp and delivery; this observation grants nothing.
pub(in crate::webview2) fn consumed(
    state: &Rc<RefCell<OriginalNavigation>>,
    stamp: crate::native_document::NativeCommittedStamp,
    delivery: u32,
) {
    observe(
        state,
        "consumer-observed",
        None,
        None,
        Some((stamp, delivery)),
    );
}

fn observe(
    state: &Rc<RefCell<OriginalNavigation>>,
    phase: &'static str,
    native_id: Option<u64>,
    admitted: Option<bool>,
    consumed: Option<(crate::native_document::NativeCommittedStamp, u32)>,
) {
    let Ok(navigation) = state.try_borrow() else {
        return;
    };
    let snapshot = navigation.post_observation();
    let probe = Rc::clone(navigation.post_observer());
    drop(navigation);
    let sequence = probe.next();
    if probe.active.get() {
        probe
            .in_call_records
            .set(probe.in_call_records.get().saturating_add(1));
        probe
            .in_call_max_depth
            .set(probe.in_call_max_depth.get().max(snapshot.callback_depth));
        if probe.first_in_call_sequence.get().is_none() {
            probe.first_in_call_sequence.set(Some(sequence));
        }
        probe.last_in_call_sequence.set(Some(sequence));
        return;
    }
    probe.emit(&serde_json::json!({
        "phase":phase,"sequence":sequence,"state":snapshot.value(),
        "native_id":native_id,"admitted":admitted,"call_active":false,
        "actual_api_calls":probe.calls.get(),"invalid":probe.invalid.get(),
        "consumed":consumed.map(|(stamp,delivery)| serde_json::json!({
            "view":stamp.view.0,"generation":stamp.generation.get(),
            "native_commit":stamp.native_commit.0.get(),"delivery":delivery,
        })),
    }));
}
