# GH541 boundaries and tasks

Part of: [index](index.md)
Owns: §§5–6, including KEL-102 T7

## 5. Boundaries: integration ownership

| Owner | Allowed responsibility | Explicit boundary |
|---|---|---|
| #541 | Companion amendment; scoped KEL-102/KEL-142 wording; architecture 03 correction; approval packet | No product implementation; old approval digest preserved |
| #552 / keld-guard, keld-ipc, diagnostic CLI, narrow keld-core forwarding adapter | See the #552 responsibility below. | See the #552 boundary below. |
| #542 / keld-wv | Checked navigation state, host stamp, attempt correlation, lifecycle validation/accessor, restricted application-document bind eligibility and native tests N01–N12 | Restricted eligibility is proposed scope for exact #541/#542 approval; consume host resource-containment identity, no new web.navigate schema, core registry, or Bun generation mint |
| #540 | wv-link v2 channel representation, typed errors, bytes/credits and opaque payload rules | Must consume the grant and generation contract; cannot infer approval of permissions |
| #465/#543/#544 | Send, outbound event delivery, endpoint codec implementation | Bounded queue/revocation and original target identity |
| #466 | ipcMain/once/reply/senderFrame/webContents facade | Cannot introduce facade-only authorization or direct Bun ingress |
| #459 | Running-app first-proof `loadURL(file://)` seam and #1096's cleared pending-call handoff | Consumes #542; does not acquire the separately owned reload/stop/rejection facade vocabulary |
| #538 | Later full webContents vocabulary including reload, stop, and loadURL rejection | Consumes the shared navigation contract; full surface is not invented as a first-proof blocker |
| #460 / KEL-79 | Guard-first navigation policy and deny-only application hooks | Owns general navigation grants; #542 owns the proposed narrow first-proof binding restriction if this remains deferred |
| KEL-102/T4 | Complete and qualify the designated shared-dispatch media integration | Media remains denied outside its separately approved scope, including any initial-generation-0 path; channel grants cannot enable capture |
| KEL-143 | Bun generation recovery and surviving document behavior | Navigation counter is unchanged by Bun restart |
| #499 | First-proof migration artifact | Main literal registrations intersect observed trace |

**#552 responsibility:** One parser/evaluator extension, typed query/context, exact renderer-to-Bun `dispatch_privileged` composition in `keld-core/src/app_session.rs` or its owned extraction, zero-write integration oracle, explain parity, allocation test

**#552 boundary:** Core-path addition is proposed scope for exact #541/#552 approval; generic names only in IPC; no Electron codec; no second counter

Keep shared wire/schema changes and other owners' branches within their recorded gates. Refresh tracker claims and main before implementation. The [#541 specification claim](https://github.com/gyldlab/keld/issues/541#issuecomment-6370773598) records this draft's coordination; it does not authorize product implementation or merging.

## 6. Tasks: dependencies

### One new KEL-102 task unit

**T7 — exact window channel grants and shared navigation identity**: consume the exact-content-approved #541 amendment; implement via #552 and #542 with their separately approved implementation specs; coordinate the transport owner; prove the acceptance matrix before enabling Electron traffic. T7 does not mark media T4, role T5, containment, or Electron oracle work complete.

### Execution order

#### T-1: GH541 documentation and merge acceptance

- **Blocked by:** none

1. Preserve the recorded #541 exact-content decision, including the explicit clarifications in §10 and paired architecture/spec edits. Complete final documentation/review gates and the still-open #541 merge acceptance; merge intent remains do-not-merge.

#### Remaining execution steps

2. In parallel, finish #464's failing-first conformance corpus and #540's protocol specification. Their drafting has no open native dependency on #541. Freeze their common identity/channel interface before integration.
3. After #541, approve and implement #552, explicitly including its narrow core forwarding adapter and P02/N02/N09b composition tests. After
   #541 and #464, approve and implement #542, explicitly including native attempt correlation and the restricted bind gate N11. Policy unit
   work may proceed independently, but the composed #552 zero-write claim waits for the trusted #542 stamp/liveness seam and the #540
   transport contract. Require the accepted T4 shared-dispatch evidence before claiming #552's repository-wide acceptance criterion.
4. Integrate #465/#543/#544 and #466 only after their recorded predecessors. Consume approved #1096/#1105 changes with their owner if available; do not allocate substitute errors.
5. Run the required real macOS boundary/race tests and exact allow/deny app-link observer. Maintain UNKNOWN Electron navigation rows until #551's later recorded oracle.

### Verified native dependency map

| Issue | Native predecessors at review time | State relevant to this review |
|---|---|---|
| #541 | none | Open; [specification claim recorded](https://github.com/gyldlab/keld/issues/541#issuecomment-6370773598); implementation gates remain |
| #540 | none | Independent spec work may proceed |
| #464 | #532, #566 | Both predecessors closed; #464 itself open |
| #552 | #541 | Blocked on amendment |
| #542 | #541, #464 | Both open |
| #465 | #464, #540 | Both open |
| #543 | #464, #540, #542 | All open |
| #544 | #464, #540 | Both open |
| #466 | #464, #465, #543, #540, #552, #542, #544, #473, #597 | All open except #597 |
| #459 | #458, #449, #567, #542 | All open; contractual #542 edge added and read back on 2026-10-23 |
| #499 | #497, #586, #452, #453, #491, #420, #541 | All open except #420; contractual #541 edge added and read back on 2026-10-23 |
| #551 | #494, #464, #495 | Later recorded-oracle milestone; do not turn it into an invented first-proof blocker |

Under the owner-delegated decision, the two contractual dependencies were added to native tracking and read back without removing existing predecessors. The owning issues receive explicit handoffs; no implementation claim or ownership transfer is made. Unassigned issues do not imply permission to take over active related PRs. #1095/#1096/#1105 retain their existing ownership.
