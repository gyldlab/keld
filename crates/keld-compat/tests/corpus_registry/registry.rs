//! Committed corpora: the fixture census (C2), static rules and records, admission of
//! every registered target, and the owner census (gh532 AC10).

use std::fs;

use crate::corpus_admission::{admit_bun, admit_libtest};
use crate::corpus_census::{
    Sources, committed_corpus_dirs, fixture_census, load_test_sources, owner_census,
    sha2_dependency_kinds,
};
use crate::corpus_manifest::{
    Corpus, CorpusError, LIFECYCLE_V0, OWNER_PATH, REGISTRY, Registration, Runner, committed_runs,
    crate_root, host_platform, validate_committed_runs,
};
use keld_compat::evidence::CellKey;

/// Loads every registered corpus.
fn registered() -> Vec<Corpus> {
    REGISTRY
        .iter()
        .map(|reg| Corpus::load(reg).unwrap_or_else(|error| panic!("{}: {error}", reg.corpus_id)))
        .collect()
}

/// gh566 C2: every committed corpus is registered once, and every registration exists.
#[test]
fn every_committed_corpus_is_registered() {
    let found = committed_corpus_dirs().unwrap_or_else(|error| panic!("{error}"));
    assert!(
        found.iter().any(|dir| dir == LIFECYCLE_V0.fixture_dir),
        "the lifecycle corpus is committed"
    );
    fixture_census(&found, REGISTRY).unwrap_or_else(|error| panic!("{error}"));

    let census_detail =
        |dirs: &[String], registry: &[Registration]| match fixture_census(dirs, registry) {
            Err(CorpusError::FixtureCensus { detail }) => detail,
            other => panic!(
                "census must reject {dirs:?} ({} registrations): {other:?}",
                registry.len()
            ),
        };

    let mut unregistered = found.clone();
    unregistered.push("fixtures/unregistered-corpus".to_owned());
    assert!(census_detail(&unregistered, REGISTRY).contains("no registration validates"));
    assert!(census_detail(&[], REGISTRY).contains("holds no committed corpus"));
    let renamed = Registration {
        fixture_dir: "fixtures/elsewhere",
        ..LIFECYCLE_V0
    };
    assert!(census_detail(&found, &[renamed]).contains("holds no committed corpus"));

    // Each duplicate is isolated: the shared id keeps distinct directories, and the
    // shared directory keeps distinct ids, so each control reaches its own check.
    let mut two_dirs = found.clone();
    two_dirs.push("fixtures/other-corpus".to_owned());
    let same_id = [
        LIFECYCLE_V0,
        Registration {
            fixture_dir: "fixtures/other-corpus",
            ..LIFECYCLE_V0
        },
    ];
    assert!(
        census_detail(&two_dirs, &same_id)
            .contains("corpus id electron-lifecycle-v0 is registered twice")
    );
    let same_dir = [
        LIFECYCLE_V0,
        Registration {
            corpus_id: "other",
            ..LIFECYCLE_V0
        },
    ];
    assert!(
        census_detail(&found, &same_dir)
            .contains("fixture dir fixtures/lifecycle-corpus is registered twice")
    );
}

/// Every registered corpus passes the static rules, and its committed records pass the
/// harness-run rules, one `(platform, arch)` run at a time.
#[test]
fn registered_corpora_validate_with_their_records() {
    let mut lifecycle_runs = 0;
    for (reg, corpus) in REGISTRY.iter().zip(registered()) {
        let runs = committed_runs(reg).unwrap_or_else(|error| panic!("{error}"));
        let validated = validate_committed_runs(&corpus, &runs)
            .unwrap_or_else(|error| panic!("{}: {error}", reg.corpus_id));
        if reg.corpus_id == LIFECYCLE_V0.corpus_id {
            lifecycle_runs += validated;
        }
    }
    // Non-vacuity is pinned to the frozen corpus only, so a new registration needs no
    // edit here (gh566 §4.4).
    assert_eq!(
        lifecycle_runs, 3,
        "the lifecycle corpus publishes three platform runs"
    );
}

/// Runs each distinct registered libtest target once and admits every mapped cell.
#[test]
fn registered_corpora_libtest_oracles_execute() {
    let corpora = registered();
    let refs: Vec<&Corpus> = corpora.iter().collect();
    let report = admit_libtest(&refs).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(report.unknown, undeclared_on_host(&corpora, true));
}

/// Runs each distinct registered Bun file once and admits every mapped cell.
#[test]
fn registered_corpora_bun_oracles_execute() {
    let corpora = registered();
    let refs: Vec<&Corpus> = corpora.iter().collect();
    let report = admit_bun(&refs).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(report.unknown, undeclared_on_host(&corpora, false));
}

/// The cells of one runner kind (libtest when `rust`) whose platforms exclude this host:
/// the admission report must list exactly these as `unknown` (gh566 D13).
fn undeclared_on_host(corpora: &[Corpus], rust: bool) -> Vec<(String, CellKey)> {
    corpora
        .iter()
        .flat_map(|corpus| {
            corpus.cells().iter().filter_map(move |cell| {
                let target = corpus
                    .registration()
                    .targets
                    .iter()
                    .find(|target| target.path == cell.test_path)?;
                let is_rust = matches!(target.runner, Runner::Libtest { .. });
                (is_rust == rust && !cell.platforms.contains(&host_platform()))
                    .then(|| (corpus.id().to_owned(), cell.key.clone()))
            })
        })
        .collect()
}

fn lib_rs() -> String {
    fs::read_to_string(crate_root().join("src").join("lib.rs")).expect("read src/lib.rs")
}

fn with_file(sources: &Sources, path: &str, text: &str) -> Sources {
    let mut sources = sources.clone();
    sources.push((path.to_owned(), text.to_owned()));
    sources
}

fn assert_rule(result: Result<(), CorpusError>, rule: u8) {
    match result {
        Err(CorpusError::CensusViolation { rule: found, .. }) if found == rule => {}
        other => panic!("expected census rule {rule} to fail, got {other:?}"),
    }
}

/// gh532 AC10 (gh566 D10 rules 1–5): one manifest parser and one digest helper, both in
/// the owner; `src/` exports no new module; `sha2` stays a dev-dependency. Each rule
/// has a synthetic negative input, built with `concat!` so no scanned file carries it.
#[test]
fn owner_census_finds_one_parser_and_one_digest_helper() {
    let sources = load_test_sources().unwrap_or_else(|error| panic!("{error}"));
    let lib = lib_rs();
    let kinds = sha2_dependency_kinds().unwrap_or_else(|error| panic!("{error}"));
    owner_census(&sources, &lib, &kinds).unwrap_or_else(|error| panic!("{error}"));

    let second_digest = with_file(
        &sources,
        "tests/extra.rs",
        concat!(
            "pub fn sha",
            "256_uri(bytes: &[u8]) -> String { String::new() }\n"
        ),
    );
    assert_rule(owner_census(&second_digest, &lib, &kinds), 1);

    let second_parser = with_file(
        &sources,
        "tests/extra.rs",
        concat!(
            "#[derive(Deserialize)]\n#[serde(deny_unknown_fields)]\nstruct Other {\n",
            "    corpus",
            "_id: String,\n}\n"
        ),
    );
    assert_rule(owner_census(&second_parser, &lib, &kinds), 2);

    for token in [
        concat!("let digest = Sha", "256::new();\n"),
        concat!("const M: &str = \"fixtures/x/corpus", ".json\";\n"),
        concat!("const D: &str = \"fixtures/x/denominator", ".json\";\n"),
    ] {
        assert_rule(
            owner_census(&with_file(&sources, "tests/extra.rs", token), &lib, &kinds),
            1,
        );
    }
    for shape in [
        concat!(
            "#[derive(Deserialize)]\nstruct Other {\n    pub(crate) cells",
            ": Vec<u8>,\n}\n"
        ),
        concat!(
            "#[derive(\n    Debug,\n    Deserialize,\n)]\nstruct Other {\n    oracle",
            "_id: String,\n}\n"
        ),
        concat!(
            "#[derive(Deserialize)] struct Other {\n    test",
            "_path: String,\n}\n"
        ),
    ] {
        assert_rule(
            owner_census(&with_file(&sources, "tests/extra.rs", shape), &lib, &kinds),
            2,
        );
    }
}

/// gh532 AC10 (gh566 D10 rules 3–5): inside the owner each definition appears exactly
/// once and no support module holds a test; `src/` exports only `evidence`; `sha2` is
/// a dev-dependency only. Each rule has a synthetic negative input.
#[test]
fn owner_census_rejects_second_definitions_exports_and_dependency_kinds() {
    let sources = load_test_sources().unwrap_or_else(|error| panic!("{error}"));
    let lib = lib_rs();
    let kinds = sha2_dependency_kinds().unwrap_or_else(|error| panic!("{error}"));
    let owner = sources
        .iter()
        .find(|(path, _)| path == OWNER_PATH)
        .map(|(_, text)| text.clone())
        .expect("owner source");
    assert_rule(
        owner_census(
            &with_file(
                &sources,
                "tests/support/extra.rs",
                concat!("#[", "test]\nfn t() {}\n"),
            ),
            &lib,
            &kinds,
        ),
        3,
    );

    // Rule 3: each owner definition exists exactly once; a second copy of each fails.
    let owner_with = |extra: &str| -> Sources {
        let mut edited: Sources = sources
            .iter()
            .filter(|(path, _)| path != OWNER_PATH)
            .cloned()
            .collect();
        edited.push((OWNER_PATH.to_owned(), format!("{owner}\n{extra}\n")));
        edited
    };
    for extra in [
        concat!(
            "pub fn sha",
            "256_uri(b: &[u8]) -> String { String::new() }"
        ),
        concat!("fn second() { let _ = Sha", "256::digest(b\"\"); }"),
        concat!(
            "fn third() { let _ = serde_json::from_slice::<",
            "ManifestV0>(b\"\"); }"
        ),
        concat!(
            "fn fourth() { let _ = serde_json::from_slice::<",
            "ManifestV1>(b\"\"); }"
        ),
        concat!("#[", "test]\nfn t() {}"),
    ] {
        assert_rule(owner_census(&owner_with(extra), &lib, &kinds), 3);
    }
    let without_parser: Sources = sources
        .iter()
        .map(|(path, text)| {
            let text = if path == OWNER_PATH {
                text.replace(concat!("from_slice::<", "ManifestV0>"), "from_slice")
            } else {
                text.clone()
            };
            (path.clone(), text)
        })
        .collect();
    assert_rule(owner_census(&without_parser, &lib, &kinds), 3);

    let public_owner = format!("{lib}\npub mod corpus_manifest;\n");
    assert_rule(owner_census(&sources, &public_owner, &kinds), 4);

    assert_rule(owner_census(&sources, &lib, &[None]), 5);
    assert_rule(
        owner_census(&sources, &lib, &[Some("dev".to_owned()), None]),
        5,
    );
}

/// gh566 C9 (census rule 6): outside the owner and the frozen v0 report, no source
/// counts fails or writes a fail label itself; a report renders through `FailSplit`.
#[test]
fn report_census_rejects_reports_that_count_fails_themselves() {
    let sources = load_test_sources().unwrap_or_else(|error| panic!("{error}"));
    let lib = lib_rs();
    let kinds = sha2_dependency_kinds().unwrap_or_else(|error| panic!("{error}"));
    owner_census(&sources, &lib, &kinds).unwrap_or_else(|error| panic!("{error}"));
    for report in [
        concat!("fn r(b: &Board) -> usize { Scoreboard::fail", "ed(&b) }\n"),
        concat!(
            "fn r(b: &[Board]) -> Vec<usize> { b.iter().map(Scoreboard::fail",
            "ed).collect() }\n"
        ),
        concat!("fn r(b: &Board) -> usize { b.fail", "ed() }\n"),
        concat!("const P: &str = \"Pending ", "implementation\";\n"),
        concat!("const D: &str = \"Intentional ", "divergence\";\n"),
    ] {
        assert!(
            matches!(
                owner_census(
                    &with_file(&sources, "tests/v1_report.rs", report),
                    &lib,
                    &kinds
                ),
                Err(CorpusError::ReportBypassesFailSplit { .. })
            ),
            "{report}"
        );
    }
    let through_split = "fn r(run: &Run) -> String { run.fail_split().to_string() }\n";
    owner_census(
        &with_file(&sources, "tests/v1_report.rs", through_split),
        &lib,
        &kinds,
    )
    .unwrap_or_else(|error| panic!("{error}"));
}
