//! KEL-102/T2 verified-manifest public contract.
#![allow(
    clippy::expect_used,
    reason = "Clippy does not classify Cargo integration-test crates as tests for allow-expect-in-tests"
)]

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use keld_guard::load_manifest;
use keld_guard::verified_manifest::load_verified_manifest;
use keld_guard::{Decision, DenyReason, ManifestError, Principal, evaluate};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

fn digest(hex: &str) -> [u8; 32] {
    assert_eq!(hex.len(), 64, "test digest must be SHA-256 hex");
    let mut bytes = [0_u8; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] =
            u8::from_str_radix(std::str::from_utf8(pair).expect("digest pair is UTF-8"), 16)
                .expect("digest pair is hexadecimal");
    }
    bytes
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "keld-guard-kel102-{}-{label}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn opened_file(label: &str, bytes: &[u8]) -> (File, PathBuf) {
    let path = temp_path(label);
    write_new(&path, bytes);
    let file = File::open(&path).expect("open retained manifest fixture");
    (file, path)
}

fn write_new(path: &std::path::Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("create new manifest fixture without following a stale path");
    file.write_all(bytes).expect("write manifest fixture");
}

#[test]
fn verified_loader_uses_the_supplied_handle_not_the_diagnostic_path() {
    let (file, actual_path) = opened_file("diagnostic-only", b"{}\n");
    let display_path = temp_path("missing-display-path");
    let expected = digest("ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356");

    let verified = load_verified_manifest(file, display_path, expected)
        .expect("diagnostic-only path must not be opened");
    assert_eq!(verified.verified_sha256(), expected);
    assert!(
        matches!(
            evaluate(
                verified.manifest(),
                Principal::AppProcess,
                "fs.read",
                "/not-granted"
            ),
            Decision::Deny(DenyReason::NotGranted { .. })
        ),
        "valid empty policy must remain deliberate all-deny"
    );

    fs::remove_file(actual_path).expect("remove manifest fixture");
}

#[cfg(unix)]
#[test]
fn one_shot_handle_is_the_only_hash_and_parse_source() {
    use std::net::Shutdown;
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

    let policy = b"{\"app\":{\"fs\":{\"read\":[\"/only-a/**\"]}}}\n";
    let expected = digest("4b99ee25f61f973f84f26565a82c5dbd3004d49ee7fb0a9da27dc33411c2d8a8");
    let (reader, mut writer) = UnixStream::pair().expect("one-shot policy stream");
    writer.write_all(policy).expect("write one-shot policy");
    writer
        .shutdown(Shutdown::Write)
        .expect("finish one-shot policy");
    let file = File::from(OwnedFd::from(reader));
    let display_path = temp_path("existing-wrong-display");
    write_new(&display_path, b"{}\n");

    let verified = load_verified_manifest(file, display_path.clone(), expected)
        .expect("one-shot handle must supply both hash and parse bytes");
    assert_eq!(verified.verified_sha256(), expected);
    assert_eq!(
        evaluate(
            verified.manifest(),
            Principal::AppProcess,
            "fs.read",
            "/only-a/file.txt"
        ),
        Decision::Allow
    );
    let path_loaded = load_manifest(&display_path).expect("load display-path control");
    assert!(
        matches!(
            evaluate(
                &path_loaded,
                Principal::AppProcess,
                "fs.read",
                "/only-a/file.txt"
            ),
            Decision::Deny(DenyReason::NotGranted { .. })
        ),
        "display-path control must distinguish a forbidden reopen"
    );

    fs::remove_file(display_path).expect("remove display-path control");
}

#[cfg(unix)]
#[test]
fn retained_handle_survives_path_substitution_without_reopen() {
    let (file, path) = opened_file("retained-handle", b"{}\n");
    fs::remove_file(&path).expect("unlink selected manifest");
    write_new(&path, b"{not the selected inode}\n");
    let expected = digest("ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356");

    let verified = load_verified_manifest(file, path.clone(), expected)
        .expect("opened inode must remain the byte source");
    assert_eq!(verified.verified_sha256(), expected);

    fs::remove_file(path).expect("remove path substitution");
}

#[test]
fn digest_mismatch_reports_expected_and_actual_without_fallback() {
    let (file, path) = opened_file("digest-mismatch", b"{}\n");
    let expected = [0_u8; 32];
    let actual = digest("ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356");

    let error = load_verified_manifest(file, path.clone(), expected)
        .expect_err("mismatched bytes must fail closed");
    assert_eq!(error.code(), "KELD-GUARD016");
    assert_eq!(
        error,
        ManifestError::IntegrityMismatch {
            path: path.clone(),
            expected,
            actual,
        }
    );
    let message = error.to_string();
    assert!(message.contains("KELD-GUARD016"), "{message}");
    assert!(
        message.to_ascii_lowercase().contains("rebuild or re-sign"),
        "{message}"
    );

    fs::remove_file(path).expect("remove manifest fixture");
}

#[test]
fn verified_bytes_keep_utf8_and_jsonc_failures_distinct() {
    let (non_utf8_file, non_utf8_path) = opened_file("non-utf8", &[0xff]);
    let non_utf8_digest =
        digest("a8100ae6aa1940d0b663bb31cd466142ebbdbd5187131b92d93818987832eb89");
    let non_utf8 = load_verified_manifest(non_utf8_file, non_utf8_path.clone(), non_utf8_digest)
        .expect_err("non-UTF-8 policy must fail closed");
    assert_eq!(non_utf8.code(), "KELD-GUARD005");
    assert!(
        matches!(non_utf8, ManifestError::InvalidUtf8 { ref path, .. } if path == &non_utf8_path),
        "{non_utf8:?}"
    );

    let (wrong_digest_file, wrong_digest_path) = opened_file("non-utf8-wrong-digest", &[0xff]);
    let wrong_digest =
        load_verified_manifest(wrong_digest_file, wrong_digest_path.clone(), [0_u8; 32])
            .expect_err("integrity must be checked before UTF-8");
    assert_eq!(wrong_digest.code(), "KELD-GUARD016");
    assert!(
        matches!(wrong_digest, ManifestError::IntegrityMismatch { .. }),
        "{wrong_digest:?}"
    );

    let (malformed_file, malformed_path) = opened_file("malformed", b"{nope}\n");
    let malformed_digest =
        digest("ed4d18e4d7f58b800fafc0e89f02e9b76eca431e8a8314df677d02cee467920e");
    let malformed =
        load_verified_manifest(malformed_file, malformed_path.clone(), malformed_digest)
            .expect_err("malformed JSONC must fail closed");
    assert_eq!(malformed.code(), "KELD-GUARD005");
    assert!(
        matches!(malformed, ManifestError::Parse { path: Some(ref path), .. } if path == &malformed_path),
        "{malformed:?}"
    );

    fs::remove_file(non_utf8_path).expect("remove non-UTF-8 fixture");
    fs::remove_file(wrong_digest_path).expect("remove wrong-digest fixture");
    fs::remove_file(malformed_path).expect("remove malformed fixture");
}

#[test]
fn verified_loader_reuses_bounded_unique_parser() {
    let (oversized_file, oversized_path) = opened_file("oversized", &vec![b' '; 64 * 1024 + 1]);
    let oversized = load_verified_manifest(oversized_file, oversized_path.clone(), [0_u8; 32])
        .expect_err("verified input over 64 KiB must fail before hashing");
    assert_eq!(oversized.code(), "KELD-GUARD017");
    assert!(
        matches!(oversized, ManifestError::TooLarge { path: Some(ref path), .. } if path == &oversized_path),
        "{oversized:?}"
    );

    let duplicate_bytes = br#"{"app":{"fs":{"read":[],"read":["/outside/**"]}}}"#;
    let duplicate_digest =
        digest("06d74274d5d6a0351deac64c75ad477105c8316f61a0e6e62eb59e3e0f73e0d1");
    let (duplicate_file, duplicate_path) = opened_file("duplicate", duplicate_bytes);
    let duplicate =
        load_verified_manifest(duplicate_file, duplicate_path.clone(), duplicate_digest)
            .expect_err("verified input with decoded duplicate keys must fail closed");
    assert_eq!(duplicate.code(), "KELD-GUARD005");
    assert!(
        matches!(duplicate, ManifestError::Parse { path: Some(ref path), .. } if path == &duplicate_path),
        "{duplicate:?}"
    );
    assert!(
        duplicate.to_string().contains("duplicate object key"),
        "{duplicate}"
    );

    fs::remove_file(oversized_path).expect("remove oversized fixture");
    fs::remove_file(duplicate_path).expect("remove duplicate fixture");
}
