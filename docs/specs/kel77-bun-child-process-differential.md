# Spec: package-agnostic Bun differential harness — child-process lifecycle (KEL-77)

Status: approved
Linear: KEL-77 · Owner: GYLDLAB · Updated: 2026-08-19 (T2: evidence round-trip after KEL-74 merge `8ff4cd6`)

## 1. Goal & non-goals

Keld ships a pinned Bun and supervises Bun children (architecture 06 §1), and the
stated compat plan is "Bun's Node-compat is the compat plan". Today that plan has no
measurement: nothing in the repository can answer "does Bun's `child_process` lifecycle
actually behave the way the Node contract a package was written against says it does?"
This spec adds the smallest harness that answers it for **one** operation family —
child-process lifecycle — by running the same committed fixture corpus under a Node arm
and a Bun arm, comparing each observation to a cited Node documentation sentence, and
emitting one `keld.compat.evidence/v1` record per cell that pins the Bun revision,
fixture-corpus digest, OS/architecture, operation oracle and a pass/fail/unknown verdict.
The observable outcome is a `cargo nextest` gate that fails when either runtime drifts
from its committed baseline, plus a machine-readable record set on disk.

Non-goals:

- The N-API async-lifecycle and libuv thread-primitive families. One family only.
- VS Code, its extension host, PersistentProtocol framing, any marketplace artifact, or
  any package-named fixture. This slice is the "extract package-agnostic Bun semantic
  fixtures into the general runtime suite" bullet of KEL-77 and nothing else.
- A second evidence schema, scorer, denominator, or percentage. `keld.compat.evidence/v1`
  is owned by KEL-74; this harness is a producer only.
- Performance numbers of any kind. Semantic parity first (KEL-77 acceptance).
- Any change to `keld-guard`, kipc frames, the permission model, or CI workflow files.
- Windows and Linux *execution*. The harness is written to be OS-portable and records
  the platform it ran on; this slice claims macOS arm64 results only.

## 2. Spec refs

- `docs/architecture/06-runtime-and-tooling.md` §1 (Bun as a versioned process contract;
  pinned per release; "Bun's Node-compat is the compat plan") and §1.1 (host-owned role
  lifecycle — the supervisor semantics this family underpins).
- `docs/architecture/01-overview.md` §2 (host owns OS; JS owns the app).
- `.agents/testing.md` (independent oracle; negative controls; subprocess proof for
  lifetime/teardown; no sleep-sync).
- `docs/specs/kel74-compat-evidence-schema.md` — the record format consumed here. This
  spec does not modify it; §10 raises the one enum gap for its owner.
- No architecture deviation. `No boundary change` to the runtime's public contract: this
  slice adds tests and fixtures only, no shipped API.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given the committed fixture corpus and both runtimes on `PATH`, when the harness runs,
   then it produces exactly one observation per (case × arm) for all 6 cases × 2 arms, and
   a missing `bun` or `node` binary is a hard test failure naming the missing binary — never
   a skip or a silent pass.
2. Given the `exit-code-propagation` case, when either arm runs it, then the `'exit'` event
   reports `code == 7` and `signal == null`, and `subprocess.exitCode == 7`. Node oracle:
   "If the process exited, `code` is the final exit code of the process, otherwise `null`."
3. Given the `signal-termination` case, when either arm runs it, then `'exit'` reports
   `code == null` and `signal == "SIGTERM"`, and `subprocess.signalCode == "SIGTERM"`.
   Node oracle: "If the process terminated due to receipt of a signal, `signal` is the
   string name of the signal, otherwise `null`. One of the two will always be non-`null`."
4. Given the `close-after-exit` case, when either arm runs it, then the observed event
   sequence has `'close'` strictly after `'exit'`. Node oracle: "The `'close'` event will
   always emit after `'exit'` was already emitted, or `'error'` if the child process
   failed to spawn."
5. Given the `spawn-failure-order` case (spawning a path that does not exist), when either
   arm runs it, then `'error'` is emitted with `code == "ENOENT"`, `'close'` is emitted
   after `'error'`, and `'exit'` is never emitted.
6. Given the `kill-after-exit` case, when the **Node** arm runs it, then
   `subprocess.kill()` returns `false` and `subprocess.killed` is `false` while a raw
   `process.kill(pid, 0)` throws `ESRCH`; when the **Bun** arm runs it at the baselined
   revision, then `subprocess.kill()` returns `true` and `subprocess.killed` is `true`
   with the same `ESRCH`, and the emitted record for the Bun arm carries
   `result: "fail"` against oracle `nodejs.child_process.subprocess-kill-return`.
7. Given the `stdout-flush-on-abrupt-exit` case, when both arms run it, then the observed
   byte counts are recorded and the emitted records carry `result: "unknown"` for both
   arms, because `process.exit()` is documented to discard pending asynchronous stdout.
   An observed divergence on an unspecified path MUST NOT be recorded as `fail`.
8. Given the `kill-after-exit` case, when the Bun arm stops exhibiting the divergence in
   acceptance §3.6 (i.e. upstream fixed it), then the test fails with a message naming the
   case, the previous observation, the new one, and the required follow-up: update the
   pinned expectation and flip the record verdict to `pass` in the same PR. A silent
   absorption of an upstream fix is a harness defect.
9. Given any arm, when the harness runs, then the exact `--revision` / `--version` string
   of that arm is recorded in every record it produces, so a verdict is never readable
   without the revision it was measured against.
10. Given the emitted record set, when it is inspected, then every record is a
    `keld.compat.evidence/v1` object whose `artifact.sha256` is the corpus digest defined
    in §4.3, whose `artifact.platform`/`arch` are the real host values, whose
    `revisions.bun` is the exact `bun --revision` string, whose `operation.oracle.id` and
    `revision` name the cited Node documentation, and whose `result` is one of
    `pass`/`fail`/`unknown`. `waived` is never emitted by this harness. Validation is
    `keld_compat::evidence::parse_evidence` (KEL-74), not a shape-only assertion. The
    harness does not call `score()` and does not mint a product percent — this corpus is
    showcase / non-product; extras cannot shrink N; `complete` needs n>0; the documented
    product corpus list is empty.
11. Error case: given a fixture file mutated on disk, when the corpus digest is
    recomputed, then it differs from the digest of the committed corpus, so a record can
    never silently describe a different corpus than the one that ran.
12. Given the harness's own comparator fed a deliberately-wrong expectation (the committed
    negative-control cell), when verdicts are derived, then it yields `fail` — proving the
    comparator can produce a failing verdict and is not a constant `pass`.

## 4. Design

### 4.1 First-principles and reuse decision

- **Ownership.** Each arm is a separate OS process tree that the Rust test owns and reaps.
  The harness mints no principal, opens no socket, and touches no `keld-guard` state. The
  fixture corpus is repository-owned and content-addressed; the record set is run-owned
  output written under `target/`, never committed.
- **Process/lifecycle.** The measured subject is the *parent* side of `child_process`.
  This is not an assumption: the pre-spec probe ran the full parent-runtime × child-runtime
  matrix and both divergences found tracked the parent, not the child. Because Keld's real
  deployment is a Bun parent supervising Bun children (06 §1.1), each arm runs the same
  runtime on both sides and the 2×2 matrix stays out of scope for this slice.
- **Trust.** A runtime's output is untrusted input to the comparator: observations are
  parsed as JSON with an explicit shape, and a fixture that fails to emit its single
  observation line is a case failure, not an absent record.
- **Failure.** The harness distinguishes three states and never collapses them: the
  observation violates a cited doc sentence (`fail`); the observation satisfies it
  (`pass`); the path is documented as unspecified or the revision is unbaselined
  (`unknown`). Collapsing `unknown` into `pass` would manufacture a compatibility claim,
  which is the exact failure mode KEL-74's honesty rules exist to prevent.
- **Reuse before rewrite.**
  - **Evidence format:** reuse `keld.compat.evidence/v1` (KEL-74). Its fields are a 1:1
    fit for KEL-77's required record contents. *Rejected:* a `keld.runtime.evidence/v1`
    schema — a second record format for the same question is an AGENTS.md §3 violation and
    would fork the honesty rules.
  - **Test-driving pattern:** reuse the pattern already proven in
    `crates/keld-compat/tests/electron_lifecycle.rs` — a Rust integration test spawning
    committed JS fixtures, line-oriented stdout oracle, hard `PATH` requirement on the
    runtime, no sleep-sync. *Rejected:* a bespoke JS runner plus a thin Rust wrapper —
    it would move assertions out of the `cargo nextest` gate and make per-case failures
    unattributable.
  - **Oracle:** reuse the published Node documentation as the upstream contract, exactly as
    `.agents/testing.md` requires for equivalent-implementation work ("Conformance or
    differential test against cited upstream behavior"). *Rejected:* using the Node arm's
    live output as the oracle — a mirror, not an oracle: it cannot distinguish "Bun is
    wrong" from "Node changed", and it would make the Node arm unfalsifiable by
    construction.
  - **Hashing (T2).** `keld.compat.evidence/v1` mandates a real `sha256:<64 hex>`
    artifact digest. T2 adds a workspace-pinned `sha2` 0.10 **dev-dependency** on
    `keld-runtime` (RustCrypto; MIT OR Apache-2.0; already in `Cargo.lock` at 0.10.9 via
    `wry`; no first-party hasher exists — `keld-update` is empty). *Rejected:*
    hand-rolling SHA-256 (forbidden duplicate of a primitive); shelling out to `node -e`
    (makes the digest depend on the runtime under test); `git hash-object` (SHA-1 blob
    id, wrong digest); using wry's transitive `sha2` without a direct dep (Cargo forbids
    it). `keld-runtime` also gains a **dev-dependency** on `keld-compat` so tests call
    `parse_evidence` rather than forking a parser.
  - Compatibility fallback: `not required` — no prior runtime-differential harness exists.
  - Performance claim: none made, none retained.

### 4.2 Why this gate stays green today and still detects regressions

An assertion of the form "Bun must satisfy every Node contract" would make `main` red on
landing, because a real Bun defect is already reproduced (§4.4 case 5). An assertion-free
recorder would violate `.agents/testing.md`. The harness therefore gates on two things
that are both true today and both falsifiable:

1. **Contracts both arms currently honor are asserted for both arms, unconditionally**
   (cases 1–4, and the drained variant of case 6). These are stable, cited Node contracts.
   If either runtime regresses on exit codes, signal names, `'close'`-after-`'exit'`
   ordering or spawn-failure ordering, CI goes red. That is a genuine compat regression on
   the runtime Keld ships and should stop a release.
2. **The one reproduced divergence is pinned by an explicit defect assertion** (case 5).
   The Node arm is asserted to satisfy the oracle; the Bun arm is asserted to exhibit the
   *current defect*. When Bun fixes `kill()`, this test goes red and forces the expectation
   and the record verdict to be updated to `pass` in the same PR — the harness notices
   upstream fixes instead of silently carrying a stale claim.
3. **Unspecified paths are never asserted as conformance** (case 6 abrupt variant). The
   observed byte counts are recorded because they are buffer-size dependent and may
   legitimately differ per OS; only the specified drained path is asserted.

Deliberately *not* built: a revision-keyed baseline table. Both arms' revisions float (CI's
`oven-sh/setup-bun` has no version pin and the Node version is whatever the runner ships),
so a revision-keyed table would spend most of its life unbaselined and would emit `unknown`
for nearly every cell — machinery that weakens the gate instead of sharpening it. Revisions
are *recorded* in every record; assertions are oracle-based. (YAGNI, per `.agents/testing.md`.)

The verdict written into the record is always derived from observation-versus-oracle, and
is independent of whether the build is green.

### 4.3 Fixture corpus digest

`artifact.sha256` is `sha256:` + lowercase hex of:

```
SHA256( for each file, ordered by byte-wise relative path ascending:
          relative_path_utf8 || 0x00 || u64_le(file_len) || file_bytes )
```

Relative paths are POSIX-separated and rooted at the corpus directory, so the digest is
identical on Windows. Length framing prevents a rename-plus-content shuffle from colliding.

### 4.4 Cases (the operation family)

Two fixture files form the corpus: a parent driver taking a case id, and one child. Each
case emits exactly one JSON observation line on stdout.

| # | `operation.id` | Dimension | Oracle strength | Cited Node sentence |
|---|---|---|---|---|
| 1 | `child-process.exit-code-propagation` | values | specified | `'exit'`: code is the final exit code, otherwise `null` |
| 2 | `child-process.signal-termination` | crash behavior | specified | `'exit'`: signal is the string name, otherwise `null`; one is always non-`null` |
| 3 | `child-process.close-after-exit` | async callback order | specified | `'close'` "will always emit after `'exit'` was already emitted" |
| 4 | `child-process.spawn-failure-order` | errors + order | specified | `'error'` when the process could not be spawned; `'close'` after `'error'` |
| 5 | `child-process.kill-after-exit` | teardown | specified | `kill()` "returns `true` if `kill(2)` succeeds, and `false` otherwise"; `killed` is set "after `subprocess.kill()` is used to *successfully* send a signal" |
| 6 | `child-process.stdout-flush-on-abrupt-exit` | cleanup | **unspecified** | `process.exit()` is documented to exit "even if there are still asynchronous operations pending, including I/O operations to `process.stdout`" |

Case 5 carries its own in-fixture control: it also performs a raw `process.kill(pid, 0)`
and records the thrown errno, so the record proves the child was truly gone rather than
merely unreaped. Case 6 additionally runs a drained variant (child awaits the documented
write callback before exiting) to demonstrate the divergence disappears on the specified
path — which is why case 6 is `unknown`, not `fail`.

### 4.5 Record mapping onto `keld.compat.evidence/v1`

| Record field | Value |
|---|---|
| `schema` | `keld.compat.evidence/v1` |
| `artifact.sha256` | corpus digest per §4.3 |
| `artifact.platform` / `arch` | real host values |
| `revisions.keld` | `git rev-parse HEAD` of the worktree |
| `revisions.bun` | exact `bun --revision` output (e.g. `1.3.14+0d9b296af`) |
| `revisions.engine` | the arm identity — `node-v26.7.0` or `bun-1.3.14+0d9b296af` |
| `authority_profile` | `legacy_sandbox_off` |
| `operation.id` | §4.4 table |
| `operation.kind` | `primary_workflow` (see §10) |
| `operation.oracle.{id,revision}` | `nodejs.child_process.<observable>` + the documentation version read |
| `result` | derived `pass` / `fail` / `unknown` |
| `evidence_uri` | `sha256:` of the run's raw observation report |

`authority_profile` is deliberately **not** `strict_bun`. The harness runs bare runtimes
from a test process with ambient OS authority; labelling it `strict_bun` would claim the
measurement was taken under Keld's zero-ambient-authority profile, which is false.

The record set references the raw observation report by content hash, so both arms' raw
observations are retained and addressable without embedding them in every record.

### 4.6 T1 hold, T2 mapping

T1 computed every datum a record needs and printed a differential report because KEL-74's
schema was still in flight. KEL-74 merged as `8ff4cd61bd7776fdc6096864d61ec343975a131f`.
T2 maps that report onto the frozen schema and validates it with
`keld_compat::evidence::parse_evidence`. No new measurement; no product `score()` panel.

- New/changed types & channels: none shipped. All new code is test-only.
- Capabilities required; manifest changes (spec 03): none.
- Wire/protocol changes (spec 02): none.
- Platform notes: mac — the slice's only executed platform. win — the corpus avoids
  POSIX-only assumptions except case 2, which uses `SIGTERM`; Windows signal semantics
  differ and the case is expected to need a Windows-specific expectation before that
  platform is claimed. linux — expected to work unmodified; unverified here.

## 5. Boundaries

- Implement in: `crates/keld-runtime/tests/child_process_differential.rs`,
  `crates/keld-runtime/fixtures/child-process/` (2 fixture files),
  `crates/keld-runtime/Cargo.toml` (dev-dependencies: `serde_json`, `keld-compat`,
  `sha2`, `tempfile`), workspace `Cargo.toml` (`sha2` pin + review comment),
  `docs/specs/kel77-bun-child-process-differential.md`, `docs/agents/learnings.md`.
- Must not touch: `.github/workflows/*` (single-writer), `crates/keld-guard/**`,
  `crates/keld-ipc/**`, `crates/keld-compat/src/**` (KEL-74 owns `evidence.rs`; this
  harness is a producer only), `packages/@keld/electron/**`, `docs/research/**`,
  `docs/architecture/*` (no deviation), any other agent's worktree or branch.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1 Fixture corpus + Rust differential harness + 6 cases + oracle comparison +
      executed negative controls. Acceptance §3.1–§3.9 and §3.12. **Done.**
- [x] T2 Corpus digest (§4.3), `keld.compat.evidence/v1` serialization (§4.5), and
      validation through `keld_compat::evidence::parse_evidence`. Acceptance §3.10–§3.11.
      Adds the `sha2` workspace pin (dev-only on `keld-runtime`) and its review gate.
      Does **not** call `score()` or mint a product percent.
- [ ] T3 (follow-up issue) Windows and Linux execution + per-platform expectations.
- [ ] T4 (follow-up issue) Upstream a minimized `kill()`-after-exit reproducer to
      `oven-sh/bun` as a general fixture, and link the issue from the record.

## 7. Test plan

| Acceptance | Test | Oracle |
|---|---|---|
| 3.1 | `every_case_produces_one_observation_per_arm` | exact count; missing-binary panic message |
| 3.2–3.5 | `specified_contracts_hold_on_both_arms` | exact event tuples vs cited Node sentences |
| 3.6 | `node_kill_after_exit_satisfies_the_oracle` + `bun_kill_after_exit_is_the_pinned_defect` | `kill()`/`killed`/`ESRCH` triple; record `result` |
| 3.7 | `abrupt_exit_flush_is_unknown_and_the_drained_path_is_lossless` | both records `unknown`; drained variant equal on both arms |
| 3.8 | `node_kill_after_exit_satisfies_the_oracle` + `bun_kill_after_exit_is_the_pinned_defect` (same tests) | pinned defect expectation + remediation message |
| 3.9 | `differential_report_pins_revision_platform_and_oracle_for_every_cell` | non-empty revision / platform / arch / oracle per cell |
| 3.10 | `emitted_records_parse_via_keld_compat` | the real `parse_evidence`, not a shape assertion; `runtime_semantics` kind is `KELD-COMPAT-005` |
| 3.11 | `corpus_digest_changes_when_a_fixture_changes` | digest over a temp corpus copy with one byte flipped |
| 3.12 | `comparator_can_emit_fail` (negative control) | deliberately-wrong expectation ⇒ `fail` |

Anti-flake: no sleeps — case 2 kills only after the child's `READY` line is observed, and
every case is driven by awaiting `'close'`. No network, no ports. Records are written to a
per-run `tempfile::tempdir()` (OS temp; dropped when the test ends), never a fixed
`target/` or crate-relative path. Case 6 asserts only the
byte counts that were shown deterministic across repeated runs on the measured platform, and
its verdict is `unknown` regardless.

Manual negative controls (per `.agents/testing.md`, to be executed and reported in the PR):
inverting the `'close'`-after-`'exit'` ordering check must fail `close_after_exit`;
replacing the derived verdict with a constant `pass` must fail `comparator_can_emit_fail`;
removing the length framing from §4.3 must fail `corpus_digest_changes_when_a_fixture_changes`.

## 8. Review gates triggered

1. unsafe: none.
2. Public API: **none** — all new code is test-only; no `pub` item is added to
   `keld-runtime`'s shipped surface.
3. Permission model: none.
4. Dependency: **`sha2` 0.10** is added to `[workspace.dependencies]` and as a
   `keld-runtime` **dev-dependency** (see workspace `Cargo.toml` review comment).
   Purpose: corpus digest + observation-report `evidence_uri`. Alternatives rejected
   in §4.1. `keld-compat` and `tempfile` are already workspace-pinned; T2 adds
   dev-only edges from `keld-runtime` to both. No new package enters the lockfile
   for `sha2` (already present via wry at 0.10.9); the lockfile gains a direct
   `keld-runtime` → `sha2` edge.
5. Wire protocol: none for kipc. This harness *emits* KEL-74's versioned JSON document
   format without defining or modifying it.

## 9. Perf impact

None on any shipped path — no code ships. Harness cost is 12 short-lived process pairs in
the test suite; it is not on the kipc, event-loop or guard hot paths and makes no
performance claim about either runtime.

## 10. Open questions

Blocking human decisions before Status: approved.

1. ~~**`operation.kind` enum gap (coordination with KEL-74).**~~ **Resolved 2026-08-19
   (human):** emit `primary_workflow` as the least-wrong existing value and leave the
   coordination note on KEL-74 for its owner to decide. This issue does not fork the enum.
   The value is a single named constant in the harness so a later switch is one line.
2. ~~**T2 is blocked, deliberately.**~~ **Resolved 2026-08-19:** KEL-74 merged as
   `8ff4cd6`. T2 serializes through `parse_evidence` on that freeze. `operation.kind`
   remains `primary_workflow` (named constant). No `runtime_semantics` fork.
3. **Baseline versus pinned Bun.** CI's `oven-sh/setup-bun` has no version pin, so the CI
   Bun revision floats and will periodically become unbaselined (acceptance §3.9 makes that
   `unknown`, not red). Pinning Bun in CI would make the gate sharper but edits a
   single-writer CI workflow file, which this agent must not do. Confirm whether to open a
   separate human-owned issue for the pin.
