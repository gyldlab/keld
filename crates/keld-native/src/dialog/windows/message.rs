//! `TaskDialog` button identifiers retain their exact caller-array positions.
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use keld_ipc::IpcError;
use windows::Win32::Foundation::{HWND, LPARAM, S_FALSE, S_OK, WPARAM};
use windows::Win32::UI::Controls::{
    TASKDIALOG_BUTTON, TASKDIALOG_COMMON_BUTTON_FLAGS, TASKDIALOG_NOTIFICATIONS, TASKDIALOGCONFIG,
    TASKDIALOGCONFIG_0, TD_ERROR_ICON, TD_INFORMATION_ICON, TD_WARNING_ICON, TDCBF_CANCEL_BUTTON,
    TDCBF_CLOSE_BUTTON, TDCBF_NO_BUTTON, TDCBF_OK_BUTTON, TDCBF_RETRY_BUTTON, TDCBF_YES_BUTTON,
    TDF_ALLOW_DIALOG_CANCELLATION, TDF_CALLBACK_TIMER, TDF_POSITION_RELATIVE_TO_WINDOW,
    TDF_SIZE_TO_CONTENT, TDF_USE_COMMAND_LINKS, TDF_VERIFICATION_FLAG_CHECKED, TDM_CLICK_BUTTON,
    TDN_BUTTON_CLICKED, TDN_CREATED, TDN_DESTROYED, TDN_TIMER, TaskDialogIndirect,
};
use windows::Win32::UI::WindowsAndMessaging::{
    IDCANCEL, IDCLOSE, IDNO, IDOK, IDRETRY, IDYES, SendMessageW,
};
use windows::core::{BOOL, HRESULT, PCWSTR};

use super::super::{DialogReplyV1, MessageKind, MessageOptions, validate_message_options};
use super::{Cancellation, invalid, native, wide};

const CUSTOM_START: i32 = 100;

struct State {
    token: isize,
    cancel: Rc<Cancellation>,
    buttons: BTreeMap<i32, u32>,
    created: Cell<u32>,
    destroyed: Cell<u32>,
    clicked: Cell<Option<i32>>,
}

thread_local! {
    static NEXT_TOKEN: Cell<isize> = const { Cell::new(1) };
    static ACTIVE: RefCell<Option<Rc<State>>> = const { RefCell::new(None) };
}

struct Registration(Rc<State>);
impl Drop for Registration {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|active| {
            if active
                .as_ref()
                .is_some_and(|state| Rc::ptr_eq(state, &self.0))
            {
                active.take();
            }
        });
    }
}

fn common(label: &str) -> Option<(i32, TASKDIALOG_COMMON_BUTTON_FLAGS)> {
    match label.to_ascii_lowercase().as_str() {
        "ok" => Some((IDOK.0, TDCBF_OK_BUTTON)),
        "yes" => Some((IDYES.0, TDCBF_YES_BUTTON)),
        "no" => Some((IDNO.0, TDCBF_NO_BUTTON)),
        "cancel" => Some((IDCANCEL.0, TDCBF_CANCEL_BUTTON)),
        "retry" => Some((IDRETRY.0, TDCBF_RETRY_BUTTON)),
        "close" => Some((IDCLOSE.0, TDCBF_CLOSE_BUTTON)),
        _ => None,
    }
}

// Native common IDs can represent only one button. Duplicate labels retain
// their separate caller-array positions by using separate custom IDs instead.
fn unique_common(
    options: &MessageOptions,
    label: &str,
) -> Option<(i32, TASKDIALOG_COMMON_BUTTON_FLAGS)> {
    let found = common(label)?;
    (options
        .buttons
        .iter()
        .filter(|label| common(label).is_some_and(|value| value.0 == found.0))
        .count()
        == 1)
        .then_some(found)
}

fn cancel_index(options: &MessageOptions) -> u32 {
    options.cancel_id.unwrap_or_else(|| {
        options
            .buttons
            .iter()
            .position(|label| matches!(label.to_ascii_lowercase().as_str(), "cancel" | "no"))
            .and_then(|index| u32::try_from(index).ok())
            .unwrap_or_else(|| {
                u32::from(options.default_id == Some(0) && options.buttons.len() > 1)
            })
    })
}

#[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
#[allow(clippy::too_many_lines)] // Keep all FFI backing storage visibly live through Show.
pub(super) fn show(
    parent: HWND,
    options: &MessageOptions,
    cancel: &Rc<Cancellation>,
) -> Result<Option<DialogReplyV1>, IpcError> {
    validate_message_options(options)?;
    let labels = options
        .buttons
        .iter()
        .map(|label| wide(label))
        .collect::<Result<Vec<_>, _>>()?;
    let message = wide(&options.message)?;
    let title = wide(
        options
            .title
            .as_deref()
            .filter(|text| !text.is_empty())
            .unwrap_or("Keld"),
    )?;
    let detail = wide(options.detail.as_deref().unwrap_or(""))?;
    let checkbox = wide(options.checkbox_label.as_deref().unwrap_or(""))?;
    // The SDK configuration is packed. Accumulate flags in aligned locals so
    // compound assignments never form unaligned references to its fields.
    let mut flags = TDF_SIZE_TO_CONTENT
        | TDF_ALLOW_DIALOG_CANCELLATION
        | TDF_POSITION_RELATIVE_TO_WINDOW
        | TDF_CALLBACK_TIMER;
    let mut common_buttons = TASKDIALOG_COMMON_BUTTON_FLAGS::default();
    let mut config = TASKDIALOGCONFIG {
        cbSize: u32::try_from(std::mem::size_of::<TASKDIALOGCONFIG>())
            .map_err(|_| invalid("Windows TaskDialog configuration size overflow"))?,
        hwndParent: parent,
        pszWindowTitle: PCWSTR(title.as_ptr()),
        ..Default::default()
    };
    config.Anonymous1 = TASKDIALOGCONFIG_0 {
        pszMainIcon: match options.kind {
            MessageKind::None => PCWSTR::null(),
            MessageKind::Info | MessageKind::Question => TD_INFORMATION_ICON,
            MessageKind::Warning => TD_WARNING_ICON,
            MessageKind::Error => TD_ERROR_ICON,
        },
    };
    if options.detail.as_deref().is_none_or(str::is_empty) {
        config.pszContent = PCWSTR(message.as_ptr());
    } else {
        config.pszMainInstruction = PCWSTR(message.as_ptr());
        config.pszContent = PCWSTR(detail.as_ptr());
    }
    let has_checkbox = options
        .checkbox_label
        .as_deref()
        .is_some_and(|text| !text.is_empty());
    if has_checkbox {
        config.pszVerificationText = PCWSTR(checkbox.as_ptr());
        if options.checkbox_checked == Some(true) {
            flags |= TDF_VERIFICATION_FLAG_CHECKED;
        }
    }
    let mut buttons = Vec::new();
    let mut mapping = BTreeMap::new();
    for (index, (label, text)) in options.buttons.iter().zip(&labels).enumerate() {
        let index = u32::try_from(index).map_err(|_| invalid("Windows button index overflow"))?;
        let custom = CUSTOM_START
            + i32::try_from(index).map_err(|_| invalid("Windows button index overflow"))?;
        let id = if let Some((id, flag)) = (!options.no_link)
            .then(|| unique_common(options, label))
            .flatten()
        {
            common_buttons |= flag;
            id
        } else {
            buttons.push(TASKDIALOG_BUTTON {
                nButtonID: custom,
                pszButtonText: PCWSTR(text.as_ptr()),
            });
            custom
        };
        mapping.insert(id, index);
        if options.default_id == Some(index) {
            config.nDefaultButton = id;
        }
    }
    if mapping.is_empty() {
        mapping.insert(IDOK.0, 0);
    }
    if !buttons.is_empty() {
        config.pButtons = buttons.as_ptr();
        config.cButtons =
            u32::try_from(buttons.len()).map_err(|_| invalid("Windows button count overflow"))?;
        if !options.no_link {
            flags |= TDF_USE_COMMAND_LINKS;
        }
    }
    config.dwFlags = flags;
    config.dwCommonButtons = common_buttons;
    if ACTIVE.with_borrow(Option::is_some) {
        return Err(invalid("Windows Message callback already retained"));
    }
    let token = NEXT_TOKEN
        .with(|next| {
            let token = next.get();
            if let Some(value) = token.checked_add(1) {
                next.set(value);
                Some(token)
            } else {
                None
            }
        })
        .ok_or_else(|| invalid("Windows Message callback token exhausted"))?;
    let state = Rc::new(State {
        token,
        cancel: Rc::clone(cancel),
        buttons: mapping,
        created: Cell::new(0),
        destroyed: Cell::new(0),
        clicked: Cell::new(None),
    });
    ACTIVE.set(Some(Rc::clone(&state)));
    let registration = Registration(Rc::clone(&state));
    config.pfCallback = Some(callback);
    config.lpCallbackData = token;
    if cancel.check() {
        return cancel.finish(None);
    }
    let mut selected = 0;
    let mut checked = BOOL::default();
    // SAFETY: config, strings, button descriptors and output variables remain
    // live for the synchronous call. Callback context is an integer lookup, not
    // a borrowed pointer. Core retains the validated parent on this same STA.
    let shown = unsafe {
        TaskDialogIndirect(
            &raw const config,
            Some(&raw mut selected),
            None,
            Some(&raw mut checked),
        )
    };
    drop(registration); // No callback can access released Rust storage.
    native(shown, "Windows Message presentation failed")?;
    if state.created.get() != 1 || state.destroyed.get() != 1 {
        return Err(invalid(
            "Windows Message callback did not physically drain exactly once",
        ));
    }
    if cancel.check() {
        return cancel.finish(None);
    }
    if state.clicked.get() != Some(selected) {
        return Err(invalid(
            "Windows Message result has no matching native button callback",
        ));
    }
    let response = if selected == IDCANCEL.0 {
        state
            .buttons
            .get(&selected)
            .copied()
            .unwrap_or_else(|| cancel_index(options))
    } else {
        state
            .buttons
            .get(&selected)
            .copied()
            .ok_or_else(|| invalid("Windows Message returned an undeclared button"))?
    };
    cancel.finish(Some(DialogReplyV1::Message {
        response,
        checkbox_checked: has_checkbox && checked.as_bool(),
    }))
}

#[allow(unsafe_code)] // Exact Windows dialog ABI in this directory's AGENTS.md.
unsafe extern "system" fn callback(
    window: HWND,
    notification: TASKDIALOG_NOTIFICATIONS,
    wparam: WPARAM,
    _: LPARAM,
    token: isize,
) -> HRESULT {
    // Release the lookup borrow before sending a message: it may synchronously
    // invoke this callback again. Tokens are never reused during thread lifetime.
    let state = ACTIVE
        .try_with(|slot| {
            slot.try_borrow()
                .ok()
                .and_then(|slot| slot.as_ref().filter(|state| state.token == token).cloned())
        })
        .ok()
        .flatten();
    let Some(state) = state else { return S_FALSE };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        match notification {
            TDN_CREATED => state.created.set(state.created.get().saturating_add(1)),
            TDN_DESTROYED => state.destroyed.set(state.destroyed.get().saturating_add(1)),
            TDN_BUTTON_CLICKED => {
                let Ok(id) = i32::try_from(wparam.0) else {
                    return S_FALSE;
                };
                if id != IDCANCEL.0 && !state.buttons.contains_key(&id) {
                    return S_FALSE; // Ignore IDs this operation never created.
                }
                if state.clicked.replace(Some(id)).is_some() {
                    state.cancel.failed.set(true);
                }
            }
            TDN_TIMER if state.cancel.check() => {
                // SAFETY: TDN_TIMER supplies the currently live TaskDialog
                // HWND; TDM_CLICK_BUTTON(IDCANCEL) closes via its real native
                // callback under ALLOW_DIALOG_CANCELLATION. No synthetic result.
                unsafe {
                    SendMessageW(
                        window,
                        TDM_CLICK_BUTTON.0 as u32,
                        Some(WPARAM(IDCANCEL.0 as usize)),
                        None,
                    );
                }
            }
            _ => {}
        }
        S_OK
    }));
    result.unwrap_or_else(|_| {
        state.cancel.failed.set(true);
        state.cancel.requested.set(true);
        S_OK
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_order_and_default_cancel_are_not_native_identifiers() {
        let mut options = MessageOptions {
            message: "dirty".into(),
            kind: MessageKind::Warning,
            buttons: vec!["&Save".into(), "&Discard Changes".into(), "Cancel".into()],
            default_id: Some(0),
            cancel_id: None,
            normalize_access_keys: true,
            title: None,
            detail: None,
            no_link: true,
            checkbox_label: None,
            checkbox_checked: None,
        };
        assert_eq!(cancel_index(&options), 2);
        options.buttons = vec!["Accept".into(), "Dismiss".into()];
        assert_eq!(cancel_index(&options), 1);
        options.cancel_id = Some(0);
        assert_eq!(cancel_index(&options), 0);
        assert_eq!(common("&Save"), None);
        assert_eq!(common("NO"), Some((IDNO.0, TDCBF_NO_BUTTON)));
        options.no_link = false;
        options.buttons = vec!["Yes".into(), "yes".into(), "Cancel".into()];
        assert_eq!(unique_common(&options, "Yes"), None);
        assert_eq!(unique_common(&options, "yes"), None);
        assert_eq!(
            unique_common(&options, "Cancel"),
            Some((IDCANCEL.0, TDCBF_CANCEL_BUTTON))
        );
    }
}
