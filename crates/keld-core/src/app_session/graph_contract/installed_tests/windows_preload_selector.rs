//! Canonical retained-root intake with the ordinary spelling emitted by Bun.
//! These tests read real sealed fixture files; no app or preload JS is evaluated.

use super::{Fixture, PRELOAD, PRELOAD_BYTES};
use std::fs;
use std::path::{Component, Path, PathBuf, Prefix};

fn canonical_fixture() -> (Fixture, PathBuf) {
    let mut fixture = Fixture::new();
    // The independent caller spelling is built from the ordinary TempDir path,
    // not by calling the production prefix-conversion helper.
    let ordinary = fixture.root.join("bootstrap").join("app-preload.cjs");
    assert!(matches!(
        ordinary.components().next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_))
    ));
    fixture.root = fixture
        .root
        .canonicalize()
        .expect("actual canonical fixture root");
    assert!(matches!(
        fixture.root.components().next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::VerbatimDisk(_))
    ));
    let canonical = fixture.root.join(PRELOAD);
    assert_eq!(
        canonical.to_str().expect("canonical UTF8"),
        format!(r"\\?\{}", ordinary.to_str().expect("ordinary UTF8")),
        "the two observed namespace spellings differ only by the verbatim prefix"
    );
    (fixture, ordinary)
}

fn close_fixture(fixture: Fixture) {
    let Fixture {
        owned,
        root,
        held,
        selected: _,
    } = fixture;
    drop(held);
    owned
        .close()
        .expect("actual retained fixture handles released before cleanup");
    assert!(!root.exists(), "owned fixture removed");
}

fn assert_selector_refused(graph: &super::AdmittedStageGraph, selector: &Path, label: &str) {
    let error = graph
        .read_selected_preload_bytes(selector)
        .expect_err("an alias cannot select the sealed preload");
    assert!(
        error.to_string().contains(
            "graph admission: preload selector: not the exact sealed boot-selected preload member"
        ),
        "{label}: {error}"
    );
}

#[test]
fn canonical_root_accepts_only_exact_selected_preload_spellings() {
    let (fixture, ordinary) = canonical_fixture();
    let graph = fixture.admitted();
    let canonical = fixture.root.join(PRELOAD);
    assert_ne!(
        ordinary, canonical,
        "the pre-fix Path comparison must observe different prefix kinds"
    );
    assert_eq!(
        graph
            .read_selected_preload_bytes(&canonical)
            .expect("original canonical selected spelling"),
        PRELOAD_BYTES
    );
    assert_eq!(
        graph
            .read_selected_preload_bytes(&ordinary)
            .expect("Bun ordinary spelling of the same sealed selected member"),
        PRELOAD_BYTES
    );

    let root = ordinary
        .parent()
        .and_then(Path::parent)
        .expect("ordinary fixture root")
        .to_str()
        .expect("ordinary UTF8");
    for (label, selector) in [
        (
            "alternate member",
            format!(r"{root}\client\assets\nested\editor.html"),
        ),
        ("different preload", format!(r"{root}\bootstrap\other.cjs")),
        (
            "dot segment",
            format!(r"{root}\bootstrap\.\app-preload.cjs"),
        ),
        (
            "parent segment",
            format!(r"{root}\bootstrap\..\bootstrap\app-preload.cjs"),
        ),
        (
            "repeated separator",
            format!(r"{root}\bootstrap\\app-preload.cjs"),
        ),
        (
            "alternate separator",
            format!(r"{root}\bootstrap/app-preload.cjs"),
        ),
        (
            "trailing separator",
            format!(r"{root}\bootstrap\app-preload.cjs\"),
        ),
        (
            "trailing dot",
            format!(r"{root}\bootstrap\app-preload.cjs."),
        ),
        (
            "trailing space",
            format!(r"{root}\bootstrap\app-preload.cjs "),
        ),
        (
            "alternate stream",
            format!(r"{root}\bootstrap\app-preload.cjs:stream"),
        ),
        (
            "member case alias",
            format!(r"{root}\bootstrap\APP-PRELOAD.CJS"),
        ),
        ("device namespace", format!(r"\\.\{}", ordinary.display())),
    ] {
        assert_selector_refused(&graph, Path::new(&selector), label);
    }
    assert_selector_refused(&graph, Path::new(PRELOAD), "relative selector");
    drop(graph);
    close_fixture(fixture);
}

#[test]
fn ordinary_selected_spelling_still_rechecks_identity_and_bytes() {
    for mutation in ["changed bytes", "identical replacement", "hardlink alias"] {
        let (fixture, ordinary) = canonical_fixture();
        let graph = fixture.admitted();
        assert_eq!(
            graph
                .read_selected_preload_bytes(&ordinary)
                .expect("unchanged original selected file"),
            PRELOAD_BYTES
        );
        match mutation {
            "changed bytes" => {
                let mut changed = PRELOAD_BYTES.to_vec();
                changed[0] ^= 1;
                fs::write(&ordinary, changed).expect("change only selected fixture bytes");
            }
            "identical replacement" => {
                let replacement = fixture.owned.path().join("replacement-preload");
                fs::write(&replacement, PRELOAD_BYTES)
                    .expect("distinct file with identical selected bytes");
                fs::rename(&replacement, &ordinary).expect("replace selected file identity");
            }
            "hardlink alias" => {
                let alias = fixture.root.join("bootstrap").join("alias.cjs");
                fs::hard_link(&ordinary, &alias).expect("real untrusted hardlink alias");
                assert_selector_refused(&graph, &alias, "unselected hardlink");
            }
            _ => unreachable!(),
        }
        let error = graph
            .read_selected_preload_bytes(&ordinary)
            .expect_err("ordinary spelling must still run retained graph and identity checks");
        assert!(
            !error.to_string().contains("preload selector"),
            "the exact selector passed; the bound graph must refuse {mutation}: {error}"
        );
        drop(graph);
        close_fixture(fixture);
    }
}
