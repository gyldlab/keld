//! keld-native — native OS API modules.
//!
//! Every module ships with all three platform implementations (or an explicit
//! documented gap), passes through `keld-guard`, and is exposed to TypeScript
//! as a typed kipc channel. Normative spec:
//! `docs/architecture/05-webview-and-native.md` §3.

/// Native modules planned for the v0.x surface, used by doctor/manifest tooling.
pub const MODULES: &[&str] = &[
    "window",
    "menu",
    "tray",
    "dialog",
    "notify",
    "clipboard",
    "shortcut",
    "screen",
    "power",
    "shell",
    "fs",
    "secrets",
    "deeplink",
    "autostart",
    "dock",
];
