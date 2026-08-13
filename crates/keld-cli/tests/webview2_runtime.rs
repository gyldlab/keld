//! Missing-`WebView2`-runtime contract on Windows (KEL-27, `KELD-WV-008`).
//!
//! The Evergreen runtime is a separate redistributable, so "it is not
//! installed" is a real state a user hits — and the one where a bad error
//! message costs the most, because the fix is entirely in the user's hands.
//! This asserts the observable contract a user actually sees: a non-zero exit,
//! no window, and a message carrying the code plus the install URL.
//!
//! The absent runtime is simulated with `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`,
//! which the `WebView2` loader honours ahead of the registry when the
//! browser-executable-folder argument is null (which is what
//! `keld_wv::webview2::runtime_version` passes). Pointing it at an empty
//! directory is the only way to exercise this without uninstalling Edge.
//!
//! Windows-only: the env var and the backend both exist only there.

#![cfg(target_os = "windows")]
#![allow(clippy::expect_used)] // extra test crate: expect is the assertion oracle

use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Spawns `keld hello` with the runtime redirected to an empty directory.
///
/// Runs on a worker thread with a receive deadline so a regression that lets
/// the window open fails as a timeout instead of hanging the suite forever.
#[test]
fn hello_without_webview2_runtime_is_keld_wv_008_and_opens_no_window() {
    let empty = tempfile::tempdir().expect("empty runtime dir");
    let empty_path = empty.path().to_owned();
    let bin = env!("CARGO_BIN_EXE_keld").to_owned();

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let out = Command::new(bin)
            .arg("hello")
            .env("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &empty_path)
            .output();
        let _ = tx.send(out);
    });

    let out = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("a missing runtime must fail fast, not open a window or hang")
        .expect("spawn keld hello");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let text = format!("{stderr}{stdout}");

    assert!(
        !out.status.success(),
        "missing runtime must exit non-zero, got {:?}. output: {text}",
        out.status.code()
    );
    assert!(
        text.contains("KELD-WV-008"),
        "missing error code in: {text}"
    );
    // The fix has to be actionable: the user cannot proceed without the URL,
    // and Keld deliberately does not fetch the installer for them.
    assert!(
        text.contains("developer.microsoft.com/microsoft-edge/webview2"),
        "missing install URL in: {text}"
    );
    assert!(
        text.contains("will not download"),
        "must state Keld does not fetch the installer, got: {text}"
    );
    // A bare HRESULT with no guidance is the failure mode this test exists to
    // prevent, so the underlying loader detail must not be the whole message.
    assert!(
        text.len() > "KELD-WV-008".len() + 40,
        "message is too terse to act on: {text}"
    );
}
