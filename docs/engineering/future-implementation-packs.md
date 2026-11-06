# Future implementation packs

Publication and discovery index for [GH-1414](https://github.com/gyldlab/keld/issues/1414). Refreshed on 6 November 2026 from immutable GitHub sources. The main source pin is [`0eb3ce203187`](https://github.com/gyldlab/keld/commit/0eb3ce203187f35c5276a4237189c906a5220d5a).

## Verified publication set

**77 feature specifications in 34 open draft PRs: 21 original packs plus 56 expansion specifications.** The planning-index PR is excluded. The files define **648 acceptance criteria and 304 ordered implementation tasks**.

Counts come from complete files at the pinned publication revisions, not PR description totals. Each specification has all ten template sections and stays within 500 lines, 40 KiB and 400 characters per line. Published bytes are verified against their Git blob identities.

Every feature remains draft/do-not-merge. Source review, document-shape checks and hosted documentation CI establish different facts from exact-content approval, implementation, original-application behavior and native platform acceptance. Those product/native tests were not executed by this documentation lane.

The earlier 21-pack and 63-total handoffs are historical snapshots. The 63 figure was a total, not 63 additional specifications. This publication table supersedes their counts and source pointers.

### Grouped expansion batches

| Draft | Specifications | Criteria | Tasks | Current review evidence |
|---|---:|---:|---:|---|
| [#1432](https://github.com/gyldlab/keld/pull/1432) | 5 | 36 | 20 | [source review](https://github.com/gyldlab/keld/pull/1432#issuecomment-6682963353) |
| [#1433](https://github.com/gyldlab/keld/pull/1433) | 3 | 18 | 12 | [source review](https://github.com/gyldlab/keld/pull/1433#issuecomment-6682970735) |
| [#1434](https://github.com/gyldlab/keld/pull/1434) | 4 | 31 | 16 | [source review](https://github.com/gyldlab/keld/pull/1434#issuecomment-6683004656) |
| [#1435](https://github.com/gyldlab/keld/pull/1435) | 6 | 35 | 18 | [source review](https://github.com/gyldlab/keld/pull/1435#issuecomment-6682974263) |
| [#1436](https://github.com/gyldlab/keld/pull/1436) | 5 | 43 | 20 | [source review](https://github.com/gyldlab/keld/pull/1436#issuecomment-6682580411) |
| [#1437](https://github.com/gyldlab/keld/pull/1437) | 3 | 26 | 12 | [source review](https://github.com/gyldlab/keld/pull/1437#issuecomment-6682574481) |
| [#1438](https://github.com/gyldlab/keld/pull/1438) | 3 | 28 | 15 | [source review](https://github.com/gyldlab/keld/pull/1438#issuecomment-6683256192) |
| [#1439](https://github.com/gyldlab/keld/pull/1439) | 4 | 41 | 16 | [source review](https://github.com/gyldlab/keld/pull/1439#issuecomment-6683363998) |
| [#1440](https://github.com/gyldlab/keld/pull/1440) | 6 | 55 | 24 | [source review](https://github.com/gyldlab/keld/pull/1440#issuecomment-6683378042) |
| [#1441](https://github.com/gyldlab/keld/pull/1441) | 3 | 29 | 12 | [source review](https://github.com/gyldlab/keld/pull/1441#issuecomment-6683595058) |
| [#1442](https://github.com/gyldlab/keld/pull/1442) | 3 | 30 | 12 | [source review at ab677a3](https://github.com/gyldlab/keld/pull/1442#issuecomment-6686069171); native proof pending |
| [#1443](https://github.com/gyldlab/keld/pull/1443) | 4 | 42 | 16 | [source review](https://github.com/gyldlab/keld/pull/1443#issuecomment-6683546084) |
| [#1444](https://github.com/gyldlab/keld/pull/1444) | 7 | 67 | 34 | Independent final-head semantic review underway |

### Every published feature file

| Owner | Specification | Draft and immutable source |
|---|---|---|
| [#1127](https://github.com/gyldlab/keld/issues/1127) | Linux isolated app preload and guarded bridge | [#1408](https://github.com/gyldlab/keld/pull/1408) · [spec](https://github.com/gyldlab/keld/blob/719ce9118d8455471481aea6e718c2b6fcfc25f3/docs/specs/gh1127-linux-preload-bridge.md) |
| [#1126](https://github.com/gyldlab/keld/issues/1126) | Linux navigation and JavaScript result delivery | [#1409](https://github.com/gyldlab/keld/pull/1409) · [spec](https://github.com/gyldlab/keld/blob/aaafcbfe69e29ff5ca68bdc9093eea1af85f40cc/docs/specs/gh1126-linux-webcontents-navigation-eval.md) |
| [#590](https://github.com/gyldlab/keld/issues/590) | static migration analysis and binding-aware scan rules | [#1410](https://github.com/gyldlab/keld/pull/1410) · [spec](https://github.com/gyldlab/keld/blob/65a406c0345c61351db7afc4fcf2132feb44d59d/docs/specs/gh590-migration-analyzer-scan-rules.md) |
| [#1189](https://github.com/gyldlab/keld/issues/1189) | Windows native dialogs and synchronous dirty-close results | [#1411](https://github.com/gyldlab/keld/pull/1411) · [spec](https://github.com/gyldlab/keld/blob/8d6a280ed43e92b654294f692dd69cc2941ff090/docs/specs/gh1189-windows-native-dialogs.md) |
| [#594](https://github.com/gyldlab/keld/issues/594) | Window web grants through the shared Guard owner | [#1412](https://github.com/gyldlab/keld/pull/1412) · [spec](https://github.com/gyldlab/keld/blob/66390503b01560ac96b1f4db9aea60116ecae792/docs/specs/gh594-window-web-grants.md) |
| [#623](https://github.com/gyldlab/keld/issues/623) | Bounded outbound retention on the existing Bun role link | [#1413](https://github.com/gyldlab/keld/pull/1413) · [spec](https://github.com/gyldlab/keld/blob/819c347f9e0e000af2f438311b63f31c76077973/docs/specs/gh623-bounded-outbound-budget.md) |
| [#591](https://github.com/gyldlab/keld/issues/591) | Reversible project writes for keld migrate | [#1415](https://github.com/gyldlab/keld/pull/1415) · [spec](https://github.com/gyldlab/keld/blob/20cfcab90c7a2dd9c74c3de42ef68a8062671521/docs/specs/gh591-reversible-migrate-write.md) |
| [#1129](https://github.com/gyldlab/keld/issues/1129) | Linux draw.io product proof | [#1416](https://github.com/gyldlab/keld/pull/1416) · [spec](https://github.com/gyldlab/keld/blob/ce0596ff0080d496610ef1675325c64ddc92fd16/docs/specs/gh1129-linux-drawio-product-proof.md) |
| [#555](https://github.com/gyldlab/keld/issues/555) | synchronous Legacy connectivity reads on macOS | [#1418](https://github.com/gyldlab/keld/pull/1418) · [spec](https://github.com/gyldlab/keld/blob/70cc54e673f6b5e76b10ed0a304ff51f93683197/docs/specs/gh555-legacy-net-online.md) |
| [#550](https://github.com/gyldlab/keld/issues/550) | Dropped-file identity for webUtils.getPathForFile | [#1419](https://github.com/gyldlab/keld/pull/1419) · [spec](https://github.com/gyldlab/keld/blob/449e61437c4106dafacdf3bdc69a163de72edaf4/docs/specs/gh550-dropped-file-path.md) |
| [#488](https://github.com/gyldlab/keld/issues/488) | Renderer process termination | [#1420](https://github.com/gyldlab/keld/pull/1420) · [spec](https://github.com/gyldlab/keld/blob/044da001a871653ef20b933251ff5183b25f736e/docs/specs/gh488-render-process-gone.md) |
| [#574](https://github.com/gyldlab/keld/issues/574) | Renderer responsiveness transitions | [#1421](https://github.com/gyldlab/keld/pull/1421) · [spec](https://github.com/gyldlab/keld/blob/5d25070bc030ec38e9e896f0acd43c27753676fa/docs/specs/gh574-renderer-responsiveness.md) |
| [#1356](https://github.com/gyldlab/keld/issues/1356) | macOS system appearance subscriptions for Zettlr | [#1422](https://github.com/gyldlab/keld/pull/1422) · [spec](https://github.com/gyldlab/keld/blob/f8a23c14bcf670b84e042d63d95995f3fc62c14b/docs/specs/gh1356-system-preferences-theme.md) |
| [#593](https://github.com/gyldlab/keld/issues/593) | Disable Bun runtime installation for supervised roles | [#1423](https://github.com/gyldlab/keld/pull/1423) · [spec](https://github.com/gyldlab/keld/blob/86765a90e378d112a1cbc55f6685a55c745db089/docs/specs/gh593-bun-auto-install-refusal.md) |
| [#534](https://github.com/gyldlab/keld/issues/534) | macOS BrowserWindow state, mutations and owner lookups | [#1424](https://github.com/gyldlab/keld/pull/1424) · [spec](https://github.com/gyldlab/keld/blob/5aee5eeb80aaea24d222b921eca3ec4038fd3e20/docs/specs/gh534-browserwindow-state-members.md) |
| [#521](https://github.com/gyldlab/keld/issues/521) | macOS application reopen and activate | [#1425](https://github.com/gyldlab/keld/pull/1425) · [spec](https://github.com/gyldlab/keld/blob/c2afd5b93e892f0a29ed1e3bb030c4e3cf6b8fca/docs/specs/gh521-macos-activate.md) |
| [#522](https://github.com/gyldlab/keld/issues/522) | App path overrides without changing the webview profile | [#1426](https://github.com/gyldlab/keld/pull/1426) · [spec](https://github.com/gyldlab/keld/blob/2b5a4f978d84ebd7ddcbda3fb1c69c8a602bb731/docs/specs/gh522-app-path-overrides.md) |
| [#474](https://github.com/gyldlab/keld/issues/474) | Default Session identity and macOS website-data clearing | [#1427](https://github.com/gyldlab/keld/pull/1427) · [spec](https://github.com/gyldlab/keld/blob/ffdbb828d36b40718f99d7b823830b924c120123/docs/specs/gh474-session-profile-clear.md) |
| [#479](https://github.com/gyldlab/keld/issues/479) | Guarded native shell dispatch | [#1428](https://github.com/gyldlab/keld/pull/1428) · [spec](https://github.com/gyldlab/keld/blob/806940d03d767b66ad880f519ad5d11002d1ba99/docs/specs/gh479-guarded-shell-broker.md) |
| [#547](https://github.com/gyldlab/keld/issues/547) | deliver named Electron invokes to the registered main handler | [#1429](https://github.com/gyldlab/keld/pull/1429) · [spec](https://github.com/gyldlab/keld/blob/b303a80c44999eac0dd33780c3b5dd7ec78db3bf/docs/specs/gh547-invoke-handle-delivery.md) |
| [#556](https://github.com/gyldlab/keld/issues/556) | macOS non-role application menu selection | [#1430](https://github.com/gyldlab/keld/pull/1430) · [spec](https://github.com/gyldlab/keld/blob/092ba4ff958ad401bf4d7d70c49492f0f6fcff4d/docs/specs/gh556-non-role-menu-click.md) |
| [#451](https://github.com/gyldlab/keld/issues/451) | Cancelable two-phase application quit | [#1432](https://github.com/gyldlab/keld/pull/1432) · [spec](https://github.com/gyldlab/keld/blob/04f65fcfd3cf82fdf1fbf68cee27ab937b4949de/docs/specs/gh451-cancelable-two-phase-quit.md) |
| [#453](https://github.com/gyldlab/keld/issues/453) | Host-owned single-instance verdict | [#1432](https://github.com/gyldlab/keld/pull/1432) · [spec](https://github.com/gyldlab/keld/blob/04f65fcfd3cf82fdf1fbf68cee27ab937b4949de/docs/specs/gh453-host-single-instance-verdict.md) |
| [#523](https://github.com/gyldlab/keld/issues/523) | Host-authorized application exit and shell status | [#1432](https://github.com/gyldlab/keld/pull/1432) · [spec](https://github.com/gyldlab/keld/blob/04f65fcfd3cf82fdf1fbf68cee27ab937b4949de/docs/specs/gh523-host-authorized-app-exit.md) |
| [#524](https://github.com/gyldlab/keld/issues/524) | Instance, file and URL handoff ordering | [#1432](https://github.com/gyldlab/keld/pull/1432) · [spec](https://github.com/gyldlab/keld/blob/04f65fcfd3cf82fdf1fbf68cee27ab937b4949de/docs/specs/gh524-instance-file-url-handoff.md) |
| [#563](https://github.com/gyldlab/keld/issues/563) | Menu role quit routing into the app quit owner | [#1432](https://github.com/gyldlab/keld/pull/1432) · [spec](https://github.com/gyldlab/keld/blob/04f65fcfd3cf82fdf1fbf68cee27ab937b4949de/docs/specs/gh563-menu-role-quit-routing.md) |
| [#456](https://github.com/gyldlab/keld/issues/456) | macOS native window chrome and capability options | [#1433](https://github.com/gyldlab/keld/pull/1433) · [spec](https://github.com/gyldlab/keld/blob/d110221dae48df373ea7a57368c3b09a08aedf6d/docs/specs/gh456-macos-window-chrome.md) |
| [#529](https://github.com/gyldlab/keld/issues/529) | Window close across a parked or recovered app role | [#1433](https://github.com/gyldlab/keld/pull/1433) · [spec](https://github.com/gyldlab/keld/blob/d110221dae48df373ea7a57368c3b09a08aedf6d/docs/specs/gh529-close-across-park-recovery.md) |
| [#535](https://github.com/gyldlab/keld/issues/535) | Hidden-window presentation and qualified readiness | [#1433](https://github.com/gyldlab/keld/pull/1433) · [spec](https://github.com/gyldlab/keld/blob/d110221dae48df373ea7a57368c3b09a08aedf6d/docs/specs/gh535-hidden-window-readiness.md) |
| [#459](https://github.com/gyldlab/keld/issues/459) | macOS webContents identity and load completion | [#1434](https://github.com/gyldlab/keld/pull/1434) · [spec](https://github.com/gyldlab/keld/blob/c7e85ab228bed45c5a9bdeefcb6db8a98f735830/docs/specs/gh459-macos-webcontents-navigation.md) |
| [#462](https://github.com/gyldlab/keld/issues/462) | Completion-backed page-world JavaScript evaluation | [#1434](https://github.com/gyldlab/keld/pull/1434) · [spec](https://github.com/gyldlab/keld/blob/c7e85ab228bed45c5a9bdeefcb6db8a98f735830/docs/specs/gh462-completion-backed-evaluation.md) |
| [#536](https://github.com/gyldlab/keld/issues/536) | Developer-tools facade with truthful native state | [#1434](https://github.com/gyldlab/keld/pull/1434) · [spec](https://github.com/gyldlab/keld/blob/c7e85ab228bed45c5a9bdeefcb6db8a98f735830/docs/specs/gh536-devtools-facade.md) |
| [#538](https://github.com/gyldlab/keld/issues/538) | Full macOS navigation event vocabulary | [#1434](https://github.com/gyldlab/keld/pull/1434) · [spec](https://github.com/gyldlab/keld/blob/c7e85ab228bed45c5a9bdeefcb6db8a98f735830/docs/specs/gh538-macos-navigation-vocabulary.md) |
| [#454](https://github.com/gyldlab/keld/issues/454) | Honest local app compatibility shims | [#1435](https://github.com/gyldlab/keld/pull/1435) · [spec](https://github.com/gyldlab/keld/blob/f60b1aca9e6a2dee22c5a94bf645c6cd01162759/docs/specs/gh454-facade-local-app-shims.md) |
| [#575](https://github.com/gyldlab/keld/issues/575) | Local-only crashReporter compatibility state | [#1435](https://github.com/gyldlab/keld/pull/1435) · [spec](https://github.com/gyldlab/keld/blob/f60b1aca9e6a2dee22c5a94bf645c6cd01162759/docs/specs/gh575-local-crash-reporter.md) |
| [#576](https://github.com/gyldlab/keld/issues/576) | Bun-side process metric compatibility | [#1435](https://github.com/gyldlab/keld/pull/1435) · [spec](https://github.com/gyldlab/keld/blob/f60b1aca9e6a2dee22c5a94bf645c6cd01162759/docs/specs/gh576-process-metrics-facade.md) |
| [#577](https://github.com/gyldlab/keld/issues/577) | Host-owned application metric snapshot | [#1435](https://github.com/gyldlab/keld/pull/1435) · [spec](https://github.com/gyldlab/keld/blob/f60b1aca9e6a2dee22c5a94bf645c6cd01162759/docs/specs/gh577-host-app-metrics.md) |
| [#578](https://github.com/gyldlab/keld/issues/578) | Named-role child-process-gone events | [#1435](https://github.com/gyldlab/keld/pull/1435) · [spec](https://github.com/gyldlab/keld/blob/f60b1aca9e6a2dee22c5a94bf645c6cd01162759/docs/specs/gh578-named-role-exit-events.md) |
| [#579](https://github.com/gyldlab/keld/issues/579) | Structured contentTracing refusal facade | [#1435](https://github.com/gyldlab/keld/pull/1435) · [spec](https://github.com/gyldlab/keld/blob/f60b1aca9e6a2dee22c5a94bf645c6cd01162759/docs/specs/gh579-content-tracing-refusal.md) |
| [#1375](https://github.com/gyldlab/keld/issues/1375) | app-declared sendSync reply mirrors | [#1436](https://github.com/gyldlab/keld/pull/1436) · [spec](https://github.com/gyldlab/keld/blob/c4e5f3973807a7435cbf10fa9ec1234f5703f274/docs/specs/gh1375-sendsync-reply-mirror.md) |
| [#543](https://github.com/gyldlab/keld/issues/543) | addressed host-to-renderer named events | [#1436](https://github.com/gyldlab/keld/pull/1436) · [spec](https://github.com/gyldlab/keld/blob/c4e5f3973807a7435cbf10fa9ec1234f5703f274/docs/specs/gh543-addressed-renderer-events.md) |
| [#545](https://github.com/gyldlab/keld/issues/545) | measured renderer payload profile for draw.io open and save | [#1436](https://github.com/gyldlab/keld/pull/1436) · [spec](https://github.com/gyldlab/keld/blob/c4e5f3973807a7435cbf10fa9ec1234f5703f274/docs/specs/gh545-measured-renderer-payloads.md) |
| [#548](https://github.com/gyldlab/keld/issues/548) | explicit sendSync fallback and conformance cells | [#1436](https://github.com/gyldlab/keld/pull/1436) · [spec](https://github.com/gyldlab/keld/blob/c4e5f3973807a7435cbf10fa9ec1234f5703f274/docs/specs/gh548-sendsync-fallback.md) |
| [#549](https://github.com/gyldlab/keld/issues/549) | bounded contextBridge return shapes for Zettlr | [#1436](https://github.com/gyldlab/keld/pull/1436) · [spec](https://github.com/gyldlab/keld/blob/c4e5f3973807a7435cbf10fa9ec1234f5703f274/docs/specs/gh549-contextbridge-return-shapes.md) |
| [#1128](https://github.com/gyldlab/keld/issues/1128) | Linux explicit-profile qualification | [#1437](https://github.com/gyldlab/keld/pull/1437) · [spec](https://github.com/gyldlab/keld/blob/62653e00c951570565728db1d8527f93e607c935/docs/specs/gh1128-linux-profile-qualification.md) |
| [#1187](https://github.com/gyldlab/keld/issues/1187) | Windows explicit-profile and descendant qualification | [#1437](https://github.com/gyldlab/keld/pull/1437) · [spec](https://github.com/gyldlab/keld/blob/62653e00c951570565728db1d8527f93e607c935/docs/specs/gh1187-windows-profile-qualification.md) |
| [#1355](https://github.com/gyldlab/keld/issues/1355) | Legacy watcher and Zettlr filesystem qualification | [#1437](https://github.com/gyldlab/keld/pull/1437) · [spec](https://github.com/gyldlab/keld/blob/62653e00c951570565728db1d8527f93e607c935/docs/specs/gh1355-legacy-watcher-qualification.md) |
| [#1101](https://github.com/gyldlab/keld/issues/1101) | Error identifier and logical carrier governance | [#1438](https://github.com/gyldlab/keld/pull/1438) · [spec](https://github.com/gyldlab/keld/blob/de8925b1ff7418964709ff1737dd968481d42071/docs/specs/gh1101-error-contract-governance.md) |
| [#1102](https://github.com/gyldlab/keld/issues/1102) | Message types, payload ownership and request outcomes | [#1438](https://github.com/gyldlab/keld/pull/1438) · [spec](https://github.com/gyldlab/keld/blob/de8925b1ff7418964709ff1737dd968481d42071/docs/specs/gh1102-message-outcome-governance.md) |
| [#1104](https://github.com/gyldlab/keld/issues/1104) | Evidence-based contract tooling and drift enforcement | [#1438](https://github.com/gyldlab/keld/pull/1438) · [spec](https://github.com/gyldlab/keld/blob/de8925b1ff7418964709ff1737dd968481d42071/docs/specs/gh1104-contract-tooling-evaluation.md) |
| [#557](https://github.com/gyldlab/keld/issues/557) | Generation-bound Tray and ordered popup menu lifecycle | [#1439](https://github.com/gyldlab/keld/pull/1439) · [spec](https://github.com/gyldlab/keld/blob/d22aac45262956a033d8f8a9c7deef3e57ff4c98/docs/specs/gh557-tray-popup-lifecycle.md) |
| [#558](https://github.com/gyldlab/keld/issues/558) | Screen selection, display events and native theme extensions | [#1439](https://github.com/gyldlab/keld/pull/1439) · [spec](https://github.com/gyldlab/keld/blob/d22aac45262956a033d8f8a9c7deef3e57ff4c98/docs/specs/gh558-screen-theme-extensions.md) |
| [#559](https://github.com/gyldlab/keld/issues/559) | Guarded clipboard promises and synchronous PNG NativeImage values | [#1439](https://github.com/gyldlab/keld/pull/1439) · [spec](https://github.com/gyldlab/keld/blob/d22aac45262956a033d8f8a9c7deef3e57ff4c98/docs/specs/gh559-clipboard-nativeimage.md) |
| [#561](https://github.com/gyldlab/keld/issues/561) | Synchronous file dialogs and native error and trust-dialog extensions | [#1439](https://github.com/gyldlab/keld/pull/1439) · [spec](https://github.com/gyldlab/keld/blob/d22aac45262956a033d8f8a9c7deef3e57ff4c98/docs/specs/gh561-dialog-extensions.md) |
| [#1032](https://github.com/gyldlab/keld/issues/1032) | Bounded live Bun output in the dev terminal | [#1440](https://github.com/gyldlab/keld/pull/1440) · [spec](https://github.com/gyldlab/keld/blob/6fe5921a7c722a7951e827acd4b2b3bf3affacfc/docs/specs/gh1032-dev-live-bun-output.md) |
| [#1097](https://github.com/gyldlab/keld/issues/1097) | Bounded renderer request liveness and fair admission | [#1440](https://github.com/gyldlab/keld/pull/1440) · [spec](https://github.com/gyldlab/keld/blob/6fe5921a7c722a7951e827acd4b2b3bf3affacfc/docs/specs/gh1097-renderer-request-liveness.md) |
| [#1098](https://github.com/gyldlab/keld/issues/1098) | Trusted-page notification after Bun restart readiness | [#1440](https://github.com/gyldlab/keld/pull/1440) · [spec](https://github.com/gyldlab/keld/blob/6fe5921a7c722a7951e827acd4b2b3bf3affacfc/docs/specs/gh1098-trusted-page-restart-ready.md) |
| [#1103](https://github.com/gyldlab/keld/issues/1103) | Public lifecycle and shipped-binary exit contracts | [#1440](https://github.com/gyldlab/keld/pull/1440) · [spec](https://github.com/gyldlab/keld/blob/6fe5921a7c722a7951e827acd4b2b3bf3affacfc/docs/specs/gh1103-lifecycle-exit-contracts.md) |
| [#503](https://github.com/gyldlab/keld/issues/503) | Corpus-demanded native-addon policy records | [#1440](https://github.com/gyldlab/keld/pull/1440) · [spec](https://github.com/gyldlab/keld/blob/6fe5921a7c722a7951e827acd4b2b3bf3affacfc/docs/specs/gh503-corpus-addon-policy-record.md) |
| [#571](https://github.com/gyldlab/keld/issues/571) | Host-sourced renderer process facts for Zettlr | [#1440](https://github.com/gyldlab/keld/pull/1440) · [spec](https://github.com/gyldlab/keld/blob/6fe5921a7c722a7951e827acd4b2b3bf3affacfc/docs/specs/gh571-renderer-process-facts.md) |
| [#471](https://github.com/gyldlab/keld/issues/471) | Host-owned custom schemes and guarded Bun response production | [#1441](https://github.com/gyldlab/keld/pull/1441) · [spec](https://github.com/gyldlab/keld/blob/8339b5c92facf16b0d45a86763fe49d969e8debc/docs/specs/gh471-guarded-custom-protocol.md) |
| [#472](https://github.com/gyldlab/keld/issues/472) | Session permission handlers that only narrow Host authority | [#1441](https://github.com/gyldlab/keld/pull/1441) · [spec](https://github.com/gyldlab/keld/blob/8339b5c92facf16b0d45a86763fe49d969e8debc/docs/specs/gh472-narrowing-permission-handlers.md) |
| [#553](https://github.com/gyldlab/keld/issues/553) | Containment-qualified WebRequest registration facade | [#1441](https://github.com/gyldlab/keld/pull/1441) · [spec](https://github.com/gyldlab/keld/blob/8339b5c92facf16b0d45a86763fe49d969e8debc/docs/specs/gh553-contained-webrequest-facade.md) |
| [#525](https://github.com/gyldlab/keld/issues/525) | Persistent Strict role-private user data and logs | [#1442](https://github.com/gyldlab/keld/pull/1442) · [spec](https://github.com/gyldlab/keld/blob/ab677a337d1a2d500b3af394e33cf045d184c6af/docs/specs/gh525-strict-persistent-role-storage.md) |
| [#530](https://github.com/gyldlab/keld/issues/530) | Windows and Linux window chrome and hidden readiness | [#1442](https://github.com/gyldlab/keld/pull/1442) · [spec](https://github.com/gyldlab/keld/blob/ab677a337d1a2d500b3af394e33cf045d184c6af/docs/specs/gh530-portable-window-chrome-readiness.md) |
| [#539](https://github.com/gyldlab/keld/issues/539) | Windows and Linux popup interception and child ownership | [#1442](https://github.com/gyldlab/keld/pull/1442) · [spec](https://github.com/gyldlab/keld/blob/ab677a337d1a2d500b3af394e33cf045d184c6af/docs/specs/gh539-portable-popup-contract.md) |
| [#492](https://github.com/gyldlab/keld/issues/492) | complete build-config disposition without executable evaluation | [#1443](https://github.com/gyldlab/keld/pull/1443) · [spec](https://github.com/gyldlab/keld/blob/e9622e0452e0c86a264897463f5912f89f5f68f8/docs/specs/gh492-build-config-dispositions.md) |
| [#581](https://github.com/gyldlab/keld/issues/581) | Host-owned update triggers and bounded information events | [#1443](https://github.com/gyldlab/keld/pull/1443) · [spec](https://github.com/gyldlab/keld/blob/e9622e0452e0c86a264897463f5912f89f5f68f8/docs/specs/gh581-host-update-trigger-channel.md) |
| [#582](https://github.com/gyldlab/keld/issues/582) | Electron-core autoUpdater with platform-specific source oracles | [#1443](https://github.com/gyldlab/keld/pull/1443) · [spec](https://github.com/gyldlab/keld/blob/e9622e0452e0c86a264897463f5912f89f5f68f8/docs/specs/gh582-core-autoupdater-facade.md) |
| [#583](https://github.com/gyldlab/keld/issues/583) | source-qualified Electron-to-Keld bridge release recipe | [#1443](https://github.com/gyldlab/keld/pull/1443) · [spec](https://github.com/gyldlab/keld/blob/e9622e0452e0c86a264897463f5912f89f5f68f8/docs/specs/gh583-electron-to-keld-bridge-recipe.md) |
| [#494](https://github.com/gyldlab/keld/issues/494) | Two-arm Electron differential oracle harness | [#1444](https://github.com/gyldlab/keld/pull/1444) · [spec](https://github.com/gyldlab/keld/blob/3b5b22b1bcc99636c1e4d7bf87355ceb3fe52e95/docs/specs/gh494-two-arm-oracle-harness.md) |
| [#495](https://github.com/gyldlab/keld/issues/495) | Lifecycle recorder and offline differential replay | [#1444](https://github.com/gyldlab/keld/pull/1444) · [spec](https://github.com/gyldlab/keld/blob/3b5b22b1bcc99636c1e4d7bf87355ceb3fe52e95/docs/specs/gh495-lifecycle-recorder-replay.md) |
| [#537](https://github.com/gyldlab/keld/issues/537) | WebContents navigation and popup conformance evidence | [#1444](https://github.com/gyldlab/keld/pull/1444) · [spec](https://github.com/gyldlab/keld/blob/3b5b22b1bcc99636c1e4d7bf87355ceb3fe52e95/docs/specs/gh537-webcontents-conformance.md) |
| [#554](https://github.com/gyldlab/keld/issues/554) | Session, protocol and online-state conformance evidence | [#1444](https://github.com/gyldlab/keld/pull/1444) · [spec](https://github.com/gyldlab/keld/blob/3b5b22b1bcc99636c1e4d7bf87355ceb3fe52e95/docs/specs/gh554-session-protocol-net-conformance.md) |
| [#585](https://github.com/gyldlab/keld/issues/585) | Parked Electron oracle drift lane | [#1444](https://github.com/gyldlab/keld/pull/1444) · [spec](https://github.com/gyldlab/keld/blob/3b5b22b1bcc99636c1e4d7bf87355ceb3fe52e95/docs/specs/gh585-scheduled-oracle-drift.md) |
| [#663](https://github.com/gyldlab/keld/issues/663) | Harness-run receipt and conformance record producer | [#1444](https://github.com/gyldlab/keld/pull/1444) · [spec](https://github.com/gyldlab/keld/blob/3b5b22b1bcc99636c1e4d7bf87355ceb3fe52e95/docs/specs/gh663-harness-run-receipts.md) |
| [#1162](https://github.com/gyldlab/keld/issues/1162) | Intended Bun case identity and runner provenance | [#1444](https://github.com/gyldlab/keld/pull/1444) · [spec](https://github.com/gyldlab/keld/blob/3b5b22b1bcc99636c1e4d7bf87355ceb3fe52e95/docs/specs/gh1162-runner-evidence-provenance.md) |

## How to dispatch implementation

1. Read the feature issue, its current trusted claim, the exact specification and later amendments. Related specifications share a PR but keep separate files and feature owners.
2. Verify the applicable content approval and section 10 prerequisites. Select the first unblocked vertical task from section 6. A published draft does not authorize implementation.
3. Use each criterion and its independent negative control. Keep caller settlement separate from committed effects, physical retirement and native resource release.
4. Publish actual per-OS/original-application evidence under the existing collector and owner. A green documentation check cannot fill a native acceptance cell.

## Repository-wide discovery disposition

The fresh membership capture at **2026-11-06 11:37:03 UTC** contains **397 unique open issues and 208 unique open PR heads**. All four issue pages reported `incomplete_results=false`; the PR capture used three complete pages. Main is unchanged from the earlier capture.
The issue table gives every captured issue an explicit discovery disposition and next action. Its entries are triage routes, not a statement that every issue needs a new specification or that every existing specification is complete. Later publication rows can update disposition without changing the captured membership.

**Publication-only reconciliation after the census:** [draft #1444](https://github.com/gyldlab/keld/pull/1444) adds seven complete read-back conformance/evidence drafts at immutable source `3b5b22b1bcc99636c1e4d7bf87355ceb3fe52e95`. These seven source-ledger rows are supplemental to, **not** members of, the 208-PR capture at 11:37 UTC. The 397 captured issue IDs are unchanged; this update is **not a fresh complete issue/PR census**. Independent exact-head semantic re-review is in progress; section 10 owner decisions and native acceptance remain open. The GH585 schedule remains parked and disabled.

**Later publication-only correction:** [draft #1442](https://github.com/gyldlab/keld/pull/1442) advanced from its captured head `f020ad12a4a944ef530273afdd2e6f10a6fa433b` to verified head `ab677a337d1a2d500b3af394e33cf045d184c6af`. All three current publication pointers now resolve at the revised head. GH525 corrected the same-identity persistent-storage admission fallback; GH530 and GH539 retain their prior file blobs. The source ledger preserves the original three historical PR-head rows and appends three explicitly later-head file observations. This is **zero new feature specifications and zero new distinct PRs**, not a new issue/PR census; the historical 397-issue / 208-PR denominator is unchanged.

| Discovery disposition | Issues |
|---|---:|
| `catalog-or-implementation-task-review` | 5 |
| `conformance-owner-evidence-review` | 11 |
| `existing-named-spec-review-owner` | 36 |
| `inventory-owner` | 1 |
| `issue-cited-existing-contract` | 13 |
| `maintenance-or-validation-review-existing-contract` | 183 |
| `parked-activation-gate` | 2 |
| `parked-owner-decision` | 2 |
| `parked-source-review` | 8 |
| `program-route-to-children` | 21 |
| `published-feature-draft` | 77 |
| `recorded-evidence-limit` | 1 |
| `shared-owner-contract-route` | 13 |
| `source-or-owner-review-needed` | 23 |
| `tracker-conflict` | 1 |

### Remaining decisions and evidence

- [GH546](https://github.com/gyldlab/keld/issues/546) retains conflicting GitHub/KEL80 tracker text. Its orchestrator must identify the authoritative claim location before another writer takes the credit-window scope.
- [GH562](https://github.com/gyldlab/keld/issues/562) and [GH595](https://github.com/gyldlab/keld/issues/595) retain the recorded strict-proof trigger and KEL102 persistence decision. Existing dialog/OS-intent facts do not authorize arbitrary runtime path grants.
- [GH1193](https://github.com/gyldlab/keld/issues/1193) and [GH1194](https://github.com/gyldlab/keld/issues/1194) retain explicit post-MVP owner activation. This inventory starts neither the adapter rewrite nor a performance campaign.
- Rows marked `source-or-owner-review-needed` or `conformance-owner-evidence-review` identify the exact issue requiring a further source/owner decision. They are not silently counted as completed specifications.

## Reproduce and interpret the ledgers

- [Issue census and dispositions](future-implementation-issues.tsv): one row per captured open issue, evidence basis, known scheduling, related PR mentions and the next action or specific blocker. A PR mention is a discovery link; it does not establish coverage or ownership.
- [Immutable spec evidence and PR coverage](future-implementation-specs.tsv): all 43 main Markdown spec paths plus changed spec occurrences at the captured PR heads, and an explicit row for every PR with no changed spec Markdown. Supplementary publications after capture are identifiable by their later PR number and source head. A repeated PR number at a later head is a revision receipt, not another PR or another distinct feature.
- Existing immutable path-presence receipts are reused only when both source SHA and path are unchanged. New/changed publication files are read completely. A spec inherited unchanged from a base can fall outside a PR's changed paths.
- PR1325 replaces the flat KEL142 spec with a directory. Its removed flat path remains explicitly absent at that head; it is not counted as a present file. Duplicate paths across stacked PRs are occurrences, not extra independent feature specifications.
- The old 397-issue / 179-PR capture remains in Git history. Refresh membership, exact heads, changed filenames and path presence together for the next census; do not relabel a publication-only update as a new full scan.

No runtime source, generated policy, architecture, schema, baseline, seal or instruction changed in this inventory. Local product format/Clippy/workspace tests, native hooks and machine session-closeout did not run in this GitHub documentation lane. No compatibility percentage or performance result is inferred.
