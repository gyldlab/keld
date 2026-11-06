# Future implementation packs

Census captured for [GH-1414](https://github.com/gyldlab/keld/issues/1414) on 6 November 2026. Its main source pin is [`0eb3ce203187`](https://github.com/gyldlab/keld/commit/0eb3ce203187f35c5276a4237189c906a5220d5a). This is a dispatch list for specification and review. The owning issue and approved contract control implementation.

## Published after the census

Fifteen issue-level packs were published after the census. Combined with the original six below, the tracked total is **21 feature packs**. The inventory PR itself is separate.
The links were reconciled on 6 November 2026 against the [21-pack dispatch](https://github.com/gyldlab/keld/issues/1414#issuecomment-6677387749) and the current feature PR heads.
At those revisions the specifications contain **167 acceptance criteria and 77 implementation tasks**. Every listed feature PR is an open draft, and its hosted `CI required` check passed. These documentation checks do not establish implementation, approval or native acceptance.
Each source link pins that publication revision. Follow the PR for later changes. The issue and PR census files remain the original historical snapshot; this publication reconciliation is not a fresh repository census.

| Owner | Pack | Published draft and source |
|---|---|---|
| [#591](https://github.com/gyldlab/keld/issues/591) | Reversible migration writes | [#1415](https://github.com/gyldlab/keld/pull/1415) · [spec at 20cfcab9](https://github.com/gyldlab/keld/blob/20cfcab90c7a2dd9c74c3de42ef68a8062671521/docs/specs/gh591-reversible-migrate-write.md) |
| [#1129](https://github.com/gyldlab/keld/issues/1129) | Installed Linux draw.io proof | [#1416](https://github.com/gyldlab/keld/pull/1416) · [spec at ce0596ff](https://github.com/gyldlab/keld/blob/ce0596ff0080d496610ef1675325c64ddc92fd16/docs/specs/gh1129-linux-drawio-product-proof.md) |
| [#555](https://github.com/gyldlab/keld/issues/555) | Network-status reads | [#1418](https://github.com/gyldlab/keld/pull/1418) · [spec at 70cc54e6](https://github.com/gyldlab/keld/blob/70cc54e673f6b5e76b10ed0a304ff51f93683197/docs/specs/gh555-legacy-net-online.md) |
| [#550](https://github.com/gyldlab/keld/issues/550) | Dropped-file identity | [#1419](https://github.com/gyldlab/keld/pull/1419) · [spec at 449e6143](https://github.com/gyldlab/keld/blob/449e61437c4106dafacdf3bdc69a163de72edaf4/docs/specs/gh550-dropped-file-path.md) |
| [#488](https://github.com/gyldlab/keld/issues/488) | Renderer termination | [#1420](https://github.com/gyldlab/keld/pull/1420) · [spec at 044da001](https://github.com/gyldlab/keld/blob/044da001a871653ef20b933251ff5183b25f736e/docs/specs/gh488-render-process-gone.md) |
| [#574](https://github.com/gyldlab/keld/issues/574) | Renderer responsiveness | [#1421](https://github.com/gyldlab/keld/pull/1421) · [spec at 5d25070b](https://github.com/gyldlab/keld/blob/5d25070bc030ec38e9e896f0acd43c27753676fa/docs/specs/gh574-renderer-responsiveness.md) |
| [#1356](https://github.com/gyldlab/keld/issues/1356) | Zettlr system appearance subscriptions | [#1422](https://github.com/gyldlab/keld/pull/1422) · [spec at f8a23c14](https://github.com/gyldlab/keld/blob/f8a23c14bcf670b84e042d63d95995f3fc62c14b/docs/specs/gh1356-system-preferences-theme.md) |
| [#593](https://github.com/gyldlab/keld/issues/593) | Bun auto-install refusal | [#1423](https://github.com/gyldlab/keld/pull/1423) · [spec at 86765a90](https://github.com/gyldlab/keld/blob/86765a90e378d112a1cbc55f6685a55c745db089/docs/specs/gh593-bun-auto-install-refusal.md) |
| [#534](https://github.com/gyldlab/keld/issues/534) | BrowserWindow state and lookups | [#1424](https://github.com/gyldlab/keld/pull/1424) · [spec at 5aee5eeb](https://github.com/gyldlab/keld/blob/5aee5eeb80aaea24d222b921eca3ec4038fd3e20/docs/specs/gh534-browserwindow-state-members.md) |
| [#521](https://github.com/gyldlab/keld/issues/521) | macOS reopen and activate | [#1425](https://github.com/gyldlab/keld/pull/1425) · [spec at c2afd5b9](https://github.com/gyldlab/keld/blob/c2afd5b93e892f0a29ed1e3bb030c4e3cf6b8fca/docs/specs/gh521-macos-activate.md) |
| [#522](https://github.com/gyldlab/keld/issues/522) | App paths and logs | [#1426](https://github.com/gyldlab/keld/pull/1426) · [spec at 2b5a4f97](https://github.com/gyldlab/keld/blob/2b5a4f978d84ebd7ddcbda3fb1c69c8a602bb731/docs/specs/gh522-app-path-overrides.md) |
| [#474](https://github.com/gyldlab/keld/issues/474) | Default session and scoped storage clearing | [#1427](https://github.com/gyldlab/keld/pull/1427) · [spec at ffdbb828](https://github.com/gyldlab/keld/blob/ffdbb828d36b40718f99d7b823830b924c120123/docs/specs/gh474-session-profile-clear.md) |
| [#479](https://github.com/gyldlab/keld/issues/479) | Guarded shell operations | [#1428](https://github.com/gyldlab/keld/pull/1428) · [spec at 806940d0](https://github.com/gyldlab/keld/blob/806940d03d767b66ad880f519ad5d11002d1ba99/docs/specs/gh479-guarded-shell-broker.md) |
| [#547](https://github.com/gyldlab/keld/issues/547) | Renderer invoke and main-process handlers | [#1429](https://github.com/gyldlab/keld/pull/1429) · [spec at b303a80c](https://github.com/gyldlab/keld/blob/b303a80c44999eac0dd33780c3b5dd7ec78db3bf/docs/specs/gh547-invoke-handle-delivery.md) |
| [#556](https://github.com/gyldlab/keld/issues/556) | Non-role menu clicks and custom accelerators | [#1430](https://github.com/gyldlab/keld/pull/1430) · [spec at 092ba4ff](https://github.com/gyldlab/keld/blob/092ba4ff958ad401bf4d7d70c49492f0f6fcff4d/docs/specs/gh556-non-role-menu-click.md) |

### Reconciled specification corrections

The current [invoke pack #1429](https://github.com/gyldlab/keld/pull/1429) separates the explicit-Legacy macOS `keld dev` lane from installed-app proof. The earlier requirement for an installer before development qualification is corrected in the linked specification.
The development lane requires independent results from two overlapping startup reads, more than 4,096 UTF-8 bytes of actual document text and 20 real edit updates. The installed lane repeats its observations from an identified installation. These remain future execution criteria.

The [Linux proof #1416](https://github.com/gyldlab/keld/pull/1416) uses a never-saved modified diagram for nested Save As cancellation, so the chooser is actually reached.
The [dropped-file pack #1419](https://github.com/gyldlab/keld/pull/1419) compares raw Electron and Keld results before application wrappers mask an empty result or thrown error; activation still requires integrated evidence.
The [menu pack #1430](https://github.com/gyldlab/keld/pull/1430) checks action-time focus, the original menu incarnation and no duplicate native-role dispatch. Its Cmd+S proof requires window A's file to change while window B's file stays unchanged.

## Start here

1. Pick a feature below. Read its latest trusted claim, linked spec at the recorded commit, and any subsequent review or superseding PR. Read its live dependencies before selecting a task.
2. Reuse a covering spec. Where a dedicated pack is still needed, give it one issue, one writer and one draft PR. Record the first observable and its negative control.
3. Before implementing, verify the exact content approval and cleared prerequisites. A PR, a file, a green docs check and a successful native run each establish different facts.

### Published first batch

| Owner | Published draft | Scope |
|---|---|---|
| [#1127](https://github.com/gyldlab/keld/issues/1127) | [#1408](https://github.com/gyldlab/keld/pull/1408) · [source](https://github.com/gyldlab/keld/blob/719ce9118d8455471481aea6e718c2b6fcfc25f3/docs/specs/gh1127-linux-preload-bridge.md) | the Linux isolated preload bridge |
| [#1126](https://github.com/gyldlab/keld/issues/1126) | [#1409](https://github.com/gyldlab/keld/pull/1409) · [source](https://github.com/gyldlab/keld/blob/aaafcbfe69e29ff5ca68bdc9093eea1af85f40cc/docs/specs/gh1126-linux-webcontents-navigation-eval.md) | Linux navigation and evaluation |
| [#590](https://github.com/gyldlab/keld/issues/590) | [#1410](https://github.com/gyldlab/keld/pull/1410) · [source](https://github.com/gyldlab/keld/blob/65a406c0345c61351db7afc4fcf2132feb44d59d/docs/specs/gh590-migration-analyzer-scan-rules.md) | static migration analyzer scan rules |
| [#1189](https://github.com/gyldlab/keld/issues/1189) | [#1411](https://github.com/gyldlab/keld/pull/1411) · [source](https://github.com/gyldlab/keld/blob/8d6a280ed43e92b654294f692dd69cc2941ff090/docs/specs/gh1189-windows-native-dialogs.md) | Windows native dialog acceptance |
| [#594](https://github.com/gyldlab/keld/issues/594) | [#1412](https://github.com/gyldlab/keld/pull/1412) · [source](https://github.com/gyldlab/keld/blob/66390503b01560ac96b1f4db9aea60116ecae792/docs/specs/gh594-window-web-grants.md) | per-window camera and microphone grants |
| [#623](https://github.com/gyldlab/keld/issues/623) | [#1413](https://github.com/gyldlab/keld/pull/1413) · [source](https://github.com/gyldlab/keld/blob/819c347f9e0e000af2f438311b63f31c76077973/docs/specs/gh623-bounded-outbound-budget.md) | aggregate outbound transport budgets |

### Complementary pack priorities

These six published packs extend the original nine. Start migration, IPC and the Linux proof in parallel. The Linux proof design can proceed while its installation and runtime dependencies are still open. Live publication receipts belong on the feature issue and the [roll-up tracker](https://github.com/gyldlab/keld/issues/1414).

| Owner | Pack | First independent check | Inputs to reconcile |
|---|---|---|---|
| [#591](https://github.com/gyldlab/keld/issues/591) Reversible migration writes | Generated files, manifest edits and undo. | Compare against the hand-authored artifact. Introduce a later user edit before undo. | [#498](https://github.com/gyldlab/keld/issues/498), [#499](https://github.com/gyldlab/keld/issues/499), [#500](https://github.com/gyldlab/keld/issues/500) |
| [#547](https://github.com/gyldlab/keld/issues/547) Electron invoke and handle | Renderer promise and main-process handler registry. | Reverse two replies and throw from a handler. Confirm the right caller receives only the permitted result/error. | [#546](https://github.com/gyldlab/keld/issues/546), [#466](https://github.com/gyldlab/keld/issues/466), [#467](https://github.com/gyldlab/keld/issues/467) |
| [#1129](https://github.com/gyldlab/keld/issues/1129) Installed Linux draw.io proof | Product evidence for an installed artifact. | Substitute a preview launcher, inert fixture or different digest. Each must fail proof admission. | [#500](https://github.com/gyldlab/keld/issues/500), [#587](https://github.com/gyldlab/keld/issues/587), [#1124](https://github.com/gyldlab/keld/issues/1124), [#1406](https://github.com/gyldlab/keld/issues/1406) |
| [#555](https://github.com/gyldlab/keld/issues/555) Legacy network status | Synchronous connectivity hint used by the Zettlr updater. | Read both APIs before/after Ready. Distinguish offline from an unconfirmed route. | [#554](https://github.com/gyldlab/keld/issues/554) |
| [#550](https://github.com/gyldlab/keld/issues/550) Dropped-file path | Host-captured file identity for the Zettlr renderer. | Drop a real file and construct a same-name File. Only the original may disclose its path. | [#467](https://github.com/gyldlab/keld/issues/467) |
| [#556](https://github.com/gyldlab/keld/issues/556) Native menu actions | Non-role selection and custom accelerators. | Invoke Cmd+S with two dirty windows. Only the action-time focused window's file changes; retired-menu actions cannot target a replacement. | [#480](https://github.com/gyldlab/keld/issues/480), [#531](https://github.com/gyldlab/keld/issues/531), [#508](https://github.com/gyldlab/keld/issues/508), [#527](https://github.com/gyldlab/keld/issues/527) |

### Original nine pack priorities

Order within a feature follows its prerequisites. The Bun and Zettlr startup work can be specified independently. Session, window and diagnostics rows share owner contracts that the selected writer needs to read.

| Owner and claim | Pack | Discriminating test to design | Inputs to reconcile |
|---|---|---|---|
| [#593](https://github.com/gyldlab/keld/issues/593) · [claim](https://github.com/gyldlab/keld/issues/593#issuecomment-6674150728) | Disable Bun package auto-install | A missing Electron alias fails on a warm install cache with the registry offline. Observe each role's actual launch arguments. | [#502](https://github.com/gyldlab/keld/issues/502), [#452](https://github.com/gyldlab/keld/issues/452) |
| [#1356](https://github.com/gyldlab/keld/issues/1356) · [claim](https://github.com/gyldlab/keld/issues/1356#issuecomment-6674149507) | Zettlr appearance at startup | Construct AppearanceProvider before the first window. Drive one theme change, unsubscribe, then drive a second change. | [#481](https://github.com/gyldlab/keld/issues/481), [#558](https://github.com/gyldlab/keld/issues/558) |
| [#522](https://github.com/gyldlab/keld/issues/522) · [claim](https://github.com/gyldlab/keld/issues/522#issuecomment-6674147760) | App data and log path overrides | Separate the reported path from the engine profile root and file permissions. Exercise a nonexistent directory. | [#452](https://github.com/gyldlab/keld/issues/452) |
| [#474](https://github.com/gyldlab/keld/issues/474) · [claim](https://github.com/gyldlab/keld/issues/474#issuecomment-6674149070) | Default session and scoped data clearing | Seed sibling origins sharing a registrable domain. Refuse any filter whose exact deletion scope cannot be preserved. | [#553](https://github.com/gyldlab/keld/issues/553), [#554](https://github.com/gyldlab/keld/issues/554) |
| [#521](https://github.com/gyldlab/keld/issues/521) · [claim](https://github.com/gyldlab/keld/issues/521#issuecomment-6674147370) | Application activation after Dock reopen | Compare initial launch, reopen with a visible window, and reopen with zero visible windows. Check the registry count at dispatch. | [#449](https://github.com/gyldlab/keld/issues/449) |
| [#534](https://github.com/gyldlab/keld/issues/534) · [claim](https://github.com/gyldlab/keld/issues/534#issuecomment-6674148187) | BrowserWindow state, bounds and lookup | Check host identities and state across two windows and a destroyed handle. Record native events before asserting their order. | [#449](https://github.com/gyldlab/keld/issues/449), [#533](https://github.com/gyldlab/keld/issues/533), [#459](https://github.com/gyldlab/keld/issues/459) |
| [#479](https://github.com/gyldlab/keld/issues/479) · [claim](https://github.com/gyldlab/keld/issues/479#issuecomment-6674148632) | Guarded shell operations | Compare an allowed literal origin to a visually similar host and forbidden schemes. Record denial before any OS action. | [#505](https://github.com/gyldlab/keld/issues/505), [#416](https://github.com/gyldlab/keld/issues/416) |
| [#488](https://github.com/gyldlab/keld/issues/488) · [claim](https://github.com/gyldlab/keld/issues/488#issuecomment-6674149919) | Renderer termination reporting | Attribute a renderer exit to the affected WebContents, independently of the browser process or Bun role. | [#573](https://github.com/gyldlab/keld/issues/573), [#459](https://github.com/gyldlab/keld/issues/459), [#449](https://github.com/gyldlab/keld/issues/449), [#508](https://github.com/gyldlab/keld/issues/508) |
| [#574](https://github.com/gyldlab/keld/issues/574) · [claim](https://github.com/gyldlab/keld/issues/574#issuecomment-6674150318) | Renderer responsiveness | Repeat a hang notification. Require separate recovery evidence before emitting responsive. | [#573](https://github.com/gyldlab/keld/issues/573), [#449](https://github.com/gyldlab/keld/issues/449), [#459](https://github.com/gyldlab/keld/issues/459), [#508](https://github.com/gyldlab/keld/issues/508) |

### Next local-agent work

| Priority | Work item | First check and reason |
|---|---|---|
| 1 | [#498](https://github.com/gyldlab/keld/issues/498) Read-only migration analysis | Reconcile the output contract in [#497](https://github.com/gyldlab/keld/issues/497) and analyzer rules in [#1410](https://github.com/gyldlab/keld/pull/1410). Compare the CLI and MCP tool against one hand-resolved call-site list. Reversible writes in [#591](https://github.com/gyldlab/keld/issues/591) consume this result. |
| 1 | [#546](https://github.com/gyldlab/keld/issues/546) Concurrent renderer requests | Reconcile the live issue's conflicting GitHub-versus-KEL-80 tracker text before claiming. Then verify the carrier in [#1122](https://github.com/gyldlab/keld/pull/1122), codec in [#1190](https://github.com/gyldlab/keld/pull/1190), and payload owner [#545](https://github.com/gyldlab/keld/issues/545). Force overlapping calls, reversed completion and refusal at capacity. |
| 1 | [#554](https://github.com/gyldlab/keld/issues/554) Session and network reference cases | Record the pinned Electron 44.4.5 calls before scoring [#474](https://github.com/gyldlab/keld/issues/474) or [#555](https://github.com/gyldlab/keld/issues/555). Separate browser storage types, origin filtering, pre-Ready access and offline observation. |
| 2 | [#533](https://github.com/gyldlab/keld/issues/533) Window reference cases | Record the order between native state, setter return and callbacks before implementing [#534](https://github.com/gyldlab/keld/issues/534) or [#535](https://github.com/gyldlab/keld/issues/535). Keep the [#521](https://github.com/gyldlab/keld/issues/521) reopen observation with its own owner. |
| 2 | [#481](https://github.com/gyldlab/keld/issues/481) / [#558](https://github.com/gyldlab/keld/issues/558) System appearance | Check whether the approved mirror can distinguish a true OS theme change from an app override, including a zero-window role. This is the integration gate for [#1356](https://github.com/gyldlab/keld/issues/1356). |
| Proof | [#1129](https://github.com/gyldlab/keld/issues/1129) Installed Linux draw.io | Use the new proof pack to prepare artifact identity and expected failures. Installed success waits for actual packaging, profile admission, app workflow and native receipts. |

### Later triage and parked work

Only queue a later feature after its current owner and prerequisites are checked. Parked rows require an explicit change in priority. The final column contains related owners as well as dependencies.

| Owner | Future deliverable | First action on GitHub | Related |
|---|---|---|---|
| [#592](https://github.com/gyldlab/keld/issues/592) | Analyzer blocker classification | Reuse the scan rules in PR #1410. Examine an unresolved native addon against the owning evidence table before proposing another rule. | [#498](https://github.com/gyldlab/keld/issues/498), [#590](https://github.com/gyldlab/keld/issues/590) |
| [#549](https://github.com/gyldlab/keld/issues/549) | Context bridge return values | Reconcile #1354 decision D-Z21 and [#1375](https://github.com/gyldlab/keld/issues/1375) with the older issue first. Select only still-required synchronous bridge shapes, then verify the isolated-world value table. | [#399](https://github.com/gyldlab/keld/issues/399), [#467](https://github.com/gyldlab/keld/issues/467) |
| [#535](https://github.com/gyldlab/keld/issues/535) | Initially hidden window | Trace ready-to-show versus first paint against the pinned Electron recording and registry. Reuse existing window ownership. | [#394](https://github.com/gyldlab/keld/issues/394), [#449](https://github.com/gyldlab/keld/issues/449), [#533](https://github.com/gyldlab/keld/issues/533) |
| [#558](https://github.com/gyldlab/keld/issues/558) | Display and theme updates | Reuse the host display/theme mirror in PR #1164. Inspect event fan-out and removal across two windows. | [#481](https://github.com/gyldlab/keld/issues/481) |
| [#561](https://github.com/gyldlab/keld/issues/561) | Synchronous dialogs | Inspect existing dialog and blocking-call contracts, including nested close behavior, before proposing another sync surface. | [#478](https://github.com/gyldlab/keld/issues/478), [#418](https://github.com/gyldlab/keld/issues/418), [#528](https://github.com/gyldlab/keld/issues/528) |
| [#578](https://github.com/gyldlab/keld/issues/578) | Child-process termination | Parked. Keep named Bun-role death distinct from renderer termination in #488. Check attribution and terminal-cause vocabulary. | [#508](https://github.com/gyldlab/keld/issues/508) |
| [#1355](https://github.com/gyldlab/keld/issues/1355) | Zettlr filesystem watcher qualification | Record boot, change and unlink under the declared profile before attributing an engine or compatibility gap. | [#1358](https://github.com/gyldlab/keld/issues/1358) |
| [#1405](https://github.com/gyldlab/keld/issues/1405) | Windows installed packaging | Parked for implementation. Reuse the Windows design in #1407 and the platform tracker. | [#1182](https://github.com/gyldlab/keld/issues/1182) |
| [#1406](https://github.com/gyldlab/keld/issues/1406) | Linux installed packaging | Parked for implementation. Reuse the Linux design in #1407. Send future installed-runtime evidence to #1129. | [#1124](https://github.com/gyldlab/keld/issues/1124), [#1129](https://github.com/gyldlab/keld/issues/1129) |

## Reproduce the inventory

- [Open issue census](future-implementation-issues.tsv) contains 397 unique open issues, including the inventory issue itself. The initial tracker recorded 396 before #1414 was created. The input was every page of the repository's open issues endpoint, with PRs removed.
- [Spec evidence and PR coverage](future-implementation-specs.tsv) records 43 Markdown paths on main and 133 changed Markdown occurrences present at the heads of 179 open PRs. Stacked PRs can contain the same path. The scan compares changed filenames under `docs/specs/` with the immutable Git tree of each relevant head.
- One changed path is absent at its pinned head. PR #1325 splits the flat KEL-142 spec into a directory. The old path has an explicit absence row. Open PRs without changed spec Markdown have a coverage row and a head pin. A spec inherited unchanged from an open PR's base can fall outside that PR's changed paths.
- The issue census also records work kind, stated milestone and specification cues. Of 397 rows, 97 contain a first-deliverable/spec-gate phrase, 22 additional rows carry a needs-spec label, and 13 more have a spec-prefixed title. These 132 signals are search leads, including 24 parked rows. They do not imply that 132 packs are missing. A dash in the blocker column means no matching historical dependency clause was extracted. Check live issue relationships before dispatch.
- The issue census joins a filename only when its basename or containing directory starts with the same GH issue number. This does not discover semantic coverage under a KEL number, another issue, or documentation outside `docs/specs/`. Search related owners and read the code for any issue marked `inspect-owner-and-source`. Program rows route to their child trackers.
- For a refresh, recapture the main commit, all open issue and PR pages, exact heads and changed filenames. Deduplicate issue numbers, confirm path presence on the relevant commit tree, and investigate any PR head that moved during enumeration. Preserve the previous snapshot in Git history.

Runtime implementation, profile guarantees and device acceptance are recorded by the feature owners. This index carries no compatibility percentage.
