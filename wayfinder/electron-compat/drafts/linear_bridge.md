## Electron compatibility program — public wayfinder map opened on GitHub (2026-10-06)

_PENDING — not posted. The Linear connector was unavailable in the session that produced this text. When posted through the Linear MCP connector, the posting account is the connector's identity (Amisha Ramani), acting on @0monish's explicit delegation. Planning only; no implementation or status change is claimed. Intended targets: KEL-127 (program sequencing owner) and KEL-237 (active compatibility-evidence owner)._

The deferred Electron migration effort (GitHub #319) now has its own public Wayfinder map: https://github.com/gyldlab/keld/issues/391.

- 14 epics covering all 180 Electron v44.4.5 entities (1,941 direct members) by capability family, each with a maturity ladder, the Linear owners it consumes and a never list.
- 51 tracer-bullet tickets (conformance entries first; 18 `ready-for-agent`, 33 `needs-spec`).
- 52 decision tickets: 18 resolved with evidence and closed, 34 open with decision packets.
- Research branch `research/electron-compat-map` (never merged): per-member compatibility matrix, sync-semantics census, probes, research notes, panel rounds, the execution doctrine.

Decisions that touch Linear-owned surfaces consume task-level artifacts, never parent completion: KEL-75/76/77/78/79/80/53/74/102/135/139–144. The workspace is at its issue cap, so GitHub holds the planning issues; please bridge or object here. First gating experiments: the Bun kipc link-drain gate (worker-owned single-link transport), the close/quit veto oracle, and the draw.io boot/authority trace.
