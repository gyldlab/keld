# KELD source census — 2026-10-15

Snapshot of pinned Git objects, not the live working tree. Physical lines include blank lines.
Audited commit: 709d302dc8df9b3623fd29a0fca0a45d9a6087db.
Tracked manifest reconciliation: **1014/1014**.
A: crate/package entrypoints; B: authored implementation; C: tests;
D: tooling/CI/config; E: authored documentation; F: explicit exclusions.

## Population and distributions

| Class | Files | Text files | Physical lines | Blob bytes | p50 | p75 | p90 | p95 | p99 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| A | 15 | 15 | 10129 | 398563 | 126.0 | 373.5 | 1685.6 | 3356.1 | 5140.8 |
| B | 140 | 140 | 120140 | 4697530 | 362.0 | 919.5 | 1694.5 | 2859.2 | 7139.3 |
| C | 304 | 304 | 108000 | 4193902 | 260.0 | 417.2 | 828.8 | 1092.9 | 1811.1 |
| D | 89 | 89 | 25599 | 1133472 | 40.0 | 220.0 | 574.4 | 1128.2 | 3496.1 |
| E | 115 | 115 | 43208 | 3084553 | 193.0 | 489.5 | 912.0 | 1364.3 | 1763.0 |
| F | 351 | 0 | 0 | 1244834 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

## Hypothetical category-specific policy impact (NOT adopted)

Hard counts include existing files that would require ratchet/grandfathering; LOC and bytes counted separately, union deduplicated. Strict greater-than.

| Proposal | Class | Files | Review LOC | Review bytes | Hard LOC | Hard bytes | Legacy union |
|---|---|---:|---:|---:|---:|---:|---:|
| candidate | A | 15 | 3 | 4 | 3 | 2 | 3 |
| candidate | B | 140 | 45 | 29 | 26 | 15 | 26 |
| candidate | C | 304 | 85 | 82 | 19 | 13 | 21 |
| candidate | D | 89 | 7 | 4 | 3 | 3 | 3 |
| candidate | E | 115 | 0 | 11 | 0 | 2 | 2 |
| roomier | A | 15 | 3 | 4 | 2 | 2 | 2 |
| roomier | B | 140 | 45 | 29 | 15 | 12 | 15 |
| roomier | C | 304 | 85 | 82 | 9 | 3 | 9 |
| roomier | D | 89 | 7 | 4 | 3 | 1 | 3 |
| roomier | E | 115 | 0 | 11 | 0 | 1 | 1 |

## Largest code modules (not automatic refactor candidates)

| Path | Class | LOC | Bytes | Test cfg marker | Cfg-tail proxy | Test-module-tail proxy |
|---|---|---:|---:|---|---:|---:|
| crates/keld-core/src/app_session.rs | B | 14496 | 585682 | yes | 0.5844 | 0.411 |
| crates/keld-runtime/src/windows_job.rs | B | 7612 | 324413 | yes | 0.3835 | 0.3812 |
| tools/ci_hygiene.rs | D | 6797 | 303148 | no | n/a | n/a |
| crates/keld-runtime/src/macos_guardian.rs | B | 6400 | 248746 | yes | 0.8956 | 0.4978 |
| crates/keld-runtime/src/lib.rs | A | 5587 | 219167 | yes | 0.8878 | 0.5737 |
| crates/keld-ipc/src/bootstrap.rs | B | 5415 | 215123 | yes | 0.9791 | 0.9695 |
| crates/keld-wv/src/webview2/mod.rs | B | 5043 | 214204 | yes | 0.2693 | 0.2691 |
| tools/product_status.rs | D | 3046 | 106917 | no | n/a | n/a |
| packages/@keld/kipc/src/transport.ts | B | 2942 | 111196 | no | n/a | n/a |
| crates/keld-ipc/src/link.rs | B | 2863 | 110800 | yes | 0.9986 | 0.6367 |
| crates/keld-wv/src/profile.rs | B | 2859 | 106499 | yes | 0.3207 | 0.3204 |
| crates/keld-update/src/windows_baseline/tests/transaction.rs | C | 2530 | 99106 | no | n/a | n/a |
| crates/keld-compat/src/evidence.rs | B | 2449 | 89873 | yes | 0.3961 | 0.3957 |
| crates/keld-native/src/fs.rs | B | 2435 | 88377 | yes | 0.4563 | 0.3856 |
| crates/keld-runtime/tests/windows_host_death_job.rs | C | 2409 | 95620 | no | n/a | n/a |
| crates/keld-guard/src/lib.rs | A | 2400 | 93800 | yes | 0.4608 | 0.4604 |
| crates/keld-wv/src/webview2/media_acceptance.rs | B | 2137 | 88227 | yes | 0.5395 | 0.5391 |
| crates/keld-update/src/windows_baseline/tests/writer.rs | C | 2128 | 84225 | no | n/a | n/a |
| crates/keld-wv/src/wkwebview/macos_profile.rs | B | 2114 | 86639 | yes | 0.0629 | 0.0624 |
| tools/atomic_protocol.rs | D | 2062 | 87552 | no | n/a | n/a |

## Recently changed code (change-frequency proxy, not ownership proof)

Count of commits touching each *current path* since 2026-09-15 on the pinned history. Renames are not followed. A frequently edited file is not automatically architecturally bad.
| Path | Class | LOC | Commits touching path |
|---|---|---:|---:|
| tools/ci-inputs.json | D | 457 | 61 |
| crates/keld-core/src/app_session.rs | B | 14496 | 47 |
| docs/engineering/product-status.tsv | D | 31 | 44 |
| crates/keld-wv/src/webview2/mod.rs | B | 5043 | 29 |
| tools/ci_hygiene.rs | D | 6797 | 25 |
| .github/workflows/ci.yml | D | 1029 | 24 |
| crates/keld-native/src/fs.rs | B | 2435 | 22 |
| crates/keld-host/tests/no_flag_windows.rs | C | 346 | 22 |
| crates/keld-guard/src/lib.rs | A | 2400 | 20 |
| crates/keld-update/src/lib.rs | A | 126 | 20 |
| crates/keld-update/src/windows_baseline/tests.rs | C | 55 | 16 |
| crates/keld-update/src/windows_baseline.rs | B | 859 | 15 |
| crates/keld-wv/src/webview2/media_acceptance.rs | B | 2137 | 14 |
| crates/keld-update/src/tests.rs | C | 1420 | 14 |
| crates/keld-native/tests/retained_fs.rs | C | 1041 | 14 |

## Verification and scope limits

- Independently spot-checked 45 blob line counts; mismatches: 0.
- Source of truth: git ls-tree -r -z and git cat-file --batch; every path has a recorded category and F exclusions explain why a file is out of scope.
- Inline-test tail is a LOW-CONFIDENCE proxy: everything after the first line-anchored Rust cfg(test) marker is counted; it is NOT a semantic test code percentage.
- Another LOW-CONFIDENCE measure starts at the first source line of a tests-like module; neither proxy is an AST-aware count, and production items may follow test-only fragments.
- Category C test entry modules use existing review triggers (>100 LOC or 4 KiB), other test modules >400 LOC or 16 KiB; review is not rejection.
- Do not impose file limits until their design implications and exceptions are independently reviewed.
- Candidate priority for next stage: inspect ownership and test discovery of test-heavy modules, compare agents' before/after read-set, check active PRs, then choose cohesive families.
- Commit-touch counts are a bounded change-frequency proxy, not evidence of coupling or ownership; the census does not measure live updater behavior, compiled test identities or platform compatibility.

## Reproduce

From a checkout of KELD:

    python tools/test_source_census.py
    python tools/source_census.py --repo . --commit 709d302dc8df9b3623fd29a0fca0a45d9a6087db --output docs/audits/source-census-2026-10-15 --date 2026-10-15

Machine-readable evidence: manifest.csv (one row per tracked path) and summary.json.
Counts include source comments/blank lines, not executable statements; file classification is deterministic, but architectural judgments remain human-reviewed.
