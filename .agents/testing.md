# Testing playbook

Load this playbook for tests, bug fixes, compatibility work, fuzzing, process
boundaries, and platform behavior. A test is evidence only when a plausible defect can
make it fail.

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

## CI tiers

- **Every PR:** `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo nextest run --workspace --profile ci`. Replay committed fuzz regressions as
  normal tests; build changed fuzz harnesses.
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
