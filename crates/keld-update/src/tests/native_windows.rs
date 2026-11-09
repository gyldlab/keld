//! One allocation owner for real per-user Windows installation and extraction fixtures.

#![forbid(unsafe_code)]

use std::io;
use std::path::PathBuf;

/// Isolated per-user native installation fixture; generic I/O fixtures use managed scratch.
pub(crate) fn native_install_tempdir() -> io::Result<tempfile::TempDir> {
    // Real per-user install ancestry is not interchangeable with generic TMP/TEMP
    // scratch. Reuse the installer's existing LocalAppData convention and retain
    // TempDir ownership; production still verifies the actual volume and ACLs.
    tempfile::Builder::new()
        .prefix("keld-update-native-")
        .tempdir_in(local_app_data())
}

pub(crate) fn local_app_data() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("current Windows LocalAppData"))
}
