# Spec: Selected Windows Legacy Guard runtime stage

Status: draft
Tracker: GH-552
Owner: GYLDLAB
Updated: 2026-11-20
Pin: 03d83be77c0bfd9d69e1cc2dec8e1d46069727c5

## 1. Goal & non-goals

Propose the umbrella parser and typed-dispatch tasks for the deliberately adopted Windows Legacy artifact. Defer the offline diagnostic API and all live forwarding.

## 2. Spec refs

[GH552 umbrella](gh552-exact-channel-grants-guard/index.md) owns the query, callback, literal and upstream contracts.
GH541 P01 owns deliberate adoption and the final-manifest control. Record both exact input revisions with the approval.
[Architecture 03](../architecture/03-security.md) owns default deny and the existing host trust boundary.

## 3. Acceptance criteria

### AC-1: Only the deliberately marked final manifest enables its exact window literal

- **Given:** A reviewed final Windows Legacy manifest, its literal list, CSP and source provenance.
- **When:** Load the delivered bytes and a control differing only by removal of the marker.
- **Then:** Record both file digests. Rebuild dependent integrity metadata and load each through its own fresh verified session.
  The exact window literal passes policy only in the marked case. A neighboring literal always denies.
  Both loads must reach Guard. Assert identical CSP and native grants.
  Absent and unsupported markers produce the same frozen channel-miss wire error.
  The future offline diagnostic owns adoption guidance for both cases.
- **Negative control:**
  - **Single fault:** Select channel authority without checking the marker.
  - **At-risk precondition:** The selected final artifact carries an otherwise valid grant and the control retains it.
  - **Expected RED:** The stripped copy returns Allow at the Guard policy boundary.

### AC-2: One typed callback owner preserves native calls and error order

- **Given:** Both Guard query types and the existing five-argument native entry.
- **When:** Evaluate marked, inactive and malformed requests through IPC.
- **Then:** Reuse Guard-owned `evaluate_query`. Only the typed IPC body invokes a callback after Allow.
  Preserve native principal denial before filesystem grammar and grammar before grant-shape errors.
  Both query kinds compile independently. Cross-kind or escaping permit borrows fail compilation.
  Prove warmed Allow uses zero heap allocations. Retain the five-argument native compile fixture.
- **Negative control:**
  - **Single fault:** Invoke the channel callback on Deny.
  - **At-risk precondition:** The valid unmarked control reaches the typed dispatcher with a counted callback.
  - **Expected RED:** The counter increases on the denied request.

## 4. Design

### D-1: Reuse Guard policy and IPC dispatch ownership

Use GH552 section 4.3 as pinned by the stage decision. Host trust remains with the existing authentication owner.
Only explicit JSON integer `1` opts into literal channel checks on the reviewed artifact.
The artifact limit is the scope of qualification. The pure query authenticates neither applications nor documents.
No new live route consumes this stage. Other applications and profiles receive no enrollment.
Keep public and strict-profile loading behavior unchanged outside the staged context. Reuse the canonical parser and verified loader.
Do not select the stage from a page input, a hardcoded path or a hardcoded artifact digest.
Pair the selected input with a distinctly verified marked artifact. Neither can reach a new live forwarding route.
Test missing, null, string, boolean, unsupported integer, decimal and exponent marker forms.
Keep existing CSP structure, validation, verified loading, native scopes and strict-profile contracts.

## 5. Boundaries

Limit source work to Guard parsing, the IPC typed callback body, its native compatibility entry and their tests.
The approved artifact owner deliberately supplies the marker. GH499 retains automatic migration.
Exclude live Core forwarding, Bun effects, native identity changes, CLI/MCP schema changes and full GH552 approval.

## 6. Tasks

### T-1: Qualify the manifest and typed callback stage

- **Blocked by:** B-1
- Run AC-1 before enabling a positive callback. Run AC-2 against the same verified policy inputs.
- Preserve the umbrella parser and typed-dispatch tasks scope, including their independently observable negative controls.

## 7. Test plan & evidence

### E-1: Runtime stage acceptance awaits exact source evidence

- **Run:** Capture both final manifest digests and normal loader outputs. Run the Guard and IPC regressions for AC-1 and AC-2.
- **n:** Two separately verified manifest variants plus independent unsupported-marker, literal and native-denial controls.
- **Environment:** Selected Windows Legacy source baseline `1f6c5730e63ed8f1d057c4fb82915ec1ca7f32bd`. Record the eventual tested revision.
- **Limits:** These tests provide no evidence of a live WebView delivery, native revocation or Bun callback.

## 8. Review gates triggered

Public API and permission model. Wire protocol, unsafe and dependency changes are outside this stage.

## 9. Perf impact

Measure warmed policy and callback allocations. Exclude transport serialization.

## 10. Open questions, blockers & unknowns

### B-1: Exact stage approval

- **Status:** open
- **Owner:** GH541 owner, Amisha for independent permission/public-API review, native-dispatch owner for precedence, and GH552 owner.
- **Impact:** Production work cannot begin.
- **Check:** Record GH541's exact P01 decision and integration. Bind reviewed stage and umbrella text, including the Rust query and manifest opt-in decisions.
  Record the named reviews and exact-content owner decisions. Set this stage alone to approved.
  The offline diagnostic API remains pending.
