---
name: "2026-08-15-bridge-bun-unix-socket-writes-partial"
description: "Bun Unix-socket writes can be partial; loop through drain instead of assuming one write completes."
area: "bridge"
tags: ["bridge", "bun", "unix", "socket", "writes"]
platform: "unspecified"
recorded: "2026-08-15"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L58"
related: ["docs/agents/learnings/cli/2026-08-13-cli-tao-eventloop-run-never-returns.md", "docs/agents/learnings/ipc/2026-08-17-ipc-second-echo-call-repeats-hello.md"]
---

## Historical source

> - 2026-08-15 [bridge] `Bun.connect({unix: "/path"})` (client-side Unix domain socket) is real and documented in `bun-types` but absent from bun.sh's own rendered docs pages and Context7's indexed `tcp.mdx` (both only show `hostname`/`port`); ground truth is `packages/bun-types/bun.d.ts` on `oven-sh/bun` (`UnixSocketOptions`/`TCPSocketConnectOptions`, both accepted by an overloaded `connect()`). `socket.write()` returns bytes written, `-1` on a closed socket, and can be a **partial** write under backpressure — must loop with the `drain` handler, not assume one `write()` call sends everything. (evidence: KEL-30, crates/keld-cli/templates/hello/src/kipc.ts, raw.githubusercontent.com/oven-sh/bun/main/packages/bun-types/bun.d.ts)
