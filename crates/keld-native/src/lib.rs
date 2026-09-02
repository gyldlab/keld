//! keld-native — native OS API modules.
//!
//! Native services pass through `keld-guard` and are exposed as typed kipc channels.
//! See [`fs`] for the filesystem broker and [`MODULES`] for the declared surface.
//! Normative spec: `docs/architecture/05-webview-and-native.md` §3. Repository
//! maturity and evidence live in `docs/engineering/product-status.tsv`.

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
