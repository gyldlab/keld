# Testing playbook

Load for tests, bug fixes, compatibility, fuzzing, process boundaries, platform
behavior and changed Mermaid diagrams. A plausible defect must falsify the test.

## Failure-first proof

- Root `AGENTS.md` § Atomic problem-solving protocol owns the decomposition. Before
  selecting a regression, conformance, benchmark, or negative control, the author MUST
  bind it to one named atom's observable contract and state why its oracle is independent
  of the implementation and the other atoms. Every negative control MUST name the one
  fault or mutation that falsifies that atom; one atom's pass MUST NOT stand in for proof
  of another.
- When feasible, a bug fix MUST first prove its regression test fails on the unfixed
  code and then passes with the fix. If that proof is infeasible, record the exact
  platform, environment, or historical limitation instead of implying it ran.
- Every test MUST use an independent oracle: exact wire bytes, typed error and code,
  exit status or signal, OS-visible effect, upstream behavior, or a specified state
  invariant. Reimplementing the production algorithm in the test is not independent.
- For protocol, permission, process-lifetime, and other critical behavior, the author
  MUST make a temporary negative-control mutation and identify the test that fails.
- A test MUST fail when the behavior under test is deleted or replaced with a no-op or
  constant result. Strengthen or remove a test that survives that change.

## Cases and test shape

- Cover applicable zero, maximum, maximum+1, truncation/splits, shutdown, mismatch,
  malformed encoding and invalid input; cancellation, restart, missing-file and
  invalid-name cases when contracted.
- Assertion-free tests, mock-only OS/process proof, and tests proving only a stub,
  derive or fixture MUST NOT ship.
- Tests MUST await observable conditions, bind port `0`, isolate temporary paths and
  clean up. No sleep synchronization; timeouts are kill switches. Explain non-obvious
  wait/resource boundaries by the real condition, not timing.
- Crash, lifetime and hostile-shutdown actions MUST run in a child; assert relevant
  stdout, stderr, exit code, signal, cleanup and the next successful operation.
- Every fuzz failure MUST retain its minimized input/seed and exact target, and become
  a fast deterministic semantic regression; a corpus entry alone is insufficient.
- Verify real OS APIs on every claimed platform or report unverified. Models/mocks
  prove state logic, not platform behavior.

## Source organization

- Keep tests with their owning module/fixture; group by observable contract, not length.
  Split mixed concerns without empty mirrored OS trees.
- Agents SHOULD read the scenario and its actual fixture/helper/resource dependencies.
  Measure this working set before/after a structural change, not just file lengths.
- Separate scenarios, setup and independent observations. Keep resource types with
  cleanup; share only equivalent mechanics with named consumers. No giant `common.rs`,
  universal harness, cycles or wildcard-export maze.
- A split MUST preserve executable boundaries unless separately justified. Compare
  native discovered cases, cfg/features and ignored reasons; map renames and update
  exact helper selectors, fixtures, runner groups and scripts together. Prove helper
  execution by its effect: exit zero may mean zero selected tests.
- Move first, deduplicate later. Preserve assertions, negative controls, resource/drop
  order and real-OS proof.

## Cross-runtime migration cases

For a changed runtime/FFI seam, select applicable approved-contract cases and reuse
existing regressions; source presence is not run evidence.

- Distinguish absent/null/empty/zero/false, numeric bounds/fractions, encoding, errors,
  callback/event order and sync/async behavior; name config/environment/clock capture.
- Exercise callers, handlers, callbacks, generated and stored-value consumers, not
  only imports/declarations. Stateful comparisons use separate disposable resources.
- Separate cancellation before admission, during work, after commit and after reply.
  Rejection proves neither stopped work nor released resources. Do not claim rollback
  or replay an ambiguous effect without the accepted idempotency/no-effect contract.
- Observe callback quiescence, stale-generation rejection, one release per reservation,
  aggregate retained work and a healthy follow-up; wrapper tests prove only wrapper state.
- Complete-app criteria require actual installed callers, permitted/denied operations
  and relevant recovery/cleanup on each claimed OS, not a HELLO/window/readiness token.

## Taxonomy

| Surface | Default proof |
|---|---|
| Pure state, parser, codec, or policy | Unit/contract test with exact values, boundaries, malformed input, and typed failures |
| Filesystem, socket, IPC, CLI, or supervisor | Integration test using real temporary resources and executables |
| Electron compatibility or equivalent implementations | Conformance or differential test against cited upstream behavior |
| Crash, teardown, restart, or lifetime | Isolated subprocess test with status/signal and cleanup assertions |
| Hostile input | `cargo-fuzz` raw-byte target plus minimized deterministic regressions |
| Webview or other platform binding | Pure state-model test plus real-OS subprocess smoke |

## Documentation and Mermaid render gate

- [`.agents/docs.md`](docs.md) owns diagram selection, accessibility, semantic labels
  and `classDef` palette. Changes MUST retain explicit current/target and
  framework/showcase meaning in labels and prose; color/layout is not an oracle.
- Every diagram change MUST run `just mermaid-test` and `just mermaid-check` for
  validator/structural proof; neither replaces rendering.
- Before using unfamiliar Mermaid syntax, apply
  [`.agents/research.md` § Current-documentation receipt](research.md#current-documentation-receipt).
  The official Mermaid docs are the primary syntax authority; Context7 remains discovery.
- Every changed/added block MUST pass `just mermaid-render-check`: official
  [`@mermaid-js/mermaid-cli`](https://github.com/mermaid-js/mermaid-cli) 11.16.0 GHCR
  image at immutable OCI digest, read-only checkout, disabled network and resource
  limits. No `latest`, beta/canary, live third-party editor or unversioned global `mmdc`.
  Tag/digest/render-config changes trigger dependency and CI review.
- Each block MUST exit successfully, produce non-empty output and accessible SVG
  `<title>`/`<desc>` from `accTitle`/`accDescr`. Inspect the rendered relationship;
  parsing misses reversed edges, misleading grouping and clipped semantic labels.
- PR/handoff MUST report files/block count, renderer name/version/digest, exact command,
  output format and observed pass/fail. Temporary output SHOULD use managed scratch;
  commit only reviewed documentation artifacts. Report blocked rendering as unverified.

## CI tiers

- [`.agents/ci.md`](ci.md) owns routing, required workflows, apt, MSRV and merge
  admission; this playbook owns lane evidence.
- **Rust-affecting PRs:** format, clippy, test and MSRV cover changed packages and Cargo
  reverse dependents. Replay committed fuzz regressions; build changed fuzz harnesses.
  Docs changes run generated-doc/Mermaid gates.
- **Nightly:** bounded `cargo-fuzz` with retained corpora; targeted sanitizer/Miri for
  applicable unsafe, FFI, allocation or lifetime risks. Print replayable seeds; promote
  every failure to a regression.
- **Weekly:** broader supported OS/architecture/backend matrix, longer fuzz campaigns,
  corpus minimization and real webview/transport process-failure smoke.

## YAGNI

Start fuzzing with `cargo-fuzz` and raw bytes. Do not add `proptest` until a concrete
invariant has interacting input dimensions that example tables cannot cover. Do not
add Loom until a real shared-state concurrency bug or queue/credit/cancellation
invariant requires schedule exploration. Frameworks, test counts and coverage percentages alone are not proof.
