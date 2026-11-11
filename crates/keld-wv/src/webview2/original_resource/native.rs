//! Narrow `WebView2` request/response operations on the owning UI apartment.
//! No filesystem path is opened here and no COM interface goes to the reader.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use crate::resource_intake::{NativeRequestShape, ORIGINAL_WEBAPP_LIMITS, check_shape};
use crate::resource_source::VerifiedBytes;
use crate::webview2::native_text::{read_task_text, read_task_text_pair};
use crate::webview2::original_navigation::HOST_ATTEMPT_HEADER;
use std::fmt::Write as _;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT,
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS,
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT,
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_SERVICE_WORKER,
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_SHARED_WORKER, ICoreWebView2, ICoreWebView2_22,
    ICoreWebView2Deferral, ICoreWebView2Environment, ICoreWebView2WebResourceRequest,
    ICoreWebView2WebResourceRequestedEventArgs, ICoreWebView2WebResourceRequestedEventArgs2,
    ICoreWebView2WebResourceResponse,
};
use windows::Win32::Foundation::{E_INVALIDARG, E_OUTOFMEMORY};
use windows::Win32::System::Com::IStream;
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::core::{BOOL, HSTRING, Interface, Result as WinResult, w};

pub(super) struct CapturedRequest {
    pub(super) uri: String,
    pub(super) nonce: Option<String>,
    pub(super) document: bool,
    pub(super) source_kind: COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS,
}

#[derive(Clone, Copy)]
pub(super) enum Refusal {
    Forbidden,
    TooLarge,
    Capacity,
}

impl Refusal {
    fn parts(self) -> (i32, windows::core::PCWSTR) {
        match self {
            Self::Forbidden => (403, w!("Forbidden")),
            Self::TooLarge => (413, w!("Content Too Large")),
            Self::Capacity => (503, w!("Service Unavailable")),
        }
    }
}

#[allow(unsafe_code)] // UI-owned SDK event registration; no pointer escapes its owner.
pub(super) fn register(
    core: &ICoreWebView2,
    handler: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2WebResourceRequestedEventHandler,
) -> WinResult<i64> {
    let mut token = 0;
    // SAFETY: the retained core and callback are from this UI apartment; token
    // is a valid out-pointer. The ResourceView retains the registration owner.
    unsafe { core.add_WebResourceRequested(handler, &raw mut token) }?;
    Ok(token)
}

#[allow(unsafe_code)] // Fixed file-only filter on the retained UI-owned core.
pub(super) fn add_filter(core: &ICoreWebView2, workers: bool) -> WinResult<()> {
    let core = core.cast::<ICoreWebView2_22>()?;
    let kinds = if workers {
        COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_SHARED_WORKER
            | COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_SERVICE_WORKER
    } else {
        COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT
    };
    // SAFETY: fixed NUL-terminated filter and valid enum flags. Installation
    // precedes the view's first file navigation. Worker kinds have one session
    // owner per actual environment, transferred before its old view closes.
    unsafe {
        core.AddWebResourceRequestedFilterWithRequestSourceKinds(
            w!("file:*"),
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            kinds,
        )
    }
}

#[allow(unsafe_code)] // Remove only the exact worker filter after a successor owns it.
pub(super) fn remove_worker_filter(core: &ICoreWebView2) -> WinResult<()> {
    let core = core.cast::<ICoreWebView2_22>()?;
    // SAFETY: same retained core, fixed filter and flags used at registration.
    unsafe {
        core.RemoveWebResourceRequestedFilterWithRequestSourceKinds(
            w!("file:*"),
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_SHARED_WORKER
                | COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_SERVICE_WORKER,
        )
    }
}

#[allow(unsafe_code)] // Bounded getters on callback-owned native request objects.
pub(super) fn capture(
    args: &ICoreWebView2WebResourceRequestedEventArgs,
    source_kind: COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS,
) -> WinResult<CapturedRequest> {
    // SAFETY: args is the real callback's retained object on its UI apartment.
    let request = unsafe { args.Request() }?;
    require_get_without_body(&request)?;
    let uri = read_task_text(
        |out| {
            // SAFETY: native_text owns this initialized out-pointer and task allocation.
            unsafe { request.Uri(out) }
        },
        ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
        4 * ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
    )?;
    // SAFETY: request/header/iterator remain on the actual callback's apartment.
    let headers = unsafe { request.Headers() }?;
    // SAFETY: the retained headers own the collection throughout this bounded walk.
    let iterator = unsafe { headers.GetIterator() }?;
    let mut has = BOOL(0);
    // SAFETY: initialized scalar out-pointer; live iterator on this UI thread.
    unsafe { iterator.HasCurrentHeader(&raw mut has) }?;
    let mut fields = 0;
    let mut units = 0;
    let mut nonce = None;
    while has.as_bool() {
        if fields == ORIGINAL_WEBAPP_LIMITS.max_header_fields {
            return Err(E_INVALIDARG.into());
        }
        let remaining = ORIGINAL_WEBAPP_LIMITS.max_header_utf16_units - units;
        let (name, value) = read_task_text_pair(
            |name, value| {
                // SAFETY: the sole paired native_text owner supplies both initialized
                // pointers, bounds both allocations before copy, and frees each once.
                unsafe { iterator.GetCurrentHeader(name, value) }
            },
            remaining,
            4 * remaining,
            |name| {
                if name.eq_ignore_ascii_case(HOST_ATTEMPT_HEADER) {
                    (32, 32)
                } else {
                    (usize::MAX, usize::MAX)
                }
            },
        )?;
        units += name.encode_utf16().count() + value.encode_utf16().count();
        fields += 1;
        if name.eq_ignore_ascii_case(HOST_ATTEMPT_HEADER) {
            if nonce.is_some() {
                return Err(E_INVALIDARG.into());
            }
            nonce = Some(value);
        }
        // SAFETY: same retained iterator and initialized scalar out-pointer.
        unsafe { iterator.MoveNext(&raw mut has) }?;
    }
    check_shape(
        NativeRequestShape {
            is_get: true,
            has_body: false,
            has_body_stream: false,
            url_utf16_units: uri.encode_utf16().count(),
            header_fields: fields,
            header_utf16_units: units,
        },
        ORIGINAL_WEBAPP_LIMITS,
    )
    .map_err(|_| windows::core::Error::from(E_INVALIDARG))?;
    let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
    // SAFETY: actual retained args and initialized enum out-pointer on the STA.
    unsafe { args.ResourceContext(&raw mut context) }?;
    Ok(CapturedRequest {
        uri,
        nonce,
        document: context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT,
        source_kind,
    })
}

#[allow(unsafe_code)] // Scalar source-kind getter on the exact callback object.
pub(super) fn source_kind(
    args: &ICoreWebView2WebResourceRequestedEventArgs,
) -> WinResult<COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS> {
    let args2 = args.cast::<ICoreWebView2WebResourceRequestedEventArgs2>()?;
    let mut source_kind = COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS(0);
    // SAFETY: actual callback args2 and initialized enum out-pointer. This native
    // source kind, not context/URI alone, excludes worker callbacks from attribution.
    unsafe { args2.RequestedSourceKind(&raw mut source_kind) }?;
    if source_kind != COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_DOCUMENT
        && source_kind != COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_SHARED_WORKER
        && source_kind != COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_SERVICE_WORKER
    {
        return Err(E_INVALIDARG.into());
    }
    Ok(source_kind)
}

#[allow(unsafe_code)] // Bounded actual method and nullable body presence, without reading content.
pub(in super::super) fn require_get_without_body(
    request: &ICoreWebView2WebResourceRequest,
) -> WinResult<()> {
    let method = read_task_text(
        |out| {
            // SAFETY: live creating-STA request and native_text's initialized output.
            unsafe { request.Method(out) }
        },
        16,
        16,
    )?;
    if method != "GET" || has_content(request)? {
        return Err(E_INVALIDARG.into());
    }
    Ok(())
}

#[allow(unsafe_code)] // Preserve nullable COM content without reading a body/stream.
fn has_content(request: &ICoreWebView2WebResourceRequest) -> WinResult<bool> {
    let mut raw = std::ptr::null_mut();
    // SAFETY: same UI-owned request and one initialized nullable out-pointer.
    // The generated nonnullable wrapper loses the distinction between S_OK/null
    // and a failed getter; use its exact vtable operation and own any returned
    // interface exactly once, including the failed-HRESULT path.
    let result = unsafe { (request.vtable().Content)(request.as_raw(), &raw mut raw) };
    let content = if raw.is_null() {
        None
    } else {
        // SAFETY: this is the getter's one owned IStream reference, never a
        // borrowed or fabricated pointer. It is dropped on this same UI turn.
        Some(unsafe { IStream::from_raw(raw) })
    };
    result.ok()?;
    Ok(content.is_some())
}

#[allow(unsafe_code)] // Native deferral remains entirely with the UI pending record.
pub(super) fn defer(
    args: &ICoreWebView2WebResourceRequestedEventArgs,
) -> WinResult<ICoreWebView2Deferral> {
    // SAFETY: actual callback-owned args; the caller retains and completes this
    // exact returned object once, after a response has been installed.
    unsafe { args.GetDeferral() }
}

#[allow(unsafe_code)] // Fixed empty refusal prevents fallback to Chromium's file loader.
pub(super) fn refuse(
    environment: &ICoreWebView2Environment,
    args: &ICoreWebView2WebResourceRequestedEventArgs,
    refusal: Refusal,
) -> WinResult<()> {
    let (status, reason) = refusal.parts();
    // SAFETY: all objects are live on the same UI apartment; null content means
    // an empty body. Fixed headers contain no page-derived text or path.
    let response = unsafe {
        environment.CreateWebResourceResponse(
        None::<&IStream>, status, reason,
        w!("Content-Length: 0\r\nCache-Control: no-store\r\nContent-Type: text/plain; charset=utf-8"),
    )
    }?;
    // SAFETY: the empty response and exact callback args remain on the STA.
    unsafe { args.SetResponse(&response) }
}

#[allow(unsafe_code)] // SDK-owned memory stream is created and passed only on the STA.
pub(super) fn response(
    environment: &ICoreWebView2Environment,
    body: &VerifiedBytes,
    document_policy: Option<&str>,
) -> WinResult<ICoreWebView2WebResourceResponse> {
    if body.bytes.len() > ORIGINAL_WEBAPP_LIMITS.max_asset_bytes {
        return Err(E_INVALIDARG.into());
    }
    // SAFETY: bounded immutable initialized bytes live for the creation call;
    // SHCreateMemStream creates the complete SDK-owned memory stream. It is
    // never supplied to a Host I/O thread. Native post-Complete lifetime is Q29.
    // https://learn.microsoft.com/windows/win32/api/shlwapi/nf-shlwapi-shcreatememstream
    let stream = unsafe { SHCreateMemStream(Some(&body.bytes)) }.ok_or(E_OUTOFMEMORY)?;
    let mut headers = String::new();
    headers
        .try_reserve_exact(256 + body.mime.len() + document_policy.map_or(0, str::len))
        .map_err(|_| E_OUTOFMEMORY)?;
    write!(
        &mut headers,
        "Content-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store",
        body.mime,
        body.bytes.len()
    )
    .map_err(|_| E_OUTOFMEMORY)?;
    if let Some(policy) = document_policy {
        headers.push_str("\r\nContent-Security-Policy: ");
        headers.push_str(policy);
    }
    let headers = HSTRING::from(headers);
    // SAFETY: response/stream/environment stay on the UI apartment; complete
    // content and bounded trusted headers precede the request's Complete call.
    unsafe { environment.CreateWebResourceResponse(&stream, 200, w!("OK"), &headers) }
}

#[allow(unsafe_code)] // One checked response effect on the exact pending native request.
pub(super) fn set_response(
    args: &ICoreWebView2WebResourceRequestedEventArgs,
    response: &ICoreWebView2WebResourceResponse,
) -> WinResult<()> {
    // SAFETY: both retained UI-owned objects belong to this pending callback.
    unsafe { args.SetResponse(response) }
}

#[allow(unsafe_code)] // Caller consumes the sole deferral before crossing this boundary.
pub(super) fn complete(deferral: ICoreWebView2Deferral) -> WinResult<()> {
    // SAFETY: this is the unique pending record's real deferral. It is moved out
    // before entry, so failure/reentry never retries Complete on the same object.
    let completed = unsafe { deferral.Complete() };
    drop(deferral);
    completed
}
