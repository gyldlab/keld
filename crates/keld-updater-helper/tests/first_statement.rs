//! `main`'s first statement restricts every later DLL search to System32 and fails
//! closed (KEL-53 §4 "Helper launch and self-anchor").
//!
//! This is a source pin, not a behavioural test. The helper loads no DLL by name that a
//! test could plant, so no hosted test can observe its post-`main` search order. S9b's
//! child-process test proves the `keld-runtime` primitive itself. This pin fails when
//! the call moves after other work, when it is removed, or when its failure no longer
//! returns the refusal.
#![allow(clippy::expect_used)] // extra test crate: expect is an assertion oracle

use std::path::Path;

/// The first four non-blank, non-comment lines of `main`'s body, exactly.
const FIRST_STATEMENT: [&str; 4] = [
    "#[cfg(windows)]",
    "if let Err(error) = keld_runtime::windows_job::restrict_dll_search_to_system32() {",
    "return refuse(&HelperError::DllSearch(error));",
    "}",
];

#[test]
fn main_restricts_the_dll_search_first_and_fails_closed() {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("read src/main.rs");
    let signature = "fn main() -> ExitCode {";
    assert_eq!(
        source.matches(signature).count(),
        1,
        "src/main.rs has exactly one `{signature}`"
    );
    let (_, body) = source
        .split_once(signature)
        .expect("src/main.rs defines main");
    let first: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .take(FIRST_STATEMENT.len())
        .collect();
    assert_eq!(
        first, FIRST_STATEMENT,
        "main's first statement must restrict the DLL search to System32 and return its refusal"
    );
}
