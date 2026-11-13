# Spec: Structured contentTracing refusal facade
Status: draft
Linear: GH-579 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Expose the documented contentTracing surface with Promise rejections that explain unsupported tracing, without synchronously
throwing or creating any tracing output or native side effect.

Non-goals: Actual tracing, profiling, trace file generation and developer-mode exceptions are excluded. The surface remains
documented never and parked for implementation.

## 2. Spec refs

- [Owning issue GH-579](https://github.com/gyldlab/keld/issues/579) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Current exports](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts).
- [Canonical error contract](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/07-agent-experience.md).
- [Electron contentTracing API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/content-tracing.md).
- [Tracing refusal owner issue](https://github.com/gyldlab/keld/issues/579).

Verified baseline: Main does not export contentTracing. Pinned Electron docs give Promise return shapes for getCategories,
startRecording, stopRecording, getTraceBufferUsage and enableHeapProfiling. Architecture 07 requires structured compatibility
errors and one central registry.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given any of the five methods, when called even with missing arguments, then the call returns a Promise and that Promise rejects
with the canonical unsupported error.

Independent negative control: Throw before constructing/returning the Promise.

### AC2

Given await startRecording or stopRecording, when rejected, then code, tier status, GH579 and the workaround are available with a
useful fix.

Independent negative control: Return a bare Error('not implemented') or resolve to a plausible value.

### AC3

Given a new emitted error code, when the repository's package-aware registry check runs, then its one canonical registry entry and
emitting package are covered.

Independent negative control: Emit an unregistered literal code or omit the package from the scanner.

### AC4

Given arbitrary resultFilePath/options, when passed, then no file, native profiler, network request, timer or child process is
created.

Independent negative control: Create an empty trace file before rejecting.

### AC5

Given repeated or detached method calls, when invoked, then the documented rejection shape stays stable and has no mutable shared
request state.

Independent negative control: Require accidental this binding or leak one rejected request into the next.

## 4. Design

### 4.1 Permanent refusal surface

Export one plain facade with the five pinned members. Each method constructs its rejection within the Promise path and performs no
host call. Unsupported status remains truthful in development and production. No lazy module import may start tracing as a side
effect.

### 4.2 Canonical error reuse

Use the existing structured compatibility error constructor and registry owner. Do not allocate a competing error schema or
arbitrary numeric code in this spec. The error records the tracking issue, never tier and the documented alternative; its
argument-specific message must not expose sensitive paths unnecessarily.

### 4.3 Validation ordering

Because all tracing is unavailable, reject consistently without touching filesystem or native options. Accessor/coercion side
effects in arbitrary user objects must not be deliberately invoked for unused validation. Method return shape is more important
than mimicking unreachable tracing configuration parsing.

### 4.4 Migration

Add named/default exports and one parameterized conformance fixture with independent per-method shape assertions. Extend an
existing package-aware registry scan only if source inspection confirms that coverage is absent. Do not duplicate the registry
tool or claim old issue text proves a current scanner gap.

### 4.5 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future changes stay in the Electron refusal facade, exports and canonical registry integration. No engine/runtime/native tracing
implementation or artifact output.

This publication changes only `docs/specs/gh579-content-tracing-refusal.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Confirm the current shared error owner and package registry coverage; add five pending/never API cells.

### T2

- [ ] Implement Promise-shaped refusal using that helper and explicit no-side-effect assertions.

### T3

- [ ] Verify all five methods, detached/repeated calls and registry failure mutations; keep tracing unsupported.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-579;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Assert separately that invocation does not throw and awaiting rejects. Check named/default exports and inspect independent
file/native/process/network effect counters. A mock error returned as an ordinary value must fail.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and central error-registry owner review. No runtime or permission boundary changes.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

No background work or retained state beyond an individual rejected Promise. No benchmark claim.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Identify the exact current canonical helper/code and package-scan owner before implementation.
2. Approve stable workaround wording and preserve the parked never-surface scope.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

