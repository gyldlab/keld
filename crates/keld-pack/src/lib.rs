//! keld-pack — packaging and cross-compilation.
//!
//! Because the host ships prebuilt per platform and app code is portable JS,
//! packaging is data assembly + signing — enabling `keld build --target ...`
//! for every platform from one machine (pure-Rust installer authoring).
//! Normative spec: `docs/architecture/06-runtime-and-tooling.md` §3.

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
