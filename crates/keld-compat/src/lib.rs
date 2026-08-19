//! keld-compat — host-side Electron emulation.
//!
//! Layer 1–2 (KEL-72): the TypeScript shim lives in `packages/@keld/electron`
//! and maps `app.whenReady` / `app.quit` / `window-all-closed` onto the
//! generic host-lifecycle kipc channel (`keld_ipc::LIFECYCLE_CHANNEL`,
//! served by `keld_core::LifecycleSession`). Electron names stay out of
//! `keld-core` / `keld-ipc`.
//!
//! This crate still owns the semantics JS cannot fake later: custom
//! `protocol` schemes, `session` cookie/proxy subsets, `webContents`
//! routing identity, window parenting/modals, `nativeImage` codecs.
//! Normative spec: `docs/architecture/04-electron-compat.md` §3.
//!
//! Generic compatibility evidence (KEL-74) lives in [`evidence`]: a versioned
//! record + committed-denominator scorer. That module is not an Electron shim
//! and does not encode VS Code or package names.

pub mod evidence;

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
