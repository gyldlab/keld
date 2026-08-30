# Keld Onboarding

Human-facing orientation for an engineer joining this repo. Narrative, not rules — the
binding rules live in [`AGENTS.md`](../../AGENTS.md) and the normative specs live in
[`docs/architecture/`](../architecture/). Everything here is traceable to a file in this
tree; where something is specified but not built, it says so and names the source.

This set avoids frozen LOC/test/HEAD counts. “Current” means the checked-out source plus
fresh gate and target-host evidence, not the date this prose was last edited.

| # | Document | Read it when |
|---|---|---|
| 01 | [What Keld is, and where it actually stands](01-project-summary.md) | First. The thesis, the competitive position, the perf budgets, and an honest spec-vs-code ledger. |
| 02 | [Architecture guide](02-architecture-guide.md) | Before touching any crate. Three-principal trust model, end-to-end request flow, all 11 crates with real status. |
| 03 | [API and CLI surface](03-api-and-cli-surface.md) | When you need to *use* Keld. Every implemented `keld` verb with real output, the public Rust surface, and what the README promises that doesn't exist yet. |
| 04 | [Wire formats and contracts](04-wire-formats-and-contracts.md) | When you touch kipc, config files, or anything on the wire. Byte-level frame layout, handshake, codec, error taxonomy. |
| 05 | [Development guide](05-development-guide.md) | Day one setup, and every day after. Prerequisites, the `just ci` gate and core Rust subset, review gates, PR conventions, troubleshooting. |
| 06 | [Documentation map](06-documentation-map.md) | When you're lost. Every document in the repo, what's normative vs exploratory, and a reading order. |
| 07 | [Use Keld from an MCP client](07-mcp-server.md) | When registering Keld's shipped local, read-only, three-tool MCP server. |
| 08 | [Optional agent memory for Keld contributors](08-optional-agent-memory.md) | Only when evaluating the external, opt-in KEL-67 contributor pilot. It is not a Keld product feature. |

## The short version

Keld replaces Electron's architecture while preserving a measured API contract: a
prebuilt Rust host owns every OS resource; the developer's JS/TS primary process and
any named compatibility roles run as supervised Bun principals whose destination strict
profile has zero undeclared ambient OS authority; UI runs in system webviews; and the sides talk over a
typed binary IPC plane (kipc) behind a default-deny capability manifest.

**It is pre-alpha and the gap between the specs and the code is large.** The architecture
documents describe the approved destination; the current tree contains CLI/MCP, guard,
framed-echo and three platform hello slices, while the supervisor, brokers, packages,
compatibility and distribution systems remain incomplete. Doc 01 records the gap by
observable behavior — read it before treating a target contract as live.

## Day one

```bash
cargo build --workspace
cargo nextest run --workspace --profile ci
just hello                                   # launches the current platform hello backend
cargo run -p keld-cli -- doctor
```

Then the verification gate that must pass before anything is "done" — see
[05 §3](05-development-guide.md):

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --profile ci
```

## Agent-readable documentation

Tracked authoritative docs are projected into [`llms.txt`](../../llms.txt) and
[`llms-full.txt`](../../llms-full.txt). The compact index defines the exact corpus;
exploratory research and local-only material are excluded. After changing an included
source, run `just llms` and verify it with `just llms-check`.
