//! `keld-updater-helper.exe`, the elevated updater helper (KEL-53 §4 "Helper launch and
//! self-anchor"; KEL-270 T4d slice S9c).
//!
//! The helper composes existing owners and holds no `unsafe`: `keld-ipc` decides its
//! argument's shape, `keld-guard` verifies its own image once, and `keld-update`
//! anchors it to the installation that holds it. Its build script links the C runtime
//! statically and sets `/DEPENDENTLOADFLAG:0x800`, so before `main` the loader
//! resolves its static imports only from System32 and searches for no runtime DLL.
//!
//! Every refusal precedes the writer lease and any write. Until their slices land,
//! both roles refuse right after a passing self-anchor (KEL-53 §4 *Interim*).
#![forbid(unsafe_code)]
// No console window: `ShellExecuteExW` starts the helper elevated, and a console
// subsystem image would open one. Tests still read a redirected stderr.
#![windows_subsystem = "windows"]

mod error;
#[cfg(windows)]
mod helper;

use std::io::Write as _;
use std::process::ExitCode;

use error::HelperError;

fn main() -> ExitCode {
    let result = run();
    report(result.as_ref().err());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

#[cfg(windows)]
fn run() -> Result<(), HelperError> {
    helper::run(std::env::args_os().skip(1))
}

#[cfg(not(windows))]
fn run() -> Result<(), HelperError> {
    Err(HelperError::Invocation {
        detail: "it runs only on Windows".to_owned(),
    })
}

/// Writes a refusal to stderr. A helper started elevated through `ShellExecuteExW` has
/// no stderr, so a failed write leaves the exit status as the refusal's only signal.
fn report(error: Option<&HelperError>) {
    if let Some(error) = error {
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "{error}");
        let _ = stderr.flush();
    }
}
