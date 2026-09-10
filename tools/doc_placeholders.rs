//! Rejects unsubstituted template placeholders in checked-in prose.
//!
//! KEL-208 shipped a literal `{BS}{BS}?{BS}C:/**` into `docs/architecture/03-security.md`
//! and its generated mirror: a Python plain string where the edit needed an f-string.
//! Every existing gate passed, because each one checks *consistency* — the generated
//! corpus faithfully reproduced a source sentence that meant nothing.
//!
//! Prose does contain legitimate template slots (`notes.{channel}`), so a blanket scan
//! would be noise. This checker uses the repository's inventory idiom instead: every
//! placeholder a document may use is listed here, and an unlisted one fails. A leaked
//! generator variable is unlisted by construction, and adding a real template slot stays
//! a deliberate one-line act.

#[path = "markdown_contract.rs"]
#[allow(
    dead_code,
    reason = "shared contract module; this checker deliberately does not strip \n              inline code, because that is where the KEL-208 placeholder shipped"
)]
mod markdown_contract;

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use markdown_contract::{fence_marker, without_struck_text};

/// Directories scanned in full, recursively.
const SCANNED_DIRS: &[&str] = &["docs", ".agents"];
/// Individual files scanned at the repository root.
const SCANNED_FILES: &[&str] = &["llms.txt", "llms-full.txt", "AGENTS.md", "README.md"];
/// Skipped: a nested private checkout with its own contracts.
const SKIPPED_PREFIX: &str = "docs/research";

/// Template slots documents are allowed to write.
///
/// Sorted, deduplicated, and every entry must still appear somewhere in the scanned
/// prose — a stale entry is as much a defect as an unlisted one, because it lets the
/// next leaked variable name hide behind a name nobody uses any more.
const KNOWN_PLACEHOLDERS: &[&str] = &[
    "channel", "digest", "id", "kind", "N", "name", "owner", "panel", "passed", "path", "repo",
];

#[derive(Debug, PartialEq, Eq)]
struct Finding {
    file: String,
    line: usize,
    detail: String,
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Placeholder tokens on one line, as `(token, byte offset)`.
///
/// A token is `{` + an identifier + `}`. Anything else — an empty `{}`, a brace holding
/// punctuation, a Rust format spec — is not a template slot and is not this gate's
/// business.
fn placeholders(line: &str) -> Vec<(String, usize)> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'{' {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut end = start;
        while end < bytes.len() && is_ident_byte(bytes[end]) {
            end += 1;
        }
        if end > start && end < bytes.len() && bytes[end] == b'}' {
            found.push((line[start..end].to_owned(), index));
            index = end + 1;
        } else {
            index += 1;
        }
    }
    found
}

/// Scans one document, returning findings and the placeholders it legitimately used.
fn scan(relative: &str, text: &str, known: &BTreeSet<&str>) -> (Vec<Finding>, BTreeSet<String>) {
    let mut findings = Vec::new();
    let mut used = BTreeSet::new();
    let mut fence: Option<(u8, usize)> = None;

    for (offset, line) in text.split('\n').enumerate() {
        let number = offset + 1;
        // A fenced block is verbatim sample text: `println!("{name}")` is code, not prose.
        if let Some((marker, width, closing_tail)) = fence_marker(line) {
            match fence {
                Some((open_marker, open_width))
                    if marker == open_marker && width >= open_width && closing_tail =>
                {
                    fence = None;
                }
                None => fence = Some((marker, width)),
                _ => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }

        // Inline code is deliberately NOT stripped: the KEL-208 placeholder shipped
        // inside backticks, which is exactly where a formatting bug lands. Struck-through
        // text is stripped, because retracted prose makes no live claim.
        let visible = without_struck_text(line);
        let line = visible.as_str();
        for (token, at) in placeholders(line) {
            if known.contains(token.as_str()) {
                used.insert(token.clone());
            } else {
                findings.push(Finding {
                    file: relative.to_owned(),
                    line: number,
                    detail: format!(
                        "unknown template placeholder `{{{token}}}`. If the writing tool \
                         should have substituted it, fix the substitution; if it is a real \
                         template slot, add `\"{token}\"` to KNOWN_PLACEHOLDERS in \
                         tools/doc_placeholders.rs"
                    ),
                });
            }
            // Concatenated slots are a formatting bug even when each name is known:
            // no document writes `{a}{b}` as a template.
            if at >= 1 && line.as_bytes()[at - 1] == b'}' {
                findings.push(Finding {
                    file: relative.to_owned(),
                    line: number,
                    detail: format!(
                        "placeholder `{{{token}}}` is concatenated with the one before it. \
                         Adjacent slots are a substitution bug, not a template"
                    ),
                });
            }
        }
    }
    (findings, used)
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn collect_markdown(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("DOC-PLACEHOLDERS: cannot read `{dir:?}`: {error}"))?;
        let path = entry.path();
        if relative_path(root, &path).starts_with(SKIPPED_PREFIX) {
            continue;
        }
        if path.is_dir() {
            collect_markdown(root, &path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
    Ok(())
}

fn documents(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for dir in SCANNED_DIRS {
        collect_markdown(root, &root.join(dir), &mut files)?;
    }
    for file in SCANNED_FILES {
        let path = root.join(file);
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn check(root: &Path) -> Result<String, String> {
    let known: BTreeSet<&str> = KNOWN_PLACEHOLDERS.iter().copied().collect();
    if known.len() != KNOWN_PLACEHOLDERS.len() {
        return Err("DOC-PLACEHOLDERS: KNOWN_PLACEHOLDERS contains a duplicate entry.".to_owned());
    }

    let files = documents(root)?;
    if files.is_empty() {
        return Err(format!(
            "DOC-PLACEHOLDERS: no documents found under {root:?}. \
             Run this from the repository root."
        ));
    }

    let mut findings = Vec::new();
    let mut used = BTreeSet::new();
    for path in &files {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("DOC-PLACEHOLDERS: cannot read `{path:?}`: {error}"))?;
        let (mut file_findings, file_used) = scan(&relative_path(root, path), &text, &known);
        findings.append(&mut file_findings);
        used.extend(file_used);
    }

    if !findings.is_empty() {
        let mut report = String::new();
        for finding in &findings {
            report.push_str(&format!(
                "DOC-PLACEHOLDERS: {}:{}: {}\n",
                finding.file, finding.line, finding.detail
            ));
        }
        report.push_str(&format!("{} placeholder defect(s).", findings.len()));
        return Err(report);
    }

    let stale: Vec<&str> = KNOWN_PLACEHOLDERS
        .iter()
        .copied()
        .filter(|token| !used.contains(*token))
        .collect();
    if !stale.is_empty() {
        return Err(format!(
            "DOC-PLACEHOLDERS: KNOWN_PLACEHOLDERS lists {stale:?}, which no scanned document \
             uses. Remove the stale entry so an unlisted placeholder cannot hide behind it."
        ));
    }

    Ok(format!(
        "doc-placeholders ok: {} document(s), {} known placeholder(s) all in use",
        files.len(),
        KNOWN_PLACEHOLDERS.len()
    ))
}

fn run_cli() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => {
            let root = args.next().unwrap_or_else(|| ".".to_owned());
            let report = check(Path::new(&root))?;
            println!("{report}");
            Ok(())
        }
        other => Err(format!(
            "DOC-PLACEHOLDERS: expected `check <root>`, got {other:?}."
        )),
    }
}

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> BTreeSet<&'static str> {
        KNOWN_PLACEHOLDERS.iter().copied().collect()
    }

    /// The exact line KEL-208 shipped into the normative security spec.
    #[test]
    fn the_kel208_placeholder_line_is_rejected() {
        let line = "  `$APPDATA/cache/https:/**` and the `{BS}{BS}?{BS}C:/**` shape";
        let (findings, _) = scan("docs/architecture/03-security.md", line, &known());
        assert!(
            findings.iter().any(|f| f.detail.contains("`{BS}`")),
            "the leaked variable must be named: {findings:?}"
        );
        assert!(
            findings.iter().any(|f| f.detail.contains("concatenated")),
            "adjacent slots must be reported too: {findings:?}"
        );
    }

    /// A single stray placeholder is the same bug without the adjacency tell.
    #[test]
    fn a_lone_unknown_placeholder_is_rejected() {
        let (findings, _) = scan("docs/x.md", "the `{BS}C:/**` shape", &known());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].detail.contains("unknown template placeholder"));
    }

    /// Documented template slots must not be flagged, or the gate gets switched off.
    #[test]
    fn documented_template_slots_are_accepted() {
        let (findings, used) = scan(
            "docs/architecture/02-ipc.md",
            "Channel `notes.{channel}` maps to `{name}` for run `{id}`.",
            &known(),
        );
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(used.len(), 3);
    }

    /// Concatenation is a substitution bug even when both names are legitimate.
    #[test]
    fn adjacent_known_placeholders_are_still_rejected() {
        let (findings, _) = scan("docs/x.md", "path `{name}{id}` here", &known());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].detail.contains("concatenated"));
    }

    /// Fenced blocks are verbatim samples; Rust format strings live there.
    #[test]
    fn fenced_code_is_sample_text_not_prose() {
        let text = "before\n```rust\nprintln!(\"{unknown_variable}\");\n```\nafter";
        let (findings, _) = scan("docs/x.md", text, &known());
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// Inline code is where the KEL-208 defect landed, so it is scanned, not stripped.
    #[test]
    fn inline_code_is_scanned_because_that_is_where_the_defect_landed() {
        let (findings, _) = scan("docs/x.md", "see `{BS}` there", &known());
        assert_eq!(findings.len(), 1, "inline code must not be exempt: {findings:?}");
    }

    /// `{}` and format specs are not template slots and are none of this gate's business.
    #[test]
    fn non_identifier_braces_are_ignored() {
        let (findings, _) = scan("docs/x.md", "use {} and {:?} and { spaced } freely", &known());
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn placeholders_reports_token_and_offset() {
        assert_eq!(
            placeholders("a {one} b {two}"),
            vec![("one".to_owned(), 2), ("two".to_owned(), 10)]
        );
        assert_eq!(placeholders("{} {:?} {a-b}"), Vec::new());
    }

    #[test]
    fn known_placeholders_is_sorted_and_unique() {
        let mut sorted = KNOWN_PLACEHOLDERS.to_vec();
        sorted.sort_by_key(|token| token.to_ascii_lowercase());
        sorted.dedup();
        let mut actual = KNOWN_PLACEHOLDERS.to_vec();
        actual.dedup();
        assert_eq!(actual.len(), KNOWN_PLACEHOLDERS.len(), "duplicate entry");
        assert_eq!(sorted, KNOWN_PLACEHOLDERS.to_vec(), "keep the list sorted");
    }
}
