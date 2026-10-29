//! Bounded dialog wire values. Native presentation and authorization are separate.
//!
//! The first implemented variants are Message and Cancel. Later operations must
//! append variants; neither nonce nor window reference conveys authority.

/// Native-owned file type grouping; this is not a picker or a new authority.
#[cfg(target_os = "macos")]
pub mod file_filter;
/// Actual macOS `NSAlert` construction and sheet custody on the AppKit thread.
/// The Host owns Guard, parent identity, correlation, and callback retirement.
#[cfg(target_os = "macos")]
pub mod macos_message;

use std::collections::HashSet;
use std::fmt;
use std::marker::PhantomData;

use keld_ipc::{IpcError, codec, window::WindowRef};
use serde::{Deserialize, Deserializer, Serialize, de};

/// Maximum postcard-encoded message options size.
pub const MAX_DIALOG_OPTIONS_BYTES: usize = 16 * 1024;
/// Maximum number of buttons in a message.
pub const MAX_DIALOG_BUTTONS: usize = 16;
/// Maximum selected file paths in a real Open result.
pub const MAX_DIALOG_PATHS: usize = 32;
/// Maximum filter groups, and extension entries inside each group.
pub const MAX_DIALOG_FILTERS: usize = 32;
/// Max encoded return payload, independent of KIPC's own frame ceiling.
pub const MAX_DIALOG_RESPONSE_BYTES: usize = 256 * 1024;
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

fn bounded_vec<'de, D, T, const N: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T, const N: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> de::Visitor<'de> for Bounded<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {N} file dialog entries")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|size| size > N) {
                return Err(de::Error::custom("too many native file dialog entries"));
            }
            let mut entries = Vec::new();
            while let Some(value) = sequence.next_element()? {
                if entries.len() == N {
                    return Err(de::Error::custom("too many native file dialog entries"));
                }
                entries.push(value);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Bounded::<T, N>(PhantomData))
}

fn deserialize_file_filters<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<FileDialogFilter>, D::Error> {
    bounded_vec::<_, FileDialogFilter, MAX_DIALOG_FILTERS>(deserializer)
}

fn deserialize_file_extensions<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    bounded_vec::<_, String, MAX_DIALOG_FILTERS>(deserializer)
}

fn deserialize_open_properties<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<OpenProperty>, D::Error> {
    bounded_vec::<_, OpenProperty, 3>(deserializer)
}

/// A named native file-type group. Extension order is preserved, not unioned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDialogFilter {
    /// Actual group title; nonempty, at most 256 UTF-8 bytes.
    pub name: String,
    /// Distinct simple extensions or the exact `*` unrestricted marker.
    #[serde(deserialize_with = "deserialize_file_extensions")]
    pub extensions: Vec<String>,
}

/// Only the three macOS first-proof Open flags; combined files/directories
/// preserve *both* native choices instead of preferring directory mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpenProperty {
    /// Allow selecting ordinary files.
    OpenFile,
    /// Allow selecting directories.
    OpenDirectory,
    /// Allow a bounded sequence of multiple native selections.
    MultiSelections,
}

/// Exact first-proof `showOpenDialog` options from the pinned draw.io caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenDialogOptions {
    /// Absolute directory/file or filename hint, with no NUL.
    pub default_path: Option<String>,
    /// Ordered native filter groups, never a union of unrelated extensions.
    #[serde(deserialize_with = "deserialize_file_filters")]
    pub filters: Vec<FileDialogFilter>,
    /// Only `OpenFile`, `OpenDirectory` and `MultiSelections`; duplicates are denied.
    #[serde(deserialize_with = "deserialize_open_properties")]
    pub properties: Vec<OpenProperty>,
}

/// Exact first-proof `showSaveDialog` options. Save only reports a path;
/// it never creates a file or a filesystem capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveDialogOptions {
    /// Directory/file or filename hint, no NUL and at most 4096 bytes.
    pub default_path: Option<String>,
    /// Ordered filter groups; selected group controls native type restriction.
    #[serde(deserialize_with = "deserialize_file_filters")]
    pub filters: Vec<FileDialogFilter>,
    /// Must be false; scope bookmarks are not part of GH478's path-only grant.
    pub security_scoped_bookmarks: bool,
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
    /// `AppKit` file Open; append-only variant 2, no implicit FS capability.
    Open {
        /// Existing current-primary generation's original request nonce.
        nonce: u64,
        /// An optional Host-minted parent window.
        parent: Option<WindowRef>,
        /// Validated exact Open options.
        options: OpenDialogOptions,
    },
    /// `AppKit` file Save; append-only variant 3, no creation or FS grant.
    Save {
        /// Existing current-primary generation's original request nonce.
        nonce: u64,
        /// An optional Host-minted parent window.
        parent: Option<WindowRef>,
        /// Validated exact Save options.
        options: SaveDialogOptions,
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
    /// Actual native Open selection or Cancel; no file handles or scopes.
    Open {
        /// Native Cancel is a value with an empty path list.
        canceled: bool,
        /// At most 32 actual OS-selected absolute POSIX paths, in order.
        file_paths: Vec<String>,
    },
    /// Actual native Save location or Cancel, with no file creation.
    Save {
        /// Native Cancel is a value, not a synthetic last path.
        canceled: bool,
        /// Actual absolute POSIX save location; empty only on Cancel.
        file_path: String,
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
        DialogRequestV1::Open { nonce, options, .. } => {
            if *nonce == 0 {
                return Err(invalid("dialog nonce must be nonzero"));
            }
            validate_open_options(options)?;
        }
        DialogRequestV1::Save { nonce, options, .. } => {
            if *nonce == 0 {
                return Err(invalid("dialog nonce must be nonzero"));
            }
            validate_save_options(options)?;
        }
    }
    Ok(request)
}

fn validate_filter_groups(filters: &[FileDialogFilter]) -> Result<(), IpcError> {
    if filters.len() > MAX_DIALOG_FILTERS {
        return Err(invalid("too many native file dialog filter groups"));
    }
    for filter in filters {
        if filter.name.is_empty() || filter.name.len() > 256 || filter.name.contains('\0') {
            return Err(invalid("invalid native file dialog filter name"));
        }
        if filter.extensions.is_empty() || filter.extensions.len() > MAX_DIALOG_FILTERS {
            return Err(invalid("invalid native file dialog extension count"));
        }
        let mut seen = HashSet::new();
        for extension in &filter.extensions {
            if extension.is_empty()
                || extension.len() > 128
                || extension.starts_with('.')
                || extension.ends_with('.')
                || (extension != "*"
                    && extension
                        .chars()
                        .any(|ch| matches!(ch, '/' | '\\' | '\0' | '*') || ch.is_control()))
                || !seen.insert(extension.as_str())
            {
                return Err(invalid("invalid or duplicate native file dialog extension"));
            }
        }
        if seen.contains("*") && seen.len() != 1 {
            return Err(invalid(
                "all-file filter cannot mix wildcard and type restrictions",
            ));
        }
    }
    Ok(())
}

fn validate_path_hint(path: Option<&String>) -> Result<(), IpcError> {
    if path.is_some_and(|path| path.len() > 4096 || path.contains('\0')) {
        return Err(invalid("native file dialog default path exceeds bounds"));
    }
    Ok(())
}

/// Checks all exact Open options before any OS presentation.
///
/// # Errors
///
/// Returns an IPC error for an unsafe extension, duplicate flag or excess size.
pub fn validate_open_options(options: &OpenDialogOptions) -> Result<(), IpcError> {
    validate_path_hint(options.default_path.as_ref())?;
    validate_filter_groups(&options.filters)?;
    let mut seen = HashSet::new();
    if options.properties.len() > 3 || options.properties.iter().any(|value| !seen.insert(value)) {
        return Err(invalid(
            "invalid or duplicate Open dialog selection property",
        ));
    }
    if codec::encode(options)?.len() > MAX_DIALOG_OPTIONS_BYTES {
        return Err(invalid("Open dialog options exceed bounded envelope"));
    }
    Ok(())
}

/// Checks all exact Save options without creating a file or scope.
///
/// # Errors
///
/// Refuses an unsafe filter, bookmark grant or oversized native options.
pub fn validate_save_options(options: &SaveDialogOptions) -> Result<(), IpcError> {
    validate_path_hint(options.default_path.as_ref())?;
    validate_filter_groups(&options.filters)?;
    if options.security_scoped_bookmarks {
        return Err(invalid("security scoped bookmarks are not supported"));
    }
    if codec::encode(options)?.len() > MAX_DIALOG_OPTIONS_BYTES {
        return Err(invalid("Save dialog options exceed bounded envelope"));
    }
    Ok(())
}

fn selected_path(path: &str) -> bool {
    path.len() <= 4096
        && path.starts_with('/')
        && !path.contains(['\0', '\\'])
        && !path.split('/').any(|part| part == "..")
}

/// Validate the native result *before* serializing the original correlation.
/// Returns path-only data, never a file, retained scope or new FS grant.
///
/// # Errors
///
/// Rejects nonabsolute, malformed, excessive or Cancel-bearing path results.
pub fn validate_dialog_reply(reply: &DialogReplyV1) -> Result<(), IpcError> {
    match reply {
        DialogReplyV1::Open {
            canceled,
            file_paths,
        } => {
            if file_paths.len() > MAX_DIALOG_PATHS
                || (*canceled && !file_paths.is_empty())
                || (!*canceled && file_paths.is_empty())
                || file_paths.iter().any(|path| !selected_path(path))
            {
                return Err(invalid("invalid native Open selection paths"));
            }
        }
        DialogReplyV1::Save {
            canceled,
            file_path,
        } => {
            if (*canceled && !file_path.is_empty()) || (!*canceled && !selected_path(file_path)) {
                return Err(invalid("invalid native Save selection path"));
            }
        }
        DialogReplyV1::Message { .. } | DialogReplyV1::Cancel { .. } => {}
    }
    if codec::encode(reply)?.len() > MAX_DIALOG_RESPONSE_BYTES {
        return Err(invalid("native dialog reply exceeds bounded envelope"));
    }
    Ok(())
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
