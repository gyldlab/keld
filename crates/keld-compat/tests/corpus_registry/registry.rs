//! Committed corpora: the fixture census (C2), static rules and records, admission of
//! every registered target, and the owner census (gh532 AC10).

use std::fs;

use crate::corpus_admission::{admit_bun, admit_libtest};
use crate::corpus_manifest::{
    Corpus, CorpusError, LIFECYCLE_V0, OWNER_PATH, REGISTRY, Registration, Sources,
    committed_corpus_dirs, committed_runs, crate_root, fixture_census, load_test_sources,
    owner_census, sha2_dependency_kinds,
};

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

    let mut unregistered = found.clone();
    unregistered.push("fixtures/unregistered-corpus".to_owned());
    let absent: Vec<String> = Vec::new();
    let renamed = Registration {
        fixture_dir: "fixtures/elsewhere",
        ..LIFECYCLE_V0
    };
    for (dirs, registry) in [
        (&unregistered, REGISTRY),
        (&absent, REGISTRY),
        (&found, &[LIFECYCLE_V0, LIFECYCLE_V0][..]),
        (
            &found,
            &[
                LIFECYCLE_V0,
                Registration {
                    corpus_id: "other",
                    ..LIFECYCLE_V0
                },
            ][..],
        ),
        (&found, &[renamed][..]),
    ] {
        assert!(
            matches!(
                fixture_census(dirs, registry),
                Err(CorpusError::FixtureCensus { .. })
            ),
            "census must reject {dirs:?} against {} registrations",
            registry.len()
        );
    }
}

/// Every registered corpus passes the static rules, and its committed records pass the
/// harness-run rules, one `(platform, arch)` run at a time.
#[test]
fn registered_corpora_validate_with_their_records() {
    let mut runs_checked = 0;
    for (reg, corpus) in REGISTRY.iter().zip(registered()) {
        for run in committed_runs(reg).unwrap_or_else(|error| panic!("{error}")) {
            corpus
                .validate_harness_run(&run, corpus.records_as_of())
                .unwrap_or_else(|error| panic!("{}: {error}", reg.corpus_id));
            runs_checked += 1;
        }
    }
    assert_eq!(
        runs_checked, 3,
        "the lifecycle corpus publishes three platform runs"
    );
}

/// Runs each distinct registered libtest target once and admits every mapped cell.
#[test]
fn registered_corpora_libtest_oracles_execute() {
    let corpora = registered();
    let refs: Vec<&Corpus> = corpora.iter().collect();
    admit_libtest(&refs).unwrap_or_else(|error| panic!("{error}"));
}

/// Runs each distinct registered Bun file once and admits every mapped cell.
#[test]
fn registered_corpora_bun_oracles_execute() {
    let corpora = registered();
    let refs: Vec<&Corpus> = corpora.iter().collect();
    admit_bun(&refs).unwrap_or_else(|error| panic!("{error}"));
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

    let owner = sources
        .iter()
        .find(|(path, _)| path == OWNER_PATH)
        .map(|(_, text)| text.clone())
        .expect("owner source");
    let mut doubled: Sources = sources
        .iter()
        .filter(|(path, _)| path != OWNER_PATH)
        .cloned()
        .collect();
    doubled.push((
        OWNER_PATH.to_owned(),
        format!(
            "{owner}\n{}",
            concat!(
                "pub fn sha",
                "256_uri(b: &[u8]) -> String { String::new() }"
            )
        ),
    ));
    assert_rule(owner_census(&doubled, &lib, &kinds), 3);
    let mut with_test: Sources = sources
        .iter()
        .filter(|(path, _)| path != OWNER_PATH)
        .cloned()
        .collect();
    with_test.push((
        OWNER_PATH.to_owned(),
        format!("{owner}\n{}\nfn t() {{}}\n", concat!("#[", "test]")),
    ));
    assert_rule(owner_census(&with_test, &lib, &kinds), 3);

    let public_owner = format!("{lib}\npub mod corpus_manifest;\n");
    assert_rule(owner_census(&sources, &public_owner, &kinds), 4);

    assert_rule(owner_census(&sources, &lib, &[None]), 5);
    assert_rule(
        owner_census(&sources, &lib, &[Some("dev".to_owned()), None]),
        5,
    );
}
