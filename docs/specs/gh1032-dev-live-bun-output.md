# Spec: Bounded live Bun output in the dev terminal
Status: draft
Linear: GH-1032 (#517) · Owner: @0monish (exact contract approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Deliver supervised Bun stdout/stderr to the keld dev terminal while the app is running, by tapping existing capture readers.
Preserve bounded retained transcripts, crash diagnostics, authenticated readiness, generation recovery and shutdown
ownership.

No file watching, restart key, log file, principal tagging, renderer/Host log multiplexing, extra capture thread/process or
macOS guardian protocol change. Runtime implementation requires exact spec approval; the owner already selected macOS stderr
mapping for this slice.

## 2. Spec refs

- [Owning issue GH-1032](https://github.com/gyldlab/keld/issues/1032); current public owner decisions remain authoritative.
- [Delegated direction and constraints](https://github.com/gyldlab/keld/issues/1032#issuecomment-6281274038).
- [Capture readers, retention and retirement](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/lib.rs).
- [Host terminal forwarding](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-host/src/main.rs).
- [App-session terminal path](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session.rs).
- [Guardian supervision and acknowledgement](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/macos_guardian/supervision_runner.rs).
- [Runtime architecture](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md).
- [Runtime native boundary rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/AGENTS.md).
- [Shipping boot ownership](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel96-no-flag-host-boot.md).

Verified source baseline: Current capture readers use 4 KiB chunks and retain a 64 KiB head plus 192 KiB tail per stream,
with 64 KiB compaction slack. Both reader ownership and retirement already exist. The macOS renderer dispatcher is unrelated
to output capture. The adopted owner decision keeps guardian stdout reserved and routes both Bun streams to terminal stderr
on macOS. This drafting session did not qualify an OS terminal-write adapter.

The proposed contract below is future work. This publication changes no runtime boundary and grants no implementation or
predecessor approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given shipping keld dev and a post-Ready fixture parked before Quit, when it emits unique stdout/stderr markers, then both
appear at the selected terminal sink before Quit and within the issue's one-second deadline.

Independent negative control: Disable only the live sink; the same current capture path must retain the markers but fail
pre-Quit terminal visibility.

### AC2

Given same-window recovery, when generation one writes a unique stderr tail and exits, then that tail is visible before
successor Ready or within one second of the first exit.

Independent negative control: Forward only at final supervision outcome; the pre-successor/exit-time observation must fail.

### AC3

Given Linux/Windows or macOS, when separate stream markers are emitted, then Linux/Windows map stdout to stdout and stderr to
stderr, while macOS maps both to stderr and emits no Bun bytes into guardian stdout.

Independent negative control: Route one macOS Bun marker through stdout; the independent control-channel byte reader must
detect it.

### AC4

Given an undrained or closed terminal pipe, when the child floods output and Quit is requested, then the sink disables or
drops promptly, capture still drains/retains under existing bounds and all existing shutdown tests pass unchanged.

Independent negative control: Use a blocking write or final blocking drop-summary print; the controlled full-pipe subprocess
must fail bounded shutdown.

### AC5

Given output exceeding retention and compaction boundaries across generations, when capture is inspected, then the original
retention constants, stderr-tail selection and elision meaning remain unchanged.

Independent negative control: Change capture capacity or skip capture when terminal delivery fails; the existing retention
tests must fail without editing their expectations.

### AC6

Given a partial sink write followed by refusal, when bytes are accounted, then delivered prefix and undelivered remainder are
counted without duplicating output or treating the unattempted remainder as delivered.

Independent negative control: Retry the whole chunk after a partial write or count requested length as completed.

### AC7

Given stdout text resembling a readiness marker but no authenticated HELLO, when startup runs, then it still fails with the
existing startup contract.

Independent negative control: Use log contents to admit Ready; the no-HELLO fixture must expose the false admission.

### AC8

Given CLI death, Host-only death, inherited pipe writers and ordered Quit, when the existing lifecycle owner retires the
session, then no added thread/process survives and the native stage census returns to baseline.

Independent negative control: Detach a live sink worker or wait for descendant-held EOF rather than the existing retirement
boundary.

### AC9

Given a healthy terminal and later session completion, when terminal output is reviewed, then live-delivered bytes are not
blindly emitted a second time; retained transcript/crash reporting remains separately available and any dropped-byte notice
is bounded best effort.

Independent negative control: Replay the complete retained transcript after already delivering every byte live, or block
completion to force a notice into a closed pipe.

## 4. Design

### 4.1 One capture owner, optional sink

The existing capture reader receives raw bytes, offers each chunk once to an optional sink before retention, and always runs
the existing capture/marker path regardless of sink success. The sink owns only an output handle and bounded counters, not a
second transcript. No unbounded queue, new thread, native process or per-byte allocation is permitted. Disablement cannot
poison the capture owner or authoritative lifecycle outcome.

### 4.2 Explicit stream mapping

Linux and Windows use the Host's selected inherited terminal stdout/stderr handles. The macOS guardian sends both Bun streams
to its inherited stderr; stdout remains its existing acknowledgement protocol. There is no cross-stream ordering guarantee,
only per-source observed chunk order. This adopted owner decision is not reopened by this spec.

### 4.3 Nonblocking adapter is a real acceptance gate

The adapter must have a qualified finite nonblocking attempt for each supported sink kind before the live feature is enabled
there. A readiness poll followed by an ordinary blocking write is not accepted as that proof. Qualification must cover
regular files, consoles/TTYs and pipes as applicable, partial writes, interruption and closed/full outputs. Do not change a
shared inherited handle's flags or blocking behavior without proving other users remain correct. Windows inherited-handle
feasibility is explicitly unproved here; no unsupported overlapped-I/O guarantee is assumed.

### 4.4 No output wait can delay retirement

Proposed interface is a bounded try-write result: Delivered(n), Partial(n), Disabled(reason) or Dropped(n). The final naming
belongs to the runtime API owner. There is no write-all loop or flush wait in capture or shutdown. On backpressure/error
disable that stream's sink for the session, count the undelivered remainder and subsequent dropped bytes with defined
saturation, and continue retention. Counters do not allocate an error per chunk. Interruption retries, if any, have a bounded
attempt budget.

### 4.5 Completion and transcript policy

Retain the exact CapturedOutput object, crash tail and elision semantics. Proposed terminal policy avoids repeating chunks
already live-delivered; expose the existing transcript to its diagnostic consumers without unconditional end-of-session
replay. A best-effort final summary states dropped-byte counts only when a bounded output attempt is possible. If existing
terminal forwarding must change to enforce this, include it in the same implementation and leave the capture tests unchanged.
Approval must freeze this presentation policy before code.

### 4.6 Clock and evidence separation

Use a controller-observed monotonic clock or a proven shared test clock to compare the fixture write and terminal-read
observation; never subtract unrelated process clocks. One second is an acceptance deadline, not an observed performance
claim. Coordinate output reads before out-of-band process censuses so test-harness pipes cannot hide a delivered marker. Raw
bytes, retention text transformations, Ready state and process cleanup are independent oracles.

### 4.7 Rejected alternatives and migration

The owner rejected a restart key, stage log file, wording-only fix and guardian-stdout multiplexing. Reuse existing capture
readers and inherited handles; if a platform cannot supply a safe nonblocking primitive under the no-new-thread constraint,
leave that lane blocked and return to the runtime owner. Do not silently relax the constraint. Update architecture 06,
architecture 07 and README through their existing owners only with implementation and actual evidence.

## 5. Boundaries

Future changes belong to existing runtime capture/sink configuration and host/guardian presentation call sites plus their
regression fixtures. Preserve restart policy, admission, grants, keeper/Job/process-group semantics, Ready and retention.
Existing unsafe allowances do not authorize new terminal operations.

This publication changes only `docs/specs/gh1032-dev-live-bun-output.md` and related grouped-PR/issue handoffs. Shared
architecture, policy, generated contracts, implementations and other owners' documents are not edited.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Qualify one finite try-write adapter per actual OS/sink kind, freeze its bounded failure behavior and terminal-completion policy, and obtain exact-content/runtime approval; preserve unsupported lanes as blockers.

### T2

- [ ] Wire the optional sink into existing capture readers and terminal owners, with current-tree pre-Quit failure and full-pipe/partial-write controls; keep retention tests unchanged.

### T3

- [ ] Run shipping dev and recovery fixtures plus unchanged lifecycle/host-death/stage-cleanup tests on each real OS, then exercise one human interval-log/Ctrl+C run per lane.

### T4

- [ ] Update the owning architecture/README behavior and generated documentation only with actual implemented mapping and receipts; coordinate GH1033 prose ownership.

## 7. Test plan

AC1–3 use real shipping keld dev, unique non-ready markers, a controller that reads both terminal pipes and explicit
park/recovery barriers. AC4/6 deliberately stop draining, close or inject partial sink completion in isolated subprocesses.
AC5 reuses unedited retention tests at zero, exact bound, bound+1 and multi-generation cases. AC7 preserves the no-HELLO
control. AC8 reuses native cleanup and inherited-writer fixtures. AC9 counts exact marker occurrences and verifies bounded
completion with the terminal closed. None of these native tests ran in this session.

Each acceptance criterion has its own observable and negative control above. Documentation verification covers all ten
sections, source-versus-proposal wording, file limits and exact GitHub readback. GitHub CI belongs to the exact published
head. Native/runtime/conformance/OS outcomes remain unrun until the implementing owner executes them; green documentation
checks do not prove those outcomes.

The user restricted this session to GitHub. Local `just ci`, hooks and machine session-closeout were not run. Official
immutable upstream GitHub sources are used where relevant; Context7 was not used under that source restriction. No
external-machine or native receipt is fabricated.

## 8. Review gates triggered

Public runtime API and native-operation review. No permission, dependency or wire change is proposed; any new unsafe path
requires its existing crate/root gate rather than an implicit allowance.

Draft publication is not exact-content approval. Pending predecessor approvals and native qualification remain separate
gates.

## 9. Perf impact

Offer at most one bounded attempt per captured chunk, no second retained byte queue, and fixed counters per stream. Measure
syscall/copy cost and shutdown latency on the actual adapter before an implementation claim. Existing capture bounds remain
independent of output throughput.

## 10. Open questions (block implementation)

1. Qualify the real nonblocking adapter for every OS/sink kind, especially Windows inherited handles, within the adopted
no-new-thread/process constraint.

2. Approve the proposed no-duplicate final transcript presentation and bounded drop-summary policy; do not weaken capture
retention or error diagnostics.

3. Freeze the exact finite write-attempt budget, counter saturation and platform error mapping under the runtime API owner.

4. Run actual shipping/native/manual evidence and coordinate GH1033 documentation updates before claiming live output
support.

Independent review must cover the final published revision. Every remaining owner/native gate stays explicit; this document
is not a completed feature.

