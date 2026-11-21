//! Guard-first Windows `WebView2` navigation and live permission/popup denial.
//! Registration and its unforgeable initial-navigation proof live together.

use keld_guard::{PermissionsManifest, Principal};
#[cfg(all(feature = "media-acceptance", test))]
use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PERMISSION_STATE_DEFAULT;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_STATE,
    COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY, ICoreWebView2,
};
use webview2_com::{NewWindowRequestedEventHandler, PermissionRequestedEventHandler};
use windows::Win32::Foundation::E_POINTER;
use windows::core::{HSTRING, IUnknown, Interface};

#[cfg(all(feature = "media-acceptance", test))]
use super::media_acceptance;
use crate::engine::NavTarget;
use crate::error::WvError;
use crate::media::{media_permission_allowed, webview2_media_kind};

/// Proof that media permissions and popup denial are registered on a webview.
///
/// [`navigate_initial`] demands it, so "content ran before the guard existed"
/// is a compile error rather than a review catch. Only
/// [`install_guarded_media_permissions`] mints one.
pub(super) struct GuardInstalled<'view>(&'view ICoreWebView2);

pub(super) fn webview2_permission_state(allowed: bool) -> COREWEBVIEW2_PERMISSION_STATE {
    if allowed {
        COREWEBVIEW2_PERMISSION_STATE_ALLOW
    } else {
        COREWEBVIEW2_PERMISSION_STATE_DENY
    }
}

pub(super) fn canonical_webview_identity(webview: &ICoreWebView2) -> windows::core::Result<usize> {
    let identity: IUnknown = webview.cast()?;
    Ok(identity.as_raw() as usize)
}

/// Registers the default-deny media-capture handler backed by `keld-guard`
/// (KEL-59 parity with the macOS backend).
///
/// Without a handler `WebView2` falls back to its own **user prompt** —
/// default-ask, not default-deny. A prompt lets a renderer obtain capture the
/// manifest never granted, and the manifest is the authority
/// (`docs/architecture/03-security.md` §1). Kinds without a Keld capability
/// (geolocation, notifications, …) are denied outright: there is no v0
/// capability for them, and fail-closed is the crate rule.
pub(super) fn install_guarded_media_permissions(
    webview: &ICoreWebView2,
    manifest: PermissionsManifest,
    principal: Principal,
) -> Result<GuardInstalled<'_>, WvError> {
    let registered_identity = canonical_webview_identity(webview)
        .map_err(|error| WvError::Webview(format!("permission identity: {error}")))?;
    // Built outside the registration's `unsafe` block so the COM calls inside
    // the callback carry their own SAFETY proofs instead of inheriting one
    // lexically.
    let handler = PermissionRequestedEventHandler::create(Box::new(move |sender, args| {
        // Fail closed: without args no state can be set, and `Ok(())` would
        // silently hand the decision back to WebView2's own prompt
        // (default-ask). An error at least refuses to report success.
        let Some(args) = args else {
            return Err(windows::core::Error::from(E_POINTER));
        };
        let Ok(Some(sender_identity)) = sender.as_ref().map(canonical_webview_identity).transpose()
        else {
            // SAFETY: failure to canonicalize the sender is an identity
            // failure, so complete the request as denied.
            return unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) };
        };
        if sender_identity != registered_identity {
            // SAFETY: live callback args; a foreign/missing sender must never
            // fall through to WebView2's DEFAULT prompt behavior.
            return unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) };
        }

        let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
        // SAFETY: `args` is live for the duration of the callback; the
        // out-pointer is valid.
        if unsafe { args.PermissionKind(&raw mut kind) }.is_err() {
            // SAFETY: the same live callback args accept a by-value state.
            // Enforcing DENY takes precedence over propagating a getter error
            // that could leave WebView2 on DEFAULT/prompt behavior.
            return unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) };
        }
        #[cfg(all(feature = "media-acceptance", test))]
        let before = {
            let mut before = COREWEBVIEW2_PERMISSION_STATE_DEFAULT;
            // SAFETY: same live callback arguments and writable enum output.
            if unsafe { args.State(&raw mut before) }.is_err() {
                // SAFETY: keep the evidence-only getter fail-closed too.
                return unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) };
            }
            before
        };

        let state = webview2_permission_state(media_permission_allowed(
            &manifest,
            Some(principal),
            webview2_media_kind(kind),
        ));
        // SAFETY: same liveness as above; `SetState` takes the enum by value.
        unsafe { args.SetState(state) }?;
        #[cfg(all(feature = "media-acceptance", test))]
        {
            let mut readback = COREWEBVIEW2_PERMISSION_STATE_DEFAULT;
            // SAFETY: the callback still owns the same live args. Reading the
            // value here proves the state before this callback returns.
            if unsafe { args.State(&raw mut readback) }.is_err() {
                return Ok(());
            }
            let Ok(uri) = media_acceptance::permission_uri(&args) else {
                return Ok(());
            };
            media_acceptance::observe_production_effect(
                kind,
                before,
                state,
                readback,
                uri,
                sender_identity,
            );
        }
        Ok(())
    }));

    let mut token = 0_i64;
    // SAFETY: `webview` lives on this thread (module note). The handler runs
    // on this same thread for the webview's whole life — WebView2 raises
    // events on the creating thread.
    let registered = unsafe { webview.add_PermissionRequested(&handler, &raw mut token) };
    registered.map_err(|err| WvError::Webview(format!("permission handler: {err}")))?;
    #[cfg(all(feature = "media-acceptance", test))]
    media_acceptance::observe_registration(registered_identity, token);

    // Windows WebView2 (KEL-168): an unhandled new-window request creates a
    // popup outside Keld's principal, permission, and lifecycle accounting.
    // v0 denies every popup; SetHandled(true) without NewWindow loads nothing.
    // https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2newwindowrequestedeventargs#put_handled
    let popup_handler = NewWindowRequestedEventHandler::create(Box::new(|_, args| {
        let Some(args) = args else {
            return Err(windows::core::Error::from(E_POINTER));
        };
        // SAFETY: WebView2 supplies live event args on the creating STA thread
        // for this callback. SetHandled takes a BOOL by value (contract above).
        unsafe { args.SetHandled(true) }
    }));
    // SAFETY: `webview` and the writable token are live on the creating STA.
    // WebView2 retains the COM handler and invokes it on that same thread.
    unsafe { webview.add_NewWindowRequested(&popup_handler, &raw mut token) }
        .map_err(|err| WvError::Webview(format!("popup handler: {err}")))?;
    Ok(GuardInstalled(webview))
}

/// Performs the first navigation of a freshly created webview.
///
/// Takes [`GuardInstalled`] so the type system enforces the crate rule that no
/// content runs before the permission guard is registered.
pub(super) fn navigate_initial(
    guard: &GuardInstalled<'_>,
    target: &NavTarget,
) -> Result<(), WvError> {
    dispatch_initial(
        target,
        // SAFETY: the guarded webview lives on this STA; each borrowed argument
        // remains valid until its synchronous COM call returns.
        |html| unsafe { guard.0.NavigateToString(html) },
        // SAFETY: the URL buffer has the same lifetime as the HTML buffer above.
        |url| unsafe { guard.0.Navigate(url) },
    )
}

/// Encodes text only if a `WebView2` `LPCWSTR` argument can carry it intact.
/// See [`ICoreWebView2`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2#navigatetostring).
pub(super) fn pcwstr_argument(text: &str) -> Option<HSTRING> {
    (!text.as_bytes().contains(&0)).then(|| HSTRING::from(text))
}

fn dispatch_initial(
    target: &NavTarget,
    navigate_to_string: impl FnOnce(&HSTRING) -> windows::core::Result<()>,
    navigate: impl FnOnce(&HSTRING) -> windows::core::Result<()>,
) -> Result<(), WvError> {
    let (NavTarget::Html(text) | NavTarget::Url(text)) = target;
    let argument = pcwstr_argument(text).ok_or_else(|| {
        WvError::Navigate(
            "initial navigation contains a NUL character; remove it before loading".to_owned(),
        )
    })?;
    let result = match target {
        NavTarget::Html(_) => navigate_to_string(&argument),
        NavTarget::Url(_) => navigate(&argument),
    };
    result.map_err(|err| WvError::Navigate(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_navigation_rejects_nul_before_either_native_call() {
        for value in ["\0suffix", "private-prefix\0private-suffix", "prefix\0"] {
            for target in [NavTarget::Html(value.into()), NavTarget::Url(value.into())] {
                let result = dispatch_initial(
                    &target,
                    |_| panic!("submitted a truncated initial document"),
                    |_| panic!("submitted a truncated initial URL"),
                );
                let Err(WvError::Navigate(detail)) = result else {
                    panic!("expected a navigation refusal");
                };
                assert!(!detail.contains("private"));
            }
        }
    }

    #[test]
    fn initial_navigation_submits_the_complete_argument_to_the_selected_method() {
        let html = "<p>東京 🦀 %00</p>";
        let url = "file:///C:/東京/%00.html";
        let mut submitted = Vec::new();
        for value in ["", html] {
            dispatch_initial(
                &NavTarget::Html(value.into()),
                |arg| {
                    submitted.push(arg.to_vec());
                    Ok(())
                },
                |_| panic!("HTML routed to Navigate"),
            )
            .expect("valid HTML submitted");
        }
        dispatch_initial(
            &NavTarget::Url(url.into()),
            |_| panic!("URL routed to NavigateToString"),
            |arg| {
                submitted.push(arg.to_vec());
                Ok(())
            },
        )
        .expect("valid URL submitted");
        assert_eq!(
            submitted,
            ["", html, url].map(|value| value.encode_utf16().collect::<Vec<_>>())
        );
    }
}
