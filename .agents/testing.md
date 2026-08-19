# Testing playbook

Load this playbook for tests, bug fixes, compatibility work, fuzzing, process
boundaries, platform behavior, and any added or changed Mermaid diagram. A test is
evidence only when a plausible defect can make it fail.

## Failure-first proof

- When feasible, a bug fix MUST first prove its regression test fails on the unfixed
  code and then passes with the fix. If that proof is infeasible, record the exact
  platform, environment, or historical limitation instead of implying it ran.
- Every test MUST use an independent oracle: exact wire bytes, typed error and code,
  exit status or signal, OS-visible effect, upstream behavior, or a specified state
  invariant. Reimplementing the production algorithm in the test is not independent.
- For protocol, permission, process-lifetime, and other critical behavior, the author
  MUST make a temporary negative-control mutation (for example, remove or invert the
  guarded branch or alter a wire constant) and identify the test that fails.
- A test MUST fail when the behavior under test is deleted or replaced with a no-op or
  constant result. Strengthen or remove a test that survives that change.

## Cases and test shape

- Cover zero, maximum, maximum plus one, truncation or split boundaries, shutdown,
  mismatch, malformed encoding, and invalid input when they apply. Add cancellation,
  restart, missing-file, or invalid-name cases when those are part of the contract.
- Assertion-free tests, mock-only proof of OS or process behavior, and tests that only
  prove a stub, derive, or fixture MUST NOT ship.
- Tests MUST await observable conditions, bind port `0`, use isolated temporary paths,
  and clean up resources. They MUST NOT use sleeps for synchronization; a timeout is
  only a kill switch.
- Crash, lifetime, and hostile-shutdown tests MUST run the risky action in a child
  process and assert the relevant stdout, stderr, exit code, signal, cleanup, and next
  successful operation.
- Every fuzz failure MUST retain its minimized input or seed and exact target. It MUST
  also become a fast deterministic regression with a semantic assertion; a corpus
  entry alone is insufficient.
- Platform code MUST be verified against the real OS API on each claimed platform or
  be reported unverified there. A model or mock may prove state logic, not platform
  behavior.

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

- Root `AGENTS.md` § Documentation diagrams owns diagram selection, accessibility,
  semantic labels and the shared `classDef` palette. A changed diagram MUST preserve
  explicit current/target and framework/showcase meaning in its labels and surrounding
  prose; color or layout alone is not an oracle.
- Run `just mermaid-test` and `just mermaid-check` for every diagram change. These prove
  the repository validator and structural policy; they do not replace the actual render
  required below.
- Before using syntax the author has not already rendered in this repository, the author
  SHOULD use Context7 when available to locate current material and MUST confirm the
  syntax in the current official Mermaid docs. Context7 is discovery; official Mermaid
  docs are the primary syntax authority and remain sufficient when the connector is absent.
- Every added or changed Mermaid block MUST pass `just mermaid-render-check`. It uses the
  official [`@mermaid-js/mermaid-cli`](https://github.com/mermaid-js/mermaid-cli)
  11.16.0 GHCR image pinned by immutable OCI digest, with the checkout read-only,
  network disabled and resource limits. `latest`, beta/canary builds, third-party live
  editors and an unversioned global `mmdc` MUST NOT satisfy the gate. Changing the image
  tag/digest or render config is a dependency + CI review gate.
- A passing render means every changed block exits successfully, produces a non-empty
  output, and preserves an accessible SVG `<title>` and `<desc>` derived from
  `accTitle`/`accDescr`. Inspect the rendered relationship at least once; parse success
  cannot detect a reversed edge, misleading grouping or clipped semantic label.
- The PR or hand-off MUST contain an actual render report: source files and block count,
  renderer name, version and digest, exact command, output format, and observed pass/fail.
  Temporary render output SHOULD live outside the repository and MUST NOT be committed
  unless it is itself a reviewed documentation artifact. If rendering cannot run, report
  the blocker and do not call the diagram change verified.

## CI tiers

- **Every PR, always-created workflow:** security scanning runs for every changed byte.
  The repository-owned change router then schedules each non-security CI lane at the
  job boundary from the observable contract and its inputs. It MUST NOT use
  workflow-level path filters for required workflows: those leave a required check
  pending. A skipped job is permitted only when it reports success and a falsifiable
  router test covers that input class. Job-level `if` must not use `matrix` (GitHub
  evaluates it before expansion). Unknown/shared/build-graph inputs run
  every potentially affected lane, including Ubuntu WebKitGTK apt. Workflow/router
  edits still create every job (GUI smoke owns live apt) but must not duplicate
  `apt-get update` onto Ubuntu clippy and MSRV. No filename heuristic may silently
  skip a proof.
- **Rust-affecting PRs:** CI runs `cargo fmt --all --check` plus clippy, tests and MSRV
  checks for the changed workspace package and every Cargo reverse-dependent consumer.
  MSRV is a rustc-version gate on macOS (WKWebView, no apt). Ubuntu WebKitGTK apt for
  clippy/test runs only when changed paths own `keld-wv` / `keld-core` / `keld-host` (or
  an unknown/lockfile fail-safe). Other Ubuntu clippy uses packages whose Cargo closure
  does not compile `keld-wv`. The Xvfb smoke separately runs when the current `keld-host`
  dependency closure or graphical build/runtime inputs change. Replay
  committed fuzz regressions as normal tests; build changed fuzz harnesses. A
  documentation-affecting PR runs the generated docs and Mermaid render gates. Agents
  still run the root `AGENTS.md` local verification gate before claiming their work done.
- **Nightly:** run bounded `cargo-fuzz` campaigns with retained corpora. Add targeted
  sanitizer or Miri lanes only where unsafe, FFI, allocation, or lifetime risk makes
  them applicable; print replayable seeds and promote every failure to a regression.
- **Weekly:** run the broader supported OS/architecture/backend matrix, longer fuzz
  campaigns and corpus minimization, and real webview/transport process-failure smoke.

## YAGNI

Start fuzzing with `cargo-fuzz` and raw bytes. Do not add `proptest` until a concrete
invariant has interacting input dimensions that example tables cannot cover. Do not
add Loom until a real shared-state concurrency bug or queue/credit/cancellation
invariant requires schedule exploration. Framework presence, test count, and coverage
percentages are not proof by themselves.
