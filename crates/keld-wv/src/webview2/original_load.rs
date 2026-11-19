//! Prepared actual `WebView2` original-document request.
//! All allocation, native construction, QI and readback happen before admission.
//! The UI owner then begins the exact shared attempt under Core's role fence,
//! releases every Rust/role guard, and invokes the retained call immediately.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::rc::{Rc, Weak};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2_2, ICoreWebView2Environment2, ICoreWebView2WebResourceRequest,
};
use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::System::Com::IStream;
use windows::core::{HSTRING, IUnknown, Interface as _};

use super::native_text::read_task_text;
use super::original_callbacks::attempt_header;
use super::original_navigation::{HOST_ATTEMPT_HEADER, OriginalNavigation, PreparedLoad};
use super::original_resource::{ResourceView, require_get_without_body};
use crate::engine::{NativeNavigationRefusal, OriginalEntryQuery};
use crate::resource_intake::ORIGINAL_WEBAPP_LIMITS;

// GH1185 ?4.6, Q22(a,b). Windows x64 native capture, 2026-11-19:
// both native header surfaces observed; absent on plain/page/same-URI-frame controls.
// Probe source 6c38cceee701439dec873000e444f66bd94a329e.
// Executable SHA-256 7510c5d25998fa66b93267ed2b2d0cd87b7be364358b4e30aeb235ccba2008e5.
// winq22-current-runtime-20261119-240ec7 / keld-w3-q22-32020-19f864345374bcdb4922b996c05b9558
// receipt.json SHA-256 0a063e18ac070efea3296deb609b389cfadf747602eae1ae9e471fa18f3a5481.
// This receipt does not cover reload (Q22c).
const Q22_AB_WINDOWS_X64: &str = "157.0.4341.60";

fn require_q22_ab(observed: Option<&str>) -> Result<(), NativeNavigationRefusal> {
    if cfg!(target_arch = "x86_64") && observed == Some(Q22_AB_WINDOWS_X64) {
        Ok(())
    } else {
        Err(NativeNavigationRefusal::FileHeadersUnproven)
    }
}

/// Only this module may mint an admission input after inspecting the actual
/// native target. Preparation alone has no generation or document side effect.
pub(super) struct QualifiedHostLoad {
    prepared: Rc<PreparedLoad>,
    owner: Weak<ResourceView>,
}

impl QualifiedHostLoad {
    /// The production caller obtains this observation from the retained view's
    /// creating environment. No other module can construct this input.
    fn from_version_observation(
        owner: &Rc<ResourceView>,
        prepared: Rc<PreparedLoad>,
        observed: Option<&str>,
    ) -> Result<Self, NativeNavigationRefusal> {
        require_q22_ab(observed)?;
        Ok(Self {
            prepared,
            owner: Rc::downgrade(owner),
        })
    }

    pub(super) fn into_prepared_for(
        self,
        navigation: &OriginalNavigation,
    ) -> Result<Rc<PreparedLoad>, NativeNavigationRefusal> {
        let owner = self
            .owner
            .upgrade()
            .ok_or(NativeNavigationRefusal::Unavailable)?;
        if !owner.holds_live_load_owner(navigation) {
            return Err(NativeNavigationRefusal::Unavailable);
        }
        Ok(self.prepared)
    }
}

pub(super) struct PreparedNativeLoad {
    qualified: QualifiedHostLoad,
    call: NativeLoadCall,
}

impl PreparedNativeLoad {
    #[allow(unsafe_code)] // Only the actual bound STA view's creating environment and version getters.
    pub(super) fn prepare(
        owner: &Rc<ResourceView>,
        query: &OriginalEntryQuery,
    ) -> Result<Self, NativeNavigationRefusal> {
        let (core, expected_environment, navigation) = owner.original_load_target();
        // Reserve the existing prepared-load metadata before the bounded native getters.
        // Version readback finishes before the URI/header readbacks use this allowance.
        let prepared = {
            let navigation = navigation
                .try_borrow()
                .map_err(|_| NativeNavigationRefusal::Unavailable)?;
            if !owner.holds_live_load_owner(&navigation) {
                return Err(NativeNavigationRefusal::Unavailable);
            }
            navigation.prepare(query)?
        };
        let view = core
            .cast::<ICoreWebView2_2>()
            .map_err(|_| NativeNavigationRefusal::FileHeadersUnproven)?;
        // SAFETY: the ResourceView retains this live Core on the creating STA.
        // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2_2#get_environment
        let environment = unsafe { view.Environment() }
            .map_err(|_| NativeNavigationRefusal::FileHeadersUnproven)?;
        let identity = environment
            .cast::<IUnknown>()
            .map_err(|_| NativeNavigationRefusal::FileHeadersUnproven)?;
        if identity.as_raw() as usize != expected_environment {
            return Err(NativeNavigationRefusal::FileHeadersUnproven);
        }
        // SAFETY: this exact environment owns the target view. read_task_text supplies
        // an initialized PWSTR out-pointer and frees its task allocation on every return.
        // https://learn.microsoft.com/microsoft-edge/webview2/reference/win32/icorewebview2environment#get_browserversionstring
        let version = read_task_text(
            |out| unsafe { environment.BrowserVersionString(out) },
            Q22_AB_WINDOWS_X64.len(),
            Q22_AB_WINDOWS_X64.len(),
        );
        let qualified =
            QualifiedHostLoad::from_version_observation(owner, prepared, version.as_deref().ok())?;
        let environment = environment
            .cast::<ICoreWebView2Environment2>()
            .map_err(|_| NativeNavigationRefusal::Unavailable)?;
        let request = create_checked_request(&environment, &qualified.prepared)
            .map_err(|_| NativeNavigationRefusal::NativeUrlRejected)?;
        Ok(Self {
            call: NativeLoadCall {
                view,
                request,
                prepared: Rc::clone(&qualified.prepared),
            },
            qualified,
        })
    }

    pub(super) fn into_parts(self) -> (QualifiedHostLoad, NativeLoadCall) {
        (self.qualified, self.call)
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

#[cfg(test)]
pub(in crate::webview2) mod tests;
