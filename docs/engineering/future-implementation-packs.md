# Future implementation packs

Snapshot for [GH-1414](https://github.com/gyldlab/keld/issues/1414) on 6 November 2026. Main is [`0eb3ce203187`](https://github.com/gyldlab/keld/commit/0eb3ce203187f35c5276a4237189c906a5220d5a). This is a dispatch list for specification and review. The owning issue and approved contract control implementation.

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

### Complementary work in progress at snapshot

These five issue claims extend the original nine. Start migration, IPC and the Linux proof in parallel. The Linux proof design can proceed while its installation and runtime dependencies are still open. Live publication receipts belong on the feature issue and the [roll-up tracker](https://github.com/gyldlab/keld/issues/1414).

| Owner | Pack | First independent check | Inputs to reconcile |
|---|---|---|---|
| [#591](https://github.com/gyldlab/keld/issues/591) Reversible migration writes | Generated files, manifest edits and undo. | Compare against the hand-authored artifact. Introduce a later user edit before undo. | [#498](https://github.com/gyldlab/keld/issues/498), [#499](https://github.com/gyldlab/keld/issues/499), [#500](https://github.com/gyldlab/keld/issues/500) |
| [#547](https://github.com/gyldlab/keld/issues/547) Electron invoke and handle | Renderer promise and main-process handler registry. | Reverse two replies and throw from a handler. Confirm the right caller receives only the permitted result/error. | [#546](https://github.com/gyldlab/keld/issues/546), [#466](https://github.com/gyldlab/keld/issues/466), [#467](https://github.com/gyldlab/keld/issues/467) |
| [#1129](https://github.com/gyldlab/keld/issues/1129) Installed Linux draw.io proof | Product evidence for an installed artifact. | Substitute a preview launcher, inert fixture or different digest. Each must fail proof admission. | [#500](https://github.com/gyldlab/keld/issues/500), [#587](https://github.com/gyldlab/keld/issues/587), [#1124](https://github.com/gyldlab/keld/issues/1124), [#1406](https://github.com/gyldlab/keld/issues/1406) |
| [#555](https://github.com/gyldlab/keld/issues/555) Legacy network status | Synchronous connectivity hint used by the Zettlr updater. | Read both APIs before/after Ready. Distinguish offline from an unconfirmed route. | [#554](https://github.com/gyldlab/keld/issues/554) |
| [#550](https://github.com/gyldlab/keld/issues/550) Dropped-file path | Host-captured file identity for the Zettlr renderer. | Drop a real file and construct a same-name File. Only the original may disclose its path. | [#467](https://github.com/gyldlab/keld/issues/467) |

### Original nine claimed packs

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

### Next triage

These rows are candidates for another dispatch. Check current code, related PR contents and the live owner before claiming. The final column contains related owners as well as dependencies.

| Owner | Future deliverable | First action on GitHub | Related |
|---|---|---|---|
| [#592](https://github.com/gyldlab/keld/issues/592) | Analyzer blocker classification | Reuse the scan rules in PR #1410. Examine an unresolved native addon against the owning evidence table before proposing another rule. | [#498](https://github.com/gyldlab/keld/issues/498), [#590](https://github.com/gyldlab/keld/issues/590) |
| [#549](https://github.com/gyldlab/keld/issues/549) | Context bridge return values | Map synchronous values, returned functions, promises and errors to the same isolated-world boundary. Check the approved value table first. | [#399](https://github.com/gyldlab/keld/issues/399), [#467](https://github.com/gyldlab/keld/issues/467) |
| [#535](https://github.com/gyldlab/keld/issues/535) | Initially hidden window | Trace ready-to-show versus first paint against the pinned Electron recording and registry. Reuse existing window ownership. | [#394](https://github.com/gyldlab/keld/issues/394), [#449](https://github.com/gyldlab/keld/issues/449), [#533](https://github.com/gyldlab/keld/issues/533) |
| [#558](https://github.com/gyldlab/keld/issues/558) | Display and theme updates | Reuse the host display/theme mirror in PR #1164. Inspect event fan-out and removal across two windows. | [#481](https://github.com/gyldlab/keld/issues/481) |
| [#561](https://github.com/gyldlab/keld/issues/561) | Synchronous dialogs | Inspect existing dialog and blocking-call contracts, including nested close behavior, before proposing another sync surface. | [#478](https://github.com/gyldlab/keld/issues/478), [#418](https://github.com/gyldlab/keld/issues/418), [#528](https://github.com/gyldlab/keld/issues/528) |
| [#578](https://github.com/gyldlab/keld/issues/578) | Child-process termination | Keep named Bun-role death distinct from renderer termination in #488. Check attribution and terminal-cause vocabulary. | [#508](https://github.com/gyldlab/keld/issues/508) |
| [#1355](https://github.com/gyldlab/keld/issues/1355) | Zettlr filesystem watcher qualification | Record boot, change and unlink under the declared profile before attributing an engine or compatibility gap. | [#1358](https://github.com/gyldlab/keld/issues/1358) |
| [#1405](https://github.com/gyldlab/keld/issues/1405) | Windows installed packaging | Design owner already exists. Reconcile the Windows install layout with the platform tracker before commissioning a spec. | [#1182](https://github.com/gyldlab/keld/issues/1182) |
| [#1406](https://github.com/gyldlab/keld/issues/1406) | Linux installed packaging | Identify install root, artifact identity and uninstall ownership here. Send runtime evidence to #1129. | [#1124](https://github.com/gyldlab/keld/issues/1124), [#1129](https://github.com/gyldlab/keld/issues/1129) |

## Reproduce the inventory

- [Open issue census](future-implementation-issues.tsv) contains 397 unique open issues, including the inventory issue itself. The initial tracker recorded 396 before #1414 was created. The input was every page of the repository's open issues endpoint, with PRs removed.
- [Spec evidence and PR coverage](future-implementation-specs.tsv) records 43 Markdown paths on main and 133 changed Markdown occurrences present at the heads of 179 open PRs. Stacked PRs can contain the same path. The scan compares changed filenames under `docs/specs/` with the immutable Git tree of each relevant head.
- One changed path is absent at its pinned head. PR #1325 splits the flat KEL-142 spec into a directory. The old path has an explicit absence row. Open PRs without changed spec Markdown have a coverage row and a head pin. A spec inherited unchanged from an open PR's base can fall outside that PR's changed paths.
- The issue census joins a filename only when its basename or containing directory starts with the same GH issue number. This does not discover semantic coverage under a KEL number, another issue, or documentation outside `docs/specs/`. Search related owners and read the code for any issue marked `inspect-owner-and-source`. Program rows route to their child trackers.
- For a refresh, recapture the main commit, all open issue and PR pages, exact heads and changed filenames. Deduplicate issue numbers, confirm path presence on the relevant commit tree, and investigate any PR head that moved during enumeration. Preserve the previous snapshot in Git history.

Runtime implementation, profile guarantees and device acceptance are recorded by the feature owners. This index carries no compatibility percentage.
