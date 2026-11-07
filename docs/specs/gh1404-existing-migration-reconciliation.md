# Spec: GH1404 existing migration source reconciliation

Status: draft
Linear: GH-1404 (#517 fallback) · Owner: GYLDLAB / prime · Updated: 2026-11-07

Independent source-review recommendation only. This revision supersedes sender-only draft SHA256 `3f0b7a43bc7c86281722baa1f4828abd8572afb8774d0bc678d7e58cc4255cfb` after worker-17's confirmed WindowRef-only destruction finding and the user's explicit instruction to finish the explained fix. Prime records delegated approval of this exact amendment and the exact application diff after worker-17 crosscheck; no renewed user permission is needed. This document grants no native/product acceptance, merge, or additional runtime authority.

The earlier sender/close revision `11e24a40e3499a3ecaf452015a90c0698fbd0338b00cf998bfc7a9a98d807933` was approved in GH1404/6696663884 and was actually built and installed as v4. This successor adds only the two independently reviewed, observed application corrections below: no reply for the existing fire-and-forget Exit action, and an explicit first-window diagnostic when optional visual-zoom limits are unavailable. Existing v4 results and failures remain separate evidence; final successor acceptance is not inferred.

## 1. Goal & non-goals

Use the user's existing KELD-powered draw.io Desktop 31.7.0 migration as the installed macOS consumer. Select sender adaptation plus original-document close coordination: retained close attempt, captured original replies, actual veto settlement and scoped native destruction. Preserve `dialog.showMessageBoxSync`, its exact original options and button meanings; the surrounding close implementation changes and requires its own source/native proof.
[Packaging §1](macos-local-app-packaging.md) already selects the original migration. After exact approval, “original main” in [installed graph/source AC8 and T2](macos-installed-graph-source.md) means the pinned upstream main plus the declared patch below. A wholly unmodified main was an implementation assumption, not a new user requirement.
Keep renderer, webapp assets, preload, physical `codeDir`/`codeUrl`, original query behavior and original source archive unchanged. Exclude importer/exporter qualification, browser-storage persistence, new URL identity APIs, native file-origin fabrication, other platforms and compatibility scores.

## 2. Spec refs

- [GH499 exact owner approval 6465227830](https://github.com/gyldlab/keld/issues/499#issuecomment-6465227830): reviewed contract at `a584bb15ceedccc0bee17e9388000e5a37fdfcf8`, raw SHA256 `ef3c2a1ab04e2bb4a1bb515e3424ef2321d64cfb0709cc6c2c75dc0b6429f395`; exact implementation still has independent gates.
- [Approved contract A02/A13/§4.4/T2](https://github.com/gyldlab/keld/blob/a584bb15ceedccc0bee17e9388000e5a37fdfcf8/docs/specs/gh499-drawio-hand-authored-migration.md): two separately gated categories, dirty-close and resource-entry/navigation/sender. This revision uses both categories. §4.4 permits the synchronous transport alternative, but its historical two-line async-dialog proposal does not automatically approve the newly enumerated close-coordination bytes; this exact amendment/diff needs prime's delegated approval.
- [GH473 chosen truthful origin 6398386345](https://github.com/gyldlab/keld/issues/473#issuecomment-6398386345) and [bounded S1 authorization 6556536264](https://github.com/gyldlab/keld/issues/473#issuecomment-6556536264): exact recorded main adjustment, native source identity, unchanged page/preload; neither authorizes unrelated edits.
- [GH499 reconciliation 6665421889](https://github.com/gyldlab/keld/issues/499#issuecomment-6665421889): older GH587/GH589/GH500 one-edit consumers and GH467 file-identity clauses remain distinct owner contracts.
- [GH564 §S10](gh564-macos-sync-message-box.md), [runtime facts](macos-installed-original-runtime-facts.md), architectures [03](../architecture/03-security.md), [04](../architecture/04-electron-compat.md) and [05](../architecture/05-webview-and-native.md) retain their independent boundaries.

## 3. Acceptance criteria

1. Preserve upstream desktop `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`, webapp `24b76c2cbd55d88e354042e8d329a2e4708972bc`, original package/lock and immutable source receipts. Hash the original main, exact patch, applied main and reverse diff; an undeclared hunk refuses admission.
2. Independently compare the complete renderer/assets tree and original preload. Expected preload SHA256 is `1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98`. A hash declaration alone is not a completed comparison.
3. Admit only the actual current native editor document under its existing source, view, role, window and exact-channel grants. Forged/copy/prototype events, wrong source with matching URL, wrong authority/window, retired document and missing grant have no privileged effect.
4. Real original page/preload request receives its correlated response exactly once. Native Open/Edit/Save/XML readback/dirty Cancel/Save-failure/Discard/Reopen/Quit remain independently observed on the final installed artifact.
5. Preserve the unadapted Electron/file oracle: original predicate accepts genuine file identity and rejects custom identity. Adapted success earns no file-parity, L2, corpus or public product score; GH564's unmodified-main S10 is not satisfied by this adapted consumer.
6. Every declared close continuation retains document A's admitted original event. Its outbound close replies and all four terminal destruction paths cannot target successor B. Stale continuations retire only their own attempt/listeners and cannot clear a newer attempt's flags or bookkeeping. Actual veto settlement, physical destruction and correlated completion remain separately checked.
7. The synchronous prompt call and complete original options remain byte-identical; all three internal helpers resolve to the existing qualified facade implementation. Watcher and other asynchronous application lifetimes are outside this bounded close claim and remain separately diagnosed by prime when relevant.
8. The existing Exit action requests Quit exactly once and returns without an unused `mainResp`; ordinary requests still receive their correlated response. The first-window load listener warns when `setVisualZoomLevelLimits` is absent and continues its original `loadFinished` join. When callable, the real method is invoked once with its original receiver/arguments; its actual error still propagates. No visual-zoom support, limit enforcement or behavior of the other two load-listener paths is claimed.

## 4. Design

**No boundary change.** The app consumes the existing authenticated event capability. Native source ownership, principal minting, Guard, process/crash ownership and transport remain with their current owners; no new permission, wire field, dependency, credential or logical-URL API is introduced.

| Independent atom / owner | Existing input → output | Failure witness |
| --- | --- | --- |
| Source / native WK owner | Retained verified source + actual main-frame URL/origin/view → qualified event facts | Different entry/authority/view refuses even with plausible URL text |
| Authentication / Core | Current native lease + admitted role + exact Guard channel → one Bun event | Retired role/document or absent grant yields no event/effect |
| Observation / API | Host window incarnation, navigation and native URL → current document observation | Same-URL successor, retirement or link end invalidates old event |
| Event identity / Electron | Module-private WeakMap event key + current observation → eligibility boolean | Copy, prototype child or invented event cannot acquire eligibility |
| Consumer / app | Eligible event + expected owning contents + equal frame/getURL → sender decision | Wrong window or inconsistent URL fails; entry validation cannot authorize a later generic destroy |
| Close / existing facade and native owner | Retained original event + actual veto settlement → captured reply and document-scoped destruction | Replace A with B between check and effect; no B reply/destroy or newer-attempt retirement |
| Evidence / prime | Exact source/provider/Host inputs + independent native observations → bounded acceptance | A passing component or old artifact cannot establish installed workflow success |

The production chain is inspectable in `macos_bridge.rs:handle_compat_message/capture_native_message_facts`, `macos_resource_source.rs:qualify_document_url/qualify_native_origin`, `macos_navigation_owner.rs`, and Core `app_session/compat_router.rs`.
Native capture checks the actual main frame and WKWebView, exact selected entry and observed origin tuple. Navigation commits bind the same retained source. Core repeats currentness and final Guard at event writing; app URL comparison does not replace these checks.
`packages/@keld/api/src/compat-source.ts` checks window incarnation, native URL, navigation and currentness. Retirement invalidates only its exact generation; duplicate commits cannot revive it. Link close ends the observer.
`packages/@keld/electron/src/ipc-main.ts` alone installs the real event in its private WeakMap. Its predicate requires the retained current callback and exact sender-object lookup. Captured WeakMap intrinsics prevent later prototype replacement from inventing membership.
The `electron/internal/original-ipc` export shares that same implementation. Import all three existing helpers: `isCurrentOriginalIpcDocument`, `awaitOriginalIpcCloseVeto` and `destroyOriginalIpcDocument`. `senderFrame` becomes null after retirement; `getURL()` reads native observed URL. Captured-event replies retain A's original view/navigation and recheck before carriage.
Worker-17 confirmed generic `mainWindow.destroy()` selects a WindowRef, not the document generation. The window can contain successor B after an await or native race; another JavaScript currentness check does not close that gap. Use the existing original-event operation, whose immutable selector includes the original window and document generation and is checked by the native destruction owner.
Each close attempt retains its original event and correlation. Validate the owning contents; register owned acknowledgement listeners before captured-event replies. Recheck the retained attempt after synchronous prompt return and asynchronous waits. Await `awaitOriginalIpcCloseVeto` for the actual authenticated Veto/ReplyApplied settlement, never infer settlement from the clicked index or a locally resolved Promise.
All four original destruction branches route through `destroyOriginalIpcDocument` on the retained event. Preserve its current-document admission and physical Destroyed/correlated-response proof; successful destruction normally retires that document, so do not convert success into failure by demanding currentness afterward. A stale refusal is not user Cancel, and an unknown admitted outcome must not be retried.
Retire only the matching attempt and its registered listeners. Close bookkeeping must respect the existing physical Destroyed ordering and must not release or reset a successor's attempt. These are bounded source-design requirements, not new native test results or cancellation of already admitted filesystem work.

**Exit delivery correction.** The original renderer's Exit call supplies no completion callback. Its main handler nevertheless sent `mainResp` after requesting Quit. The current Core owner admits one main-to-renderer DATA operation until both renderer consumption and native quiescence; the close handler's `isModified` message can overlap that unused reply. Actual v4 stderr records an unadmitted `webContents.send` capacity error, while native title-bar close and application-menu Quit succeed. It does not identify the historical incumbent correlation, so retain that causal limit. Reuse the donor's exact `break` to `return` change only in `case 'exit'`. No capacity increase, retry, queue or general response suppression is permitted. The controlled exact-handler regression fails on v4's held unused reply and passes with this change; the final real File → Exit path remains required.

**First-window optional zoom correction.** Actual installed v4 stderr records a missing `setVisualZoomLevelLimits` method before the original `loadFinished()` call. This prevents the existing native-load/renderer-load join from finishing. At this one first-window site, check whether the method is callable; otherwise issue a diagnostic explicitly stating that the limit is not applied, then execute the existing join. Do not install a method, return a fabricated Promise/result, catch a real implementation's error, or claim native zoom behavior. The existing `zoomFactor` assignment remains unchanged. The separate second-instance and OS-open-file call sites remain unqualified and are not silently covered by this bounded startup adaptation.

### Exact selective patch and complete donor exclusions

The complete 26-hunk difference between these files was inspected. Source paths are recorded relative to the retained `finish-20261106-1847/` evidence root:

- Baseline: `provider-and-app/sync-provider-successor/final-run/original-app/src/main/electron.js`; 119176 bytes; SHA256 `55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d`.
- Donor: `source-recovery/migrated-app/src/main/electron.js`; 123713 bytes; SHA256 `0ac84264a7744be0b75e2c8a8182900032d297e3137e13d1784c24243b3c74a9`.

Line anchors below refer to the baseline, not the final patched file.

| Baseline location / donor delta | Selected disposition |
| --- | --- |
| Import after line 6 | Import `isCurrentOriginalIpcDocument`, `awaitOriginalIpcCloseVeto` and `destroyOriginalIpcDocument` from the same existing `electron/internal/original-ipc` module |
| `validateSender`, 694–700 | Reuse donor event/optional-expectedContents predicate: real capability; present string URL and nonempty/non-`null` native origin; live sender; expected sender if supplied; exact `frame.url === sender.getURL()` |
| 22 existing call sites | Pass whole `e`/`event`; preserve existing null checks/correlations and add the declared owning-contents/retained-attempt checks inside close handling. Commented or unsupported-path text creates no grant |
| Close/lifecycle, 849–984 | Select only scoped attempt state/retirement, owned listener cleanup, captured-event reply ordering, actual veto settlement and correlated native-close bookkeeping needed by the four branches below; exclude the donor's async dialog replacement and unrelated cleanup |
| Prompt, 875–887 | Keep the original `showMessageBoxSync` call and complete options, including Save/Cancel/Discard mapping; no awaited `showMessageBox` substitution |
| Four original destroys, 904/921/931/943 | Scope Save-success acknowledgement, Discard after awaited draft deletion, Discard after `draftRemoved`, and clean-close destruction to the attempt's captured original event; no generic WindowRef-only destroy remains in these branches |
| Zoom, 1836 | After the actual v4 startup failure, select only the first-window callable check and explicit unavailable-limit diagnostic, followed by original `loadFinished`; retain the real call/error when present. Other two call sites and native zoom implementation remain outside scope |
| Watcher, 4367–4371 and 4520 | Exclude donor signature/caller rewrite and post-await guard; prime diagnoses this separate path if needed, without extending this amendment's lifecycle claim |
| Exit, 4528 | After the actual v4 File → Exit failure, select only donor `break` → `return` after the unchanged Quit request; no unused reply and no change to ordinary request responses |

The 22 original sender-call anchors are 853, 898, 929, 1029, 1036, 1440, 1447, 1737, 1779, 1842, 1855, 1868, 1881, 1898, 2161, 3061, 3149, 3351, 3366, 3378, 3385 and 4433. The additional close changes are confined to the declared 849–984 region and helper import. Prime's final exact diff, applied hash and reverse diff must enumerate their actual hunks; the donor's full 26-hunk patch is not adopted wholesale.
This full diff contains no change to `codeDir`, `codeUrl`, entry-loading logic or print/export algorithms. Import/export validator call conversions do not qualify those windows or authorize their channels.
The original `watchFile` awaits path validation before installing a watcher and later uses current-window `send`. That separate concern remains for prime's bounded diagnosis if exercised or implicated; the donor's single post-await recheck alone does not prove callback isolation. This sender/close amendment neither adopts the watcher edits nor claims all application continuations are protected.
The scoped close patch addresses worker-17's confirmed native-generation gap while preserving the synchronous prompt. Worker-17 crosschecks the exact implementation before prime approves it. The successor's two declared application corrections require their own exact combined diff and review. No further zoom, exit, watcher or framework URL/wire change is authorized implicitly.

## 5. Boundaries

This worker updates only this existing document. Worker-17 owns the separately assigned exact selective application patch; prime owns adoption, source/provider sealing, Git, Cargo, builds and native GUI work.
After exact approval, the source change is confined to the declared main-file patch. Renderer/preload, framework URL identity, source mint, wire, Guard/grants, platform implementation and unrelated WIP are outside it.
The patch belongs to the app migration artifact, not a new framework special case. The original file remains independently available and reversible.

## 6. Tasks

- [ ] Worker-17 crosschecks this sender/close contract against its exact patch; prime records delegated approval of the final document, patch and applied hashes under the existing GH1404 claim. The user's continuation authority is already granted.
- [ ] Apply only the approved subset to an isolated prepared source; independently verify reverse diff and unchanged webapp/preload before regenerating the sealed graph and artifact receipts.
- [ ] Run the composed source/native denial and synchronous-close checks, then the installed workflow; publish separate passed/failed/unrun outcomes.
- [ ] Preserve consumer-contract holds until GH587/GH589/GH500/GH467 owners reconcile their own clauses; this document does not modify those scores or contracts.

## 7. Test plan

AC1–2, AC7: independent full byte/diff census; one undeclared hunk and one changed immutable source each refuse. Confirm all three packed internal helpers resolve to the same facade instance and the original synchronous prompt/options bytes remain unchanged.
AC3: genuine native event succeeds; forged/prototype/copied event, wrong WindowRef/incarnation, same-URL replacement, retirement, link death, foreign entry/authority and missing literal grant each fail independently.
AC4: root performs real installed macOS Open → Edit → Save → XML readback → unsaved Cancel/Save failure/Discard → Reopen → Quit. Preserve native and child-cleanup evidence for final signed .app and DMG.
AC5: execute unchanged Electron/file reference separately; preserve its custom-origin rejection. Adapted workflow and GH564 unmodified-main S10 must have different evidence labels.
AC6: hold A across synchronous-dialog return, actual veto settlement, Save acknowledgement and both Discard branches; retire A/commit B and release each held operation. Verify captured replies cannot reach B, all four original destruction branches reject a replacement document at native admission, and stale cleanup cannot remove B's listeners/flags or close bookkeeping. Exercise repeated close, wrong-window acknowledgement, failed/cancelled Save and successful physical destruction; a generic-destroy substitution or captured-reply removal must fail the corresponding control.
AC8: exact extracted Exit and close handlers must request Quit, veto and send `isModified`, without a pending unused `mainResp`; deleting Quit or all ordinary replies must fail independent positive controls. Original first-window callback, in both native/renderer event orders: absent method fails before the correction; explicit absence diagnostic permits the original join afterward; callable invocation preserves receiver/arguments and a real thrown error is not swallowed. These source controls do not substitute for native File → Exit or installed startup qualification.
Watcher behavior is separately diagnosed by prime if needed. No watcher-isolation or whole-application asynchronous-lifetime PASS is asserted by these close controls.
No tests, build or native execution were performed by this review. Existing test source establishes available cases only; root reports actual runs.

## 8. Review gates triggered

Independent source/security and exact-content amendment review cover both GH499 categories and the exact close-coordination bytes. Permission model: consumption of existing Legacy/source/Guard contracts, no widening. Public API: three existing internal helper imports and app-source admission reviewed; no new public API. Unsafe, dependency addition and wire protocol: none introduced.
Existing framework/component review holds are neither removed nor automatically reimposed from old prose; their current owners supply exact-current evidence. Prime can approve the bounded decision under the user's existing delegation after independent challenge.

## 9. Perf impact

No performance claim. Existing WeakMap/current-observation checks, actual veto settlement and document-scoped destruction are reused. The synchronous prompt remains synchronous; close continuations may await their existing owners. No new transport, clock, native budget or performance claim is introduced.

## 10. Open questions

Blocking before adoption: worker-17's exact patch/contract crosscheck, prime's recorded delegated amendment/diff approval and qualified identity of all three packed helpers. Blocking before full source/product acceptance: composed native source denials, the four document-scoped close branches and their retirement/cross-window tests, and final installed workflow evidence. Prime separately diagnoses the watcher concern if needed; this bounded decision does not certify its lifecycle.
Approval of this amendment settles only the declared GH1404 migrated-consumer interpretation and exact authorized patch. It cannot turn pending tests, original-file divergence, legacy corpus clauses or unavailable native receipts into PASS.
