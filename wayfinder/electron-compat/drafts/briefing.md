# Briefing: KELD Electron Compatibility Program — wayfinder map gyldlab/keld#391 (planning only, no implementation)

Date: 2026-10-06. Keld repo: <keld repo> (read-only for you). Electron oracle: v44.4.5 pinned docs/sources
(https://raw.githubusercontent.com/electron/electron/v44.4.5/docs/api/<page>.md, docs/tutorial/<page>.md, shell/...).
This scratchpad (the directory holding this file) contains:
- DOCTRINE.md — the owner's execution doctrine. READ IT. Its operating rules below are binding on you.
- publish_plan.json — the PUBLISHED plan (118 issues: MAP, 14 epics F01..F09/X01..X05-EPIC, 51 tickets F0N-T<n>/X0N-T<n>,
  52 decision tickets F0N-A<n>/D<n>, X0N-A<n>/D<n>, PANEL-D1..D23, PANEL-P1..P3); publish_state.json maps key → GitHub issue number.
- swarm.json — 15 research units (units[].research with decisions, semantic_traps, receipts) and refuter verdicts (units[].verdicts).
- wayfinder/electron-compat/notes/<unit>.md (rendered research), panel/synthesis.md + panel/*.json (eight-persona panel),
  compat-matrix.tsv (2,128 members × family/tier/status/demand), sync-census.md, probes/ (park-probe etc.).
- families/F0N-*.json (Electron v44.4.5 API model split by family), electron-api.json, corpus/<app>/ (drawio-desktop @2edf9fb,
  zettlr @e6c7fd8, electron-quick-start @82dbb35) and corpus/<app>.usage.json (token-scan leads, NOT demand facts).

## Execution doctrine — binding operating rules (full text: DOCTRINE.md)
1. Reconstruct reality first: read the owning source (Keld code/spec at origin/main, pinned Electron doc, corpus clone) before
   asserting anything. Never inherit a claim from this briefing, a research note or another agent without re-opening its source.
2. Label every material statement FACT | INFERENCE | ASSUMPTION | UNKNOWN | BLOCKER. An unknown disguised as fact is a defect.
   When sources disagree, state the contradiction; never average it away.
3. Decompose into decision-bearing atoms: owner, inputs/outputs, trust boundary, process boundary, resource ownership, I/O path,
   lifecycle, failure modes, recovery, observable contract, dependencies, and the independent evidence that could falsify it.
   Make hidden coupling explicit as its own atom or edge.
4. Confront decisions: exact decision, evidence that determines it, real alternatives with cost, new invariant created, existing
   invariant at risk, falsifier, reversibility. If evidence determines the path, choose it. If not, name exactly what is missing.
   A human-owned decision gets a decision packet (options, consequences, recommendation, the one remaining question). No "needs discussion".
5. Blockers are engineering problems: classify (missing local knowledge → inspect owner; reproducible uncertainty → smallest
   discriminating experiment; platform semantics → primary documentation receipt; missing OS/device → exact OS handoff; missing
   approval → finish every authorized analysis). Never answer a blocker with helplessness.
6. Research is a scalpel: Unknown → why it matters → decision it blocks → evidence required. Seek evidence that would DISPROVE the
   preferred conclusion. A research task that must leave this session is a copy-ready Prompt Tracker node in the existing taxonomy
   (node form: <prompt-tracker clone>/docs/06-graph-engineering.md § Node form; model admission docs/04) — never a new prompt system.
7. Reuse before invention; one rule, one owner, one source of truth. Name the existing owner before proposing anything new.
8. YAGNI, mercilessly: "Can the current milestone ship correctly without this?" The current milestone is the FIRST PROOF:
   drawio-desktop on macOS, explicit legacy profile, install + activation + 4-step primary workflow. Anything that milestone does
   not need is parked, not designed in loving detail.
9. Proof: independent oracles, negative controls that fail when the behavior is deleted/inverted, no mocks as OS proof, no sleeps,
   never weaken an oracle. "Probably" is not "verified".
10. Attack the design: ask for the strongest plausible reason it is wrong; look for authority escalation, lifecycle holes,
    authorization/use races, stale state after teardown, bypass paths, oracles sharing the implementation's mistake, and a way to
    delete the abstraction entirely.
11. Protect scope: separate what must change here from what was merely discovered here; park the rest with an exact next route.
12. Closure: a precise blocker beats a fake pass. Never manufacture Done.

## Non-negotiable Keld invariants (root AGENTS.md)
Four uniques only (prebuilt Rust host; supervised Bun roles with zero ambient OS authority in strict; kipc; generated host-enforced
default-deny) — never a fifth. Default-deny is sacred; a missing Electron permission handler is a deny. No Electron-isms in
keld-core/keld-ipc. No second process model, transport, schema, parser or policy owner. Performance claims need attributed
measurement (arch 01 §5/§5.1); measured / target / projection are never blended. Features need an approved spec + Linear owner.
Compat lands a conformance entry (citing the pinned Electron doc sentence) before implementation; ordering is tested; negative
controls required. Claims only as `{passed}/{N} of {panel} corpus {id}@{digest} ({kind})`; no percentage without a committed denominator.

## Decisions already resolved on the map (do not re-litigate; attack only with new primary evidence)
Three-seam contract clone (@keld/electron facade over @keld/api; keld-compat host emulation behind keld-guard; renderer compat
script in a non-page content world). The Bun kipc client cannot block today; a parked Bun main thread stalls the link after 8 KiB
(probe); worker-owned single-link transport is the only candidate for sync emulation; a second link per role is refused (second
principal). Close/quit vetoes are async AppKit hooks; the host never auto-closes on timeout. Tombstone flips before `closed`.
Boot-static values must be synchronous at import. sendSync stays a deadline-bounded blocking CALL, SCAFFOLDED until the WKWebView
sync-XHR experiment passes. Renderer→main boundary = enumerated per-window `el:<channel>` grant (never `el:*`). Updater: setFeedURL
recorded no-op, never thrown. webRequest listeners = recorded no-ops only after host file:// containment + CSP injection with a
negative control. First proof = drawio-desktop on macOS under explicit legacy profile (authority_profile LegacySandboxOff).
Prior GitHub decisions: #312 (native slice → migration proof → distribution), #313 (draw.io first, Zettlr second), #319 (migration
proof deferred from the spine map; #391 is that effort), #322/#323/#325 (spine routing; consume task-level artifacts).
Linear owners to consume, never duplicate: KEL-75/76/77/78/79/80/53/74/102/135/139–144, KEL-237, KEL-215, KEL-89, KEL-90/129, KEL-15/17/19.

## Corpus facts (resolved call sites)
draw.io: 0 sendSync, `<webview>` disabled everywhere, no native addons, one `dialog.showMessageBoxSync` on the close path, preload
uses contextBridge + ipcRenderer.send/on/once, electron-store constructed at module top level, electron-updater with setFeedURL
outside the disable guard, `disableBlinkFeatures` on every window, file:// loads + two session.webRequest listeners.
Zettlr: 186 ipcRenderer.invoke, 49 webContents.send, 6 sendSync, protocol.handle('safe-file'), sandbox:false ×2, nodehun/chokidar.

## Rules of engagement
Do NOT write to the Keld repo, GitHub or Linear. Write only inside this scratchpad (prototypes/, notes/). Return only the
structured object asked for. Ticket text is behavioral and durable: name contracts/observables, never file paths or line numbers.
Record a receipt (source + retrieved 2026-10-06 + exact claim) for every external semantic you rely on.
