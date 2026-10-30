---
name: "2026-08-13-cli-windows-doctor-json-strip-verbatim"
description: "Windows doctor JSON must strip the verbatim path prefix returned by fs::canonicalize."
area: "cli"
tags: ["cli", "windows", "doctor", "json", "strip"]
platform: "windows"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L47"
related: []
---

## Historical source

> - 2026-08-13 [cli] Windows `fs::canonicalize` yields `\\?\…`; doctor JSON must strip that verbatim prefix so MCP (`project_root`) matches CLI (`current_dir`). (evidence: crates/keld-cli/src/doctor.rs `strip_verbatim_prefix`, windows-latest `mcp_doctor_matches_cli_json_over_stdio`)
