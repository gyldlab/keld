//! Process-isolated integration target so Common Controls v6 is embedded.
#![cfg(windows)]

#[path = "../src/dialog/windows/tests.rs"]
mod acceptance;
