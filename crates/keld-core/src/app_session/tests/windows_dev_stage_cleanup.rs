// GH-1118: the Windows dev-stage cleanup sentinel reports one code. keld-core's stage
// validate, prepare and delete steps carry `KELD-CORE-046` like the host's own sentinel
// steps, never the no-flag session code `KELD-CORE-037`.

#[test]
#[cfg(windows)]
fn windows_cleanup_root_refusal_carries_the_sentinel_code() {
    let temp = tempfile::tempdir().expect("sentinel root fixture");
    let error = super::validate_windows_cleanup_root(&temp.path().join("not-a-nonce"))
        .expect_err("a root outside the .keld/dev/<nonce> layout must be refused");
    assert_eq!(error.code(), "KELD-CORE-046", "{error}");
    let message = error.to_string();
    assert!(message.contains("KELD-CORE-046"), "{message}");
    assert!(!message.contains("KELD-CORE-037"), "{message}");
    assert!(message.contains("relaunch through `keld dev`"), "{message}");

    // Negative control: the canonical layout is accepted, so the code belongs to the
    // refusal and not to every call.
    let canonical = temp
        .path()
        .join(".keld")
        .join("dev")
        .join("0123456789abcdef0123456789abcdef");
    super::validate_windows_cleanup_root(&canonical)
        .expect("the canonical nonce layout is accepted");
    // Negative control: the shared session helper still carries the session code, so the
    // sentinel code cannot be inherited from it by routing through that helper.
    assert_eq!(
        app_detail("control", "the generic session helper").code(),
        "KELD-CORE-037"
    );
}

#[test]
#[cfg(windows)]
fn windows_cleanup_prepare_refusals_carry_the_sentinel_code() {
    let temp = tempfile::tempdir().expect("sentinel stage fixture");
    let stage = temp
        .path()
        .join(".keld")
        .join("dev")
        .join("0123456789abcdef0123456789abcdef");
    // A missing stage root fails at canonicalization, before any process handle exists.
    let error = super::WindowsDevStageCleanup::prepare(&stage, std::process::id())
        .expect_err("a missing stage root must be refused");
    assert_eq!(error.code(), "KELD-CORE-046", "{error}");
    assert!(!error.to_string().contains("KELD-CORE-037"), "{error}");

    // A bare directory with the canonical layout is refused by the owner-private ACL step
    // or, failing that, by the missing staged host image; both are sentinel steps.
    fs::create_dir_all(&stage).expect("create bare stage root");
    let error = super::WindowsDevStageCleanup::prepare(&stage, std::process::id())
        .expect_err("a bare directory is not a validated stage");
    assert_eq!(error.code(), "KELD-CORE-046", "{error}");
    assert!(!error.to_string().contains("KELD-CORE-037"), "{error}");
}
