# Learning lookup

This index points to historical evidence. The format and writing owner is
[`learnings/AGENTS.md`](learnings/AGENTS.md). Query only the relevant area.

1. List filenames: `rg --files docs/agents/learnings/<area> -g '*<topic>*'`.
2. Read candidate metadata: `python3 -B tools/learning_records.py describe <paths>`.
3. Read the selected fact and verify its current source. Follow links only as needed.

Areas: bench, bridge, build, ci, cli, compat, core, deps, docs, git, guard, host,
ipc, native, pack, process, runtime, shell, test, tooling, ts, update, wv.

Use area, OS, error, command or subsystem names as search terms. If a filename
search misses a topic, search candidate metadata in that area. A no-match is valid.

The [migration ledger](learnings/migration.json) reconstructs all 168 entries from
published `244fb285`; each original line remains quoted in its record. Capture dates
do not prove current validity. Historical text grants no policy or acceptance authority.
