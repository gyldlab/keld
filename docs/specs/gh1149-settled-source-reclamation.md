# Spec: PR1181 settled Source snapshot reclamation
Status: approved — bounded implementation under human conditional authorization; production qualification remains open
Tracker: GH-1149; integration: existing PR1181/GH449 custody
Updated: 2026-10-27
Source: fab00474c045f5b5b303e7f8752c2004af5ea10f

## 1. Goal and non-goals
Release full closed-window Source snapshots after their terminal fact dispatch while retaining exact observed-ID membership in the existing map. Preserve blocking destroy, earlier facts/answers, current cumulative Host admission candidate1024, accepted pending Destroy64, and recovery/adoption. Correct two existing consumer violations required by this lifetime guarantee: eager Source disposal on explicit close, and throwing close-receipt bookkeeping that replaces an earlier answer.
No cap removal, replacement quota, publication serial, wire/permission change, native rewrite, merge, physical-byte upper-bound claim, long-session capacity claim, or acceptance closure. Host Entry/native storage stays unchanged: native resources and the Destroy vector already leave at release; a Host-container replacement is deferred absent a measured benefit. This is the smallest independently separable part of the authorized first stage, not a claim that every original memory obligation is solved.

## 2. Governing contracts and approval provenance
Public approval and scoped claim receipt: https://github.com/gyldlab/keld/issues/1149#issuecomment-6426874535.

GH531 §4.d mirror and §4.f facade; GH1142 approved AC1–7, especially AC4/5/7; GH1149 source/consumer qualification. Frozen note255 is a proposal, not prior approval. Human conditionally authorized stage-one implementation after source comparison, counter-questioning and isolated other-agent review/refutation validate it. Two isolated reviewers and a separate refuter inspected exact source; the refuter admitted the corrected direction, with the preservation constraints below. Exact draft SHA256 b9a0e5cf186cd4435b09a4f05da74ff8e2e9305fc745ddb00d9d7b3c2927468d was independently admitted for bounded implementation after review and separate refutation; this status/provenance update changes no technical acceptance or design requirement. Source/consumer proof and all PR acceptance holds remain open after design admission.

## 3. Binary acceptance
A1: After Created and applied Destroyed, the matching full terminal snapshot remains readable through blocking Destroy's continuation and while that Destroyed callback runs. After its dispatch completes (including throw), Source owns no full snapshot for that ID.
A2: Retired history still rejects Created with either the old incarnation or a different incarnation for the same observed ID. Other facts require an existing nonterminal exact pair. Unknown/stale/post-terminal facts retain sticky IPC005; sparse IDs including u32::MAX remain legal within the observed-count guard.
A3: Retiring a snapshot does not reduce Source cumulative census; the 1025th distinct Created still invalidates. Host failed-build charging, adoption-before-new-admission, exhaustion precedence and Destroy64 remain unchanged.
A4: Explicit LifecycleLink.close rejects further calls immediately, closes WorkerLink, and lets successfully applied earlier facts dispatch before Source callback/map disposal through existing onEnd. Subscription-error cleanup uses the same path. The first Source/transport cause is preserved; no fabricated terminal events.
A5: A correlated ReplyApplied received before a later Source failure still resolves invokeWindow to ReplyApplied. Receipt bookkeeping is Source-owned, nonthrowing for failed/absent/terminal pairs, and performs no mutation in those cases. Later Source reads retain their failure. A live exact pair still advances the dispatched receipt's acknowledged watermark even when its pending close sequence has advanced beyond the acknowledged one; only a matching sequence clears Source pending state.
A6: Nested Destroy, callback throw, late import, replay/adoption and terminal source cleanup preserve existing wrapper identity, order and tombstone behavior. No new global terminal-wrapper map or separate admission/dispatch queue.
A7: Demonstrate full-snapshot reachability reduction and actual retained-heap observations on the same Bun/runtime/build and equivalent workload; include retaining-snapshot negative control and independent ordinary-link callback-root control. Charge retained Map backing and shared callback roots once. This is not a complete physical bound or RSS claim; GH1142 AC5 stays independently gated.

## 4. Design and reuse
No boundary change to native handles, crash ownership or principal minting. Keep the Host registry, router locks/writer, native classification, BuildFailureOutcome and ReleaseOutcome unchanged. Their existing failures, ERR041→eligible deferred LastWindowClosed and T11 ordered effects remain compatibility requirements, not new bugs asserted here.

Reuse WindowSource's existing Map<number,...> with a shared scalar terminal marker (for example null) as its retired value. The key is exact observed WindowId membership; no second per-ID container is necessary. Created checks Map.has(id), not the value's truthiness. The map's size remains the historical observed census. get(ref) returns undefined for a retired marker; all() omits retired markers, preserving host-ID order of remaining full snapshots. Caller-held immutable snapshot copies remain caller-owned. Nothing infers a historical incarnation or publication from an allocated numeric frontier.

Applying Destroyed keeps its current immutable full terminal snapshot. Source.dispatch invokes its existing callback, then in finally replaces only the exact matching host-destroyed snapshot with the shared marker. It never removes an earlier helper fact or invokes user code from apply. WorkerLink's existing callBlocking wake applies facts before returning and defers listener dispatch; therefore the synchronous destroy continuation reads terminal evidence before the matching terminal dispatch can retire it. Reentrant dispatch cases must verify this existing boundary; do not add a waiter/receipt counter without a reachable trace requiring it. attachMirror retains its decode-once, exact-object, release-before-user-callback behavior unchanged.

LifecycleLink.close closes admission via its existing #closed state and delegates termination to WorkerLink; Source.end runs in existing onEnd after retained-record drain. Remove eager disposal rather than add a second terminal queue or finalizer. The Source remains governed by its existing state failure/terminal semantics; do not claim immediate Source disposal at explicit close.

Reuse Source.acceptHostCloseReply as the single internal eligibility/settlement owner, returning whether the exact pair is still live and eligible for receipt bookkeeping. Failed/absent/destroyed Source returns false without throwing or mutation. For a live exact pair it returns true even if pending_close differs; it clears pending state only on exact sequence match. API uses this result to gate existing WindowCloseReceipts acknowledgement, so later failure cannot replace the earlier Host response and old acknowledgements still suppress delayed old dispatch. No broad catch, error swallowing, new public export or duplicated policy check.

## 5. Boundaries
Production: packages/@keld/api/src/window-source.ts, link.ts, app.ts only if required by this design. Tests: existing Source/link/ack fixtures and actual WorkerLink integration; owning docs GH531 §4.d with this approval provenance. No production Host/Rust/native/transport implementation, wire table, guard, Cargo/dependency, global instructions, or parallel state machine changes. Test fixtures may extend owning Rust integration harness after nearest rules are read. Generated documentation comes from the current generator.

## 6. Ordered work
1. Bind this exact text to final independent admission and human conditional provenance, refresh custody and claim scoped paths.
2. Add failing regressions for A1/A2/A4/A5; retain existing native recovery and identity/capacity tests as independent controls.
3. Apply the minimal Source/link/API change; record literal controlled-seam, real-link and native observations separately.
4. Measure source-specific retention with identical workloads; retain only justified changes. Run required repository and scoped gates, then independent exact-diff review/CodeRabbit.
5. Publish reviewable code in existing PR workflow if custody remains uncontested. No merge, issue closure or qualification approval.

## 7. Tests and falsifiers
A1–A3: source tests for apply versus dispatch lifetime, throwing callback finally, same/different incarnation duplicate Created, sparse IDs, max/max+1 historical census; omission of terminal-marker replacement must fail actual reachability observation.
A4: actual WorkerLink blocking-return→immediate-close→earlier fact exactly once→onEnd; eager Source.end is the negative control. Controlled-seam diagnostic already fails on baseline and passes when eager disposal is removed; this is not real-link proof.
A5: actual WorkerLink ReplyApplied→invalid fact in one fenced drain→API answer preserved and sticky source failure; existing throwing Source.get is the negative control. Controlled-seam diagnostic already fails baseline and passes a narrow bookkeeping-eligibility control. Preserve old-sequence/new-sequence and acknowledgement-before-dispatch cases.
A6: existing BrowserWindow recovery/late-import/destroy/throwing-listener regressions and real macOS normal close/veto/destroy/recovery tests. Native Retina and exceptional whole-Host close/Quit scope remain separately unverified, not waived.
A7: source-specific before/after graph and allocation diagnostics at H=0,1,1024 with equal live/pending/fact counts; caller-held snapshot controls, shared ordinary-link callback controls and terminal after-drain proof. Exact runtime/build and raw results required; no imported Bun1.4.2 formula.
Run actual format, warning-denied workspace Clippy, workspace tests, TypeScript/Bun, relevant llms/generated/policy checks and exact-head review. Timeouts are kill switches; synchronization uses observable fences.

## 8. Review gates
Public API/compatibility lifecycle and source-memory ownership: independent evidence required on exact diff. Permission model: no change; preserve stale-generation controls. Unsafe: no production change; test-only operations follow nearest owner. Dependency: none. Wire protocol: no wire change; validate unchanged ordered transcript through existing harness. No gate is closed by this document.

## 9. Performance and memory
Expected structural result is removal of four copied snapshot objects per settled historical Source entry, while Map keys/backing and a shared marker remain. Verify actual graphs/bytes before claiming savings. Host native release and moved Destroy vectors are existing behavior, not a new benefit. Shared callbacks, pending facts, wrappers, allocator capacity and native queues remain separate terms; native mpsc remains unbounded. No speed, net whole-Host bound or lifetime-independent scaling claim.

## 10. Open decisions
No new human-owned public/wire/admission decision is requested. Independent exact-text admission passed for the draft hash above; implementation evidence remains a successor requirement, not a fabricated prior pass. If a reentrant trace refutes dispatch-based retirement, stop and revise within the owning contract before adding any new retention owner.
