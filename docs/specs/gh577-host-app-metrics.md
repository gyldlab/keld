# Spec: Host-owned application metric snapshot
Status: draft
Linear: GH-577 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Return a truthful diagnostic snapshot of the existing architecture census through one generated host call, with stable process
lifetime identity and independent CPU intervals.

Non-goals: A second census, parent-PID discovery, private engine PID APIs and performance budget scoring are excluded.
Engine-helper diagnostics are separate from the scored Keld-owned family.

## 2. Spec refs

- [Owning issue GH-577](https://github.com/gyldlab/keld/issues/577) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Authoritative census](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/01-overview.md).
- [Runtime ownership](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md).
- [Blocking transport](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh527-worker-owned-blocking-call-transport.md).
- [Electron API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md).
- [Independent CPU intervals](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/cpu-usage.md).
- [Metric owner issue](https://github.com/gyldlab/keld/issues/577).

Verified baseline: Architecture 01 section 5.1 names host and supervised family, excludes engine/dev-loop helpers from the scored
census and requires truthful units. The parked issue's optional WebView2 helper rows are diagnostics, not permission to expand
that census. Native counter availability is not established in this session.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given an authoritative runtime family snapshot, when metrics are queried, then each admitted Keld-owned lifetime has one row and
no unrelated process appears.

Independent negative control: Walk parent PIDs or include the CLI/stage-cleanup sentinel as a Keld-owned row.

### AC2

Given a PID reused after its old process retires, when sampled, then the new lifetime gets a fresh baseline and cannot inherit old
CPU usage.

Independent negative control: Key the sample cache only by numeric PID.

### AC3

Given process.getCPUUsage runs between app metric calls, when app rows are sampled, then their independent per-lifetime intervals
remain unchanged.

Independent negative control: Share the per-process facade sampler between the two APIs.

### AC4

Given a process exits or is inaccessible during collection, when the snapshot completes, then the approved unavailable/retired
disposition is explicit and no counter is attributed to a replacement PID.

Independent negative control: Reopen the PID without validating original lifetime or fill missing values with zero.

### AC5

Given the generated call schema, when Rust and TypeScript clients build, then both use the owning generated identifier and literal
request/reply vectors.

Independent negative control: Hand-copy a call ID or introduce a second handwritten decoder.

### AC6

Given unqualified engine helpers or Linux memory shape, when rows are published, then omissions/divergences are explicit and
cannot be counted as benchmark acceptance.

Independent negative control: Combine helper diagnostics with the scored memory family sum.

## 4. Design

### 4.1 Census snapshot owner

Consume the existing runtime role/host ledger and its lifetime identifiers. Take a membership snapshot before sampling, then
validate each original lifetime while reading counters. Exclude arbitrary caller-supplied PIDs. Keep engine-helper observations in
a separately labeled diagnostic collection only if a public engine API is qualified.

### 4.2 API shape and transport

Electron getAppMetrics is synchronous; the future facade must use the approved GH527 blocking host-call path or remain typed
unsupported. Do not silently change it to a Promise. Use one existing generated call/schema owner and bounded reply admission
based on the existing family size and frame limits.

### 4.3 Counter and identity policy

Use safe KiB conversion and one independent CPU baseline per admitted lifetime for this API. No private WKWebView PID SPI. Prefer
existing public owned-process counter surfaces; inaccessible counters require an approved explicit error/partial-data disposition,
not zeros. Snapshot identity does not prove a native counter read remained bound to the same process.

### 4.4 Authority

An authenticated app can request only its existing admitted family diagnostic. The permission owner must approve whether that
surface needs an additional guard capability; until then it remains disabled. No runtime-minted filesystem or process grant is
inferred from membership.

### 4.5 Migration

Generated source, handler, facade and lifetime-cache cleanup land together. Do not copy architecture census definitions into a
second list. Row type/serviceName mapping and Linux omissions are frozen in an approved conformance table before enabling the API.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation stays in the existing runtime/host diagnostic owner, generated call source and facade. No private task-port
workaround, arbitrary PID query or benchmark-registry edit.

This publication changes only `docs/specs/gh577-host-app-metrics.md` and grouped-PR/issue metadata. Shared policies, architecture,
generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Approve row/serviceName, Linux field and guard-capability decisions against the current census and public counter sources.

### T2

- [ ] Implement one generated synchronous-call vertical slice with lifetime-bound snapshot/counters and independent CPU intervals.

### T3

- [ ] Run actual process churn/access-denial fixtures plus literal schema and exact-census controls; publish diagnostic evidence
without budget claims.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-577;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use an independent runtime ledger snapshot to compare membership, and inject process exit/reuse between snapshot and native read.
Record unavailable states rather than timing-dependent fake zero values. Native OS qualification is separate from serialization
tests.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API, wire protocol, permission-model and runtime census owner review.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Cold on-demand call; no sampling timer. Bound retained samples by existing admitted family lifetimes and clear them on retirement.
Diagnostic values do not prove product memory budgets.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve synchronous transport admission and current generated schema allocation.
2. Approve exact row and unavailable/retired counter semantics, including Linux memory.
3. Approve capability requirements and public per-platform counter sources without expanding the census.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

