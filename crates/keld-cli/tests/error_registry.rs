//! KEL-35: canonical `KELD-*` registry integrity.
//!
//! The registry file is the docs stub. These tests fail when:
//! - a code is listed twice
//! - an entry is missing `crate` / `message` / `fix`
//! - scanned crates or workspace tools emit a code that is not listed
//! - the registry lists a code the scanned sources do not emit
//!
//! Why: a deleted uniqueness check, an empty `fix`, or a new `KELD-WV-008` in
//! `error.rs` without a heading must fail CI. Scanning only developer-facing source
//! trees (not the whole repo) is the v0 scraper.

#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are assertion oracles

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

const REGISTRY_REL: &str = "docs/engineering/keld-error-codes.md";

const SCAN_REL: &[&str] = &[
    "crates/keld-ipc/src",
    "crates/keld-wv/src",
    "crates/keld-cli/src",
    "crates/keld-cli/templates",
    "crates/keld-guard/src",
    "crates/keld-runtime/src",
    "crates/keld-native/src",
    "crates/keld-compat/src",
    "tools",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryEntry {
    code: String,
    crate_name: String,
    message: String,
    fix: String,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/keld-cli -> workspace root")
        .to_path_buf()
}

fn registry_path() -> PathBuf {
    workspace_root().join(REGISTRY_REL)
}

/// Pulls `KELD-AREA-001` and `KELD-AREA001` tokens (3+ digits).
fn extract_keld_codes(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 5 < bytes.len() {
        if &bytes[i..i + 5] != b"KELD-" {
            i += 1;
            continue;
        }
        let mut j = i + 5;
        while j < bytes.len() && bytes[j].is_ascii_uppercase() {
            j += 1;
        }
        if j == i + 5 {
            i += 1;
            continue;
        }
        if j < bytes.len() && bytes[j] == b'-' {
            j += 1;
        }
        let digit_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j.saturating_sub(digit_start) >= 3 {
            out.push(text[i..j].to_owned());
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

fn parse_registry(text: &str) -> Result<Vec<RegistryEntry>, String> {
    let mut entries = Vec::new();
    let mut current: Option<String> = None;
    let mut crate_name: Option<String> = None;
    let mut message: Option<String> = None;
    let mut fix: Option<String> = None;

    let flush = |current: &mut Option<String>,
                 crate_name: &mut Option<String>,
                 message: &mut Option<String>,
                 fix: &mut Option<String>,
                 entries: &mut Vec<RegistryEntry>|
     -> Result<(), String> {
        let Some(code) = current.take() else {
            return Ok(());
        };
        let crate_name = crate_name.take().unwrap_or_default();
        let message = message.take().unwrap_or_default();
        let fix = fix.take().unwrap_or_default();
        if crate_name.is_empty() {
            return Err(format!("{code}: missing `- crate:`"));
        }
        if message.is_empty() {
            return Err(format!("{code}: missing `- message:`"));
        }
        if fix.is_empty() {
            return Err(format!(
                "{code}: missing `- fix:` (arch/07 §2 requires a fix)"
            ));
        }
        entries.push(RegistryEntry {
            code,
            crate_name,
            message,
            fix,
        });
        Ok(())
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("## ") {
            let heading = rest.trim();
            if heading.starts_with("KELD-") {
                flush(
                    &mut current,
                    &mut crate_name,
                    &mut message,
                    &mut fix,
                    &mut entries,
                )?;
                let extracted = extract_keld_codes(heading);
                if extracted.len() != 1 || extracted[0] != heading {
                    return Err(format!(
                        "heading `{heading}` is not a single KELD-* code (hyphenated or compact)"
                    ));
                }
                current = Some(heading.to_owned());
                crate_name = None;
                message = None;
                fix = None;
            }
            continue;
        }
        if current.is_none() {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("- crate:") {
            crate_name = Some(value.trim().to_owned());
        } else if let Some(value) = trimmed.strip_prefix("- message:") {
            message = Some(value.trim().to_owned());
        } else if let Some(value) = trimmed.strip_prefix("- fix:") {
            fix = Some(value.trim().to_owned());
        }
    }
    flush(
        &mut current,
        &mut crate_name,
        &mut message,
        &mut fix,
        &mut entries,
    )?;

    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (idx, entry) in entries.iter().enumerate() {
        if let Some(first) = seen.insert(entry.code.as_str(), idx) {
            return Err(format!(
                "duplicate code {} (first at entry {}, again at {})",
                entry.code,
                first + 1,
                idx + 1
            ));
        }
    }
    Ok(entries)
}

fn collect_source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("dirent {}: {e}", dir.display()))
            .path();
        if path.is_dir() {
            collect_source_files(&path, out);
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("rs" | "ts")) {
            out.push(path);
        }
    }
}

fn emitted_codes(root: &Path) -> BTreeSet<String> {
    let mut files = Vec::new();
    for rel in SCAN_REL {
        let dir = root.join(rel);
        assert!(
            dir.is_dir(),
            "scan root missing (workspace layout changed): {}",
            dir.display()
        );
        collect_source_files(&dir, &mut files);
    }
    assert!(
        !files.is_empty(),
        "scanner found no .rs/.ts files under {SCAN_REL:?}"
    );
    let mut codes = BTreeSet::new();
    for path in files {
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for code in extract_keld_codes(&text) {
            codes.insert(code);
        }
    }
    codes
}

#[test]
fn extract_accepts_hyphenated_and_compact_forms() {
    let text = "see KELD-CLI-020 and KELD-MCP001 then KELD-GUARD001 end";
    assert_eq!(
        extract_keld_codes(text),
        vec![
            "KELD-CLI-020".to_owned(),
            "KELD-MCP001".to_owned(),
            "KELD-GUARD001".to_owned()
        ]
    );
}

#[test]
fn extract_ignores_kel_issues_and_short_tails() {
    let text = "KEL-26 KELD-FOO KELD-CLI- KELD-X12 tracking";
    assert!(
        extract_keld_codes(text).is_empty(),
        "must not treat issue ids or incomplete tokens as codes: {:?}",
        extract_keld_codes(text)
    );
}

#[test]
fn duplicate_code_in_registry_text_is_an_error() {
    let text = "\
## KELD-IPC-001
- crate: keld-ipc
- message: a
- fix: do a

## KELD-IPC-001
- crate: keld-ipc
- message: b
- fix: do b
";
    let err = parse_registry(text).expect_err("duplicates must fail");
    assert!(err.contains("duplicate"), "{err}");
    assert!(err.contains("KELD-IPC-001"), "{err}");
}

#[test]
fn empty_fix_in_registry_text_is_an_error() {
    let text = "\
## KELD-IPC-001
- crate: keld-ipc
- message: a
";
    let err = parse_registry(text).expect_err("missing fix must fail");
    assert!(err.contains("fix"), "{err}");
}

#[test]
fn empty_registry_with_no_codes_parses_empty() {
    let entries = parse_registry("# title\n\n## Adding a code\n\nNot a code.\n")
        .expect("prose-only file is parseable");
    assert!(
        entries.is_empty(),
        "prose headings must not become codes: {entries:?}"
    );
}

#[test]
fn registry_file_exists_and_is_unique_with_fixes() {
    let path = registry_path();
    assert!(
        path.is_file(),
        "canonical registry missing: {} (KEL-35)",
        path.display()
    );
    let text = fs::read_to_string(&path).expect("read registry");
    let entries = parse_registry(&text).unwrap_or_else(|e| panic!("registry parse: {e}"));
    assert!(
        !entries.is_empty(),
        "{} must list at least one KELD-* code",
        path.display()
    );
    for entry in &entries {
        assert!(
            entry.fix.contains(' ') || entry.fix.len() >= 8,
            "{} fix is too thin (arch/07 §2): {:?}",
            entry.code,
            entry.fix
        );
        assert_ne!(
            entry.message, entry.fix,
            "{} message and fix must not be the same string",
            entry.code
        );
    }
}

#[test]
fn scanned_source_codes_match_registry() {
    let root = workspace_root();
    let text = fs::read_to_string(registry_path()).expect("read registry");
    let entries = parse_registry(&text).unwrap_or_else(|e| panic!("registry parse: {e}"));
    let registered: BTreeSet<String> = entries.iter().map(|e| e.code.clone()).collect();
    let emitted = emitted_codes(&root);

    let missing: Vec<&String> = emitted.difference(&registered).collect();
    assert!(
        missing.is_empty(),
        "codes emitted in scanned crates/tools but missing from {REGISTRY_REL}: {missing:?}"
    );

    let extra: Vec<&String> = registered.difference(&emitted).collect();
    assert!(
        extra.is_empty(),
        "codes in {REGISTRY_REL} not emitted in scanned crates/tools (remove or emit them): {extra:?}"
    );
}
