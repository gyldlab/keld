# Spec: Original Zettlr macOS product workflow evidence
Status: draft
Linear: GH-1354 (#517) · Owner: @0monish (exact-content approval pending) · Updated: 2026-11-06
Source baseline: 0eb3ce203187f35c5276a4237189c906a5220d5a
Publication: future implementation specification; draft and do-not-merge.

## 1. Goal & non-goals

Define falsifiable evidence for the original Zettlr v4.9.0 application on real macOS under explicit Legacy. Observe the
actual application becoming usable, opening a document, delivering edits, saving exact expected bytes and honoring dirty
close/quit decisions. A mounted editor, successful import or helper-only fixture is insufficient.

Non-goals: implement any facade; change app business logic to hide unsupported calls; enable synchronous XHR; prove Strict,
Windows, Linux, installation, citation rendering or updater behavior; change a corpus parser, profile label or score rule.
GH1359/GH1360 own the other OS lanes. Existing parked decisions remain parked.

## 2. Spec refs

- [GH1354 scope and source-derived workload](https://github.com/gyldlab/keld/issues/1354).
- [GH1358 D-Z20/D-Z21 reply-mirror decisions](https://github.com/gyldlab/keld/issues/1358#issuecomment-6556349232).
- [GH532 first-proof rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh532-first-proof-evidence-rules.md).
- [GH566 manifest owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh566-corpus-manifest-owner.md).
- [GH587 product evidence and its exact approval](https://github.com/gyldlab/keld/blob/16e15d42f833ca2942e4312b80987c6e541c6c71/docs/specs/gh587-electron-apps-v0-corpus-cells.md); consume its approved design and subsequent recorded errata, never infer implementation from approval.
- [GH589 first-product scoring predecessor](https://github.com/gyldlab/keld/blob/bfb2124957916bfa0360cb4690ba6f15d996d339/docs/specs/gh589-drawio-first-proof-scoring.md).
- [Architecture 04, evidence and migration](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md).
- [Zettlr demand source](https://github.com/Zettlr/Zettlr/tree/04128af42b58f7fea3f39f7b1efd597f5021f48a), tag v4.9.0.
- [Pinned Electron lifecycle contract](https://github.com/electron/electron/blob/v44.4.5/docs/api/app.md).
- [Pinned Electron window close/readiness contract](https://github.com/electron/electron/blob/v44.4.5/docs/api/browser-window.md).

Direct source review on 2026-11-06 confirms the source paths in section 4.2, asynchronous dirty dialogs, application quit
re-entry, the separate storage-clearing close listener, menu-directed save and the hidden main window's ready-to-show
listener. These are source facts, not executed Keld observations. Context7 /electron/electron/v44.4.5 was discovery for
lifecycle semantics; the pinned official pages above were independently retrieved. Complete original-app execution is unrun.

GH1354 selects Electron 44.4.5 as contract oracle and separately records the app-lock runtime 44.9.1. The current v1 owner
admits only its one closed upstream object. The GH532/GH566 app-lock amendment is a prerequisite, not an invented field in
this consumer. Source-only app behavior remains unknown under the existing citation grammar until its owner admits it.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1 — Source and admission

Given the independently selected original Zettlr source closure, when the product observer starts a run, it verifies app
commit, lock/dependency closure, generated bundle identity, permitted integration changes and actual Host/Bun/engine
identities. A substituted app with internally consistent self-reported hashes is refused. An undeclared behavioral source
edit fails admission; a configuration alias is recorded separately from an app edit.

### AC2 — Physical kinds and truthful gaps

Given proposed Zettlr evidence, when the existing owner parses it, each physical v1 manifest has one kind, its own denominator
and exact covered records. Activation, primary workflow and installation are separate. Installation remains unknown without
an actual distribution. Mixing kinds, using a logical family id as a physical manifest or scoring uncited source-only cells
pass fails. A blank/partial partition produces no overall product percentage.

### AC3 — Activation and launch are distinct

Given a fresh isolated profile with the original onboarding path, when the real app starts, activation observes the admitted
role and original first document. In the main window's first inline page script, observe all five original globals:
`window.ipc`, `window.config`, `window.process`, `window.getPathForFile` and `window.getCitationCallback`, before later renderer
execution can supply them. Launch additionally witnesses completed original provider boot, the usable main editor and
synchronous config-store construction from the seeded mirror. Preserve both onboarding and returning-profile runs.

Negative control: substitute a blank page, skip appearance or protocol provider boot, remove the config mirror, or omit/delay
any one of the five globals until after the first inline script. A visible window alone cannot satisfy launch; an
expected-pass mapped case turns red without silently rewriting expected status. Presence of the citation callback does not
make the separately unsupported synchronous citation workflow pass.

### AC4 — Open and overlapping calls

Given a controlled document whose actual UTF-8 text exceeds 4,096 bytes, when the user opens it through the real chooser, the
application loads its exact content. Capture the original overlapping startup reads with separate request identities and
results; one fulfilled Promise cannot stand in for both. A chooser success without model load, truncated text or crossed
responses fails the observable. Report tested byte lengths and the accepted payload profile, not an unlimited-size claim.

### AC5 — Sustained edits

Given the active loaded document, when the observer makes distinguishable edits, at least 20 consecutive actual
push-updates invokes from that window complete with correct versions and content. Witness update delivery and the native
document-edited state. Count transmitted application updates, not keystrokes that the app may batch. Dropping one update,
reusing an old success or accepting a stale document generation fails.

### AC6 — Real menu save and bytes

Given modified content with controlled BOM and line-ending variants, when Cmd+S reaches the original application menu, its
shortcut reaches the focused document and the real filesystem target is updated. Independent byte readback equals the app's
source-defined encoding, including the unique edit. A fake FILE_SAVED event, wrong focused window or unchanged disk bytes
fails. A second window/file with a distinct marker stays unchanged.

### AC7 — Three dirty-close outcomes

Given a new dirty edit after a successful save, when native Close requests the application's asynchronous dialog, the
original window remains until a decision. In independent reset runs: Yes saves and closes; No closes with the last saved
bytes unchanged; Cancel keeps the dirty window and unchanged saved file. Observe the actual prompt and final disk/window
state. Immediate close, swapped button mapping or a test-owned imitation prompt fails.

### AC8 — Close listeners and quit re-entry

Given the original two close listeners and Cmd+Q handler chain, when close/quit runs, storage clearing neither throws a
synchronous veto nor replaces the dirty decision. Observe the original before-quit cancellation, async decision, subsequent
app.quit, will-quit shutdown and final re-entry. A rejected clearing Promise is handled by the original catch and logged;
it does not become another close veto. Each expected ordering edge comes from the pinned app plus its oracle receipt.

Negative control: remove the dirty listener, turn clearing into a synchronous throw, or suppress the post-shutdown quit
re-entry. Do not transplant draw.io's two-window trace or infer Dock/logout/update behavior from this Cmd+Q run.

### AC9 — Explicit compatibility and authority

Given the reviewed configuration and grant census, a run declares Legacy explicitly and uses the owner's effective profile
receipt. The 32 main-side literals and the orphan renderer-only message refusal follow the pinned GH1358 decision; regenerate
and compare that census before execution. The sandbox:false windows remain declared divergences, renderer process.env stays
own-property-free, and will-attach-webview count is zero. A profile inferred from sandbox:false or a granted orphan fails.

### AC10 — Mirror and unsupported citation call

Given the six original sendSync sites, config-provider reads use the approved app-declared mirror; writes and push updates
follow GH1375's ordered optimistic divergence. The computed get-citation-sync call remains an explicit unsupported feature
with its fix-it, outside the citation-free workflow document. Missing seed, reordered config updates or enabling a hidden
synchronous carrier fails. No normative error code is invented by this spec.

### AC11 — Causal evidence and honest publication

Given an asynchronous action initiated in one role/window/document generation, only that action's independently observed
result is attributed to it. A delayed G1 file write after G2 admission is not G2 success. App-authored JSON, console strings,
old UI events and screenshots alone cannot supply Host identity or effective profile. Preserve the complete attempt census,
including failures. Removing a failed attempt or swapping source/artifact/generation fails the GH587 consumer controls.

### AC12 — Registration is an independent gate

Given valid admitted records, public claim eligibility additionally requires the existing owner's committed physical ids,
matching code and scoreboard entries and exact-revision named reviewer evidence in the same change. A whitelist-only or
docs-only edit fails. No source review, draft approval, green docs CI or source-only observed match substitutes for this gate.

## 4. Design

### 4.1 Reuse, ownership and trust

No runtime boundary changes in this publication. GH566 owns manifests/targets/citations; GH587 owns product receipts and
bounded collector provenance; KEL-74 owns scoring; KEL-78/#588 owns effective profile; GH541/GH552/GH542 own native document
and channel identity; the existing Host owns windows, privileged handles and recovery. GH1354 only selects the app workload
and supplies its consumer cases. No second parser, scorer, token, daemon, runtime receipt protocol or sandbox is introduced.

The collector uses GH587's cooperating-original-app trust model. Immutable retained observations support a bounded behavior
claim. Same-user Legacy access means file modes and locally accessible hashes/signing material do not establish hostile-app
unforgeability; that stronger claim remains unknown. A caller timeout does not prove a file write was cancelled or rolled
back. After ambiguity, inspect the effect and rerun using new unique data, never silently retry a non-idempotent action.

### 4.2 Original application paths

All app paths below are relative to the selected Zettlr source commit in section 2.

| Stage | Existing source and observable |
|---|---|
| Startup | source/main.ts; source/app/lifecycle.ts; source/app/app-service-container.ts: lock, ready, providers, environment probes and original window |
| Preload/config | source/common/modules/preload/index.ts; config-provider consumers: original five exposed globals and seeded config-store reads |
| Main window | source/app/service-providers/windows/create-main-window.ts: hidden creation, ready-to-show listener and storage-clearing close listener |
| Open/read | source/app/service-providers/documents/index.ts and fsal/fsal-file.ts: registered handler, actual selected document and whole-text result |
| Edits | source/common/modules/markdown-editor/plugins/remote-doc.ts; documents/index.ts; windows/index.ts: versioned push updates, broadcast and edited state |
| Save | source/app/service-providers/menu/menu.darwin.ts; documents/index.ts; fsal/index.ts: focused shortcut, real write, BOM/line-ending conversion and completion |
| Close/quit | windows/index.ts; windows/dialog/ask-save-changes.ts; documents/index.ts; source/main.ts: async decisions, re-entry and shutdown |

The paths shorthand documents/index.ts, fsal/* and windows/* in this table means source/app/service-providers/<path>.
Original provider startup includes safe-file protocol registration, appearance subscription and environmentCheck's external
tool probes. Classify missing optional tools by the pinned app's behavior; do not disable provider boot to obtain a pass.
Default autosave is off and leaveAppRunning is false; record those effective inputs and isolate the observer's test profile
from the user's data. Use real callbacks/effects as barriers; timeout only bounds a failed run.

### 4.3 Corpus partition proposal

Proposed logical family zettlr-v1 is a report grouping. Exact physical identifiers must be frozen by GH566 before admission:
activation partition (kind activation), primary-workflow partition (kind primary_workflow) and a separate installation
partition (kind install, unknown until qualified). The primary workflow has launch, open, edit, save and close-unsaved
observables; activation only establishes the narrower first original document/live role milestone. Neither duplicates the
other's evidence requirement. No full-feature denominator is invented.

The app-lock metadata amendment, any app-source-oracle citation amendment, admitted app-source closure and physical ids
are separate GH532/GH566 decisions. While missing, source-backed fixtures and this draft can be reviewed, but product
registration stays blocked and uncited cells stay unknown. Do not relax closed upstream fields or invent a second oracle pin.

### 4.4 Runtime seam and migration unit

Before and after: the original app main/preload/renderer execute on their declared Keld roles/worlds; existing facade owners
supply the behavior. No runtime migration is implemented here. Freeze config/profile/app source before launch and capture
actual role/window/document generations at admission and at each action. The unit includes original handlers, callbacks,
preload globals, emitted events, actual files and corresponding evidence consumers, not just imports.

Consume the reviewed GH547 invoke path, GH543 addressed events, GH545 measured payloads, GH1375 mirror, GH549 local returns,
GH550 file identity, GH451 quit, GH474 clearing, GH556 menu and GH1355/GH1356 provider qualification. Compatibility facade
remains permanent; no temporary wrapper that fabricates success is allowed. Development and installed-app proofs stay separate.

Capabilities, manifest keys, wire and protocol changes: none in this specification. The underlying owners approve any
required implementation change independently. Missing capability is a typed refusal, never a broader permission fallback.

## 5. Boundaries

Publish only this specification and associated issue/PR evidence. Future consumer work stays under the GH566-owned fixture,
registry and test paths and the GH587 collector seam after approval. Original-app source integration uses the approved
configuration/patch census. Product-id and scoreboard updates require the designated single writer and exact-content review.

Do not edit architecture, policy, corpus schemas, shared registries, baselines, app business logic, source pins, packaging,
active native worktrees or implementation owners' paths in this draft publication.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Freeze the GH532/GH566 prerequisite amendments, physical ids, source/lock closure, effective config and approved consumer cases; retain unresolved blockers explicitly.
- [ ] T2 Add the admitted consumer fixtures and original source census using existing parser/collector owners; prove substituted-app, mixed-kind and fake-receipt controls fail.
- [ ] T3 Execute original-app activation/launch and open/edit/save on real macOS with both profiles of app data (fresh onboarding and returning), distinct inputs and complete attempt evidence.
- [ ] T4 Execute Close Yes/No/Cancel and Cmd+Q with reset dirty documents, real UI/disk observations, stale-generation and fake-completion controls; retain unsupported feature cells.
- [ ] T5 Independently review exact source/receipts and native evidence, then register only qualified physical product partitions under the existing code/docs gate; keep installation and other OS/Strict gaps explicit.

## 7. Test plan

AC1/2: existing manifest, source-closure and product-receipt admission tests with byte substitution, undeclared patch, wrong
physical kind and uncited-result controls. AC3/4: real original application through provider startup, first-inline-script
observation of all five globals with separate omit/delay controls, two request identities, chooser/model readback and large
unique text. AC5/6: at least 20 real invokes plus native state and independent file bytes;
compare a second untouched window. AC7/8: separately reset Yes/No/Cancel runs and event-ordered Cmd+Q trace. AC9/10: exact
grant/config/renderer fact census and missing-seed/update-order controls. AC11/12: collector provenance, retained failed
attempts, generation mismatch and registration-gate negatives.

A test expected to pass becomes red when the real operation fails; changing expected status to hide that failure is not a
fix. Source-only cells remain unknown under current GH532. Future native results must record OS/engine/runtime/source,
collector identity and actual observed actions. No native or product test ran during this documentation publication.
Document checks cover ten sections, limits, cited-source presence, independent review and exact GitHub readback. Local
product fmt/Clippy/workspace tests, hooks and machine closeout are unrun in this GitHub documentation lane.

## 8. Review gates triggered

Public API and permission-model review of consuming evidence/profile meaning; no runtime grant or new wire API is approved.
GH532/GH566 amendments and single-writer product registration retain their own gates. Unsafe/dependency/wire gates apply to
any later implementation diff that actually triggers them. This draft is not exact-content approval.

## 9. Perf impact

No measured claim. Large-document and 20-update tests are correctness/workload requirements. Record sizes and completion
observations; do not label them throughput benchmarks or derive a latency budget from this draft.

## 10. Open questions

1. GH532/GH566 must approve and land the app-lock metadata amendment and decide admission of any app-source-only oracle; current grammar is unchanged.
2. GH566/GH1354 must freeze physical ids and the activation-versus-launch split before committing a denominator; no cross-kind aggregate score is authorized.
3. GH587/#588 and document-identity owners must supply qualified collector/profile/generation evidence; absent qualification stays unknown.
4. GH589's predecessor and named native feature dependencies remain open until their actual artifacts qualify. A diagram or source inspection cannot satisfy them.
5. The repository owner must designate the product-id/scoreboard writer and approve the exact implementation spec. This file remains draft; no runtime, release or merge authorization follows.
