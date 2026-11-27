# Spec: Session, protocol and online-state conformance cells
Status: draft
Linear: GH-554 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: future conformance proposal; documentation only, do not merge.

## 1. Goal & non-goals

Add cited, independently tested cells for the next session/protocol/net tranche. Each cell
distinguishes the Electron contract, Keld's current declared behavior, intentional divergence
and an unexecuted platform. A green regression test must not turn pending or divergent behavior
into a compatibility pass.

This slice owns conformance evidence. GH471 owns protocol design, GH474 owns profile/session
behavior and GH555 owns online-state policy. Permission cells remain GH472; DownloadItem,
net.request/fetch/WebSocket implementation and persistent partition support are outside scope.

## 2. Spec refs

- [GH554 Agent Brief](https://github.com/gyldlab/keld/issues/554#issuecomment-6026168636).
- Pinned Electron [protocol](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/protocol.md), [session](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/session.md) and [net](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/net.md).
- [GH532](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh532-first-proof-evidence-rules.md) owns citations, red cells, unknown and authority labels.
- [GH566](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh566-corpus-manifest-owner.md) owns registration, validation and expected statuses.
- [KEL74](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel74-compat-evidence-schema.md) owns records/score; [GH663](https://github.com/gyldlab/keld/issues/663) owns future v1 run receipts.
- [GH471](https://github.com/gyldlab/keld/issues/471), [GH474](https://github.com/gyldlab/keld/issues/474) and [GH555](https://github.com/gyldlab/keld/issues/555) own implementing policies.
- Current draft [GH474 subset](https://github.com/gyldlab/keld/blob/ffdbb828d36b40718f99d7b823830b924c120123/docs/specs/gh474-session-profile-clear.md)
  and [GH555 host observation](https://github.com/gyldlab/keld/blob/70cc54e673f6b5e76b10ed0a304ff51f93683197/docs/specs/gh555-legacy-net-online.md) are proposals, not implementation approval.
- [Architecture 03](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md), [04](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md) and [05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md).

No production boundary change in this publication. The pinned docs and issue policies were read;
no native behavior, storage clear, network state, recorder or conformance test was executed.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given every cited cell, the exact pinned sentence and reviewed snapshot hashes are present,
   the declared expected verdict has the correct implementing/divergence owner, and its mapped
   test asserts current behavior. Flipping only the expected verdict fails.
2. Given registerSchemesAsPrivileged calls, separate before-ready, after-ready and repeated-call
   cells test the documented timing/once rules. Keld's static-name check and privilege divergence
   remain GH471 policy; no native privilege is inferred from an accepted name.
3. Given protocol.handle, controlled requests preserve the documented Request/Response contract
   or the explicitly declared unsupported part. No handler result bypasses host resource binding,
   navigation generation, byte bounds or authority checks.
4. Given a non-standard scheme, relative URL and storage restrictions have separate per-engine
   cells, with a real macOS observation and provenance before any claim. Engine differences are
   reported as approved divergence or unknown, not silently treated as Electron compatibility.
5. Given session lookup, persistent-prefix, in-memory and empty/default cases are separate,
   including repeated identity and isPersistent. Tests preserve GH474's single-profile/dev
   divergence and never claim persistent partitions because Electron supports them.
6. Given clearStorageData, exact documented storage names and completion semantics are tested.
   Unsupported backend storage remains declared divergence; resolution is checked against real
   completed clearing, not just dispatch or an immediately fulfilled promise.
7. Given net.online/isOnline, false and true retain their documented asymmetric meaning.
   A true value is never accepted as proof that an arbitrary remote connection succeeds.
   Before-ready and strict-profile unknown behavior stay visible under GH555.
8. Given macOS, Windows or Linux coverage, only an executed declared engine lane can contribute
   a pass/fail record; unqualified Windows/Linux and uncited engine observations stay unknown.
   A real machine receipt cannot be replaced by a mock that returns expected values.
9. Given an implementing change, assertion, expected status, ownership metadata, denominator and
   records change together. Matching cited behavior may become pass; approved permanent
   divergence retains fail/intentional_divergence. The producer consumes actual admitted tests.

## 4. Design

### 4.1 Owners and the meaning of red

GH554's planning text calls initially unsupported cells red-until-implemented. This does not
promise every implementing ticket will converge to Electron behavior. GH471 preserves several
privilege differences, GH474 explicitly rejects partitions and reports dev persistence honestly,
and the current GH555 draft selects a host observation over the issue's earlier constant-true
alternative. These exact drafts remain unapproved; conformance consumes their eventual adopted version.

Use GH532's mutually exclusive forms: pending `fail + implementing_ticket`, approved permanent
`fail + intentional_divergence`, cited matching `pass`, and uncited `unknown`.
One cell cannot contain both failure-owner fields. A passing test proves its declared current
behavior, and does not by itself establish Electron conformance.

Reuse GH566's single corpus owner, shared doc snapshots, verdict checking and scorer.
GH663 supplies v1 showcase receipts after its approval. No new receipt parser, schema or
authority label belongs to this file. Compatibility fallback: additive cells only.

### 4.2 Protocol cells and independent observables

| Cell family | Required check | Implementation owner |
|---|---|---|
| Registration timing | Before-ready eligibility; after-ready rejection; separate process for repeat-call case | GH471 |
| Static name/privilege policy | Accepted configured name does not imply privilege bits or filesystem authority | GH471 |
| Handler request | Controlled URL/method/body and relevant documented request fields | GH471 |
| Handler response | Response or Promise<Response>, status/header/body and bounded completion | GH471 |
| Relative URL restriction | Controlled subresource request from non-standard scheme per engine | GH471 |
| Storage restriction | Controlled local/session storage, cookies and database availability per engine | GH471 |

The table is an inventory, not an executed manifest. The exact Electron after-ready error
message remains unknown: capture it at the pin before adding an exact-message assertion.
Do not synthesize that message from Keld's typed config diagnostic.

The pinned protocol documentation includes initiatorOrigin semantics. If a cell needs origin
authority, use the owner-adopted authoritative field and its presence/absence cases, never
page-controlled referrer as an authorization substitute. This spec does not expand GH471's
Request subset; unsupported fields remain explicitly unimplemented/divergent until approved.

Separate privilege registration from actual resource access. A non-standard scheme's ability
to fetch or resolve a relative URL on WebKit is an engine observation, not permission to bypass
the guard. Preserve the exact approved adapter/grant contract.

### 4.3 Session cells and storage names

| Cell family | Electron oracle | Keld policy constraint |
|---|---|---|
| Empty partition | Returns default session | GH474 maps it to the one profile |
| persist: partition | Persistent identity shared by same partition | GH474 declares typed divergence |
| Other named partition | In-memory session | GH474 declares typed divergence |
| Default persistence | Default BrowserWindow session is persistent | Dev profile is truthfully nonpersistent |
| Same lookup | Observe object/profile identity under the selected lookup | Do not invent multiple backends |
| Clear types | Exact supported name mapping and completed removal | KEL135 backend/lease/barrier remain owners |

Pinned clearStorageData names are `cookies`, `filesystem`, `indexdb`, `localstorage`,
`shadercache`, `serviceworkers` and `cachestorage`. Preserve the documented spelling
`indexdb`; do not quietly replace it with the browser API spelling `indexedDB`.
Omitted storages means all storage types under the cited Electron contract.

Tests create isolated fixture data, prove its presence, call the selected clear, wait for the
actual clear completion, and independently prove removal. A negative control resolves before
clearing and must fail. The current GH474 draft supports an explicit cookies/localstorage subset, with one documented
partial shadercache combination. Omitted options/list, empty list, other storage types and
origin-filtered calls are rejected before native mutation. These need distinct divergence cells;
do not infer support for Electron's omitted-list all-types contract.

Origin-scoped custom-scheme clearing is a future owner decision, not a capability in this draft
subset. Its cell must assert current rejection until the owner approves and implements precision.
Never clear a user's real profile to obtain test evidence. Use the real engine with an isolated
profile, preserving lease and asynchronous-clear ownership.

### 4.4 Online-state cells

The pinned docs describe both `net.online` and `net.isOnline()`, available only after ready.
False strongly indicates inability to connect; true does not establish reachability of a
particular destination. A conformance test must not require a public website to respond.

The current GH555 draft selects one host-owned macOS path observation, with distinct pending
and ready states, lifetime/epoch binding and the same snapshot for both reads. It rejects the
constant-true alternative. Cells must consume that exact approved selection if adopted and
exercise its status mapping, stale updates and getter purity. A controlled source can test policy,
but it cannot count as a real engine/OS connectivity observation where that is claimed.

A successful property read does not authorize HTTP, fetch, WebSocket or a strict-profile
network operation. The strict cell remains unknown until its parked decision is resolved.
No new socket, grant or host capability is proposed by this conformance work.

### 4.5 Per-engine evidence and migration

For the first macOS tranche, record actual engine identity, host/architecture, tested Keld
commit, exact Bun revision and the owning test/fixture. A raw engine observation without a
doc-cited matching contract stays unknown. Windows/Linux retain explicit unknown coverage.

All cells use the existing showcase v1 class and `artifact.sha256 = manifest_bytes`.
They do not silently adopt GH494's future differential artifact class. If a family observation
is recorded through GH495, cite that recording as provenance and preserve the v1 owner meaning
for this registered cell set; do not reuse a differential denominator as a v1 denominator.

Migration is additive registration/cell/test/snapshot work, followed by owner-approved behavior
changes. No new KEL74 field, scorer, wire frame or public API is introduced here.

Runtime seam: tests exercise existing host resource/profile/connectivity paths with isolated
inputs. Host grants, resource bounds, native teardown and callback lifetime remain with their
implementing owners. A local promise rejection is not a successful native cancellation proof.

## 5. Boundaries

Future work: keld-compat session/protocol/net corpus files and registration; existing mapped
family test targets and doc snapshots. This publication creates only this file.
Do not implement protocol handlers, partitions, profile clearing, network APIs, permissions,
DownloadItem, grant vocabulary or runtime bridges under this issue.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Approve this spec and reconcile exact GH471/GH474/GH555 policy versions; choose
  registered CellKeys and pending-versus-permanent divergence without changing their owners.
- [ ] T2 Add pinned protocol and session citations plus current-behavior regression cells,
  including timing, storage names and rejected partition cases.
- [ ] T3 Add online-state cells with independent false/true and before-ready controls, preserving
  strict unknown and the selected legacy policy.
- [ ] T4 Execute real macOS engine restriction and clear-completion fixtures with isolated
  profile data and negative controls; retain other platforms as unknown.
- [ ] T5 Produce approved GH663 receipt/records from admitted executions; update a cell to pass
  only in the actual implementing change and keep divergence counts separate.

## 7. Test plan

| AC | Positive and independent negative evidence |
|---|---|
| 1 | Shared snapshot/quote and expected-status validation; edit only one status or citation |
| 2 | Separate-process registration cases; allow late/repeated registration or privilege escalation |
| 3 | Controlled handler request/response; bypass guard or alter declared body/response contract |
| 4 | Actual engine relative-resource/storage attempts; relabel an engine mismatch pass |
| 5 | Empty/default and named partitions; invent persistent identity or lie about dev persistence |
| 6 | Seed/read/clear/read with real completion; resolve before clear or silently omit a requested type |
| 7 | Explicit connectivity policy; interpret true as guaranteed remote reachability |
| 8 | Engine-tagged lane receipts; substitute mock or another platform's result |
| 9 | Coherent implementation/evidence update; flip pending or intentional failure green alone |

Tests are planned, not run. Use controlled origins, isolated profiles, explicit readiness and
clear barriers; do not use sleeps, internet reachability or destructive real-profile clearing.

## 8. Review gates triggered

Documentation publication: none. Future conformance work needs exact human spec approval,
shared receipt owner approval and real engine acceptance. Runtime public API/permission/profile
changes belong to GH471/GH474/GH555 and their own gates. No dependency/wire change is proposed.

## 9. Perf impact

No production cost or benchmark claim. Measure fixture runtime, retained response bytes and
profile cleanup during real qualification. Keep native/backend bounds from their owners;
conformance assertions cannot relax them to make a corpus green.

## 10. Open questions (block implementation)

- Exact after-ready Electron error text needs a verified pinned run.
- Confirm each approved implementing policy and pending/divergence owner before cell admission.
- Per-engine custom-scheme behavior and real clear completion remain unexecuted.
- GH663 receipt/admission prerequisites remain separate; Windows/Linux and strict-net are unknown.
