# Spec: Host-owned custom schemes and guarded Bun response production
Status: draft
Tracker: GH-471 (#517) · Owner: @0monish (exact approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future documentation; do not merge or implement from this draft.

## 1. Goal & non-goals

Define protocol.handle for the default Session as one guarded route from a current native document to its authenticated Bun
producer and back. Host construction fixes scheme names; the existing resource adapter owns native request admission, response
delivery and retirement.

Non-goals: General protocol work stays next-milestone work. It does not gate GH473's narrow built-in resource scheme. No
file/http/https interception, hardening opt-out, service worker support, arbitrary partition, new filesystem grant, Strict virtual
Node-fs implementation, or cross-platform support claim.

## 2. Spec refs

- [Owning issue GH-471](https://github.com/gyldlab/keld/issues/471).
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md).
- [Architecture 05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md).
- [Current exports](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts).
- [Guarded native builder](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/media.rs).
- [Resource adapter draft](https://github.com/gyldlab/keld/blob/8e8ec354f03e01b15a043d74a361e263967ef496/docs/specs/gh473-macos-resource-origin.md).
- [Fixed scheme reservation](https://github.com/gyldlab/keld/issues/471#issuecomment-6396933045).
- [Default Session owner](https://github.com/gyldlab/keld/blob/ffdbb828d36b40718f99d7b823830b924c120123/docs/specs/gh474-session-profile-clear.md).
- [Retained filesystem owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel130-retained-filesystem.md).
- [Electron protocol oracle](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/protocol.md).
- [Original Zettlr producer](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/util/custom-protocols.ts).

Verified source facts and limits: The pinned main export has no protocol or session. Architecture 05 explicitly distinguishes the
destination register_scheme sketch from the live engine API. The pinned Zettlr safe-file producer
parses/decodeURIComponent-transforms the URL, performs fs.access and fs.readFile, and creates Uint8Array, Blob and Response
values. That producer can allocate before the adapter sees a response. A transport limit therefore cannot prove a bound on
arbitrary Bun handler allocation. Raw Bun node:fs bypasses the Host broker in explicit Legacy; no claim of OS containment or
guarded producer reads follows from this facade.

The design below is proposed where it goes beyond the cited owner's adopted contract. Existing approved scopes stay approved;
draft publication does not transfer their authority or prove runtime behavior.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given the trusted scheme-name table is frozen, when the native engine is constructed and app code calls
registerSchemesAsPrivileged before Ready, then only already-listed names with no privilege bits are accepted, at most once; late
registration, absent names, duplicates and nonempty privilege declarations refuse without mutating the table.

Independent negative control: Permit a new name or bypassCSP through the app check; inspect the native registration table, not
just the return value.

### AC2

Given the admitted default Session is Ready, when protocol.handle registers safe-file and the same Session.protocol is used, then
both facades address one producer registry. isProtocolHandled reflects actual registration; unhandle removes it for new requests.
A nonempty partition or protected built-in scheme refuses.

Independent negative control: Create an independent registry for Session.protocol; registration through one facade must be visible
through the other.

### AC3

Given raw URL paths containing dot-dot, encoded dot-dot, encoded slash/backslash, malformed escapes or a path outside the approved
resource scope, when native ingress receives them, then the shared adapter rejects before Bun producer entry and exposes no
outside-file bytes.

Independent negative control: Move the only check after URL normalization or remove one traversal check; use a reachable outside
marker and a positive allowed sibling.

### AC4

Given two current native views and a retired document, when requests carry a foreign, fabricated or stale view/document
correlation, then the Host rejects before producer dispatch or response delivery.

Independent negative control: Trust a request-supplied view id or omit the final current-document check; a retired response must
fail its independent destination/effect observer.

### AC5

Given an approved bounded response lane, when a body has exactly the selected 8 MiB maximum or one more byte, then the exact-limit
body is delivered intact and the excess request ends with a named size refusal, never successful truncation. An unavailable
large-body transport refuses explicitly.

Independent negative control: Truncate the excess body and report success; compare the native delivered bytes and completion class
to an independent fixture hash.

### AC6

Given concurrent requests across views and role turnover, when cancellation or local rejection occurs while native delivery is
held, then retained adapter records and bytes remain charged against the approved view/session aggregate until actual release;
stale completions cannot release another request.

Independent negative control: Refund on caller rejection or reset aggregate counters at restart; independently count retained
callback/body owners while submitting further requests.

### AC7

Given the original safe-file producer and a macOS Legacy workspace fixture, when the recorded Zettlr resource call sites execute,
then allowed images/PDFs have correct bytes, status and MIME while outside-scope resources refuse visibly. No blanket filesystem
grant is introduced.

Independent negative control: Replace the producer with a constant Response or remove Host resource admission; original rendered
content and the outside marker must discriminate both faults.

### AC8

Given unhandle, handler replacement or Bun loss races with an admitted request, when its old producer responds, then that response
cannot reach a successor registration or new document. The native request terminates once and a healthy newly admitted request
succeeds.

Independent negative control: Key pending work only by scheme name; an old response must not satisfy a replacement producer's
request.

### AC9

Given each documented CustomScheme privilege on each engine, when recording the compatibility table, then every cell is supported
by an exact native receipt, an explicit divergence/refusal or UNKNOWN. Registration alone does not prove storage, fetch, relative
URLs or service workers.

Independent negative control: Mark all privilege cells supported when the scheme registration succeeds; a native storage/fetch
witness must fail this claim.

### AC10

Given deprecated register/intercept APIs, unsupported methods/body streams, or unqualified Range semantics, when invoked in this
first tranche, then they refuse with the owning typed diagnostic and zero new producer/native effect.

Independent negative control: Silently map an interceptFileProtocol call onto the admitted scheme or read an unsupported native
upload body before rejecting.

## 4. Design

### 4.1 One table and one adapter

GH471 owns the proposed static scheme-name configuration amendment. Coordinate GH473's fixed reservation in that same table; never
create two registries for native scheme names. Native registration precedes WebView construction. The table carries names, not
permissions. No app call changes it at runtime. Reserve engine/Host-owned names so a Bun handler cannot replace them.

### 4.2 Default Session facade

Reuse GH553's one exported Session and GH474's per-admitted-generation default identity. Global protocol and
defaultSession.protocol share one producer registry for that Session. Other partitions remain refused. Follow the pinned
before-Ready rule only for registerSchemesAsPrivileged; the handle family requires Ready. Propose typed refusal for duplicate
active handlers; final exact error/argument behavior needs GH554's oracle before approval. A null/invalid handler is not an
implicit unhandle.

### 4.3 Ingress, authority and producer reads

The resource adapter binds the native requesting view, document and scheme entry before parsing or copying attacker-controlled
body/metadata. Reuse its qualified pre-conversion gate; the public Wry callback alone does not bound earlier work. Accept GET
without upload body in the first tranche; reject unsupported methods/streams before reading them. Preserve the raw lexical
evidence required to reject encoded traversal before canonical URL processing erases it. Reuse the guard's parser/matcher and
retained filesystem owner; do not introduce a second path grammar.

### 4.4 Scope limit in Legacy

Host resource admission and the producer's filesystem effects are different atoms. The original Zettlr producer uses raw node:fs;
explicit Legacy does not turn those calls into brokered reads. A passed URL-scope test does not establish retained-object
confinement or prevent arbitrary Bun reads. Before claiming the issue's host-enforced read-scope acceptance, the existing
filesystem/migration owners must select and qualify a producer path through their retained broker or approve a precisely narrower
resource-only claim. Preserve the original caller and report this acceptance blocker; never disguise a monkeypatch as OS
containment. Strict remains unavailable until its own fs+ owner exists.

### 4.5 Bounded response choice

Select a bounded stream of existing owner-approved transport records, with an 8 MiB per-response ceiling proposed for this
tranche, rather than enlarging canonical 4096-byte control messages. Reuse the resource/transport owners' chunk and credit
contracts; no new virtual-port parser or numerical wire constant is allocated here. Large bodies are unavailable until that
transport lane is approved and live. Reject oversize with an explicit divergence; never truncate to fit. Range/media seeking
remains refused until independently specified.

### 4.6 Aggregate and completion ownership

Propose at most eight adapter requests per view lifetime and 32 per Host session, with 16 MiB of total adapter-owned retained body
backing across those requests. Admission accounts for queues, copies and unquiesced retired work before making an adapter-owned
copy. The producer's already-created Response and whole-engine allocation remain separately measured/unbounded by this claim.
Bytes shared by aliases are attributed once, but every additional owned copy is charged. The approved implementation must prove
backpressure and pre-copy enforcement for each boundary before enabling the lane.

### 4.7 Failure and native lifetime

Use host-minted request/registration incarnations tied to the authenticated role and native document. Keep native handles on the
UI owner. unhandle removes new admission; old work retains its original identity until completion or proven teardown. A local
cancel revokes delivery, not native completion. If partial response bytes were already delivered, report failed/incomplete
transfer with that disposition; no rollback or replay. Retain bounded retirement state until the actual native callback/task
quiesces, and refuse excess work rather than forgetting it.

### 4.8 Migration and platform evidence

Retain the original Zettlr path transformation as evidence: drive-letter slash stripping and macOS share-path rewriting are not
silently adopted as Host authority. Record exact raw and mapped cases, and leave UNC/mounted-share policy with the migration
owner. macOS is the first proposed lane; WebView2/WebKitGTK, custom-origin storage and each privilege need separate native
receipts. No Wry-to-native rewrite is required by this document. Existing error/schema owners allocate any new categories and
channels.

## 5. Boundaries

Only this proposed spec and its PR/issue metadata change now. Future implementation spans the existing resource adapter, generated
transport, protocol facade and migrate configuration producer after each owner's approval. Do not change GH473's initial built-in
resource behavior, other Session APIs, the frozen permission snapshot, shared wire constants, native backend ownership or active
worktrees incidentally.

## 6. Tasks (ordered vertical slices)

### T1

- [ ] Approve the static-table amendment, fixed reservation handoff, GH554 reference cases, resource-versus-producer authority
resolution and bounded transport contract; publish exact rejected inputs before implementation.

### T2

- [ ] Implement one admitted default-Session safe-file vertical path through the shared native adapter, guard and Bun producer,
with exact bytes/status/MIME plus traversal/foreign-identity controls.

### T3

- [ ] Add unhandle/replacement, held completion, cancellation, generation turnover and aggregate-pressure cases; prove pre-copy
admission and native retirement separately.

### T4

- [ ] Run original Zettlr resource callers on macOS, record outside-workspace breakage and source-specific path transforms, then
populate privilege/storage/engine cells only from their actual receipts.

## 7. Test plan

Map AC1–AC4 to facade, schema and real native-ingress tests; AC5–AC6 to exact-size/credit/retained-owner controls; AC7 to
original-app rendering and independent outside markers; AC8 to fenced registration/native completion races; AC9–AC10 to the
per-engine/reference matrix. Use actual binary bytes rather than JSON length as the transfer oracle. A model proves bookkeeping,
not Wry's earlier copies or native release. Synchronize on callbacks and held owner barriers; deadlines are kill switches. The
original producer's multiple body copies must appear in the memory census even though arbitrary Bun allocations are not bounded by
the adapter contract.

Every negative control changes one named contract fault while a legitimate positive remains reachable. Native and original-app
tests above are future acceptance, not runs performed here. Documentation validation checks source pins, complete template
sections, independently falsifiable criteria, bounds and exact published readback. Hosted CI is reported at the actual PR head.
This GitHub-only session ran no local hooks, just ci, native test or machine closeout. Official upstream GitHub sources were
retrieved on 2026-11-06. Context7 and other machine evidence are not claimed; the user's source restriction governs this pass.

## 8. Review gates triggered

Public API, permission model and wire protocol. Native unsafe/design gates apply to changed binding seams. Configuration changes
require their owning schema review. No dependency addition is proposed. All referenced draft dependencies require their own exact
approval.

## 9. Perf impact

No measured speed or memory improvement. Separate producer allocation, adapter-owned retention, wire buffers, native consumption
and actual rendered-resource latency. The proposed caps require corpus pressure evidence and explicit approval; they do not
establish a whole-process heap bound.

## 10. Open questions and implementation gates

1. GH471/GH473/configuration owners: approve one static table and the narrow built-in reservation without making general protocol
support a first-proof dependency.

2. Filesystem/migration owners: resolve the original raw node:fs producer versus host-enforced read-scope criterion; an unmodified
Legacy producer cannot be described as broker-contained.

3. Resource/transport owners: approve the response lane, chunk/credit contract, 8 MiB body and 8/32 request plus 16 MiB
adapter-retention proposals; prove native pre-conversion admission.

4. GH554/macOS operator: pin duplicate/invalid-input oracle, original resource inventory, custom-scheme storage and privilege
receipts; Windows/Linux remain unknown.

Independent review and fixes must bind the actual publication revision. Unavailable native proof remains an acceptance gap, not a
completed feature or an implicit owner decision.
