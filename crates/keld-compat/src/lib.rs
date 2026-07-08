//! keld-compat — host-side Electron emulation.
//!
//! The TypeScript shim (`@keld/electron`) covers the API surface; this crate
//! covers the semantics JS cannot fake: custom `protocol` schemes wired into
//! the engine, `session` cookie/proxy subsets, `webContents` routing identity,
//! window parenting/modal behavior, and `nativeImage` codecs. Normative spec:
//! `docs/architecture/04-electron-compat.md` §3.

/// Compat tiers, mirrored on the public scoreboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Lifecycle, windows, IPC, dialogs, menus, tray, clipboard, notifications.
    One,
    /// Shortcuts, power, safeStorage, session/protocol subsets, updater bridge.
    Two,
    /// `<webview>` mapping, capture, net module.
    Three,
}
