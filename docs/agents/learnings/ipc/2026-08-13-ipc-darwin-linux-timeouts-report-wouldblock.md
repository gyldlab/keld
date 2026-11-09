---
name: "2026-08-13-ipc-darwin-linux-timeouts-report-wouldblock"
description: "Darwin and Linux timeouts may report WouldBlock; map deadline errors consistently and keep the peer alive."
area: "ipc"
tags: ["ipc", "darwin", "linux", "timeouts", "report", "multiple"]
platform: "unspecified"
recorded: "2026-08-13"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L38"
related: []
---

## Historical source

> - 2026-08-13 [ipc] Darwin/Linux `SO_RCVTIMEO` on a live silent UnixStream is `WouldBlock` (EAGAIN), not always `TimedOut`; map both to `KELD-IPC-006` or a deadline is misclassified as `KELD-IPC-001`. Keep the peer socket alive in the test or you get EOF instead. (evidence: crates/keld-ipc/src/link.rs `read_frame_silent_peer_is_ipc_006`)
