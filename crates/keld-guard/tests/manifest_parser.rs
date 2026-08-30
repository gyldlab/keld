//! Hostile-input contracts for the single permissions-manifest parser.

#![allow(clippy::expect_used, clippy::panic)] // test setup/cleanup failures are assertion oracles

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use keld_guard::{Decision, Principal, evaluate, load_manifest, parse_manifest};

const MAX_MANIFEST_BYTES: usize = 64 * 1024;

fn manifest_with_size(size: usize) -> String {
    const PREFIX: &str = r#"{"ignored":""#;
    const SUFFIX: &str = r#""}"#;
    assert!(size >= PREFIX.len() + SUFFIX.len());
    format!(
        "{PREFIX}{}{SUFFIX}",
        "x".repeat(size - PREFIX.len() - SUFFIX.len())
    )
}

struct TempManifest(PathBuf);

impl TempManifest {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "keld-guard-manifest-parser-{}-{nonce}.jsonc",
            std::process::id()
        )))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempManifest {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
            && !std::thread::panicking()
        {
            panic!("remove temporary manifest {}: {error}", self.0.display());
        }
    }
}

#[test]
fn duplicate_keys_reject_recursively_after_json_decoding() {
    let cases = [
        r#"{"app":{"fs":{"read":[],"read":["/outside/**"]}}}"#,
        r#"{"app":{"fs":{"read":["/outside/**"],"read":[]}}}"#,
        r#"{"app":{"fs":{"read":[],"\u0072ead":["/outside/**"]}}}"#,
        r#"{"app":{"fs":{"\u0072ead":["/outside/**"],"read":[]}}}"#,
        r#"{"app":{"fs":{"read":[]/* split */,"read":["/outside/**"]}}}"#,
        r#"{"app":{},"app":{"fs":{"read":["/outside/**"]}}}"#,
        r#"{"windows":{"main":{"channels":[],"channels":["fs.read"]}}}"#,
        r#"{"audit":[{"log":"deny","log":"all"}]}"#,
    ];

    for text in cases {
        let error = parse_manifest(text)
            .expect_err("decoded duplicate key must reject before policy construction");
        let rendered = error.to_string();
        assert!(rendered.contains("KELD-GUARD005"), "{rendered}: {text}");
        assert!(rendered.contains("duplicate object key"), "{rendered}");
    }

    let dangerous = r#"{"app":{"fs":{"read":[],"read":["/outside/**"]}}}"#;
    if let Ok(manifest) = parse_manifest(dangerous) {
        assert_ne!(
            evaluate(
                &manifest,
                Principal::AppProcess,
                "fs.read",
                "/outside/secret"
            ),
            Decision::Allow,
            "last-wins decoding must never turn an ambiguous manifest into authority"
        );
    }
}

#[test]
fn comment_removal_cannot_join_two_json_tokens() {
    let error = parse_manifest(r#"{"ignored":1/* separator */2}"#)
        .expect_err("a comment cannot turn two adjacent numbers into one value");
    assert!(error.to_string().contains("KELD-GUARD005"), "{error}");
}

#[test]
fn line_comments_end_at_cr_only_and_crlf_terminators() {
    let cr_only = "{\r// comment\r\"app\":{}}";
    let crlf = "{\r\n// comment\r\n\"app\":{}}";
    parse_manifest(cr_only).expect("CR must terminate a line comment");
    parse_manifest(crlf).expect("CRLF must terminate a line comment without swallowing content");
}

#[test]
fn unterminated_block_comment_rejects_instead_of_truncating_to_valid_json() {
    let error = parse_manifest(r#"{"app":{}} /* unterminated"#)
        .expect_err("EOF inside a block comment must reject");
    let rendered = error.to_string();
    assert!(rendered.contains("KELD-GUARD005"), "{rendered}");
    assert!(
        rendered.contains("unterminated block comment"),
        "{rendered}"
    );
}

#[test]
fn memory_and_disk_inputs_enforce_the_same_64_kib_boundary() {
    let exact = manifest_with_size(MAX_MANIFEST_BYTES);
    let over = manifest_with_size(MAX_MANIFEST_BYTES + 1);
    assert_eq!(exact.len(), MAX_MANIFEST_BYTES);
    assert_eq!(over.len(), MAX_MANIFEST_BYTES + 1);
    parse_manifest(&exact).expect("exactly 64 KiB remains valid");

    let memory_error = parse_manifest(&over).expect_err("64 KiB + 1 must reject in memory");
    let memory_rendered = memory_error.to_string();
    assert!(
        memory_rendered.contains("KELD-GUARD017"),
        "{memory_rendered}"
    );
    assert!(memory_rendered.contains("64 KiB"), "{memory_rendered}");

    let temp = TempManifest::new();
    let path = temp.path();
    fs::write(path, &exact).expect("write exact-boundary manifest");
    load_manifest(path).expect("disk input at exactly 64 KiB remains valid");
    fs::write(path, &over).expect("write over-boundary manifest");
    let disk_error = load_manifest(path).expect_err("disk input at 64 KiB + 1 must reject");

    let disk_rendered = disk_error.to_string();
    assert!(disk_rendered.contains("KELD-GUARD017"), "{disk_rendered}");
    assert!(disk_rendered.contains("64 KiB"), "{disk_rendered}");
    assert!(
        disk_rendered.contains(&path.display().to_string()),
        "{disk_rendered}"
    );
}
