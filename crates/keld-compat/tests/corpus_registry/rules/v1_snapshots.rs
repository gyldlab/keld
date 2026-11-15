//! Snapshot rules over an in-memory store (gh532 AC16, rule 2 order; gh566 D5), plus the
//! Git normalisation check over one temporary file and the owner's store reader over a
//! temporary tree (gh566 C11, A5).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::v1_fixture::{
    COMMIT, PAGE, READY, SNAPSHOT, V0_COMMIT, V1, bytes, denominator, edit, manifest, parse_with,
    store,
};
use crate::corpus_manifest::{
    Corpus, CorpusError, SNAPSHOT_DIR, check_checkout_attributes, check_normalisation, join_rel,
    read_store_snapshot, sha256_uri, snapshot_repo_path, workspace_root,
};

/// gh532 AC16: a fabricated quote is rejected even with its own correct digest, and the
/// rejection names the cell and the page. The substring check runs before the quote
/// digest, so a fabricated quote with a wrong digest is still `QuoteAbsent`.
#[test]
fn snapshot_quote_must_be_in_the_pinned_page() {
    let fabricated = "Returns `Promise<void>` - fulfilled when Electron is ready.";
    let correct_digest = edit(|m| {
        m["cells"][0]["doc_citation"]["quote"] = json!(fabricated);
        m["cells"][0]["doc_citation"]["quote_sha256"] = json!(sha256_uri(fabricated.as_bytes()));
    });
    let error = parse_with(&V1, &correct_digest, &store()).expect_err("fabricated quote");
    assert!(
        matches!(&error, CorpusError::QuoteAbsent { cell, page, .. } if cell == "app.when-ready" && page == PAGE),
        "{error}"
    );
    let wrong_digest = edit(|m| m["cells"][0]["doc_citation"]["quote"] = json!(fabricated));
    assert!(matches!(
        parse_with(&V1, &wrong_digest, &store()),
        Err(CorpusError::QuoteAbsent { .. })
    ));
    assert!(SNAPSHOT.contains(READY), "the base quote is in the page");
}

/// gh532 AC16: the snapshot must match its declared digest, have a `doc_snapshots`
/// entry and a file under the pinned commit; an uncited entry is rejected.
#[test]
fn snapshot_entry_file_and_digest_bind_the_page() {
    let base = manifest();
    let mut tampered = store();
    tampered[0].1.extend_from_slice(b"appended\n");
    assert!(matches!(
        parse_with(&V1, &base, &tampered),
        Err(CorpusError::SnapshotDigestMismatch { .. })
    ));

    let no_entry = edit(|m| m["doc_snapshots"] = json!({}));
    assert!(matches!(
        parse_with(&V1, &no_entry, &store()),
        Err(CorpusError::MissingSnapshotEntry { .. })
    ));
    assert!(matches!(
        parse_with(&V1, &base, &[]),
        Err(CorpusError::MissingSnapshotFile { .. })
    ));
    let other_commit = vec![(
        format!("doc-snapshots/{V0_COMMIT}/{PAGE}"),
        SNAPSHOT.as_bytes().to_vec(),
    )];
    assert!(matches!(
        parse_with(&V1, &base, &other_commit),
        Err(CorpusError::MissingSnapshotFile { .. })
    ));
    let uncited =
        edit(|m| m["doc_snapshots"]["docs/api/browser-window.md"] = json!(sha256_uri(b"x")));
    assert!(matches!(
        parse_with(&V1, &uncited, &store()),
        Err(CorpusError::UncitedSnapshotEntry { .. })
    ));
    let expected_path = format!("doc-snapshots/{COMMIT}/{PAGE}");
    assert_eq!(
        store()[0].0,
        expected_path,
        "snapshots live under the pinned commit"
    );
}

/// Removes its directory when the test ends, pass or fail.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// gh566 C11 (A5): the owner's store reader reads a cited page only from the shared
/// store. A page filed only under a corpus directory's `doc-snapshots/` is never read,
/// so its citation fails with `MissingSnapshotFile`. The same bytes in the store parse.
#[test]
fn corpus_local_page_is_not_read_by_the_store_reader() {
    let root = TempDir(
        std::env::temp_dir().join(format!("keld-compat-store-reader-{}", std::process::id())),
    );
    let rel = format!("{SNAPSHOT_DIR}/{COMMIT}/{PAGE}");
    let base = manifest();
    let manifest_bytes = bytes(&base);
    let denominator_bytes = denominator(&manifest_bytes, &base);
    let parse_from = |workspace: &Path| {
        Corpus::parse_with_snapshots(&V1, &manifest_bytes, &denominator_bytes, &|key: &str| {
            read_store_snapshot(workspace, key)
        })
    };

    let local = join_rel(
        &root.0,
        &format!("crates/keld-compat/{}/{rel}", V1.fixture_dir),
    );
    fs::create_dir_all(local.parent().expect("corpus-local parent")).expect("create local dir");
    fs::write(&local, SNAPSHOT).expect("write corpus-local page");
    match parse_from(&root.0) {
        Err(CorpusError::MissingSnapshotFile { page, .. }) => assert_eq!(page, PAGE),
        other => panic!("a corpus-local page must not satisfy a citation: {other:?}"),
    }

    let stored = join_rel(&root.0, &snapshot_repo_path(&rel));
    fs::create_dir_all(stored.parent().expect("store parent")).expect("create store dir");
    fs::write(&stored, SNAPSHOT).expect("write store page");
    parse_from(&root.0).unwrap_or_else(|error| panic!("the store page must parse: {error}"));
}

/// gh566 D5: Git must store a snapshot byte-for-byte. CRLF bytes would be normalised on
/// commit, so they are rejected; LF bytes and a lone CR are stored unchanged.
#[test]
fn snapshot_would_be_normalised_rejects_crlf() {
    let dir =
        TempDir(std::env::temp_dir().join(format!("keld-compat-snapshot-{}", std::process::id())));
    fs::create_dir_all(&dir.0).expect("create temp dir");
    let repo_rel = snapshot_repo_path(&format!("{SNAPSHOT_DIR}/{COMMIT}/{PAGE}"));
    for (label, bytes, normalised) in [
        ("crlf", &b"line one\r\nline two\r\n"[..], true),
        ("lf", &b"line one\nline two\n"[..], false),
        ("lone cr", &b"a\rb\n"[..], false),
    ] {
        let file = dir.0.join(format!("{label}.md"));
        fs::write(&file, bytes).expect("write snapshot");
        let result = check_normalisation(&repo_rel, &file);
        if normalised {
            assert!(
                matches!(result, Err(CorpusError::SnapshotWouldBeNormalised { .. })),
                "{label}: {result:?}"
            );
        } else {
            result.unwrap_or_else(|error| panic!("{label}: {error}"));
        }
    }
}

/// gh566 D5 (checkout): a snapshot path whose attributes transform bytes at checkout
/// is rejected, so a clone never checks out other bytes than the committed blob. The
/// controls run in a throwaway Git repository with one `.gitattributes` each.
#[test]
fn snapshot_checkout_attributes_reject_transforming_filters() {
    // The repository's own attributes on the real store path (gh566 D5 A5).
    let page = snapshot_repo_path(&format!("{SNAPSHOT_DIR}/{COMMIT}/{PAGE}"));
    check_checkout_attributes(&workspace_root(), &page)
        .unwrap_or_else(|error| panic!("the repository's own attributes: {error}"));

    let dir = TempDir(
        std::env::temp_dir().join(format!("keld-compat-attributes-{}", std::process::id())),
    );
    fs::create_dir_all(&dir.0).expect("create temp repo");
    let init = std::process::Command::new("git")
        .args(["-c", "init.defaultBranch=main", "init", "-q"])
        .current_dir(&dir.0)
        .status()
        .expect("run git init");
    assert!(init.success(), "git init");
    for (attributes, rejected) in [
        ("*.md eol=crlf\n", Some("eol")),
        ("* text=auto\n*.md !eol\n", Some("eol")),
        ("* eol=lf\n*.md ident\n", Some("ident")),
        (
            "* eol=lf\n*.md working-tree-encoding=UTF-16\n",
            Some("working-tree-encoding"),
        ),
        ("* eol=lf\n*.md filter=lfs\n", Some("filter")),
        ("* text=auto eol=lf\n", None),
        ("*.md -text\n", None),
    ] {
        fs::write(dir.0.join(".gitattributes"), attributes).expect("write attributes");
        let result = check_checkout_attributes(&dir.0, PAGE);
        match rejected {
            Some(name) => assert!(
                matches!(&result, Err(CorpusError::SnapshotCheckoutFilter { attribute, .. }) if attribute == name),
                "{attributes:?}: {result:?}"
            ),
            None => result.unwrap_or_else(|error| panic!("{attributes:?}: {error}")),
        }
    }
}
