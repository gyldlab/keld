# Keld Onboarding

Human-facing orientation for an engineer joining this repo. Narrative, not rules — the
binding rules live in [`AGENTS.md`](../../AGENTS.md) and the normative specs live in
[`docs/architecture/`](../architecture/). Everything here is traceable to a file in this
tree; where something is specified but not built, it says so and names the source.

Written 2026-08-10 against commit `6d642c4` plus the uncommitted working tree.

| # | Document | Read it when |
|---|---|---|
| 01 | [What Keld is, and where it actually stands](01-project-summary.md) | First. The thesis, the competitive position, the perf budgets, and an honest spec-vs-code ledger. |
| 02 | [Architecture guide](02-architecture-guide.md) | Before touching any crate. Three-principal trust model, end-to-end request flow, all 11 crates with real status. |
| 03 | [API and CLI surface](03-api-and-cli-surface.md) | When you need to *use* Keld. Every implemented `keld` verb with real output, the public Rust surface, and what the README promises that doesn't exist yet. |
| 04 | [Wire formats and contracts](04-wire-formats-and-contracts.md) | When you touch kipc, config files, or anything on the wire. Byte-level frame layout, handshake, codec, error taxonomy. |
| 05 | [Development guide](05-development-guide.md) | Day one setup, and every day after. Prerequisites, the three-command verification gate, review gates, PR conventions, troubleshooting. |
| 06 | [Documentation map](06-documentation-map.md) | When you're lost. Every document in the repo, what's normative vs exploratory, and a reading order. |

## The short version

Keld replaces Electron's architecture, not its API: a prebuilt Rust host owns every OS
resource, the developer's JS/TS main process runs on a supervised Bun child with zero
ambient OS authority, UI runs in system webviews, and the two sides talk over a typed
binary IPC plane (kipc) behind a default-deny capability manifest.

**It is pre-alpha and the gap between the specs and the code is large.** The architecture
documents describe a finished framework; the tree holds roughly 2,300 lines of Rust that
open one macOS window and echo one IPC message. Both are true at once. Doc 01 quantifies
the gap crate by crate — read it before you trust anything the specs imply about what
works today.

## Day one

```bash
cargo build --workspace
cargo nextest run --workspace --profile ci
just hello                                   # macOS only: opens the WKWebView window
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
