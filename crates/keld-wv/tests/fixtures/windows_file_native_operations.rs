//! Exact owned-fixture navigation and raw native evaluation observations for Q22.
//! These calls do not implement the Keld facade or its pending-result admission rules.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use webview2_com::Microsoft::Web::WebView2::Win32 as wv;
use webview2_com::{CallDevToolsProtocolMethodCompletedHandler, ExecuteScriptCompletedHandler};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::IStream;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, WM_APP,
};
use windows::core::{HSTRING, Interface, w};

use super::{Completion, HEADER, Session, callbacks, failure, hresult, pump_until};

impl Session {
    #[allow(unsafe_code)] // Private helper-thread queue calibration, before any COM/window allocation.
    pub(in super::super) fn observe_message_pump() -> Result<Value, String> {
        const MESSAGE: u32 = WM_APP + 0x1185;
        const QUEUED: u32 = 100;
        let mut message = MSG::default();
        // SAFETY: this creates/accesses only the current private helper thread's queue.
        unsafe {
            let _ = PeekMessageW(&raw mut message, None, MESSAGE, MESSAGE, PM_NOREMOVE);
        }
        // SAFETY: obtain this live thread's ID solely to post its private calibration messages.
        let thread = unsafe { GetCurrentThreadId() };
        let posted = (|| {
            for _ in 0..QUEUED {
                // SAFETY: no pointer payload; fixed private ID on this same helper thread.
                unsafe { PostThreadMessageW(thread, MESSAGE, WPARAM(0), LPARAM(0)) }
                    .map_err(|error| failure("post owned pump calibration", &error))?;
            }
            Ok::<(), String>(())
        })();
        let mut polls = 0;
        let returned = posted.is_ok()
            && pump_until(Instant::now() + Duration::from_secs(1), || {
                polls += 1;
                polls >= 2
            });
        let mut remaining = 0;
        for _ in 0..=QUEUED {
            // SAFETY: remove only this calibration's private message ID. No external
            // window/input or unrelated thread message is consumed by this cleanup.
            if !unsafe { PeekMessageW(&raw mut message, None, MESSAGE, MESSAGE, PM_REMOVE) }
                .as_bool()
            {
                break;
            }
            remaining += 1;
        }
        posted?;
        Ok(
            json!({"queued":QUEUED,"remaining_at_readiness":remaining,"readiness_polls":polls,"returned_on_readiness":returned,
            "ready_before_queue_drained":returned && remaining > 0 && remaining <= QUEUED,
            "scope":"real private thread queue; readiness revisitation, not preemption or a late WebView2 callback"}),
        )
    }

    #[allow(unsafe_code)] // Test-only native request construction and navigation; no trust decision is made.
    pub(in super::super) fn host_request(&self, nonce: &str) -> Result<Value, String> {
        let environment = self
            .environment
            .as_ref()
            .ok_or("missing environment")?
            .cast::<wv::ICoreWebView2Environment2>()
            .map_err(|error| failure("Environment2", &error))?;
        let mut result = json!({"uri":self.uri,"header_name":HEADER,"sent_value":nonce});
        // SAFETY: exact owned fixture URI, GET, no body, and a test observation marker.
        let request = unsafe {
            environment.CreateWebResourceRequest(
                &HSTRING::from(&self.uri),
                w!("GET"),
                None::<&IStream>,
                &HSTRING::from(format!("{HEADER}: {nonce}\r\n")),
            )
        };
        let request = match request {
            Ok(request) => {
                result["create_hresult"] = json!("S_OK");
                request
            }
            Err(error) => {
                result["create_hresult"] = json!(hresult(&error));
                return Ok(result);
            }
        };
        // SAFETY: native request still belongs to this STA; read back the sent marker before use.
        result["input_header"] = callbacks::header(
            &unsafe { request.Headers() }.map_err(|error| failure("request Headers", &error))?,
        )
        .map_err(|error| failure("request header readback", &error))?;
        let view = self
            .view()?
            .cast::<wv::ICoreWebView2_2>()
            .map_err(|error| failure("WebView2_2", &error))?;
        // SAFETY: request is live on this STA; its scheme support is measured, not assumed.
        result["navigate_hresult"] =
            json!(
                match unsafe { view.NavigateWithWebResourceRequest(&request) } {
                    Ok(()) => "S_OK".to_owned(),
                    Err(error) => hresult(&error),
                }
            );
        Ok(result)
    }

    #[allow(unsafe_code)] // Test-only plain file navigation is the healthy native control.
    pub(in super::super) fn navigate_plain(&self) -> Result<(), String> {
        // SAFETY: live view and exact owned fixture URI on the creating STA.
        unsafe { self.view()?.Navigate(&HSTRING::from(&self.uri)) }
            .map_err(|error| failure("Navigate", &error))
    }

    #[allow(unsafe_code)] // Test-only page action; only the ensuing native events prove navigation.
    pub(in super::super) fn navigate_from_page(&self) -> Result<(), String> {
        let trace = Rc::clone(&self.trace);
        let callback = ExecuteScriptCompletedHandler::create(Box::new(move |result, body| {
            trace.borrow_mut().record("page-navigation-command-completion", json!({
                "hresult":result.map_or_else(|error| hresult(&error), |()| "S_OK".to_owned()),"body":body
            }));
            Ok(())
        }));
        // SAFETY: fixed action in the owned page; no callback result fabricates a load event.
        unsafe {
            self.view()?
                .ExecuteScript(w!("location.assign(location.href)"), &callback)
        }
        .map_err(|error| failure("page navigation dispatch", &error))
    }

    #[allow(unsafe_code)] // Test-only evaluation of fixed owned fixture expressions; native completion is recorded.
    pub(in super::super) fn evaluate(&self, expression: &str, cdp: bool) -> Result<Value, String> {
        let slot: Completion<String> = Rc::new(RefCell::new(None));
        let output = Rc::clone(&slot);
        let view = self.view()?;
        if cdp {
            let callback = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                move |result, body| {
                    *output.borrow_mut() = Some(result.map(|()| body));
                    Ok(())
                },
            ));
            let params = json!({"expression":expression,"awaitPromise":true,"returnByValue":true,"allowUnsafeEvalBlockedByCSP":false});
            // SAFETY: live view on its STA; this is an observation, not Keld's pending-evaluation path.
            unsafe {
                view.CallDevToolsProtocolMethod(
                    w!("Runtime.evaluate"),
                    &HSTRING::from(params.to_string()),
                    &callback,
                )
            }
            .map_err(|error| failure("Runtime.evaluate", &error))?;
        } else {
            let callback = ExecuteScriptCompletedHandler::create(Box::new(move |result, body| {
                *output.borrow_mut() = Some(result.map(|()| body));
                Ok(())
            }));
            // SAFETY: live view and completion owned by this STA.
            unsafe { view.ExecuteScript(&HSTRING::from(expression), &callback) }
                .map_err(|error| failure("ExecuteScript", &error))?;
        }
        let body = self.completion(
            &slot,
            if cdp {
                "Runtime.evaluate callback"
            } else {
                "ExecuteScript callback"
            },
        )?;
        let parsed: Value = serde_json::from_str(&body).map_err(|error| {
            format!("native evaluation returned invalid JSON: {error}; body={body:?}")
        })?;
        Ok(json!({"hresult":"S_OK","body":parsed}))
    }
}
