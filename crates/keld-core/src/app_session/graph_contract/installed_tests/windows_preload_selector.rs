//! Canonical retained-root intake with the ordinary spelling emitted by Bun.
//! These tests read real sealed fixture files; no app or preload JS is evaluated.

use super::{Fixture, PRELOAD, PRELOAD_BYTES};
use std::fs;
use std::path::{Component, Path, PathBuf, Prefix};

fn canonical_fixture() -> (Fixture, PathBuf, Option<PathBuf>) {
    let mut fixture = Fixture::new();
    // The runner's TEMP spelling can contain a short-name alias (RUNNER~1).
    // That alias is not the same canonical source component and must not be
    // promoted into a positive preload selector. Use only the independently
    // OS-resolved test-owned root, then strip its verbatim prefix to model the
    // ordinary Bun spelling of that *canonical* selected member.
    let initial = fixture.root.join(PRELOAD);
    assert!(matches!(
        initial.components().next(),
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
    let canonical_text = canonical.to_str().expect("canonical UTF8");
    let ordinary = PathBuf::from(
        canonical_text
            .strip_prefix(r"\\?\")
            .expect("canonical file has a disk-path spelling"),
    );
    assert!(matches!(
        ordinary.components().next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_))
    ));
    assert_eq!(
        canonical_text,
        format!(r"\\?\{}", ordinary.to_str().expect("ordinary UTF8")),
        "the two qualified namespace spellings differ only by the verbatim prefix"
    );
    // Keep the original TEMP spelling as a hostile alias if this Windows
    // host exposes one. Do not count a canonical TEMP path as an alias test.
    let original_alias = (initial != ordinary).then_some(initial);
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        let alias = original_alias
            .as_ref()
            .expect("GitHub Windows runner must expose its original TEMP short-name spelling");
        assert!(
            alias
                .components()
                .any(|component| { component.as_os_str().to_string_lossy().contains('~') }),
            "GitHub Windows TEMP must provide an observable 8.3 short-name component: {alias:?}"
        );
    }
    (fixture, ordinary, original_alias)
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
    let (fixture, ordinary, original_alias) = canonical_fixture();
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
    if let Some(alias) = original_alias {
        assert_eq!(
            fs::canonicalize(&alias).expect("original TEMP alias names a real file"),
            canonical,
            "the rejected original TEMP spelling must name this same sealed file"
        );
        assert_selector_refused(&graph, &alias, "original TEMP alias (e.g. RUNNER~1)");
    } else {
        eprintln!("original TEMP path is canonical; no short-name alias refusal was exercised");
    }
    drop(graph);
    close_fixture(fixture);
}

#[test]
fn ordinary_selected_spelling_still_rechecks_identity_and_bytes() {
    for mutation in ["changed bytes", "identical replacement", "hardlink alias"] {
        let (fixture, ordinary, _original_alias) = canonical_fixture();
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
