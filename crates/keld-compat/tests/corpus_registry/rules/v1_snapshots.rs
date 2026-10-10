//! Snapshot rules over an in-memory store (gh532 AC16, rule 2 order; gh566 D5), plus the
//! Git normalisation check over one temporary file.

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use super::v1_fixture::{
    COMMIT, PAGE, READY, SNAPSHOT, V0_COMMIT, V1, edit, manifest, parse_with, store,
};
use crate::corpus_manifest::{
    CorpusError, check_checkout_attributes, check_normalisation, sha256_uri, workspace_root,
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

/// gh566 D5: Git must store a snapshot byte-for-byte. CRLF bytes would be normalised on
/// commit, so they are rejected; LF bytes and a lone CR are stored unchanged.
#[test]
fn snapshot_would_be_normalised_rejects_crlf() {
    let dir =
        TempDir(std::env::temp_dir().join(format!("keld-compat-snapshot-{}", std::process::id())));
    fs::create_dir_all(&dir.0).expect("create temp dir");
    let repo_rel = format!("crates/keld-compat/fixtures/example/doc-snapshots/{COMMIT}/{PAGE}");
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
    let page = format!("crates/keld-compat/fixtures/example/doc-snapshots/{COMMIT}/{PAGE}");
    check_checkout_attributes(&workspace_root(), &page)
        .unwrap_or_else(|error| panic!("the repository's own attributes: {error}"));

    let dir = TempDir(
        std::env::temp_dir().join(format!("keld-compat-attributes-{}", std::process::id())),
    );
    fs::create_dir_all(&dir.0).expect("create temp repo");
    // No detached `git maintenance` child may race the temp-repo cleanup (#670).
    for args in [
        &["-c", "init.defaultBranch=main", "init", "-q"][..],
        &["config", "maintenance.auto", "false"],
    ] {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&dir.0)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?}");
    }
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
