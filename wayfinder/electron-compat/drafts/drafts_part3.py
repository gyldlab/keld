import json
exec(open('drafts_part1.py').read().split('# ---------------- F01')[0])  # helpers only
DRAFTS={}
# ---------------- F08 diagnostics ----------------
unit('F08','compat(diagnostics): process-gone events, crashReporter local-state facade, app metrics census and Bun-side process stats (contentTracing never)','Tier 2',
"""## Scope
12 entities / 63 members: crashReporter (8), contentTracing (5), app metrics/GPU structures, process memory/CPU methods. Corpus demand for the module surfaces is zero; adjacent demand is real: draw.io listens to `render-process-gone` and installs `uncaughtException → app.exit(1)` in export mode; Zettlr handles `unresponsive` → destroy and logs `unhandledRejection`; both call `disableHardwareAcceleration()` before ready.

## Maturity ladder
- L0: every member exists; contentTracing rejects with the arch 07 structured compat error; never rows published.
- L1: `render-process-gone` from engine web-content-process termination (WKWebView webContentProcessDidTerminate; WebView2 ProcessFailed beside the existing BrowserProcessExited observation; WebKitGTK web-process-terminated); Bun-side `process.getHeapStatistics/getCPUUsage/getProcessMemoryInfo` with documented ▲ fields.
- L2: `getAppMetrics` as the arch 01 §5.1 census (host = Browser row; guardian; Bun primary = Utility `keld.primary`; named roles) with KiB units; crashReporter local-state facade at BEHAVIOR_MATCH.

## Linear owners consumed
KEL-105/KEL-116 (crash ledger), KEL-90/KEL-129 (census rules), KEL-75 (generations), KEL-143, KEL-78, KEL-132 (GpuSafeMode).

## Design facts fixed by research (two refuters)
- Bun-primary death is never mapped to any Electron event; a host-emitted process-gone family over the app link feeds `render-process-gone` and `child-process-gone` facades with reason fidelity recorded per engine.
- crashReporter is a local-state facade (start/add/remove/get/setUploadToServer; getLastCrashReport null; getUploadedReports []); keys ≥ 40 bytes ignored with a warning, values truncated (documented); no uploader, no minidumps.
- `getAppMetrics` consumes the §5.1 census (KiB, `>> 10`); percentCPUUsage normalised by CPU count with a per-process last sample.
- contentTracing: never, with Promise rejections for the Promise-returning methods; GPU getters are a research atom (Linux GpuSafeMode yields at most one derived fact).
""",
[
 T('F08-T1','feat(diagnostics): host-emitted process-gone events → render-process-gone/child-process-gone facades with per-engine reason fidelity','feature','Tier 2','L1',
   'Wire engine web-content-process termination hooks per backend and the role-generation ledger into one host-emitted event family over the app link; facades map them to `webContents`/`app` `render-process-gone` (reason per engine, documented ▲ where the engine gives none) and `child-process-gone` for named roles; Bun-primary death is never mapped.',
   ['Killing the WebContent process on macOS emits `render-process-gone` with a reason and the window stays','A named role crash emits `child-process-gone` with the role name; the primary\'s death emits nothing Electron-shaped','WebView2 lane reuses the existing browser-process-exited observation and adds per-webview ProcessFailed'],
   ['Mapping primary death to render-process-gone fails the never entry'],
   ext=['KEL-143','KEL-75 generations'],owner='KEL-143 / KEL-75',gates=['wire protocol'],members=['render-process-gone','child-process-gone','RenderProcessGoneDetails','unresponsive','responsive'],size='M',ready='needs-spec',
   current='Crash ledger exists; no engine process-gone hooks beyond WebView2 browser-process exit.',interfaces='keld-wv per-backend termination hooks; app-link event family; facades.',oos='Automatic recovery policy (KEL-143).'),
 T('F08-T2','task(diagnostics): crashReporter local-state facade, Bun-side process stats, getAppMetrics census, contentTracing never rows','task','Tier 2','L2',
   'Implement the crashReporter local-state facade with documented limits; `process.getHeapStatistics/getCPUUsage/getProcessMemoryInfo/getSystemMemoryInfo` from Bun primitives (KiB, camelCase, documented ▲ for underivable fields); `app.getAppMetrics` as a cold host Call returning the §5.1 census; `contentTracing.*` rejects with the structured compat error; `getGPUInfo/getGPUFeatureStatus` tracked per F08-D6.',
   ['`crashReporter.start({submitURL})` then `getParameters()` round-trips; a 40-byte key is ignored with a warning','`getAppMetrics()` rows name host/guardian/primary/roles with KiB memory and normalised CPU','`contentTracing.startRecording()` rejects with the compat error code and tracking issue'],
   ['Uploading anything from crashReporter fails the default-deny negative test'],
   blocked=['F08-T1'],ext=['KEL-129 census'],owner='KEL-129',gates=['none'],members=['crashReporter.start','crashReporter.addExtraParameter','crashReporter.removeExtraParameter','crashReporter.getParameters','crashReporter.getLastCrashReport','crashReporter.getUploadedReports','crashReporter.getUploadToServer','crashReporter.setUploadToServer','process.getHeapStatistics','process.getCPUUsage','process.getProcessMemoryInfo','process.getSystemMemoryInfo','app.getAppMetrics','contentTracing'],size='M',ready='ready-for-agent',
   current='None exist.',interfaces='Facade modules; host census Call.',oos='GPU getters (F08-D6).'),
],
[D('F08-D6','What can getGPUFeatureStatus/getGPUInfo/disableHardwareAcceleration honestly report per engine?','research','Linux GpuSafeMode yields at most one derived fact (DMA-BUF renderer disabled when degraded); macOS WKWebView exposes no public GPU-feature API; WebView2 could be launched with host-owned arguments. GPUFeatureStatus has no unknown value. Settle per engine with receipts; the post-ready throw text must match Electron.')],
['contentTracing in production (and in dev)','automatic crash upload honoring uploadToServer:true','minidump/core capture shipped to app code','mapping Bun-primary death to render-process-gone','a second crash ledger or process census'],
[],
['Diagnostic shape of an unhealthy role when a close-veto reply never arrives (panel open)'])

# ---------------- F09 update packaging ----------------
unit('F09','compat(update): autoUpdater and electron-updater facades over the host-owned KEL-53 updater, keld.build.ts translation, bridge release recipe','Tier 2',
"""## Scope
`autoUpdater` (10 members, macOS+Windows) plus the ecosystem reality: draw.io uses electron-updater v6 (setFeedURL provider github, autoDownload=false, downloadUpdate, download-progress, quitAndInstall) with five electron-builder configs and @electron/fuses; Zettlr ships its own downloader. Live today: keld-update verifies signed v0 manifests/full packages, Windows extraction/staging/journaled activation and startup repair; keld-pack streams Windows v0 packages; `keld build`/`migrate` reserved; no update channel to Bun; no keld.build.ts schema.

## Maturity ladder
- L0: both facades exist; feed-URL writes are recorded no-ops ▲; `checkForUpdates` emits a typed error only when invoked without a host updater.
- L1: KEL-53 state machine events reach Bun (checking/available/not-available/progress/downloaded/error) with versions/dates/sizes, never paths.
- L2: `quitAndInstall` ordering split by facade × platform passes pinned entries; bridge release recipe validated on Windows NSIS (publisher match) and macOS.

## Linear owners consumed
KEL-53 (host-owned updater: full-package activation first), KEL-254/KEL-270 (Windows install modes/journal), KEL-19/KEL-15 (keld.build.ts schema — RFC text missing), KEL-103/KEL-141/KEL-137 (macOS package cells), KEL-98 (contract source for update.* calls).

## Design facts fixed by research (two refuters + panel)
- Two thin Bun facades over one host state machine; electron-updater rewritten by migrate to a Keld shim exporting the AppUpdater subset draw.io exercises; pin 6.8.9.
- `setFeedURL` is accepted and recorded (never thrown — draw.io routes throws to a user-facing Update Error dialog); mismatch vs `keld.build.ts` feed → dev diagnostic; the host never reads it; `DRAWIO_DISABLE_UPDATE=true` written into the host-declared role environment by migrate; never a fabricated `update-not-available`.
- `update-downloaded` = staged-and-verified (not published); `quitAndInstall` → host activation with health check and rollback; ordering atoms split per facade × platform (macOS native inverts before-quit).
- Authority: all network/verify/stage/activate/relaunch in the host; Bun gets `update.check/download/quitAndInstall` guarded calls (new guard operations need KEL-102 admission; contract source is open).
- Bridge release: Windows NSIS requires the Keld installer be signed by the same publisher (publisherName verification is skipped when absent from app-update.yml); macOS zip/latest-mac.yml path; Linux AppImage/deb per electron-updater ≥ 6.x sentinel.
""",
[
 T('F09-T1','feat(update): autoUpdater core facade + electron-updater shim over the KEL-53 state machine with recorded no-op setFeedURL and info-only events','feature','Tier 2','L1',
   'Expose `update.check/download/quitAndInstall` guarded calls and an info-only host→Bun event stream; implement the 10-member `autoUpdater` and the electron-updater AppUpdater subset draw.io uses; setFeedURL recorded (▲) and compared with `keld.build.ts`; typed error on explicit check without a host updater; `update-downloaded` only after stage+verify; `quitAndInstall` ordering split per facade × platform with conformance entries.',
   ['draw.io boots with its updater code unmodified, `setFeedURL` recorded, no boot-time error dialog','`checkForUpdates()` against a declared host feed walks checking → available/not-available with versions and sizes; no filesystem path reaches Bun','`quitAndInstall()` triggers host activation; a failed health check rolls back and the facade reports `error`'],
   ['Throwing from setFeedURL triggers draw.io\'s Update Error dialog — negative test must fail the activation entry'],
   ext=['KEL-53 activation + feed base owner (F09-A2)','KEL-102 new guard operations','contract source (open)'],blocked=['F09-A2'],owner='KEL-53',gates=['permission model','wire protocol','public API'],members=['autoUpdater.setFeedURL','autoUpdater.getFeedURL','autoUpdater.checkForUpdates','autoUpdater.quitAndInstall','error','checking-for-update','update-available','update-not-available','update-downloaded','before-quit-for-update'],size='L',ready='needs-spec',
   current='No update channel to Bun; no facade.',interfaces='keld-update state machine; guarded update.* calls; facades.',oos='Delta transport; bridge recipe (F09-T2).'),
 T('F09-T2','task(update): keld.build.ts translation of electron-builder/forge configs and the bridge-release recipe per platform','task','Tier 2','L0',
   'Specify `keld.build.ts` (KEL-15/KEL-19 schema; RFC text to be located — F09-A6) and the per-field disposition report migrate emits for electron-builder JSON/YAML and forge configs (appId → ExpectedAppIdentity, files/asar → canonical tree, targets, signing/notarize, fuses dropped with explanation incl. wasmTrapHandlers); document the bridge release recipe: final Electron release whose electron-updater feed points at a Keld installer signed by the same publisher (Windows NSIS publisherName rule), macOS latest-mac.yml, Linux AppImage sentinel.',
   ['draw.io\'s five electron-builder configs translate with every field dispositioned (mapped/▲/✘)','The bridge recipe names the exact client constraints per platform with receipts','`keld build` remains reserved; no execution of app build hooks'],
   ['Executing an afterPack hook during translation fails the never entry'],
   blocked=['F09-A6'],ext=['KEL-15/KEL-19','KEL-137 package representation'],owner='KEL-19',gates=['none'],members=['keld.build.ts','electron-builder','electron-forge','@electron/fuses'],size='M',ready='needs-spec',
   current='No keld.build.ts schema exists.',interfaces='migrate translation report; packaging policy docs.',oos='Implementing keld build.'),
],
[
 D('F09-A2','Who owns the compiled-in feed base (KEL-53/KEL-19 amendment) that autoUpdater.getFeedURL reports and setFeedURL is compared against?','grilling','The host feed identity is compiled into the signed host (ExpectedAppIdentity) but no owner names the feed base field; `keld.build.ts` has no schema. Name the owner and the field before any facade contract; record the ▲ per platform (Windows: one `error` event "Update URL is not set"; macOS: no event — inference needing one Electron run).'),
 D('F09-A6','Locate the accepted KEL-15 configuration RFC text (repo has no spec file) and land it as docs/specs before keld.build.ts work','research','Linear marks KEL-15 Done but the repository holds no RFC document; the keld.build.ts/keld.config.ts schema work cannot start from prose. Ask the KEL-15 owner for the accepted text and land it.'),
],
['app-selected feed URLs/headers/servers reaching the privileged updater','allowDowngrade / allowAnyVersion under the monotonic floor','app-executed installers or exposure of staged artifacts','@electron/fuses as a runtime API','ASAR virtual filesystem','executing electron-builder/forge JS hooks','running Electron autoupdater specs under Bun'],
[],
['Zettlr\'s got-based self-updater (shell.openPath of an installer + app.quit) → migrate fix-it wording'])

# ---------------- X01 conformance oracle harness ----------------
unit('X01','program(conformance): differential oracle harness — pinned Electron recorder, Keld replayer, comparator, canonical bytes, CI lanes, maturity ladder','Tier 1',
"""## Scope
The measurement machine behind every compat claim: Arm A records Keld-owned fixture apps under the pinned Electron 44.4.5 binary (npm dist shasum recorded; tag → commit 694f45852a0f1726cd23bfd379854de489cccb65) into line-oriented transcripts; Arm B replays the same unmodified fixtures under `@keld/electron`; a Rust comparator in keld-compat normalises declared nondeterministic fields, diffs ordered sequences, and emits one KEL-74 evidence record per cell. The pinned doc sentence is the oracle; the golden transcript is evidence-of-record (observed-only cells cap at BEHAVIOR_MATCH, recorded `unknown` with the oracle id naming the unspecified path).

## Maturity ladder (this is the ladder every family uses)
BEHAVIOR_MATCH (Keld transcript == golden under normalisers, ≥1 OS, legacy_sandbox_off) → CONFORMANCE_PASS (cited doc sentence + Keld-owned test independent of the diff + named negative control + all three OS lanes or explicit ▲) → CORPUS_VERIFIED (same observable matched in a real corpus app through migrate + dev smoke, panel product once a product corpus id is committed, authority profile printed).

## Linear owners consumed
KEL-74 (schema/scorer; shared Manifest/exact-bytes helpers must be promoted out of test-local code), KEL-237 (lifecycle corpus; canonical-bytes rule already live there), KEL-77 (two-arm harness shape; fixture-set digest algorithm §4.3), KEL-81 (CI routing), KEL-78 (authority profile truth), product-corpus owner (none today — DOCUMENTED_COMMITTED_PRODUCT list is empty).

## Design facts fixed by research (two refuters)
- Fixture family: `main.js` (ESM, type:module) + `preload.cjs` (CJS) + `index.html`, markers via synchronous stdout writes; the Electron binary is never committed and never runs in the PR lane.
- Canonical bytes: manifest exact committed bytes (sha256 asserted with a one-byte mutation control — live in the lifecycle corpus test); fixture-set digest per KEL-77 §4.3; each golden is its own immutable blob; the spec must state which digest `artifact.sha256` carries.
- Lanes: PR lane replays committed goldens inside the keld-compat test target on the existing three-OS matrix (offline, deterministic); a weekly recorder lane (TARGET: no scheduled workflow exists) re-records at the same pin to detect oracle nondeterminism and needs its own failure sink or "regression = P1" is unenforced.
- Comparison is strict within one process's sequence space plus explicit causal edges; timing never compared in conformance cells.
""",
[
 T('X01-T1','spec(conformance): differential oracle harness spec — recorder, replayer, comparator, canonical bytes, normalisers, lanes, ladder, authority-profile truth','spec','Tier 1','L0',
   'Write the approved spec (docs/agents/spec-template.md) for the two-arm harness: fixture packaging, Electron pin and digest fields, transcript row shape, per-cell normalisers, strict ordered comparison with causal edges, observed-only tagging, lanes (PR replay; weekly recorder with a failure sink), the maturity ladder and panel/authority-profile rules; promote the manifest/exact-bytes helpers out of test-local code into keld-compat.',
   ['Spec approved by a human with every section filled; rejected alternatives recorded (Electron spec/ tree; timing comparison; re-recording from the Keld arm)','Canonical-bytes and digest fields are unambiguous (which digest `artifact.sha256` carries)','Observed-only cells cannot exceed BEHAVIOR_MATCH by construction'],
   ['A spec that lets a golden re-record from the Keld arm is rejected at review'],
   ext=['KEL-74','KEL-237','KEL-77 §4.3'],owner='KEL-74 / KEL-237',gates=['none'],members=[],size='M',ready='ready-for-agent',
   current='KEL-74 scorer and a 3-cell lifecycle corpus exist; no recorder/replayer/comparator.',interfaces='keld-compat evidence/denominator/scorer; lifecycle corpus runner pattern.',oos='Running any Electron binary in CI.'),
 T('X01-T2','feat(conformance): recorder + replayer + comparator for the app-lifecycle family (first end-to-end cells) with the committed goldens','feature','Tier 1','L2',
   'Implement the harness for the F01 fixtures first: record goldens under electron@44.4.5 on macOS (weekly lane or developer command), commit goldens + manifest with exact-bytes digests, replay under `@keld/electron` in the keld-compat test target, emit KEL-74 records, and prove the one-byte mutation and reordering negative controls.',
   ['`cargo test -p keld-compat` replays the committed lifecycle goldens offline on all three OS lanes','A reordered golden line fails the comparator; a one-byte manifest change fails the digest assertion','Records carry authority_profile legacy_sandbox_off and the engine identity'],
   ['Removing the normaliser declaration makes a pid-bearing row flake — the design test must fail deterministically instead'],
   blocked=['X01-T1','F01-T1'],owner='KEL-237',gates=['dependency'],members=[],size='L',ready='needs-spec',
   current='None.',interfaces='keld-compat harness crate module; fixtures directory; corpus manifest.',oos='Corpus apps (X02-T4).'),
],
[D('X01-A5','Oracle nondeterminism: do repeated Electron 44.4.5 runs of the quit-order fixture produce identical transcripts, and which normalisers are needed?','prototype','Run the quit-order fixture under `npx electron@44.4.5` ten times on macOS and diff transcripts; derive the normaliser set (pid, window id, paths, Error.stack) and the strict-ordering rule within a process plus causal edges. No CI or Keld change.')],
['running or translating Electron spec/ under Bun','fetching electron latest/main','committing the Electron binary','re-recording goldens from the Keld arm','retries/reruns/todoIf(isCI)','percentages without a committed denominator','labelling the harness strict_bun'],
[],
['Weekly recorder lane home and failure sink (no scheduled workflow exists today)','Product corpus id ownership (DOCUMENTED_COMMITTED_PRODUCT list is empty; GH #313 chose draw.io but nobody owns adding it)'])

# ---------------- X02 migrate tooling ----------------
unit('X02','program(migrate): read-only analyzer, module alias, five-file generation, honest report without percentages, corpus harness manifest','Tier 1',
"""## Scope
`keld migrate` (reserved today), the module alias, the five generated files, the report format and the corpus manifest for real apps. Live: keld-guard manifest parser (64 KiB, unique keys, deny fix-text that prints the exact patch), KEL-74 scorer, KEL-237 lifecycle corpus runner pattern, KEL-215 addon evidence rows, the scratchpad demand scan (prototype of the analyzer).

## Design facts fixed by research (two refuters + panel)
- Analyzer is a read-only Rust verb: lexical, import-binding-aware scan of JS/TS/Vue plus static parsing of package.json/tsconfig/bunfig/electron-builder/forge configs; never evaluates app code; reports KEL-127's four blocker classes separately; emits reversible changes only after authorization (`--write`).
- The report prints static coverage COUNTS by status (✔/▲/✘/unclassified) labelled "static estimate, not a compatibility score"; a KEL-74 claim line appears only for committed corpus members. Arch 04 §1's sample `compat score: 91%` contradicts §4/KEL-74 and must be fixed in the same PR (also arch 07 §4's `keld_migrate_analyze` row).
- Module alias: a package.json dependency whose key is `electron` (npm: alias once published; file:/link: in dev) so every resolver sees `node_modules/electron` as the shim; tsconfig paths is editor/type mapping only (the refuter re-ran the probe: paths does NOT reach a node_modules dependency's require); the `bunfig.toml [alias]` wording in arch 04 §2 is a spec defect. Open: X03 proposes a Bun preload plugin for the five subpath specifiers — decide one owner (X02-A1).
- `--write` seeds `keld.permissions.jsonc` with literal-only grants through the live vocabulary; `windows.<w>.channels` has no live evaluator (blocker shared with F04-A1); no recorder exists.
- Boot facts are host-published at spawn (panel consensus); blocking pre-ready CALLs wait on PANEL-P1.
- Corpus harness: a new manifest (`electron-apps-v0`, panel product) reusing KEL-237's schema and runner pattern, pinned by upstream commit + canonical bytes; analyzer counts are leads attached to rows, never cells.
""",
[
 T('X02-T1','spec(migrate): read-only analyzer + report format + five-file generation contract (fixes the arch 04 §1 percentage contradiction in the same PR)','spec','Tier 1','L0',
   'Write the approved spec for `keld migrate`: input discovery, static scan rules, blocker classes (compatibility/security/engine/native-dependency), report shape (counts not percentages; KEL-74 claim line only for committed corpus members; authority profile printed), `--write` outputs (keld.config.ts, keld.permissions.jsonc literal seeding, keld.build.ts, keld.compat.ts, package.json alias + scripts, tsconfig paths), and the same-PR correction of arch 04 §1/§4/§6 and arch 07 §4 wording.',
   ['Spec approved; rejected alternatives recorded (executing app configs; percentage output; wildcard seeding)','Every generated file maps to its owning schema (KEL-15/KEL-19) or names the missing RFC text as a blocker'],
   ['A spec that prints a percentage without a denominator is rejected'],
   ext=['KEL-17 (Done RFC without a spec file)','KEL-15/KEL-19','KEL-127 analyzer-first direction'],owner='KEL-17 / KEL-19',gates=['none'],members=['keld migrate','keld.config.ts','keld.permissions.jsonc','keld.build.ts','keld.compat.ts'],size='M',ready='ready-for-agent',
   current='`keld migrate` is reserved (KELD-CLI-045); the demand scanner is a scratchpad prototype.',interfaces='keld-cli verb; guard manifest vocabulary; KEL-74 claim shape.',oos='Implementation (X02-T2/T3).'),
 T('X02-T2','feat(migrate): analyzer v0 — static Electron usage inventory per call site classified against the compat matrix, security/engine/native blockers listed separately','feature','Tier 1','L1',
   'Implement the read-only analyzer in keld-cli: resolve `electron` import bindings, inventory member usage per call site, classify against the committed matrix status, list weak-isolation webPreferences, dynamic `openExternal` targets, spawn/exec sites, custom scheme privileges, native addons (KEL-215 evidence lookup: qualified/unqualified) and `<webview>`; emit `--json` and human reports with counts only.',
   ['draw.io and Zettlr inventories match the committed demand scan within declared tolerance and list every blocker class','The report prints "static estimate, not a compatibility score" and no percentage','A dynamic channel dispatcher (Zettlr generic window.ipc) is reported as unenumerable with the recorder fix-it'],
   ['Printing a percentage fails the honesty negative test'],
   blocked=['X02-T1'],owner='KEL-17 / KEL-19',gates=['public API'],members=['keld migrate (analyze)','keld_migrate_analyze (MCP)'],size='L',ready='needs-spec',
   current='Reserved verb; no analyzer.',interfaces='keld-cli migrate verb; matrix status source; MCP tool.',oos='Writing files (X02-T3).'),
 T('X02-T3','feat(migrate): --write generation of the five files with literal-only grant seeding and the package.json electron alias; draw.io hand-authored artefact first','feature','Tier 1','L1',
   'Generate keld.config.ts (identity, windows, scheme table, singleInstance), keld.permissions.jsonc (literal grants only: userData/logs roots per F01-A4, literal openExternal origins, enumerated `el:` channels, `app.system` tokens; `el:*` refused), keld.build.ts (translation per F09-T2), keld.compat.ts quirks, package.json (`electron` dependency alias + scripts), tsconfig paths; the first draw.io artefact is hand-authored to the exact shape the generator must emit, then the generator is specified from it.',
   ['`keld migrate --write` on draw.io produces files byte-identical to the approved hand-authored artefact','Every seeded grant is a literal; a wildcard in the input configs is reported, never seeded','electron-store/electron-log/electron-updater resolve `require("electron")` to the shim through the package.json alias on Bun 1.4.2'],
   ['Seeding `$HOME/**` fails the literal-only negative test'],
   blocked=['X02-T2','X02-A1','F01-A4'],owner='KEL-19',gates=['permission model'],members=['keld migrate --write'],size='L',ready='needs-spec',
   current='None.',interfaces='Generators per file; guard vocabulary; alias mechanism.',oos='Recorder/dev-permissive profile (arch 03 §3 v0: none).'),
 T('X02-T4','feat(corpus): electron-apps-v0 product corpus manifest (draw.io first) reusing the KEL-237 schema and runner pattern; install/activation/primary_workflow cells via migrate + dev smoke','feature','Tier 1','L3',
   'Create a new corpus manifest with its own id and `panel: product`, pinned by draw.io\'s upstream commit (with the webapp submodule populated — the shallow clone has it empty) and canonical bytes; cells = install, activation, and the 4-step primary workflow (open → edit → save → close-with-unsaved-prompt); records carry `authority_profile: LegacySandboxOff` until strict is admitted; a product corpus id owner must add it to the documented committed list before any percentage.',
   ['The manifest digest assertion and mutation control pass','draw.io cells run through migrate + `keld dev` smoke on macOS with per-cell pass/fail/unknown and immutable evidence URIs','The scoreboard shows `{passed}/{N} of product corpus electron-apps-v0@<digest> (primary_workflow)` only once the id is documented'],
   ['Marking a cell pass on import success fails the honesty entry'],
   blocked=['X02-T3','X01-T2'],ext=['KEL-237','KEL-74 committed-id list owner (open)'],owner='KEL-237',gates=['none'],members=[],size='L',ready='needs-spec',
   current='Only the 3-cell lifecycle corpus exists.',interfaces='keld-compat corpus runner; KEL-74 records.',oos='Zettlr cells (second app).'),
],
[D('X02-A1','One module-alias owner: package.json `electron` dependency alias (X02) vs a Bun preload plugin for the five Electron specifiers (X03)','grilling','The refuter proved tsconfig paths does not reach a node_modules dependency\'s `require("electron")`; a package.json dependency alias does on Bun 1.4.2. X03 proposes a Bun plugin to also cover `electron/main|renderer|common|utility`. Recommendation: package.json alias as the single runtime resolver (bundler-neutral) with package `exports` for the subpaths; a plugin only if exports cannot express them. Decide and fix the arch 04 §2 bunfig wording.')],
['executing app build/packaging configs','writing files without --write authorization','printing a compatibility percentage','seeding wildcard grants','translating setFeedURL into a feed','carrying nodeIntegration:true','mapping child_process with shell:true','claiming native-addon support without KEL-215-shaped evidence'],
[],
['Recorder/dev-permissive profile (arch 03 §3 destination) as the source of dynamic grants','`keld doctor --web-compat` scope'])

# ---------------- X03 node/bun/addons ----------------
unit('X03','program(runtime): Node/Bun compatibility layer for migrated mains — process facts, module specifiers, native addon policy, child_process broker, ecosystem packages','Tier 1',
"""## Scope
Process-level Electron facts consumed at module evaluation (type, versions, defaultApp, resourcesPath, mas, windowsStore, sandboxed, contextIsolated), the five module specifiers (`electron`, `electron/main|renderer|common|utility`), native addon classes (better-sqlite3, nodehun, node-pty, keytar, sharp, @parcel/watcher; chokidar 5 is pure JS over fs.watch; fsevents transitive), strict vs legacy labelling, child_process replacement, and the ecosystem packages that wrap Electron (electron-store, electron-log, electron-context-menu).

## Design facts from research (NOT yet independently refuted — batch 2; decisions below stay open except where the panel settled them)
- Boot facts are host-minted at spawn and read synchronously at import (settled by the panel).
- Migrated apps start `unverified` (KEL-78 default) and the report prints it; `legacy` is written only when the user accepts the forfeit; every surface prints "legacy — zero-authority claim forfeited" (settled by the panel).
- Native addon policy follows KEL-78's four-outcome tree with a KEL-215-shaped evidence row per (version, OS/arch, Bun revision, artifact digest, operation); no in-process load under strict.
- child_process under strict is a host `process` broker row (KEL-76 owner) with a child_process-shaped facade conforming to KEL-77's five Node oracles; under legacy Bun's node:child_process with documented gaps.
- electron-store/electron-log/electron-context-menu are an aggregate oracle for the median main-process surface (candidate `package` panel).
""",
[
 T('X03-T1','feat(runtime): host-minted boot facts at spawn for process.* and sync app getters (with F01-T4) and the five Electron module specifiers resolved by one owner','feature','Tier 1','L1',
   'Deliver the bootstrap payload beside `KELD_APP_LINK` (role spawn contract) carrying process facts and the path catalogue; `@keld/electron` reads it synchronously at import; resolve `electron`, `electron/main|renderer|common|utility` through the single alias owner decided in X02-A1 (package exports for subpaths); `process.versions.electron` reports the Keld shim version, never a fake Electron release.',
   ['`require("electron/main")` and `import "electron"` resolve to the shim in app code and in node_modules packages','`process.defaultApp/mas/windowsStore` are undefined unless the packaged layout proves them (Electron documents undefined)','A role spawned without the payload fails closed typed'],
   ['Reporting process.versions.electron as a real Electron release fails the honesty entry'],
   blocked=['X02-A1','F01-T4'],ext=['KEL-75 spawn contract','KEL-72'],owner='KEL-75 / KEL-72',gates=['public API'],members=['process.type','process.versions','process.defaultApp','process.resourcesPath','process.mas','process.windowsStore','process.sandboxed','process.contextIsolated','electron/main','electron/renderer','electron/common','electron/utility'],size='M',ready='needs-spec',
   current='Only process.type/versions.electron at import; bare `electron` via tsconfig paths in fixtures.',interfaces='Role bootstrap payload; package exports; facade import-time initialisation.',oos='Alias decision itself (X02-A1).'),
 T('X03-T2','task(runtime): native addon policy ledger per class with KEL-215-shaped evidence rows and strict/legacy outcomes; ecosystem package panel candidates','task','Tier 1','L0',
   'Publish the per-addon outcome table (reuse-in-worker / in-process legacy / broker / reject) with evidence rows: better-sqlite3 (13.0.3 qualified on Bun 1.4.2 win-x64 only), nodehun (node-gyp source build; raw .node import), node-pty (KEL-76), keytar (→ safeStorage), sharp/@parcel/watcher (worker), chokidar 5 (fs.watch differential, KEL-77 family extension), fsevents; inventory electron-store/electron-log/electron-context-menu requirements as a candidate package panel; `keld doctor` reports only version-qualified evidence.',
   ['Every addon in the corpus has a row with outcome, evidence or "unqualified — evidence required"','No row claims support from import success','The package-panel inventory lists each Electron member the three packages need'],
   ['Marking nodehun supported without a load evidence row fails the honesty entry'],
   blocked=[],ext=['KEL-215','KEL-78','KEL-76','KEL-77'],owner='KEL-215 / KEL-78',gates=['none'],members=[],size='M',ready='ready-for-agent',
   current='Two better-sqlite3 rows exist.',interfaces='Scoreboard/doctor evidence rows.',oos='Addon workers (KEL-78 T5–T7).'),
],
[
 D('X03-A5','child_process replacement under strict: host `process` broker row (KEL-76) with a child_process-shaped facade conforming to KEL-77 oracles — scope and owner','research','Both corpus apps spawn executables (draw.io: Windows attrib.exe + one execFile that tolerates failure; Zettlr: pandoc/quick-look). Under strict, raw spawn is never; a host process broker with executable allow-list + argument schema + cwd/env policy is proposed. Confirm KEL-76 ownership and the facade\'s KEL-77 five-oracle conformance; under legacy record Bun gaps.'),
 D('X03-A6','Should electron-store / electron-log / electron-context-menu (unmodified, corpus-pinned) form a `package` panel in the compat corpus?','grilling','They are an aggregate oracle for the median main-process surface (getPath, getVersion, isPackaged, ipcMain + a sync renderer channel in electron-store, shell.openPath, Menu.popup/context-menu). KEL-74 kinds are a closed set; decide whether to record them as cells inside the draw.io product denominator or as a separate showcase panel.'),
],
['loading opaque native addons into the primary Bun role under strict','inferring legacy/strict from Electron sandbox/fuses','raw child_process/exec from strict roles','auto-downgrade to unsandboxed','NODE_OPTIONS/--inspect passthrough into roles','ASAR emulation','Keld-specific native ABI or electron-rebuild','bunfig.toml [alias] as the resolver','fake Electron version strings','per-subpath shim packages with fabricated exports'],
[],
['Bun fs.watch differential (chokidar 5) as a KEL-77 family extension','Zettlr nodehun load status on Bun 1.4.2 per OS'])

# ---------------- X04 security mapping ----------------
unit('X04','program(security): every Electron security surface mapped onto keld-guard, the KEL-142 bridge, principalized roles and the host-owned signed feed — with negative tests','Tier 1',
"""## Scope
Permission request/check handlers, weak-isolation webPreferences, protocol privileges + CSP, shell.openExternal grants, single-instance/open-file intents, autoUpdater/fuses/ASAR trust, device permissions, media, devtools-in-release. Live today: `/app` grants for AppProcess only; a minted webview principal is `KELD-GUARD006` until window-level grants ship (KEL-102/T4); media callbacks evaluate resource `*` (wry exposes no origin); `windows`, `audit`, `roles` manifest sections parse-and-ignore.

## Design facts fixed by research (two refuters + panel)
- One guard decision, two Electron views: `windows.<w>.web.<permission>` evaluated as the webview principal; a missing handler is a deny; handlers only narrow; permission vocabulary is engine-hook-dependent (three columns).
- Weak-isolation webPreferences: privilege-escalating fields fail loudly; `disableBlinkFeatures` and engine flags are ignored with a diagnostic (draw.io sets it on every window); `sandbox:false` ▲ ignored, never a profile key.
- Scheme privileges are host declarations; `bypassCSP` never; `allowServiceWorkers` ▲ per engine; the CSP profile must admit draw.io's `'wasm-unsafe-eval'` and its style/img/connect sources (prototype P2 scope).
- OS-delivered intents (second-instance, open-file) are untrusted data admitted by the host; the file path becomes a host-scoped one-shot intent, not an ambient grant (corrected by the semantics refuter).
- Feed trust is host-only; fuses: draw.io flips RunAsNode=false, EnableCookieEncryption=true, NodeOptions=false, OnlyLoadAppFromAsar=true, EmbeddedAsarIntegrityValidation=false (resolved from its build script); nine fuses incl. wasmTrapHandlers dispositioned.
- Device permissions (HID/USB/serial/Bluetooth): documented-unsupported, not legacy.
""",
[
 T('X04-T1','spec(security): the Electron→keld-guard mapping table with grant shapes (no new schema), the legacy-profile list, and the negative test per mapping (research 186 gaps 1–7)','spec','Tier 1','L0',
   'Write the mapping spec: every Electron security surface → owning Keld mechanism (guard grant shape under arch 03 §2, bridge, roles, signed feed) with its never/▲/legacy classification, the per-engine permission-hook table, the CSP profile shape, and one negative test per mapping (custom-scheme privilege minting, second-instance argv, openExternal matrix, missing handler, IPC sender confusion/error redaction, utilityProcess env/ports, artifact trust chain). Revise the `el:*` example in arch 03 §3 in the same PR.',
   ['Spec approved; every row names its test and its owner','No mapping defaults to allow; no new manifest schema is introduced','The CSP profile admits draw.io\'s declared sources and is linted'],
   ['A row whose negative test cannot fail is rejected'],
   ext=['KEL-102/T4 window grants','KEL-78','KEL-208/KEL-209','KEL-53'],owner='KEL-102 / KEL-78',gates=['permission model'],members=[],size='M',ready='ready-for-agent',
   current='Mapping exists only in research notes.',interfaces='arch 03 §2 grant vocabulary; keld-guard evaluate; migrate security report.',oos='Implementing brokers.'),
],
[
 D('X04-D3','Custom-scheme privilege matrix and CSP profile prototype (P2): can a renderer turn a resource locator into filesystem authority, and does the host CSP admit draw.io?','prototype','Build the falsifiable matrix: privilege declaration timing, standard/secure/CSP/CORS/fetch/service-worker combinations, malformed and encoded traversal URLs, and the CSP profile with draw.io\'s exact directives; also the missing spec for user-granted persisted filesystem roots under a frozen manifest (KEL-79/KEL-130 gap).'),
 D('X04-D4','shell.open grant spelling for scheme classes: KEL-208 owner ruling (bare `https:` is inert today; a distinct confirm-class token vs exact literals)','grilling','The proposed bare-scheme literal rests on a wrong reading of the live matcher. Options: (a) literal origins only + host confirmation flow for everything else; (b) a distinct confirm-class token (`confirm:https`). Needs the KEL-208 owner ruling; the broker canonicalises the URL into two resources evaluated as exact literals either way.'),
],
['remote / enableRemoteModule','nodeIntegration:true / contextIsolation:false / webSecurity:false / allowRunningInsecureContent / experimentalFeatures / enableBlinkFeatures','protocol privileges minted at runtime (bypassCSP)','app-selected updater feed / headers / servers','certificate-error callback(true) in strict','utilityProcess env/cwd/execArgv/stdio-inherit','permission handlers returning true after a guard deny','Electron sandbox:false as a Keld profile selector','full CDP in production'],
[],
['Window-level grants (KEL-102/T4) timing — every webview-principal row depends on it'])

# ---------------- X05 performance ----------------
unit('X05','program(perf): facade cost model, sync-getter mirrors, compat-overhead benchmark lane, channel-id allocation rule and parallelism plan','Tier 1',
"""## Scope
How compatibility and performance meet: every Electron main-process call that is synchronous in Electron and needs host state becomes a facade mirror read (zero traffic), a write-through Call, or a documented ▲; renderer→main hops double versus Electron; the kipc client cannot block today (panel); measurement rules from arch 01 §5/§5.1.

## Measured today (never blended with targets)
Windows host-process RSS 19,552 KB vs Electron 89,140 KB (Electron won total process tree); host-only first paint Keld 469 / Tauri 479 / Electron 275 ms (Electron leads); Windows kipc Bun↔Rust diagnostic p99 ≈ 101 µs (1 session, not publication-eligible); macOS Rust↔Rust library arm p99 ≈ 9.3–9.6 µs; Bun extension-host bootstrap PoC 53% sooner (narrow). Forbidden claims are listed on the map.

## Design facts from research (NOT yet independently refuted — batch 2; X05-A1/A3 settled by the panel)
- Window-state mirror: host events stamped with a monotonic per-window state generation; getters read the mirror synchronously, setters are write-through; zero steady-state allocation per read.
- The KEL-142 one-pending floor blocks Zettlr at L1 (186 invoke sites): floor v2 (F04-T2) is a hard prerequisite for ipcRenderer.invoke.
- Hot-path cost model: Keld webContents.send = facade serialisation → kipc frame → host routing → engine script evaluation into the isolated world (≥ 3 serialise stages vs Electron's 2); per-frame allocation exists today in the TS reader/writer and the Rust frame read.
- Benchmark lane: same-app compat-overhead arms in gyldlab/keld-benches (electron-quick-start first, then draw.io), registered metric ids FACADE-RTT (renderer invoke → ipcMain.handle → reply), FACADE-DELTA (`@keld/electron` minus raw `@keld/api`), SEND-FANOUT, SYNC-BLOCK, plus the §5.1 product census (idle RSS, first paint) with interleaved rounds, ≥20 sessions × 100k calls for IPC.
- Serial step 0: a channel-id and capability-name allocation rule (reserved id ranges per family, generated constants) because every family touches the same hand-mirrored constants in keld-ipc and the TS transport.
""",
[
 T('X05-T1','task(perf): register compat-overhead metric ids in keld-benches and define the same-app Electron-vs-Keld lane (quick-start first, draw.io after migrate)','task','Tier 1','L0',
   'Add registry metric ids with census/clock/statistic rules per arch 01 §5.1: FACADE-RTT, FACADE-DELTA, SEND-FANOUT (1/100/1000 msgs; 64 B / 1 KiB / 64 KiB), SYNC-BLOCK (blocking time distribution, deadline-hit rate), plus product-census idle RSS and first paint for the migrated app; arms Electron 44.4.5 vs Keld at a named sha, same machine, interleaved rounds, fresh/warm separate; no number is published before ≥20 sessions × 100k calls with session-block bootstrap CI.',
   ['Registry entries exist with oracle, census and cache class; a fixture committed per metric','A diagnostic run on macOS produces raw JSON in the benches repo (publication.eligible=false until the sample policy is met)'],
   ['Publishing a 1-session p99 as a pass fails the policy test'],
   ext=['KEL-90','KEL-129'],owner='KEL-90 / KEL-129',gates=['none'],members=[],size='M',ready='ready-for-agent',
   current='No compat-overhead lane exists.',interfaces='keld-benches metric registry; fixtures.',oos='Optimisation work.'),
 T('X05-T2','task(ipc): channel-id and capability-name allocation rule with generated constants (serial step 0 before family fan-out)','task','Tier 1','L0',
   'Specify reserved channel-id ranges per family and a generated constants file consumed by both the Rust frame layer and the TS transport (replacing hand-mirrored ECHO/LIFECYCLE constants), plus the capability-name registry for guard grants; name the contract owner since KEL-98 is echo-only.',
   ['Both sides compile from one generated source; a mismatch is a build failure','Family ranges are reserved and documented'],
   ['Hand-editing a constant on one side fails the consistency test'],
   ext=['KEL-98 (bounded codegen)','KEL-136 transport'],owner='KEL-98 / KEL-136',gates=['wire protocol'],members=[],size='S',ready='needs-spec',
   current='Constants are hand-mirrored in two files.',interfaces='Generated constants; frame layer; transport.',oos='General schema codegen.'),
],
[D('X05-A4','Hot-path cost model for chatty one-way IPC (webContents.send fan-out, ipcRenderer.send) and the allocation-free rules for facade hot paths — measure before design','research','Count serialise/copy stages per hop on each engine, per-frame allocations in the TS reader/writer and Rust frame read, and engine script-evaluation cost per host→renderer message (batched vs per-message); produce the rules (no hidden queues that reorder; bounded batching) with measured numbers before any optimisation ticket.')],
['skipping or back-filling conformance entries to ship faster','widening a guard grant or defaulting a missing handler to allow','retries/sleeps/CI reruns to make numbers pass','a Rust rewrite, shared memory or runtime removal presented as proof','blending measured/target/projection','publishing any compatibility percentage without a committed denominator','renderer-side shared memory or spin-wait','facade-side hidden queues that reorder','PIDs as identity','hand-mirroring new channel ids'],
[],
['Worker-owned single-link transport (panel fog) — the only candidate for blocking emulation'])

# ---------------- PANEL experiment tickets (first slices S0–S2) ----------------
PANEL_P=[
 D('PANEL-P1','S0 link-drain gate: worker-owned single-link transport prototype (arms A current, B worker+SAB/Atomics, C host credit frames, E revoke-during-park)','prototype',
   'Extend the park-probe against real @keld/kipc frames: arm A (main-thread transport, expected stall after 8 KiB), arm B (socket in a Worker, SAB ring + Atomics.notify, main Atomics.wait for sync CALLs, in-order EVENT replay after wake), arm C (host credit frames via the existing Grant frame kind), arm E (generation retire/quit during a park). Exit: pass/fail per arm on bytes-to-stall, KELD-IPC-006 observation against the Rust host writer, 10k ordered replay, sync RTT ≤ 2× raw (diagnostic). Nothing that parks the Bun main thread may land before this decides; B passing makes the worker-owned transport a KEL-80/KEL-97 spec with wire-protocol + public-API gates.'),
 D('PANEL-P2','S1 close/quit veto oracle: objc2 harness (windowShouldClose:NO + applicationShouldTerminate later reply) diffed against Electron 44.4.5 fixtures (E3 re-entrancy, E5 two dirty windows)','prototype',
   'Record Electron\'s transcript for close → preventDefault → isModified → showMessageBoxSync → destroy, tombstone-before-closed, quit ordering (before-quit / per-window close / will-quit; quitAndInstall inversion) and whether JS re-enters during the Cocoa modal; mirror on an AppKit harness with async veto hooks. Exit: a diffed transcript and ▲/✔ marks for close-veto fail-closed, serialized close requests across a modal, and re-entrancy.'),
 D('PANEL-P3','S2 draw.io boot and authority trace (E1): run its main under Bun 1.4.2 with a throwing @keld/electron stub and a Node-API tracer over install + activation + 4-step primary workflow','prototype',
   'Classify every fs/net/child_process row as scopable to a dialog-minted grant, scopable to a declared role, or unscopable; list the pre-evaluation descriptor set the import needs; draft the committed denominator (install, activation, four primary_workflow cells) and the fix-it list; populate the drawio webapp submodule at the pinned desktop commit first (the shallow clone has it empty). Decides whether the first proof is "config-only under legacy" or "config + N edits".'),
]
json.dump({'units':DRAFTS,'panel_p':PANEL_P},open('drafts_part3.json','w'),indent=1); print('part3 units:',list(DRAFTS),'tickets:',sum(len(u['tickets']) for u in DRAFTS.values()),'panel P:',len(PANEL_P))
