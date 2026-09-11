# Spec: one generated TypeScript app-link transport
Status: implementing
Linear: KEL-136 · Owner: GYLDLAB · Updated: 2026-09-11

## 1. Goal & non-goals

Hello scaffold and `@keld/electron` previously hand-maintained separate kipc
readers, writers, and constants. They diverged, and both concatenated buffered
prefixes on every chunk. T1 makes one TypeScript source the owner of framing,
HELLO, absolute deadlines, bounded buffering, and serialized writes. Both
consumers reuse that source: `keld create` embeds it; `@keld/electron` imports
it. Each borrowed socket callback buffer is copied once before retention, and
both retained input and outbound payloads are capped before they can create an
unbounded queue or an oversized frame. Echo and lifecycle codecs stay thin adapters.

Non-goals:

- no hello Close/Quit lifecycle consumer (KEL-185);
- no native-Close requalification (KEL-182);
- no KEL-133 artifact republication, corpus row copy, or protocol-version bump;
- no new Rust crate;
- no Electron runtime import;
- no public TypeScript `any`.

## 2. Spec refs

- `docs/architecture/02-ipc.md` §2 and §7: v0 frame layout, HELLO, session, I/O deadlines.
- `docs/architecture/06-runtime-and-tooling.md` §1: supervised Bun client.
- `docs/specs/kel133-kipc-receiver-semantics.md`: consumer of the canonical TSV
  `crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv`.
- Linear KEL-136 acceptance (GYLDLAB), executed under orchestrator assignment.

This spec does not change the wire. It records the TypeScript ownership split
already required by KEL-133 criterion 10's later consumer.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given the repository, when both the generated hello scaffold and `@keld/electron`
   speak kipc, then constants and transport code come from
   `packages/@keld/kipc/src/transport.ts`. A second `MAGIC_BYTES` or `class FrameReader`
   in production TypeScript fails.
2. Given an in-flight `readFrame()`, when a second `readFrame()` is called, then
   the second call rejects `KELD-IPC-005` and the first waiter still receives the frame.
3. Given two drain waiters, when `fire()` runs, then both resolve.
4. Given a mid-frame write failure, when a later `writeFrame` is attempted, then it
   rejects without sending a second frame (poisoned queue).
5. Given an unknown kind byte, when `decodeHeader` runs, then the result is
   `KELD-IPC-002`. Given `Err` on the wrong channel for a lifecycle reply waiter,
   when `validateReceivedHeader` runs, then the result is `KELD-IPC-005`.
6. Given N unread one-byte socket chunks of an incomplete frame, when the reader
   buffers them, then `pendingChunkCount() === N` (not one merged prefix).
7. Given the canonical TSV path, when Bun and Rust suites run, then they load that
   one file (digest `375f50c4bea1b690dbf7f385aee0464eae0946218058445306240b997d7e9746`)
   and no second `receiver-semantics-v0.tsv` exists.
8. Given `tsc --strict` and the committed Bun 1.4.2 / TypeScript 7.0.2 lockfile,
   when `just typescript` / the CI Bun lane run, then they pass and public sources
   contain no `any`. A new package lockfile MUST copy `@keld/electron`'s resolved
   `@types/node@26.4.1`; regenerating can pick 22.x and fail `tsc --strict`.
9. Existing lifecycle API tests (`@keld/electron`) and echo golden-vector tests
   (hello `kipc.test.ts`) remain separate from the shared transport tests.
10. Given a `Ready` Event then an Echo Reply on one HELLO'd stream, when
    `DirectedReader.receive(echoReplyWaiter, lifecycleEventReceiver)` /
    `AppLinkSession.echo` runs, then the echo succeeds and the Event stays
    parked for a later `receive(lifecycleEventReceiver)`. Close/Quit is still
    not sent here (KEL-185).
11. Given a postcard u32 varint, when it exceeds five bytes or uses more than
    four payload bits in byte five, then the shared decoder rejects
    `KELD-IPC-003`; string and scalar decoding reuse that one bound.
12. Given a borrowed callback chunk, mutation after `FrameReader.push` cannot
    alter retained bytes. More than `HEADER_LEN + MAX_FRAME_LEN` retained bytes
    fail `KELD-IPC-004`, and an outbound payload above `MAX_FRAME_LEN` fails
    before header allocation or any socket write without poisoning the queue;
    a later valid write still succeeds.
13. Given the generated transport sidecar, the boot compiler stages it and the
    Linux strict profile binds only the admitted regular file at
    `/code/kipc-transport.ts`; absence remains valid for self-contained fixtures,
    while a directory or symbolic link fails closed.
14. Given more than 65,536 unread fragments for an incomplete frame, the reader
    fails closed with `KELD-IPC-004`, releases the queue, and rejects later reads.

## 4. Design

- First-principles: one I/O owner per connection (FrameReader + WriteQueue), one
  semantic validator (KEL-133 consumer), one mux (`DirectedReader`) that parks
  lifecycle Events while waiting for Echo Reply. Adapters own codecs and Quit
  policy. No handle, crash, or principal boundary change.
- Reuse: electron WriteQueue / multi-waiter drain / overlapping-read rejection /
  absolute write deadline; hello decodeHeader detail and echo postcard codec;
  `Bun.connect({unix: pipe})` Windows path. Rejected a new Rust crate (Phase 2
  ships without it). Rejected putting kipc inside `@keld/electron`. Rejected
  keeping two copies plus a sync script.
- Compatibility fallback: stock scaffold stays dependency-free via `include_str!`.
- Memory ownership: Bun's `SocketHandler.data` callback type does not document a
  retain-after-callback lifetime. `FrameReader.push` therefore treats input as
  borrowed and takes exactly one owned copy before queueing it. This replaces
  the old repeated prefix copies without relying on an undocumented runtime
  lifetime.
- Envelope ownership: `FrameReader` owns at most one maximum frame envelope of
  retained bytes; `WriteQueue` owns the matching outbound cap and rejects before
  header allocation or socket effects. Rust `keld-ipc` remains the wire owner.
- New types: `packages/@keld/kipc` (npm package, not a Rust crate).
- Capabilities / manifest: none.
- Wire/protocol: none (no version bump). Public TS surface of `@keld/kipc` is new
  (public API review gate).
- Platform: same `unix:` named-pipe client on Windows; diagnostic TCP port retained.

## 5. Boundaries

Implement in:

- `packages/@keld/kipc/**`;
- `crates/keld-cli/templates/hello/src/{kipc.ts,kipc-transport.ts,kipc.test.ts}`;
- `crates/keld-cli/src/template.rs` embed path;
- `crates/keld-cli/src/{boot.rs,create.rs,template.rs}` for generated/staged sidecar behavior;
- `crates/keld-core/src/app_session.rs` for the exact Linux read-only sidecar mount;
- `crates/keld-host/tests/no_flag_{linux,macos,windows}.rs` and the existing
  `keld-runtime` platform fixtures for product-path reuse;
- `packages/@keld/electron/src/{link.ts,corpus.test.ts}`;
- `justfile` typescript subset;
- `tools/ci_changes_test.sh` for the derived test-routing contract;
- architecture 02/06 current-state, onboarding 03/04, the product-status source,
  and their generated documentation.

Must not touch: KEL-185 hello Close/Quit body, KEL-182 evidence, KEL-133 TSV rows,
keld-guard, principal minting, workspace Cargo.toml.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1 — one transport package, embed + import, hostile tests, type/lockfile gates.

## 7. Test plan

| Criterion | Test |
|---|---|
| 1 | `@keld/kipc` `one source / no second copy`; `template_embeds_canonical_transport_not_the_in_repo_shim` |
| 2–6 | `@keld/kipc` `transport.test.ts`; electron `link.test.ts` DrainSignal/WriteQueue/FrameReader |
| 7 | corpus digest in Rust `receiver_corpus.rs` and Bun `corpus.test.ts`; TSV uniqueness |
| 8 | `just typescript`; no-`any` grep |
| 9 | hello `kipc.test.ts` echo vectors; electron `app.test.ts` / lifecycle fixtures |
| 10 | `@keld/kipc` Ready-before-Echo park/FIFO/overflow; hello `AppLinkSession.receive`/`writeFrame` |
| 11 | `@keld/kipc` fifth-byte overflow, six-byte, valid-u32-max, invalid-offset, and postcard-string regressions |
| 12 | borrowed-chunk mutation, retained-envelope overflow, and zero-effect/non-poisoning oversized-write regressions |
| 13 | CLI create/stage sidecar tests; core absent/regular/directory/lstat/symlink tests; no-flag Linux product fixture |
| 14 | `@keld/kipc` pending-fragment overflow and queue-release regression |

Anti-flake: no sleeps; write-deadline tests keep their existing 4–12s wall-clock bounds.

## 8. Review gates triggered

unsafe: none. public API: `@keld/kipc` transport exports. permission model:
Linux strict containment adds one exact read-only file mount, independently
covered by lstat/symlink admission and the no-flag Linux product fixture.
dependency addition: `@types/bun` 1.4.0 and `typescript` 7.0.2 already pinned by
`@keld/electron` (same versions; new package lockfile). wire protocol: none.

## 9. Perf impact

None on architecture 01 §5 host/first-paint budgets. Fragmented max-size frames
stop doing quadratic prefix copies; no absolute latency target is claimed.

Performance decomposition and reproducible evidence:

- census: 8 MiB/32,768 and 16 MiB/65,536 payload/chunk pairs at 256 bytes per chunk;
- work: one owned admission copy per chunk and one final payload assembly;
- queue/copy: one entry per unread chunk, retained bytes capped at one maximum
  frame envelope and unread entries capped at 65,536, with a first-active cursor
  that releases consumed buffers and amortizes prefix compaction instead of
  reindexing via `Array.shift()`;
- clock: `performance.now()` in one pinned Bun process;
- statistic: three samples after a warm-up, reported as median and 16/8 scaling
  ratio without a flaky timing threshold;
- artifact: `bun run bench:fragmented` from `packages/@keld/kipc`, plus the
  deterministic queue/cap/ownership tests that remain the correctness oracle.

## 10. Open questions

none.
