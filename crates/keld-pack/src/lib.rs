//! keld-pack — packaging and cross-target assembly contracts.
//!
//! [`Format`] names installer outputs. Architecture, signing, and assembly rules are in
//! `docs/architecture/06-runtime-and-tooling.md` §3; repository maturity and evidence
//! live in `docs/engineering/product-status.tsv`.

/// Installer formats keld-pack can author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// macOS application bundle.
    App,
    /// macOS disk image.
    Dmg,
    /// Windows NSIS installer.
    Nsis,
    /// Windows MSI installer.
    Msi,
    /// Debian package.
    Deb,
    /// RPM package.
    Rpm,
    /// Linux `AppImage`.
    AppImage,
}
