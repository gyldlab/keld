# Spec: Contract-oriented test layout
Status: draft
Linear: KEL-252 · Owner: Keld maintainers · Updated: 2026-09-25

This is a source-organization proposal, not an implemented Rust migration or a
claim of new platform coverage. Approval precedes the migration tasks below.

## 1. Goal & non-goals

Make a failed contract discoverable without loading unrelated acceptance scenarios,
while preserving everything that makes the existing tests meaningful. Optimize the
smallest *diagnostic working set*: scenario, fixture, independent observation,
resource owner and governing contract. File length alone is not that working set.

No production-boundary, permission, wire, dependency or test-runner change. No
universal harness crate, custom test DSL, repository-wide supersuite, compulsory
property-testing framework, blanket line-count gate or cosmetic folder explosion.
No retry, weakened assertion, reduced native coverage or silent manual-test removal.

## 2. Spec refs

- [Overview](../architecture/01-overview.md), sections 1–4: process/trust and ownership.
- [Runtime and tooling](../architecture/06-runtime-and-tooling.md), sections 1–2:
  supervised roles and the shipping CLI chain.
- [KEL-96](kel96-no-flag-host-boot.md), sections 3–4 and 7: native boot/session proof.
- [KEL-135](kel135-persistent-profile-identity.md): authenticated profile identity.
- [Host invariants](../../crates/keld-host/AGENTS.md): independent native observation.
- [Testing playbook](../../.agents/testing.md): normative test-authoring policy.
- [Workflow](../agents/workflow.md): claims, approval, review and actual acceptance.

### Observed baseline

Inspected `gyldlab/keld@14aeb4783b8ce70d55cfa7dcba98522d91c9e8d0` on 2026-09-25.
Counts below are physical source lines and UTF-8 bytes, not test execution or tokens.
Reproduce with `wc -lc crates/keld-host/tests/no_flag_*.rs` at that revision.

| File under `crates/keld-host/tests/` | Lines | Bytes | Different responsibilities present |
| --- | ---: | ---: | --- |
| `no_flag_macos.rs` | 7,429 | 295,143 | Boot admission, recovery, descriptor/window observation, signed profiles, media, second-user and reboot acceptance |
| `no_flag_windows.rs` | 4,134 | 158,910 | Staging ACLs, lifecycle, HANDLE observation, signed storage/media, second-user coordination, fixture servers |
| `no_flag_linux.rs` | 968 | 38,544 | Staging, boot, recovery, CLI/host death, strict-process identity and fixture setup |
| Total | 12,531 | 492,597 | Not a duplication measurement |

The macOS profile/signing/media section begins around line 3678; the file is not
merely one long boot test. Existing Swift, TypeScript, JavaScript and shell fixtures
already live in `tests/fixtures/`; reuse them rather than recreating them in Rust.
Other integration-source hotspots include `keld-native/tests/macos/retained_fs_acceptance.rs`
(1,767 lines), `keld-runtime/tests/window_bound_contract.rs` (1,596), and
`keld-cli/tests/bun_echo.rs` (1,062). These are follow-up candidates, not automatic
refactor mandates. This inspection is not a full audit of every Rust unit test.

### Upstream comparison and decision

Source inspections below are a representative seven-repository sample, not a claim
to have audited every large Rust repository. Each link is pinned to the inspected
revision; retrieval date is 2026-09-25. No upstream suite was executed for this study.

| Source and observed mechanism | Keld decision | Limit / rejected transfer |
| --- | --- | --- |
| [Bun test guide][bun-guide] organizes API, CLI, bundler and regression tests; [harness][bun-harness] centralizes utilities (2,367 physical lines at this pin). | Reuse domain-first navigation and shared proven mechanics. | Do not reproduce a giant all-purpose harness; a shared file can itself overload context. |
| [Cargo testsuite entry][cargo-entry] declares focused modules; [contributor guide][cargo-guide] separates temporary-project setup and command/output assertions. | Keep small executable entrypoints with contract modules; isolate test-private resources. | Cargo's support crate serves Cargo's own scale and semantics; do not add it or imitate its entire framework. |
| [rustc categories][rustc-categories] and [compiler test guide][rustc-guide] organize by test purpose with local fixtures and suite-specific expectations. | Keep contract ownership and nearby minimal failure fixtures discoverable. | Compiler diagnostics/snapshot machinery is not native process/window proof; no compiletest adoption. |
| [Tokio process case][tokio-process] is a focused process contract; its [support directory][tokio-support] separates narrow mechanics. | Learn narrow source ownership and explicit platform conditions. | The inspected case uses timed sleeps and an unavailable-shell early return. Keld's existing event-based/no-skip-green acceptance rules remain stronger requirements. |
| [Deno integration entry][deno-entry] groups command/domain modules; [integration manifest][deno-manifest] uses a separate [test utility library][deno-util]. | Separate scenarios from reusable setup and server mechanics. | A shared crate is justified only by actual cross-crate consumers and a demonstrated unmet need, not resemblance to Deno. |
| [Tauri ACL fixtures][tauri-acl] load capability/plugin fixtures and compare resolved ACL snapshots. | Reuse focused capability fixtures in the existing guard owner. | Resolution snapshots cannot establish OS containment or live denial. No extra ACL crate for Keld; an owner already exists. |
| [Electron utility-process specs][electron-process] use real Electron APIs, fixture apps and [focused helpers][electron-support]. | Preserve shipping-runtime tests and explicit window/process cleanup. | An Electron process model is not Keld's Rust/Bun/system-webview model; do not port its harness or assume all its spec files are small. |

[bun-guide]: https://github.com/oven-sh/bun/blob/29d9638da3dd5b498a5b608d3fa02549b0bdddf1/test/README.md
[bun-harness]: https://github.com/oven-sh/bun/blob/29d9638da3dd5b498a5b608d3fa02549b0bdddf1/test/harness.ts
[cargo-entry]: https://github.com/rust-lang/cargo/blob/3d7cf6e937d6127d0f49881bf689c560b36d35c4/tests/testsuite/main.rs
[cargo-guide]: https://github.com/rust-lang/cargo/blob/3d7cf6e937d6127d0f49881bf689c560b36d35c4/doc/contrib/src/tests/writing.md
[rustc-categories]: https://github.com/rust-lang/rust/blob/4e701dc6ba63a40c908a173df269a55e1fd6297e/tests/ui/README.md
[rustc-guide]: https://rustc-dev-guide.rust-lang.org/tests/compiletest.html
[tokio-process]: https://github.com/tokio-rs/tokio/blob/f987088648d9ea95bf091fdd0b05445a40ca4999/tokio/tests/process_kill_on_drop.rs
[tokio-support]: https://github.com/tokio-rs/tokio/tree/f987088648d9ea95bf091fdd0b05445a40ca4999/tokio/tests/support
[deno-entry]: https://github.com/denoland/deno/blob/461f32ec6174d88b7e43654a054d46d67ae2fdab/tests/integration/mod.rs
[deno-manifest]: https://github.com/denoland/deno/blob/461f32ec6174d88b7e43654a054d46d67ae2fdab/tests/integration/Cargo.toml
[deno-util]: https://github.com/denoland/deno/blob/461f32ec6174d88b7e43654a054d46d67ae2fdab/tests/util/lib/Cargo.toml
[tauri-acl]: https://github.com/tauri-apps/tauri/blob/8062864a5e70b97dd738a96dc063f28ce6e0e2e2/crates/tests/acl/src/lib.rs
[electron-process]: https://github.com/electron/electron/blob/cde6f39fb37e5f09d38f9cad449157532705c545/spec/api-utility-process.spec.ts
[electron-support]: https://github.com/electron/electron/tree/cde6f39fb37e5f09d38f9cad449157532705c545/spec/lib

## 3. Acceptance criteria (binary, each becomes a test)

1. On each migrated native OS and applicable feature configuration, every original
   discovered case has exactly one destination. Preserve ignored/manual status and
   its reason. No unaccounted addition, omission or duplicate passes migration review.
2. Existing `no_flag_*` executable target names remain unchanged in the initial
   migration. Runner selectors, test groups, timeouts and concurrency stay equivalent.
3. Every self-invoked helper still executes its intended body: exact selection plus
   an independent handshake/effect, not exit zero alone. Missing selection fails proof.
4. A representative defect is diagnosed from the named scenario and its actual
   dependency closure without requiring unrelated profile/media/OS scenario modules.
   Before/after source-read receipts demonstrate a reduction; no fixed LOC shortcut.
5. Moved assertions, event order, fixture bytes, identities, feature conditions,
   cleanup-on-failure and healthy follow-up retain their meaning. Existing relevant
   negative controls still fail for the same reason, not a harness/build error.
6. Every shared helper has named live consumers with equivalent semantics, one
   resource/policy owner and bounded visibility. No giant replacement support module,
   blanket dead-code/unused-import allowance or dependency cycle is introduced.
7. Real OS results and signed/media/multi-user/reboot prerequisites are reported
   separately. Default CI or compilation is not evidence for a manual qualification.
8. Agent guidance has one routed owner, stays within existing budgets and passes
   representative routing/placement controls; always-loaded chains do not grow.

## 4. Design

### 4.1 First-principles decomposition

No production boundary change. These are test-source ownership decisions.

| Atom / owner | Boundary and failure | Independent observable / current state |
| --- | --- | --- |
| Navigation / scenario module | Failure identifier -> relevant contract and dependencies; mixed unrelated concerns inflate reads. | Source outline and representative read-set comparison. Mixed current responsibilities observed; improved read cost remains to be measured after migration. |
| Registration / existing test target | Source modules -> compiled cases; moves can silently alter selection. | Native runner inventory and exercised helper handshake. Cargo module behavior is documented; Keld post-move parity is unrun. |
| Observation / native probe | OS state -> contract evidence; shared implementation can make circular proof. | OS identities/effects and probe-negative tests. Existing oracles remain owners; extraction is not new proof. |
| Lifetime / fixture resource owner | Acquisition -> release on normal/error/unwind paths; field or scope changes can alter Drop order. | Native cleanup and relaunch plus existing failure controls. No ownership rewrite in the mechanical move. |
| Instruction routing / testing playbook | Task -> minimal instructions; extra always context or missing route defeats the goal. | Byte/token receipt, route controls and task trace. Root/nested chains remain unchanged. |

Navigation and registration are coupled: changing a Rust module path can change a
libtest name. Observation and lifetime are coupled: cleanup must not erase the
resource before the observation. Explicitly verify both edges rather than assuming
independence from a passing test total.

### 4.2 Source modules are not executable boundaries

[Cargo's target reference](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#integration-tests)
documents a separate executable for each integration-test target and permits a
single target composed of multiple modules. Keep the existing native targets at
first. Use an explicit path declaration in each thin root, for example
`#[path = "no_flag_macos/mod.rs"] mod suite;`, so the root file is not mistaken for
its own module. The module root contains declarations, not scenario implementations.

Proposed macOS layout (create only modules actually populated by a migration):

```text
crates/keld-host/tests/
  README.md
  host_flags.rs
  no_flag_macos.rs             # cfg/lints, suite declaration, required helper entrypoints
  no_flag_macos/
    mod.rs                    # module declarations only
    boot_admission.rs         # invalid boot/policy, resource-free rejection
    session_lifecycle.rs      # shipping launch, lease, ordered shutdown
    recovery.rs               # generations, link loss, crash breaker
    descriptor_census.rs      # FD attribution scenarios and observer controls
    profiles/
      mod.rs
      identity.rs
      storage.rs
      purge.rs
      cross_user.rs
      crash_reboot.rs
    media/
      mod.rs
      restart.rs
      ephemeral.rs
      query.rs
      evidence.rs             # media oracle and its own counterexamples
    support/
      mod.rs
      stage.rs
      control.rs
      process.rs
      unix_descriptors.rs
      native_window.rs
      signed_app.rs
      profile_origin.rs
  no_flag_windows.rs          # existing target; equivalent contract-specific directory
  no_flag_linux.rs            # existing target; smaller, fewer modules are appropriate
  fixtures/                   # existing TS/JS/Swift/shell assets; no second fixture tree
```

Windows initially separates staging, lifecycle/recovery, handle census, storage,
second-user and media contracts; Linux needs only staging/admission, lifecycle,
recovery and its smaller support set. Do not create empty mirrored directories.
Do not move all profile code into one new 3,000-line `profiles.rs`.
Tests elsewhere remain under their owning crate: codec/guard logic near its owner,
CLI staging in CLI tests, runtime supervision in runtime tests, and real shipping
cross-component acceptance in host tests. Keep the few full-chain acceptance
scenarios even when lower-level cases cover individual components.

### 4.3 Mechanics, observations and resource ownership

Scenario modules express the input, action and expected contract. Setup helpers
construct temporary stages/apps. Observation helpers report independently observed
OS facts. Resource-owning types retain acquisition, explicit finish and Drop fallback
in the same module. Do not scatter a struct and its cleanup across unrelated files.
Retain existing `LiveCycle`, `RecoveryCycle`, `ShippingDevCycle`, process guards and
observers when moving; do not create a new universal process supervisor for tests.

The dependency direction is scenarios -> narrow support -> OS/test resources or the
public production API being exercised. Support does not import scenarios. Oracle
calculation must not import the production decision it is supposed to validate.
Prefer private items and explicit imports; use the narrowest necessary ancestor
visibility for sibling access. Broad re-exports or `use super::*` across the whole
suite are not an acceptable way to hide unchanged coupling.

Share across OS targets only after proving identical semantics and at least two live
consumers. Windows retained HANDLE identity, Linux PID/start-time identity and macOS
process-group/descriptor observation are not interchangeable implementations of a
single generic `kill_process` helper. Shared fixture bytes are reusable; OS authority
and observation rules remain OS-owned. Do not copy production wire constants into a
second policy implementation; distinguish generating valid input from independently
checking the contract and its approved literal values.

Readiness uses observable events, not sleeps. Extracted servers preserve current
request/byte/time bounds, cancellation, output draining and shutdown. A cleanup
failure cannot become a successful test. Diagnostics retain the failing contract,
phase, expected/observed evidence, toolchain and platform details in managed per-run
artifacts, with secrets and private account/path data excluded from public reports.

### 4.4 Lossless migration, including hidden consumers

Before a move, retain the exact SHA, tool versions, native target, features, runner
configuration, test listings and relevant baseline results. Enumerate references in
CI, nextest, scripts, specs, embedded commands and external operator instructions.
The macOS and Linux helpers are launched with `--exact`; Windows also embeds exact
selection inside a PowerShell command. Moving those tests into a module changes
names even when function bodies are identical. Prefer retaining the small existing
root helper entrypoints; map any intentional scenario rename explicitly and update
all its consumers in that same PR. No registry of hundreds of forwarding tests.

Use native Cargo/libtest and nextest listings, not source-regex counts. Capture the
mapping `old target/name -> new target/name` with cfg/features and ignored reasons
as migration evidence; it is not a second permanent handwritten test registry.
Exercise helper effects to catch a successful zero-test subprocess. Preserve the
`binary(no_flag_windows)` and `binary(no_flag_linux)` group filters currently in
`.config/nextest.toml`; inspect any current macOS scheduling/isolation separately.
Do not infer macOS serialization from a historical issue title.

Move code first, changing only paths/imports/visibility and reviewed name consumers.
Keep assertion values, deadlines, environment capture, resource scope, struct-field
order and embedded fixture bytes unchanged. Fix `include_str!` paths explicitly;
prefer stable manifest-root paths when necessary. A relative-path repair is part of
the move, not permission to alter the embedded program. Keep the old/new source
mapping reviewable. Deduplicate mechanics in a later PR after equivalence evidence.
Do not hide copied chunks behind `include!`, aliases or generated wrapper boilerplate.

### 4.5 Agent working set and future admission

An agent starts from the failed test/contract name and the small local navigation
map, then reads only that scenario and the helper/fixture/resource definitions it
uses. If that dependency closure still spans unrelated domains, the split has not
succeeded. Conversely, fifty tiny files with circular imports are not an improvement.
Keep scenario names descriptive; retain issue IDs as provenance rather than the only
meaningful name. A local README describes domains and commands, not every test body.

The existing testing playbook is the rule owner. Root/nested AGENTS already route to
it; do not paste this design into every crate or require all agents to read this spec.
For new tests, extend the correct existing contract module. For a mixed concern,
create only the needed module and explicit dependency boundary. File size is a review
signal, not evidence of bad code or a universal numeric prohibition.

After the pilot, add the smallest executable discovery/selector regression to the
existing test/tooling owner if needed. A negative control removes a module declaration
or breaks an exact helper selector and must fail the parity/handshake check. Do not
invent a source parser as a substitute for asking the compiled runner. Any broader
CI ratchet needs its own measured baseline, route tests and CI-owner review. The
guidance change alone does not enforce a future layout in CI.

## 5. Boundaries

This design slice edits only this spec, `.agents/testing.md`, and the host-test
README. No root/nested AGENTS, budget manifest, generator, Cargo or CI edits.
Implementation later touches one native suite and its actual consumers per PR.
Shared fixture or runner changes require coordination across all consumers.
PR #287 was active against Windows signed-profile tests at inspection; refresh its
state and KEL-135 claims before that migration. Do not overlap another active writer.
KEL-85 retains negative-chain/fuzz research; KEL-245 retains managed workspace policy.

## 6. Tasks (each one reviewable PR, dependency ordered)

- [ ] T1: Review/approve this spec and the routed guidance; retain instruction receipts.
- [ ] T2: Native Linux pilot: preserve listings/selectors, split admission and lifecycle
  from existing helpers, and exercise real strict-process/window acceptance. Establish
  the smallest executable migration controls before claiming the pilot complete.
- [ ] T3: Native macOS boot/census slice: preserve helper entrypoints, native observation
  and cleanup controls; do not mix signed profile/media relocation into this PR.
- [ ] T4: Native macOS profile/storage slice, then a separate media/reboot slice when
  signing, devices, standard-user and reboot acceptance are available. Each slice is
  independently reviewable and leaves unavailable criteria open.
- [ ] T5: Native Windows slice after KEL-135 overlap resolves: preserve HANDLE/ACL
  semantics and signed/multi-user/media acceptance. Split into further contract PRs
  rather than submitting an unreviewable whole-file move.
- [ ] T6: Compare actual repeated mechanics and diagnostic read sets across migrated
  suites; extract only proven common code, then reassess other hotspot crates.

T3 and T5 may proceed on separate OS owners after the pilot decision, only with
non-overlapping paths. No parallel edits to shared fixture or policy owners.
Rebase each slice onto current main and refresh inventories; this historical baseline
is not permission to ignore newly landed tests.

## 7. Test plan

| Criteria | Proof required before the corresponding migration lands |
| --- | --- |
| 1–3 | Native before/after runner listings; bijective name map including ignored/features; actual exact-helper handshake; module/selector removal controls |
| 4, 6 | Trace startup rejection, recovery, descriptor/handle failure and profile/media diagnosis; record unique source files, read bytes/tokens and unrelated modules; inspect imports/visibility and each shared consumer |
| 5 | Per-slice assertion/fixture/resource-scope review; same existing negative controls fail semantically; normal shutdown, injected failure cleanup and healthy relaunch pass |
| 7 | Separate default, feature-enabled and signed/operator execution receipts per real OS; preserve unverified rows rather than calling an empty/default run complete |
| 8 | Existing `just agent-context`, `just atomic-protocol`, `just llms-test` and `just llms-check`; measured instruction delta; representative route/rule-removal controls |

Use `cargo test -p keld-host --test no_flag_macos -- --list` on macOS (corresponding
native target elsewhere) and the installed nextest version's structured list output.
Repeat discovery for `--features profile-test-hooks` where applicable. A listing is
registration evidence only. Run `just ci` and the slice's governed product acceptance;
default tests do not stand in for ignored camera/signing/reboot scenarios.

The before/after comparison also records executable count, cold build time, focused
and full-suite duration, and observed first-attempt failures under the same environment.
Do not optimize away evidence to improve those numbers. No measured speedup or
coding-agent success-rate improvement is claimed by this draft.

Rollback is the individual mechanical-move commit plus its consumer mappings; avoid
undoing unrelated newer tests. Revert a shared-helper extraction independently of
scenario moves. Restore prior behavior, not a renamed disabled test.

## 8. Review gates triggered

Design/guidance slice: independent architecture and instruction review; none of the
five product gates changes. Native moves: test-only unsafe/FFI observation review
where touched; each PR explicitly declares public API, permissions, dependencies and
wire changes as none or separately reviewed. Runner/checker changes additionally
require the CI owner. A green source check does not replace native acceptance.

## 9. Perf impact

No shipped-app performance impact is intended; no production code changes. Source
modularity does not inherently improve compile time or test runtime. Preserve target
count initially, measure the diagnostic working set and execution cost separately,
and reject indirection that merely shifts rather than reduces context.

## 10. Open questions

The maintainer approval of this concrete draft is outstanding. No unresolved product
architecture choice is delegated to the implementation agent. Native runner/name
parity, diagnostic read reduction and signed/manual acceptance remain proof obligations,
not presumed results. Choosing actual OS owners and coordinating current claims is
required at each migration slice; it is not approval to run interactive media, account
changes or a reboot without the corresponding operator authorization.
