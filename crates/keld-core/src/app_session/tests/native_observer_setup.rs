//! Existing thread-affine Native scope/channel construction, shared by test owners.
use super::{FsWorkerTestGate, GuardedFsTestSetup, app_detail};
use keld_native::fs::acceptance_observer::{Hold, Snapshot, install_current_thread};
use std::any::Any;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

pub(super) const OBSERVATION_BOUND: Duration = Duration::from_secs(5);
const NATIVE_HOLD_BOUND: Duration = Duration::from_secs(10);

pub(super) struct NativeSignals {
    pub(super) initialized: Receiver<()>,
    pub(super) held: Receiver<Snapshot>,
    pub(super) returned: Receiver<Snapshot>,
    pub(super) final_snapshot: Receiver<Snapshot>,
    pub(super) native_release: Option<Sender<()>>,
    pub(super) pre_entry: Option<Receiver<()>>,
    pub(super) pre_release: Option<Sender<()>>,
}

pub(super) fn native_setup(
    hold_native: bool,
    pre_entry: bool,
) -> (GuardedFsTestSetup, NativeSignals) {
    let (initialized_tx, initialized) = mpsc::sync_channel(1);
    let (held_tx, held) = mpsc::sync_channel(1);
    let (returned_tx, returned) = mpsc::sync_channel(8);
    let (final_tx, final_snapshot) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let wait_control = hold_native.then(|| {
        Hold::new(held_tx, release_rx, NATIVE_HOLD_BOUND).expect("valid native fixture hold")
    });
    let (before_entry, pre_entry_rx, pre_release) = if pre_entry {
        let (taken, observed) = mpsc::sync_channel(1);
        let (release, receiver) = mpsc::channel();
        (
            Some(Arc::new(FsWorkerTestGate {
                taken,
                release: Mutex::new(receiver),
                before_entry: None,
                initializer: Mutex::new(None),
            })),
            Some(observed),
            Some(release),
        )
    } else {
        (None, None, None)
    };
    let setup: GuardedFsTestSetup = Box::new(move |gate| {
        gate.before_entry = before_entry;
        *gate.initializer.lock().expect("private initializer slot") = Some(Box::new(move || {
            assert_eq!(
                thread::current().name(),
                Some("keld-core-fs-worker"),
                "native scope installed on a different execution owner"
            );
            let guard =
                install_current_thread(wait_control, returned_tx, final_tx).ok_or_else(|| {
                    app_detail("native observer fixture", "scope installation refused")
                })?;
            println!(
                "KELD_AC7_OBSERVER_INSTALLED pid={} worker={:?}",
                std::process::id(),
                thread::current().id()
            );
            initialized_tx
                .send(())
                .map_err(|_| app_detail("native observer fixture", "setup witness disconnected"))?;
            Ok(Box::new(guard) as Box<dyn Any>)
        }));
    });
    (
        setup,
        NativeSignals {
            initialized,
            held,
            returned,
            final_snapshot,
            native_release: hold_native.then_some(release_tx),
            pre_entry: pre_entry_rx,
            pre_release,
        },
    )
}
