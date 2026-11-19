//! GH540 compatibility storage for the existing primary application session.
//! Native CONTROL is physical custody only. The Runtime and Window Registry
//! keep the authority to admit the operation that holds it.

mod native_control;

use super::primary_writer::PrimaryWriter;
use std::sync::Mutex;

pub(super) struct CompatSession {
    ledger: Mutex<native_control::CompatLedger>,
    writer: PrimaryWriter,
}

impl CompatSession {
    pub(super) fn new(writer: PrimaryWriter) -> Self {
        Self {
            ledger: Mutex::new(native_control::CompatLedger::default()),
            writer,
        }
    }
}
