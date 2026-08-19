# keld-compat — adds root AGENTS.md

Spec: `docs/architecture/04-electron-compat.md`.

- Electron documented behavior = oracle. Conformance entry (citing doc/fixture) *before* implementation.
- Divergence explicit: `keld.compat.ts` quirks flag OR scoreboard ▲/✘, chosen in PR with report wording.
- Event ordering tested (sequences, not just outcomes): `ready` → `window-all-closed` → `before-quit`, etc.
- No Electron-isms in `keld-core`/`keld-ipc`; compat pressure stays here via quirks flags.
- Corpus score = release gate; any score drop = P1 regression.
- Tests MUST follow repository `.agents/testing.md`.
- `evidence` is framework-generic (KEL-74). Agents MUST NOT add VS Code
  package names, Electron API tables, or a percentage/`complete` that lacks a
  committed product corpus. Opaque turn citations and sandbox paths are leads,
  not evidence. RFC1918, link-local, and unique-local hosts are not public
  https evidence.
