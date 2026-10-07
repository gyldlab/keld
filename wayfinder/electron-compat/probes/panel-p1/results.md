# PANEL-P1 (gyldlab/keld#418): link-drain gate results

Scratch prototype. Nothing in `/Users/centillionaire/WORK/keld` was edited, committed or pushed. Nothing was posted to GitHub or Linear.

**Environment (FACT).**
- macOS 26.5.1 (25F80) on arm64, Bun 1.4.2 run with `--no-install`, Rust 1.97.1 from the repo's pinned `rust-toolchain.toml`.
- Keld checkout `main` @ `9dfe4a11`. Since the PANEL-P1 pin `b4b907c3`, only `crates/keld-ipc/src/lib.rs` changed (attempt-module exports). `packages/@keld/kipc/src/transport.ts`, `link.rs`, `frame.rs` and `codec.rs` are byte-identical to the pin.
- `sysctl net.local.stream.sendspace = 8192`, `recvspace = 8192`.

## What drives each side (FACT)

**Host (`host/src/main.rs`).** It links the real `keld-ipc` by path.
- It uses `link::handshake_server` (v2 HELLO, `SessionToken::random`) and `link::write_frame`. The writer runs on a blocking `UnixStream` whose `SO_SNDTIMEO = APP_LINK_IO_DEADLINE` (5 s) is set through `AppLinkDeadlines`.
- The reader is `link::read_frame_interruptible`, with `SO_RCVTIMEO = APP_LINK_READER_POLL`.
- Payloads use `codec::{encode,decode}` (postcard), `echo::handle_echo`, and `LifecycleRequest/Response::Quit`.
- This is the real writer, not a harness. Production `keld-ipc/session.rs` and `keld-core` write through the same `write_frame`.
- Not exercised: the keld-core `app_session` orchestration, and the validated-policy reader. Scratch channel 9 is in no `ReceivePolicy`.

**Client (`client/*.ts`).** It imports `@keld/kipc` `transport.ts` by absolute path. All frames use its `encodeHeader`, `decodeHeader`, `FrameReader`, `WriteQueue`, `connectKipcSocket`, `validateReceivedHeader`, `echoReplyWaiter`, `lifecycleReplyWaiter`, `parseAppLink` and `timingSafeEqual`.

**Scratch-only definitions (INFERENCE: v0 defines no schema for either).**
- EVENTs ride channel 9. The payload is postcard `(seq: u32, pad: Vec<u8>)`, sized so every EVENT frame is exactly 64 bytes.
- GRANT (`FrameKind::Grant` = 9) carries a postcard `u32` frame credit on channel 9. Spec 02 §2 says that Grant "has no live sender/receiver" in v0.

**Load (per the PANEL-P1 packet).**
- Main issues a sync echo CALL (`"scenario"`). The host then writes a 10,000-EVENT burst, then 100 EVENT/s for 10 s (1,000 more), and only then the REPLY.
- Main is therefore parked about 10 s while about 704 KB arrive.
- Arm A and arm C park with `Atomics.wait(12 s)`.
- Arm A stalls inside the 10k burst. The packet's steady-only 100/s load was not run separately. INFERENCE: it would hit the same 8 KiB wall after about 1.3 s of steady load.

**One link.** The host unlinks the endpoint after authentication, so the locator is consumed. Every arm uses exactly one socket.

## Arm A (control): socket on the Bun main thread. 5 runs

| Criterion | Result |
|---|---|
| zero link stall while parked | **FAIL** 5/5 |
| no `KELD-IPC-006` from Rust writer | **FAIL** 5/5 |
| 10,000 EVENTs replayed in order after wake | **FAIL** 5/5 (128 received) |
| sync REPLY while parked | **FAIL** (cannot; reader is parked) |

Evidence (FACT):
- The writer accepted **8,192 bytes = 128 frames** in 5/5 runs. That equals `sendspace`.
- The writer then returned `KELD-IPC-006: app-link I/O deadline exceeded` after blocking 5,000–5,001 ms (5,000.4–5,001.2 ms after the scenario started).
- The client saw 0 bytes during the park, and 128 EVENTs and no REPLY after wake.
- The negative control reproduced, so the load is valid.

## Arm B: socket in a Worker, SAB ring + `Atomics.notify`, main `Atomics.wait`. 5 runs

| Criterion | Result |
|---|---|
| zero link stall while parked | **PASS** 5/5 |
| no `KELD-IPC-006` | **PASS** 5/5 |
| 10,000 EVENTs replayed in order after wake | **PASS** 5/5 (11,000/11,000, 0 dupes, 0 gaps, 0 reorder) |
| sync REPLY returned while parked | **PASS** 5/5 |
| second link for the role refused | **PASS** 5/5 (`ENOENT`, locator consumed) |

Evidence (FACT):
- The host wrote all 704,043 bytes. The worst single `write_frame` block was 0.66–10.2 ms, and the 10k burst was written in 10–67 ms. No write blocked ≥250 ms.
- Main was parked 10,003–10,060 ms and woke exactly once (`waits:1`). That was 0.58–2.2 ms after the host wrote the REPLY.
- The worker held all 11,000 EVENTs before main woke. The ring high-water mark was 704,000 B of a 1 MiB ring.
- A post-run lifecycle Quit got its real reply, and the host serve loop returned.
- RTT diagnostic only, not a claim: 3×1,000 sync echo calls gave p50 0.028–0.030 ms, p99 0.063–0.082 ms and max 0.31 ms.

## Arm C: host credit over `FrameKind::Grant`, socket on main. 5 runs

| Criterion | Result |
|---|---|
| zero link stall while parked | **FAIL** 5/5 (producer suspended 12,002–12,006 ms at zero credit) |
| no `KELD-IPC-006` | **PASS** 5/5 |
| 10,000 EVENTs replayed in order after wake | **PASS** 5/5 (11,000, in order; all delivered only after wake) |
| sync REPLY while parked | **FAIL** (REPLY arrived 10,007–10,021 ms after wake) |

Credit window: 64 frames, with W/2 re-grants (311 credit waits per run).

**B+C variant (3 runs, extra).** This combines a 64 KiB worker ring with credit equal to free ring slots, and a host producer that suspends rather than blocking the writer.
- PASS on stall, `KELD-IPC-006`, order (11,000) and sync REPLY while parked.
- The host producer, however, deferred **9,976 EVENTs** until after wake. Delivery finished 23–24 ms after wake.
- So the bound moves to the host producer. That is not zero buffering.

## Arm E: generation retire / Quit during a park, on the arm B transport

| Criterion | E1 retire (5 runs) | E2 Quit (5 runs) |
|---|---|---|
| zero link stall | PASS | PASS |
| no `KELD-IPC-006` | PASS | PASS |
| EVENTs in order after wake | PASS (10,200/10,200) | PASS (10,000/10,000) |
| completes without hang | PASS (all exit 0, host serve returned) | PASS |
| no fabricated reply | PASS: typed `KELD-IPC-001` throw, 0.41–1.68 ms after the host's `shutdown_app_link` | PASS: real `LifecycleResponse::Quit` (0x00), main woke 0.30–1.04 ms after the reply; the close came after it |

FACT: v0 has no wire "retire" message. E1 models retire as the host shutting down that generation's link.

**Extra probe, worker death during a park (2 runs). FAIL as a typed wake.**
- The host saw `KELD-IPC-001` Broken pipe at once.
- The parked main thread woke only at its own 3 s CALL deadline, with `KELD-IPC-006`, 2,503–2,504 ms after the death. No reply was fabricated.

## Negative controls

| Control | Result |
|---|---|
| NC1: drain on main (arm A) | stalls at 8,192 B, `KELD-IPC-006` (5/5) |
| NC2: worker omits `Atomics.notify` | sync CALL times out `KELD-IPC-006` at 2,002.6–2,005.5 ms (2/2) |
| NC3: replay drops seq 5000 | order check fails: 10,999 received, 1 gap (2/2) |
| Ring overflow (64 KiB ring, no credit) | fails closed: typed `KELD-IPC-004` to main. The first 1,024 EVENTs replay in order and no reply is fabricated. The worker closes the link, and the host gets `KELD-IPC-001` EPIPE rather than `006` (3/3) |

## Reproduce

```sh
P1=/private/tmp/claude-501/-Users-centillionaire-WORK-keld/9d46ef65-830a-4067-90bc-f68a26ad7f85/scratchpad/p1
cd $P1/host && CARGO_INCREMENTAL=0 cargo build --release --offline   # Cargo.lock copied from keld main
cd $P1 && for s in "A 5" "B 5" "C 5" "E-retire 5" "E-quit 5" "E-worker-death 2" "BC-credit-ring-64KiB 3" \
  "NC2-no-notify 2" "NC3-drop-one 2" "B-overflow-64KiB 3" "B-rtt 3"; do bun --no-install run.ts ${=s}; done
bun --no-install run.ts report   # -> s0-link-drain.json
```

`${=s}` is zsh word-splitting; in bash use `$s`. Raw logs are in `runs/<spec>/<i>/{host,client}.jsonl` and per-spec rows in `runs/<spec>/rows.json`.

## FACT / INFERENCE / UNKNOWN

**FACT** (measured above, macOS arm64 only):
1. A parked main-thread transport stalls at exactly the 8 KiB AF_UNIX send space, and the Rust writer then fails with `KELD-IPC-006` at 5 s.
2. A Bun 1.4.2 Worker can own the single socket for the whole session and drain it while main is parked in `Atomics.wait`.
3. `postMessage` from main reaches the worker while main is parked, and `Atomics.notify` wakes main within about 2 ms.
4. Credit alone prevents `KELD-IPC-006` but cannot deliver a reply to a parked main-thread reader.
5. Worker death is not signalled to a parked main except through the CALL deadline.

**INFERENCE:**
- Linux and Windows socket buffer sizes differ, so the stall byte count changes. The arm-B result is engine-neutral but unproven off macOS.
- The B ring only *moves* the bound. A long modal with a high event rate needs either fail-closed overflow or host-side credit suspension (B+C).
- Typed codes `KELD-IPC-001` (close/retire) and `KELD-IPC-004` (overflow) are reused here. F04-T19 must register the final codes.

**UNKNOWN:**
- The real EVENT byte rate during a modal, and therefore the ring size.
- Whether host producers (window/focus mirrors) can suspend or coalesce at zero credit without an unbounded queue.
- Behaviour on Linux and Windows.
- Steady-state allocation: the prototype allocates per frame.
- Ring counters wrap at 2^31 bytes (prototype only).
- The TS files were run by Bun, not `tsc`-checked.

## VERDICT

**F04-T18 should adopt arm B** (worker-owned single link, SAB ring, `Atomics.wait`/`notify`). It is the only arm that passed every exit criterion, 5/5. Arm E passes on the B transport. Arm A is the confirmed failure. Arm C is rejected as a standalone arm, but it is retained as B's overflow and backpressure policy.

**Required invariants:**
1. Exactly one link and one principal per role generation, owned by the worker from HELLO to close. The locator is consumed at authentication, and a second link is refused.
2. All main-thread kipc traffic, sync and async, goes through the worker.
3. One in-flight blocking CALL per role. The reply is matched by correlation id and validated with the real waiter policy before return.
4. EVENTs are retained in arrival order in a bounded ring. On overflow the system either fails closed with a typed error, or suspends the host producer through Grant credit equal to the free ring slots.
5. Revocation and retire during a park: the host shutdown wakes main with a typed error and never a fabricated result. A real reply that precedes the close wins.
6. Every blocking CALL has a deadline.
7. Worker liveness must wake a parked main (gap found here). Options: a supervisor-owned liveness word, or the host treating link loss as a role crash.

**Falsifier.** Any one of these would falsify the verdict:
- the stall test passes with draining moved back to main;
- any run where the host writer blocks ≥250 ms or returns `KELD-IPC-006` while main is parked;
- any dup, gap or reorder in the 10,000-EVENT replay;
- a reply value returned after link close, retire or worker death;
- a second link accepted for the role;
- the same result failing to reproduce on Linux or Windows.
