//! STA message pump and narrow COM helpers reused from the preserved E5 probe.
use crate::trace::Shared;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler, ExecuteScriptCompletedHandler, take_pwstr,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE,
    PeekMessageW, QS_ALLINPUT, TranslateMessage,
};
use windows::core::{HSTRING, PWSTR};

pub(crate) fn pump_until(deadline: Instant, mut done: impl FnMut() -> bool) -> bool {
    loop {
        if done() {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let ms = u32::try_from((deadline - now).as_millis().min(20)).unwrap_or(20);
        // SAFETY: bounded wait and dispatch on the thread that owns all fixture COM/HWNDs.
        unsafe {
            let _ = MsgWaitForMultipleObjectsEx(None, ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}
pub(crate) fn wait(
    state: &Shared,
    timeout: Duration,
    what: &str,
    mut done: impl FnMut() -> bool,
) -> Result<(), String> {
    let ready = pump_until(Instant::now() + timeout, || {
        state.borrow().failure.is_some() || done()
    });
    if let Some(error) = state.borrow().failure.clone() {
        return Err(error);
    }
    if ready {
        Ok(())
    } else {
        state.borrow_mut().push("Timeout", json!({"what": what}));
        Err(format!(
            "timed out awaiting {what}; no cancellation or cleanup inferred"
        ))
    }
}
pub(crate) fn hr(result: &windows::core::Result<()>) -> String {
    match result {
        Ok(()) => "S_OK".to_owned(),
        Err(e) => format!("0x{:08X}: {e}", e.code().0),
    }
}
pub(crate) fn wide(
    read: impl FnOnce(&mut PWSTR) -> windows::core::Result<()>,
) -> windows::core::Result<String> {
    let mut pointer = PWSTR::null();
    read(&mut pointer)?;
    Ok(take_pwstr(pointer))
}
pub(crate) fn register_script(
    wv: &ICoreWebView2,
    state: &Shared,
    source: &str,
    label: &str,
    timeout: Duration,
) -> Result<String, String> {
    let slot = Rc::new(RefCell::new(None));
    let out = slot.clone();
    let log = state.clone();
    let name = label.to_owned();
    let handler =
        AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(move |status, id| {
            log.borrow_mut().push(
                "ScriptRegistrationCompleted",
                json!({"label": name, "status": hr(&status), "scriptId": id}),
            );
            *out.borrow_mut() = Some(status.map(|()| id).map_err(|e| e.to_string()));
            Ok(())
        }));
    state.borrow_mut().push(
        "ScriptRegistrationCalled",
        json!({"label": label, "sourceBytes": source.len()}),
    );
    // SAFETY: live WebView and one-shot callback remain owned by the STA.
    unsafe { wv.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(source), &handler) }
        .map_err(|e| format!("AddScriptToExecuteOnDocumentCreated: {e}"))?;
    wait(
        state,
        timeout,
        "document-created registration callback",
        || slot.borrow().is_some(),
    )?;
    let result = slot
        .borrow_mut()
        .take()
        .ok_or("missing script registration result")?;
    result
}
pub(crate) fn enqueue_execute(
    wv: &ICoreWebView2,
    state: &Shared,
    source: &str,
) -> Result<(), String> {
    let log = state.clone();
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |status, value| {
        log.borrow_mut().push(
            "ExecuteScriptCompleted",
            json!({"status": hr(&status), "raw": value}),
        );
        Ok(())
    }));
    state.borrow_mut().push("ExecuteScriptCalled", json!({"sourceBytes": source.len(),
        "synchronization": "await independent forcing-start; do not await script callback before Navigate"}));
    // SAFETY: live WebView on its owning STA; source is a bounded fixture program.
    unsafe { wv.ExecuteScript(&HSTRING::from(source), &handler) }
        .map_err(|e| format!("ExecuteScript call: {e}"))
}
