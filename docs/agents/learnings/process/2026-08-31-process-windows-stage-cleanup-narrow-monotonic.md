---
name: "2026-08-31-process-windows-stage-cleanup-narrow-monotonic"
description: "Windows stage cleanup needs a narrow monotonic retry for final-directory sharing violations."
area: "process"
tags: ["process", "windows", "stage", "cleanup", "narrow"]
platform: "windows"
recorded: "2026-08-31"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L151"
related: []
---

## Historical source

> - 2026-08-31 [process/windows] After an abnormal host exit, Windows may briefly retain a Job member's current-directory handle after the host process object is signaled. Rust's Windows `remove_dir_all` bounds/yields on child-entry sharing violations but not the final directory's error 32, so an exact validated stage cleanup needs the same narrow whole-operation retry bounded by a monotonic deadline; a fixed yield count exhausted under CI load. Generic retries and sleeps remain forbidden. (evidence: KEL-150 negative control, KEL-150 regression follow-up, GitHub Windows run `33355298740`)
