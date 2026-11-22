//! Bounded dialog wire values. Native presentation and authorization are separate.
//!
//! The first implemented variants are Message and Cancel. Later operations must
//! append variants; neither nonce nor window reference conveys authority.

/// Actual macOS `NSAlert` construction and sheet custody on the AppKit thread.
/// The Host owns Guard, parent identity, correlation, and callback retirement.
#[cfg(target_os = "macos")]
pub mod macos_message;

use std::fmt;

use keld_ipc::{IpcError, codec, window::WindowRef};
use serde::{Deserialize, Deserializer, Serialize, de};

/// Maximum postcard-encoded message options size.
pub const MAX_DIALOG_OPTIONS_BYTES: usize = 16 * 1024;
/// Maximum number of buttons in a message.
pub const MAX_DIALOG_BUTTONS: usize = 16;
/// Message options plus the maximum postcard discriminant, nonce and parent bytes.
const MAX_REQUEST_BYTES: usize = MAX_DIALOG_OPTIONS_BYTES + 32;

/// Native message style; declaration order is the postcard discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageKind {
    /// No icon requested.
    None,
    /// Informational message.
    Info,
    /// Error message.
    Error,
    /// Question message.
    Question,
    /// Warning message.
    Warning,
}

/// Validated, bounded native message options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageOptions {
    /// Main message text.
    pub message: String,
    /// Requested native style.
    pub kind: MessageKind,
    /// Button labels, or an empty list for the native OK button.
    #[serde(deserialize_with = "deserialize_buttons")]
    pub buttons: Vec<String>,
    /// Optional default button index.
    pub default_id: Option<u32>,
    /// Optional escape button index.
    pub cancel_id: Option<u32>,
    /// Whether to normalize access keys in labels.
    pub normalize_access_keys: bool,
    /// Optional title, validated but inert on macOS.
    pub title: Option<String>,
    /// Optional informative text.
    pub detail: Option<String>,
    /// Validated Windows-specific option, inert on macOS.
    pub no_link: bool,
    /// Optional nonempty checkbox label.
    pub checkbox_label: Option<String>,
    /// Initial checkbox state.
    pub checkbox_checked: Option<bool>,
}

fn deserialize_buttons<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    struct Buttons;
    impl<'de> de::Visitor<'de> for Buttons {
        type Value = Vec<String>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {MAX_DIALOG_BUTTONS} button labels")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|size| size > MAX_DIALOG_BUTTONS)
            {
                return Err(de::Error::custom("too many dialog buttons"));
            }
            let mut buttons = Vec::new();
            while let Some(button) = seq.next_element()? {
                if buttons.len() == MAX_DIALOG_BUTTONS {
                    return Err(de::Error::custom("too many dialog buttons"));
                }
                buttons.push(button);
            }
            Ok(buttons)
        }
    }
    deserializer.deserialize_seq(Buttons)
}

/// Dialog calls, in append-only postcard variant order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogRequestV1 {
    /// Show one native message.
    Message {
        /// Nonzero, generation-bound cancellation nonce.
        nonce: u64,
        /// Optional host-minted parent reference.
        parent: Option<WindowRef>,
        /// Validated message options.
        options: MessageOptions,
    },
    /// Cancel the matching admitted request, on a fresh correlation.
    Cancel {
        /// Original request's nonzero nonce.
        nonce: u64,
    },
}

/// Outcome of a matching cancellation request, not proof of native cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelDisposition {
    /// Cancellation was requested for active custody.
    Requested,
    /// The matching operation had already completed.
    AlreadyCompleted,
}

/// Dialog success values, in append-only postcard variant order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogReplyV1 {
    /// Actual native button index and checkbox state.
    Message {
        /// Zero-based clicked button index.
        response: u32,
        /// Actual native checkbox state.
        checkbox_checked: bool,
    },
    /// Disposition of the distinct cancellation call.
    Cancel {
        /// Matching custody disposition.
        disposition: CancelDisposition,
    },
}

fn invalid(detail: &'static str) -> IpcError {
    IpcError::Protocol { detail }
}

/// Decodes and validates bounded child-supplied dialog requests before UI work.
///
/// # Errors
///
/// Returns a typed IPC error for oversized, malformed, trailing or invalid input.
pub fn decode_dialog_request(payload: &[u8]) -> Result<DialogRequestV1, IpcError> {
    if payload.len() > MAX_REQUEST_BYTES {
        return Err(invalid("dialog request exceeds the bounded envelope"));
    }
    let request: DialogRequestV1 = codec::decode(payload)?;
    match &request {
        DialogRequestV1::Cancel { nonce } if *nonce == 0 => {
            return Err(invalid("dialog nonce must be nonzero"));
        }
        DialogRequestV1::Message { nonce, options, .. } => {
            if *nonce == 0 {
                return Err(invalid("dialog nonce must be nonzero"));
            }
            validate_message_options(options)?;
        }
        DialogRequestV1::Cancel { .. } => {}
    }
    Ok(request)
}

fn validate_message_options(options: &MessageOptions) -> Result<(), IpcError> {
    if codec::encode(options)?.len() > MAX_DIALOG_OPTIONS_BYTES {
        return Err(invalid("dialog options exceed the bounded envelope"));
    }
    if options.checkbox_checked.is_some()
        && options.checkbox_label.as_deref().is_none_or(str::is_empty)
    {
        return Err(invalid("invalid dialog checkbox"));
    }
    let count = options.buttons.len().max(1);
    if options.default_id.is_some_and(|id| id as usize >= count)
        || options.cancel_id.is_some_and(|id| id as usize >= count)
    {
        return Err(invalid("dialog button index is out of range"));
    }
    Ok(())
}
