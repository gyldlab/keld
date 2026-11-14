//! Bounded trace reads on `OriginalRun`'s existing capture/process/profile owners.
use super::{Fixture, OriginalRun};
use crate::PRODUCT_DEADLINE;
use crate::support::process::{PipeText, pipe_line_after};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};

const PREFIXES: [(&str, &str); 9] = [
    ("hold", "KELD_X10_OUTCOME_HOLD "),
    ("scope", "KELD_X10_NATIVE_SCOPE "),
    ("owner", "KELD_X10_NATIVE_OWNER "),
    ("diagnostic", "KELD_COMPAT_EVENT_DIAGNOSTIC "),
    ("fresh", "KELD_X10_FRESH_OFFER "),
    ("physical", "KELD_X10_PHYSICAL_VALUE "),
    ("selected", "KELD_X10_SELECTED_DIAGNOSTIC "),
    ("pressure", "KELD_X11_NATIVE_PRESSURE "),
    ("navigation", "KELD_X10_NAVIGATION_POST "),
];

pub(crate) struct QueuedCloseRecord {
    pub(crate) end: usize,
    pub(crate) kind: &'static str,
    pub(crate) value: Value,
}
#[derive(Default)]
pub(crate) struct QueuedCloseTrace {
    after: [usize; 9],
    pub(crate) records: Vec<QueuedCloseRecord>,
}
fn parsed(kind: &'static str, prefix: &str, line: &str, end: usize) -> QueuedCloseRecord {
    let text = line
        .strip_prefix(prefix)
        .expect("actual queued-close trace prefix");
    assert!(text.len() <= 32768, "bounded native observation");
    let value = if kind == "hold" {
        assert!(!text.starts_with("phase=failed"), "X10_HOLD_FAILED: {text}");
        let mut fields = serde_json::Map::new();
        for token in text.split_whitespace() {
            let (name, value) = token.split_once('=').expect("fixed hold metadata");
            assert!(matches!(
                name,
                "phase" | "mode" | "webview" | "navigation" | "delivery"
            ));
            let value = value
                .parse::<u64>()
                .map_or_else(|_| Value::String(value.to_owned()), Value::from);
            assert!(fields.insert(name.to_owned(), value).is_none());
        }
        Value::Object(fields)
    } else {
        serde_json::from_str(text).expect("actual bounded native JSON")
    };
    assert!(
        kind != "physical" || value["phase"] != "packet-hold-failed",
        "actual physical Packet hold failed"
    );
    QueuedCloseRecord { end, kind, value }
}
impl QueuedCloseTrace {
    fn poll(&mut self, capture: &PipeText) {
        for (index, (kind, prefix)) in PREFIXES.iter().enumerate() {
            while let Some((line, end)) = pipe_line_after(capture, prefix, self.after[index]) {
                self.after[index] = end;
                assert!(self.records.len() < 256, "bounded X10 trace");
                self.records.push(parsed(kind, prefix, &line, end));
            }
        }
    }
    pub(crate) fn from_complete(trace: &str) -> Self {
        let mut result = Self::default();
        let mut end = 0;
        for line in trace.split_inclusive('\n') {
            end += line.len();
            let text = line.trim_end_matches(['\r', '\n']);
            for (kind, prefix) in PREFIXES {
                if text.starts_with(prefix) {
                    assert!(result.records.len() < 256, "bounded complete X10 trace");
                    result.records.push(parsed(kind, prefix, text, end));
                }
            }
        }
        result
    }
}
impl OriginalRun {
    pub(crate) fn start_queued_close(
        cli: &Path,
        project: &Path,
        directory: &Path,
        after_event: bool,
    ) -> Self {
        let fixture = if after_event {
            Fixture::QueuedCloseAfter
        } else {
            Fixture::QueuedCloseBefore
        };
        Self::start_with_native_hold(cli, project, fixture, Some(directory))
    }

    pub(crate) fn start_fresh_offer(cli: &Path, project: &Path, directory: &Path) -> Self {
        Self::start_with_native_hold(cli, project, Fixture::ClosedProducer, Some(directory))
    }

    pub(crate) fn start_navigation_order(
        cli: &Path,
        project: &Path,
        directory: &Path,
        fixture: Fixture,
    ) -> Self {
        assert!(matches!(
            fixture,
            Fixture::NavigationBefore | Fixture::NavigationAfter | Fixture::NavigationSynthetic
        ));
        Self::start_with_native_hold(cli, project, fixture, Some(directory))
    }

    pub(crate) fn queued_close_record(
        &mut self,
        trace: &mut QueuedCloseTrace,
        kind: &str,
        accepted: impl Fn(&Value) -> bool,
    ) -> Value {
        self.queued_close_record_by(trace, kind, accepted, Instant::now() + PRODUCT_DEADLINE)
    }

    pub(crate) fn queued_close_record_by(
        &mut self,
        trace: &mut QueuedCloseTrace,
        kind: &str,
        accepted: impl Fn(&Value) -> bool,
        deadline: Instant,
    ) -> Value {
        loop {
            self.assert_live(
                "actual source/custody condition by original deadline",
                deadline,
            );
            trace.poll(&self.stderr);
            if let Some(record) = trace
                .records
                .iter()
                .find(|record| record.kind == kind && accepted(&record.value))
            {
                return record.value.clone();
            }
            self.assert_live("actual X10 source/custody observation", deadline);
            std::thread::park_timeout(Duration::from_millis(10));
        }
    }
}
