# Electron compatibility wayfinder — research branch

Throwaway research branch (never merged) holding the primary sources behind the GitHub wayfinder map
[#391](https://github.com/gyldlab/keld/issues/391) for the KELD Electron Compatibility Program (Electron v44.4.5 baseline). Generated 2026-10-06 by an AI agent
(Claude Code, Fable 5.1) on behalf of @0monish; every claim is labeled fact/inference/unknown by its author and
decision-bearing atoms were reviewed by two independent refuters.

| Artifact | What it is |
|---|---|
| `compat-matrix.tsv` | Per-member compatibility matrix: 2,128 raw members (1,941 direct) × entity, kind, process, platforms, tags, inherited-from, family, tier (arch 04 §4), Keld status, subsystem, Linear owners, corpus demand (draw.io / Zettlr / quick-start), GitHub ticket |
| `electron-api-v44.4.5.sha256` | Digest of the Electron release artifact `electron-api.json` the matrix was derived from |
| `families/*.json` | The API model split into nine capability families |
| `corpus-usage/*.usage.json` + `scan_electron_usage.py` | Static Electron API demand scan of the three corpus apps (prototype of the `keld migrate` analyzer) |
| `notes/F0N-*.md`, `notes/X0N-*.md` | Fifteen research units (nine families + six cross-cutting) with semantic traps, security mapping, decisions and refuter verdicts |
| `panel/` | Eight-persona perspective panel: isolated positions, cross-critiques, three judges, synthesis |
| `prototypes/*.html` | Four single-file logic prototypes (open by double-click): window lifecycle, IPC bridge, migrate report, compat-matrix cell |
| `critic.json` | Completeness critic output over all ticket drafts |

Rules: nothing here is normative. Keld code, approved specs, and live GitHub/Linear state outrank this branch.

| `probes/` | Agent-run Bun/Swift probes behind the panel's findings (park-probe: parked main thread stalls the kipc link after 8 KiB; sync-probe; alias/bunfig/file-alias probes; WK scheme probe source) |
| `drafts/` | The hand-drafted ticket plan (`drafts_part*.py`, `publish_plan.json`) and the merged research/verdict corpus (`swarm.json`) |
| `map.md` | The map body as published |
| `sync-census.md` | Which corpus call sites return synchronously in Electron (the sync-semantics kill-condition census) |
