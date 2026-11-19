//! One labelled fault appended after a real native matching normal callback.
#![forbid(unsafe_code)]

use crate::webview2::{
    BrowserExitReceipt, BrowserExitSource, BrowserInstance, ReleaseSite, browser_exit_is_relevant,
};
use std::cell::Cell;
use std::sync::mpsc::Sender;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Healthy,
    SamePidFault,
    ForeignPidFault,
}

thread_local! {
    static MODE: Cell<Option<Mode>> = const { Cell::new(None) };
    static NATIVE_RECEIPTS: Cell<u32> = const { Cell::new(0) };
    static FAULTS_SENT: Cell<u32> = const { Cell::new(0) };
}

pub(crate) struct Scope;
impl Scope {
    pub(crate) fn enter(mode: Mode) -> Self {
        MODE.with(|slot| {
            assert!(
                slot.replace(Some(mode)).is_none(),
                "overlapping receipt fixture"
            );
        });
        NATIVE_RECEIPTS.set(0);
        FAULTS_SENT.set(0);
        Self
    }
    pub(crate) fn observations() -> (u32, u32) {
        (NATIVE_RECEIPTS.get(), FAULTS_SENT.get())
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        MODE.set(None);
    }
}

pub(in crate::webview2) fn after_native_receipt(
    sender: &Sender<BrowserExitReceipt>,
    pid: u32,
    current: BrowserInstance,
    normal: bool,
    site: ReleaseSite,
) -> windows::core::Result<()> {
    let expected = current.pid;
    let Some(mode) = MODE.get() else {
        return Ok(());
    };
    if !matches!(site, ReleaseSite::Host) || !normal || pid == 0 || pid != expected {
        return Ok(());
    }
    NATIVE_RECEIPTS.set(NATIVE_RECEIPTS.get() + 1);
    println!("KELD_RELEASE_NATIVE_NORMAL pid={pid} expected={expected} mode={mode:?}");
    if mode == Mode::Healthy {
        return Ok(());
    }
    // This is an explicitly injected attribution field, not a real second crash.
    // It never opens, signals or claims authority over another process.
    let fault_pid = if mode == Mode::ForeignPidFault {
        if expected == 1 { 2 } else { 1 }
    } else {
        pid
    };
    let admitted = browser_exit_is_relevant(expected, fault_pid, false, site);
    println!(
        "KELD_RELEASE_INJECTED_FAULT normal_pid={pid} fault_pid={fault_pid} admitted={admitted}"
    );
    if admitted {
        sender
            .send(BrowserExitReceipt::Abnormal {
                instance: BrowserInstance {
                    generation: current.generation,
                    pid: fault_pid,
                },
                source: BrowserExitSource::Environment,
            })
            .map_err(|_| windows::core::Error::from(crate::webview2::E_UNEXPECTED))?;
        FAULTS_SENT.set(FAULTS_SENT.get() + 1);
    }
    Ok(())
}
