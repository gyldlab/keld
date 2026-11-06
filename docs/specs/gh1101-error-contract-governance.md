# Spec: Error identifier and logical carrier governance
Status: draft
Linear: GH-1101 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Give every framework diagnostic one stable meaning and one allocation owner, and make the code
available as structured data wherever an application-visible failure promises it. Preserve existing
identifier spellings, wire bytes and approved redaction exceptions.

Non-goals: renumbering existing codes, choosing a new generator, moving error types into a shared
crate, changing CallError fields, selecting retry rules, implementing GH1095 or repairing unrelated
emitters. GH1104 owns tooling choice; GH1034 owns one-meaning enforcement.

## 2. Spec refs

- [Owning issue GH1101](https://github.com/gyldlab/keld/issues/1101); [census GH1100](https://github.com/gyldlab/keld/issues/1100).
- [Architecture 07 error contract](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/07-agent-experience.md).
- [Canonical registry](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/engineering/keld-error-codes.md).
- [Actual scanner and structural checks](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-cli/tests/error_registry.rs).
- [CallError and its approved auth exception](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/src/call_error.rs).
- [CLI/MCP logical object](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-cli/src/error_object.rs).
- [Canonical TypeScript errors and WorkerLink](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/kipc/src/transport.ts).
- [API closed-link paths](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/api/src/link.ts); [API channel errors](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/api/src/channels.ts).
- [Renderer outcome carrier](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/macos_bridge.rs).
- [SDK argument boundary](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-cli/src/mcp/server.rs).
- [Request-outcome owner, r2 still draft](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh1095-request-error-codes.md).
- [Semantic enforcement owner GH1034](https://github.com/gyldlab/keld/issues/1034).
- [Reservation finding](https://github.com/gyldlab/keld/issues/1101#issuecomment-6372558865).
- [Later correction of the sentinel-code finding](https://github.com/gyldlab/keld/issues/1101#issuecomment-6374368389).
- [SDK-generated argument-error finding](https://github.com/gyldlab/keld/issues/1101#issuecomment-6428645752).
- [Phase-specific diagnostic findings](https://github.com/gyldlab/keld/issues/1101#issuecomment-6429628845).

Verified baseline: the scanner accepts hyphenated and compact tokens with at least three digits,
including tokens in comments. It checks lexical set equality, unique headings and required
crate/message/fix fields; it does not prove one meaning, correct phase guidance or real emission.
Its current SCAN_REL includes @keld/electron; older prose lists must not become a second scan owner.
Historical issue counts are not a current census.

The current TypeScript kipcError and channels.ts apiError return message-only Error objects.
WorkerLink already wraps many paths with structured codes. The malformed-ERR helper and the live
WorkerLink boundary have different dispositions; neither may be inferred from the other.
GH1095 r2 remains draft on main even though its documentation PR was merged.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given the approved grammar, when new identifiers are allocated, then they use the one chosen
grammar while every existing code retains its exact spelling and meaning.

Independent negative control: accept a third spelling, silently normalize a compact existing code,
or renumber a historical code; the immutable compatibility census must fail.

### AC2

Given emitted, reserved and retired identifiers in the complete owner census, when an allocation
is proposed, then it conflicts with none of them and has one owning issue before an emitter lands.

Independent negative control: consider an approved but un-emitted spec reservation free because
the lexical emission set lacks it. The GH531 reservation fixture must detect that omission.

### AC3

Given a registered code with one semantic contract, when a second distinct emitted meaning is
introduced, then GH1034's canonical semantic check fails even if lexical token sets are unchanged.

Independent negative control: preserve the string while changing a failure phase or no-effect
claim. A lexical-only equality test must not satisfy this criterion.

### AC4

Given every carrier in section 4.3, when representative failures cross it, then presence, absence,
provenance and redaction match that table. Code lookup never supplies routing or authorization.

Independent negative control: let a peer code choose a route, reconstruct a token in fix text, or
invent cause/fix/docs fields on the unchanged CallError wire carrier.

### AC5

Given each application-visible TypeScript promise failure with a framework code, when it rejects,
then its own code field is available through the approved existing error constructor, or an exact
reviewed carrier exception names that path. Correct existing prefix behavior is preserved.

Independent negative control: use bare kipcError on a closed-session promise path or attach the
whole formatted message as code. Assert code and message separately.

### AC6

Given malformed ERR bytes, when the bare decoder/helper and the live WorkerLink are exercised
separately, then each retains its own current failure class and terminal behavior with the
approved structured-code shape. A malformed peer payload cannot supply its claimed code.

Independent negative control: let errorFromErrFrame's helper fallback replace WorkerLink's
link-terminal path, or let a malformed payload become a recoverable peer application error.
Observe the public rejection, terminal link state and refusal of a subsequent call independently.

### AC7

Given the canonical rule/scanner owners, when documentation is reconciled, then alternative
spelling rules and copied scan-root/per-code tables become links. SCAN_REL remains authoritative.

Independent negative control: add a TypeScript scan root without its derived coverage receipt,
or leave a contradictory new-code rule in onboarding/architecture prose.

### AC8

Given a retirement or narrowing proposal, when reviewed, then it records old/new meanings,
affected consumers, compatibility and exact approval before emitters change; retired numbers
remain unavailable. Pending GH1095 revisions are not implicitly adopted.

Independent negative control: cite a merged draft PR as exact-content approval, or change one
code's retry certainty without the owning outcome decision and consumer tests.

### AC9

Given a known MCP tool with invalid arguments, when the SDK rejects argument extraction, then
the chosen adapter produces the approved Keld object or a specifically approved carrier exception.
True JSON-RPC protocol errors retain their own protocol classification.

Independent negative control: test only valid input, classify all tool-result errors as protocol
errors, or map unknown-tool protocol failure to a business denial merely to satisfy the shape.

## 4. Design

### 4.1 Ownership, reuse and independent atoms

The emitting domain owns the failure meaning; the canonical registry owns allocation metadata;
GH1034 owns semantic enforcement; each existing boundary owns its carrier conversion. These are
separate from dispatch certainty, which belongs to GH1095/GH1097 and the actual request owner.

Reuse domain Display implementations, CallError/write_call_error, KeldErrorObject and the existing
canonical TypeScript coded-error path. Do not create another global error registry or require a
runtime lookup on kipc, guard or event-loop paths. No process, handle, memory or I/O owner changes
in this publication. Application/peer strings remain untrusted diagnostic data.

### 4.2 Identifier and allocation proposal

For new codes, propose hyphenated `KELD-[A-Z]+-[0-9]{3,}` with a positive monotonically allocated
number per existing area. Existing compact and hyphenated identifiers remain valid forever.
This proposal needs exact owner approval; current published rules are unchanged by this draft.

Allocation consults emitted definitions, approved reservations and retirement history together.
A new number is not free merely because a source-token scan misses it. Reconcile outstanding
conflicting reservations with their owners before choosing a number. This file allocates none.

Retain the registry as the discoverable owner. Its future logical record includes exact code,
one semantic owner, meaning and failure phase, actual emitter references, actionable fix,
redaction policy, state (reserved/emitted/retired), owning decision and compatibility history.
Keep current crate/message/fix compatibility; multiple emitting crates do not imply multiple
semantic owners. GH1104 chooses a checked representation after evidence, not a new runtime object.

Reserved and retired entries must be distinguishable from emitted registry headings in the
canonical check; they must not be made to pass by placing fake emitters or comments in production.
Coordinate this change with GH1034's check. Reservation cancellation does not silently recycle a
number; any exception needs an explicit reviewed allocation rule. Until that mechanism is approved,
the allocation owner must inspect open approved specs and block ambiguous numbers.

### 4.3 One logical error, explicit carrier differences

R = required; O = optional; A = absent as a separate field. A code appearing in Display text is
not a separately accessible field. This table specifies mapping, not a wire-schema extension.

| Carrier | code | message | cause | fix | docs | Provenance and redaction |
| --- | --- | --- | --- | --- | --- | --- |
| Domain Rust Display | R in text | R in text | O in text | R in text | A | Domain-owned rendering; redact resource/token details before crossing its audience boundary. |
| kipc CallError | R string | R string | A | A; included in ordinary message | A | Peer-supplied on receipt; never authority. Ordinary guard mapping and approved auth.token resource-free constants stay with their current owners. |
| CLI/MCP KeldErrorObject | R string | R string | O string | R string | R string | Boundary-selected code/fix/docs; cause is diagnostic input, sanitized for the caller. The existing docs URL shape is not proof a website exists. |
| TypeScript KeldCallError | R property | R Error message | A in this contract | A | A | Local registered code or validated peer carrier; message cannot establish origin, dispatch or trust. |
| RendererBridgeOutcome::Error | R string | R detail | A | A | A | Host-stamped view/document/request metadata stays separate. Sanitize detail; the page cannot mint those identities through error fields. |
| SDK argument-extraction tool result | A today | Plain text today | A | A | A | Explicit unresolved carrier exception/adapter gate; do not claim Keld shape on an unadapted SDK path. |

The ordinary CallError message contains its existing rendering and remedy; no new fields are
serialized. Preserve the approved auth.token constant renderer and its single owner. Do not
copy its special redaction into another capability without that owner's shared-boundary decision.

### 4.4 Structured TypeScript failures

Audit actual promise rejection sites and caller-visible synchronous failures separately. Prefer
the existing canonical coded-error constructor/conversion, retaining the original diagnostic and
terminal class. Do not add another parser that extracts authority from message text.

Current isCallError checks an Error plus a KELD- prefixed string; that is a shape check, not registry
membership, trusted-origin proof or a retry decision. New property attachment must not change
frame routing, cause a second message prefix or wrap away useful original error identity.

The bare errorFromErrFrame helper currently falls back to a message-only IPC-005 on empty/malformed
payload. WorkerLink first validates CallError, fails the link on malformed input, and returns its
already-selected terminal code. Its first-terminal precedence must remain intact. Test the actual
WorkerLink promise separately from a helper unit fixture.

### 4.5 Retirement, narrowing and migration

Never reuse an emitted/retired code. A narrowing proposal records the previous meaning, proof
boundary, changed producers/consumers and an explicit compatibility disposition. GH1095 owns its
request-code split and prefix rules; this draft cannot approve its r2 text or edit its PR.

Migrate one owning boundary with its real callers, tests, registry metadata and generated docs
together. No blanket rename and no extra runtime dependency. Review current findings before
migration: the earlier Windows sentinel split was subsequently corrected; phase-guidance and
frozen-corpus-title findings are not interchangeable and retain their named owners.

For SDK argument failures, reuse the existing MCP boundary and decide with #1248 whether a small
adapter or a precise approved exception is correct. #1258 still owns byte/resource budgets.
Do not invent numeric error allocations before the canonical census and allocation decision.

## 5. Boundaries

Future implementation: the existing registry/check, owned domain emitters, canonical TypeScript
error boundary and documentation links. Each vertical change needs its own issue authorization.

This PR changes only this new specification and grouped PR/issue metadata. Do not alter CallError
wire fields, protocol version, permission checks, workspace dependencies, #1095 implementation,
CLI-022 public error shape, or another owner's active worktree/PR.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Reconcile GH1100 census and exact GH1095 approval; approve grammar, allocation/reservation
representation and carrier exceptions with their existing owners.

### T2

- [ ] Extend GH1034's single semantic-enforcement path and reservation census with independent
duplicate-meaning, comment-only, reservation-collision and retired-number controls.

### T3

- [ ] Migrate one TypeScript error boundary at a time, including closed-session and malformed-ERR
public paths; preserve message, terminal state and original error information.

### T4

- [ ] Resolve the MCP argument-extraction carrier decision with real stdio valid/invalid/protocol
controls and unchanged redaction/resource boundaries.

### T5

- [ ] Replace contradictory copied rules with owner links, regenerate included docs through the
current generator, and publish exact reviewed migration evidence.

## 7. Test plan

AC1–AC3 use the immutable identifier/emitter census plus semantic mutations in the canonical check.
AC4–AC6 inspect actual serialized fields and application-visible errors; WorkerLink malformed-ERR
tests also observe link terminality and a subsequent refused call. Existing decoder-only tests
cannot prove that boundary. AC7 checks canonical links and scan-root coverage; AC8 checks approved
revision/consumer mappings; AC9 runs real MCP stdio requests through SDK extraction and the adapter.

Use deterministic byte fixtures, explicit link/child completion barriers and bounded watchdogs,
not sleeps. Preserve both valid and negative controls. Changed emitters must retain independent
code/message assertions; a formatted string alone is insufficient.

This publication checks document shape, source/readback and hosted documentation gates. No local
just ci, native/runtime/MCP execution, semantic mutation or machine closeout ran under the user's
GitHub-only restriction. Planned acceptance remains unrun until its implementing revision.

## 8. Review gates triggered

Future public API gate for structured code properties and approved carrier changes. Wire protocol
review verifies unchanged CallError bytes; no version bump or schema field is proposed. Unsafe,
permission model and dependency addition: none unless a later concrete implementation changes them.

Status remains draft. Publication and a passing documentation workflow do not approve any rule,
code allocation, predecessor draft or implementation.

## 9. Perf impact

No runtime effect from this documentation PR. Future enforcement stays cold tooling; compare its
time/memory against the current scanner before choosing implementation under GH1104. No new hot-path
lookup, allocation or async dependency is justified by this spec, and no measured result is claimed.

## 10. Open questions (block implementation)

1. Approve new-code grammar and canonical reservation/retirement representation after reconciling
all open-spec reservations; do not infer availability from lexical emission alone.
2. Approve the exact current GH1095 revision and remaining typed-error migration boundaries.
3. Decide the SDK argument-error adapter versus precise exception with #1248 and keep #1258 budgets.
4. Approve the semantic-check representation with GH1034 and the tooling decision with GH1104.
