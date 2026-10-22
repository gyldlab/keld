//! Source-local opt-in real Bun response-body witness for KEL89 actual auth.
//! The original KEL89 four-case selection is unchanged.
use super::{CASES, QuitBodyReceipt, SmokeResult, require, run_case_with_body_receipt};
use std::path::{Path, PathBuf};

#[test]
fn actual_default_deny_quit_body_is_verified_by_real_bun_before_kernel_exit() -> SmokeResult<()> {
    // This separate opt-in never changes the original KEL89 four-case test.
    // The same real Host/Guardian/Bun chain and original per-CLI 20s clock
    // remain mandatory. A native one-use GET/ACK proves body consumption
    // before the fixture reaches its actual app.quit() callsite.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("workspace root")?;
    let host = PathBuf::from(env!("CARGO_BIN_EXE_keld-host"));
    let cli = host.parent().ok_or("host installation root")?.join("keld");
    require(
        cli.is_file() && host.is_file(),
        "shipping CLI and Host must exist",
    )?;
    let owned = tempfile::Builder::new()
        .prefix("kel89-body-receipt-")
        .tempdir_in("/tmp")?;
    let mut receipt = QuitBodyReceipt::arm();
    let _ = run_case_with_body_receipt(
        owned.path(),
        repo,
        &cli,
        &host,
        None,
        &CASES[0],
        Some(&mut receipt),
    )?;
    Ok(())
}

#[test]
fn shipping_auth_fixture_really_checks_quit_body_before_native_ack_and_app_quit() {
    let fixture = include_str!("../../../../fixtures/public_auth_app.ts");
    let body = fixture
        .find("await quit.text() !== \"quit\"")
        .expect("original real Bun body comparison before native receipt");
    let receipt_anchor = fixture
        .find("console.error(\"KELD_KEL89_PUBLIC_QUIT_REQUEST \" + smokeCase);")
        .expect("real Bun post-body marker used as the unique opt-in insertion point");
    let quit = fixture
        .find("await app.quit();")
        .expect("shipping ordinary API Quit callsite");
    assert!(
        body < receipt_anchor && receipt_anchor < quit,
        "real Bun quit.text() must precede the test-only ACK and public app.quit()"
    );
}
