# Spec: Local-only crashReporter compatibility state
Status: draft
Linear: GH-575 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Provide the requested crashReporter data methods while making uploads and minidumps explicitly unavailable and preventing every
option from initiating network activity.

Non-goals: Actual crash capture, upload, minidumps, utility/renderer crashReporter and system crash-handler replacement are
excluded. This is a parked future facade.

## 2. Spec refs

- [Owning issue GH-575](https://github.com/gyldlab/keld/issues/575) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Current exports](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts).
- [Structured errors](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/07-agent-experience.md).
- [Electron crashReporter source](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/crash-reporter.ts).
- [Electron crashReporter docs](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/crash-reporter.md).
- [GH452 app facts](https://github.com/gyldlab/keld/blob/c2dd2ff53767adf20dc0e91d276310095fe31acf/docs/specs/gh452-app-session-facts.md).

Verified baseline: Main does not export crashReporter. The pinned TypeScript source confirms start's submitURL validation. Pinned
docs conflict on extra-value limits (127 versus 20320 bytes); the owning source plus a recorded Electron oracle must settle that
row before approval. This session did not run that oracle.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given start({}) with default upload intent, when called, then it throws Error with message submitURL must be specified when
uploadToServer is true.

Independent negative control: Treat missing submitURL as a successful start.

### AC2

Given uploadToServer true and any submitURL, when any facade method is called, then no DNS, socket, HTTP request, file upload or
native crash handler is started.

Independent negative control: Add a reachable fetch/socket/native upload path.

### AC3

Given getParameters before start and after extra add/remove, when read, then it returns a safe state snapshot without a pre-start
exception or a mutable alias to internal state.

Independent negative control: Return the internal map so caller mutation changes future reads.

### AC4

Given 39-byte and 40-byte UTF-8 keys under the approved oracle, when added, then the former is admitted and the latter ignored
with one process warning.

Independent negative control: Count UTF-16 code units or admit the 40-byte boundary.

### AC5

Given no upload implementation, when reports are queried, then getLastCrashReport is null and getUploadedReports is empty
regardless of stored upload intent.

Independent negative control: Invent a local report or report stored upload intent as successful delivery.

### AC6

Given product/version defaults, when start initializes parameters, then they come from GH452 app facts and explicit allowed
overrides, not fabricated runtime strings.

Independent negative control: Use the host executable name/version instead of the selected app facts.

## 4. Design

### 4.1 Pure-data owner

One process-local facade stores approved parameters and upload intent. submitURL is inert data. getUploadToServer reflects stored
API intent only with an explicit never-upload diagnostic; it is not a consent gate to a hidden sender. Returned parameter/report
objects are copies.

### 4.2 Bounded inputs

Apply the oracle-approved UTF-8 key/value limits before state mutation. Propose 64 entries and 64 KiB aggregate parameter storage
as a declared safety divergence, counting global/default and ordinary parameters together. Overflow is a typed failure and leaves
the old state intact. No disk persistence or secret-bearing diagnostic dump.

### 4.3 Source discrepancy gate

The pinned docs disagree about value length; do not choose 127 or 20320 as verified behavior from text alone. Read the linked
native crash-key owner and run 127/128/20320/20321 probes. Also pin repeated-start and pre-start setUploadToServer behavior before
exact acceptance is enabled.

### 4.4 Migration and trust

Reuse GH452 name/version and the canonical error/warning facilities. Host policy, network authority and crash-capture ownership do
not change. Code/data options and prototypes must not become host commands. This pure-data scope can be reviewed independently of
any future user decision about off-device crash reporting.

### 4.5 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future changes stay in one Electron crashReporter facade and its exports/tests. No networking dependency, native crash handler,
filesystem artifact or telemetry pipeline.

This publication changes only `docs/specs/gh575-local-crash-reporter.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Resolve the source/doc discrepancies with exact pinned oracle receipts and approve aggregate bounds; add pending/never rows.

### T2

- [ ] Implement local state and faithful validation/return shapes with no-egress and caller-mutation controls.

### T3

- [ ] Run pre-start/repeated-start/multibyte/boundary tests and independently instrument process/network/native effects; leave
upload permanently unavailable in this scope.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-575;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Capture network/native/file side-effect counters independently of facade internals. Key limits need multibyte strings whose byte
length differs from JS string length. Native Electron observations are required only for the disputed compatibility rows, not
presumed from this draft.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and shared app-facts/error owners. Any future upload proposal requires a separate explicit contract and user decision.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Bound retained local parameters; no background work, native capture or network path. No performance claim.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Resolve exact extra-value and repeated-start semantics with the native owner/oracle.
2. Approve 64-entry/64-KiB aggregate divergence and default/override rules.
3. Preserve parked scheduling and the explicit no-upload/no-minidump contract.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

