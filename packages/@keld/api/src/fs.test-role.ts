/** Private role endpoint for fs.test.ts; each invocation owns one app-link realm. */
import { readSync, writeSync } from "node:fs";
import { app, channels, echoChannel, fs, isCallError } from "./index.ts";
import { LifecycleLink } from "./link.ts";

function report(key: string, value: unknown): void {
  writeSync(1, `${key}=${JSON.stringify(value)}\n`);
}

async function rejected(promise: Promise<unknown>): Promise<Error> {
  try {
    await promise;
  } catch (error) {
    if (error instanceof Error) return error;
    throw error;
  }
  throw new Error("expected the public FS operation to reject");
}

function codecFailure(error: Error): void {
  if (!error.message.startsWith("KELD-IPC-003")) throw error;
}

const scenario = process.argv[2];
if (scenario === "snapshot") {
  const unsubscribe = channels.handle(echoChannel, (request) => request);
  const source = new Uint8Array([0, 255, 65]);
  const write = fs.write("/p", source);
  source.fill(7);
  await app.whenReady();
  await write;
  const bytes = await fs.read("/p");
  report("read", Array.from(bytes));
  unsubscribe();
} else if (scenario === "variants") {
  for (let index = 0; index < 4; index += 1) {
    codecFailure(await rejected(fs.read("/p")));
  }
  codecFailure(await rejected(fs.write("/p", new Uint8Array())));
  report("read", Array.from(await fs.read("/p")));
  await fs.write("/p", new Uint8Array());
} else if (scenario === "host-error") {
  const error = await rejected(fs.read("/outside"));
  report("error", { code: isCallError(error) ? error.code : undefined, message: error.message });
} else if (scenario === "pending-quit") {
  await app.whenReady();
  const quit = app.quit();
  for (const call of [fs.read("/p"), fs.write("/p", new Uint8Array([1]))]) {
    const error = await rejected(call);
    if (error.message !== "KELD-IPC-001: session is closed" || isCallError(error)) {
      throw new Error(`unexpected closed-session error: ${error.message}`);
    }
  }
  report("rejected-before-quit-reply", true);
  await quit;
  const closed = await rejected(fs.read("/p"));
  if (closed.message !== "KELD-IPC-001: session is closed" || isCallError(closed)) throw closed;
  report("done", true);
  process.exit(0);
} else if (scenario === "close") {
  let ready!: () => void;
  const hostReady = new Promise<void>((resolve) => { ready = resolve; });
  const link = await LifecycleLink.connect(process.env.KELD_APP_LINK!, {
    onReady: ready,
    onLastWindowClosed() {},
    onLinkDead(error) { throw error; },
  });
  await hostReady;
  link.close();
  const error = await rejected(link.callFs(new Uint8Array([0, 2, 47, 112])));
  if (error.message !== "KELD-IPC-001: session is closed" || isCallError(error)) throw error;
  report("closed", true);
  report("done", true);
  process.exit(0);
} else if (scenario === "dead") {
  await rejected(fs.read("/p"));
  // Canonical onEnd runs in a later task after pending callers reject. Observe
  // that owner publication before comparing its sticky, recorded dead cause.
  await new Promise<void>((resolve) => setImmediate(resolve));
  const recorded = await rejected(fs.write("/p", new Uint8Array()));
  const later = await rejected(fs.read("/p"));
  if (recorded !== later || !isCallError(recorded)) throw new Error("recorded dead cause was replaced");
  report("dead", { code: recorded.code, message: recorded.message });
  report("done", true);
  process.exit(0);
} else if (scenario === "capacity-fit") {
  const bytes = await fs.read("/p");
  report("length", bytes.length);
} else if (scenario === "capacity-over" || scenario === "capacity-occupied") {
  await app.whenReady();
  const pending = fs.read("/p");
  // The next turn follows the adapter's call-admission microtasks. The host
  // also observes the real FS CALL before supplying any reply.
  await new Promise<void>((resolve) => setImmediate(resolve));
  if (scenario === "capacity-occupied") {
    report("parked", true);
    // The host releases this private fixture only after observing socket close.
    const release = new Uint8Array(1);
    if (readSync(0, release, 0, 1, null) !== 1 || release[0] !== 1) {
      throw new Error("occupied-ring fixture did not receive its host release byte");
    }
  }
  const error = await rejected(pending);
  if (!isCallError(error) || error.code !== "KELD-IPC-026") throw error;
  report("overflow", error.code);
  report("done", true);
  // Let the canonical Worker finish its already-observed terminal close and
  // flush diagnostics. An abrupt process.exit can drop Worker console output
  // while the main realm has been parked, despite a real IPC026 rejection.
} else {
  throw new Error(`unknown FS test scenario ${scenario}`);
}
if (scenario !== "capacity-over" && scenario !== "capacity-occupied") {
  await app.quit();
  report("done", true);
}
