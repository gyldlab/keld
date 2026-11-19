//! Prepared actual `WebView2` original-document request.
//! All allocation, native construction, QI and readback happen before admission.
//! The UI owner then begins the exact shared attempt under Core's role fence,
//! releases every Rust/role guard, and invokes the retained call immediately.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::RefCell;
use std::rc::Rc;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2_2, ICoreWebView2Environment, ICoreWebView2Environment2,
    ICoreWebView2WebResourceRequest,
};
use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::System::Com::IStream;
use windows::core::{HSTRING, Interface as _};

use super::native_text::read_task_text;
use super::original_callbacks::attempt_header;
use super::original_navigation::{HOST_ATTEMPT_HEADER, OriginalNavigation, PreparedLoad};
use super::original_resource::require_get_without_body;
use crate::engine::{NativeNavigationRefusal, OriginalEntryQuery};
use crate::resource_intake::ORIGINAL_WEBAPP_LIMITS;

/// No document identity has been minted merely by constructing this request.
pub(super) struct PreparedNativeLoad {
    prepared: Rc<PreparedLoad>,
    call: NativeLoadCall,
}

impl PreparedNativeLoad {
    pub(super) fn prepare(
        view: &ICoreWebView2,
        environment: &ICoreWebView2Environment,
        navigation: &Rc<RefCell<OriginalNavigation>>,
        query: &OriginalEntryQuery,
    ) -> Result<Self, NativeNavigationRefusal> {
        let prepared = navigation
            .try_borrow()
            .map_err(|_| NativeNavigationRefusal::Unavailable)?
            .prepare(query)?;
        // No navigation RefCell borrow is alive across any native entry.
        let view = view
            .cast::<ICoreWebView2_2>()
            .map_err(|_| NativeNavigationRefusal::Unavailable)?;
        let environment = environment
            .cast::<ICoreWebView2Environment2>()
            .map_err(|_| NativeNavigationRefusal::Unavailable)?;
        let request = create_checked_request(&environment, &prepared)
            .map_err(|_| NativeNavigationRefusal::NativeUrlRejected)?;
        Ok(Self {
            call: NativeLoadCall {
                view,
                request,
                prepared: Rc::clone(&prepared),
            },
            prepared,
        })
    }

    /// The same prepared object enters the shared owner exactly once. Keeping
    /// the callable separate lets the final fence contain local state only.
    pub(super) fn into_parts(self) -> (Rc<PreparedLoad>, NativeLoadCall) {
        (self.prepared, self.call)
    }
}

/// Same-STA retained call target; no subsequent lookup can redirect this load.
pub(super) struct NativeLoadCall {
    view: ICoreWebView2_2,
    request: ICoreWebView2WebResourceRequest,
    prepared: Rc<PreparedLoad>,
}

pub(super) enum LoadDispatch {
    NotAdmitted,
    Returned(windows::core::Result<()>),
    OutcomeUnknown,
}

impl NativeLoadCall {
    #[allow(unsafe_code)] // Exact admitted GET on the retained creating-STA view.
    #[expect(
        clippy::single_match_else,
        reason = "Preserve the explicit native-return and unwind arms with their original payload drop scopes"
    )]
    pub(super) fn invoke(&self) -> LoadDispatch {
        if !self.prepared.begin_native_call() {
            return LoadDispatch::NotAdmitted;
        }
        // SAFETY: the attached OriginalView retains these exact interfaces and
        // the same prepared argument/credit owner through native return or unwind.
        // The Core fence just marked entry and released all interior guards.
        // No getter, allocation, pump or user callback intervenes.
        let returned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            self.view.NavigateWithWebResourceRequest(&self.request)
        }));
        match returned {
            Ok(result) => {
                if self.prepared.native_returned(result.is_ok()) {
                    LoadDispatch::Returned(result)
                } else {
                    LoadDispatch::OutcomeUnknown
                }
            }
            Err(_) => {
                self.prepared.native_unwound();
                LoadDispatch::OutcomeUnknown
            }
        }
    }

    pub(super) fn native_quiesced(&self) -> bool {
        self.prepared.native_quiesced()
    }
}

#[allow(unsafe_code)] // Exact request creation and bounded creating-STA readback.
fn create_checked_request(
    environment: &ICoreWebView2Environment2,
    prepared: &PreparedLoad,
) -> windows::core::Result<ICoreWebView2WebResourceRequest> {
    // PreparedLoad already retains the source-account metadata reservation for
    // these temporary native UTF16 fields and their bounded readback copies.
    let uri = HSTRING::from(prepared.uri());
    let headers = HSTRING::from(format!(
        "{HOST_ATTEMPT_HEADER}: {}",
        prepared.header_value(),
    ));
    // SAFETY: synchronous call with live NUL-terminated input strings. The body
    // is explicitly absent; the returned request remains on the creating STA.
    // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2environment2#createwebresourcerequest
    let request = unsafe {
        environment.CreateWebResourceRequest(
            &uri,
            &HSTRING::from("GET"),
            None::<&IStream>,
            &headers,
        )
    }?;
    require_get_without_body(&request)?;
    // SAFETY: retained actual request and a bounded RAII task-string output.
    let actual_uri = read_task_text(
        |out| unsafe { request.Uri(out) },
        ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
        4 * ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units,
    )?;
    // SAFETY: returned headers remain owned on this creating STA.
    let actual_header = attempt_header(&unsafe { request.Headers() }?)?;
    if !prepared.qualifies_uri(&actual_uri)
        || actual_header.as_deref() != Some(prepared.header_value())
    {
        return Err(E_INVALIDARG.into());
    }
    Ok(request)
}
