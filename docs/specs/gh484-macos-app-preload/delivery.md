# Delivery and evidence

Part of: [index](index.md)
Owns: sections 5–7, 9 and 10

## 5. Boundaries

**This draft PR changes only** `docs/specs/gh484-macos-app-preload.md`. No implementation or registry mutation.

After human approval, #484 may propose macOS world/handler changes in `crates/keld-wv/src/wkwebview/*` with `keld-wv/AGENTS.md` sanctioned unsafe coverage, and a retained app package-source owner in the appropriate host/engine seam. The exact source owner must be coordinated with package boot. #567 owns conformance baseline edits; #467 owns JS facade and
IPC. `gyldlab/keld-benches` owns the one metric registry ID separately.

Do not edit #467's facade, #541/#542/#552 grants/generation/wire contracts, #473 page resource loading, unrelated core session files or a shared architecture document under another claim. Architecture 05 §2 is generic and does not explicitly prohibit a third app realm; propose a reviewed cross-reference only if its owner agrees. KEL-142 original contract
remains unchanged until the separately approved additive amendment is landed.

## 6. Tasks

### T-1 ? F0 pre-approval feasibility
- **Blocked by:** none


- [ ] **F0 pre-approval feasibility (test fixture only):** physical Mac probes demonstrated **3/3** first-inline static wrappers with **2/2** no-wrapper controls, earlier **16/16** first-inline DOM-handshake shapes, **6/6** nested callback local effects and a separate 33-receipt native two-world credential fixture with independent RED mutants. Remaining
      **BLOCKED** gate is authenticated fixed-four page→app *actual-method* selection that does **not** use shared DOM/page-native authority, followed by real #540/#542/#552 Host/Bun proof. Keep all test-only evidence labelled; doc-cited general dynamic graph conformance stays expected-fail/recorded approved divergence, and unqualified engine-only
      observations stay UNKNOWN. If no secure fixed-wrapper composition exists, stop and seek GH467/GH484 owner review rather than use the insecure F0 carrier. GH441 placement remains unchanged.
### T-2 ? S0 specification approval
- **Blocked by:** B-1, B-2, B-3, B-4, B-5, B-6


- [ ] **S0 specification approval:** obtain independent architecture/security review, resolve critical feasibility and root-authority decisions Q-1–Q-3 with evidence, and seek **human sign-off** on this exact contract. Set `Status: approved` only after that sign-off. All later production tasks require S0.
### T-3 ? F1 package source
- **Blocked by:** T-2, B-2


- [ ] **F1 package source:** first approve a packaging-owned independent digest trust anchor and exact manifest path source. Implement retained admitted root, exact staged-absolute-to-relative mapping, **deny-all-symlink** first-proof traversal, bounded one-descriptor read and authenticated immutable snapshot before JS. Verify in-place same-UID racing
      writer, mutable-manifest substitution, root rename and caps with RED controls. If draw.io's staged source needs a symlink, block pending separately approved verified alias mapping.
### T-4 ? F2 world and wrapper
- **Blocked by:** T-2, B-1, B-4


- [ ] **F2 world/wrapper:** add one named app world/handler to the KEL-142 controller, exact nine-module wrapper, renderer process facts, **fixed page-world four-method document-start shape** and app-world real method registry, preserving native eligibility. Do not turn page-wrapper visibility into a bind/grant or claim generic dynamic contextBridge
      publication.
### T-5 ? F3 isolation and grant acceptance
- **Blocked by:** T-2, T-3, T-4, B-3, B-4, B-5, B-6


- [ ] **F3 isolation/grant acceptance (after #540-approved contract, #465/#543/#541/#542/#552 implementations and #464 conformance land):** consume the canonical v2 decode/credit, world-scoped bind and literal grant path; KEL-142 real Mac AC-2 regression, hostile preload, two independent windows, navigation/teardown/recovery and per-window `el:` default-deny;
      independent RED fault for each guard.
### T-6 ? F4 facade composition
- **Blocked by:** T-2, T-4, T-5, B-1, B-3, B-4, B-5


- [ ] **F4 facade composition (owned by #467):** connect approved bounded Option A four-method facade to actual private preload registrations with copied arguments, nested callbacks and approved IPC; prove unmodified draw.io two-request scenario. General dynamic graph stays deferred/expected-fail. #567 flips only eligible F07-T4 cells after its own native
      proof, never arbitrary Electron parity.
### T-7 ? F5 diagnostic
- **Blocked by:** T-2, B-6


- [ ] **F5 diagnostic:** register exactly one handler-cost metric under keld-benches' owner and capture diagnostic-only Mac samples. No threshold or performance claim.

These slices may split for review size without transferring authority between owners.

## 7. Test plan & evidence

**Production acceptance tests below remain planned and UNRUN at `Status: draft`.** The separately identified **test-only F0 macOS oracle** ran the 16/16 handshake, 6/6 nested callbacks and their described RED controls (sections 2 and 4.3); its synthetic IPC and forgeable DOM fail production authority requirements. For implementation acceptance, tests must
observe WebKit/host/Bun independently of implementation self-report, execute each **one-fault temporary RED mutation** on a disposable branch/fixture, restore bytes exactly, then observe GREEN. Assertions wait on events, not sleeps; timeout is a kill switch.

### f07.first_inline_complete

- **AC coverage:** 01–02
- **Independent oracle:** Real WKWebView first inline script captures exactly four **predeclared** callable electron functions and copied/frozen process type/versions, plus **the real preload's applicable conditional `.configuration` state**. Independently observe real current app preload execution/registration and a native guarded call; static page shape
  alone is insufficient and full dynamic graph is unqualified.

- **Individual RED mutation:** Move fixed wrapper to microtask or `AtDocumentEnd` → first-inline RED; **delay only actual app-world preload execution past first inline while keeping wrapper visible** → conditional seed absent (RED); suppress only app-private method registration → wrapper visible but actual-method effect zero (RED). Restore clean fixture for
  GREEN.

### f07.adaptive_seed_valid_nullish

- **AC coverage:** AC-2
- **Independent oracle:** Valid flag for each of `auto`, `simple` and `none`, absent or nullish key in valid configuration object: first inline sees exactly that mode and unchanged unrelated keys

- **Individual RED mutation:** Skip seed on nullish field; positive fixture fails.

### f07.adaptive_seed_no_invalid_flag

- **AC coverage:** AC-2
- **Independent oracle:** No flag, then invalid flag value, with separate initial storage snapshots: each remains byte-identical and still loads

- **Individual RED mutation:** Treat any flag as accepted; invalid fixture detects overwritten data.

### f07.adaptive_seed_explicit

- **AC coverage:** AC-2
- **Independent oracle:** Valid flag with explicit prior non-nullish value, including `false` or empty string: original bytes remain untouched

- **Individual RED mutation:** Replace user value unconditionally; sentinel changes.

### f07.adaptive_seed_malformed

- **AC coverage:** AC-2
- **Independent oracle:** Valid flag with malformed `.configuration` JSON: startup navigation finishes, first-inline methods still exist, stored bytes remain unchanged and console records caught seed failure

- **Individual RED mutation:** Remove preload's catch: the preload wrapper may still permit navigation, but the subsequent `contextBridge` publication is skipped and the exact handled-seed-error oracle goes RED.

### f07.world_and_k142_isolation

- **AC coverage:** AC-1, AC-10
- **Independent oracle:** Existing KEL-142 real Mac AC-2 + separate hostile app preload: trusted handler/nonce unobservable and zero forged native effects

- **Individual RED mutation:** Install app script in trusted world or remove world check.

### f07.require_nine

- **AC coverage:** 03–04
- **Independent oracle:** Exact nine alias probes and `require('fs')` exact error; unsupported members throw typed code

- **Individual RED mutation:** Add `fs` resolver.

### f07.js_exception

- **AC coverage:** AC-5
- **Independent oracle:** Observe exact console marker **and independently completed page load**

- **Individual RED mutation:** Propagate JS throw into navigation failure.

### f07.process_env_argv_versions

- **AC coverage:** AC-6
- **Independent oracle:** Canonical session-facts values, fixed additionalArguments, `Object.keys(process.env).length===0`, no `KELD_APP_LINK`

- **Individual RED mutation:** Clone Bun role env; separately hardcode `versions` then vary canonical input.

### f07.root_path_and_symlinks

- **AC coverage:** 07–08
- **Independent oracle:** Actual unmodified draw.io absolute path maps to **one** trusted staged relative entry. Directory descriptors and inode identity prove no out-of-root read; any symlink (including in-root), stage substitution, traversal and similar-prefix outside path deny before source read.

- **Individual RED mutation:** Reopen by display path, permit in-root symlink, use lexical-prefix-only matching or skip a directory `O_NOFOLLOW` check (separate REDs).

### f07.manifest_external_trust

- **AC coverage:** 07–08
- **Independent oracle:** Prove the expected digest comes from an independently authenticated package/Host image outside the same-UID-writable file tree; tamper editable sibling manifest or change pinned expected digest and get typed refusal before JS executes.

- **Individual RED mutation:** Accept hash derived at the same open/read step or an adjacent attacker-writable manifest; independent trust-anchor observer must go RED. **BLOCKED until packager supplies this authority.**

### f07.source_kind_caps

- **AC coverage:** AC-8
- **Independent oracle:** FIFO/device/dir fail without blocked UI; 512 KiB permitted, +1 denied; aggregate 8 MiB+1 denied

- **Individual RED mutation:** Skip file-kind check or cap (separate REDs).

### f07.inplace_concurrent_write

- **AC coverage:** AC-8
- **Independent oracle:** Independent same-UID writer changes file bytes **in place** during descriptor read, including equal metadata before/after; mixed/changed owned bytes mismatch trusted SHA-256 and cause refusal **before script side effects**. The unchanged trusted image loads; no path reread.

- **Individual RED mutation:** Remove independent expected digest check or substitute `fstat` comparison; forbidden JS marker must appear and test go RED. **BLOCKED until trustworthy digest and real reader exist.**

### f07.snapshot_lifecycle

- **AC coverage:** AC-9
- **Independent oracle:** Native snapshot hash stable across re-navigation, JS callback state resets; Bun crash leaves unsent typed page text unchanged

- **Individual RED mutation:** Reopen source on navigation or reload WebView on Bun restart.

### f07.frame_document_two_window

- **AC coverage:** 10–11
- **Independent oracle:** Two real WKWebViews, wrong frame/view, stale navigation, canceled navigation and same-document change, view destruction; app handler effect count remains zero for denied cases and #542 stamp remains stable on same-document navigation

- **Individual RED mutation:** Remove checked #542 generation or rotate on `pushState`/hash change (separate runs).

### f07.world_scoped_bind

- **AC coverage:** 10–12
- **Independent oracle:** One real #542 checked generation owns distinct 128-bit App/Trusted credentials and their native handler registrations. Absent stored+supplied token, swapped world tokens, wrong role operation, duplicate bind, stale navigation and page-forged DOM yield **zero actual Host/Bun writes** while valid app call is guarded; trusted
  credential remains invisible to app/page.

- **Individual RED mutation:** Remove **present stored bind** requirement, world/role validation or revocation (separate REDs). 33-receipt fixture is feasibility only. **BLOCKED pending #540/#542 binding approval and product proof.**

### f07.early_void_no_queue

- **AC coverage:** AC-2, 11–13
- **Independent oracle:** Before native app bind or admitted Bun session, an exposed void method returns undefined, causes one private typed refusal, reserves no pending work and cannot replay after later bind/restart. Fresh bound call succeeds. If actual draw.io calls early, C14 remains unqualified.

- **Individual RED mutation:** Buffer early call or rebind it to successor role; independent Bun counter and no-retained-work oracle go RED.

### f07.grants_decoder

- **AC coverage:** AC-12
- **Independent oracle:** Approved #540 v2 canonical decoder, #465 send and #543 host-event implementation, #464 IPC conformance and #541/#552 literal guard; no-grant/wrong-topic zero Bun effects; correct grant permits only bounded credited effect

- **Individual RED mutation:** Treat missing grant as allow, or let numeric v1 invoke count as named v2 send (separate runs).

### f07.native_dom_silence

- **AC coverage:** AC-13
- **Independent oracle:** Under current **AC-17/A1**, prove the approved alternative page→app route without any exposed page native endpoint and reject the old F0 path. **Only if Q-1 A2 is formally approved**, the raw page handler may exist as untrusted request-data ingress: a permitted direct call cannot be distinguished or categorically denied solely by
  wrapper provenance, so it must receive exactly the same private method/current-document/Host checks as the normal wrapper. Unexposed, stale and ungranted calls yield zero forbidden native/Bun effects.

- **Individual RED mutation:** Under A1, accidentally installing a page-native endpoint must fail; under approved A2, treating raw page metadata as app principal/registration, dispatching an unexposed method or omitting final grant makes the independent forbidden-effect observer go RED. Neither arm qualifies without owner choice and integrated Host/native
  proof.

### f07.metric_registry

- **AC coverage:** AC-14
- **Independent oracle:** Unique keld-benches registry ID, diagnostic class and oracle/census/clock/statistic, no threshold

- **Individual RED mutation:** Make ID publishable or add threshold.

### f07.expected_fail_flip

- **AC coverage:** 15–16
- **Independent oracle:** #567 runner proves expected-fail baseline before implementation and owned cells pass only after Mac native evidence

- **Individual RED mutation:** Pass a cell based only on source presence/sentinel.

Real native receipt must include exact KELD/draw.io/Electron hashes, macOS/WebKit and Bun versions, redacted document/view/frame identity, native event ordering, guard/admission counts, mutation RED/GREEN evidence. No macOS test is implied by source inspection. Windows/Linux remain unqualified. On the low-disk shared machine, this draft's applicable
validation is markdown/spec review and lightweight hygiene; complete Cargo fmt, warning-denied clippy, full nextest, docs freshness and hosted matrix remain **implementation PR** gates.

## 9. Perf impact

Unmeasured. Third-world script injection, byte snapshotting and one additional native handler could affect startup CPU/I/O, memory per window and handler cost. Do not infer savings. Candidate metric: `macos.app_preload.handler_callback_ns` in the KEL-90-owned benches registry, monotonic clock, exact accepted/denied census, count/median/p95 and OS/commit
provenance; **diagnostic only**, no target, no performance pass/fail and no public comparison. The **proposed 512 KiB/window and 8 MiB/host bounds** are resource safety limits, not measured performance. A true architecture 01 §5 regression claim requires an attributed end-to-end baseline on physical macOS.

## 10. Open questions, blockers & unknowns

**Status remains DRAFT/BLOCKED.** The source and runtime policies below are
proposed contracts for independent approval, **not** production capability.

### Q-1 — Option A product direction selected; page ingress security decision UNRESOLVED (#484/#467/KEL-142)

the [GH467 first-proof scope](https://github.com/gyldlab/keld/issues/467#issuecomment-6390880361) selects only four predeclared pinned draw.io methods and copied/frozen process facts, **not** general dynamic `contextBridge` parity. This product
scope is **not** an authorized WebKit transport. As independently challenged in the [Q-1 A1/A2 packet](https://github.com/gyldlab/keld/issues/484#issuecomment-6390973710), the existing F0 page-native carrier contradicts **AC-17**. The native/KEL-142 security owners must select **A1** (keep no-page-handler, prove a different real cross-world route) or
formally approve and review **A2** (explicit AC-17/architecture amendment for page-visible **data-only untrusted** ingress, never app/trusted principal). Then GH-484 must prove admitted unmodified preload functions and conditional `.configuration` seed before first inline; GH-467 must prove four current method/callback bindings. Two genuine Host/Bun
requests, per-call release, missing-registry/unexposed/wrong-world/stale/missing-grant mutants remain mandatory. Existing static 3/3, 2/2 omission, F0 16/16, 6/6 and 33-receipt fixtures are **test-only**. General dynamic graph stays deferred and no cell is scored PASS. GH441 separate-world choice stays CLOSED. No production code or approval until all these
named gates pass.
### Q-2 — absolute preload root, proposed mapping, BLOCKED (package boot/#484)

first produce an independently admitted stage-root/manifest authority, then prove the unmodified absolute draw.io path selects one exact relative entry with retained descriptor, no page `file://` root or CWD, and no path replacement or alias escape. A source rewrite is a
failed proof.
### Q-3 — same-UID source integrity, proposed immutable byte image, BLOCKED (package trust owner/#484)

authenticate the expected SHA-256 outside a same-UID-writable source tree, read one descriptor, compare *complete bytes* before JS, retain immutable owned snapshot. Deny **all symlinks** in this first slice. Independently race an in-place writer while
holding the descriptor; absent manifest authority or digest mismatch must prevent any preload effect. `fstat` alone cannot close this gate.
### Q-4 — snapshot budget, selected for draft

512 KiB per preload/window and 8 MiB retained across Host; one snapshot per window, fresh JS on navigation. Pinned upstream draw.io preload blob `44f2f22f159a8a845ff7e573dd10b816383703ec` reports **3,201 bytes** through GitHub's immutable blob API, within the proposed individual cap; the actual staged image,
exact/max+1 and aggregate-accounting RED controls still require verification before approval.
### Q-5 — errors and metric registry, BLOCKED

allocate a new source-admission code distinct from reserved `KELD-WV-012/013`, a #467-owned graph/unsupported code and one diagnostic-only KEL-90 metric ID. Verify exact code uniqueness and typed reason with positive/negative cases before F1.
### Q-6 — early bind, selected fail-closed

#467 specifies **zero queued early calls**, public void-return and internal typed refusal until #542 native bind and a live Bun generation; no automatic replay. Record real startup order. If draw.io's first required request arrives early, application acceptance stays BLOCKED; any alternate buffering design needs
#540/#542 reapproval.
### Q-7 — original KEL-142 architecture amendment, BLOCKED

#484 and the architecture-05/KEL-142 owners must explicitly approve the third-world/second-handler addition without changing historical KEL-142 approval or native trusted facade.
### Q-8 — product/native acceptance, BLOCKED

independent real WKWebView/Host/Bun tests must meet AC-2, AC-7–AC-13 and each one-fault mutation. No test-only fixture, digest-only handoff or mock qualifies product evidence.
### Q-9 — v2 prerequisites, BLOCKED

#540 specification still requires actual #465/#543 transport, #464 conformance and #541/#542/#552 grants/navigation implementation. Until that, F2 world/wrapper feasibility may be studied but named sends stay disabled.
### Q-10 — two-world native custody, proposed, BLOCKED (#540/#542)

two distinct native 128-bit credentials under **one** checked-u32 document generation with actual stored-bind/present-token checks, independent role/handler identity and shared revocation. The 33-receipt fixture supports feasibility only; independent owner approval, real code and hostile
swapped/replayed/missing tokens with zero Host/Bun writes are mandatory.

### B-1 ? first-inline callable publication

- **Status:** open
- **Owner:** GH484, GH467 and KEL-142 native/security owners
- **Impact:** No production page-to-app ingress or app-world method binding may ship; A1/A2 remains unresolved.
- **Check:** Approve one secure ingress decision and prove the unmodified preload's actual four methods and conditional seed before first inline on real WKWebView with independent Host/Bun witnesses.

### B-2 ? independent package-image authority and same-UID source custody

- **Status:** open
- **Owner:** package boot/trust owner and GH484
- **Impact:** No external preload source may execute; adjacent editable manifest hashes and fstat-only checks are insufficient.
- **Check:** Authenticate the expected digest outside the same-UID-writable tree, retain the admitted root, read one bounded descriptor, and pass in-place concurrent-writer and manifest-substitution RED controls.

### B-3 ? canonical v2 transport and Host event delivery

- **Status:** open
- **Owner:** GH540, GH465, GH543 and GH464 owners
- **Impact:** Named app sends/events remain refused; no v1 numeric invoke may be reinterpreted as v2 named send.
- **Check:** Approve and implement the single v2 decoder/send/event path, bounded credits and terminal release, then pass the independent transport conformance cells.

### B-4 ? dual-world native document identity

- **Status:** open
- **Owner:** GH542 and KEL-142 native owner
- **Impact:** No app-world Bind or Host/Bun dispatch; tokenless, stale or swapped credentials remain fail-closed.
- **Check:** Qualify two distinct stored-plus-supplied native world credentials under one checked current-document generation, including stale, replayed, wrong-world and view-destruction RED controls.

### B-5 ? exact renderer grants and shared Host Guard

- **Status:** open
- **Owner:** GH541, GH552 and KEL-102 Guard owners
- **Impact:** No renderer channel receives native authority; missing or wrong literal grants remain denied.
- **Check:** Approve the exact literal grant contract and prove one canonical Guard evaluation before any Bun write, with cross-window, missing-grant and native-capability separation controls.

### B-6 ? product acceptance, conformance and registry decisions

- **Status:** open
- **Owner:** GH464, GH567, KEL-90 benches and KELD error-registry owners
- **Impact:** No shipped draw.io compatibility, native product acceptance, publishable metric or new compatibility error is claimed.
- **Check:** Pass real physical macOS WKWebView/Host/Bun receipts and owned F07 cells; allocate exact error/metric registry entries with diagnostic-only classification and no invented thresholds.

**Approval and non-claims:** Original KEL-142 Done does not grant this new app-world authority. Only the explicitly cited **test-only** native F0 executions exist; this document has no production acceptance, source-integrity oracle, authenticated full-API timing/dispatch proof, approval, renderer implementation, shipped draw.io compatibility or merge
authority.
