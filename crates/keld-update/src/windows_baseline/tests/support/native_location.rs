//! Native installation-location and cleanup proof, independent of managed scratch.

use std::path::PathBuf;

use crate::tests::native_windows::native_install_tempdir;
use crate::windows_baseline::load_windows_baseline;
use crate::windows_baseline::tests::writer::seed_per_user_baseline;

#[test]
fn per_user_native_fixture_uses_local_app_data_is_unique_and_cleans_up() {
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("native LocalAppData"))
        .canonicalize()
        .expect("resolve existing native location");
    let first = native_install_tempdir().expect("first unique native fixture");
    let second = native_install_tempdir().expect("second unique native fixture");
    let path = first.path().canonicalize().expect("native fixture exists");
    let second_path = second.path().canonicalize().expect("second fixture exists");
    assert_eq!(
        path.parent(),
        Some(local.as_path()),
        "native installations must not inherit generic scratch"
    );
    assert_eq!(second_path.parent(), Some(local.as_path()));
    assert_ne!(
        path, second_path,
        "two concurrent fixtures must not share an installation"
    );
    let trust = seed_per_user_baseline(&path);
    let loaded = load_windows_baseline(&trust).expect("actual native baseline remains admissible");
    assert_eq!(loaded.version_floor(), "1.0.0");
    assert_eq!(
        loaded.identity().install_mode,
        crate::DirectInstallMode::PerUserDirect
    );
    drop(loaded);
    first
        .close()
        .expect("close and remove real native installation");
    second
        .close()
        .expect("close and remove independent fixture");
    assert!(
        !path.exists() && !second_path.exists(),
        "native fixture cleanup is observable"
    );
}
