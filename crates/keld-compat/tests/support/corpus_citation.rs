//! Doc citations and their committed snapshots (gh532 rule 2, AC3, AC16; gh566 D5). A
//! child module of `corpus_manifest.rs`, split out under the gh566 D1 review condition.
//! Its invariant: every cited quote is a byte-exact substring of the page at the corpus
//! pin, checked offline against a committed, digest-bound snapshot.

use std::io;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use super::{Cell, CorpusError, Pin, sha256_uri, workspace_root};

/// A cell's pinned Electron docs citation (gh532 rule 2). Unknown fields are rejected,
/// so no second citation kind exists (gh532 §10 Q1).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocCitation {
    /// The docs page at the corpus commit, optionally with an anchor.
    pub url: String,
    /// The cited sentence, a byte-exact substring of the page.
    pub quote: String,
    /// `sha256:` plus the SHA-256 of the quote's UTF-8 bytes.
    pub quote_sha256: String,
}

/// Snapshot store keyed by the path relative to the fixture directory (gh566 D5).
pub type SnapshotReader<'a> = &'a dyn Fn(&str) -> io::Result<Vec<u8>>;

/// The page path a citation URL names: everything after the pinned blob prefix, up to
/// an optional anchor. It starts with `docs/`, ends in `.md`, and has no `?`, empty,
/// `.` or `..` segment, and no character outside `[A-Za-z0-9._-]` (gh566 D5).
pub fn page_path(id: &str, cell: &Cell, pin: Pin, url: &str) -> Result<String, CorpusError> {
    let reject = |reason: &str| CorpusError::CitationUrl {
        corpus_id: id.to_owned(),
        cell: cell.key.operation_id.clone(),
        url: url.to_owned(),
        reason: reason.to_owned(),
    };
    let rest = url
        .strip_prefix(&pin.doc_blob_prefix())
        .ok_or_else(|| reject("it is not a blob URL at the corpus commit"))?;
    if rest.contains('?') {
        return Err(reject("it carries a query"));
    }
    let (page, anchor) = rest
        .split_once('#')
        .map_or((rest, None), |(page, anchor)| (page, Some(anchor)));
    if !page.starts_with("docs/") {
        return Err(reject("the page is outside docs/"));
    }
    let segment_ok = |segment: &str| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    if !page.split('/').all(segment_ok) {
        return Err(reject("the page path has an empty, dot or unsafe segment"));
    }
    if Path::new(page)
        .extension()
        .is_none_or(|extension| extension != "md")
    {
        return Err(reject("the page is not a Markdown file"));
    }
    if let Some(anchor) = anchor
        && (anchor.is_empty()
            || !anchor
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')))
    {
        return Err(reject("the anchor is empty or has an unsafe character"));
    }
    Ok(page.to_owned())
}

/// Verifies every citation in the gh532 rule 2 order: entry, file, page digest, quote
/// substring, then quote digest. Each page is read and digest-checked once. Every
/// `doc_snapshots` key must be cited (gh566 D5).
pub fn verify(
    id: &str,
    pin: Pin,
    cells: &[Cell],
    doc_snapshots: &[(String, String)],
    read_snapshot: SnapshotReader<'_>,
) -> Result<(), CorpusError> {
    let mut pages: Vec<(String, Vec<u8>)> = Vec::new();
    for cell in cells {
        let Some(citation) = &cell.citation else {
            continue;
        };
        let page = page_path(id, cell, pin, &citation.url)?;
        if citation.quote.is_empty() {
            return Err(CorpusError::CitationUrl {
                corpus_id: id.to_owned(),
                cell: cell.key.operation_id.clone(),
                url: citation.url.clone(),
                reason: "the quote is empty".to_owned(),
            });
        }
        let declared = doc_snapshots
            .iter()
            .find(|(path, _)| *path == page)
            .map(|(_, digest)| digest.clone())
            .ok_or_else(|| CorpusError::MissingSnapshotEntry {
                corpus_id: id.to_owned(),
                cell: cell.key.operation_id.clone(),
                page: page.clone(),
            })?;
        if !pages.iter().any(|(known, _)| *known == page) {
            let rel = format!("{}{page}", pin.snapshot_dir());
            let bytes = read_snapshot(&rel).map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    CorpusError::MissingSnapshotFile {
                        corpus_id: id.to_owned(),
                        page: page.clone(),
                        path: rel.clone(),
                    }
                } else {
                    CorpusError::Io {
                        path: rel.clone(),
                        error: error.to_string(),
                    }
                }
            })?;
            let computed = sha256_uri(&bytes);
            if computed != declared {
                return Err(CorpusError::SnapshotDigestMismatch {
                    corpus_id: id.to_owned(),
                    page,
                    declared,
                    computed,
                });
            }
            pages.push((page.clone(), bytes));
        }
        let snapshot = pages
            .iter()
            .find(|(known, _)| *known == page)
            .map_or(&[][..], |(_, bytes)| bytes.as_slice());
        let quote = citation.quote.as_bytes();
        if !snapshot.windows(quote.len()).any(|window| window == quote) {
            return Err(CorpusError::QuoteAbsent {
                corpus_id: id.to_owned(),
                cell: cell.key.operation_id.clone(),
                page,
            });
        }
        let computed = sha256_uri(quote);
        if computed != citation.quote_sha256 {
            return Err(CorpusError::QuoteDigestMismatch {
                corpus_id: id.to_owned(),
                cell: cell.key.operation_id.clone(),
                declared: citation.quote_sha256.clone(),
                computed,
            });
        }
    }
    if let Some((page, _)) = doc_snapshots
        .iter()
        .find(|(path, _)| !pages.iter().any(|(known, _)| known == path))
    {
        return Err(CorpusError::UncitedSnapshotEntry {
            corpus_id: id.to_owned(),
            page: page.clone(),
        });
    }
    Ok(())
}

/// Requires that Git store `file` byte-for-byte at `repo_rel`. A CRLF page matches its
/// digest in the working tree, but Git normalises it on commit, so CI would read other
/// bytes (gh566 D5, F12). Compares `git hash-object --path` (attribute filters applied)
/// with `git hash-object --no-filters`.
pub fn check_normalisation(repo_rel: &str, file: &Path) -> Result<(), CorpusError> {
    let hash = |filters: &[&str]| -> Result<String, CorpusError> {
        let output = Command::new("git")
            .arg("hash-object")
            .args(filters)
            .arg(file)
            .current_dir(workspace_root())
            .output()
            .map_err(|error| CorpusError::RunnerFailed {
                target: "git hash-object".to_owned(),
                detail: format!("cannot run git: {error}"),
            })?;
        if !output.status.success() {
            return Err(CorpusError::RunnerFailed {
                target: "git hash-object".to_owned(),
                detail: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let stored = hash(&[&format!("--path={repo_rel}")])?;
    let raw = hash(&["--no-filters"])?;
    if stored != raw {
        return Err(CorpusError::SnapshotWouldBeNormalised {
            page: repo_rel.to_owned(),
            path: file.display().to_string(),
        });
    }
    Ok(())
}
