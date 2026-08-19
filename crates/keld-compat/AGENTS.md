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
  not evidence. RFC1918, CGNAT (`100.64.0.0/10`), NAT64 well-known prefix
  (`64:ff9b::/96`), 6to4 (`2002::/16`), link-local, and unique-local hosts are
  not public https evidence (including a trailing FQDN dot, IPv4-mapped,
  IPv4-compatible `::a.b.c.d`, and IPv4-translated `::ffff:0:a.b.c.d`). Host
  checks are literal (no DNS). Abbreviated IPv4 (`127.1`, `010.0.0.1`, `1.1`)
  and DNS-to-private (`nip.io`) are T1 residuals.
