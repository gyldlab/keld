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
