//! Bounded application-menu projection. Authorization, current role custody,
//! native handles and actual activation remain the Host/UI owner's facts.

/// Retained AppKit menu installation and generation-stamped main-thread activation.
#[cfg(target_os = "macos")]
pub mod macos;

use keld_ipc::{CallError, IpcError, codec};
use serde::{Deserialize, Deserializer, Serialize};

/// Maximum number of nodes in one immutable menu projection.
pub const MAX_MENU_NODES: usize = 256;
/// Maximum submenu edges below a root item.
pub const MAX_MENU_DEPTH: u8 = 8;
/// Maximum bytes in one menu label.
pub const MAX_MENU_LABEL_BYTES: usize = 4096;
/// Maximum actual Postcard bytes in one installation request.
pub const MAX_MENU_REQUEST_BYTES: usize = 128 * 1024;

macro_rules! menu_faults {
    ($( $variant:ident, $code:literal, $message:literal, $fix:literal; )+) => {
        /// Registry-owned, resource-free menu error renderer shared with Bun codegen.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum MenuFault {
            $(#[doc = $message]
            $variant,)+
        }
        impl MenuFault {
            /// Registered stable error code.
            #[must_use]
            pub const fn code(self) -> &'static str { match self { $(Self::$variant => $code,)+ } }
            /// Resource-free message, without app labels, paths or selectors.
            #[must_use]
            pub const fn message(self) -> &'static str { match self { $(Self::$variant => $message,)+ } }
            /// Single canonical corrective action.
            #[must_use]
            pub const fn fix(self) -> &'static str { match self { $(Self::$variant => $fix,)+ } }
            /// Canonical privileged ERR shape; logging and transport use the same owner.
            #[must_use]
            pub fn call_error(self) -> CallError {
                CallError { code:self.code().to_owned(), message:format!("{}: {}. Fix: {}",self.code(),self.message(),self.fix()) }
            }
        }
    };
}

menu_faults! {
    InvalidProjection, "KELD-COMPAT-052", "The menu projection is outside the approved bounded grammar or local ownership contract", "Use this facade's Menu/MenuItem objects and the approved fields, types, roles and limits before requesting installation";
    UnsupportedClick, "KELD-COMPAT-053", "Native custom-click or checkbox activation is unsupported until GH556", "Keep the initial checkbox state and app data unchanged; use a supported role or wait for GH556's approved callback protocol";
    UnsupportedQuit, "KELD-COMPAT-054", "Native application-menu Quit/Cmd+Q is held until GH563's cancelable OS-quit integration", "Close windows through their vetoable close policy; do not bypass the hold with native terminate or infer before-quit parity";
    NativeFailure, "KELD-NATIVE-017", "Native application-menu materialization failed without a confirmed installation receipt", "Inspect Host/AppKit diagnostics and correct the cause before a fresh deliberate install; never replay an unknown effect automatically";
}

macro_rules! menu_roles {
    ($( $variant:ident, $name:literal, $label:literal, $key:literal, $modifiers:literal; )+) => {
        /// Closed native role vocabulary; declaration order owns Postcard IDs.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum MenuRole {
            $(#[doc = concat!("The ", $name, " native menu role.")]
            $variant,)+
        }
        impl MenuRole {
            /// Lowercase compatibility name from the single source role table.
            #[must_use]
            pub const fn name(self) -> &'static str { match self { $(Self::$variant => $name,)+ } }
            /// Default native label; `{app}` is replaced by Host-owned app metadata.
            #[must_use]
            pub const fn label(self) -> &'static str { match self { $(Self::$variant => $label,)+ } }
            /// Native key equivalent; empty means there is no default shortcut.
            #[must_use]
            pub const fn key(self) -> &'static str { match self { $(Self::$variant => $key,)+ } }
            /// Portable modifier mask: Command=1, Shift=2, Option=4.
            #[must_use]
            pub const fn modifiers(self) -> u8 { match self { $(Self::$variant => $modifiers,)+ } }
        }
    };
}

// Single source of truth for Rust action enum and generated Bun role defaults.
// Append-only IDs. Native selectors are a fixed typed match in the UI owner,
// never caller-supplied selector text or a second role/default table.
menu_roles! {
    Hide, "hide", "Hide {app}", "h", 1;
    HideOthers, "hideothers", "Hide Others", "h", 5;
    Unhide, "unhide", "Show All", "", 0;
    Quit, "quit", "Quit {app}", "q", 1;
    Close, "close", "Close Window", "w", 1;
    Undo, "undo", "Undo", "z", 1;
    Redo, "redo", "Redo", "z", 3;
    Cut, "cut", "Cut", "x", 1;
    Copy, "copy", "Copy", "c", 1;
    Paste, "paste", "Paste", "v", 1;
    PasteAndMatchStyle, "pasteandmatchstyle", "Paste and Match Style", "v", 7;
    SelectAll, "selectall", "Select All", "a", 1;
}

/// Native node type; declaration order owns Postcard IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MenuKind {
    /// Ordinary role, unsupported custom click, or inert item.
    Normal,
    /// Structural separator without a target.
    Separator,
    /// Structural parent of following deeper nodes.
    Submenu,
    /// Frozen initial checkmark; custom activation is diagnosed until GH556.
    Checkbox,
}

/// Fixed action type; no callable code, selector or command identity crosses IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MenuAction {
    /// Execute the approved native role through the current-generation UI router.
    NativeRole(MenuRole),
    /// Record the registered GH556 diagnostic without invoking or toggling Bun state.
    UnsupportedClick,
    /// Structural or explicitly inert item.
    Inert,
}

/// Flat preorder node. Depth is structure only, never a native identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuNodeV1 {
    /// Root depth is zero; each child increases by one.
    pub depth: u8,
    /// Projected structural kind.
    pub kind: MenuKind,
    /// Explicit/default-resolved visible label; well-formed UTF-8 without NUL.
    pub label: String,
    /// Source enabled bit; UI additionally gates on current role/menu generation.
    pub enabled: bool,
    /// Some only for Checkbox, including the explicit false state.
    pub checked: Option<bool>,
    /// Typed fixed action.
    pub action: MenuAction,
}

/// One immutable installation request. A flat sequence bounds hostile decode depth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuRequestV1 {
    /// Entire preordered tree, bounded before deserializing any node allocation.
    #[serde(deserialize_with = "deserialize_menu_nodes")]
    pub nodes: Vec<MenuNodeV1>,
}

/// Actual installed native-generation receipt, never the public void return value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuInstallReceipt {
    /// Host-minted nonzero menu generation, independent of PID or `WindowRef`.
    pub generation: u64,
}

fn deserialize_menu_nodes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<MenuNodeV1>, D::Error> {
    crate::dialog::bounded_vec::<_, MenuNodeV1, MAX_MENU_NODES>(deserializer)
}

fn invalid(detail: &'static str) -> IpcError {
    IpcError::Protocol { detail }
}

/// Decode one bounded menu request before the UI owner allocates native handles.
///
/// # Errors
/// Refuses oversized, malformed, trailing, invalid-enum or structurally invalid data.
pub fn decode_menu_request(payload: &[u8]) -> Result<MenuRequestV1, IpcError> {
    if payload.len() > MAX_MENU_REQUEST_BYTES {
        return Err(invalid("menu request exceeds its bounded envelope"));
    }
    let request: MenuRequestV1 = codec::decode(payload)?;
    validate_menu_request(&request)?;
    Ok(request)
}

/// Validate the sole typed projection schema. Callers must also prove Guard/currentness.
///
/// # Errors
/// Refuses invalid total/depth/label bounds, child structure and kind/action/check state.
pub fn validate_menu_request(request: &MenuRequestV1) -> Result<(), IpcError> {
    if request.nodes.len() > MAX_MENU_NODES {
        return Err(invalid("too many menu nodes"));
    }
    let mut prior: Option<&MenuNodeV1> = None;
    for node in &request.nodes {
        if node.depth > MAX_MENU_DEPTH || prior.is_none() && node.depth != 0 {
            return Err(invalid("menu depth is invalid"));
        }
        if let Some(previous) = prior
            && (node.depth > previous.depth + 1
                || node.depth == previous.depth + 1 && previous.kind != MenuKind::Submenu)
        {
            return Err(invalid("menu preorder parent is invalid"));
        }
        if node.label.len() > MAX_MENU_LABEL_BYTES || node.label.contains('\0') {
            return Err(invalid("menu label exceeds its bounded UTF-8 grammar"));
        }
        if (node.kind == MenuKind::Checkbox) != node.checked.is_some() {
            return Err(invalid("menu check state does not match its kind"));
        }
        if (matches!(node.kind, MenuKind::Separator | MenuKind::Submenu)
            && node.action != MenuAction::Inert)
            || (node.kind != MenuKind::Normal && matches!(node.action, MenuAction::NativeRole(_)))
        {
            return Err(invalid("menu action does not match its kind"));
        }
        prior = Some(node);
    }
    if codec::encode(request)?.len() > MAX_MENU_REQUEST_BYTES {
        return Err(invalid("menu request exceeds its bounded envelope"));
    }
    Ok(())
}
