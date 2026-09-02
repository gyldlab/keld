//! keld-native — native OS API modules.
//!
//! The destination native-service contract routes privileged operations through
//! `keld-guard` and exposes them as typed kipc channels. See [`fs`] for the implemented
//! filesystem broker, [`MODULES`] for the declared surface, and
//! `docs/engineering/product-status.tsv` for current coverage and evidence. Normative
//! spec: `docs/architecture/05-webview-and-native.md` §3.

pub mod fs;

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
