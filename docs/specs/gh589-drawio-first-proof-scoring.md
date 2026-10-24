# Spec: draw.io macOS first-proof execution and scoring (GH-589)

Status: draft
Linear: GH-589 (#517 GitHub tracker of record; KEL-237 reference only) · Owner: @0monish / GYLDLAB · Updated: 2026-10-24
Disposition: OPEN / DRAFT / DO NOT MERGE. Do not implement this spec, publish a product score, or declare a draw.io MVP pass before exact-content approval and actual OS proof.
Basis: verified `origin/main` `b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50`; [GH-589 independent validation](https://github.com/gyldlab/keld/issues/589#issuecomment-6381795475). The approved, unmerged GH-587 corpus contract is an external prerequisite, not redefined here.

## 1. Goal & non-goals

Produce the **shortest honest macOS draw.io Desktop first proof**: independently run the pinned actual application using an explicit Legacy KELD profile, observe successful activation and the four real document workflow cells (open, edit, save, close with unsaved-changes decision), and admit each cell through the *existing* KEL-74 evidence system with qualified Host-generation, OS and source provenance. This is a development-staged behavioral first proof and must describe **configuration plus one disclosed behavioral source edit**, as decided in #420, unless a separately qualified unchanged-source route actually passes. The proof is not the finished distribution or a generic Electron parity score.

Non-goals: no claiming packaged installation from `keld dev`; no strict/zero-ambient-authority guarantee under Legacy; no Windows/Linux proof inherited from macOS; no Zettlr/full-feature/median-Electron percentage; no new manifest parser, scorer, role principal, authority map, IPC message, additional ambient grant, native reaper or OS privilege; no editing any other issue owner's active branch. A screenshot, module import, Ready token, app-printed JSON or successful broker call alone is not product acceptance.

## 2. Spec refs

- `docs/architecture/04-electron-compat.md` §§4,6: actual app compatibility and corpus harness; its GH-587 replacement text remains unmerged until #1146 integration.
- `docs/specs/kel74-compat-evidence-schema.md` and `crates/keld-compat/src/evidence.rs`: `keld.compat.evidence/v1` operation kinds, `Scoreboard`, profile labels and no-uncommitted-percentage rule.
- GH-532 and GH-566 approved evidence rules and single `keld.compat.corpus/v1` parser: pinned Electron normative documentation, immutable committed manifest bytes, expected-fail/unknown distinctions and one operation kind per physical manifest.
- **GH-587 approved design**, [draft #1146](https://github.com/gyldlab/keld/pull/1146) current head `4ae2a1223dff34742d79c367b2755dada6a0ecde`, with [human approval of exact design bytes](https://github.com/gyldlab/keld/issues/587#issuecomment-6376877459): C01–C24, Host-origin collection, attempt census, causal G1/G2 and WebView/document-generation binding, bounded cooperative-app trust. Test-only receipt-shape [draft #1147](https://github.com/gyldlab/keld/pull/1147) is NOT live provenance.
- #499 hand-authored migrate artifact; #586/#588 and Linear KEL-78 own explicit verified Legacy state; #541/#552/#542 own exact-literal channel grants and navigation/document identity; #445/#446 Ready, #449 first window, #466/#467 communication, #478 dialog and #564 sync-dialog decision; KEL-141 owns distribution.
- #589 [live dependency tracker](https://github.com/gyldlab/keld/issues/589): all named open prerequisites remain gates for applicable operations. #418 and #420 are **decisions**, not evidence that the chosen transports/app workflow ran successfully. Whole-run requirements include zero `will-attach-webview` on every window and both `webRequest` registrations completing before subsequent `ipcMain` registration.
- Electron API normative oracle **44.4.5** (GitHub commit `694f45852a0f1726cd23bfd379854de489cccb65`). Pin draw.io desktop `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`, whose gitlink `drawio` points to webapp `24b76c2cbd55d88e354042e8d329a2e4708972bc` (both product 31.7.0). **Important distinct fact:** pinned desktop `package-lock.json` resolves Electron **44.5.1**; the 44.4.5 oracle is NOT that lock's installed Electron executable.

## 3. Acceptance criteria (binary, each becomes a test)

| ID | Input / event | Observable / independent negative control |
|---|---|---|
| P01 Source and lock | Prepare pinned two-repo app and declared one-edit patch | Independently check gitlink, both exact full commits, frozen lockfile, patch path/hunks, installed package tree, applied-tree hash; a different internally consistent pair or undeclared extra source edit rejects |
| P02 Runtime and profile | Launch on a real macOS runner using approved manifest | Host/Bun running executable identities, OS/arch/engine, frozen verified policy and **effective** Legacy from Host-origin admitted generation observed; forged Bun `admitted legacy` stderr or an actual Unverified successor under frozen Legacy request cannot count |
| P03 Ready and window | Invoke actual pinned app entry | Authenticated Host Ready before any facade window, app creates first live window and loads its *own* page, no fatal evaluation, both webRequest hooks completed and later ipcMain handlers registered; a blank placeholder/early whenReady/throwing webRequest fails |
| P04 Open | Select a controlled existing `.drawio` file through actual app UI | Active real document identity/content equals the chosen file; successful dialog with unchanged/wrong document fails |
| P05 Edit | Modify a unique shape/label via real app input | Independent before/after document-model or visible artifact changes, dirty state caused by the mutation; fake keystroke or modified console message without model change fails |
| P06 Save | Save edited document to isolated real target | Independent readback bytes/semantic content contain unique edit, and reopen verifies it; success event without changed disk bytes fails |
| P07 Dirty close / decision | Make a second unsaved edit and invoke actual close | App-owned decision UI observed, **macOS** choices Save(0), Cancel(1), Discard(2); Cancel preserves dirty window; Save destroys only after verified successful persistence, failed/cancelled Save retains it; Discard destroys without persisting new dirty edit. Missing/fake prompt, stale `saveAndClose-result` or inverted button map fails |
| P08 Reentrancy and recovery | Repeat close, use two dirty windows, test async boundary / G1→G2 | No double prompt or unintended window destruction; old generation/old document result cannot close or score successor, delayed G1 save cannot become G2 success; stale decision accepted => FAIL |
| P09 Whole-run invariants | Observe all windows and startup registrations | Aggregate `will-attach-webview` count exactly zero across **all** windows and both actual webRequest registrations finish; inject one emission or one throwing registration => run fails |
| P10 Corpus partitions | Build first-proof manifests/records | GH-587's three physical single-kind partitions: `electron-apps-v0-install` (1 install), `electron-apps-v0-activation` (1 activation), `electron-apps-v0-primary-workflow` (4 workflow); one mixed-kind v1 manifest is rejected |
| P11 Receipt and attempts | Submit candidate evidence | Host-current/reader-installed role, WebView/document identity, initiating request/effect completion and independent collector custody match exact source/runtime/manifest and selected predeclared attempt; duplicate/empty receipt cells, forged/stale receipt, missing failed attempt or cherry-picked retry reject |
| P12 Citation and verdict | Record each actual operation | Existing GH-532/GH-566 source/citation grammar and expected verdict agree. Unknown/inconclusive/unrun stays UNKNOWN; actual failed expected-pass test stays RED until its manifest and implementation ticket are reviewed. Fake Electron citation for app-only semantics or unsupported FAIL record rejects |
| P13 Publishability | Add physical corpus identity and emit public report | Exact committed manifest-byte digest, physical denominator, code registry, matching scoreboard doc and attributable **named reviewer on exact head** plus evidence/attempt archive all agree; whitelist-only edit, uncommitted id or mixed-profile percentage refuses |
| P14 Install separation | Attempt to count developer-stage as installed | Install remains UNKNOWN with KEL-141 until an actual prebuilt distribution, OS installation and image identity are verified; substituting `keld dev` must not PASS |

Acceptance is **per physical partition and per real platform/profile**, never a single cross-kind aggregate. P04–P07 are actual app observations, not automatic consequences of each Electron API's conformance test. A kind-compatible scaffold is not execution evidence.

## 4. Design

### 4.1 First-principles atoms, boundaries and reuse

| Atom / owner | Input -> output | Failure mode and falsifier |
|---|---|---|
| Source identity / #499, app harness | Two upstream commits + lockfile + disclosed patch -> verified executable app tree | Wrong gitlink, dependency resolution or unauthorized hunk refuses before launch; different original-lock Electron from oracle must be disclosed |
| Host/profile / KEL-78, #588 | Verified request/policy + actual bound role/current reader -> true effective state, G1/G2-bound observation | Printed child message, stale G1 or false G2 Legacy success rejected; a broker write alone is not the profile witness |
| App operation / #499, #589 native Mac | Real GUI initiating action + active document generation -> independent effect | A fake event, unrelated file, stale page, or no-op write fails |
| Corpus/receipt / #587/#500 | Exact committed manifest bytes and trusted operation observations -> one record per covered cell | Wrong digest, kind, runtime image, profile label or receipt cardinality rejects |
| Reporting / #589, KEL-74 owner | Qualified recorded attempts + reviewer-approved selected run -> per-partition scores and bounded claim | Discarding an earlier fail or publishing unsupported cross-kind percent rejects |
| Packaging / KEL-141 | Prebuilt distributable + OS install identity -> install cell | Staged dev run cannot satisfy install |

No boundary change in this **specification**. Reuse KEL-74 scorer, GH-532/GH-566 manifest parser/digests, GH-587 receipt shape and existing Host identity owner. Do not add another owner, principal, parser, cross-process event, or generalized app automation framework just for draw.io.

### 4.2 Fixtures and independent observers

Commit deterministic fixtures with a stable `fixture_id`, byte hash, shape/label model, expected semantic content and a **per-attempt unique** mutation marker. At minimum: (a) small valid draw.io XML, (b) distinct valid wrong-document control, (c) larger valid XML whose **encoded wire size is independently measured**, not assumed from file size. Place the actual immutable source/fixture under the #500 implementation's reviewed corpus/fixture owner, not under this spec branch or transient `/tmp` alone.

Use isolated OS temp directories and outside sentinel. Control file chooser, document identity and save destination with independent OS readback; observe app-owned dirty-close UI and actual window lifetime on real macOS. Where native GUI capture is required, record permitted observer/TCC state; missing capture authority is UNKNOWN, never a pretend PASS. Reopen saved target to verify persistence; verify original saved bytes are unchanged after Discard and Cancel. Real application source indicates Mac button order Save/Cancel/Discard and a synchronous original dialog; a proposed `await dialog.showMessageBox` patch must run on the actual Mac and cover repeated-close races, cancelled saves, source hunk census and cleanup. If the accepted one-edit scope proves insufficient, **stop and seek revised reviewed scope**; do not conceal additional hunks.

### 4.3 Evidence artifact and attempt publication

Before first native attempt freeze: Keld/Host source commit, actual Host executable SHA, Bun version and actual live Bun image SHA, actual WebKit/OS/arch, Electron normative oracle 44.4.5, both draw.io full source commits, original lockfile digest, installed post-migration dependency tree, selected profile/permission digest, declared patch digest and applied tree, exact committed manifest bytes/digest, run attempt ID and selected-run rule. The manifest digest is NOT any executable/source/policy digest.

Each attempt retains initiation and completion order, Host generation and current/reader installation witness, Host-owned window/WebView/navigation/document identity, OS file checksums and UI/window observers, zero-webview census, both webRequest registrations, actual dialog/cancel/save/discard results, terminal status, cleanup and any failed/unknown attempt. Preserve a content-addressed immutable CI/controlled archive with exact hashes and publicly reviewable evidence locators allowed by GH-532; never use mutable self-authored URLs or a Bun log as source-of-authority. Trust scope: cooperating-app bounded behavioral proof; malicious same-user Legacy adversarial authenticity stays UNKNOWN without independently qualified stronger custody.

Predeclare **one deterministic selected run per partition/source/OS/profile** (first qualifying attempt, where qualification is provenance/fixture/environment, NOT a requirement to pass). Keep all failures and diagnostic reruns. Do not rescue a failed selected run by publishing only a later success, or recompute the selected population after seeing results. Source-only/differential Electron observations are **oracle evidence**, not KELD product evidence.

### 4.4 Specific close-with-unsaved path

The pinned desktop source uses `mainWindow.on('close')`, `isModified` / `isModified-result`, validates sender and correlated unique IDs, then `dialog.showMessageBoxSync` when dirty. The native macOS options are Save(0), Cancel(1), Discard(2). Save posts `saveAndClose` and waits for `saveAndClose-result.success` before destroying. Cancel resets state and retains the window; Discard destroys without the extra dirty write. First proof follows #420 **configuration + one disclosed async-dialog behavioral edit**; retain and name the #564/F06-T7 native sync-dialog outcome, never substitute #528 transport success for a native dialog acceptance. Negative controls target early close, repeated close while pending, stale unique IDs, second dirty window, failed/cancelled Save, and post-G2 old callbacks.

### 4.5 Dependency and critical-path strategy

Execution lane: #445 conformance -> #446 actual authenticated Ready with zero initial facade window -> #449 actual constructor/window -> required #452/#453/#455/#459/#462/#466/#467/#473/#478/#480/#481/#484/#485/#491/#502/#545/#553/#565/#570 and #499 migrated app for actual operations. Separate admission lane: #586/#588 actual explicit profile G1/G2 Host witness + #541/#552/#542 active document/channel owner -> #587 receipt consumer -> #500 physical corpus manifests -> #589 scoring. #528 supports #564's unmodified sync path but neither alone proves draw.io; the approved patch path can be independently exercised while that work proceeds. Do not infer all dependencies passed merely because issue/PR source exists.

**Priority:** #446 is highest structural product-execution unlock; #588 Host-origin generation/state evidence is highest independent scoring unlock. Work can proceed in parallel under disjoint existing issue owners. Mac integration waits for both real execution and trustworthy receipts. KEL-141 install work can remain separate without delaying a precisely labelled dev-staged MVP observation.

## 5. Boundaries

This GH-589 *spec* branch may edit only `docs/specs/gh589-drawio-first-proof-scoring.md` and generator-owned `llms.txt` / `llms-full.txt` when actually selected. No GH-587/#588 or #446 runtime files, evidence parser/scorer, application source, distribution scripts, dependency pins, CI routing, GitHub labels/checklists, `docs/research`, guard permission grammar or installed executable are modified.

After exact-content approval, **separate claimed worktree(s)** may write GH-589 physical-run/test/CI receipts and the eventual code/docs/manifest registration *only* as authorized by the GH-587 and GH-500 spec/owner gates, with named independent reviewers. Actual source patches under #499/app fixture owner must remain separate from the scoring authority. The live Host receipt producer belongs to #588, GUI document generation to #541/#552/#542, and runtime/Ready construction to #446/#449.

## 6. Tasks (ordered, vertical slices)

- [x] T0 — Reconcile current primary source, KEL-74 infrastructure, #587 approved design and tracker/status; publish independently verified initial #589/#499/#500/#391 findings. No runtime tests.
- [ ] T1 — Publish **this specification** as an unmerged DRAFT PR, complete relevant docs/tests and independent exact-head source review; obtain explicit exact-content design approval.
- [ ] T2 — Reuse approved #499 migration artifact and qualified native Host/profile/document observers; independently validate pinned source, patch, lockfile and real runtime identities with fail-first negative controls. Do not bypass dependency owners.
- [ ] T3 — Qualify real macOS app startup, open/edit/save/dirty-close Save/Cancel/Discard with independent OS and UI effects, including zero-webview count, webRequest completion, repeated close, two windows and causal G1/G2.
- [ ] T4 — With GH-587/#500 and KEL-74 owners, admit truthful per-partition records, exact manifests, all attempts, named review and code/docs registration. Only then publish a narrowly scoped claim for actually passing cells; UNKNOWN install is not a packaged-product pass.
- [ ] T5 — KEL-141 real installed distribution and other OS/strict profile proofs stay separately gated; do not relabel T3/T4 dev staging as those outputs.

## 7. Test plan

P01: independent source/gitlink/lock/hunk check; wrong internally consistent pair and second hunk RED.
P02: native Host generation/effective-state/reader witness; forged stdout, stale G1 and true unauthorized Unverified G2 under Legacy RED.
P03: actual Mac facade Ready/page/window observer; placeholder page, early Ready, throwing either webRequest RED.
P04: controlled chooser file A vs distinct file B; unchanged active document RED.
P05: real unique shape mutation; suppress model change while emitting input RED.
P06: save and re-read/reopen chosen destination; emit success but suppress write RED.
P07: dirty edit then Cancel/Save/Discard; swap button indices or accept false save-success RED.
P08: two windows/repeated close, delayed G1 action after G2, stale window/doc generation; false successor closure or G2 attribution RED.
P09: all-window `will-attach-webview` event census and two webRequest hooks; inject one emission or one thrown registration RED.
P10: existing v1 schema/denominator tests; mixed-kind physical manifest RED.
P11: GH-587 trusted receipt reader once producer exists; empty/duplicate/self-authored/stale receipt, hidden earlier failed attempt RED.
P12: pinned Electron 44.4.5 snapshot/citation and app-source-oracle review; forged citation, uncited app-only PASS or wrong expected FAIL RED.
P13: exact git code registry, scoreboard document, manifest bytes, evidence and reviewer consistency test; whitelist-only/docs-only or missing review RED.
P14: OS installation/digest oracle, pending KEL-141; substitute dev-stage RED/UNKNOWN.

Tests must await observable events rather than sleep to imply readiness, complete cleanup under existing owner deadlines, and preserve failed attempts. Product operations remain **UNKNOWN** until real KELD execution and trusted receipts. CI/document tests are not native product tests. A genuine expected-pass failure leaves the corresponding test RED; GH-532/GH-566 rules govern when a cited FAIL record may be admitted.

## 8. Review gates triggered

- Public API: proposed product publication/claim shape consumes existing KEL-74 only; independent KEL-74/GH-532/GH-566 and #587 source review required for the later implementation. This **spec PR changes no public API**.
- Permission model: explicit Legacy and Host-origin generation/custody assumptions need independent security review before product acceptance. This **spec PR grants no authority**.
- Unsafe: none in spec; any later native unsafe must undergo its own owner gate.
- Dependency addition: none.
- Wire protocol: none; no new receipt IPC frame.
- Exact content: human approval of this spec before T2/T3/T4 implementing work; named independent review on corresponding real Mac and code/docs product registration; do-not-merge retained.

## 9. Perf impact

None for this documentation-only PR. Do not infer performance benefit from Rust/Bun architecture or a staged run. Any future runtime collector overhead is measured separately for cold start and the complete workflow using actual comparable KELD/Electron artifacts, with the existing performance budget and independent proof.

## 10. Open questions

**Blocking for implementation, owned externally (not unresolved design choices for this draft):** #446 genuine Ready+first-window integration; #588 typed trusted G2 effective-state/current/reader observation; #541/#552/#542 GUI document-generation evidence; GH-532/GH-566 approved app-specific source-oracle citation semantics when Electron documentation is not normative for Open/Edit/Save; #499 final literal permission/patch/lock artifact; #500 admitted manifests; #564 actual sync-dialog outcome and adequacy of the accepted single-edit async fallback; KEL-141 real distribution if installation is later claimed. A failure of any gate keeps its product cell UNKNOWN/BLOCKED as appropriate, without promoting an unexecuted result.

**Design approval needed now:** approve or request exact-content corrections to the above GH-589 *scoring specification*. No new decision on the already approved #587 profile/corpus design, #420 config-plus-one-edit path, or #586 authority mapping is requested.
