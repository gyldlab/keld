//! Read only the existing native trace and fixed page-readback streams.
use serde_json::Value;
use std::thread;
use std::time::{Duration, Instant};

use super::OriginalRun;
use crate::startup_kill_switch;
use crate::support::process::pipe_line_after;

impl OriginalRun {
    pub(crate) fn native_record(&mut self, prefix: &str) -> Value {
        let deadline = Instant::now() + startup_kill_switch();
        loop {
            if let Some((line, after)) = pipe_line_after(&self.stderr, prefix, self.native_after) {
                self.native_after = after;
                let text = line
                    .strip_prefix(prefix)
                    .expect("actual native trace prefix");
                assert!(text.len() <= 32768, "bounded native fixture trace");
                return serde_json::from_str(text).expect("actual native observation JSON");
            }
            self.assert_live("actual native observation", deadline);
            thread::park_timeout(Duration::from_millis(10));
        }
    }

    /// Public ids and native view ids come from different allocators. This is
    /// the actual Registry Created association, never numeric-id equality.
    pub(crate) fn window_view(&mut self, window: u64) -> Value {
        let record = self.native_record("KELD_WINDOW_VIEW_ASSOCIATION ");
        assert_eq!(record["window"], window, "actual public window association");
        record["build"]
            .as_u64()
            .expect("actual native build correlation");
        record["incarnation"]
            .as_u64()
            .expect("actual WindowRef incarnation");
        let view = record["view"].as_u64().expect("actual native WebviewId");
        u32::try_from(view).expect("actual native WebviewId width");
        record
    }

    /// Wait for the actual consumed reply admission and physical DATA clear for
    /// the sole native event already observed by this page. Neither log order
    /// nor the void public send return establishes that the DATA slot is free.
    pub(crate) fn consumed_event_storage_cleared(&mut self, view: u64, page: &Value) -> Value {
        let document = page["rawChallenge"].as_str().expect("actual page document");
        let records = page["rawRecords"]
            .as_array()
            .expect("actual native page records");
        assert!(!records.is_empty() && records.len() <= 12);
        let mut event = None;
        for record in records {
            let text = record["text"].as_str().expect("actual native page text");
            assert!(text.len() <= 2048);
            let value: Value = serde_json::from_str(text).expect("actual native page JSON");
            if value["kind"] != "event" {
                continue;
            }
            assert_eq!(value["v"], 2);
            assert_eq!(value["document"], document);
            assert_eq!(value["channel"], "args-obj");
            assert_eq!(record["topIssued"], false);
            assert!(
                event.is_none(),
                "one native coordination event before top control"
            );
            event = Some(value);
        }
        let event = event.expect("actual native coordination event");
        let message = event["delivery"]
            .as_u64()
            .expect("actual native event delivery");
        let completion = self.event_storage_completion(view, message, None, true);
        serde_json::json!({
            "event": event, "consumedReply": completion["consumedReply"],
            "cleared": completion["cleared"],
        })
    }

    /// Wait for the exact already-observed native event to release its DATA slot.
    /// This is a scheduling barrier, never proof of a later page or Bun effect.
    pub(crate) fn native_event_storage_cleared(
        &mut self,
        view: u64,
        message: u64,
        navigation: Option<u64>,
    ) -> Value {
        self.event_storage_completion(view, message, navigation, false)["cleared"].clone()
    }

    fn event_storage_completion(
        &mut self,
        view: u64,
        message: u64,
        navigation: Option<u64>,
        require_consumed_reply: bool,
    ) -> Value {
        assert_ne!(message, 0);
        u32::try_from(message).expect("actual event delivery width");
        let prefix = "KELD_COMPAT_EVENT_COMPLETION ";
        let deadline = Instant::now() + startup_kill_switch();
        // A local cursor preserves independent native/page observers. The frame
        // contract still requires both exact-key facts, in either printing order.
        let mut after = 0;
        let mut admitted = None;
        let mut cleared = None;
        loop {
            if let Some((line, next)) = pipe_line_after(&self.stderr, prefix, after) {
                after = next;
                let text = line.strip_prefix(prefix).expect("actual completion prefix");
                assert!(text.len() <= 32768, "bounded actual completion record");
                let record: Value = serde_json::from_str(text).expect("actual completion JSON");
                if record["webview"] == view && record["message"] == message {
                    let actual_navigation = record["navigation"]
                        .as_u64()
                        .expect("actual native navigation");
                    u32::try_from(actual_navigation).expect("actual navigation width");
                    if let Some(expected) = navigation {
                        assert_eq!(
                            actual_navigation, expected,
                            "same native document generation"
                        );
                    }
                    match record["phase"].as_str() {
                        Some("consumed-reply-admitted") => {
                            assert!(admitted.is_none(), "one actual consumed reply admission");
                            admitted = Some(record);
                        }
                        Some("native-data-cleared") => {
                            assert!(cleared.is_none(), "one actual native DATA clear");
                            cleared = Some(record);
                        }
                        _ => panic!("unexpected actual completion record: {record}"),
                    }
                    if let Some(cleared) = &cleared
                        && (!require_consumed_reply || admitted.is_some())
                    {
                        if let Some(admitted) = &admitted {
                            assert_eq!(
                                admitted["navigation"], cleared["navigation"],
                                "same actual native document generation"
                            );
                        }
                        return serde_json::json!({
                            "consumedReply": admitted, "cleared": cleared,
                        });
                    }
                }
            }
            self.assert_live("actual consumed native DATA release", deadline);
            thread::park_timeout(Duration::from_millis(10));
        }
    }

    pub(crate) fn page_terminal(&mut self, view: u64) -> Value {
        self.page_when(view, |page| !page["terminal"].is_null())
    }

    pub(crate) fn page_when(&mut self, view: u64, accepted: impl Fn(&Value) -> bool) -> Value {
        self.read_page_when(view, false, |page| {
            assert!(page["error"].is_null(), "actual page failure: {page}");
            accepted(page)
        })
    }

    /// NAV waits on the existing page/control captures and original deadline.
    pub(crate) fn navigation_page_terminal(&mut self, view: u64) -> Value {
        self.read_page_when(view, true, |page| {
            assert!(page["error"].is_null(), "actual page failure: {page}");
            page["documentKind"] == "B" && page["terminal"].is_object()
        })
    }

    /// Return the actual frozen first snapshot before classifying a later error.
    pub(crate) fn page_first_author(&mut self, view: u64) -> Value {
        self.read_page_when(view, false, |page| page["first"].is_object())
    }

    fn read_page_when(
        &mut self,
        view: u64,
        reject_fixture_error: bool,
        accepted: impl Fn(&Value) -> bool,
    ) -> Value {
        let prefix = format!("PAGE_READBACK {view} ");
        let deadline = Instant::now() + startup_kill_switch();
        loop {
            if reject_fixture_error
                && let Some((line, _)) = pipe_line_after(&self.control, "FIXTURE_ERROR ", 0)
            {
                panic!("actual navigation fixture failure: {line}");
            }
            if let Some((line, after)) = pipe_line_after(&self.stderr, &prefix, self.page_after) {
                self.page_after = after;
                let readback: Value = serde_json::from_str(
                    line.strip_prefix(&prefix).expect("actual readback prefix"),
                )
                .expect("actual fixed ExecuteScript result");
                assert_ne!(
                    readback["originalProbeOversize"], true,
                    "actual probe exceeded bound"
                );
                if let Some(text) = readback["originalProbe"].as_str() {
                    assert!(text.len() <= 32768, "bounded probe readback");
                    let page: Value = serde_json::from_str(text).expect("actual probe snapshot");
                    if accepted(&page) {
                        return page;
                    }
                }
            }
            self.assert_live("actual native page readback", deadline);
            thread::park_timeout(Duration::from_millis(10));
        }
    }
}
