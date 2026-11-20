# GH541 test plan

Part of: [index](index.md)
Owns: §7 and its control IDs

## 7. Test plan & evidence: independent controls

These are required implementation acceptance tests, not assertions that a failure
has been reproduced or that the proposed boundary is implemented. This specification
supplies no new native qualification. Each owner must record the exact revision,
platform, command and observed outcome before claiming its criterion complete.

Each expected-red test must be observed failing before the corresponding product
change when its real harness is available. A negative control is a deliberate single
fault that the test must detect. Preserve paired valid controls. A source inspection,
a state model, or a page-only observer cannot prove a native effect boundary.

| ID / criteria | Contract under test | Independent required oracle | Single-fault negative control |
|---|---|---|---|
| P01 / 1–2 | Selected Windows Legacy artifact opt-in and strict channel parsing | An unmarked exact list grants nothing. Marking it enables exact channels only. Malformed opted-in input yields GUARD005. Existing CSP validation runs in both cases. | Ignore the marker. Separately disable decoded-string validation. |
| P02 / 3 | Guard-before-Bun forwarding | Pin GUARD003 and its complete text; independently observe zero app-link writes and zero handler entries on denial, one each for the exact permitted control. | Bypass dispatch_privileged. |
| P03 / 4 | Per-window grant isolation | A host-authenticated window lacking a literal cannot use another window's grant; independently observe no ingress. | Use a global grant union or renderer-selected window key. |
| P04 / 5 | Exact channel equality | Distinct prefix, suffix, case and Unicode spellings are denied unless separately granted. | Replace equality with prefix matching or normalization. |
| P05 / 4,16 | Host-owned renderer identity | Authority fields supplied by a page cannot change the native principal or permit an unauthorized effect. | Derive authority from a renderer header. |
| P06 / 4,6 | Channel/native capability separation | Channel permission cannot enter a native adapter or permit capture; paired permitted app operation still succeeds. | Route a native capability query through the channel-grant branch. |
| P07 / 6 | Single authoritative evaluation boundary | Review authority-bearing callsites and prove real denial through the shared dispatcher; preserve the diagnostic-only exception. | Add a direct-authority evaluator caller. |
| P08 / 7 | Allocation-free allowed dispatch | A counting allocator around the actual warmed policy/dispatcher path observes zero allocations. | Clone a channel or window string on the allowed path. |
| P09 / 8 | Diagnostic parity without authority | Explain and authoritative evaluation yield the same decision; diagnostic output cannot produce live admission or invoke an effect callback. | Use an independent matcher or export a usable live permit. |
| P10 / 1,20 | Immutable session policy | A policy-file edit does not change decisions in an existing session; a fresh authorized session uses the new snapshot. | Reparse policy per request. |
| N01 / 9 | Single navigation source | Capture equality between the trusted admitted-request stamp and guard-principal generation. | Rotate only one of those values. |
| N02 / 10 | Revocation orders queued admission | Use deterministic barriers to prove that revocation winning before handoff prevents Bun ingress, with a permitted control. | Omit the final currentness comparison. |
| N03 / 10,19 | Document-scoped completion | A completion whose captured document differs from the receiving document has no callback or application effect; a matching completion succeeds. | Remove the document-binding comparison. |
| N04 / 11 | Same-document preservation | Real WKWebView same-document events preserve the stamp, nonce and valid pending result. | Rotate on a same-document event. |
| N05 / 12 | Cancelled renderer-navigation preservation | A real cancelled provisional navigation preserves the current document identity and next valid send. | Rotate at renderer provisional start. |
| N06 / 13 | Host-load admission closure | The native harness proves that AwaitingCommit admits neither bind nor invoke from a retired document; a qualified successor is admitted. | Remove the AwaitingCommit refusal. |
| N06b / 13 | Native navigation-attempt correlation | Only the expected native attempt can satisfy a pending transition; mismatched and retired attempts cannot admit a document. | Omit the expected-native-attempt comparison. |
| N06c / 13 | Native event identity and duplicate handling | A duplicate event cannot rotate a healthy binding twice; an unrecognized replacement still closes authority. | Treat every callback as a new commit. |
| N07 / 14 | Generation exhaustion | At the maximum u32 generation, the next invalidation produces permanent refusal and no dispatch or delivery. | Use wrapping/saturating arithmetic or unchecked narrowing. |
| N08 / 15 | WebView lifetime revocation | A native subprocess proves no post-destruction effects or callback authority; unrelated live views remain healthy. | Omit the destroyed-state or lifetime comparison. |
| N09 / 17 | Bun recovery isolation | Old and new links have distinct authority; no replay occurs, navigation identity remains stable, and a fresh request succeeds. | Replay pending work or rotate navigation on Bun restart. |
| N09b / 17 | Queued work remains bound to its admitted Bun session | Controlled recovery proves that old admitted work cannot enter the successor link; a newly admitted request can. | Select the current link at dequeue without comparing the captured generation. |
| N10 / 10,19 | Back-forward-cache document binding | Real restore and rebind evidence proves that a retired document binding is never reused as current authority. | Reuse a retired cached nonce. |
| N11 / 4,9,16 | Qualified application-document admission | The real native binding gate accepts approved application documents and refuses documents outside the approved set. | Admit a document from its WebView id alone. |
| N12 / 10 | Bounded final handoff and revocation progress | Independently stalled transport cannot prevent native navigation/destruction and revocation within the harness deadline. | Hold a revocation-critical lock across blocking transport or handler waits. |
| W01 / 16,18 | Bounded public-relay envelope | The production relay and native harness accept no wider effect-bearing request than the reviewed facade contract. | Remove an admitted-source, channel, byte or request bound. |
| W02 / 18 | Queue and credit lifetime bounds | A held consumer cannot exceed message/byte bounds; every reservation is retired exactly once across lifetime changes. | Remove a bound or release credit twice. |
| W03 / 18 | Payload snapshot integrity | Mutating caller-owned bytes after admission cannot change the admitted payload observed independently at the receiving boundary. | Omit the required synchronous payload copy. |
| E01 / 3 | Stable denial propagation | An independent code/message/target observer proves the exact ChannelForbidden contract survives delivery. | Substitute a generic error for GUARD003. |
| M01 / 20–21 | Restricted migration grant source | Compare emitted literals with main-side literal registrations and the required trace intersection; no wildcard or renderer-derived grant is emitted. | Use preload/page names or an unrestricted observation union. |
| C01 / 22 | Conformance provenance | The corpus validator refuses an Electron PASS without the required pinned recorded oracle. | Promote UNKNOWN without the oracle artifact. |

### Required evidence and qualification

The implementation owners must supply actual guard/parser tests, the zero-write
app-link observer, allocation measurements, real macOS navigation/lifetime tests,
and independent negative-control outcomes. No row above is qualified by this draft.
A test plan or a written but unexecuted test is not a passing artifact. Electron
conformance remains subject to #464/#551's recorded-oracle requirements.

Reuse the owning crates' existing parser, guard-dispatch, allocation, wv-link and
native renderer test harnesses. Keep waits tied to observable events; timeouts are
kill switches, not synchronization. Use isolated subprocesses for destruction and
recovery. Retain exact result provenance in the authorized evidence channel.

Handle any suspected or reproduced security defect under [SECURITY.md](../../../SECURITY.md).
A public implementation report must not disclose an unfixed or uncertain defect,
private report content, or reproduction details ahead of coordinated disclosure.
Existing public [#1096](https://github.com/gyldlab/keld/pull/1096) and
[#1105](https://github.com/gyldlab/keld/pull/1105) retain their recorded contract and
implementation ownership; this specification does not duplicate that work.
