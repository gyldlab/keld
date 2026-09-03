# KELD error codes

Canonical registry for `docs/architecture/07-agent-experience.md` §2.
This file **is** the docs stub: one `## KELD-…` heading per code. There is no
error website in v0; `KeldErrorObject.docs` still uses the
`https://keld.dev/e/<code>` URL shape for agents.

CI: `crates/keld-cli/tests/error_registry.rs` (runs with workspace nextest).

- Duplicate `## KELD-…` headings fail the test.
- A `KELD-*` code in `keld-ipc` / `keld-wv` / `keld-cli` / `keld-guard` /
  `keld-runtime` / `keld-native` / `keld-compat` / `keld-core` `src`, `keld-cli` templates, or workspace
  `tools/` that has no heading here fails the test.
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
- fix: Peers must speak kipc v2 with magic 'KI'.

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

## KELD-IPC-007

- crate: keld-ipc
- message: HELLO session token rejected
- fix: Mint the token with the host (`keld dev`) into KELD_APP_LINK as `<endpoint>#<64 hex chars>` and send those exact 32 bytes as the HELLO payload. A wrong HELLO semantic shape is `KELD-IPC-005`; `KELD-IPC-007` is reserved for invalid bootstrap token text or an exactly shaped foreign token.

## KELD-WV-001

- crate: keld-wv
- message: No webview backend for this OS
- fix: Track the named issue, or run on macOS, Windows, or Linux — all three have live backends since KEL-28 (2026-08-16).

## KELD-WV-002

- crate: keld-wv
- message: Window creation failed
- fix: Check display permissions and that a window server is available.

## KELD-WV-003

- crate: keld-wv
- message: Webview creation failed
- fix: On macOS ensure WKWebView is available (10.13+); on Windows ensure the WebView2 runtime is installed.

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

## KELD-WV-008

- crate: keld-wv
- message: WebView2 runtime unavailable
- fix: Install the Evergreen Runtime from https://developer.microsoft.com/microsoft-edge/webview2/ and re-run. Keld will not download or execute an installer for you.

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

## KELD-CLI-047

- crate: keld-cli
- message: Owner-private no-flag boot staging failed
- fix: Fix the named project input or host-copy integrity failure, then generate a fresh dev stage.

## KELD-CLI-048

- crate: keld-cli
- message: The delegated staged host exited unsuccessfully
- fix: Fix the preceding host diagnostic, then re-run `keld dev`.

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
- message: Permissions manifest is ambiguous, malformed, or exceeds 64 KiB
- fix: Remove duplicate keys, fix the JSONC, or reduce the file to 64 KiB or less.

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
- message: Permissions manifest is not UTF-8, is ambiguous, or is not valid JSONC
- fix: Write UTF-8, remove duplicate object keys, or fix the JSON (comments are allowed; trailing commas are not).

## KELD-GUARD006

- crate: keld-guard
- message: v0 evaluate does not apply app grants to this principal
- fix: Do not apply `/app` scopes to a webview or plugin principal; window-level grants are not in this slice.

## KELD-GUARD007

- crate: keld-guard
- message: Camera/microphone requires a minted webview principal
- fix: Mint the requesting webview principal before evaluating the capability. Do not present AppProcess.

## KELD-GUARD008

- crate: keld-guard
- message: Required strict-profile OS primitive is unavailable
- fix: Do not start a child and do not restart unsandboxed; keep requested=Strict or declare explicit Keld legacy.

## KELD-GUARD009

- crate: keld-guard
- message: Unexpected grant while requesting Strict
- fix: Do not start. Electron sandbox/appSandbox is not the Keld profile key; do not inherit the host sandbox.

## KELD-GUARD010

- crate: keld-guard
- message: Host handle would leak into the Strict child
- fix: Strip inherited handles to app-link + log sinks, then retry admission. Do not start.

## KELD-GUARD011

- crate: keld-guard
- message: Strict requested but the OS-containment proof is missing
- fix: Do not start a child and do not restart unsandboxed; keep requested=Strict or declare explicit Keld legacy.

## KELD-GUARD012

- crate: keld-guard
- message: Strict proof archive is stale for this artifact or policy generation
- fix: Do not start a child and do not restart unsandboxed; keep requested=Strict or declare explicit Keld legacy.

## KELD-GUARD013

- crate: keld-guard
- message: Strict proof identity does not match this spawn
- fix: Do not start a child and do not restart unsandboxed; keep requested=Strict or declare explicit Keld legacy.

## KELD-GUARD014

- crate: keld-guard
- message: Strict OS-containment catalog is incomplete
- fix: Do not start a child and do not restart unsandboxed; keep requested=Strict or declare explicit Keld legacy.

## KELD-GUARD015

- crate: keld-guard
- message: Strict archive row used the wrong layer or oracle (Display names recorded oracle, not only layer)
- fix: Do not start a child and do not restart unsandboxed; keep requested=Strict or declare explicit Keld legacy.

## KELD-GUARD017

- crate: keld-guard
- message: Permissions manifest exceeds the 64 KiB input limit
- fix: Reduce keld.permissions.jsonc to 64 KiB or less and retry.

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

## KELD-DOCS006

- crate: Mermaid documentation tools
- message: Mermaid documentation policy or pinned SVG rendering failed
- fix: Add the required accessibility metadata and canonical palette, then run `just mermaid-test`, `just mermaid-check`, and `just mermaid-render-check` with Docker available.

## KELD-RUNTIME-001

- crate: keld-runtime
- message: The supervisor's first spawn of the app-process child failed
- fix: Check that `bun` is on PATH and re-run `keld doctor`.

## KELD-RUNTIME-002

- crate: keld-runtime
- message: The app-process child crashed repeatedly; the crash-loop breaker tripped
- fix: Fix the crash shown in the captured stderr, then re-run `keld dev`.

## KELD-RUNTIME-003

- crate: keld-runtime
- message: Prepared child lifecycle provisioning or revocation failed
- fix: Check the role bootstrap endpoint and its owner-only directory, then retry `keld dev`.

## KELD-RUNTIME-004

- crate: keld-runtime
- message: Virtual port end is not owned by the presented principal
- fix: Use the capability minted for the live role generation.

## KELD-RUNTIME-005

- crate: keld-runtime
- message: Virtual port principal generation is stale
- fix: Provision a fresh role generation before routing or transferring ports.

## KELD-RUNTIME-006

- crate: keld-runtime
- message: Virtual port end is closed or its pair was revoked
- fix: Create a new host-owned pair for the live role generations.

## KELD-RUNTIME-007

- crate: keld-runtime
- message: Virtual port transfer target is the current owner
- fix: Choose a different authenticated role generation.

## KELD-RUNTIME-008

- crate: keld-runtime
- message: Virtual port end was already transferred once
- fix: Port transfer is one-shot per end generation.

## KELD-RUNTIME-009

- crate: keld-runtime
- message: Original virtual port owner cannot transfer after relinquishing the end
- fix: Use the current owner's capability.

## KELD-RUNTIME-010

- crate: keld-runtime
- message: Virtual port queue is full
- fix: Drain the peer or close the end before sending more.

## KELD-RUNTIME-011

- crate: keld-runtime
- message: Virtual port message exceeds inline length limit
- fix: Split the payload or use a later bulk lane when available.

## KELD-RUNTIME-012

- crate: keld-runtime
- message: A supervised app-process generation self-terminated without tripping the crash-loop breaker, including status zero
- fix: Apply the owning session policy. The no-flag host tears down on an unrequested status-zero exit; an accepted correlated Quit or completed windowless work may accept status zero. For non-zero or signal termination, fix the captured stderr and relaunch the owning session.

## KELD-RUNTIME-013

- crate: keld-runtime
- message: The private macOS host-death guardian exited while the host session was still live
- fix: Confirm the registered-group fail-safe completed; restart the host session, diagnose the guardian exit, then relaunch the app.

## KELD-RUNTIME-014

- crate: keld-runtime
- message: Windows host-death Job installation failed before child creation
- fix: Stop before spawning Bun, verify nested Job support and exact non-breakaway `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` configuration, then retry.

## KELD-RUNTIME-015

- crate: keld-runtime
- message: Windows zero-capability LPAC admission failed
- fix: Do not start an unconfined replacement. Repair the AppContainer profile, reviewed path ACLs, explicit environment, or private inherited-handle allowlist and refresh the hostile proof.

## KELD-RUNTIME-016

- crate: keld-runtime
- message: Linux strict-profile construction, namespace, mount, seccomp, Landlock, FD isolation, readiness, or target exec failed
- fix: Do not start an uncontained replacement. For an unsupported architecture or unavailable unprivileged user namespaces, move the workload to a supported x86_64 host with unprivileged user namespaces. Otherwise install or repair the reviewed Bubblewrap and Keld launcher artifacts and the exact runtime-file, mount, seccomp, enabled-Landlock, readiness-channel, or target configuration, then refresh the hostile proof. A kernel without Landlock is recorded rather than treated as this error; `legacy` remains an explicit policy choice that forfeits the zero-authority claim and is never selected automatically.

## KELD-NATIVE-001

- crate: keld-native
- message: A host fs.read/fs.write call was allowed by the guard but the OS call itself failed
- fix: Check the path exists and is accessible (permissions, disk, or a bad path passed by the app).

## KELD-COMPAT-001

- crate: keld-compat
- message: Compatibility evidence or denominator JSON exceeded the size cap
- fix: Split the ledger or shrink the document.

## KELD-COMPAT-002

- crate: keld-compat
- message: Compatibility evidence bytes are not UTF-8
- fix: Re-encode the document as UTF-8 without a BOM.

## KELD-COMPAT-003

- crate: keld-compat
- message: Compatibility evidence JSON is invalid or has trailing bytes
- fix: Supply a single UTF-8 JSON object with no trailing bytes.

## KELD-COMPAT-004

- crate: keld-compat
- message: Compatibility evidence schema is not a known v1 id
- fix: Use `keld.compat.evidence/v1` or `keld.compat.denominator/v1`.

## KELD-COMPAT-005

- crate: keld-compat
- message: Compatibility evidence record failed closed-set validation
- fix: Use the closed field set in docs/specs/kel74-compat-evidence-schema.md.

## KELD-COMPAT-006

- crate: keld-compat
- message: Compatibility waiver is missing, extra, or expired
- fix: Waive only with owner, reason, and a future YYYY-MM-DD expiry.

## KELD-COMPAT-007

- crate: keld-compat
- message: Compatibility evidence URI is a lead, not an immutable location
- fix: Use sha256:<64 lowercase hex> or an https URL with a public host (not loopback, RFC1918, CGNAT, NAT64/6to4, link-local, or unique-local; a colon in an unbracketed authority must be a decimal u16 port) whose blob/tree/raw (or GitHub raw CDN) ref is itself a 40- or 64-character lowercase-hex git object id — not a later path segment on a live branch; turn citations, sandbox paths, and mutable branch URLs are non-normative leads only.

## KELD-COMPAT-008

- crate: keld-compat
- message: Compatibility denominator is empty, duplicated, or unusable
- fix: Commit a v1 denominator with unique cells before scoring.

## KELD-COMPAT-009

- crate: keld-compat
- message: Two evidence records named the same denominator cell
- fix: Keep one record per (operation_id, oracle_id).

## KELD-CORE-030

- crate: keld-core
- message: Host-owned app-link I/O error
- fix: Check that the temp/session directory is writable.

## KELD-CORE-031

- crate: keld-core
- message: Host-owned hello session failed
- fix: Re-run `keld doctor` and fix the reported checks.

## KELD-CORE-032

- crate: keld-core
- message: Timed out waiting for a Bun ready marker
- fix: Confirm Bun is on PATH and the project entry speaks kipc.

## KELD-GUARD016

- crate: keld-guard
- message: Retained permissions-manifest bytes do not match the validated boot digest
- fix: Rebuild or re-sign the boot artifact so its digest matches the exact policy bytes.

## KELD-CORE-033

- crate: keld-core
- message: The supervised app process stopped while the host owned the window
- fix: Fix the cause named by the nested `KELD-RUNTIME-*` diagnostic, then relaunch the no-flag host or re-run `keld dev`.

## KELD-CORE-034

- crate: keld-core
- message: No-flag application boot is unavailable on this platform
- fix: Complete and prove the named KEL-96/T4 platform slice before launching the no-flag host.

## KELD-CORE-035

- crate: keld-core
- message: The private schema-v1 boot descriptor is invalid
- fix: Remove duplicate or unknown fields and regenerate a bounded strict schema-v1 `keld.boot.json`.

## KELD-CORE-036

- crate: keld-core
- message: The staged app root or a fixed boot target failed validation
- fix: Regenerate the owner-private stage with exact mode, readable regular files, and no symlink or path escape.

## KELD-CORE-037

- crate: keld-core
- message: The no-flag authenticated app session failed
- fix: Fix the named dev lease, guardian, app-link, Bun, window, or ordered-cleanup failure and relaunch the staged host.
