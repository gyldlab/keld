//! U18 fixed page readback, shared by both native loops outside view obligations.
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use tao::event_loop::EventLoopProxy;
use webview2_com::ExecuteScriptCompletedHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use windows::Win32::Foundation::E_UNEXPECTED;
use windows::core::HSTRING;

use super::super::WindowsLoopEvent;
use super::{TEST_WINDOWS_PAGE_READBACK_ENV, invalid_hook, read_env};
use crate::{WebviewId, WvError};

// The hook accepts only an interval. No caller or environment can supply script.
const SCRIPT: &str = "({screenX:window.screenX,screenY:window.screenY,innerWidth:window.innerWidth,visibilityState:document.visibilityState,hasFocus:document.hasFocus()})";

type Reply = Receiver<windows::core::Result<String>>;

pub(in crate::webview2) struct PageReadback {
    interval: Option<Duration>,
    next: Option<Instant>,
    pending: BTreeMap<u32, Reply>,
    wake: EventLoopProxy<WindowsLoopEvent>,
}

impl PageReadback {
    pub(in crate::webview2) fn from_env(
        wake: EventLoopProxy<WindowsLoopEvent>,
    ) -> Result<Self, WvError> {
        let interval = read_env(TEST_WINDOWS_PAGE_READBACK_ENV)?
            .as_deref()
            .map(parse_interval)
            .transpose()?;
        Ok(Self {
            interval,
            next: interval.map(|_| Instant::now()),
            pending: BTreeMap::new(),
            wake,
        })
    }

    /// This only schedules normal asynchronous calls. It never starts a nested
    /// pump, retains a controller, or calls a host-window obligation.
    pub(in crate::webview2) fn tick<'a>(
        &mut self,
        now: Instant,
        views: impl IntoIterator<Item = (WebviewId, &'a ICoreWebView2)>,
    ) -> Result<(), WvError> {
        let Some(interval) = self.interval else {
            return Ok(());
        };
        let live: BTreeMap<_, _> = views
            .into_iter()
            .map(|(id, webview)| (id.0, webview))
            .collect();
        // A closed view no longer has page observations. Its callback owns only
        // a sender, so dropping this receiver cannot extend any native lifetime.
        self.pending.retain(|id, _| live.contains_key(id));
        let mut completed = Vec::new();
        for (id, reply) in &self.pending {
            match reply.try_recv() {
                Ok(result) => {
                    let value = result.map_err(|error| WvError::Script(error.to_string()))?;
                    let value = compact_readback(&value)?;
                    if super::super::t2_test_observation::enabled() {
                        eprintln!("KELD_D16_PAGE_COMPLETED view={id}");
                    }
                    eprintln!("PAGE_READBACK {id} {value}");
                    completed.push(*id);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    return Err(invalid_hook(
                        TEST_WINDOWS_PAGE_READBACK_ENV,
                        "page callback disconnected",
                    ));
                }
            }
        }
        for id in completed {
            self.pending.remove(&id);
        }
        if self.next.is_some_and(|next| now < next) {
            return Ok(());
        }
        self.next = Some(now.checked_add(interval).ok_or_else(|| {
            invalid_hook(
                TEST_WINDOWS_PAGE_READBACK_ENV,
                "interval exceeds the native clock range",
            )
        })?);
        if super::super::t2_test_observation::enabled() {
            eprintln!(
                "KELD_D16_PAGE_TICK live={} pending={}",
                live.len(),
                self.pending.len()
            );
        }
        for (id, webview) in live {
            if let std::collections::btree_map::Entry::Vacant(entry) = self.pending.entry(id) {
                entry.insert(request(id, webview, self.wake.clone())?);
            }
        }
        Ok(())
    }

    /// The loop folds this into its existing earliest-deadline calculation.
    pub(in crate::webview2) fn next_deadline(&self) -> Option<Instant> {
        self.next
    }
}

fn parse_interval(value: &str) -> Result<Duration, WvError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_hook(
            TEST_WINDOWS_PAGE_READBACK_ENV,
            "expected positive whole milliseconds",
        ));
    }
    let milliseconds = value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            invalid_hook(
                TEST_WINDOWS_PAGE_READBACK_ENV,
                "expected positive whole milliseconds",
            )
        })?;
    Ok(Duration::from_millis(milliseconds))
}

fn compact_readback(value: &str) -> Result<String, WvError> {
    let json: serde_json::Value = serde_json::from_str(value)
        .map_err(|_| invalid_hook(TEST_WINDOWS_PAGE_READBACK_ENV, "page returned invalid JSON"))?;
    let object = json.as_object().ok_or_else(|| {
        invalid_hook(
            TEST_WINDOWS_PAGE_READBACK_ENV,
            "page did not return an object",
        )
    })?;
    for name in ["screenX", "screenY", "innerWidth"] {
        if !object.get(name).is_some_and(serde_json::Value::is_number) {
            return Err(invalid_hook(
                TEST_WINDOWS_PAGE_READBACK_ENV,
                "page numeric observation is missing",
            ));
        }
    }
    if !object
        .get("visibilityState")
        .is_some_and(serde_json::Value::is_string)
        || !object
            .get("hasFocus")
            .is_some_and(serde_json::Value::is_boolean)
    {
        return Err(invalid_hook(
            TEST_WINDOWS_PAGE_READBACK_ENV,
            "page visibility/focus observation is missing",
        ));
    }
    // Serialize the actual result, never an expected value; one record stays on
    // one stderr line even if the page returned escaped control characters.
    Ok(json.to_string())
}

// This is the actual callback-delivery owner. The tests exercise its wake
// boundary with no native resources, including after the receiver has retired.
fn publish_reply(
    sender: &mpsc::Sender<windows::core::Result<String>>,
    result: windows::core::Result<String>,
    wake: impl FnOnce() -> windows::core::Result<()>,
) -> windows::core::Result<()> {
    if sender.send(result).is_ok() {
        wake()?;
    }
    Ok(())
}

fn request(
    id: u32,
    webview: &ICoreWebView2,
    wake: EventLoopProxy<WindowsLoopEvent>,
) -> Result<Reply, WvError> {
    let observing = super::super::t2_test_observation::enabled();
    let (sender, receiver) = mpsc::channel();
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |status, value| {
        // The loop owns the receiver only while the view is live. The callback
        // owns no borrowed output slot, native object, or loop reference.
        if observing {
            let code = status.as_ref().err().map_or(0, |error| error.code().0);
            eprintln!("KELD_D16_PAGE_CALLBACK view={id} result={code}");
        }
        publish_reply(&sender, status.map(|()| value), || {
            // Publish first, then post through this original loop's proxy. This
            // schedules ordinary dispatch without pumping or retaining a View.
            wake.send_event(WindowsLoopEvent::PageReadbackWake)
                .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
        })
    }));
    // SAFETY: the caller lends a live view on its creating UI/STA thread.
    // The fixed HSTRING lives through the call; the completion owns its sender
    // and returns without pumping. No COM interface escapes to another thread.
    // https://learn.microsoft.com/microsoft-edge/webview2/concepts/threading-model
    // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2#executescript
    if observing {
        eprintln!("KELD_D16_PAGE_REQUEST_ENTER view={id}");
    }
    // SAFETY: the same borrowed UI-thread webview, fixed string and existing callback owner.
    let accepted = unsafe { webview.ExecuteScript(&HSTRING::from(SCRIPT), &handler) };
    if observing {
        let code = accepted.as_ref().err().map_or(0, |error| error.code().0);
        eprintln!("KELD_D16_PAGE_REQUEST_RETURN view={id} result={code}");
    }
    accepted.map_err(|error| WvError::Script(error.to_string()))?;
    Ok(receiver)
}

#[cfg(test)]
#[path = "tests/page_readback.rs"]
mod tests;
