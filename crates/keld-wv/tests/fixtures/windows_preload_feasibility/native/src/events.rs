//! Native observations. These handlers do not implement KELD source admission.
use crate::config::Config;
use crate::pump::{hr, wide};
use crate::trace::Shared;
use serde_json::{Value, json};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PERMISSION_STATE_DENY, COREWEBVIEW2_PROCESS_FAILED_KIND,
    COREWEBVIEW2_WEB_ERROR_STATUS, ICoreWebView2, ICoreWebView2_2,
};
use webview2_com::{
    ContentLoadingEventHandler, DOMContentLoadedEventHandler, DocumentTitleChangedEventHandler,
    ExecuteScriptCompletedHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler, ProcessFailedEventHandler,
    WebMessageReceivedEventHandler,
};
use windows::core::{BOOL, HSTRING, Interface};

pub(crate) fn install(wv: &ICoreWebView2, state: &Shared, config: &Config) -> Result<(), String> {
    let mut token = 0i64;
    let log = state.clone();
    let handler = NavigationStartingEventHandler::create(Box::new(move |sender, args| {
        if let (Some(sender), Some(args)) = (sender, args) {
            let mut id = 0u64;
            // SAFETY: callback arguments are live on the owning STA; getters only.
            let (uri, current) = unsafe {
                args.NavigationId(&mut id)?;
                (wide(|p| args.Uri(p))?, wide(|p| sender.Source(p))?)
            };
            log.borrow_mut().push(
                "NavigationStarting",
                json!({"navigationId": id, "uri": uri, "currentSource": current}),
            );
        }
        Ok(())
    }));
    // SAFETY: handler retained by the live WebView until explicit Close.
    unsafe { wv.add_NavigationStarting(&handler, &mut token) }.map_err(|e| e.to_string())?;

    let log = state.clone();
    let handler = ContentLoadingEventHandler::create(Box::new(move |sender, args| {
        if let (Some(sender), Some(args)) = (sender, args) {
            let mut id = 0u64;
            let mut error_page = BOOL(0);
            // SAFETY: native callback arguments on their owning STA; getters only.
            let source = unsafe {
                args.NavigationId(&mut id)?;
                args.IsErrorPage(&mut error_page)?;
                wide(|p| sender.Source(p))?
            };
            let mut trace = log.borrow_mut();
            trace.committed_nav = Some(id);
            trace.commit_serial += 1;
            let serial = trace.commit_serial;
            trace.push(
                "ContentLoading",
                json!({"navigationId": id, "source": source,
                "fixtureCommitSerial": serial, "isErrorPage": error_page.as_bool()}),
            );
        }
        Ok(())
    }));
    // SAFETY: handler retained by the live WebView until explicit Close.
    unsafe { wv.add_ContentLoading(&handler, &mut token) }.map_err(|e| e.to_string())?;

    let log = state.clone();
    let handler = WebMessageReceivedEventHandler::create(Box::new(move |sender, args| {
        if let (Some(sender), Some(args)) = (sender, args) {
            // SAFETY: read browser-authenticated event source separately from body claims.
            let (source, current, raw) = unsafe {
                (
                    wide(|p| args.Source(p))?,
                    wide(|p| sender.Source(p))?,
                    wide(|p| args.WebMessageAsJson(p))?,
                )
            };
            let outer = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw.clone()));
            let body = outer
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .unwrap_or(outer);
            let mut trace = log.borrow_mut();
            let mut message = json!({"source": source, "currentSource": current, "raw": raw,
                "body": body, "committedNavigationId": trace.committed_nav,
                "fixtureCommitSerial": trace.commit_serial});
            let seq = trace.push("WebMessageReceived", message.clone());
            message["nativeSeq"] = json!(seq);
            if let Some(event) = trace.events.last() {
                message["elapsedMs"] = event["elapsedMs"].clone();
            }
            trace.messages.push(message);
        }
        Ok(())
    }));
    // SAFETY: handler retained by the live WebView until explicit Close.
    unsafe { wv.add_WebMessageReceived(&handler, &mut token) }.map_err(|e| e.to_string())?;

    let log = state.clone();
    let handler = DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
        if let Some(sender) = sender {
            // SAFETY: getter on live callback sender on this STA.
            let title = unsafe { wide(|p| sender.DocumentTitle(p))? };
            log.borrow_mut()
                .push("DocumentTitleChanged", json!({"title": title}));
        }
        Ok(())
    }));
    // SAFETY: handler retained by the live WebView until explicit Close.
    unsafe { wv.add_DocumentTitleChanged(&handler, &mut token) }.map_err(|e| e.to_string())?;

    let log = state.clone();
    let handler = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
        if let (Some(sender), Some(args)) = (sender, args) {
            let mut id = 0u64;
            let mut success = BOOL(0);
            let mut error = COREWEBVIEW2_WEB_ERROR_STATUS(0);
            // SAFETY: callback getters on the owning STA.
            let source = unsafe {
                args.NavigationId(&mut id)?;
                args.IsSuccess(&mut success)?;
                args.WebErrorStatus(&mut error)?;
                wide(|p| sender.Source(p))?
            };
            log.borrow_mut().push(
                "NavigationCompleted",
                json!({"navigationId": id,
                "source": source, "isSuccess": success.as_bool(), "webErrorStatus": error.0}),
            );
        }
        Ok(())
    }));
    // SAFETY: handler retained by the live WebView until explicit Close.
    unsafe { wv.add_NavigationCompleted(&handler, &mut token) }.map_err(|e| e.to_string())?;

    let wv2: ICoreWebView2_2 = wv.cast().map_err(|e| e.to_string())?;
    let late_bundle = if config.mode == "order" && config.variant == "dom-content-loaded" {
        config.bundle.clone()
    } else {
        None
    };
    let log = state.clone();
    let handler =
        DOMContentLoadedEventHandler::create(Box::new(move |sender, args| {
            if let (Some(sender), Some(args)) = (sender, args) {
                let mut id = 0u64;
                // SAFETY: getters before the deliberate late-injection mutation.
                let (title, source) = unsafe {
                    args.NavigationId(&mut id)?;
                    (
                        wide(|p| sender.DocumentTitle(p))?,
                        wide(|p| sender.Source(p))?,
                    )
                };
                log.borrow_mut().push("DOMContentLoaded",
                json!({"navigationId": id, "source": source, "titleBeforeLateInjection": title}));
                if let Some(bundle) = late_bundle.as_deref() {
                    let complete = log.clone();
                    let callback =
                        ExecuteScriptCompletedHandler::create(Box::new(move |status, raw| {
                            let mut trace = complete.borrow_mut();
                            trace.push(
                                "LateBundleInjectionCompleted",
                                json!({"status": hr(&status), "raw": raw}),
                            );
                            trace.late_injection_done = true;
                            if let Err(e) = status {
                                trace.failure = Some(format!("late bundle injection: {e}"));
                            }
                            Ok(())
                        }));
                    log.borrow_mut().push(
                        "LateBundleInjectionCalled",
                        json!({"sourceBytes": bundle.len()}),
                    );
                    // SAFETY: deliberate test-only late injection on the owning STA.
                    let result = unsafe { sender.ExecuteScript(&HSTRING::from(bundle), &callback) };
                    if let Err(e) = result {
                        log.borrow_mut().failure = Some(format!("late injection call: {e}"));
                    }
                }
            }
            Ok(())
        }));
    // SAFETY: handler retained by the live WebView until explicit Close.
    unsafe { wv2.add_DOMContentLoaded(&handler, &mut token) }.map_err(|e| e.to_string())?;

    let log = state.clone();
    let handler = PermissionRequestedEventHandler::create(Box::new(move |_, args| {
        if let Some(args) = args {
            // SAFETY: reject fixture permissions through the native event argument.
            unsafe {
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
            }
            log.borrow_mut().push("PermissionDenied", json!({}));
        }
        Ok(())
    }));
    // SAFETY: installed before first fixture navigation.
    unsafe { wv.add_PermissionRequested(&handler, &mut token) }.map_err(|e| e.to_string())?;

    let log = state.clone();
    let handler = NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
        if let Some(args) = args {
            // SAFETY: suppress all fixture popups; create no secondary WebView.
            unsafe {
                args.SetHandled(true)?;
            }
            log.borrow_mut().push("NewWindowDenied", json!({}));
        }
        Ok(())
    }));
    // SAFETY: installed before first fixture navigation.
    unsafe { wv.add_NewWindowRequested(&handler, &mut token) }.map_err(|e| e.to_string())?;

    let log = state.clone();
    let handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
        if let Some(args) = args {
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(0);
            // SAFETY: read native failure kind on its STA.
            unsafe {
                args.ProcessFailedKind(&mut kind)?;
            }
            let mut trace = log.borrow_mut();
            trace.push("ProcessFailed", json!({"kind": kind.0}));
            trace.failure = Some(format!("WebView process failure: {}", kind.0));
        }
        Ok(())
    }));
    // SAFETY: handler retained by the live WebView until explicit Close.
    unsafe { wv.add_ProcessFailed(&handler, &mut token) }.map_err(|e| e.to_string())?;
    state
        .borrow_mut()
        .push("ObserversAndDenyHandlersInstalled", json!({}));
    Ok(())
}
