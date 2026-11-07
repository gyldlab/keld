//! D1 verified-policy provenance, selection and exact catalog contract.
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use keld_guard::verified_manifest::{VerifiedManifest, load_verified_manifest};
use keld_guard::{WindowCspProfile, parse_manifest};
use sha2::{Digest as _, Sha256};

static NEXT: AtomicU64 = AtomicU64::new(0);
const POLICY: &str =
    r#"{"profile":"legacy","windows":{"editor":{"web":{"csp":"original-editor-offline"}}}}"#;
const HEADER: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self' data:; media-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'";
const EMPTY_STYLE_POLICY: &str = r#"{"profile":"legacy","windows":{"editor":{"web":{"csp":"original-editor-offline-empty-style"}}}}"#;
const EMPTY_STYLE_HEADER: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='; font-src 'self' data:; media-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'";

struct PolicyFile(PathBuf);
impl PolicyFile {
    fn new(bytes: &[u8]) -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "keld-window-csp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        file.write_all(bytes)?;
        Ok(Self(path))
    }
    fn verified(&self, bytes: &[u8]) -> Result<VerifiedManifest, Box<dyn std::error::Error>> {
        Ok(load_verified_manifest(
            File::open(&self.0)?,
            self.0.clone(),
            Sha256::digest(bytes).into(),
        )?)
    }
}
impl Drop for PolicyFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn explicit_empty_style_catalog_choice_preserves_the_existing_strict_header()
-> Result<(), Box<dyn std::error::Error>> {
    let selected = PolicyFile::new(EMPTY_STYLE_POLICY.as_bytes())?;
    let verified = selected.verified(EMPTY_STYLE_POLICY.as_bytes())?;
    let resolved = verified.resolve_window_csp("editor")?;
    assert_eq!(resolved.response_header(), EMPTY_STYLE_HEADER);
    assert_eq!(
        resolved.verified_sha256(),
        <[u8; 32]>::from(Sha256::digest(EMPTY_STYLE_POLICY.as_bytes()))
    );
    assert!(!resolved.response_header().contains("unsafe-inline"));
    assert!(!resolved.response_header().contains("unsafe-hashes"));
    let strict = PolicyFile::new(POLICY.as_bytes())?;
    assert_eq!(
        strict
            .verified(POLICY.as_bytes())?
            .resolve_window_csp("editor")?
            .response_header(),
        HEADER
    );
    assert_eq!(
        EMPTY_STYLE_HEADER.replace(" 'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='", ""),
        HEADER,
        "the explicit new choice adds exactly one initial-empty-text hash",
    );
    Ok(())
}

#[test]
fn original_policy_resolution_counts_both_choices_without_preference_or_fallback()
-> Result<(), Box<dyn std::error::Error>> {
    for (bytes, expected) in [(POLICY, HEADER), (EMPTY_STYLE_POLICY, EMPTY_STYLE_HEADER)] {
        let file = PolicyFile::new(bytes.as_bytes())?;
        let verified = file.verified(bytes.as_bytes())?;
        let selected = verified.resolve_unique_original_window_csp()?;
        assert_eq!(selected.window_key(), "editor");
        assert_eq!(selected.response_header(), expected);
        assert_eq!(selected, verified.resolve_window_csp("editor")?);
    }
    for bytes in [
        "{}",
        r#"{"windows":{"a":{"web":{"csp":"original-editor-offline"}},"b":{"web":{"csp":"original-editor-offline"}}}}"#,
        r#"{"windows":{"a":{"web":{"csp":"original-editor-offline-empty-style"}},"b":{"web":{"csp":"original-editor-offline-empty-style"}}}}"#,
        r#"{"windows":{"a":{"web":{"csp":"original-editor-offline"}},"b":{"web":{"csp":"original-editor-offline-empty-style"}}}}"#,
    ] {
        let file = PolicyFile::new(bytes.as_bytes())?;
        let error = file
            .verified(bytes.as_bytes())?
            .resolve_unique_original_window_csp()
            .expect_err("missing or mixed/duplicate choices cannot select an original window");
        assert_eq!(error.code(), "KELD-GUARD005");
        assert!(error.to_string().contains("exactly one trusted window key"));
    }
    for selector in [
        "original-editor-offline-empty-style-extra",
        "unsafe-inline",
        "",
    ] {
        assert!(
            parse_manifest(
                &EMPTY_STYLE_POLICY.replace("original-editor-offline-empty-style", selector)
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn exact_catalog_policy_retains_actual_key_and_verified_digest_without_inline_fallback()
-> Result<(), Box<dyn std::error::Error>> {
    let file = PolicyFile::new(POLICY.as_bytes())?;
    let verified = file.verified(POLICY.as_bytes())?;
    let policy = verified
        .resolve_unique_window_csp(WindowCspProfile::OriginalEditorOffline)
        .expect("one trusted policy key");
    assert_eq!(policy.window_key(), "editor");
    assert_eq!(
        policy.verified_sha256(),
        <[u8; 32]>::from(Sha256::digest(POLICY.as_bytes()))
    );
    assert_eq!(policy.response_header(), HEADER);
    assert!(!policy.response_header().contains("unsafe-inline"));
    assert!(!policy.response_header().contains("'unsafe-eval'"));
    assert_eq!(
        verified.resolve_window_csp("editor").expect("same key"),
        policy
    );
    assert!(
        parse_manifest(POLICY)
            .expect("data parser")
            .declares_window_csp()
    );
    Ok(())
}

#[test]
fn missing_and_ambiguous_policy_never_guess_first_window_or_main()
-> Result<(), Box<dyn std::error::Error>> {
    for bytes in [
        b"{}".as_slice(),
        br#"{"windows":{"main":{"web":{}}}}"#.as_slice(),
        br#"{"windows":{"a":{"web":{"csp":"original-editor-offline"}},"b":{"web":{"csp":"original-editor-offline"}}}}"#.as_slice(),
    ] {
        let file = PolicyFile::new(bytes)?;
        let verified = file.verified(bytes)?;
        let error = verified.resolve_unique_window_csp(WindowCspProfile::OriginalEditorOffline).expect_err("missing or ambiguous must refuse");
        assert_eq!(error.code(), "KELD-GUARD005");
        assert!(error.to_string().contains("exactly one trusted window key"), "{error}");
    }
    let file = PolicyFile::new(POLICY.as_bytes())?;
    let verified = file.verified(POLICY.as_bytes())?;
    let error = verified
        .resolve_window_csp("main")
        .expect_err("never guess main");
    assert_eq!(error.code(), "KELD-GUARD005");
    assert!(error.to_string().contains("main"));
    assert!(error.to_string().contains("before loading content"));
    Ok(())
}

#[test]
fn app_network_grants_cannot_rewrite_verified_window_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let changed = r#"{"profile":"legacy","app":{"net":{"fetch":["https://outside.invalid/**"]}},"windows":{"editor":{"web":{"csp":"original-editor-offline"}}}}"#;
    let a = PolicyFile::new(POLICY.as_bytes())?;
    let b = PolicyFile::new(changed.as_bytes())?;
    let va = a.verified(POLICY.as_bytes())?;
    let vb = b.verified(changed.as_bytes())?;
    assert_ne!(va.verified_sha256(), vb.verified_sha256());
    assert_eq!(
        va.resolve_window_csp("editor")
            .expect("policy a")
            .response_header(),
        HEADER
    );
    assert_eq!(
        vb.resolve_window_csp("editor")
            .expect("policy b")
            .response_header(),
        HEADER
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn retained_verified_policy_cannot_follow_later_path_or_manifest_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let file = PolicyFile::new(POLICY.as_bytes())?;
    let opened = File::open(&file.0).expect("retain exact source");
    fs::remove_file(&file.0).expect("retire selected inode path");
    fs::write(&file.0, b"{}").expect("unrelated replacement");
    let verified = load_verified_manifest(
        opened,
        file.0.clone(),
        Sha256::digest(POLICY.as_bytes()).into(),
    )
    .expect("same retained bytes");
    assert_eq!(
        verified
            .resolve_window_csp("editor")
            .expect("retained selected policy")
            .response_header(),
        HEADER
    );
    let mismatch = load_verified_manifest(
        File::open(&file.0).expect("replacement inode"),
        file.0.clone(),
        Sha256::digest(POLICY.as_bytes()).into(),
    )
    .expect_err("replacement cannot mint verified policy");
    assert_eq!(mismatch.code(), "KELD-GUARD016");
    Ok(())
}
