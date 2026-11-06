# Spec: Public lifecycle and shipped-binary exit contracts
Status: draft
Linear: GH-1103 (#517) · Owner: @0monish (exact contract approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Create one source-backed public/internal lifecycle census and per-shipped-binary exit table, with explicit consumers,
ordering, terminal triggers and real process-status tests. Preserve current wire bytes and process ownership while exposing
unknown guarantees as owned findings.

No app.exit implementation, renderer deadline design, CLI environment-exit decision, replacement lifecycle runtime, new event
taxonomy or tooling selection. GH1100 owns the overall catalog, GH1101/1102 the adjacent error/wire rules and GH1104 tooling.

## 2. Spec refs

- [Owning issue GH-1103](https://github.com/gyldlab/keld/issues/1103); current public owner decisions remain authoritative.
- [Overall contract census owner](https://github.com/gyldlab/keld/issues/1100).
- [CLI exit-3 and drift owner](https://github.com/gyldlab/keld/issues/1034).
- [Existing audit input and limitations](https://github.com/gyldlab/keld/issues/1103#issuecomment-6408158087).
- [Lifecycle payload source](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/src/lifecycle.rs).
- [Current TypeScript app consumer](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/api/src/app.ts).
- [Supervisor events/outcomes and capture](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/lib.rs).
- [Named-role event owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/role.rs).
- [Core app-session consumer](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session.rs).
- [CLI exit sites](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-cli/src/main.rs).
- [Host exit sites](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-host/src/main.rs).
- [Updater-helper exit owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-updater-helper/src/main.rs).
- [Host role-launcher entry](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-host/src/bin/keld-role-launcher.rs).
- [Runtime strict-launcher entry](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/bin/keld-linux-strict-launcher.rs).

Verified source baseline: Current SupervisorEvent and SupervisorOutcome explicitly warn that Stopped is not proof of clean
child exit; CrashLedger must be consulted. RoleEvent exposes generation and revocation causes, while OS PID is diagnostic.
wait_for_outcome has no internal deadline and still contains an unconditional-termination sentence; a healthy long-lived
child and sufficiently spaced restarts falsify that sentence. The event path uses mpsc::channel, distinct from bounded crash
history. Two launcher source entries exist and must be classified against shipped package contents rather than assuming every
src/bin file is delivered.

The proposed contract below is future work. This publication changes no runtime boundary and grants no implementation or
predecessor approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given the pinned source and every discovered lifecycle family, when the census is built, then each row names exactly one
owner, its identifier, real producers/consumers, public/internal status, ordering, terminal trigger and version rule.

Independent negative control: Omit RoleEvent, a TypeScript consumer or an internal state leaked to an external consumer; the
independent source/consumer inventory must differ.

### AC2

Given LifecycleEvent/Request/Response, when Rust and TypeScript contract fixtures run, then exact literal bytes remain
unchanged and all current consumers decode the same values.

Independent negative control: Change one discriminant/order or a handwritten Quit byte while leaving the other side
unchanged.

### AC3

Given a healthy long-lived supervisor, when it is awaited without a stop/failure trigger, then the census makes no bounded
session-termination promise; after an owned terminal trigger, only the independently qualified cleanup bound may be claimed.

Independent negative control: Kill healthy work or add an arbitrary lifetime timeout to make an unconditional
always-terminates sentence pass.

### AC4

Given Stopped after a prior crash, shutdown acceptance or denied successor, when classified, then its exact cause/crash
ledger is preserved and it is not relabeled as a clean child exit.

Independent negative control: Map every Stopped to clean exit zero; the crash-then-stop fixture must fail.

### AC5

Given the selected shipped artifact set, when every executable's exit sites are reconciled, then every reachable emitted
value has an exact meaning/owner and an actual process wait-status test, including default returns and helper/launcher paths.

Independent negative control: Inspect only integer literals in CLI main or treat Error text as proof of the process exit
status.

### AC6

Given GH1034 has not approved the CLI environment-exit choice, when the table is published, then the current observed/source
behavior and unresolved target remain separate, without inventing exit 3 as shipping.

Independent negative control: Copy architecture's promised exit 3 into the current table despite no executed producer
receipt.

### AC7

Given a retained supervisor with an undrained event consumer, when backlog behavior is examined, then event buffering is
measured/classified separately from the bounded CrashLedger and any missing bound has an owner.

Independent negative control: Use fixed crash-history size as proof that lifecycle-event queue memory is bounded, or add a
blocking channel that deadlocks shutdown.

### AC8

Given child exit, capture completion, read failure or capture-worker panic, when outcomes are classified, then normal EOF,
unexpected observation failure and cleanup failure remain distinct where proven; unqualified Windows fault behavior stays
explicitly open.

Independent negative control: Convert a swallowed read/join error into a success claim solely because an existing
inherited-writer test passes.

### AC9

Given a new internal enum or debug-only fixture exit, when discovered, then its shipping/public classification follows actual
consumers and cfg/package boundaries rather than its public Rust visibility or filename.

Independent negative control: Promote every pub enum to wire API or score a debug-only failure code as a shipped user
contract.

### AC10

Given the final catalog and table, when a fresh reviewer reruns the documented census on the exact revision, then all rows
and unknowns reconcile with actual source and dependencies without a duplicate per-code or per-channel registry.

Independent negative control: Copy current registry/channel rows into this document or accept a source census from a
different head.

## 4. Design

### 4.1 One census, linked owners

GH1100's contract catalog/glossary is the destination or parent index. This slice supplies lifecycle/event and binary-exit
rows by links to their canonical definitions. A public Rust item, internal channel event, wire payload and user-facing CLI
status are different surfaces. Promotion to a stable public contract is an explicit owner decision. Do not create a parallel
code registry or allocation authority.

### 4.2 Required row shape

Each lifecycle row records source symbol/immutable revision, producer, actual consumer paths, lifecycle identity, ordering
edges, public or internal scope, terminal versus intermediate kind, required trigger, delivered/dropped/consumed semantics,
resource-retirement evidence and version owner. Each binary row records delivered artifact identity, invocation arm/cfg, exit
value or signal, meaning, source site, process-status test and current versus proposed status. The executable inventory
includes default return and exec-replaced child behavior, not only process::exit calls.

### 4.3 Baseline family inventory

Start with kipc LifecycleEvent/Request/Response and Host-to-app projection; SupervisorEvent/Outcome and durable CrashLedger;
RoleEvent/RevocationCause and generation state; Core app-session phase state; WorkerLink exported STATE consumers; and
shipped keld, keld-host, updater helper and role launchers. The runtime's separate strict-launcher entry is a discovered
candidate whose shipped/test-only role must be resolved from build/package sources. This list is a search seed, not a closed
universe.

### 4.4 Terminal claims are conditional

Distinguish healthy entity lifetime, a request's deadline, acceptance of a stop trigger, terminal result delivery and
physical cleanup. A healthy supervisor may run indefinitely; a sliding crash window is not a finite lifetime restart count.
wait_for_outcome waits for a terminal outcome and does not promise an unsolicited terminal. Every asserted bound must name
its trigger, cancellation/cleanup owner and independent evidence; hung requests remain GH1097, not an excuse to terminate
healthy sessions.

### 4.5 Events, history and failure visibility

The current channel and CrashLedger have different storage/consumer semantics. Enumerate all production and test drain paths
before proposing coalescing or limits. A blocking send can hold the producer/teardown hostage; it cannot be selected merely
to make memory finite. Windows capture currently has different join/read-failure visibility from Unix; qualify EOF, failure
and panic separately and preserve the existing native failure issue instead of calling all paths tested.

### 4.6 Process status and exit ownership

GH1034 owns environment-exit 3 and the overloaded CLI diagnostic decision; consume its exact approved result. GH523 owns
app.exit propagation. A supervisor reason, a serialized error, an updater-helper refusal and the CLI's final OS exit status
are separate facts. Test success/failure/reserved verb/delegated-host failure on the real binary and wait status. Record
signal behavior per OS without manufacturing a portable signal number or assuming the updater parent's process code equals
the helper's.

### 4.7 Reproducible census and migration

T1 runs rg over process::exit, ExitCode, main return types, Lifecycle variants, Supervisor/Role events, emitted state values
and all constructors/matches; trace aliases and cfg branches rather than treating grep as a full call graph. Pair that with
Cargo binary targets and actual delivered package contents. The catalog links the existing byte tests and source definitions.
Correct misleading comments in a later approved source/documentation slice without changing bytes or crash ownership. No
authority is transferred from an earlier approved subsection.

## 5. Boundaries

Future work belongs in the GH1100 catalog's lifecycle/exit sections, exact misleading source comments and behavior-preserving
tests under their owning crates. Do not change lifecycle discriminants, Quit bytes, process identity, restart policy, GH1034
choice or app.exit implementation.

This publication changes only `docs/specs/gh1103-lifecycle-exit-contracts.md` and related grouped-PR/issue handoffs. Shared
architecture, policy, generated contracts, implementations and other owners' documents are not edited.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Freeze the exact source/package inventory and GH1100 catalog home; enumerate actual event producers/consumers, binary entrypoints and source exit sites, preserving unknown public/internal decisions.

### T2

- [ ] Build literal-byte and actual-process-status coverage for existing behavior, then publish the per-binary table with GH1034/GH523 decisions explicitly pending where unresolved.

### T3

- [ ] Classify conditional terminal guarantees, event-consumer obligations and capture-failure observations using independent subprocess controls; track every unproved bound on its current owner.

### T4

- [ ] Review complete source-to-row parity and correct misleading comments/docs without semantic change; hand the linked contract census to GH1104 for tooling evaluation.

## 7. Test plan

Run a reproducible future census such as rg -n 'process::exit|ExitCode::|fn main|-> ExitCode|-> Result' crates, followed by
symbol/call-site and Cargo/package review; record the exact command output and source revision. AC1/9/10 compare independent
consumer and delivered-artifact inventories. AC2 uses existing literal byte fixtures. AC3/4/7/8 use subprocess
lifetime/stop/capture barriers and raw exit receipts; no fixed sleeps or mock-only native claims. AC5/6 wait for real binary
status on every shipping OS arm. The census commands and all process tests are planned, not executed in this GitHub-only
drafting session.

Each acceptance criterion has its own observable and negative control above. Documentation verification covers all ten
sections, source-versus-proposal wording, file limits and exact GitHub readback. GitHub CI belongs to the exact published
head. Native/runtime/conformance/OS outcomes remain unrun until the implementing owner executes them; green documentation
checks do not prove those outcomes.

The user restricted this session to GitHub. Local `just ci`, hooks and machine session-closeout were not run. Official
immutable upstream GitHub sources are used where relevant; Context7 was not used under that source restriction. No
external-machine or native receipt is fabricated.

## 8. Review gates triggered

Public API review for any newly promised stable exit/event contract. Wire, unsafe, permission and dependency changes are none
in the behavior-preserving census; changing their owned semantics requires a separate approved amendment.

Draft publication is not exact-content approval. Pending predecessor approvals and native qualification remain separate
gates.

## 9. Perf impact

Catalog and tests only. No queue or process implementation is selected. Any future queue-bound proposal must measure retained
events and shutdown behavior independently of bounded crash history.

## 10. Open questions (block implementation)

1. GH1100 must approve the single catalog/glossary home and public/internal decisions; this draft does not choose new public
promises.

2. Consume GH1034's exit-3 decision and GH523's propagation contract without settling them here.

3. Resolve delivered versus diagnostic role-launcher artifacts and identify all actual process-status tests.

4. Qualify terminal-after-trigger bounds, event-drain obligations and Windows capture fault visibility; source comments and
existing success paths do not establish those facts.

Independent review must cover the final published revision. Every remaining owner/native gate stays explicit; this document
is not a completed feature.

