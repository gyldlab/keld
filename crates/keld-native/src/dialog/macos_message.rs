//! Main-thread-owned macOS Message sheet. This owns only the native NSAlert:
//! caller authentication, Guard, WindowRef and original/Cancel correlations
//! remain Core responsibilities. Parentless modal dispatch is not exposed here.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use block2::RcBlock;
use keld_ipc::IpcError;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSControlStateValueOff, NSControlStateValueOn,
    NSModalResponse, NSWindow,
};
use objc2_foundation::NSString;

use super::{DialogReplyV1, MessageKind, MessageOptions, validate_message_options};

/// The typed `AppKit` parent shared by the native and Host UI service. This is
/// only a Rust type alias; no caller mints or stores a raw native pointer.
pub type MacMessageParent = NSWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelState {
    Prepared,
    Presented,
    CancelRequested,
    Drained,
}

/// What the `AppKit` callback actually reported after the sheet was ordered out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacMessageOutcome {
    /// Genuine native response and checkbox state; not a default-value guess.
    Selected(DialogReplyV1),
    /// A Host-owned cancellation, with no fabricated button index.
    Dismissed,
}

fn invalid(detail: &'static str) -> IpcError {
    IpcError::Protocol { detail }
}

fn response_index(response: NSModalResponse, count: usize) -> Result<Option<u32>, IpcError> {
    if response < NSAlertFirstButtonReturn {
        // Only our explicit `endSheet:returnCode:` cancellation is -1. AppKit
        // also has Stop/Abort/Continue negative modal outcomes: none of those
        // is a user's click or proof that the caller requested cancellation.
        return if response == -1 {
            Ok(None)
        } else {
            Err(invalid("unrecognized native Message dismissal code"))
        };
    }
    let index = response - NSAlertFirstButtonReturn;
    if index >= isize::try_from(count).unwrap_or(isize::MAX) {
        return Err(invalid("native Message selected an undeclared button"));
    }
    u32::try_from(index)
        .map(Some)
        .map_err(|_| invalid("native Message button index overflow"))
}

fn normalize_access_keys(input: &str) -> String {
    let mut chars = input.chars().peekable();
    let mut output = String::with_capacity(input.len());
    while let Some(ch) = chars.next() {
        if ch == '&' {
            if chars.peek() == Some(&'&') {
                chars.next();
                output.push('&');
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn button_title(label: &str, normalize: bool) -> String {
    let text = if normalize {
        normalize_access_keys(label)
    } else {
        label.to_owned()
    };
    // AppKit rejects empty titles; match Electron after access-key removal.
    if text.is_empty() {
        "(empty)".to_owned()
    } else {
        text
    }
}

// Electron's omitted cancelId is selected after access-key normalization.
// A single native button keeps Return; it must not be rebound to Escape.
fn escape_button(options: &MessageOptions) -> Option<u32> {
    if options.buttons.len() <= 1 {
        return None;
    }
    if let Some(index) = options.cancel_id {
        return Some(index);
    }
    options
        .buttons
        .iter()
        .position(|label| {
            let text = button_title(label, options.normalize_access_keys);
            text.eq_ignore_ascii_case("cancel") || text.eq_ignore_ascii_case("no")
        })
        .and_then(|index| u32::try_from(index).ok())
        .or(Some(u32::from(options.default_id == Some(0))))
}

/// One retained native sheet and its one-shot completion block. This type is
/// `MainThreadOnly` through its owned `NSAlert`; it must never cross a worker.
/// The caller MUST hold it until [`Self::poll_outcome`] confirms invisibility
/// and the completion callback has run. No locks are held while `AppKit` runs.
pub struct MacMessagePanel {
    alert: Retained<NSAlert>,
    completion: Rc<Cell<Option<(NSModalResponse, Instant)>>>,
    callback: RcBlock<dyn Fn(NSModalResponse)>,
    buttons: usize,
    checkbox: bool,
    state: PanelState,
}

impl std::fmt::Debug for MacMessagePanel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MacMessagePanel")
            .field("state", &self.state)
            .field("buttons", &self.buttons)
            .field("checkbox", &self.checkbox)
            .finish_non_exhaustive()
    }
}

impl MacMessagePanel {
    /// Prepares one bounded native alert without showing it or borrowing an
    /// unvalidated parent. The Host must first authenticate and authorize it.
    ///
    /// # Errors
    ///
    /// Returns a typed IPC error for invalid options or an absent checkbox.
    pub fn prepare(main: MainThreadMarker, options: &MessageOptions) -> Result<Self, IpcError> {
        validate_message_options(options)?;
        let alert = NSAlert::new(main);
        alert.setMessageText(&NSString::from_str(&options.message));
        if let Some(detail) = &options.detail {
            alert.setInformativeText(&NSString::from_str(detail));
        }
        alert.setAlertStyle(match options.kind {
            MessageKind::Error => NSAlertStyle::Critical,
            MessageKind::Warning => NSAlertStyle::Warning,
            MessageKind::None | MessageKind::Info | MessageKind::Question => {
                NSAlertStyle::Informational
            }
        });
        let count = options.buttons.len().max(1);
        let escape = escape_button(options);
        for (index, label) in options.buttons.iter().enumerate() {
            let index = u32::try_from(index)
                .map_err(|_| invalid("native Message button count exceeds u32 domain"))?;
            let text = button_title(label, options.normalize_access_keys);
            let button = alert.addButtonWithTitle(&NSString::from_str(&text));
            if index == 0 && options.default_id.is_some_and(|id| id > 0) {
                button.setKeyEquivalent(&NSString::from_str(""));
            }
            if options.default_id == Some(index) {
                button.setKeyEquivalent(&NSString::from_str("\r"));
            }
            if escape == Some(index) {
                button.setKeyEquivalent(&NSString::from_str("\u{1b}"));
            }
        }
        if let Some(label) = &options.checkbox_label {
            alert.setShowsSuppressionButton(true);
            let checkbox = alert
                .suppressionButton()
                .ok_or_else(|| invalid("native Message checkbox unavailable"))?;
            checkbox.setTitle(&NSString::from_str(label));
            checkbox.setState(if options.checkbox_checked.unwrap_or(false) {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        let completion = Rc::new(Cell::new(None));
        let completion_for_block = Rc::clone(&completion);
        let callback: RcBlock<dyn Fn(NSModalResponse)> = RcBlock::new(move |response| {
            if completion_for_block.get().is_none() {
                completion_for_block.set(Some((response, Instant::now())));
            }
        });
        Ok(Self {
            alert,
            completion,
            callback,
            buttons: count,
            checkbox: options.checkbox_label.is_some(),
            state: PanelState::Prepared,
        })
    }

    /// Presents the genuine `AppKit` sheet against a previously Host-resolved
    /// live parent. The Core/WV caller is responsible for current identity.
    ///
    /// # Errors
    ///
    /// Refuses a duplicate or previously cancelled presentation.
    pub fn begin_sheet(&mut self, parent: &NSWindow) -> Result<(), IpcError> {
        if self.state != PanelState::Prepared {
            return Err(invalid("native Message may be presented only once"));
        }
        self.state = PanelState::Presented;
        self.alert
            .beginSheetModalForWindow_completionHandler(parent, Some(&self.callback));
        Ok(())
    }

    /// Requests native cancellation without inventing a clicked button. The
    /// caller must still poll to observe completion and physical sheet exit.
    ///
    /// # Errors
    ///
    /// Returns an error if the active panel has lost its native parent before
    /// its callback completed, rather than hiding the cleanup failure.
    pub fn request_cancel(&mut self) -> Result<bool, IpcError> {
        match self.state {
            PanelState::Prepared => {
                self.state = PanelState::Drained;
                Ok(true)
            }
            PanelState::CancelRequested | PanelState::Drained => Ok(false),
            PanelState::Presented => {
                if self.completion.get().is_some() {
                    return Ok(false);
                }
                let sheet = self.alert.window();
                let parent = sheet
                    .sheetParent()
                    .ok_or_else(|| invalid("presented native Message lost its sheet parent"))?;
                parent.endSheet_returnCode(&sheet, -1);
                self.state = PanelState::CancelRequested;
                Ok(true)
            }
        }
    }

    /// The first native callback's monotonic timestamp, independent of when
    /// the Host next polls or when the window becomes physically invisible.
    #[must_use]
    pub fn completed_at(&self) -> Option<Instant> {
        self.completion.get().map(|(_, completed_at)| completed_at)
    }

    /// Returns the single genuine outcome only after callback and real window
    /// invisibility. An outstanding cancellation is not proof of that drain.
    ///
    /// # Errors
    ///
    /// Refuses a native button index that was never declared by the request.
    pub fn poll_outcome(&mut self) -> Result<Option<MacMessageOutcome>, IpcError> {
        if self.state == PanelState::Prepared || self.state == PanelState::Drained {
            return Ok(None);
        }
        let Some((response, _completed_at)) = self.completion.get() else {
            return Ok(None);
        };
        if self.alert.window().isVisible() {
            return Ok(None);
        }
        let value = response_index(response, self.buttons)?.map_or(
            MacMessageOutcome::Dismissed,
            |response| {
                let checked = self.checkbox
                    && self
                        .alert
                        .suppressionButton()
                        .is_some_and(|button| button.state() == NSControlStateValueOn);
                MacMessageOutcome::Selected(DialogReplyV1::Message {
                    response,
                    checkbox_checked: checked,
                })
            },
        );
        self.state = PanelState::Drained;
        Ok(Some(value))
    }

    /// True only after no panel was presented or its native callback was
    /// observed after physical sheet release.
    #[must_use]
    pub fn quiescent(&self) -> bool {
        self.state == PanelState::Drained
    }
}

impl Drop for MacMessagePanel {
    fn drop(&mut self) {
        // A normal caller must retain this owner through `quiescent()`. The
        // fallback only requests native visibility teardown during an unwind
        // or an unexpected owner drop; it is NOT a callback/drain receipt.
        if self.state == PanelState::Presented && self.completion.get().is_none() {
            let sheet = self.alert.window();
            if let Some(parent) = sheet.sheetParent() {
                parent.endSheet_returnCode(&sheet, -1);
            } else if sheet.isVisible() {
                // Parent already disappeared. Hide this owned window but do
                // not fabricate a completion or treat the outcome as success.
                sheet.orderOut(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_result_codes_preserve_button_index_and_refuse_fabricated_click() {
        assert!(matches!(
            response_index(NSAlertFirstButtonReturn, 3),
            Ok(Some(0))
        ));
        assert!(matches!(
            response_index(NSAlertFirstButtonReturn + 2, 3),
            Ok(Some(2))
        ));
        assert!(matches!(response_index(-1, 3), Ok(None)));
        for native_failure in [-1000, -1001, -1002] {
            assert!(response_index(native_failure, 3).is_err());
        }
        assert!(response_index(NSAlertFirstButtonReturn + 3, 3).is_err());
        assert!(response_index(0, 3).is_err());
    }

    #[test]
    fn button_titles_are_nonempty_after_access_key_normalization() {
        for (label, normalize, expected) in [
            ("", false, "(empty)"),
            ("", true, "(empty)"),
            ("&", true, "(empty)"),
            ("&", false, "&"),
            ("&&", true, "&"),
            ("&&&", true, "&"),
            ("&Save", true, "Save"),
            ("&Save", false, "&Save"),
            ("Save && Close", true, "Save & Close"),
            ("保存", true, "保存"),
            (" ", true, " "),
        ] {
            assert_eq!(button_title(label, normalize), expected);
        }
    }

    #[test]
    fn escape_selection_matches_electron_and_preserves_singleton_return() {
        for (labels, default_id, cancel_id, normalize, expected) in [
            (vec![], None, None, false, None),
            (vec!["OK"], Some(0), Some(0), false, None),
            (vec!["Cancel"], None, None, false, None),
            (vec!["Yes", "No"], None, None, false, Some(1)),
            (vec!["No", "Cancel"], None, None, false, Some(0)),
            (vec!["YES", "cAnCeL"], Some(0), None, false, Some(1)),
            (vec!["A", "B"], None, None, false, Some(0)),
            (vec!["A", "B"], Some(0), None, false, Some(1)),
            (vec!["A", "B"], Some(1), None, false, Some(0)),
            (vec!["Yes", "No"], Some(0), Some(0), false, Some(0)),
            (vec!["A", "&No"], None, None, true, Some(1)),
            (vec!["A", "&No"], None, None, false, Some(0)),
            (vec!["A", "&&No"], None, None, true, Some(0)),
        ] {
            let options = MessageOptions {
                message: String::new(),
                kind: MessageKind::None,
                title: None,
                detail: None,
                buttons: labels.into_iter().map(str::to_owned).collect(),
                default_id,
                cancel_id,
                normalize_access_keys: normalize,
                no_link: false,
                checkbox_label: None,
                checkbox_checked: None,
            };
            assert_eq!(escape_button(&options), expected, "{options:?}");
        }
    }

    #[test]
    fn access_key_normalization_does_not_swallow_literal_ampersands() {
        assert_eq!(normalize_access_keys("&Save"), "Save");
        assert_eq!(normalize_access_keys("Save && Close"), "Save & Close");
        assert_eq!(normalize_access_keys("Discard Changes"), "Discard Changes");
    }
}
