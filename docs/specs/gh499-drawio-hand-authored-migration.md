# Spec: pinned draw.io Desktop hand-authored first-proof migration (GH-499)

Status: draft
Linear: GH-499 (#517 GitHub tracker of record; KEL-78 reference) · Owner: @0monish / GYLDLAB · Updated: 2026-10-24
Disposition: **OPEN / DRAFT / DO NOT MERGE**. This contract is a proposed, unexecuted migration artifact; no Mac draw.io product operation or packaged installation is proven.
Source basis: `origin/main c5c3302189120a6753395a2533a3b7f50bb7b928`, [GH #420 PANEL-P3 source experiment](https://github.com/gyldlab/keld/issues/420#issuecomment-6034980014), [GH #499 independent source reconciliation](https://github.com/gyldlab/keld/issues/499#issuecomment-6381951308).

## 1. Goal & non-goals

Produce an auditable, **hand-authored** description of exactly what KELD's future `keld migrate --write` must output for **draw.io Desktop 31.7.0** on macOS with **affirmatively requested Legacy**, and later provide a real reviewable migration output under the approved #497 key-to-owner contract. The first credible configuration path uses **one disclosed behavioral source patch** to replace the dirty-close synchronous dialog with an awaited asynchronous one, unless native macOS proof later permits the unchanged-source synchronous route. Every output must be identifiable, reversible and independently validated. This spec authorizes **no runtime implementation** before its exact-content human approval.

Non-goals: no generalized `keld migrate` generator, full app compatibility, automatic source rewriting, installer, code signing or notarization, strict OS containment under Legacy, self-generated `el:*` permissions, implicit profile downgrade, extra bundler/compat configuration, second module resolver, new JS/TS formatter, KEL-74 product percentage, or app log taken as a Host-origin receipt. Actual application launch/Open/Edit/Save/dirty close belongs to #589, not the static artifact.

## 2. Spec refs and immutable source closure

| Item | Exact source / owner | Required verification |
|---|---|---|
| Desktop source | [jgraph/drawio-desktop @ `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`](https://github.com/jgraph/drawio-desktop/tree/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e), package version `31.7.0` | Resolve full git commit and source tree, not just the project name |
| Webapp gitlink | Desktop git tree `drawio` is mode `160000`, commit `24b76c2cbd55d88e354042e8d329a2e4708972bc` | Populate/verify that **exact** webapp tree; missing/wrong gitlink refuses |
| Original package manifest | `package.json`: Git blob `95a7e931fc43abc9b42dd3ef0007a1b9c1296c7f`; raw SHA-256 `c3612ea5534ab3a2be739b2ae40752d13cf497ee401af84e72a0c8c18ac7d077` | Preserve original bytes separately from modified output |
| Original dependency lock | `package-lock.json`: Git blob `aab2e8562ea5fd1b46e6a9cbc2a432feb78de550`; raw SHA-256 `b99f2d8b1a166f84495252b4d0cb82da02ec4aa57aef005e50f6ff81cf669f26` | Original locked `node_modules/electron` is **44.5.1**, not KELD's normative oracle 44.4.5; installed post-migration tree must be measured separately |
| Main source | `src/main/electron.js`: blob `b57b5e2047710e829bfc25ab09f32b09ad2114d7`; raw SHA-256 `55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d` | Retain original, reviewed edit and applied-source SHA |
| Preload | `src/main/electron-preload.js`: blob `44f2f22f159a8a845ff7e573dd10b816383703ec`; raw SHA-256 `1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98` | Keep unchanged unless separately amended and reviewed |
| Normative Electron API oracle | v44.4.5 tag peeled to `694f45852a0f1726cd23bfd379854de489cccb65` (GH-532/GH-566) | **Not** the original installed 44.5.1 binary. Never label a normal locked `npm ci` runtime as 44.4.5 without measurement |
| PANEL-P3 research | `research/electron-compat-map@23f50274`, `wayfinder/electron-compat/probes/panel-p3/p3_decision.md` and trace fixtures | **Bun-stub/source trace**, not KELD native app success or an immutable Host-run receipt |

Governing KELD contracts: [#497 migrate output/key-to-owner specification](https://github.com/gyldlab/keld/issues/497), [#586/KEL-78 explicit-profile specification](https://github.com/gyldlab/keld/issues/586) (approved root `/profile`, enforced by #588), [#541 exact per-window channel grants](https://github.com/gyldlab/keld/issues/541), #452 role session facts and allowed environment, #453 single-instance declaration, #491 updater adapter, #446/#449 first-window architecture, #467/#484 preload and IPC, #478 dialog broker and #564 synchronous alternative. #587/#500 corpus and [#589 approved scoring specification](https://github.com/gyldlab/keld/pull/1157) own evidence and any product outcome. Read current exact owner schemas before materializing keys; **spec proposals are not permission to guess or invent a field in an unmerged parser**.

## 3. Acceptance criteria (binary; each needs an independent negative control)

| ID | Operation | Required observable / falsifier |
|---|---|---|
| A01 Source closure | Original desktop commit, webapp gitlink, package/lock/main/preload hashes and actual resolved dependency tree match the above immutable inputs; substitute a *self-consistent but different* pair or one-byte-edited source -> refuse |
| A02 Committed patch identity | Every source edit is exactly the approved, reviewed macOS-scoped patch in section 4.4 with recorded original/patch/applied bytes and **macOS-only artifact admission**; add a second behavior hunk, claim config-only after applying this patch, or run the same unguarded edited source as a Windows/Linux-qualified artifact -> refuse |
| A03 Sole profile authoring source | Only decoded root `/profile: "legacy"` of `keld.permissions.jsonc` is accepted, based on the retained verified permissions bytes. A duplicate, `null`, a new CLI/env/profile hint, or moving it into `keld.config.ts` refuses; **explicit user consent to weaker Legacy guarantees** is required in addition to write consent |
| A04 Source-registered channel grants | Every proposed per-window `el:<channel>` literal is an **active** trusted main-side `ipcMain.on/once/handle` registration (not text inside a comment) at the pinned complete source, appears in PANEL-P3's **synthetic** operation-demand trace, and is separately bound to a **real, Host-observed window and document generation** before native acceptance under #541/#589. A research-script trace or commented-out `ipcMain` call cannot mint window authority. Unregistered, renderer/preload-nominated, global-unioned or wildcard channels refuse before reaching Bun |
| A05 Denied library/dev channels | Never seed `el:__ELECTRON_LOG__`, `el:electron-store-get-data`, `el:openDevTools`, `el:checkForUpdates` for the scoped first proof. Adding any, prefix matching, or permitting `el:*` refuses even if a library registered it |
| A06 Minimum first-proof messages | Reconcile `el:rendererReq`, `el:app-load-finished`, and `el:isModified-result` against pinned source and synthetic PANEL-P3 demand, then require an independent **real Mac** Host-owned window/document binding before treating the candidate as admitted. Their appearance in a research table alone **does not automatically grant** them; missing native binding blocks acceptance |
| A07 System permissions | Use only source-defined exact `app.system` dialog/menu literals approved by #404/#478 and parser; no invented `app.system` key, wildcard, filesystem grant, `shell.openExternal` grant, host-less glob or `$HOME/**` expansion; one forbidden scope mutation refuses |
| A08 Declared role environment | Approved #452/KEL-75 host-owned declaration carries exactly `DRAWIO_DISABLE_UPDATE=true`; no reserved loader/authority env name including `NODE_OPTIONS` or `KELD_*`. Process environment confers no capability; adding a loader variable or changing `true` to `false` refuses |
| A09 Facade entry / single instance | `keld.config.ts` includes only the actual approved `name`/relative `entry: src/main/electron.js`, and #453's reviewed single-instance declaration in its **implemented** grammar; rejecting a guessed field or absolute machine path is required |
| A10 Package resolver | In the reviewed package output, one `electron` specifier resolves to **@keld/electron** through the one #502 approved package-manager resolver and `electron-updater` resolves to #491's inert adapter; no second alias/source shim. Nested original `electron` packages and unresolved default/CJS imports fail installed-tree admission |
| A11 Scripts and forbidden outputs | `start: electron .` and every package script using the Electron binary or electron-builder are rewritten or removed; no `keld.build.ts`, `keld.compat.ts`, Bun alias, `tsconfig paths.electron` or hidden per-machine path written. Reinserting one invokes refusal |
| A12 Static KELD grant parser | Emitted `keld.permissions.jsonc` passes the **actual** `keld-guard` manifest reader (64 KiB cap/unique decoded keys); a duplicated JSONC key or wildcard must fail before any Host resource |
| A13 App preflight facts | Preserve required synchronous session facts (getVersion, getPath, isPackaged, process shape), native display/window mirrors and `file://` code URL identity without a Promise, fake page, or source patch. Missing first-window/Ready/URL guarantees keep #589 activation **UNKNOWN** |
| A14 Honest runtime report | Every output path/key and before→after digest, dropped input and owner/disposition is recorded with ✔/▲/✘/unclassified as a **static estimate, not a compatibility score**. An uncommitted KEL-74 product ID, install staging, or fabricated percentage must not produce a claim |
| A15 No auto-install/side effects | The approved Bun role launcher prevents runtime dependency auto-install; no unintended network request, install script or updater check under `DRAWIO_DISABLE_UPDATE=true`. Removing the declaration must trigger an observer failure, not a silent green result |
| A16 Dirty-close semantics | On a real Mac, the app-owned Save(0)/Cancel(1)/Discard(2) prompt appears only with a new dirty edit; Cancel retains window and bytes, Save destroys only after successful actual persisted bytes, failed/cancelled Save retains it, Discard destroys without persisting new dirty bytes. Repeated close, two windows, stale replies and blocked dialog must independently fail if incorrect |
| A17 Honest installation label | `keld dev`/staging is **never installed application proof**. No KEL-141 prebuilt distribution/OS-installed image -> the install cell remains UNKNOWN; substituting a staged Host for installed artifact is rejected |
| A18 Cross-owner scope integrity | Config/profiles/IPC declarations consume the sole owned #497/#452/#453/#541/#588 grammars; migration changes do not edit guards, runtime, navigation generator, shared parser, CI or other issue owners. A duplicate parser, authority table or unsupported field fails review |

A true unrun/broken dependency does not become `PASS` because the manifest parses. Capture observed blockers as **UNKNOWN** for the product cell and **BLOCKED** for the task, never turn an expected-pass test green by deleting or weakening it.

## 4. Design

### 4.1 Atomic ownership, trust and failure map

| One owner | Input → output | Failure witness |
|---|---|---|
| GH #497 / migrate source gate | Trusted pinned repository trees + approved key table → reversible output manifest/report | Mutate gitlink or invented/ownerless field; refuse before writing |
| KEL-78 / #586/#588 | Retained digest-verified permissions declaration → Host-admitted **effective** Legacy per current generation | A raw request or Bun stdout cannot mint a Legacy receipt; mismatched G2 stays failed/unknown |
| #541/#552 / Host Guard | Source-literal channel+window-generation grants → exact-allowed messages | Wildcard/cross-window/page-supplied string rejected **before Bun** |
| #452/#453 / Host facts | Approved role env and single-instance lock captured at admitted boot → synchronous Electron-visible values | Loader environment, stale/Promise getter, duplicate lock authority refused |
| #502 + #491 / package owner | One resolver and inert updater package → ESM/default/CJS modules without surprise installer | String-valued `electron`, missing named/default member, nested real Electron or live updater call detected |
| #446/#449/#467/#484 / facade | Genuine Init→Ready→first BrowserWindow and real WebView/preload bridge → real app page | Placeholder page or mock constructor denied; URL/sender document ownership verified |
| GH #499 / one edit | Original main bytes + approved patch digest → reviewed applied source | Undeclared second behavioral patch, missing edit or different source tree rejected |
| GH #589 and #587/#500 | Real Mac actions + independent Host/OS observers → authenticated per-cell receipt | Stale G1, forged child log, wrong document, missing disk effect, selected retry hiding failure rejected |

This migration is **not a new runtime trust boundary**. No new wire protocol, host principal, renderer authority, parser, scorer or process timeout is justified. Reuse one source of truth per owner. The future automated generator (#498 sibling, X02-T8) is *outside* this first-proof scope; a speculative helper or generalized AST rewrite is not required to hand-author the reference artifact.

### 4.2 Proposed write-set and exact key-to-owner plan

| Output | Permitted content and precise owner | State at this spec |
|---|---|---|
| `keld.config.ts` | `name`, `entry` (existing hello config); approved single-instance declaration (#453) and host-requested role environment #452/KEL-75 **only under the grammar they land** | Static sample may show name/entry; no guessed schema is implemented |
| `keld.permissions.jsonc` | Root `profile: "legacy"` (#586/#588/KEL-78) and window-specific literal `el:<channel>` + `app.system` rights (#541/#552, F06-D3) | No generic global union, fs scopes, or implicit consent |
| `package.json` | Replace existing dev-only `electron: ^44.2.0` with one @keld/electron resolution; replace `electron-updater: ^6.8.9` with #491 adapter; rewrite or remove incompatible `start: electron .` and `electron-builder` scripts | Existing original package remains unchanged in source snapshot |
| `package-lock.json` (or exact chosen lock) | Solver-produced reviewed new lock with original preimage saved; verify installed package tree, no nested real `electron`, frozen local resolution | Do not hand-edit hashes or run network-auto-install here |
| One separately declared patch | macOS-only `src/main/electron.js:876–887` async dialog replacement; record patch SHA, original and applied file SHA, hunk count | Do not apply without approval and real native close oracle qualification |

**Explicitly not written:** `keld.build.ts` (build remains reserved and has no supported source schema), `keld.compat.ts` (no reader), `bunfig.toml` alias (second conflicting resolver), `tsconfig paths.electron` (ineffective/duplicated resolver, remove if present), machine-absolute scope paths, automatically seeded fs/network/shell grants, native updater feed or installer metadata. GH #497 owns generic `keld migrate` CLI report/errors; GH #499 hand-authors only this pinned case. The approved profile switch requires a visible **weaker-security consent distinct from `--write`**; unverified/strict must not silently become Legacy.

Current `keld.config.ts` concept (only keys whose reader already exists; **not** the complete future artifact):

```ts
export default {
  name: "draw.io",
  entry: "src/main/electron.js",
};
```

Do not infer `singleInstance`, `roleEnv`, `windows` or another grammar from this illustration. Their exact fields and compiled descriptor mappings come from the named approved, implemented owners. A compliant later artifact must include them where its owning grammar requires; absence is an explicit **BLOCKER**, not a reason to invent keys.

### 4.3 Channel census: source, trace, and authority are different

At the immutable main file SHA in §2, there are **26 active literal `ipcMain.on/once/handle` call sites and 24 distinct active channel names** (three uses of `app-load-finished`). **A naive regex incorrectly reports 27/25** because `src/main/electron.js:1098–1101` contains a fully **commented-out** `/*ipcMain.on('log', ...)*/` call; it cannot create a channel grant. PANEL-P3's decision [at `23f50274`](https://github.com/gyldlab/keld/blob/23f50274/wayfinder/electron-compat/probes/panel-p3/p3_decision.md#c-fix-it-list-keld-migrate-output) proposed 15 candidate grants: `rendererReq`, `app-load-finished`, `isModified-result`, `saveAndClose-result`, `draftRemoved`, `newfile`, `toggleSpellCheck`, `toggleStoreBkp`, `toggleGoogleFonts`, `toggleFullscreen`, `zoomIn`, `zoomOut`, `resetZoom`, `export`, `export-finalize`. An independent **comment-aware** source review verified **all 15 are active main-side literals**. Its tight first-proof demand was **three candidates**: `rendererReq`, `app-load-finished`, `isModified-result`. **PANEL-P3 used a script in place of a real renderer, queued dialogs, and assumed Ready ordering; it did not produce a Host-authenticated real window or document-generation receipt.** These 15 names are therefore source+synthetic-trace candidates, **not** 15 automatic permissions, proof of current channel enforcement, or completion of #541.

Four explicit exclusions: `__ELECTRON_LOG__` and `electron-store-get-data` come from libraries without sufficient sender validation; `openDevTools` is debug-only; `checkForUpdates` is out of the update-disabled four-step path. The two library channels are *not* among the **24 active main-file literals** and must never be seeded merely because the dependency can register them. Other active main-file literal registrations, such as `svg-data`, `render-finished`, `xml-data` and `import-success`, require their own observed workflow trace and owning grant decision; do not union the whole **24-name** set.

Procedure: inspect the exact source and imported modules using a **syntax- and comment-aware** registration census (do not promote text in `//`, `/* */`, template strings or unused examples into active handlers); enumerate only actual literal registrations with file/line and handler phase, intersect them with the **synthetic PANEL-P3 operation-demand trace** as a preflight filter, then require **new, independent real-macOS #589 Host-owned window and navigation/document-generation observations** before admitting a particular per-window `channels` map under #541. The research trace has no trustworthy window principal. Static source/demand agreement is necessary but **not sufficient**. Mark any computed/dynamic registration **unenumerable** and require a human-reviewed explicit literal before granting it. Never allow `el:*`, prefix/wildcard matching, missing or wrong-generation window scopes, renderer/preload requests that create authority, or library channels by default. Native tests must prove forbidden channels fail prior to entering Bun.

### 4.4 The single disclosed behavior edit and acceptance limits

The pinned original `src/main/electron.js:851` handler is already `async`. On macOS it presents `Save`, `Cancel`, `Discard Changes` in indices **0, 1, 2**, sets `modifiedModalOpen`, correlates `uniqueIsModifiedId`, registers `saveAndClose-result` on the save path and destroys only after a successful result. The **only proposed macOS first-proof behavior hunk** replaces the call start and terminal assignment:

```diff
- let response = dialog.showMessageBoxSync(
+ let response = (await dialog.showMessageBox(
    mainWindow,
    { /* retain the exact pinned original dialog options unchanged */ }
- });
+ })).response;
```

The actual pinned call spans lines **876–887**; the above is a *description of one two-line hunk*, not an independently applied/verified patch or permission to change the object. Record the **exact** generated patch and its preimage, bytes/hunks, applied file SHA, and a revert control. **The changed shared JavaScript handler would run unconditionally on all platforms if distributed:** the phrase *macOS-only* restricts this **entire patched artifact's admissible platform to macOS**; it is **not** a source-level `if (isMac)` guard. The upstream line 875 warns about an async/destroy crash on Linux, so applying this artifact to Windows/Linux or claiming cross-platform correctness must fail until a separately reviewed, platform-gated patch and native acceptance exist. The async replacement changes event-loop interleavings; its correctness remains **UNKNOWN** until native Save/Cancel/Discard/repeated close/two windows/quit and delayed-result observers qualify. If the patch or approved bridge fails, do **not** quietly increase N: report the failure and seek a revised reviewed contract. An unmodified-source first proof instead requires the real #564 synchronous dialog transport to pass; the config-only claim cannot rest on a test stub.

Whole-app blockers independent of the patch: the two `session.defaultSession.webRequest` registrations at source lines 994/1010 must complete before later `ipcMain` setup, and every window must produce **zero** `will-attach-webview` events. `senderFrame.url` must preserve the real `file://` root identity the app checks; P3's symlink/URL mismatch made its own test fail. Do not substitute a mock or source-only promise for these native facts.

### 4.5 Reproducible evidence and fail-closed output

Freeze before attempting any OS run: desktop+webapp commits/tree/gittag/gitlink, original package and lockfile byte hashes, resolved original and actual migrated package trees, explicit Legacy permission source/digest, selected Bun/Host executable SHA and OS/WebKit, exact source patch and applied tree, attempted run ID and chosen observation protocol. Use independent OS file readback and window/dialog observer as GH-589 defines, plus typed Host-principal/current-reader and Host-owned document generation as GH-587/#588/#541 define. Preserve every attempt, including failures and retries. A `keld dev` run is **staged** and cannot pass installation.

Report proposed/static `✔/▲/✘/unclassified` only under the explicit label **static estimate, not a compatibility score**, with a reason/owner for each unsupported element and a reversible file diff. No product percentage or real KEL-74 `pass` may be printed until the approved physical corpus, authentic receipts, named review and code/docs/evidence registration are complete.

## 5. Boundaries

**This PR:** only `docs/specs/gh499-drawio-hand-authored-migration.md` and generated `llms.txt`/`llms-full.txt` *only if the allowlisted generator actually selects the new source*. No app source patch, package lock modifications, parser, permission defaults, Host, WebView, native dialog, CI/security policy, GH-497/541 spec edits, or product-registry changes.

**After a separate exact-content approval:** use a new claimed GH #499 worktree for source-verified, hand-authored *fixture* files and one reviewed patch, not inside `gyldlab/keld` framework runtime or someone else's worktree. Use the existing #497 output contract without re-implementing its analysis, and stop if #452/#453/#541/#588/#491 necessary grammar or real controlled app source is unavailable. The #499 output is a candidate consumed by #589; it **cannot** score itself.

## 6. Tasks (each independently reviewable slice)

- [ ] T0 — Publish this pinned GH-499 migration specification, source/trace/gate census and negative controls on an isolated **OPEN/DRAFT/DO NOT MERGE** PR. Run the docs/CI checks and independent exact-head source/permission review. Obtain human exact-content approval before writing fixtures or editing the app.
- [ ] T1 — After #497 and the actual schema owners qualify, prepare immutable original+modified package/lock/config/permissions artifacts, source-pinned and line-attributed minimal window grants. Run the *actual* keld-guard parser/uniqueness and relative-scope oracles, and prove no hidden/wildcard fields or nested Electron dependency. If runtime installer is missing, mark its test BLOCKED/UNKNOWN.
- [ ] T2 — When the native async-dialog or #564 synchronous alternative can be observed, publish the separately reviewed one-hunk macOS app patch with pre/post hash, failing-first duplicate-close/save-failure negative controls. No behavioral edit without native verification; preserve original app source tree.
- [ ] T3 — Give GH #589 the approved immutable prepared tree and patch receipt. It runs the actual app on real Mac with #446/#449/#541/#588 producers; record activation/open/edit/save/dirty-close effects and all attempts. Keep install UNKNOWN until KEL-141. Do not change KEL-74 corpus expectations here.

## 7. Test plan

| AC | Test and single-fault controls |
|---|---|
| A01–A02 | Independent Git object/tree/hash reader plus one-byte wrong gitlink, package lock, source and undeclared second behavior hunk; patched-source mismatch rejects before launch |
| A03, A07, A12 | Actual `keld-guard` manifest parser and static allowlist; duplicate decoded profile, misplaced key, `el:*`, dollar-bearing fs scope, host-less wildcard and invented `app.system` token each RED |
| A04–A06 | Actual **comment-aware** pinned-main registration census intersected with the source-pinned *synthetic* PANEL-P3 trace, then separately qualified by a real window/document generation and #541 grant reader; adding a renderer-only/library/unregistered/dynamic channel, admitting the **commented-out `ipcMain.on('log')` at upstream line 1098**, or granting a wrong window binding each RED |
| A08–A09 | #452/#453 approved session-declaration validator; `NODE_OPTIONS`, false updater env, missing lock or absolute entry RED; unchanged existing renderer/no-profile boot remains GREEN |
| A10–A11 | One installed-node-tree census (actual package imports, ESM/default/CJS), no runtime auto-install, one approved resolver and no incompatible scripts or forbidden configs; reintroducing `electron .` or nested real Electron RED |
| A13–A16 | Genuine macOS app/init/window, real dirty-close dialog/button choices/unique IDs/file bytes, disabled updater/zero unexpected network and zero webview attachment; async reentry, wrong URL and spoofed/missing receipts RED |
| A14, A17–A18 | Static report code/docs/evidence gate, no percentage, no dev-stage install PASS, no unqualified strict/macOS-other-platform claim; source-only mock success must remain UNKNOWN |

Use event-driven native OS/Host observers and the **existing** GH-587 receipt format, not sleeps or app console messages as proof. Tests cannot be removed, waived or asserted to pass from a mocked source import. Historical experiment data is explanatory input; actual native acceptance must be newly executed.

## 8. Review gates triggered

- **Permission model:** mandatory, for explicit Legacy consent, role-env declaration and exact-literal per-window grants. Independent GH-541/KEL-78 reviewer must verify no new ambient/default authority; Legacy explicitly forfeits strict OS containment.
- **Public API:** the future migration output/CLI contract consumes GH-497 and host-config reader; no API is changed by this docs-only PR.
- **Dependency and resolver:** future `electron`/updater replacement requires one package owner and an actual installed-tree review; this spec adds no dependency.
- **Wire protocol:** none. No new frame, principal, token, channel evaluator or IPC grant parsing in GH-499.
- **Unsafe:** none.
- Exact-content human approval of this **GH-499 spec** is not inferred from approval of GH-500 or GH-589. Independent final-head permission/source review and actual CI are required before any subsequent implementation; keep this PR DRAFT/DO NOT MERGE.

## 9. Perf impact

None from the documentation-only contract. It asserts no KELD startup, throughput or latency improvement over Electron. Future cold-start measurements must compare semantically equivalent pinned app artifacts under a declared clock, OS, workload and source patch, and must never conflate 44.4.5 documentation with the upstream 44.5.1 dependency.

## 10. Open questions

**Named external implementation gates, not silently filled decisions:** #497's authoritative output/key table and exact CLI report; #452 role env/synchronous session facts; #453 single-instance boot grammar; #491 inert updater adapter; #502 single facade resolver's ESM/default/CJS and `node_modules` behavior; #541/#552 exact channel grants, #542 document generation; #586/#588 effective Legacy/current-reader profile facts; #446/#449 actual Ready/window; #478/#564 Mac native dialog and the safe one-edit fallback; KEL-79 `file://` resource identity; #589 real Mac app and independent product collector. Until each owner supplies an implementable reviewed artifact, this spec may be approved as a bounded contract but the corresponding concrete migration/OS task is **BLOCKED** and its product operation **UNKNOWN**.

**Human action for T0:** approve or correct this exact GH-499 first-proof migration contract. Approval means permission for its separate, gated implementation steps—**not** merger, runtime/security bypass, extra app edits, installed-app proof or a product-compatibility claim.
