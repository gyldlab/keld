//! Workspace integration coverage for the standalone competitor sync utility.
//
// The competitor sync utility is intentionally standalone and outside the
// Cargo workspace. Import its source so its real-Git regression tests run in
// the required native workspace Nextest matrix on Linux, macOS and Windows.
// The CLI entry point and some production helpers are intentionally unused here.
#[allow(dead_code)]
#[path = "../../../tools/competitors_sync.rs"]
mod competitors_sync;
