//! Links `keld-updater-helper.exe` against the static C runtime and makes the loader
//! resolve its static imports only from System32 (KEL-53 §4 "Helper launch and
//! self-anchor"; KEL-270 T4d S9c).
//!
//! `+crt-static` is a target-wide flag, so this one binary swaps the runtime through
//! link arguments instead. rustc names the dynamic runtime as the default library
//! `msvcrt` (the `libc` crate's link attribute), which `/NODEFAULTLIB` removes together
//! with the runtime import libraries it would pull in; the static `libcmt`,
//! `libvcruntime` and `libucrt` replace them. The resulting image imports no C runtime
//! DLL, so the loader searches for none of `vcruntime140.dll`, `ucrtbase.dll` or the
//! `api-ms-win-crt-*` sets. `/DEPENDENTLOADFLAG:0x800`
//! (`LOAD_LIBRARY_SEARCH_SYSTEM32`) covers the static imports that remain.

use std::ffi::OsStr;
use std::process::ExitCode;

/// The one binary target these arguments apply to.
const BIN: &str = "keld-updater-helper";

/// The helper's loader-hardening and static-runtime link arguments, in order.
const LINK_ARGS: [&str; 7] = [
    "/DEPENDENTLOADFLAG:0x800",
    "/NODEFAULTLIB:msvcrt.lib",
    "/NODEFAULTLIB:vcruntime.lib",
    "/NODEFAULTLIB:ucrt.lib",
    "/DEFAULTLIB:libcmt.lib",
    "/DEFAULTLIB:libvcruntime.lib",
    "/DEFAULTLIB:libucrt.lib",
];

fn main() -> ExitCode {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_CFG_TARGET_OS").as_deref() != Some(OsStr::new("windows")) {
        // Every other target builds the non-Windows refusal, which loads nothing.
        return ExitCode::SUCCESS;
    }
    if std::env::var_os("CARGO_CFG_TARGET_ENV").as_deref() != Some(OsStr::new("msvc")) {
        eprintln!(
            "keld-updater-helper builds only for the Windows MSVC targets: its static runtime \
             and System32-only import resolution are MSVC link arguments, and an unhardened \
             elevated helper is never built. Build it for x86_64-pc-windows-msvc."
        );
        return ExitCode::FAILURE;
    }
    for arg in LINK_ARGS {
        println!("cargo:rustc-link-arg-bin={BIN}={arg}");
    }
    ExitCode::SUCCESS
}
