//! The owner and fixture censuses (gh532 AC10, gh566 C2 and D10 rules 1–6). They are
//! split from the owner under the gh566 D1 review condition (the owner passed 1,500
//! lines). The census has its own invariant: exactly one corpus owner exists among
//! keld-compat's test sources, every committed corpus is registered, and no corpus
//! directory holds its own doc-snapshot copy (gh566 D5 A5). Every pattern
//! is assembled with `concat!`, so this file never matches itself. It holds no test
//! function, and it needs the owner module `corpus_manifest` beside it at the crate root.
// Each including target uses a different subset of this module (keld-ipc precedent).
#![allow(dead_code)]
// Typed rejections come from the owner's `CorpusError` (see its module note).
#![allow(clippy::result_large_err)]

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::corpus_manifest::{
    CorpusError, MANIFEST_FILE, OWNER_PATH, Registration, SNAPSHOT_DIR, collect_files, crate_root,
    io_error, join_rel, workspace_root,
};

/// Directories under `crates/keld-compat/fixtures/` that hold a `corpus.json`.
pub fn committed_corpus_dirs() -> Result<Vec<String>, CorpusError> {
    let fixtures = crate_root().join("fixtures");
    let entries = fs::read_dir(&fixtures).map_err(io_error(&fixtures))?;
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_error(&fixtures))?;
        if entry.path().join(MANIFEST_FILE).is_file() {
            dirs.push(format!("fixtures/{}", entry.file_name().to_string_lossy()));
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// Fixture census (C2): the committed corpus directories equal the registry's, and
/// corpus ids and directories are each unique.
pub fn fixture_census(found: &[String], registry: &[Registration]) -> Result<(), CorpusError> {
    let fail = |detail: String| Err(CorpusError::FixtureCensus { detail });
    for (index, reg) in registry.iter().enumerate() {
        if registry[..index]
            .iter()
            .any(|other| other.corpus_id == reg.corpus_id)
        {
            return fail(format!("corpus id {} is registered twice", reg.corpus_id));
        }
        if registry[..index]
            .iter()
            .any(|other| other.fixture_dir == reg.fixture_dir)
        {
            return fail(format!(
                "fixture dir {} is registered twice",
                reg.fixture_dir
            ));
        }
        if !found.iter().any(|dir| dir == reg.fixture_dir) {
            return fail(format!(
                "{} is registered at {}, which holds no committed corpus",
                reg.corpus_id, reg.fixture_dir
            ));
        }
    }
    for dir in found {
        if !registry.iter().any(|reg| reg.fixture_dir == dir) {
            return fail(format!(
                "{dir} holds a corpus that no registration validates"
            ));
        }
    }
    Ok(())
}

/// Snapshot-store census (gh566 D5 A5): a cited page is committed once, in the shared
/// store, so no committed corpus directory holds its own `doc-snapshots/`. Lists every
/// offending directory. `has_local` answers for one corpus directory, relative to
/// `crates/keld-compat`; [`has_local_snapshots`] is the filesystem answer.
pub fn snapshot_store_census(
    found: &[String],
    has_local: &dyn Fn(&str) -> bool,
) -> Result<(), CorpusError> {
    let dirs: Vec<String> = found.iter().filter(|dir| has_local(dir)).cloned().collect();
    if dirs.is_empty() {
        Ok(())
    } else {
        Err(CorpusError::CorpusLocalSnapshot { dirs })
    }
}

/// Whether the corpus directory `dir` (`/`-separated, relative to `root`) holds a
/// `doc-snapshots` entry of its own.
pub fn has_local_snapshots(root: &Path, dir: &str) -> bool {
    join_rel(root, dir).join(SNAPSHOT_DIR).exists()
}

/// keld-compat test sources as (`/`-separated path relative to the crate, text).
pub type Sources = Vec<(String, String)>;

/// Reads every `tests/**/*.rs` file of keld-compat.
pub fn load_test_sources() -> Result<Sources, CorpusError> {
    let root = crate_root();
    let mut files = Vec::new();
    collect_files(&root.join("tests"), "tests", &mut files)?;
    let mut sources: Sources = files
        .into_iter()
        .filter(|(path, _)| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "rs")
        })
        .map(|(path, bytes)| (path, String::from_utf8_lossy(&bytes).into_owned()))
        .collect();
    sources.sort();
    Ok(sources)
}

/// `sha2` dependency kinds of keld-compat from `cargo metadata` (census rule 5).
pub fn sha2_dependency_kinds() -> Result<Vec<Option<String>>, CorpusError> {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(workspace_root())
        .output()
        .map_err(|error| CorpusError::RunnerFailed {
            target: "cargo metadata".to_owned(),
            detail: error.to_string(),
        })?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|error| CorpusError::RunnerFailed {
            target: "cargo metadata".to_owned(),
            detail: error.to_string(),
        })?;
    let kinds = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|package| package["name"] == "keld-compat")
        .flat_map(|package| package["dependencies"].as_array().into_iter().flatten())
        .filter(|dependency| dependency["name"] == "sha2")
        .map(|dependency| dependency["kind"].as_str().map(str::to_owned))
        .collect();
    Ok(kinds)
}

/// The frozen v0 report, the one file outside the owner that may count fails itself
/// (gh566 C9). The exemption ends when KEL-237 re-records the lifecycle corpus.
const FROZEN_REPORT: &str = "tests/lifecycle_evidence_report.rs";

/// Field names that mark a manifest parser (census rule 2).
const MANIFEST_FIELDS: &[&str] = &[
    "corpus_id",
    "cells",
    "upstream",
    "oracle_id",
    "expected_verdict",
    "test_path",
];

/// The owner census (gh532 AC10, gh566 D10 rules 1–6). Patterns are assembled with
/// `concat!` so this file's own text matches only its real definitions.
pub fn owner_census(
    sources: &Sources,
    lib_rs: &str,
    sha2_kinds: &[Option<String>],
) -> Result<(), CorpusError> {
    let violation = |rule: u8, file: &str, line: usize, detail: String| {
        Err(CorpusError::CensusViolation {
            rule,
            file: file.to_owned(),
            line,
            detail,
        })
    };
    let forbidden = [
        concat!("fn sha", "256_uri"),
        concat!("Sha", "256"),
        concat!("corpus", ".json\""),
        concat!("denominator", ".json\""),
    ];
    let test_attribute = concat!("#[", "test]");
    // Rule 6 (gh566 C9): fail counts and their labels render only through FailSplit.
    // The fail-count method in any call form: a method call, a fully qualified call
    // through the `Scoreboard` path, or that path passed to `map`.
    let report_tokens = [
        concat!("Pending ", "implementation"),
        concat!("Intentional ", "divergence"),
        concat!(".fail", "ed("),
        concat!("::fail", "ed"),
    ];
    let mut owner_seen = false;
    for (path, text) in sources {
        if path.starts_with("tests/support/") && text.contains(test_attribute) {
            return violation(
                3,
                path,
                0,
                "a support module holds a test, which would run in every including target"
                    .to_owned(),
            );
        }
        if path == OWNER_PATH {
            owner_seen = true;
            census_owner(path, text)?;
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            if let Some(token) = forbidden.iter().find(|token| line.contains(**token)) {
                return violation(1, path, index + 1, format!("`{token}` outside the owner"));
            }
        }
        if let Some((line, field)) = deserialize_manifest_field(text) {
            return violation(
                2,
                path,
                line,
                format!("a Deserialize struct declares manifest field `{field}`"),
            );
        }
        if path != FROZEN_REPORT
            && let Some((index, token)) = text.lines().enumerate().find_map(|(index, line)| {
                report_tokens
                    .iter()
                    .find(|token| line.contains(**token))
                    .map(|token| (index, *token))
            })
        {
            return Err(CorpusError::ReportBypassesFailSplit {
                file: path.clone(),
                line: index + 1,
                detail: format!("`{token}` outside FailSplit"),
            });
        }
    }
    if !owner_seen {
        return violation(3, OWNER_PATH, 0, "the owner is missing".to_owned());
    }
    let public: Vec<&str> = lib_rs
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub mod ") || line.starts_with("pub use "))
        .collect();
    if public != ["pub mod evidence;"] {
        return violation(
            4,
            "src/lib.rs",
            0,
            format!("public items {public:?}; only `pub mod evidence;` is allowed"),
        );
    }
    if sha2_kinds != [Some("dev".to_owned())] {
        return violation(
            5,
            "Cargo.toml",
            0,
            format!("sha2 dependency kinds {sha2_kinds:?}; it must be a dev-dependency only"),
        );
    }
    Ok(())
}

/// Census rule 3: inside the owner every definition appears exactly once.
fn census_owner(path: &str, text: &str) -> Result<(), CorpusError> {
    let checks = [
        (concat!("pub fn sha", "256_uri("), 1, true),
        (concat!("Sha", "256::digest("), 1, false),
        (
            concat!("serde_json::from_slice::<", "ManifestV0>"),
            1,
            false,
        ),
        (
            concat!("serde_json::from_slice::<", "ManifestV1>"),
            1,
            false,
        ),
    ];
    for (pattern, expected, line_start) in checks {
        let count = text
            .lines()
            .filter(|line| {
                if line_start {
                    line.trim_start().starts_with(pattern)
                } else {
                    line.contains(pattern)
                }
            })
            .count();
        if count != expected {
            return Err(CorpusError::CensusViolation {
                rule: 3,
                file: path.to_owned(),
                line: 0,
                detail: format!("`{pattern}` appears {count} times; expected {expected}"),
            });
        }
    }
    Ok(())
}

/// Finds a field named in [`MANIFEST_FIELDS`] inside a struct that derives `Deserialize`.
/// A derive may span lines (rustfmt splits long ones) and may share a line with the
/// `struct` it decorates; any `pub` or `pub(...)` visibility is ignored.
fn deserialize_manifest_field(text: &str) -> Option<(usize, &'static str)> {
    let mut in_derive = false;
    let mut armed = false;
    let mut depth: i64 = 0;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if depth == 0 {
            let mut rest = trimmed;
            if in_derive || rest.starts_with("#[derive(") {
                armed |= rest.contains("Deserialize");
                if let Some((_, after)) = rest.split_once(")]") {
                    in_derive = false;
                    rest = after.trim();
                } else {
                    in_derive = true;
                    continue;
                }
            }
            if armed && rest.contains("struct ") && rest.ends_with('{') {
                depth = 1;
                armed = false;
            } else if !rest.is_empty() && !rest.starts_with("#[") && !rest.starts_with("//") {
                armed = false;
            }
            continue;
        }
        let field = strip_visibility(trimmed);
        if depth == 1
            && let Some(name) = MANIFEST_FIELDS.iter().find(|name| {
                field
                    .strip_prefix(**name)
                    .is_some_and(|rest| rest.trim_start().starts_with(':'))
            })
        {
            return Some((index + 1, name));
        }
        for character in trimmed.chars() {
            match character {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
    }
    None
}

/// Drops a leading `pub` or `pub(...)` visibility from a field line.
fn strip_visibility(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix("pub(") {
        return rest
            .split_once(')')
            .map_or(line, |(_, after)| after.trim_start());
    }
    line.strip_prefix("pub ").unwrap_or(line)
}
