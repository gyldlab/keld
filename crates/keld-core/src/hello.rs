//! Phase 1 hello-world window (delegates to `keld-wv`).

use keld_wv::{HELLO_HTML, WvError, run_hello_window as wv_run_hello};

/// Opens the default Keld hello window until the user closes it.
///
/// # Errors
///
/// Forwards [`keld_wv::WvError`] from the webview layer.
pub fn run_hello_window() -> Result<(), WvError> {
    wv_run_hello("Keld", HELLO_HTML)
}
