# keld-ipc — adds root AGENTS.md

Spec: `docs/architecture/02-ipc.md`. Hot path.

- Wire = versioned protocol. Frame layout/`FrameKind`/flags/handshake change → version bump + wire review gate + spec §2, one PR.
- Test wire constants as facts (`HEADER_LEN == 16`), not struct layout. Assert hot struct sizes.
- Tests MUST follow repository `.agents/testing.md`.
- State-machine readers/writers. No async, no steady-state alloc (`Vec`/frame = wrong design).
- Credit-window backpressure; no unbounded queues. Every OS-block await has deadline.
  v0: `SO_RCVTIMEO`/`SO_SNDTIMEO` of 5s on the connected stream; expiry is `KELD-IPC-006`.
  Exception (KEL-72 `LIFECYCLE_CHANNEL`): `HELLO` still uses `APP_LINK_IO_DEADLINE`;
  the host then sets a short reader poll (`SO_RCVTIMEO`) and retries idle
  timeouts via `read_frame_interruptible` so a quiet `whenReady` wait is not
  `KELD-IPC-006` and so Drop can join. Win32 `TcpStream::shutdown` on a cloned
  handle does not wake a blocking `read` (rust-lang/rust#121594) — that is not
  peer-FIN. The writer deadline (`SO_SNDTIMEO`) stays `APP_LINK_IO_DEADLINE`.
  `read_frame` still cannot retry after Timeout. Spec 02 §2/§7 v0 host lifecycle.
- `unsafe` only in future `shm` module (`deny(unsafe_op_in_unsafe_fn)`, `// SAFETY:`). Framing stays `#![forbid(unsafe_code)]`.
- postcard on hot path; JSON only for `--inspect-ipc` debug.
- Fuzz decode paths — malformed webview input is expected, not a bug.
