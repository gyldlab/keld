# 07 — Use Keld from an MCP client

`keld mcp serve` exposes Keld's environment checks, embedded documentation, and
permission explanations to coding agents over stdio. It is offline and read-only: it
opens no network listener, edits no manifest, and has the same local authority as the
`keld` process that the client starts.

## Prerequisite

Build or install the `keld` binary, then make sure the client can execute it:

```bash
cargo build -p keld-cli
./target/debug/keld --version
```

During repository development, replace `keld` in the configurations below with the
absolute path to `target/debug/keld`. Starting the server manually is useful only as a
smoke check; an MCP client starts it and owns its stdin:

```bash
keld mcp serve
```

## Claude Code registration

Add this project-scoped `.mcp.json`:

```json
{
  "mcpServers": {
    "keld": {
      "type": "stdio",
      "command": "keld",
      "args": ["mcp", "serve"]
    }
  }
}
```

## Cursor registration

Add this project-scoped `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "keld": {
      "command": "keld",
      "args": ["mcp", "serve"]
    }
  }
}
```

Restart or reload the client's MCP servers after changing its configuration. A
successful connection lists exactly these three tools:

1. `keld_doctor`
2. `keld_docs_search`
3. `keld_permissions_explain`

## Tool workflow

### 1. Diagnose the environment

Call `keld_doctor` before attempting a build or dev session. Pass `project_root` when
the MCP client did not start the server from inside the app:

```json
{
  "project_root": "/absolute/path/to/my-app"
}
```

Its `structuredContent` is the same findings array as `keld doctor --json`. Failed
findings include a stable `KELD-*` code and an imperative `fix`.

### 2. Search before guessing

Call `keld_docs_search` for architecture, security, and error-code questions:

```json
{
  "query": "capability manifest",
  "max_results": 5
}
```

Results are deterministic, cite repo-relative source paths, and are bounded to 20
chunks. If results were cut, follow the returned hint by narrowing the query or raising
`max_results`.

### 3. Explain a permission decision

After a capability denial, call `keld_permissions_explain` with the exact manifest,
principal, capability, and arguments:

```json
{
  "manifest_path": "/absolute/path/to/my-app/keld.permissions.jsonc",
  "operation": {
    "principal": "app",
    "capability": "fs.read",
    "args": {
      "path": "$DOCUMENTS/notes.txt"
    }
  }
}
```

A denial is a normal result, not a protocol failure. Read `deny_reason`, then present
the returned `patch` and `error.fix` for human review. The tool never applies the patch.
A missing manifest is a tool error with code `KELD-MCP010`; its fix names the path that
was tried. Omit `operation.channel` in v0 — a present value is `KELD-MCP014` (channel
grants are not evaluated; the tool must not answer the path question).

## Supported protocol surface

The server negotiates MCP `2026-07-28` and `2025-11-25`. The 2026 protocol includes
`server/discover`, tool output schemas, cache hints, and `resultType: "complete"`.
There is no HTTP transport, remote access, elicitation, task extension, telemetry, or
write tool in v1.
