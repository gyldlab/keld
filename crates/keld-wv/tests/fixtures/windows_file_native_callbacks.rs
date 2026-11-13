//! Native callback observations and exact owned-fixture response routing for the Q22 probe.
//! No shared parser, policy, identity or facade behavior is implemented here.

use std::rc::Rc;

use serde_json::{Value, json};
use webview2_com::Microsoft::Web::WebView2::Win32 as wv;
use webview2_com::{
    ContentLoadingEventHandler, DOMContentLoadedEventHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, NewWindowRequestedEventHandler,
    PermissionRequestedEventHandler, WebResourceRequestedEventHandler,
};
use windows::Win32::Foundation::{E_OUTOFMEMORY, E_POINTER};
use windows::Win32::System::Com::IStream;
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::core::{BOOL, HSTRING, Interface, w};

use super::{HEADER, NativeResult, Session, SharedTrace, failure, read_string, record_result};

#[allow(unsafe_code)] // Test-only native getters; every COM pointer remains on its creating STA.
pub(super) fn header(headers: &wv::ICoreWebView2HttpRequestHeaders) -> NativeResult<Value> {
    let mut present = BOOL(0);
    // SAFETY: live callback/request headers and a valid writable BOOL on this STA.
    unsafe {
        headers.Contains(&HSTRING::from(HEADER), &raw mut present)?;
    }
    if !present.as_bool() {
        return Ok(json!({"state":"absent"}));
    }
    // SAFETY: the getter allocates the PWSTR; read_string releases it with take_pwstr.
    let value = read_string(|out| unsafe { headers.GetHeader(&HSTRING::from(HEADER), out) })?;
    Ok(json!({"state":"present","value":value}))
}

#[allow(unsafe_code)] // Test-only native event getters on the callback's owning STA.
fn navigation(args: &wv::ICoreWebView2NavigationStartingEventArgs) -> NativeResult<Value> {
    let mut id = 0;
    // SAFETY: event args are supplied by WebView2 for this callback; outputs are valid.
    let uri = read_string(|out| unsafe { args.Uri(out) })?;
    // SAFETY: live args on this STA and an owned writable navigation ID.
    unsafe {
        args.NavigationId(&raw mut id)?;
    }
    // SAFETY: live args; returned headers remain on this STA.
    let observed_header = header(&unsafe { args.RequestHeaders()? })?;
    Ok(json!({"uri":uri,"navigation_id":id,"header":observed_header}))
}

// Own the real native deferral until it is installed in Session. Every early
// error (stream, response, duplicate hold) must Complete before returning.
struct PendingIframeDeferral {
    value: Option<wv::ICoreWebView2Deferral>,
    trace: SharedTrace,
}

#[allow(unsafe_code)] // Test-only COM completion on the callback's creating STA.
impl Drop for PendingIframeDeferral {
    fn drop(&mut self) {
        if let Some(deferral) = self.value.take() {
            // SAFETY: this guard never leaves the original WebResourceRequested STA.
            let completed = unsafe { deferral.Complete() };
            self.trace.borrow_mut().record(
                "race-child-abort-complete",
                json!({"completed":completed.is_ok(),
                    "error":completed.err().map(|error| error.to_string())}),
            );
        }
    }
}

impl Session {
    #[allow(unsafe_code)] // Test-only WebView2 native event observers, all on the owning STA.
    pub(super) fn register_navigation(&self) -> Result<(), String> {
        let view = self.view()?;
        let mut token = 0;
        for frame in [false, true] {
            let trace = Rc::clone(&self.trace);
            let callback = NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let observed = args
                    .ok_or_else(|| windows::core::Error::from(E_POINTER))
                    .and_then(|args| navigation(&args));
                record_result(
                    &trace,
                    if frame {
                        "frame-navigation-starting"
                    } else {
                        "navigation-starting"
                    },
                    observed,
                );
                Ok(())
            }));
            // SAFETY: both event families use NavigationStarting args; view retains each callback.
            unsafe {
                if frame {
                    view.add_FrameNavigationStarting(&callback, &raw mut token)
                } else {
                    view.add_NavigationStarting(&callback, &raw mut token)
                }
            }
            .map_err(|error| failure("register navigation starting", &error))?;
            let trace = Rc::clone(&self.trace);
            let callback = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                let observed = (|| {
                    let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                    let mut id = 0;
                    let mut success = BOOL(0);
                    let mut status = wv::COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN;
                    // SAFETY: native callback args and writable stack outputs on this STA.
                    unsafe {
                        args.NavigationId(&raw mut id)?;
                        args.IsSuccess(&raw mut success)?;
                        args.WebErrorStatus(&raw mut status)?;
                    }
                    Ok(
                        json!({"navigation_id":id,"success":success.as_bool(),"web_error_status":status.0}),
                    )
                })();
                record_result(
                    &trace,
                    if frame {
                        "frame-navigation-completed"
                    } else {
                        "navigation-completed"
                    },
                    observed,
                );
                Ok(())
            }));
            // SAFETY: top-level WebView2 frame-completion event uses NavigationCompleted handler/args.
            unsafe {
                if frame {
                    view.add_FrameNavigationCompleted(&callback, &raw mut token)
                } else {
                    view.add_NavigationCompleted(&callback, &raw mut token)
                }
            }
            .map_err(|error| failure("register navigation completed", &error))?;
        }
        let trace = Rc::clone(&self.trace);
        let callback = ContentLoadingEventHandler::create(Box::new(move |_, args| {
            let observed = (|| {
                let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                let mut id = 0;
                let mut is_error = BOOL(1);
                // SAFETY: native callback args and stack outputs on this STA.
                unsafe {
                    args.NavigationId(&raw mut id)?;
                    args.IsErrorPage(&raw mut is_error)?;
                }
                Ok(json!({"navigation_id":id,"is_error_page":is_error.as_bool()}))
            })();
            record_result(&trace, "content-loading", observed);
            Ok(())
        }));
        // SAFETY: live view retains callback; no navigation has begun.
        unsafe { view.add_ContentLoading(&callback, &raw mut token) }
            .map_err(|error| failure("register ContentLoading", &error))?;
        let trace = Rc::clone(&self.trace);
        let callback = DOMContentLoadedEventHandler::create(Box::new(move |_, args| {
            let observed = (|| {
                let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                let mut id = 0;
                // SAFETY: native event args and owned writable output on this STA.
                unsafe {
                    args.NavigationId(&raw mut id)?;
                }
                Ok(json!({"navigation_id":id}))
            })();
            record_result(&trace, "dom-content-loaded", observed);
            Ok(())
        }));
        let view2 = view
            .cast::<wv::ICoreWebView2_2>()
            .map_err(|error| failure("WebView2_2", &error))?;
        // SAFETY: live versioned view interface retains callback until Close.
        unsafe { view2.add_DOMContentLoaded(&callback, &raw mut token) }
            .map_err(|error| failure("register DOMContentLoaded", &error))?;
        Ok(())
    }

    #[allow(unsafe_code)] // Test-only exact-fixture response and deny-all native permission/popup handlers.
    pub(super) fn register_resources(&self) -> Result<(), String> {
        let view = self.view()?;
        let mut token = 0;
        let environment = self
            .environment
            .as_ref()
            .ok_or("missing environment")?
            .clone();
        let document = self.document.clone();
        let owned_uri = self.uri.clone();
        let held_iframe = Rc::clone(&self.held_iframe);
        let trace = Rc::clone(&self.trace);
        let callback = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let observed = (|| {
                let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                // SAFETY: callback args return a request owned by WebView2 on this STA.
                let request = unsafe { args.Request()? };
                // SAFETY: live request; read_string frees the returned PWSTR.
                let uri = read_string(|out| unsafe { request.Uri(out) })?;
                let mut context = wv::COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
                // SAFETY: callback getter writes the owned context output.
                unsafe {
                    args.ResourceContext(&raw mut context)?;
                }
                // SAFETY: native request headers remain on this STA.
                let seen_header = header(&unsafe { request.Headers()? })?;
                let args2 = args.cast::<wv::ICoreWebView2WebResourceRequestedEventArgs2>()?;
                let mut requested_kind = wv::COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS(0);
                // SAFETY: exact native args2 and bounded scalar out-pointer on the creating STA.
                unsafe {
                    args2.RequestedSourceKind(&raw mut requested_kind)?;
                }
                // This is exact owned-fixture routing, not a file parser or containment mechanism.
                let is_owned = uri == owned_uri;
                // Hold a genuine same-URI iframe DOCUMENT response while the Host
                // starts its distinct top-level NavigationId. No injected nonce.
                let hold_child = is_owned
                    && context == wv::COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT
                    && requested_kind
                        == wv::COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT
                    && seen_header["state"] == "absent"
                    && trace.borrow().phase == "race-child";
                let mut deferral = if hold_child {
                    // SAFETY: exact callback args on the creating STA. The guard
                    // completes failures until Session takes lasting ownership.
                    Some(PendingIframeDeferral {
                        value: Some(unsafe { args.GetDeferral()? }),
                        trace: Rc::clone(&trace),
                    })
                } else {
                    None
                };
                // SAFETY: memory stream copies live bytes; only this one fixture URI receives them.
                let response = unsafe {
                    if is_owned {
                        let stream = SHCreateMemStream(Some(&document))
                            .ok_or_else(|| windows::core::Error::from(E_OUTOFMEMORY))?;
                        environment.CreateWebResourceResponse(&stream, 200, w!("OK"), w!("Content-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\n"))?
                    } else {
                        environment.CreateWebResourceResponse(
                            None::<&IStream>,
                            403,
                            w!("Forbidden"),
                            w!("Content-Length: 0\r\n"),
                        )?
                    }
                };
                // SAFETY: response and event args are live on their owning STA for this request.
                unsafe {
                    args.SetResponse(&response)?;
                }
                if let Some(guard) = deferral.as_mut() {
                    if held_iframe.borrow().is_some() {
                        return Err(windows::core::Error::from(E_POINTER));
                    }
                    let retained = guard
                        .value
                        .take()
                        .ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                    held_iframe.borrow_mut().replace(retained);
                    trace.borrow_mut().record(
                        "race-child-held",
                        json!({
                            "uri": uri, "header": seen_header, "source_kind": requested_kind.0,
                            "resource_context": context.0, "real_deferral": true
                        }),
                    );
                }
                Ok(
                    json!({"uri":uri,"resource_context":context.0,"header":seen_header,
                        "source_kind": requested_kind.0,"owned_fixture":is_owned,"held_iframe":hold_child}),
                )
            })();
            record_result(&trace, "resource-request", observed);
            Ok(())
        }));
        // SAFETY: callback registered before filter and before the first Navigate; view retains it.
        unsafe { view.add_WebResourceRequested(&callback, &raw mut token) }
            .map_err(|error| failure("register WRR", &error))?;
        let view22 = view
            .cast::<wv::ICoreWebView2_22>()
            .map_err(|error| failure("WebView2_22", &error))?;
        // SAFETY: filter is installed before any fixture load; every source kind is observed.
        unsafe {
            view22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                w!("file:*"),
                wv::COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                wv::COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )
        }
        .map_err(|error| failure("register source-kind filter", &error))?;
        Self::register_guard_denials(view)
    }

    #[allow(unsafe_code)] // Deny permissions and popups before first navigation.
    fn register_guard_denials(view: &wv::ICoreWebView2) -> Result<(), String> {
        let mut token = 0;
        let permission = PermissionRequestedEventHandler::create(Box::new(|_, args| {
            let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
            // SAFETY: native request is denied, never delegated to a user prompt or app grant.
            unsafe { args.SetState(wv::COREWEBVIEW2_PERMISSION_STATE_DENY) }
        }));
        // SAFETY: view retains this denial handler before any navigation.
        unsafe { view.add_PermissionRequested(&permission, &raw mut token) }
            .map_err(|error| failure("register permission denial", &error))?;
        let popup = NewWindowRequestedEventHandler::create(Box::new(|_, args| {
            let args = args.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
            // SAFETY: handling without creating a child view denies the popup.
            unsafe { args.SetHandled(true) }
        }));
        // SAFETY: view retains this popup-denial handler before any navigation.
        unsafe { view.add_NewWindowRequested(&popup, &raw mut token) }
            .map_err(|error| failure("register popup denial", &error))?;
        Ok(())
    }

    #[allow(unsafe_code)] // Only complete the exact owned native iframe deferral on this STA.
    pub(in super::super) fn release_held_iframe(&self) -> Result<(), String> {
        let deferral = self
            .held_iframe
            .borrow_mut()
            .take()
            .ok_or_else(|| "missing native held iframe response".to_owned())?;
        // SAFETY: the exact WebResourceRequested GetDeferral returned this STA-owned object.
        unsafe { deferral.Complete() }
            .map_err(|error| failure("complete held native iframe", &error))?;
        self.trace
            .borrow_mut()
            .record("race-child-released", json!({"completed":true}));
        Ok(())
    }
}
