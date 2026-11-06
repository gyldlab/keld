# Spec: Honest local app compatibility shims
Status: draft
Linear: GH-454 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Provide deliberately local app shims with truthful diagnostics, exact readiness errors and no implied engine, sandbox,
registration or login-item effect.

Non-goals: Real protocol/login registration, engine switch forwarding, hardware policy changes and process.noAsar implementation
remain outside this parked future contract.

## 2. Spec refs

- [Owning issue GH-454](https://github.com/gyldlab/keld/issues/454) and its latest public owner decisions.
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md): native ownership, engine boundary and capability separation.
- [Current app facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Facade exports](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts).
- [Structured errors](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/07-agent-experience.md).
- [Electron command-line API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/command-line.md).
- [Exact readiness errors](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_app.cc).
- [Registration owner](https://github.com/gyldlab/keld/issues/393).

Verified baseline: Current main exports app and BrowserWindow; this future app surface is not inferred from an issue label. The
pinned Electron source confirms separate readiness errors for enableSandbox and disableHardwareAcceleration. GH454 remains parked
for implementation.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given appendSwitch('x','1'), when hasSwitch/getSwitchValue/removeSwitch are called, then the private map is self-consistent and no
native engine argument changes.

Independent negative control: Forward the stored switch to the engine or return a constant getter result.

### AC2

Given the actual Ready state, when disableHardwareAcceleration is called afterward, then it throws Error with message
app.disableHardwareAcceleration() can only be called before app is ready.

Independent negative control: Read a facade-local guessed Ready flag or omit the throw.

### AC3

Given actual Ready, when enableSandbox is called afterward, then it throws Error with message app.enableSandbox() can only be
called before app is ready.

Independent negative control: Use the wrong method's text or silently succeed.

### AC4

Given either pre-ready intent, when accepted by the approved local shim, then a diagnostic says its engine/security effect is not
applied and the captured host profile remains unchanged.

Independent negative control: Claim a switch changed the host's security profile.

### AC5

Given protocol/login registration methods without a materialized declaration, when called, then return shapes match their
individual approved signatures and cannot report registration success; diagnostics name GH393.

Independent negative control: Return true or a registered login state without any OS declaration.

### AC6

Given a new unapproved app member or process.noAsar, when accessed under its documented contract, then the canonical
unsupported/never outcome is used without adding a hidden implementation.

Independent negative control: Catch unsupported operations and return a generic successful value.

## 4. Design

### 4.1 Local state only

Keep commandLine in the permanent Electron facade. append/remove/query use one private map; appendArgument records ordered local
intent. Do not mutate process.argv, compiled boot inputs or engine startup options. Propose at most 256 keys/arguments and 64 KiB
total UTF-8 state, with a typed limit failure before mutation; approve these divergence bounds before implementation.

### 4.2 Readiness and diagnostics

Use the existing authoritative app Ready mirror. The two pinned post-ready error messages are exact conformance targets. Before
readiness, accepted intent remains a diagnosed no-op and cannot widen policy. Deduplicate switch warnings using the same bounded
map lifetime; do not create an unbounded diagnostic cache.

### 4.3 Honest return shape

set/remove/isDefaultProtocolClient return false until real registration exists. Do not force every method to boolean:
setLoginItemSettings has its own void contract and getLoginItemSettings returns an explicit not-registered shape. Pin each
signature in T1 and keep unsupported fields typed or divergent.

### 4.4 Ownership and migration

Real registration stays GH393/packaging, GPU policy stays its engine owner and Strict/Legacy admission stays the host. Add exports
and the shared structured-error helper with existing owners; no new wire message, grant or process-global override. This draft
does not schedule the parked implementation.

### 4.5 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future changes are confined to the Electron app facade and its conformance rows. Do not change native boot, profile, packaging
declarations or engine switches.

This publication changes only `docs/specs/gh454-facade-local-app-shims.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Pin the complete per-method signatures and readiness text, approve local-state budgets and add pending/divergence rows.

### T2

- [ ] Implement the local map, no-effect intents and honest per-method return/error shapes with zero-host-call controls.

### T3

- [ ] Test exact Ready transitions, repeated registration, cap boundaries and unchanged engine/profile inputs; publish only facade
evidence.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-454;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Use an independently instrumented native-command boundary to assert zero calls. Exercise valid map operations and mutation
negatives rather than testing a copied map implementation. Platform registration remains unimplemented even if local shape tests
pass.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and shared error-owner review. No new runtime authority or wire contract is proposed.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

Bound local map and diagnostic state by the approved limits; no host round trip or startup work. No product performance claim.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve the proposed local-state limits and each method's exact unsupported/diagnostic shape.
2. Confirm existing error-registry owner coverage and preserve the parked implementation priority.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

