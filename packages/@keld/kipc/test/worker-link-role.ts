/**
 * Bun role fixture for the GH-528 T1 harness (spec gh527 §7).
 *
 * `crates/keld-ipc/tests/worker_link.rs` is the host: it mints `KELD_APP_LINK`,
 * spawns `bun worker-link-role.ts <scenario>`, and drives the link with the
 * real `keld_ipc::link::write_frame` writer. This file is the role half. It
 * prints one `KELD_WL key=value` line per observation; the host asserts them.
 */
import {
  DrainSignal,
  ECHO_CHANNEL,
  FrameKind,
  FrameReader,
  RECEIVE_POLICIES,
  WriteQueue,
  connectKipcSocket,
  kipcError,
  parseAppLink,
  timingSafeEqual,
  validateReceivedHeader,
  withIoDeadline,
} from "../src/transport.ts";

function report(key: string, value: string | number | boolean): void {
  console.log(`KELD_WL ${key}=${value}`);
}

function requireLink(): string {
  const link = process.env.KELD_APP_LINK;
  if (link === undefined || link.length === 0) {
    throw kipcError("KELD-IPC-007", "KELD_APP_LINK is unset");
  }
  return link;
}

/**
 * Criterion 1, failing-first arm: today's main-thread client issues one
 * blocking CALL and parks the main thread in `Atomics.wait`. Nothing reads the
 * socket while it is parked, so the host writer fills the platform send space.
 */
async function criterion1(): Promise<void> {
  const { endpoint, token } = parseAppLink(requireLink());
  const reader = new FrameReader();
  const drain = new DrainSignal();
  const socket = await connectKipcSocket(endpoint, reader, drain);
  const writes = new WriteQueue(socket, drain);
  await withIoDeadline(writes.writeFrame(FrameKind.Hello, 0, 0, 0, token));
  const hello = await withIoDeadline(reader.readFrame());
  validateReceivedHeader(RECEIVE_POLICIES.clientAwaitHello, hello.header);
  if (!timingSafeEqual(hello.payload, token)) {
    throw kipcError("KELD-IPC-007", "HELLO session token mismatch");
  }
  await withIoDeadline(
    writes.writeFrame(FrameKind.Call, 0, ECHO_CHANNEL, 1, new TextEncoder().encode("arm-b")),
  );
  report("parked", true);
  // The park is the defect under test. The host kills this process once its
  // writer has failed, so this bound is only a kill switch.
  const word = new Int32Array(new SharedArrayBuffer(4));
  Atomics.wait(word, 0, 0, 30_000);
  report("unparked", true);
}

const SCENARIOS: Record<string, () => Promise<void>> = {
  criterion1,
};

const name = process.argv[2] ?? "";
const scenario = SCENARIOS[name];
if (scenario === undefined) {
  console.error(`unknown worker-link scenario: ${name}`);
  process.exit(2);
}
try {
  await scenario();
  report("done", true);
  process.exit(0);
} catch (err) {
  console.error(err instanceof Error ? err.stack ?? err.message : String(err));
  process.exit(1);
}
