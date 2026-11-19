//! Document-start registration for a retained Windows original view.

mod event;
mod installer;
pub use event::NativeEventRetirement;
pub(super) use installer::{InstallStart, InstallStatus, PreloadInstaller};
