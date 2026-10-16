# KELD source census — 2026-10-16

Snapshot of pinned Git objects, not the live working tree. Physical lines include blank lines.
Audited commit: 16eaccbbc7f5d95f8daad6b432baa7d033426b65.
Tracked manifest reconciliation: **1114/1114**.
A: crate/package entrypoints; B: authored implementation; C: tests;
D: tooling/CI/config; E: authored documentation; F: explicit exclusions.

## Population and distributions

| Class | Files | Text files | Physical lines | Blob bytes | p50 | p75 | p90 | p95 | p99 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| A | 15 | 15 | 10129 | 398563 | 126.0 | 373.5 | 1685.6 | 3356.1 | 5140.8 |
| B | 177 | 177 | 107467 | 4159931 | 288.0 | 695.0 | 1286.4 | 1898.0 | 5024.6 |
| C | 361 | 361 | 125824 | 4900052 | 247.0 | 395.0 | 751.0 | 1022.0 | 1941.4 |
| D | 92 | 92 | 22701 | 1009887 | 43.5 | 266.5 | 556.2 | 1115.7 | 2808.5 |
| E | 115 | 115 | 43208 | 3084578 | 193.0 | 489.5 | 912.0 | 1364.3 | 1763.0 |
| F | 354 | 0 | 0 | 1445756 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

## Hypothetical category-specific policy impact (NOT adopted)

Hard counts include existing files that would require ratchet/grandfathering; LOC and bytes counted separately, union deduplicated. Strict greater-than.

| Proposal | Class | Files | Review LOC | Review bytes | Hard LOC | Hard bytes | Legacy union |
|---|---|---:|---:|---:|---:|---:|---:|
| candidate | A | 15 | 3 | 4 | 3 | 2 | 3 |
| candidate | B | 177 | 42 | 25 | 22 | 12 | 22 |
| candidate | C | 361 | 95 | 92 | 20 | 14 | 22 |
| candidate | D | 92 | 7 | 4 | 3 | 3 | 3 |
| candidate | E | 115 | 0 | 11 | 0 | 2 | 2 |
| roomier | A | 15 | 3 | 4 | 2 | 2 | 2 |
| roomier | B | 177 | 42 | 25 | 12 | 8 | 12 |
| roomier | C | 361 | 95 | 92 | 10 | 4 | 10 |
| roomier | D | 92 | 7 | 4 | 3 | 1 | 3 |
| roomier | E | 115 | 0 | 11 | 0 | 1 | 1 |

## Largest code modules (not automatic refactor candidates)

| Path | Class | LOC | Bytes | Test cfg marker | Cfg-tail proxy | Test-module-tail proxy |
|---|---|---:|---:|---|---:|---:|
| crates/keld-core/src/app_session.rs | B | 8145 | 319838 | yes | 0.9899 | n/a |
| crates/keld-runtime/src/windows_job.rs | B | 5780 | 246682 | yes | 0.9931 | 0.3917 |
| crates/keld-runtime/src/lib.rs | A | 5587 | 219167 | yes | 0.8878 | 0.5737 |
| crates/keld-ipc/src/bootstrap.rs | B | 4786 | 186842 | yes | 0.9764 | 0.9655 |
| crates/keld-wv/src/webview2/mod.rs | B | 4243 | 176028 | yes | 0.9809 | 0.1897 |
| tools/ci_hygiene_tests.rs | C | 3651 | 148728 | no | n/a | n/a |
| tools/product_status.rs | D | 3046 | 106917 | no | n/a | n/a |
| packages/@keld/kipc/src/transport.ts | B | 2942 | 111196 | no | n/a | n/a |
| tools/ci_hygiene.rs | D | 2785 | 131298 | no | n/a | n/a |
| crates/keld-native/src/fs.rs | B | 2435 | 88377 | yes | 0.4563 | 0.3856 |
| crates/keld-runtime/tests/windows_host_death_job.rs | C | 2409 | 95620 | no | n/a | n/a |
| crates/keld-guard/src/lib.rs | A | 2400 | 93800 | yes | 0.4608 | 0.4604 |
| crates/keld-update/src/windows_baseline/tests/transaction.rs | C | 2365 | 92580 | no | n/a | n/a |
| crates/keld-wv/src/webview2/media_acceptance.rs | B | 2137 | 88227 | yes | 0.5395 | 0.5391 |
| crates/keld-update/src/windows_baseline/tests/writer.rs | C | 2128 | 84225 | no | n/a | n/a |
| crates/keld-wv/src/wkwebview/macos_profile.rs | B | 2114 | 86639 | yes | 0.0629 | 0.0624 |
| tools/atomic_protocol.rs | D | 2062 | 87552 | no | n/a | n/a |
| crates/keld-ipc/src/windows_named_pipe.rs | B | 1938 | 73844 | yes | 0.9912 | 0.0408 |
| crates/keld-update/src/windows_baseline/load.rs | B | 1888 | 77987 | yes | 0.5466 | 0.0413 |
| crates/keld-ipc/src/link.rs | B | 1865 | 71202 | yes | 0.9979 | 0.4424 |

## Recently changed code (change-frequency proxy, not ownership proof)

Count of commits touching each *current path* since 2026-09-15 on the pinned history. Renames are not followed. A frequently edited file is not automatically architecturally bad.
| Path | Class | LOC | Commits touching path |
|---|---|---:|---:|
| tools/ci-inputs.json | D | 457 | 121 |
| crates/keld-core/src/app_session.rs | B | 8145 | 53 |
| docs/engineering/product-status.tsv | D | 31 | 44 |
| crates/keld-wv/src/webview2/mod.rs | B | 4243 | 33 |
| tools/ci_hygiene.rs | D | 2785 | 27 |
| .github/workflows/ci.yml | D | 1072 | 26 |
| crates/keld-native/src/fs.rs | B | 2435 | 22 |
| crates/keld-host/tests/no_flag_windows.rs | C | 346 | 22 |
| crates/keld-guard/src/lib.rs | A | 2400 | 20 |
| crates/keld-update/src/lib.rs | A | 126 | 20 |
| crates/keld-runtime/src/windows_job.rs | B | 5780 | 18 |
| crates/keld-runtime/src/macos_guardian.rs | B | 437 | 18 |
| crates/keld-wv/src/profile.rs | B | 993 | 16 |
| crates/keld-update/src/windows_baseline/tests.rs | C | 55 | 16 |
| crates/keld-ipc/src/link.rs | B | 1865 | 15 |

## Verification and scope limits

- Independently spot-checked 46 blob line counts; mismatches: 0.
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
    python tools/source_census.py --repo . --commit 16eaccbbc7f5d95f8daad6b432baa7d033426b65 --output docs/audits/source-census-2026-10-16 --date 2026-10-16

Machine-readable evidence: manifest.csv (one row per tracked path) and summary.json.
Counts include source comments/blank lines, not executable statements; file classification is deterministic, but architectural judgments remain human-reviewed.
