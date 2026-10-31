//! Reconcile saved `WebView2` camera/microphone permission decisions to fail-closed state.
//! The live event handler, popup denial and first-navigation guard remain parent-owned.

use std::sync::mpsc;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
    COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE,
    COREWEBVIEW2_PERMISSION_STATE_DENY, ICoreWebView2, ICoreWebView2_13, ICoreWebView2Profile4,
};
use webview2_com::{
    GetNonDefaultPermissionSettingsCompletedHandler, SetPermissionStateCompletedHandler,
    wait_with_pump,
};
use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::{Interface as _, PCWSTR, PWSTR};

use super::profile_failure;
use crate::error::WvError;
use crate::profile::ProfileErrorKind;

pub(super) struct SavedPermission {
    pub(super) kind: COREWEBVIEW2_PERMISSION_KIND,
    pub(super) state: COREWEBVIEW2_PERMISSION_STATE,
    pub(super) origin: String,
}

pub(super) fn saved_media_permission_needs_deny(permission: &SavedPermission) -> bool {
    matches!(
        permission.kind,
        COREWEBVIEW2_PERMISSION_KIND_CAMERA | COREWEBVIEW2_PERMISSION_KIND_MICROPHONE
    ) && permission.state != COREWEBVIEW2_PERMISSION_STATE_DENY
}

pub(super) fn profile_permission_settings(
    profile: &ICoreWebView2Profile4,
) -> Result<Vec<SavedPermission>, WvError> {
    let (tx, rx) = mpsc::channel();
    let handler = GetNonDefaultPermissionSettingsCompletedHandler::create(Box::new(
        move |result, settings| {
            tx.send(result.and(settings.ok_or_else(|| windows::core::Error::from(E_POINTER))))
                .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
        },
    ));
    // SAFETY: the profile and one-shot completion handler live on this STA;
    // `wait_with_pump` retains the receiver until the callback completes.
    unsafe { profile.GetNonDefaultPermissionSettings(&handler) }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let settings = wait_with_pump(rx)
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let mut count = 0_u32;
    // SAFETY: the returned collection is live and `count` is writable.
    unsafe { settings.Count(&raw mut count) }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let mut permissions = Vec::new();
    for index in 0..count {
        // SAFETY: `index` is bounded by the collection's observed count.
        let setting = unsafe { settings.GetValueAtIndex(index) }
            .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
        let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
        let mut state = COREWEBVIEW2_PERMISSION_STATE::default();
        let mut origin = PWSTR::null();
        // SAFETY: this live setting owns all three synchronous getter outputs.
        let observed = unsafe { setting.PermissionKind(&raw mut kind) }
            .and_then(|()| unsafe { setting.PermissionState(&raw mut state) })
            .and_then(|()| unsafe { setting.PermissionOrigin(&raw mut origin) });
        observed.map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
        // SAFETY: the successful origin getter returned one COM-allocated,
        // NUL-terminated string. It is copied before being freed exactly once.
        let text = unsafe { origin.to_string() };
        // SAFETY: this is the one allocation returned by PermissionOrigin.
        unsafe { CoTaskMemFree(Some(origin.as_ptr().cast())) };
        permissions.push(SavedPermission {
            kind,
            state,
            origin: text.map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?,
        });
    }
    Ok(permissions)
}

pub(super) fn set_profile_permission_state(
    profile: &ICoreWebView2Profile4,
    permission: &SavedPermission,
    state: COREWEBVIEW2_PERMISSION_STATE,
) -> Result<(), WvError> {
    let origin = permission
        .origin
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let (tx, rx) = mpsc::channel();
    let handler = SetPermissionStateCompletedHandler::create(Box::new(move |result| {
        tx.send(result)
            .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
    }));
    // SAFETY: the profile, NUL-terminated origin and one-shot handler live on
    // this STA through the pumped completion. The current v0 webview policy is
    // fail-closed, so persisted camera/microphone decisions are reconciled to
    // deny before any app navigation can consult them.
    unsafe {
        profile.SetPermissionState(permission.kind, PCWSTR(origin.as_ptr()), state, &handler)
    }
    .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    wait_with_pump(rx)
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))
}

pub(super) fn webview_profile(webview: &ICoreWebView2) -> Result<ICoreWebView2Profile4, WvError> {
    let webview13: ICoreWebView2_13 = webview
        .cast()
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    // SAFETY: the webview is live on its owning STA.
    unsafe { webview13.Profile() }
        .and_then(|profile| profile.cast())
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))
}

pub(super) fn reconcile_saved_media_permissions(webview: &ICoreWebView2) -> Result<(), WvError> {
    let profile = webview_profile(webview)?;
    for permission in profile_permission_settings(&profile)?
        .iter()
        .filter(|permission| saved_media_permission_needs_deny(permission))
    {
        set_profile_permission_state(&profile, permission, COREWEBVIEW2_PERMISSION_STATE_DENY)?;
    }
    Ok(())
}

pub(super) fn saved_permission_reconciliation_enabled() -> bool {
    #[cfg(all(feature = "media-acceptance", test))]
    if std::env::var_os("KELD_PROFILE_TEST_SKIP_SAVED_RECONCILIATION").is_some() {
        return false;
    }
    true
}
