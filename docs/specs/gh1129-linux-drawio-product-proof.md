# Spec: Linux draw.io product proof

- Status: draft
- Tracker: GH-1129
- Owner: @0monish (specification coordination)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Qualify draw.io Desktop on a declared Ubuntu or Debian x86_64 system. Give each installed or preview run its own evidence. Separate the Linux display backends. Keep unexecuted outcomes unknown.

Scope includes source demand, independent observation and the handoff to the Linux operator. Exclude release publication, installer selection, new privileges, app rewrites, cross-platform scores and runs on macOS or Windows.

## 2. Spec refs

- FACT: [Architecture 04](../architecture/04-electron-compat.md) sections 4–6 govern native modules, operation evidence and the product corpus. [Architecture 03](../architecture/03-security.md) sections 1–4 govern authority.
- FACT: [GH-532](gh532-first-proof-evidence-rules.md) and [GH-566](gh566-corpus-manifest-owner.md) own evidence and manifest admission. KEL-74 owns scoring.
- FACT: #1146 `docs/specs/gh587-electron-apps-v0-corpus-cells.md:1@16e15d42f833ca2942e4312b80987c6e541c6c71` defines authenticated product observation.
- FACT: #1159 `docs/specs/gh500-drawio-product-corpus-partitions.md:1@384ffa7cb52ff32afd993035a9434bd54c934ee2` defines the first macOS partitions. Linux inclusion requires its owner's review.
- FACT: [#1406](https://github.com/gyldlab/keld/issues/1406) owns Linux installed packaging design. Its proposed installation path lacks Linux product qualification.
- FACT: [#1124](https://github.com/gyldlab/keld/issues/1124) assigns Linux terminal proof here. #589 and #1183 own other platforms.

### Pinned application trace

FACT: These GitHub source observations were retrieved on 2026-11-06. A reachable call establishes demand. The operator observes actual execution separately.

| Step | Pinned source and literal | Linux requirement and owner |
|---|---|---|
| Module entry | [package.json, lines 3–7][trace-1] declares `"main": "src/main/electron.js"`. [Imports][trace-2] include `electron-updater`. | #499, #502, #580. Resolve actual named exports, aliases and transitive imports at startup. |
| Dependencies | [lockfile][trace-3], git blob `aab2e8562ea5fd1b46e6a9cbc2a432feb78de550`. Entries include `node_modules/electron` 44.5.1 and `node_modules/electron-updater` 6.8.9. | #391, #532. Record executed binary separately from the API reference. |
| Resources and constructor | [Window construction][trace-4] uses `screen.getAllDisplays()`, theme, preload, and a file URL for index.html. | #1125, #1126, #1127, #481. Observe the actual content document. |
| Bridge | [Isolated preload][trace-5] sends `rendererReq` and receives `mainResp`. | #1127, #463. Verify same-window correlated replies, sender identity and current document. |
| Lifecycle | [Startup][trace-6] reaches both webRequest registrations before later IPC setup. | #446, #469. A partial startup can create a page yet omit handlers. Exercise the resulting file operation. |
| Open/save | [Renderer open][trace-7] requests `showOpenDialog` and `readFile`. [Main][trace-8] uses the focused window for dialogs. | #476, #478 and Linux adapter owner. Exercise the native chooser and a real file. |
| Dirty close | [Main branch][trace-9] sets Linux Save 0, Discard 1 and Cancel 2. [Renderer][trace-10] sends `saveAndClose-result`. | #1125, #450, #564. Probe the actual parented prompt and each disk/window effect. |
| Menu/updater | [Ready callback][trace-11] calls `menu.setApplicationMenu(null)` outside macOS. Update checks depend on flags and stored settings. | #480, #580. Validate the Linux branch and inert updater. Record fresh-profile state. |

[trace-1]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/package.json#L3-L7
[trace-2]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L4-L24
[trace-3]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/package-lock.json
[trace-4]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L714-L777
[trace-5]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js#L62-L129
[trace-6]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L987-L1027
[trace-7]: https://github.com/jgraph/drawio/blob/24b76c2cbd55d88e354042e8d329a2e4708972bc/src/main/webapp/js/diagramly/ElectronApp.js#L520-L552
[trace-8]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L4191-L4227
[trace-9]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L849-L945
[trace-10]: https://github.com/jgraph/drawio/blob/24b76c2cbd55d88e354042e8d329a2e4708972bc/src/main/webapp/js/diagramly/ElectronApp.js#L458-L501
[trace-11]: https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L2063-L2100

## 3. Acceptance criteria

### AC-1: Pin the input and the reference

- **Given:** A clean desktop source tree, its webapp gitlink, the lockfile, #499 migration inputs and an empty attempt ledger.
- **When:** The operator freezes an application and an Electron reference before first launch.
- **Then:** Admit only identified source, resolved dependencies, migrated tree, binaries, approved configuration, declared profile and a named observation lane.
- **Negative control:**
  - **Single fault:** Change one webapp byte after materialization while retaining the declared gitlink.
  - **At-risk precondition:** The changed file belongs to the actual webapp resource closure.
  - **Expected RED:** The applied-tree digest differs before launch. Preserve the rejected attempt.

### AC-2: Separate installation from activation

- **Given:** A qualified distribution and authenticated installed-root admission from B-1.
- **When:** An operator installs into a clean environment and launches from the supported application entry outside the checkout.
- **Then:** Match the installed component inventory and actual running Host/resources to the selected distribution and registered application. Qualify installation independently of activation.
- **Negative control:**
  - **Single fault:** Redirect only the launcher to a development stage carrying identical application resources.
  - **At-risk precondition:** Both paths launch successfully and display the same document.
  - **Expected RED:** The runtime identity/path admission rejects the installed claim.

### AC-3: Attribute activation to this application

- **Given:** B-2 is clear. The qualified collector binds the current Host, admitted role, window and document.
- **When:** The real app finishes loading its pinned editor and services an action through its own preload.
- **Then:** Record live role, committed app page, absence of fatal startup errors and independently observed application response.
- **Negative control:**
  - **Single fault:** Launch a hello fixture that reproduces the title and image.
  - **At-risk precondition:** The fixture has a real window and commits a successful navigation.
  - **Expected RED:** Application resource identity or actual editor action fails.

### AC-4: Observe open, edit and save separately

- **Given:** A fresh test-owned diagram with a unique visible label A, an unrelated canary file and a destination outside the checkout.
- **When:** Open the source via the app, change A to B in the editor, then save through the app.
- **Then:** Observe A after open and B before save. Independently decode the written diagram and verify B in the correct node. Verify the canary bytes.
- **Negative control:**
  - **Single fault:** Suppress the save write while returning its usual success response.
  - **At-risk precondition:** The live editor shows B while the last persisted version still contains A.
  - **Expected RED:** Open and edit observations stand. Save fails on disk content.

### AC-5: Resolve the Linux dirty prompt correctly

- **Given:** After a verified save, change B to C without saving. Use independent fresh documents for each branch.
- **When:** Request a native close and choose Cancel, Discard or Save by its observed action identity.
- **Then:** Cancel retains C in the window and B on disk. Discard closes and retains B on disk. Save closes after persistence of C.
- **Negative control:**
  - **Single fault:** Exchange the Cancel and Discard responses in the Linux driver.
  - **At-risk precondition:** The same prompt exposes both actions and the window contains C.
  - **Expected RED:** The Cancel case loses its live window, even if both actions left identical disk bytes.

### AC-6: Prove authorization and generation loss

- **Given:** The collector witnessed effective explicit Legacy under #1128/KEL-78 and an authenticated live document under B-2.
- **When:** Exercise one authorized app action and an undeclared guarded operation at the same boundary. Retire the role during a distinct in-flight action.
- **Then:** Observe permitted effect, refused unauthorized effect and rejection of late or unbound evidence. Qualify a successor only after independent new admission.
- **Negative control:**
  - **Single fault:** Relabel a late G1 result as G2 after authentic G2 admission.
  - **At-risk precondition:** Both generations used the same frozen policy and returned identical success text.
  - **Expected RED:** The initiating generation/request or document identity mismatches.

### AC-7: Close, clean up and relaunch

- **Given:** A ledger of run-owned processes, connection identities, stage paths and persisted user files.
- **When:** Close via the app, observe retirement, then launch again through the same declared entry.
- **Then:** Verify retired identities refuse work, owned temporary resources close, package and user data survive, and the fresh session reopens the last saved marker.
- **Negative control:**
  - **Single fault:** Keep the previous run's child alive after terminal acknowledgment.
  - **At-risk precondition:** The successor opens a fresh window successfully.
  - **Expected RED:** The old process incarnation remains observable. Reject cleanup even if relaunch succeeds.

### AC-8: Preserve the attempt census

- **Given:** Independent hosted X11/Xvfb and real Wayland runs, each with its own backend evidence, source and artifact identities.
- **When:** The corpus owner reconciles every mapped attempt and cell against the declared partition.
- **Then:** Reject omitted, duplicated, silently retried or mislabelled attempts. Report outcomes per operation, distribution, profile, backend and artifact.
- **Negative control:**
  - **Single fault:** Replace the Wayland activation evidence with an authentic hosted X11 record from the same build.
  - **At-risk precondition:** Both operators used the same fixture and observed an editor.
  - **Expected RED:** The actual display backend differs from the claimed lane.

## 4. Design

### D-1: Make the run reproducible

FACT: The desktop gitlink points to webapp commit `24b76c2cbd55d88e354042e8d329a2e4708972bc`. See the [pinned desktop tree](https://github.com/jgraph/drawio-desktop/tree/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e).
FACT: [Electron 44.5.1](https://github.com/electron/electron/releases/tag/v44.5.1) lists Linux x64 ZIP SHA-256 `5bcd217611d6843ececd6c9e9c1fcd1da3ab066c43d8b1a9e4b44689a1fba6f5` (retrieved 2026-11-06).
FACT: [Electron v44.4.5](https://github.com/electron/electron/commit/694f45852a0f1726cd23bfd379854de489cccb65) is the normative documentation pin under GH-532.
The archive digest identifies a distribution file. Hash the actual extracted Electron executable and record its reported version. Keep both values.
Record Linux release, architecture, kernel, actual display backend, GTK/WebKitGTK library identities, KELD commit, Host/resource digests, Bun binary digest/version and dependency resolution.
Record imported native addons separately from platform-conditional dependencies present only in the lockfile. For each, name import location, invocation, outcome and existing owner #503.
Hash both repos' applied trees plus reviewed configuration and alias edits. A behavioral source patch requires its own approval and receipt. The unmodified-code criterion requires zero behavioral hunks.
Use the upstream build to calibrate driver actions and file observations. Resolve any 44.5.1 versus 44.4.5 behavioral difference under #391 before making a parity claim.

### D-2: Keep the observers independent

The Host owns role and permission facts. The collector consumes #587's current-role and current-document witnesses at action time. A PID, application log, screenshot or writable receipt file cannot mint either identity.
The driver supplies input through the app's visible controls. An independent observer reads actual UI and file state. Reopening saved data must follow the installed entry for an installed-workflow claim.
Match initiator and completion using the existing opaque correlation mechanism. Preserve completed G1 evidence. Deny new G1 observations after retirement. Test completion before and after retirement.
Observe a harmless side effect or its independent absence for each granted/denied action. Scope denial to a Host-brokered action. Explicit Legacy retains ambient same-user authority.
UNKNOWN: hostile same-user evidence integrity beyond #587's qualified custody. Preserve that limit. Never export raw authority tokens.
Use bounded waits for explicit UI, file, process or collector events. Each attempt declares its deadline and a cleanup deadline. On expiry, retain the attempt and last observation.

### D-3: Admit only the observed operation

Use #500's install, activation and primary_workflow partitions through #566. Keep the four workflow operations separate.
Linux receipts require the physical manifest owner's explicit platform/citation admission. A rejected receipt cannot become a product pass through an auxiliary report.
A preview may diagnose activation or workflow under its declared context. Leave install unknown without B-1. Never promote preview results to installed-workflow results.
Record failed writes, a canceled Save As, stale replies, unexpected dialogs, backend fallback and missing resources as distinct outcomes. A canceled or failed save during dirty close must keep the app open.
Use only approved recovery semantics. Reacquire every required generation witness. Never replay a possibly delivered write to produce a passing result.
No boundary change in this documentation proposal. No new channel, capability, policy parser, scorer or receipt collector. No runtime or API fallback.

## 5. Boundaries

Future implementation starts under #1129 after exact-spec approval. Limit changes to the Linux driver, its fixtures, and reviewed Linux registration via the existing corpus owner.
Coordinate with #1125 for windows/close, #1126 for navigation, #1127 for the bridge, #1128 for profiles and #1406 for installed-root admission.
File minimal adapter gaps under the existing family owner with a reproducer. Keep product fixes and unrelated OS or corpus changes in their own claims.

## 6. Tasks

### T-1: Freeze a reproducible Linux reference

- **Blocked by:** none
- After spec approval, qualify AC-1 against upstream Electron. Record the full path census and distinguish executed, conditional and missing calls.
- Publish a dependency-to-owner table. Name the first failing action, actual call path, expected result and exact upstream observation.

### T-2: Qualify the Linux preview workflow

- **Blocked by:** T-1, B-2
- Register eligible Linux cells with the existing physical manifest owner. Run AC-3 through AC-8 independently on each claimed backend.
- Stop each blocked cell at its first missing dependency. Keep unaffected observations and all attempts.

### T-3: Qualify an installed artifact

- **Blocked by:** T-1, B-1, B-2
- Execute AC-1 through AC-8 from the installed entry. Preserve independent activation, workflow, cleanup and relaunch evidence.
- Publish immutable artifact links on #1129. Group receipts by operation and observed environment. Leave unqualified cells open.

## 7. Test plan & evidence

### E-1: Static application and owner trace

- **Run:** Inspect the immutable files in section 2, the upstream lockfile, Electron release metadata and both corpus owner specifications.
- **n:** One desktop revision, one webapp revision and one lockfile.
- **Environment:** GitHub source review. The repository pin is in the header.
- **Limits:** Source inspection cannot show activation, install, containment, correct file I/O or recovery on Linux.

| Check | Future independent test |
|---|---|
| AC-1, AC-2 | Source admission rejects a modified webapp. Installed admission rejects a redirected launcher against an otherwise valid installation. |
| AC-3, AC-4 | A title-matching fixture fails activation. Independently mutate open input, suppress the edit, then suppress the write in separate runs. |
| AC-5 | Run Cancel, Discard and Save from equivalent dirty states. Cancel the nested Save As. Acknowledge success with an unchanged file in a control. |
| AC-6 | Refuse an ungranted Host action, app-printed receipts, missing admission and post-revocation replay. Preserve the expected pre-retirement success. |
| AC-7 | Retain a child deliberately in a disposable test. Include process start identity to detect PID reuse. Verify canary and saved document survive cleanup. |
| AC-8 | Swap backend evidence, remove one attempt, and duplicate another in separate controls. Verify the same owner rejects each mutation. |

An unavailable environment yields unknown. An executed and admissibly observed failure follows #532/#566 expected-fail rules. Cite the implementing issue. Missing normative citation stays unresolved under B-2.
Do not calculate a cross-kind percentage. A matching expected failure can make a harness test green while product behavior still fails.

## 8. Review gates triggered

This draft changes documentation only. Future changes to public evidence semantics, permission enforcement or the wire format require review by their existing owners.
No permission, unsafe, dependency, public API or wire change is proposed by this proof driver.

## 9. Perf impact

None. This proof tests correctness and attribution.

## 10. Open questions, blockers & unknowns

### B-1: Linux installed distribution

- **Status:** open
- **Owner:** #1406 and KEL-141 distribution with the installed-admission owner.
- **Impact:** Installed delivery and installed-workflow qualification remain blocked.
- **Check:** Obtain an independently identified supported package, verified installed root and matching launched component inventory.

### B-2: Admissible product evidence and native adapters

- **Status:** open
- **Owner:** #500/#587 for Linux partition and collector, #532/#566 for citations, #1125–#1128 and native family owners for demanded behavior.
- **Impact:** A missing prerequisite blocks only operations that consume it. Missing trusted Host admission blocks every product success.
- **Check:** Name exact accepted artifacts for each exercised dependency. Demonstrate one admitted current-generation observation and one rejected stale observation.

### U-1: Linux execution

UNKNOWN: Actual install, activation, workflow, denial, recovery, cleanup and relaunch for every backend. A Linux operator supplies the first native evidence under #1129.
