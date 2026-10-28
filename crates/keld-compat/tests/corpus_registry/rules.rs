//! Negative controls for the T2 rules, each on an in-memory mutation of the committed
//! lifecycle bytes. `Corpus::parse` checks the digest first (gh566 D2), so every
//! manifest mutation aimed at another rule rehashes the denominator, and every control
//! asserts the exact `CorpusError` variant (gh566 §7, "Negative-control routing").

mod v1_fixture;
mod v1_manifest;
mod v1_runs;
mod v1_snapshots;

use keld_compat::evidence::{EvidenceError, EvidenceRecord, Platform, parse_evidence};

use crate::corpus_admission::{RunnerKind, RunnerOutput, check_admission, check_cells};
use crate::corpus_manifest::{
    Corpus, CorpusError, LIFECYCLE_V0, Registration, Runner, TestTarget, V0_FROZEN,
    committed_record_files, sha256_uri,
};

const ELECTRON_LIFECYCLE: TestTarget = TestTarget {
    path: "crates/keld-compat/tests/electron_lifecycle.rs",
    runner: Runner::Libtest {
        target: "electron_lifecycle",
    },
};
const APP_TEST: TestTarget = TestTarget {
    path: "packages/@keld/electron/src/app.test.ts",
    runner: Runner::Bun,
};

fn committed() -> (Vec<u8>, Vec<u8>) {
    Corpus::fixture_bytes(&LIFECYCLE_V0).unwrap_or_else(|error| panic!("{error}"))
}

fn replace_once(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).expect("fixture is UTF-8");
    let replaced = text.replacen(from, to, 1);
    assert_ne!(replaced, text, "mutation must change `{from}`");
    replaced.into_bytes()
}

/// Rewrites the denominator's `corpus_sha256` to the mutated manifest's digest, so a
/// mutation passes the digest gate and reaches the check it targets.
fn rehash(manifest: &[u8], denominator: &[u8]) -> Vec<u8> {
    let mut value: serde_json::Value =
        serde_json::from_slice(denominator).expect("denominator JSON");
    value["corpus_sha256"] = serde_json::Value::String(sha256_uri(manifest));
    serde_json::to_vec_pretty(&value).expect("denominator JSON")
}

fn rejected(reg: &Registration, manifest: &[u8], denominator: &[u8]) -> CorpusError {
    Corpus::parse(reg, manifest, denominator).expect_err("mutation must be rejected")
}

fn corpus() -> Corpus {
    Corpus::load(&LIFECYCLE_V0).unwrap_or_else(|error| panic!("{error}"))
}

/// The committed macOS run as raw record texts, in file order.
fn macos_texts() -> Vec<String> {
    committed_record_files(&LIFECYCLE_V0)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .filter(|(path, _)| path.starts_with("evidence/macos-aarch64--"))
        .map(|(_, bytes)| String::from_utf8(bytes).expect("record UTF-8"))
        .collect()
}

fn records(texts: &[String]) -> Vec<EvidenceRecord> {
    texts
        .iter()
        .map(|text| parse_evidence(text.as_bytes()).expect("schema-valid record"))
        .collect()
}

/// Replaces `from` with `to` in the macOS record of `operation_id` and validates the run.
fn run_with(operation_id: &str, from: &str, to: &str) -> Result<(), CorpusError> {
    let mut texts = macos_texts();
    let index = texts
        .iter()
        .position(|text| text.contains(&format!("\"id\": \"{operation_id}\"")))
        .expect("record for the operation");
    let mutated = texts[index].replacen(from, to, 1);
    assert_ne!(mutated, texts[index], "mutation must change `{from}`");
    texts[index] = mutated;
    corpus()
        .validate_harness_run(&records(&texts), V0_FROZEN.records_as_of)
        .map(|_| ())
}

fn field_of(error: &CorpusError) -> Option<&'static str> {
    match error {
        CorpusError::RecordMismatch { field, .. } => Some(field),
        _ => None,
    }
}

/// gh532 AC11 controls: a new id in the frozen shape, a 44.4.5 oracle inside the
/// 44.3.0 corpus, and an unadmitted upstream commit.
#[test]
fn v0_shape_rejects_new_ids_and_foreign_pins() {
    let (manifest, denominator) = committed();
    Corpus::parse(&LIFECYCLE_V0, &manifest, &denominator).unwrap_or_else(|error| panic!("{error}"));

    let renamed = Registration {
        corpus_id: "electron-lifecycle-v1",
        ..LIFECYCLE_V0
    };
    assert!(matches!(
        rejected(&renamed, &manifest, &denominator),
        CorpusError::V0ShapeNotAdmitted { .. }
    ));

    let foreign_oracle = replace_once(
        &manifest,
        "\"oracle_id\": \"electron-v44.3.0.app.quit-void\"",
        "\"oracle_id\": \"electron-v44.4.5.app.quit-void\"",
    );
    let error = rejected(
        &LIFECYCLE_V0,
        &foreign_oracle,
        &rehash(&foreign_oracle, &denominator),
    );
    assert!(
        matches!(&error, CorpusError::PinMismatch { cell, .. } if cell == "app.quit.return-contract"),
        "{error}"
    );

    let foreign_commit = replace_once(
        &manifest,
        "\"electron_commit\": \"07e460719c75b2ec5ee4893f7d2192ef31c7b8c2\"",
        "\"electron_commit\": \"694f45852a0f1726cd23bfd379854de489cccb65\"",
    );
    let error = rejected(
        &LIFECYCLE_V0,
        &foreign_commit,
        &rehash(&foreign_commit, &denominator),
    );
    assert!(
        matches!(error, CorpusError::UnadmittedPin { .. }),
        "{error}"
    );

    // D4 compares the version and the commit together: a foreign version alone fails.
    let foreign_version = replace_once(
        &manifest,
        "\"electron_version\": \"44.3.0\"",
        "\"electron_version\": \"44.4.5\"",
    );
    let error = rejected(
        &LIFECYCLE_V0,
        &foreign_version,
        &rehash(&foreign_version, &denominator),
    );
    assert!(
        matches!(error, CorpusError::UnadmittedPin { .. }),
        "{error}"
    );
}

/// gh566 C8: the denominator agrees with the manifest's `corpus_id`, panel and exact
/// `CellKey` set, and manifest cells are unique.
#[test]
fn denominator_must_match_manifest_cells_and_id() {
    let (manifest, denominator) = committed();
    let base: serde_json::Value = serde_json::from_slice(&denominator).expect("denominator");
    let edit = |change: &dyn Fn(&mut serde_json::Value)| {
        let mut value = base.clone();
        change(&mut value);
        serde_json::to_vec_pretty(&value).expect("denominator JSON")
    };

    let missing_fail_cell = edit(&|value| {
        value["cells"]
            .as_array_mut()
            .expect("cells")
            .retain(|cell| cell["operation_id"] != "app.quit.return-contract");
    });
    let extra_cell = edit(&|value| {
        value["cells"]
            .as_array_mut()
            .expect("cells")
            .push(serde_json::json!({
                "operation_id": "app.extra",
                "oracle_id": "electron-v44.3.0.app.extra"
            }));
    });
    let other_id = edit(&|value| {
        value["corpus_id"] = serde_json::json!("electron-lifecycle-other");
    });
    let other_panel = edit(&|value| {
        value["panel"] = serde_json::json!("product");
    });
    for (label, mutated) in [
        ("missing fail cell", missing_fail_cell),
        ("extra cell", extra_cell),
        ("other corpus_id", other_id),
        ("other panel", other_panel),
    ] {
        let error = rejected(&LIFECYCLE_V0, &manifest, &mutated);
        assert!(
            matches!(error, CorpusError::DenominatorMismatch { .. }),
            "{label}: {error}"
        );
    }

    let mut repeated: serde_json::Value = serde_json::from_slice(&manifest).expect("manifest");
    let first = repeated["cells"][0].clone();
    repeated["cells"].as_array_mut().expect("cells").push(first);
    let repeated = serde_json::to_vec_pretty(&repeated).expect("manifest JSON");
    let error = rejected(&LIFECYCLE_V0, &repeated, &rehash(&repeated, &denominator));
    assert!(
        matches!(&error, CorpusError::DenominatorMismatch { detail, .. } if detail.contains("repeats")),
        "{error}"
    );
}

/// gh532 AC2, AC7 and AC9 over v0 records, and the AC8 clause that rejects
/// `strict_bun` on a conformance-harness record.
#[test]
fn harness_run_rejects_digest_revision_engine_and_label_mutations() {
    let run = corpus()
        .validate_harness_run(&records(&macos_texts()), V0_FROZEN.records_as_of)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(run.board().passed(), 2);

    let when_ready = "app.when-ready.host-ready-gate";
    let error = run_with(
        when_ready,
        "\"sha256\": \"sha256:badc0aaf",
        "\"sha256\": \"sha256:00000aaf",
    )
    .expect_err("foreign artifact digest");
    assert_eq!(field_of(&error), Some("artifact.sha256"), "{error}");

    let error = run_with(
        when_ready,
        "electron-v44.3.0@07e460719c75b2ec5ee4893f7d2192ef31c7b8c2",
        "electron-v44.3.0@694f45852a0f1726cd23bfd379854de489cccb65",
    )
    .expect_err("foreign oracle revision");
    assert!(matches!(error, CorpusError::PinMismatch { .. }), "{error}");

    for (from, to) in [
        (
            "\"engine\": \"headless-lifecycle-conformance@",
            "\"engine\": \"wkwebview@",
        ),
        (
            "\"engine\": \"headless-lifecycle-conformance@38db257ba2d1f377bd2e24f7bc871faca895c6d5\"",
            "\"engine\": \"headless-lifecycle-conformance\"",
        ),
    ] {
        let error = run_with(when_ready, from, to).expect_err("foreign engine identity");
        assert_eq!(field_of(&error), Some("revisions.engine"), "{to}: {error}");
    }

    for label in ["strict_bun", "unverified"] {
        let error = run_with(
            when_ready,
            "\"authority_profile\": \"legacy_sandbox_off\"",
            &format!("\"authority_profile\": \"{label}\""),
        )
        .expect_err("relabelled harness record");
        assert!(
            matches!(&error, CorpusError::LabelMismatch { label: found, receipt_state, .. }
                if *found == label && *receipt_state == "conformance-harness"),
            "{error}"
        );
    }
}

/// gh566 C5 v0 controls: each record agrees with its cell, at most once per cell.
#[test]
fn records_must_match_their_v0_cell() {
    let when_ready = "app.when-ready.host-ready-gate";
    let quit = "app.quit.return-contract";

    let error = run_with(quit, "\"result\": \"fail\"", "\"result\": \"pass\"")
        .expect_err("pass on the divergence cell");
    assert_eq!(field_of(&error), Some("result"), "{error}");
    let error = run_with(when_ready, "\"result\": \"pass\"", "\"result\": \"fail\"")
        .expect_err("fail on a pass cell");
    assert_eq!(field_of(&error), Some("result"), "{error}");
    let error = run_with(
        when_ready,
        "\"result\": \"pass\",",
        "\"result\": \"waived\",\n  \"waiver\": { \"owner\": \"o\", \"reason\": \"r\", \"expires_on\": \"2027-01-01\" },",
    )
    .expect_err("waived record");
    assert_eq!(field_of(&error), Some("result"), "{error}");
    let error = run_with(
        when_ready,
        "\"kind\": \"primary_workflow\"",
        "\"kind\": \"activation\"",
    )
    .expect_err("foreign kind");
    assert_eq!(field_of(&error), Some("operation.kind"), "{error}");
    let error = run_with(
        when_ready,
        "\"id\": \"app.when-ready.host-ready-gate\"",
        "\"id\": \"app.not-a-cell\"",
    )
    .expect_err("record outside the manifest");
    assert_eq!(field_of(&error), Some("cell"), "{error}");
    // The cell key is the full (operation_id, oracle_id) pair: a foreign oracle id on a
    // known operation is outside the manifest too (`score()` would only ignore it).
    let error = run_with(
        when_ready,
        "\"id\": \"electron-v44.3.0.app.when-ready-initialized\"",
        "\"id\": \"electron-v44.3.0.app.when-ready-foreign\"",
    )
    .expect_err("record with a foreign oracle id");
    assert_eq!(field_of(&error), Some("cell"), "{error}");

    run_with(
        when_ready,
        "\"result\": \"pass\"",
        "\"result\": \"unknown\"",
    )
    .unwrap_or_else(|error| panic!("an unrun lane may record unknown: {error}"));

    let corpus = corpus();
    let mut duplicated = records(&macos_texts());
    duplicated.push(duplicated[0].clone());
    let error = corpus
        .validate_harness_run(&duplicated, V0_FROZEN.records_as_of)
        .expect_err("two records for one cell");
    assert!(
        matches!(
            error,
            CorpusError::Evidence(EvidenceError::DuplicateCell { .. })
        ),
        "{error}"
    );

    let mut mixed = records(&macos_texts());
    let linux = committed_record_files(&LIFECYCLE_V0)
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .find(|(path, _)| path.starts_with("evidence/linux-x86_64--"))
        .map(|(_, bytes)| parse_evidence(&bytes).expect("linux record"))
        .expect("a linux record");
    assert_eq!(linux.artifact().platform, Platform::Linux);
    mixed.push(linux);
    assert!(matches!(
        corpus.validate_harness_run(&mixed, V0_FROZEN.records_as_of),
        Err(CorpusError::MixedRun { .. })
    ));
    assert!(matches!(
        corpus.validate_harness_run(&[], V0_FROZEN.records_as_of),
        Err(CorpusError::EmptyRun { .. })
    ));
}

/// gh566 C3: cells map only registered targets, and a registered target is an existing
/// keld-compat libtest file or `packages/**/*.test.ts` that does not include the owner.
#[test]
fn targets_reject_unregistered_recursive_foreign_and_missing() {
    const NO_BUN: &[TestTarget] = &[ELECTRON_LIFECYCLE];
    const RECURSIVE: &[TestTarget] = &[
        ELECTRON_LIFECYCLE,
        APP_TEST,
        TestTarget {
            path: "crates/keld-compat/tests/lifecycle_corpus.rs",
            runner: Runner::Libtest {
                target: "lifecycle_corpus",
            },
        },
    ];
    const FOREIGN: &[TestTarget] = &[
        ELECTRON_LIFECYCLE,
        APP_TEST,
        TestTarget {
            path: "crates/keld-host/tests/x.rs",
            runner: Runner::Libtest { target: "x" },
        },
    ];
    const MISSING: &[TestTarget] = &[
        ELECTRON_LIFECYCLE,
        APP_TEST,
        TestTarget {
            path: "crates/keld-compat/tests/does_not_exist.rs",
            runner: Runner::Libtest {
                target: "does_not_exist",
            },
        },
    ];
    const BUN_MISSING: &[TestTarget] = &[
        ELECTRON_LIFECYCLE,
        APP_TEST,
        TestTarget {
            path: "packages/@keld/electron/src/missing.test.ts",
            runner: Runner::Bun,
        },
    ];
    const BUN_OUTSIDE_PACKAGES: &[TestTarget] = &[
        ELECTRON_LIFECYCLE,
        APP_TEST,
        TestTarget {
            path: "crates/keld-compat/tests/app.test.ts",
            runner: Runner::Bun,
        },
    ];
    let (manifest, denominator) = committed();
    let with = |targets| Registration {
        targets,
        ..LIFECYCLE_V0
    };

    let error = rejected(&with(NO_BUN), &manifest, &denominator);
    assert!(
        matches!(&error, CorpusError::UnregisteredTarget { test_path, .. } if test_path == APP_TEST.path),
        "{error}"
    );
    for (label, targets) in [
        ("recursive", RECURSIVE),
        ("foreign package", FOREIGN),
        ("missing file", MISSING),
        ("bun outside packages", BUN_OUTSIDE_PACKAGES),
        ("missing bun file", BUN_MISSING),
    ] {
        let error = rejected(&with(targets), &manifest, &denominator);
        assert!(
            matches!(error, CorpusError::InvalidTarget { .. }),
            "{label}: {error}"
        );
    }
}

/// Execution admission is the pure check over runner output: a green runner without the
/// case, a skipped Bun case and an ignored libtest case are each not admitted.
#[test]
fn admission_rejects_missing_skipped_and_ignored_cases() {
    let corpus = corpus();
    let corpora = [&corpus];
    let rust_name = "when_ready_does_not_resolve_before_host_ready_event";
    let rust = |text: &str| {
        check_admission(
            &corpora,
            Platform::Macos,
            RunnerKind::Libtest,
            &[RunnerOutput {
                path: ELECTRON_LIFECYCLE.path.to_owned(),
                text: text.to_owned(),
            }],
        )
    };
    rust(&format!("test {rust_name} ... ok\n")).unwrap_or_else(|error| panic!("{error}"));
    for text in [
        "test result: ok. 0 passed; 0 failed;\n".to_owned(),
        format!("test {rust_name} ... ignored\n"),
    ] {
        assert!(
            matches!(rust(&text), Err(CorpusError::CaseNotAdmitted { .. })),
            "{text}"
        );
    }

    let bun_cells: Vec<&str> = corpus
        .cells()
        .iter()
        .filter(|cell| cell.test_path == APP_TEST.path)
        .map(|cell| cell.test_name.as_str())
        .collect();
    let bun = |lines: &[String]| {
        check_admission(
            &corpora,
            Platform::Macos,
            RunnerKind::Bun,
            &[RunnerOutput {
                path: APP_TEST.path.to_owned(),
                text: lines.join("\n"),
            }],
        )
    };
    let passing: Vec<String> = bun_cells
        .iter()
        .map(|name| format!("(pass) app > {name} [1.00ms]"))
        .collect();
    bun(&passing).unwrap_or_else(|error| panic!("{error}"));
    // Defence in depth: admission fails closed on a cell whose path maps no target.
    let mut stray = corpus.cells()[0].clone();
    stray.test_path = "crates/keld-compat/tests/unregistered.rs".to_owned();
    for kind in [RunnerKind::Libtest, RunnerKind::Bun] {
        assert!(matches!(
            check_cells(
                corpus.id(),
                &[stray.clone()],
                corpus.registration(),
                Platform::Macos,
                kind,
                &[]
            ),
            Err(CorpusError::UnregisteredTarget { .. })
        ));
    }

    let mut skipped = passing.clone();
    skipped[0] = format!("(skip) app > {}", bun_cells[0]);
    assert!(matches!(
        bun(&skipped),
        Err(CorpusError::CaseNotAdmitted { .. })
    ));
}

/// GH-569: cross-crate targets are trusted static registrations, never manifest commands.
#[test]
fn exact_cross_crate_target_validates_package_path_and_recursion() {
    use crate::corpus_manifest::check_targets;
    const VALID: &[TestTarget] = &[TestTarget {
        path: "crates/keld-host/tests/no_flag_macos.rs",
        runner: Runner::ExactLibtest {
            package: "keld-host",
            target: "no_flag_macos",
            case: "module_process::shipping_role_early_dependency_reports_process_facts",
        },
    }];
    const WRONG: &[TestTarget] = &[TestTarget {
        path: "crates/keld-host/tests/no_flag_macos.rs",
        runner: Runner::ExactLibtest {
            package: "keld-compat",
            target: "no_flag_macos",
            case: "module_process::shipping_role_early_dependency_reports_process_facts",
        },
    }];
    const RECURSIVE: &[TestTarget] = &[TestTarget {
        path: "crates/keld-compat/tests/lifecycle_corpus.rs",
        runner: Runner::ExactLibtest {
            package: "keld-compat",
            target: "lifecycle_corpus",
            case: "lifecycle_corpus_denominator_matches_exact_manifest_bytes",
        },
    }];
    const CONSUMER: &[TestTarget] = &[TestTarget {
        path: "crates/keld-compat/tests/module_process_corpus.rs",
        runner: Runner::ExactLibtest {
            package: "keld-compat",
            target: "module_process_corpus",
            case: "actual_observations_match_manifest_statuses",
        },
    }];
    assert!(matches!(
        check_targets(&Registration {
            targets: CONSUMER,
            ..LIFECYCLE_V0
        }),
        Err(CorpusError::InvalidTarget { .. })
    ));
    assert!(
        check_targets(&Registration {
            targets: VALID,
            ..LIFECYCLE_V0
        })
        .is_ok()
    );
    assert!(matches!(
        check_targets(&Registration {
            targets: WRONG,
            ..LIFECYCLE_V0
        }),
        Err(CorpusError::InvalidTarget { .. })
    ));
    assert!(matches!(
        check_targets(&Registration {
            targets: RECURSIVE,
            ..LIFECYCLE_V0
        }),
        Err(CorpusError::InvalidTarget { .. })
    ));
}

#[test]
fn exact_case_selection_and_platform_filter_fail_closed() {
    use crate::corpus_admission::{rust_case_passed, target_eligible};
    use crate::corpus_manifest::MODULE_PROCESS_V1;
    let corpus = Corpus::load(&MODULE_PROCESS_V1).expect("validated module/process corpus");
    let host = corpus
        .registration()
        .targets
        .iter()
        .find(|target| matches!(target.runner, Runner::ExactLibtest { .. }))
        .expect("host registration");
    assert!(target_eligible(&corpus, host, Platform::Macos));
    assert!(!target_eligible(&corpus, host, Platform::Linux));
    assert!(!target_eligible(&corpus, host, Platform::Windows));
    assert!(!rust_case_passed("test missing ... ignored\n", "missing"));
    assert!(!rust_case_passed(
        "test similarly_named ... ok\n",
        "missing"
    ));
    assert!(!rust_case_passed(
        "test missing ... ok\ntest missing ... ok\n",
        "missing"
    ));
    for detail in ["cannot spawn cargo", "compilation failed"] {
        let unrelated = Err(CorpusError::RunnerFailed {
            target: "no_flag_macos".to_owned(),
            detail: detail.to_owned(),
        });
        assert!(
            !is_zero_selected(&unrelated),
            "unrelated runner failure cannot prove zero selection: {detail}"
        );
    }
    let selected_nothing = Err(CorpusError::RunnerFailed {
        target: "no_flag_macos".to_owned(),
        detail: "exit exit status: 0. stdout:\nrunning 0 tests\n".to_owned(),
    });
    assert!(
        is_zero_selected(&selected_nothing),
        "a zero-selection detail must be recognized"
    );
}

/// The oracle is the runner's observed libtest output, not merely its error type.
fn is_zero_selected(result: &Result<String, CorpusError>) -> bool {
    matches!(result, Err(CorpusError::RunnerFailed { detail, .. })
        if detail.lines().any(|line| line.trim() == "running 0 tests"))
}

#[cfg(target_os = "macos")]
#[test]
fn macos_exact_missing_selector_rejects_zero_selected_tests() {
    use crate::corpus_admission::run_exact_case;
    let wrong = run_exact_case("keld-host", "no_flag_macos", "gh569_nonexistent_exact_case");
    assert!(
        is_zero_selected(&wrong),
        "missing selector must run and report zero selected tests: {wrong:?}"
    );
}

#[test]
fn exact_registered_case_mismatch_is_rejected_before_execution() {
    use crate::corpus_manifest::{MODULE_PROCESS_V1, read_store_snapshot, workspace_root};
    let (manifest, denominator) =
        Corpus::fixture_bytes(&MODULE_PROCESS_V1).expect("module corpus bytes");
    let mut edited: serde_json::Value = serde_json::from_slice(&manifest).expect("manifest JSON");
    let cell = edited["cells"]
        .as_array_mut()
        .expect("cells")
        .iter_mut()
        .find(|cell| cell["operation_id"] == "process.early.type")
        .expect("early type cell");
    cell["test_name"] = serde_json::json!("wrong::selector");
    let changed = serde_json::to_vec_pretty(&edited).expect("mutated manifest");
    let denominator = rehash(&changed, &denominator);
    let error = Corpus::parse_with_snapshots(&MODULE_PROCESS_V1, &changed, &denominator, &|key| {
        read_store_snapshot(&workspace_root(), key)
    })
    .expect_err("mismatched selector must fail before spawn");
    assert!(
        matches!(error, CorpusError::InvalidTarget { .. }),
        "{error}"
    );
}

#[test]
fn duplicate_static_target_path_is_rejected_as_ambiguous() {
    use crate::corpus_manifest::{MODULE_PROCESS_V1, check_targets};
    const DUPLICATE: &[TestTarget] = &[MODULE_PROCESS_V1.targets[1], MODULE_PROCESS_V1.targets[1]];
    let registration = Registration {
        targets: DUPLICATE,
        ..MODULE_PROCESS_V1
    };
    assert!(
        matches!(
            check_targets(&registration),
            Err(CorpusError::InvalidTarget { .. })
        ),
        "duplicate target path must fail before selection"
    );
}
