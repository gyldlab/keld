# KELD error codes

Canonical registry for `docs/architecture/07-agent-experience.md` §2.
This file **is** the docs stub: one `## KELD-…` heading per code. There is no
error website in v0; `KeldErrorObject.docs` still uses the
`https://keld.dev/e/<code>` URL shape for agents.

CI: `crates/keld-cli/tests/error_registry.rs` (runs with workspace nextest).

- Duplicate `## KELD-…` headings fail the test.
- A `KELD-*` code in `keld-ipc` / `keld-wv` / `keld-cli` / `keld-guard` `src`,
  `keld-cli` templates, or workspace `tools/` that has no heading here fails the test.
- A heading here that is not emitted in those trees fails the test.
- Every entry MUST have non-empty `crate`, `message`, and `fix` lines.

Hyphenated (`KELD-CLI-020`) and compact (`KELD-MCP001`) forms are both valid;
match the crate that already emits the code. Do not invent a third spelling.

## Adding a code

1. Pick the next free number in that area (do not reuse).
2. Add a heading + the three fields below.
3. Emit the same code from `Display` / `KeldErrorObject`, with the `fix` in the
   message.
4. Run `cargo nextest run -p keld-cli -- error_registry`.

## KELD-IPC-001

- crate: keld-ipc
- message: I/O error on the kipc control plane
- fix: Check the app-link path is live and the peer has not closed the socket.

## KELD-IPC-002

- crate: keld-ipc
- message: Bad kipc frame header
- fix: Peers must speak kipc v1 with magic 'KI'.

## KELD-IPC-003

- crate: keld-ipc
- message: Postcard codec failure on a kipc payload
- fix: Payload must be postcard for the channel's schema type.

## KELD-IPC-004

- crate: keld-ipc
- message: Frame payload exceeds MAX_FRAME_LEN
- fix: Shrink the payload or move large transfers to the bulk plane (shm / keld://).

## KELD-IPC-005

- crate: keld-ipc
- message: Unexpected kipc frame or session state
- fix: Check frame kind and channel match the session contract.

## KELD-IPC-006

- crate: keld-ipc
- message: App-link I/O deadline exceeded
- fix: Check the peer is still running and sending kipc frames; a silent or wedged process will not be waited on forever.

## KELD-WV-001

- crate: keld-wv
- message: No webview backend for this OS
- fix: Track KEL-27 (Windows) / KEL-28 (Linux) or run on macOS.

## KELD-WV-002

- crate: keld-wv
- message: Window creation failed
- fix: Check display permissions and that a window server is available.

## KELD-WV-003

- crate: keld-wv
- message: Webview creation failed
- fix: On macOS ensure WKWebView is available (10.13+).

## KELD-WV-004

- crate: keld-wv
- message: Event loop error
- fix: Check the window server is running and the event loop was started on the UI thread.

## KELD-WV-005

- crate: keld-wv
- message: Navigation (load HTML/URL) failed
- fix: Check the target URL scheme and that the webview still exists.

## KELD-WV-006

- crate: keld-wv
- message: Script evaluation failed
- fix: Verify the script parses and the webview finished creating.

## KELD-WV-007

- crate: keld-wv
- message: Unknown webview id
- fix: Create one with `WebEngine::create` and drop stale ids after `destroy`.

## KELD-CLI-010

- crate: keld-cli
- message: KELD_APP_LINK is unset in the app process
- fix: run the app with `keld dev`, not `bun` directly.

## KELD-CLI-020

- crate: keld-cli
- message: Invalid project name
- fix: Use lowercase letters, numbers, and hyphens.

## KELD-CLI-021

- crate: keld-cli
- message: Target directory already exists
- fix: Choose another name or remove the folder.

## KELD-CLI-022

- crate: keld-cli
- message: Failed to write the hello template
- fix: Check the parent path exists and is a writable directory.

## KELD-CLI-030

- crate: keld-cli
- message: Dev session I/O error
- fix: Check that `bun` is on PATH and the project files are readable.

## KELD-CLI-031

- crate: keld-cli
- message: Dev session failed
- fix: Re-run `keld doctor` and fix the reported checks.

## KELD-CLI-032

- crate: keld-cli
- message: Environment checks failed
- fix: Fix the failed doctor checks listed in the message.

## KELD-CLI-033

- crate: keld-cli
- message: Bun runtime not found on PATH
- fix: install Bun from https://bun.sh and ensure `bun` is on PATH

## KELD-CLI-034

- crate: keld-cli
- message: Project layout incomplete
- fix: missing keld.config.ts or src/main.ts — run `keld create <name>` first

## KELD-CLI-035

- crate: keld-cli
- message: Cannot load renderer
- fix: Set `renderer` in keld.config.ts to a project-relative HTML file (no `..` or absolute paths) and create it.

## KELD-CLI-040

- crate: keld-cli
- message: Missing --link for ipc-client echo
- fix: set KELD_APP_LINK from `keld dev`

## KELD-CLI-041

- crate: keld-cli
- message: --message requires a value
- fix: Pass `--message <text>`.

## KELD-CLI-042

- crate: keld-cli
- message: --count missing or not a u32
- fix: Pass `--count <u32>`.

## KELD-CLI-043

- crate: keld-cli
- message: Unknown ipc-client echo flag
- fix: Use only `--link`, `--message`, and `--count`.

## KELD-CLI-044

- crate: keld-cli
- message: Unknown CLI flag
- fix: Use only the flags that verb documents (`keld doctor --json`; `keld create <name>` / `keld hello` / `keld dev` take none besides create's name).

## KELD-CLI-045

- crate: keld-cli
- message: Reserved CLI verb is not implemented
- fix: Use `keld create <name>` then `keld dev` (Phase 2). Track KEL-17 (migrate), KEL-19 (build/ext), or KEL-13 (gen).

## KELD-CLI-046

- crate: keld-cli
- message: Unknown CLI command
- fix: Use a live verb: create, dev, doctor, mcp, hello, ipc-echo, ipc-client.

## KELD-MCP001

- crate: keld-cli
- message: Failed to start the MCP tokio runtime
- fix: Reinstall the `keld` binary or report a bug.

## KELD-MCP002

- crate: keld-cli
- message: MCP stdio session ended with error
- fix: Ensure the client speaks MCP over stdio and keeps stdin open.

## KELD-MCP010

- crate: keld-cli
- message: Permissions manifest not found
- fix: create keld.permissions.jsonc at the tried path (expected file name: keld.permissions.jsonc)

## KELD-MCP011

- crate: keld-cli
- message: Permissions manifest is not valid JSONC
- fix: Fix the JSON (comments are allowed; trailing commas are not).

## KELD-MCP012

- crate: keld-cli
- message: Unknown principal for permissions explain
- fix: v0 evaluate only supports principal "app"

## KELD-MCP013

- crate: keld-cli
- message: Permissions manifest exists but cannot be read
- fix: Check that the path is a readable file (not a directory) and retry.

## KELD-MCP014

- crate: keld-cli
- message: Channel grants are not evaluated in v0
- fix: omit `channel` — v0 evaluate covers app path/host scopes only

## KELD-MCP020

- crate: keld-cli
- message: Failed to serialize doctor findings
- fix: Re-run `keld doctor` without --json or report a bug.

## KELD-GUARD001

- crate: keld-guard
- message: Capability is not granted
- fix: Add a grant for that capability in keld.permissions.jsonc.

## KELD-GUARD002

- crate: keld-guard
- message: Capability denied by scope
- fix: Widen that grant's scope in keld.permissions.jsonc so it includes the requested path.

## KELD-GUARD003

- crate: keld-guard
- message: Channel is not granted to this principal
- fix: Add the channel to this principal's channels list in keld.permissions.jsonc.

## KELD-GUARD004

- crate: keld-guard
- message: Permissions manifest not found or unreadable
- fix: Create keld.permissions.jsonc at that path.

## KELD-GUARD005

- crate: keld-guard
- message: Permissions manifest is not valid JSONC
- fix: Fix the JSON (comments are allowed; trailing commas are not).

## KELD-DOCS001

- crate: llms-docs
- message: Required documentation source is missing
- fix: Restore the source or remove its entry from the generator source list.

## KELD-DOCS002

- crate: llms-docs
- message: Required documentation source is empty
- fix: Add authoritative Markdown content or remove its source-list entry.

## KELD-DOCS003

- crate: llms-docs
- message: Documentation source is outside the authoritative corpus
- fix: Remove it from the source list and include only reviewed public Markdown.

## KELD-DOCS004

- crate: llms-docs
- message: Generated documentation output is missing or stale
- fix: Run `just llms` and commit both generated outputs.

## KELD-DOCS005

- crate: llms-docs
- message: Documentation generator invocation or file I/O failed
- fix: Follow the command-specific fix, correct the path or permissions, and rerun the generator.
