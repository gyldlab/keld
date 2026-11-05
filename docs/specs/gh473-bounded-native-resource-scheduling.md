# Spec: Bounded native resource scheduling
Status: approved
Linear: GH-473 (#517) · Owner: @0monish through delegated Root source-owner decisions · Updated: 2026-11-02

## 1. Goal & non-goals

A healthy burst of local resources waits for the existing source reader instead of failing because two native tasks are outstanding. Qualified pending metadata, submitted file work, UI delivery and actual memory custody have separate bounds under the existing owners. Sixty-four total native tasks can be retained; at most one read/result is submitted until its UI response and owned references actually drop. Every response still requires the original native ticket, source and document lease.

Non-goals: application-specific scripts or filenames, a new resource protocol, another reader/thread/runtime, wider CSP or permissions, a new ledger, streaming or relaxed hash verification, unlimited admission, retrying deterministic failures, performance improvement claims, and attributing the independent preload replay no-commit failure to resource scheduling.

The initial two-task source limit is current behavior, not acceptance of this proposal. Root publishes this approved spec before adopting the separate numeric policy patch. Native memory and original-application qualification remain acceptance work.

## 2. Spec refs

Architecture 05 §2 owns host-mediated resource delivery and current view/navigation binding; architecture 03 owns default-deny authority; architecture 02 owns IPC and receives no protocol change. Architecture 01 §5 remains the performance-budget owner. GH473 owns native task/source custody, GH499 owns the authenticated graph and retained directory source, GH542 owns resource-document leases, and GH553 owns verified response CSP. This specification changes GH473 scheduling and admission, not their authorization contracts.

The scoped source-owner decision approves total native capacity64, one submitted read/result, the existing reader/channel1, maxasset32MiB, source-retained256MiB and unchanged URL/header caps. The same delegated source-owner decision approves variable checked metadata charge with a hard40KiB maximum per retained native task, using actual owned path capacity and record size. Root publishes/adopts this approved contract before changing cap2; implementation remains subject to its acceptance gates. Repository `AGENTS.md` remains the exact unsafe-selector owner. No architecture destination contract is claimed implemented merely by this spec.

## 3. Acceptance criteria (binary, each becomes a test)

1. AC1: with one actual first-party file read held by the existing source gate, five sibling CSS/JS requests are admitted without native failure and without starting another read. After gate release all exact script effects and CSS properties appear once, authenticated bodies match, and the page reports zero load errors.
2. AC2: capacity0 denies the first task; capacity64 retains exactly64 pending+active native tasks; the65th is explicitly refused before task retention or file read. Released capacity admits a healthy successor. No permanent unbounded fallback exists.
3. AC3: service follows ticket incarnation order, independent of pointer value. A worker job, queued result or UI delivery prevents another submission. Cancellation may remove a ticket but cannot reorder remaining live tickets.
4. AC4: pending admission reserves source-wide metadata credit before native retention. Bodies reserve four-copy headroom only at dispatch. Count, charge, maxasset32MiB and aggregate256MiB boundaries reject exact max+1 and checked overflow. A failed reservation neither starts work nor remains charged.
5. AC5: a stopped pending task never starts a file read or receives another native callback; its local metadata is released. A stopped active task stays charged until actual result ownership or shutdown join allows release. A reused native identity receives a different incarnation; an old completion cannot stop or release its successor.
6. AC6: a queued task whose view/source/document lease became stale is retired without file read or native callback. Every native response/data/finish/failure remains covered by the existing currentness and callback guards. Native task-currentness is never inferred from page URL text.
7. AC7: DestroyWindow and Quit with pending+active work tombstone all tasks and cancel the existing reader immediately. View, source FD and native owners remain retained until the actual worker join wake and all UI delivery custody complete. No callback occurs after stop; metadata/body credit drains and exactly one WindowReleased follows. A blocked read or missing completion wake cannot fabricate successful cleanup.
8. AC8: source bytes still pass exact size/hash and nofollow checks through the retained source. Tampered, absent, escaping, oversized and stale reads retain existing refusals. No page or Bun role acquires filesystem or native authority.
9. AC9: no submitted read is retried on channel Full or Disconnected. A retained-byte shortage with no guaranteed credit wake receives explicit failure. Subsequent healthy work recovers after real capacity release, without sleep synchronization.
10. AC10: static flag-gated admission traces distinguish task capacity, receiving-view mismatch, duplicate native task, incarnation exhaustion and shutdown before attributing an original-app failure. Trace off performs no new diagnostic calculations. Logs contain no pointers, URLs, credentials or payload.
11. AC11: all required pure-state, source, native lifecycle, max+1 and negative-control gates pass on the exact integrated source manifest. The actual original application is assessed separately; a first-party workload pass never becomes original MVP proof.

## 4. Design

No boundary change: the native handler still owns UI-only WK references, the existing ReaderCustody still owns one worker and join witness, the source still owns its retained directory and source-wide credit account, and the document owner still mints/revokes resource leases. No new principal, handle owner or crash domain is introduced. Pending lifetime and admission policy do change.

The pre-decision atoms are admission (TaskLedger identity/count), pending metadata (existing native tasks map), work (existing reader/channel), authorization (source/document), lifetime/revocation (ledger/custody) and evidence provenance (Root exact-source native runs). Admission proof does not establish authorization; callback denial does not establish worker termination; a pure state test does not establish native behavior. Dependencies are explicit: metadata admission precedes dispatch, dispatch requires the original live lease and body credit, and release consumes actual work/delivery completion. Original trace attribution and preload replay remain independent unknowns until their own observables pass.

Reuse decision: increasing the old task cap alone leaves racing `sync_channel(1)` submission failure and eagerly reserves every body. Adding workers, another queue container or a second identity ledger is unnecessary. The existing BTreeMap already owns retained native tasks; it gains an optional qualified `SelectedRead` and body-charge value while waiting. TaskLedger gains a queued marker and UI-delivery custody marker, and selects the smallest existing admission incarnation. No standalone FIFO container or counter duplicates ownership. Compatibility fallback is not required because no public API or resource response semantics change; explicit bounded refusal still applies beyond the admitted envelope.

The private state interface is `queue_read(ticket)`, `next_queued_read()`, `delivery_started(ticket)`, `delivery_quiesced(ticket)` and `has_active_delivery()`. `worker_started` remains the sole work claim. `next_queued_read` returns none while any submitted job/result or delivery is retained. The selected request moves out of the same native map into the same ReadWork message only after dispatch revalidates currentness and reserves body credit. Actual result handling is wrapped so all response buffers/native references/RAII body credit drop before delivery custody clears and the next work dispatch runs. Reentrant native callbacks therefore cannot create overlapping body ownership by observing worker quiescence early.

Pending charge retains the existing conservative `4*(URL UTF16 + header UTF16)` conversion allowance, bounded response-policy overhead4096, actual catalog policy bytes, the actual owned selected path capacity and `size_of::<PendingNativeTask>()`. At allowed maxima the first term is24576 bytes and the selected path bound is8192 bytes, so a32KiB fixed record charge is insufficient before policy/record overhead. The approved variable charge is rejected before native task retention unless checked arithmetic succeeds and the derived charge is at most40960 bytes (40KiB). At64 tasks, accounted retained metadata is at most2621440 bytes (2.5MiB), within the unchanged aggregate source budget. Actual String capacity, not merely text length, and compiled `size_of` are charged. If the actual record or future policy exceeds this envelope, admission explicitly refuses; the implementation never silently expands the limit. Opaque WK object/native allocator RSS is not quantified and needs measured qualification. Body charge remains four times authenticated size, capped at32MiB per asset and256MiB source-wide including retained old results/navigation aliases. Count is an independent upper bound, not a promise that every maximum-sized combination fits bytes.

The reader remains a single named thread and `sync_channel(1)`. Normal dispatch is driven by initial admission or actual result consumption; no polling, sleeps, retries, additional transport or body-credit wait is introduced. A byte shortage retains explicit refusal when no guaranteed wake exists. A submission fault retires its task and may service remaining FIFO work; no per-turn CPU latency bound is claimed for native reentry, and fault/reentry behavior requires the native acceptance cell.

Stop tombstones the native ticket before disposal. Pending-only tasks have no worker; their metadata can drop immediately. Active tasks retain work and byte custody until actual response consumption or joined reader shutdown. Stale qualified metadata and stale results receive an exact-incarnation terminal transition before ledger release; pointer-only stop never retires a successor. Destroy/Quit calls the existing ledger's `stop_all`, including delivery tickets temporarily absent from the native tasks map, and cancels the reader; `try_finish_shutdown` additionally refuses release while UI-delivery custody is active. Native/source references are not cleared before the exact join witness and delivery quiescence.

Capabilities/manifest changes: none. Wire/protocol changes: none. Runtime seam: existing UI→single source worker→Tao UI result, same process/crash domain and authority, with bounded metadata waiting on the existing UI owner. Configuration is the Host-controlled fixed intake policy, never renderer input. Migration unit: intake ledger, native handler, same debug first-party fixture/lifecycle callers; source parser/reader, generated contracts, persistent state and public compat facade stay unchanged. Platform scope is macOS only; no Windows/Linux qualification is implied. No new unsafe operation is required.

## 5. Boundaries

Implementation: `crates/keld-wv/src/wkwebview/macos_resource_intake.rs`, `macos_resource_native.rs`, and bounded extensions to existing S1 resource fixture/lifecycle tests; this spec and exact evidence receipts. Existing `macos_resource_source.rs` may expose an existing debug source gate only if fixture integration requires it, without changing verified-read authority.

Do not change generated code, Cargo dependencies, guard policy/catalog, IPC, authentication, source graph parser, native selector allowlist or original application assets. No special-case MIME, script or application behavior. Root remains canonical writer, Cargo builder and real OS/original-app operator.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1: publish static admission diagnostics and capture the actual rejection category on the exact existing source; retain the live result and source manifest.
- [ ] T2: publish this approved numeric/lifetime contract, demonstrate the old policy regression, then integrate existing-ledger FIFO/credit/custody changes with pure-state and source negative controls.
- [ ] T3: integrate the fixed first-party native held-read fanout fixture, max/max+1, pending cancellation/currentness and pending+active Destroy/Quit cells; measure native retention and complete source/unsafe review on the final diff.
- [ ] T4: run workspace gates, native baseline matrix, actual original application and separate replay cell; record precise passes and blockers without conflating first-party and original evidence.

## 7. Test plan

AC1/AC3: authored index/bootstrap/hold/two-CSS/three-JS fixture. Arm existing ReadGate for hold.txt, await actual Held, invoke the fixed fanout, observe six native tasks before releasing the gate, then require exact CSS properties, JS effects, body bytes and no errors. Timeout is only a kill switch. No clock-based release or retry.

AC2/AC4: ledger zero/max/max+1 and checked overflow tests, source-credit RAII tests and native admission census at64/65. Measure actual Rust/source account and opaque native retention separately. Policy regression using the existing begin API is behavioral RED at cap2; new interface tests alone may initially fail compilation and are not claimed behavioral RED.

AC3/AC5/AC7: deterministic FIFO test with out-of-order native identity keys; active worker and terminal/UI-delivery exclusion; stop/reuse; exact credit held through response drop; existing join-wake/blocked-reader tests plus native pending+active Destroy/Quit and healthy follow-up. Native crash/lifetime tests use dedicated children and check exit status, callback count, release count and real termination.

AC6/AC8: stale queued document/source before release, stale completed result, wrong view, existing nofollow/hash/size faults. Negative controls remove dispatch-currentness or source verification one at a time and require the matching independent failure.

AC9: actual stopped/disconnected reader fault, capacity pressure and recovery without retry; native reentrant callback observation checks that no second body starts before prior custody drops. AC10: static category trace on/off. AC11: exact-source manifests, rustfmt, warning-denied workspace Clippy, workspace tests, applicable generated-doc gates and all native cells. No unrun OS is reported passed.

Required negative controls additionally restore old cap2, replace incarnation ordering with pointer order, bypass work/delivery exclusion, release before join, and suppress the exact join wake. The first-party fixture's script/CSS effects, real native callback census, exact owned-byte accounts and actual child termination are separate oracles.

## 8. Review gates triggered

unsafe: affected existing native callback/lifetime boundary; named independent exact-diff native/unsafe evidence required even though no new unsafe operation is added. public API: none. permission model: none. dependency addition: none. wire protocol: none. External review remains user-owned; a specialist source inspection is not independent gate evidence.

## 9. Perf impact

No performance improvement claimed. The one-outstanding-result policy may reduce worker/UI overlap; it bounds ownership rather than proving speed. Startup/resource completion latency and native high-water may move, so compare the same first-party/original workload and environment before and after with attributed census, work, queue/copy, clock, statistic and artifact. Architecture01 §5 budgets remain in force; any measured >5% regression needs the repository-required written waiver and benchmark evidence.

## 10. Open questions

None. The delegated source-owner decision approves max64 pending+active total, max1 submitted read/result through final UI drop, variable checked metadata charge bounded by40KiB per task, existing reader/channel1 and unchanged asset/source/URL/header caps. Root records publication/adoption before the numeric cap patch. Opaque native high-water, cancellation/currentness/fault behavior and original-application success remain required acceptance evidence; none is assumed passed.
