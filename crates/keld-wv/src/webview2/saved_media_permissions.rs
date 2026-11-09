//! Reconcile saved `WebView2` camera/microphone permission decisions to fail-closed state.
//! The live event handler, popup denial and first-navigation guard remain parent-owned.

use std::sync::mpsc;
use std::time::Instant;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
    COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE,
    COREWEBVIEW2_PERMISSION_STATE_DENY, ICoreWebView2, ICoreWebView2_13, ICoreWebView2Profile4,
};
use webview2_com::{
    GetNonDefaultPermissionSettingsCompletedHandler, SetPermissionStateCompletedHandler,
};
use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::{Interface as _, PCWSTR, PWSTR};

use super::{check_launch_deadline, profile_failure, wait_for_launch_with_message_pump_until};

#[cfg(all(feature = "media-acceptance", test))]
pub(super) mod test_control;
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
    deadline: Instant,
) -> Result<Vec<SavedPermission>, WvError> {
    check_launch_deadline(deadline)?;
    let (tx, rx) = mpsc::channel();
    #[cfg(all(feature = "media-acceptance", test))]
    let suppress = test_control::begin(test_control::Site::Read, deadline);
    #[cfg(all(feature = "media-acceptance", test))]
    let _fault_sender = suppress.then(|| tx.clone());
    let handler = GetNonDefaultPermissionSettingsCompletedHandler::create(Box::new(
        move |result, settings| {
            let result = result.and(settings.ok_or_else(|| windows::core::Error::from(E_POINTER)));
            #[cfg(all(feature = "media-acceptance", test))]
            {
                test_control::completed(test_control::Site::Read, result.is_ok());
                if suppress {
                    return Ok(());
                }
            }
            tx.send(result)
                .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
        },
    ));
    // SAFETY: the profile and one-shot completion handler live on this STA;
    // the bounded pump retains the receiver only through the caller's deadline.
    // A later callback owns its sender/result and cannot access a dead stack frame.
    unsafe { profile.GetNonDefaultPermissionSettings(&handler) }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let settings = wait_for_launch_with_message_pump_until(&rx, deadline)?
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let mut count = 0_u32;
    // SAFETY: the returned collection is live and `count` is writable.
    unsafe { settings.Count(&raw mut count) }
        .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    let mut permissions = Vec::new();
    for index in 0..count {
        check_launch_deadline(deadline)?;
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
    check_launch_deadline(deadline)?;
    Ok(permissions)
}

pub(super) fn set_profile_permission_state(
    profile: &ICoreWebView2Profile4,
    permission: &SavedPermission,
    state: COREWEBVIEW2_PERMISSION_STATE,
    deadline: Instant,
) -> Result<(), WvError> {
    check_launch_deadline(deadline)?;
    let origin = permission
        .origin
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let (tx, rx) = mpsc::channel();
    #[cfg(all(feature = "media-acceptance", test))]
    let suppress = test_control::begin(test_control::Site::Write, deadline);
    #[cfg(all(feature = "media-acceptance", test))]
    let _fault_sender = suppress.then(|| tx.clone());
    let handler = SetPermissionStateCompletedHandler::create(Box::new(move |result| {
        #[cfg(all(feature = "media-acceptance", test))]
        {
            test_control::completed(test_control::Site::Write, result.is_ok());
            if suppress {
                return Ok(());
            }
        }
        tx.send(result)
            .map_err(|_| windows::core::Error::from(E_UNEXPECTED))
    }));
    // SAFETY: the profile, NUL-terminated origin and one-shot handler live on
    // this STA through the pumped completion. The current v0 webview policy is
    // fail-closed, so persisted camera/microphone decisions are reconciled to
    // deny before any app navigation can consult them. A deadline bounds waiting,
    // not the asynchronous native write: timeout does not promise cancellation.
    check_launch_deadline(deadline)?;
    unsafe {
        profile.SetPermissionState(permission.kind, PCWSTR(origin.as_ptr()), state, &handler)
    }
    .map_err(|_| profile_failure(ProfileErrorKind::LifecycleUnproven))?;
    wait_for_launch_with_message_pump_until(&rx, deadline)?
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

pub(super) fn reconcile_saved_media_permissions(
    webview: &ICoreWebView2,
    deadline: Instant,
) -> Result<(), WvError> {
    check_launch_deadline(deadline)?;
    let profile = webview_profile(webview)?;
    for permission in profile_permission_settings(&profile, deadline)?
        .iter()
        .filter(|permission| saved_media_permission_needs_deny(permission))
    {
        set_profile_permission_state(
            &profile,
            permission,
            COREWEBVIEW2_PERMISSION_STATE_DENY,
            deadline,
        )?;
    }
    check_launch_deadline(deadline)
}

pub(super) fn saved_permission_reconciliation_enabled() -> bool {
    #[cfg(all(feature = "media-acceptance", test))]
    if std::env::var_os("KELD_PROFILE_TEST_SKIP_SAVED_RECONCILIATION").is_some() {
        return false;
    }
    true
}
