# Spec: Bun-side process metric compatibility
Status: draft
Linear: GH-576 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Expose only qualified process metric fields with correct units and sampling intervals, without pretending JavaScriptCore heap
counters are V8 measurements or introducing a host round trip.

Non-goals: Host app-family census, budget scoring, Blink metrics, heap snapshots, crash/hang and fabricated missing fields are
excluded. This remains parked for implementation.

## 2. Spec refs

- [Owning issue GH-576](https://github.com/gyldlab/keld/issues/576) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Current process shim](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts).
- [Budget unit rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/01-overview.md).
- [Electron process API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/process.md).
- [CPU interval semantics](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/cpu-usage.md).
- [GH570 process-facts owner](https://github.com/gyldlab/keld/blob/9b6b4b495d1e8d060cc8a189ade9cc87c23edd0e/docs/specs/gh570-main-role-process-facts.md).

Verified baseline: GH570 owns process identity facts, not these statistics. Electron's pinned CPU documentation specifies zero on
first sample and independent intervals for process.getCPUUsage and app.getAppMetrics. The issue's >> 10 unit notation must not
become a JavaScript bitwise operation, which coerces to 32 bits.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given qualified byte counters including 5 GiB, when converted, then memory fields equal floor(bytes/1024), so 5 GiB becomes
5242880 KiB without sign or wrap loss.

Independent negative control: Use JS >> 10 or >>> 10; the above-4-GiB control must fail.

### AC2

Given the pinned heap field mapping, when returned, then numeric memory fields use KiB and doesZapGarbage stays boolean;
JavaScriptCore-derived fields carry the declared divergence.

Independent negative control: Apply numeric conversion to the boolean or label JavaScriptCore counters as measured V8 heap.

### AC3

Given the first CPU sample and later controlled CPU/wall deltas, when queried, then the first percent is zero and subsequent
values use the approved logical-processor normalization and reset only this API's interval.

Independent negative control: Share a sampler with app.getAppMetrics or omit the processor divisor.

### AC4

Given getProcessMemoryInfo cannot derive a required field, when called, then its Promise rejects with a typed explanation; it
never invents zero or throws synchronously instead.

Independent negative control: Catch source failure and return an all-zero success object.

### AC5

Given getSystemMemoryInfo for a platform, when source qualification lacks any required field, then that operation remains
explicitly unsupported until its complete mapping is approved.

Independent negative control: Treat total/free alone as the complete platform shape.

### AC6

Given process retirement or invalid/nonmonotonic sample deltas, when sampled, then stale state is discarded and the documented
fresh/error behavior applies without negative or infinite usage.

Independent negative control: Reuse another process's baseline or divide by zero elapsed time.

## 4. Design

### 4.1 Qualified local sources

Use the selected Bun version's verified local process/OS APIs; T1 pins their code and unit meanings. Do not silently add a host
query to fill gaps. The whole operation is unsupported if required fields cannot be derived, unless an explicit per-field
divergence is approved and exposed in its evidence row.

### 4.2 Safe units and field mapping

Convert safe nonnegative byte counts with Math.floor(bytes / 1024), never JavaScript bitwise shifting. Validate range/finite
values first. Map the eight documented numeric heap fields separately; doesZapGarbage is a boolean. No missing source is filled
with a plausible number. Preserve macOS residentSet omission in ProcessMemoryInfo.

### 4.3 CPU state

Keep one last cumulative user/system CPU and monotonic wall sample per process for this API. Proposed formula is 100 *
delta_cpu_seconds / delta_wall_seconds / logical_processor_count, reconciled with the pinned upstream oracle. First call returns
zero and initializes the interval. idleWakeupsPerSecond needs a qualified source or explicit divergence; zero must not masquerade
as a measurement.

### 4.4 System memory choice

Prefer a qualified Bun-local OS surface only if it supplies all platform-required fields with correct semantics. Linux available
is not free; macOS fileBacked/purgeable and swap fields are not inferred from total-minus-free. Otherwise retain typed unsupported
behavior. A future host-backed design would be a separate boundary amendment.

### 4.5 Migration

Install through GH570's process-surface owner without redefining identity/sandbox facts. No new global performance sampler or
benchmark scorer. Keep synchronous versus Promise-returning methods distinct and preserve their canonical error shapes.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future changes stay in the Electron/Bun process facade and tests. Do not touch native process inspection, process identity
installation or the architecture metric registry.

This publication changes only `docs/specs/gh576-process-metrics-facade.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Pin the actual Bun source mapping and independent Electron unit/interval oracle for each method; approve documented
divergences.

### T2

- [ ] Implement safe conversion and a process-local independent sampler with exact large-value/first-sample/interval controls.

### T3

- [ ] Implement Promise rejection and unsupported-platform behavior, then qualify all declared fields on each supported runtime
lane.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-576;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use injected independent numeric samples for exact unit/math controls and a bounded real busy interval only for source
qualification. Do not assert exact live memory or CPU numbers. Test 0, 1023, 1024, 5 GiB and invalid ranges, plus simultaneous
calls to the separate app-metrics API.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and GH570 process installation owner. No host or permission boundary change is proposed.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

On-demand local sampling only, one prior sample per process. These diagnostic values are not KELD benchmark scores and establish
no RAM/CPU improvement.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Qualify the selected Bun APIs and every required platform field.
2. Approve CPU normalization, idle-wakeup divergence and underivable-field behavior.
3. Keep implementation parked until scheduled; this pack supplies its future contract only.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

