//! KEL-237 Candidate B: validate published lifecycle evidence and its readable report.
//!
//! Behavioral truth remains in `electron_lifecycle.rs` and `lifecycle_corpus.rs`.
//! This file only binds the already-executed Candidate-A CI receipts to KEL-74
//! records, scores each platform independently, and proves `report.md` is a
//! deterministic view of those records.
//!
//! gh566 T2: the generic record rules (digest, pin revision, engine, result, label)
//! and the scoring call live in the shared owner `support/corpus_manifest.rs`.

#![allow(clippy::expect_used, clippy::panic)] // integration-test assertions and String writes

#[path = "support/corpus_admission.rs"]
mod corpus_admission;
#[path = "support/corpus_manifest.rs"]
mod corpus_manifest;

use std::fmt::Write as _;

use corpus_admission::{list_libtest, rust_case_listed};
use corpus_manifest::{
    Corpus, LIFECYCLE_V0, Run, V0_FROZEN, arch_token, host_platform, platform_token, sha256_uri,
    verdict_token,
};
use keld_compat::evidence::{
    Arch, AuthorityProfile, EvidenceRecord, OperationKind, Panel, Platform, Scoreboard,
    parse_evidence, score,
};
use serde::Deserialize;

const REPORT_MD: &[u8] = include_bytes!("../fixtures/lifecycle-corpus/report.md");

const PR_HEAD: &str = "fd3c875c59e3cb0b0530166b21013571afd05754";
const TESTED_COMMIT: &str = "38db257ba2d1f377bd2e24f7bc871faca895c6d5";
const ACTIONS_RUN_ID: u64 = 35_015_086_640;
const CI_REQUIRED_JOB_ID: u64 = 104_538_742_340;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: String,
    source: ReceiptSource,
    runner: ReceiptRunner,
    runtime: ReceiptRuntime,
    corpus: ReceiptCorpus,
    test: ReceiptTest,
    scope: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptSource {
    pull_request: u64,
    pr_head: String,
    tested_commit: String,
    actions_run_id: u64,
    actions_job_id: u64,
    actions_job_name: String,
    ci_required_job_id: u64,
    conclusion: String,
    run_url: String,
    job_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptRunner {
    platform: String,
    arch: String,
    runner_arch: String,
    os: String,
    image: String,
    image_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptRuntime {
    bun_version: String,
    bun_revision: String,
    rust_version: String,
    rust_commit: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptCorpus {
    id: String,
    sha256: String,
    upstream_electron_version: String,
    upstream_electron_commit: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptTest {
    #[serde(rename = "crate")]
    crate_name: String,
    passed: usize,
    skipped: usize,
    mapped_cases: Vec<String>,
}

#[derive(Clone, Copy)]
struct PublishedPlatform {
    label: &'static str,
    platform: Platform,
    arch: Arch,
    receipt: &'static [u8],
    evidence: [&'static [u8]; 3],
    expected_tests: usize,
}

const PUBLISHED: [PublishedPlatform; 3] = [
    PublishedPlatform {
        label: "macOS",
        platform: Platform::Macos,
        arch: Arch::Aarch64,
        receipt: include_bytes!("../fixtures/lifecycle-corpus/receipts/macos-aarch64.json"),
        evidence: [
            include_bytes!(
                "../fixtures/lifecycle-corpus/evidence/macos-aarch64--app-when-ready-host-ready-gate.json"
            ),
            include_bytes!(
                "../fixtures/lifecycle-corpus/evidence/macos-aarch64--app-window-all-closed-policy.json"
            ),
            include_bytes!(
                "../fixtures/lifecycle-corpus/evidence/macos-aarch64--app-quit-return-contract.json"
            ),
        ],
        expected_tests: 48,
    },
    PublishedPlatform {
        label: "Linux",
        platform: Platform::Linux,
        arch: Arch::X86_64,
        receipt: include_bytes!("../fixtures/lifecycle-corpus/receipts/linux-x86_64.json"),
        evidence: [
            include_bytes!(
                "../fixtures/lifecycle-corpus/evidence/linux-x86_64--app-when-ready-host-ready-gate.json"
            ),
            include_bytes!(
                "../fixtures/lifecycle-corpus/evidence/linux-x86_64--app-window-all-closed-policy.json"
            ),
            include_bytes!(
                "../fixtures/lifecycle-corpus/evidence/linux-x86_64--app-quit-return-contract.json"
            ),
        ],
        expected_tests: 48,
    },
    PublishedPlatform {
        label: "Windows",
        platform: Platform::Windows,
        arch: Arch::X86_64,
        receipt: include_bytes!("../fixtures/lifecycle-corpus/receipts/windows-x86_64.json"),
        evidence: [
            include_bytes!(
                "../fixtures/lifecycle-corpus/evidence/windows-x86_64--app-when-ready-host-ready-gate.json"
            ),
            include_bytes!(
                "../fixtures/lifecycle-corpus/evidence/windows-x86_64--app-window-all-closed-policy.json"
            ),
            include_bytes!(
                "../fixtures/lifecycle-corpus/evidence/windows-x86_64--app-quit-return-contract.json"
            ),
        ],
        expected_tests: 49,
    },
];

/// Loads the committed lifecycle corpus through the shared owner.
fn corpus() -> Corpus {
    Corpus::load(&LIFECYCLE_V0).unwrap_or_else(|error| panic!("{error}"))
}

/// The harness engine identity every lifecycle record carries.
fn engine_revision() -> String {
    format!("{}@{TESTED_COMMIT}", V0_FROZEN.engine_token)
}

/// Parses one committed lifecycle CI receipt with unknown fields rejected.
fn parse_receipt(bytes: &[u8]) -> Receipt {
    serde_json::from_slice(bytes).expect("published lifecycle CI receipt")
}

/// Parses the three KEL-74 evidence cells committed for one hosted platform.
fn parse_records(platform: PublishedPlatform) -> Vec<EvidenceRecord> {
    platform
        .evidence
        .iter()
        .map(|bytes| parse_evidence(bytes).expect("published KEL-74 lifecycle record"))
        .collect()
}

/// Validates one platform's records with the shared owner and returns its score.
fn platform_board(corpus: &Corpus, published: PublishedPlatform) -> Scoreboard {
    corpus
        .validate_harness_run(&parse_records(published), V0_FROZEN.records_as_of)
        .map_or_else(
            |error| panic!("{} records: {error}", published.label),
            Run::into_board,
        )
}

/// Validates and scores every published platform, in `PUBLISHED` order.
fn published_boards(corpus: &Corpus) -> Vec<Scoreboard> {
    PUBLISHED
        .iter()
        .map(|published| platform_board(corpus, *published))
        .collect()
}

/// gh566 C7: the one profile every board shares, rendered through `as_str`. `None`
/// when any board has no single profile or two boards disagree.
fn authority_line(boards: &[Scoreboard]) -> Option<&'static str> {
    let first = boards.first()?.authority_profile()?;
    boards
        .iter()
        .all(|board| board.authority_profile() == Some(first))
        .then_some(first.as_str())
}

/// Explains the bounded compatibility meaning of one frozen lifecycle operation.
fn operation_meaning(operation_id: &str) -> &'static str {
    match operation_id {
        "app.when-ready.host-ready-gate" => {
            "`app.whenReady()` stays pending until the host lifecycle `Ready` event."
        }
        "app.window-all-closed.policy" => {
            "Listener presence suppresses default quit; removing the last listener restores it."
        }
        "app.quit.return-contract" => {
            "Intentional divergence: Keld returns `Promise<void>` so typed `KELD-IPC-*` failure remains observable."
        }
        other => panic!("unexpected lifecycle operation in report: {other}"),
    }
}

/// Renders the human-readable report from the same canonical evidence records.
fn render_report() -> String {
    let corpus = corpus();
    let pin = corpus.pin();
    let boards = published_boards(&corpus);
    let authority =
        authority_line(&boards).expect("every published board shares one authority profile");
    let mut out = String::new();

    writeln!(out, "# Bounded Electron lifecycle evidence report").expect("String write");
    writeln!(out).expect("String write");
    writeln!(
        out,
        "This report publishes the committed `electron-lifecycle-v0` **showcase** corpus only. It is not a product/median-app Electron compatibility percentage and it does not qualify native windows, webview engines, strict-profile sandboxing, packaging, releases, or adoption."
    )
    .expect("String write");
    writeln!(out).expect("String write");
    writeln!(out, "- Corpus digest: `{}`", corpus.digest()).expect("String write");
    writeln!(
        out,
        "- Upstream oracle: Electron {} @ `{}`",
        pin.version, pin.commit
    )
    .expect("String write");
    writeln!(out, "- PR source head: `{PR_HEAD}`").expect("String write");
    writeln!(
        out,
        "- Exact GitHub Actions tested merge: `{TESTED_COMMIT}`"
    )
    .expect("String write");
    writeln!(
        out,
        "- Candidate-A Actions run: [{ACTIONS_RUN_ID}](https://github.com/gyldlab/keld/actions/runs/{ACTIONS_RUN_ID})"
    )
    .expect("String write");
    writeln!(
        out,
        "- Authority profile: `{authority}` (the CI conformance harness is an ordinary test process, not a product strict-Bun session)"
    )
    .expect("String write");
    writeln!(
        out,
        "- Harness engine identity: `{}` (headless conformance identity; not WKWebView/WebView2/WebKitGTK)",
        engine_revision()
    )
    .expect("String write");
    writeln!(out).expect("String write");
    writeln!(out, "## Platform results").expect("String write");
    writeln!(out).expect("String write");
    writeln!(
        out,
        "| Platform | Hosted runner | keld-compat batch | Oracle matches | Intentional divergence | Receipt |"
    )
    .expect("String write");
    writeln!(out, "| --- | --- | ---: | ---: | ---: | --- |").expect("String write");

    for (published, board) in PUBLISHED.iter().zip(&boards) {
        let receipt = parse_receipt(published.receipt);
        writeln!(
            out,
            "| {} {} | `{}` `{}` | {} passed / {} skipped | {}/{} | {} (`app.quit()` return contract) | `{}` |",
            published.label,
            receipt.runner.arch,
            receipt.runner.image,
            receipt.runner.image_version,
            receipt.test.passed,
            receipt.test.skipped,
            board.passed(),
            board.denominator(),
            board.failed(),
            sha256_uri(published.receipt),
        )
        .expect("String write");
    }

    writeln!(out).expect("String write");
    writeln!(
        out,
        "Each platform is scored separately against the same committed three-cell denominator. Two cells match Electron's pinned lifecycle oracle; the third intentionally records Keld's `app.quit(): Promise<void>` divergence from Electron's `void` return. Counts are published instead of turning this bounded showcase into an overall Electron-compatibility percentage."
    )
    .expect("String write");
    writeln!(out).expect("String write");
    writeln!(out, "## Cell results").expect("String write");
    writeln!(out).expect("String write");
    writeln!(out, "| Operation | Oracle | Result | Meaning |").expect("String write");
    writeln!(out, "| --- | --- | --- | --- |").expect("String write");

    for record in parse_records(PUBLISHED[0]) {
        let operation = record.operation();
        writeln!(
            out,
            "| `{}` | `{}` | {} | {} |",
            operation.id,
            operation.oracle.id,
            verdict_token(record.result()),
            operation_meaning(&operation.id),
        )
        .expect("String write");
    }

    writeln!(out).expect("String write");
    writeln!(out, "## Reproduction boundary").expect("String write");
    writeln!(out).expect("String write");
    writeln!(
        out,
        "The machine-readable evidence records under `evidence/` are parsed by the existing KEL-74 owner. Their `evidence_uri` values are SHA-256 digests of the corresponding committed `receipts/*.json` bytes. The receipts bind the exact Actions run/job, tested commit, runner image, architecture, Bun/Rust revisions, corpus digest, and successful mapped-oracle batch."
    )
    .expect("String write");
    writeln!(out).expect("String write");
    writeln!(
        out,
        "GitHub Actions tested the pull-request merge commit rather than the PR head directly. Both identities are retained above and in each receipt; neither is rewritten as the later landed squash/merge source."
    )
    .expect("String write");
    writeln!(out).expect("String write");
    writeln!(
        out,
        "The lifecycle corpus validator executes the mapped Rust and Bun behavioral oracles and contains negative controls for missing/skipped/comment-only/helper-only cases. A parser-only success is not treated as lifecycle compatibility evidence."
    )
    .expect("String write");

    out
}

/// Verifies one CI receipt is tied to the claimed runner, source, runtime, and corpus.
fn assert_receipt(corpus: &Corpus, published: PublishedPlatform, receipt: &Receipt) {
    assert_eq!(receipt.schema, "keld.lifecycle.ci-receipt/v1");
    assert_eq!(receipt.source.pull_request, 242);
    assert_eq!(receipt.source.pr_head, PR_HEAD);
    assert_eq!(receipt.source.tested_commit, TESTED_COMMIT);
    assert_eq!(receipt.source.actions_run_id, ACTIONS_RUN_ID);
    assert_eq!(receipt.source.ci_required_job_id, CI_REQUIRED_JOB_ID);
    assert_eq!(receipt.source.conclusion, "success");
    assert!(receipt.source.actions_job_id > 0);
    assert!(
        receipt
            .source
            .actions_job_name
            .starts_with("clippy + test (")
    );
    assert!(
        receipt
            .source
            .run_url
            .ends_with(&ACTIONS_RUN_ID.to_string())
    );
    assert!(
        receipt
            .source
            .job_url
            .ends_with(&receipt.source.actions_job_id.to_string())
    );

    assert_eq!(receipt.runner.platform, platform_token(published.platform));
    assert_eq!(receipt.runner.arch, arch_token(published.arch));
    assert!(!receipt.runner.runner_arch.is_empty());
    assert!(!receipt.runner.os.is_empty());
    assert!(!receipt.runner.image.is_empty());
    assert!(!receipt.runner.image_version.is_empty());

    assert_eq!(receipt.runtime.bun_version, "1.4.2");
    assert_eq!(
        receipt.runtime.bun_revision,
        "744846f844374847c902b5e7fd59b4342a51ef99"
    );
    assert_eq!(receipt.runtime.rust_version, "1.97.1");
    assert_eq!(
        receipt.runtime.rust_commit,
        "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
    );
    assert_eq!(receipt.corpus.id, corpus.id());
    assert_eq!(receipt.corpus.sha256, corpus.digest());
    assert_eq!(
        receipt.corpus.upstream_electron_version,
        corpus.pin().version
    );
    assert_eq!(receipt.corpus.upstream_electron_commit, corpus.pin().commit);
    assert_eq!(receipt.test.crate_name, "keld-compat");
    assert_eq!(receipt.test.passed, published.expected_tests);
    assert_eq!(receipt.test.skipped, 0);
    assert!(
        receipt
            .scope
            .contains("Headless lifecycle conformance only"),
        "receipt must not read as desktop acceptance"
    );

    for required in [
        "electron_lifecycle::when_ready_does_not_resolve_before_host_ready_event",
        "lifecycle_corpus::lifecycle_corpus_cells_map_to_existing_behavioral_oracles",
        "lifecycle_corpus::lifecycle_corpus_denominator_matches_exact_manifest_bytes",
        "lifecycle_corpus::lifecycle_corpus_typescript_oracles_execute",
        "lifecycle_corpus::lifecycle_corpus_rust_oracles_execute",
    ] {
        assert!(
            receipt
                .test
                .mapped_cases
                .iter()
                .any(|case| case == required),
            "{} receipt is missing mapped case {required}",
            published.label
        );
    }
}

/// Verifies the lifecycle-only provenance of each row: platform, revisions and the
/// receipt binding. The generic record rules are the owner's (`validate_harness_run`).
fn assert_records(
    corpus: &Corpus,
    published: PublishedPlatform,
    records: &[EvidenceRecord],
    receipt_uri: &str,
) {
    assert_eq!(records.len(), corpus.cells().len());

    for record in records {
        assert_eq!(record.artifact().platform, published.platform);
        assert_eq!(record.artifact().arch, published.arch);
        assert_eq!(record.revisions().keld, TESTED_COMMIT);
        assert_eq!(record.revisions().bun, "1.4.2+744846f84");
        assert_eq!(record.revisions().engine, engine_revision());
        assert_eq!(record.evidence_uri(), receipt_uri);
    }
}

/// Verifies each platform scores to the same bounded two-match/one-divergence shape.
fn assert_platform_score(board: &Scoreboard) {
    assert_eq!(board.panel(), Panel::Showcase);
    assert_eq!(board.denominator(), 3);
    assert_eq!(board.passed(), 2);
    assert_eq!(board.failed(), 1);
    assert_eq!(board.unknown(), 0);
    assert_eq!(board.waived(), 0);
    assert_eq!(board.missing(), 0);
    assert_eq!(board.unweighted_percent(), Some(66));
    assert!(
        !board.complete(),
        "intentional app.quit divergence keeps this showcase incomplete"
    );
}

/// Confirms all published platform evidence is schema-valid and provenance-bound.
#[test]
fn published_lifecycle_evidence_is_schema_valid_receipt_bound_and_platform_scoped() {
    let corpus = corpus();
    assert_eq!(corpus.denominator().panel(), Panel::Showcase);
    assert_eq!(corpus.denominator().kind(), OperationKind::PrimaryWorkflow);
    assert_eq!(corpus.denominator().corpus_sha256(), corpus.digest());

    for published in PUBLISHED {
        let receipt = parse_receipt(published.receipt);
        assert_receipt(&corpus, published, &receipt);

        let receipt_uri = sha256_uri(published.receipt);
        let records = parse_records(published);
        assert_records(&corpus, published, &records, &receipt_uri);
        assert_platform_score(&platform_board(&corpus, published));
    }
}

/// Confirms the checked-in report is the deterministic view of canonical records.
#[test]
fn lifecycle_report_is_a_deterministic_view_of_canonical_records() {
    let expected = std::str::from_utf8(REPORT_MD).expect("report is UTF-8");
    let rendered = render_report();
    assert_eq!(
        rendered, expected,
        "report.md must be regenerated from the evidence records"
    );
    assert!(
        !rendered.contains('%'),
        "bounded showcase counts must not be marketed as an Electron compatibility percentage"
    );
}

/// Proves verdict or receipt-byte tampering changes the published evidence outcome.
#[test]
fn lifecycle_evidence_negative_controls_change_score_and_break_receipt_binding() {
    let corpus = corpus();
    let published = PUBLISHED[0];
    let mut records = parse_records(published);

    let original = std::str::from_utf8(published.evidence[0]).expect("evidence UTF-8");
    let mutated = original.replacen("\"result\": \"pass\"", "\"result\": \"fail\"", 1);
    assert_ne!(mutated, original, "negative control must mutate a pass");
    records[0] = parse_evidence(mutated.as_bytes()).expect("mutated evidence remains schema-valid");

    let board = score(corpus.denominator(), &records, V0_FROZEN.records_as_of)
        .expect("score mutated records");
    assert_eq!(board.passed(), 1);
    assert_eq!(board.failed(), 2);
    assert_ne!(board.unweighted_percent(), Some(66));

    let expected_uri = records[1].evidence_uri().to_owned();
    let mut mutated_receipt = published.receipt.to_vec();
    mutated_receipt.push(b' ');
    assert_ne!(
        sha256_uri(&mutated_receipt),
        expected_uri,
        "a changed receipt must not retain the committed evidence URI"
    );
}

/// gh566 C7: the report's authority line is the boards' one shared profile, rendered
/// through `AuthorityProfile::as_str`, and the render refuses on any disagreement.
#[test]
fn report_authority_line_comes_from_the_board() {
    let corpus = corpus();
    let boards = published_boards(&corpus);
    assert_eq!(
        authority_line(&boards),
        Some(AuthorityProfile::LegacySandboxOff.as_str())
    );

    let relabel = |bytes: &[u8]| {
        let text = std::str::from_utf8(bytes).expect("evidence UTF-8");
        let relabelled = text.replacen(
            "\"authority_profile\": \"legacy_sandbox_off\"",
            "\"authority_profile\": \"unverified\"",
            1,
        );
        assert_ne!(relabelled, text, "negative control must relabel the record");
        parse_evidence(relabelled.as_bytes()).expect("relabelled record stays schema-valid")
    };
    let published = PUBLISHED[0];
    let scored = |records: &[EvidenceRecord]| {
        score(corpus.denominator(), records, V0_FROZEN.records_as_of).expect("score board")
    };

    let mut mixed = parse_records(published);
    mixed[0] = relabel(published.evidence[0]);
    assert_eq!(
        authority_line(&[scored(&mixed)]),
        None,
        "a board mixing two profiles must not render one"
    );

    let all_unverified: Vec<EvidenceRecord> = published
        .evidence
        .iter()
        .map(|bytes| relabel(bytes))
        .collect();
    let line = authority_line(&[scored(&all_unverified)]);
    assert_eq!(line, Some("unverified"));
    assert_ne!(
        line,
        authority_line(&boards),
        "the line follows the records"
    );

    let disagreeing = [boards[0].clone(), scored(&all_unverified)];
    assert_eq!(
        authority_line(&disagreeing),
        None,
        "two platform boards with different profiles must not render one line"
    );
}

/// gh566 C1: every case the host OS's published receipt maps is a live, non-ignored
/// test of the named target, so the frozen receipts never name a vanished case.
#[test]
fn published_receipt_cases_remain_live_tests() {
    let published = PUBLISHED
        .iter()
        .find(|published| published.platform == host_platform())
        .expect("a published receipt for this host platform");
    let receipt = parse_receipt(published.receipt);
    let mut targets: Vec<&str> = receipt
        .test
        .mapped_cases
        .iter()
        .map(|case| case.split_once("::").expect("target::case").0)
        .collect();
    targets.sort_unstable();
    targets.dedup();
    assert!(!targets.is_empty(), "the receipt maps cases");
    for target in targets {
        let (list, ignored) = list_libtest(target).unwrap_or_else(|error| panic!("{error}"));
        for case in &receipt.test.mapped_cases {
            let Some(name) = case.strip_prefix(&format!("{target}::")) else {
                continue;
            };
            assert!(
                rust_case_listed(&list, &ignored, name),
                "{} receipt maps {case}, which is not exactly one live, non-ignored test",
                published.label
            );
        }
    }
}

/// gh566 C1 negative controls: missing, duplicated, prefixed and ignored names are not live.
#[test]
fn rust_case_listed_rejects_missing_duplicated_prefixed_and_ignored_names() {
    let live = "mapped: test\nother: test\n";
    assert!(rust_case_listed(live, "", "mapped"));
    for (list, ignored) in [
        ("other: test\n", ""),
        ("mapped: test\nmapped: test\n", ""),
        ("module::mapped: test\n", ""),
        ("mapped_extra: test\n", ""),
        (live, "mapped: test\n"),
    ] {
        assert!(
            !rust_case_listed(list, ignored, "mapped"),
            "false admission: {list:?} / {ignored:?}"
        );
    }
}
