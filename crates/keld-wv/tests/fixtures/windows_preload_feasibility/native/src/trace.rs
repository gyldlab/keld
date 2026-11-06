//! Independent native event receipt and bounded observation storage.
use serde_json::{Value, json};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

pub(crate) type Shared = Rc<RefCell<Trace>>;
pub(crate) struct Trace {
    start: Instant,
    writer: BufWriter<File>,
    pub(crate) seq: u64,
    pub(crate) events: Vec<Value>,
    pub(crate) messages: Vec<Value>,
    pub(crate) committed_nav: Option<u64>,
    pub(crate) commit_serial: u64,
    pub(crate) late_injection_done: bool,
    pub(crate) failure: Option<String>,
}
impl Trace {
    pub(crate) fn open(path: &Path) -> Result<Shared, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("create new trace: {e}"))?;
        Ok(Rc::new(RefCell::new(Self {
            start: Instant::now(),
            writer: BufWriter::new(file),
            seq: 0,
            events: Vec::new(),
            messages: Vec::new(),
            committed_nav: None,
            commit_serial: 0,
            late_injection_done: false,
            failure: None,
        })))
    }
    pub(crate) fn push(&mut self, kind: &str, mut value: Value) -> u64 {
        self.seq += 1;
        value["nativeSeq"] = json!(self.seq);
        value["elapsedMs"] = json!(self.start.elapsed().as_secs_f64() * 1000.0);
        value["event"] = json!(kind);
        if let Err(e) = writeln!(self.writer, "{value}") {
            self.failure = Some(format!("trace write failed: {e}"));
        }
        if self.events.len() >= 50_000 {
            self.failure = Some("native trace event bound exceeded".to_owned());
        } else {
            self.events.push(value);
        }
        self.seq
    }
    pub(crate) fn flush(&mut self) {
        if let Err(e) = self.writer.flush() {
            self.failure = Some(format!("trace flush failed: {e}"));
        }
    }
}
pub(crate) fn observed(state: &Shared, kind: &str, source: &str) -> Option<Value> {
    state
        .borrow()
        .messages
        .iter()
        .find(|m| m["source"].as_str() == Some(source) && m["body"]["kind"].as_str() == Some(kind))
        .map(|m| m["body"].clone())
}
