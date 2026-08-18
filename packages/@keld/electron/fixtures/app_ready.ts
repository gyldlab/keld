/**
 * Isolation + connect-retry probe for `app.ts` (no host).
 *
 * Oracles:
 * - A throwing `ready` listener must not skip a later listener and must
 *   not leave `whenReady()` pending.
 * - A rejected `LifecycleLink.connect` must not be cached: a later
 *   `whenReady()` retries (connect call count increases).
 * - An unawaited first `whenReady()` must not become `unhandledRejection`.
 *
 * `LifecycleLink.connect` is replaced here so the probe is independent of
 * the wire client. The live kipc path is `lifecycle.ts`.
 */
import { writeSync } from "node:fs";
import { LifecycleLink } from "../src/link.ts";

function marker(line: string): void {
  writeSync(1, `${line}\n`);
}

// Prefer the unique unused app-link minted by `electron_lifecycle.rs`.
// Standalone `bun ./app_ready.ts` still needs a well-formed value so
// `ensureLink` does not reject before the stubbed `connect` runs.
if (!process.env.KELD_APP_LINK) {
  process.env.KELD_APP_LINK = `/tmp/keld-kel72-unused.sock#${"a".repeat(64)}`;
}

type Handlers = { onReady: () => void; onLastWindowClosed: () => void };

let connectCalls = 0;
let failNextConnect = true;

LifecycleLink.connect = (async (_link: string, handlers: Handlers) => {
  connectCalls += 1;
  if (failNextConnect) {
    failNextConnect = false;
    throw new Error("KELD-IPC-001: simulated connect failure");
  }
  queueMicrotask(() => handlers.onReady());
  return {
    async quit() {},
    close() {},
  };
}) as typeof LifecycleLink.connect;

const { app } = await import("../src/app.ts");

let unhandled = 0;
process.on("unhandledRejection", (reason) => {
  unhandled += 1;
  marker(`KEL72_UNHANDLED=${String(reason)}`);
});

app.on("ready", () => {
  throw new Error("KEL72_READY_LISTENER_THROW");
});
let secondReady = false;
app.on("ready", () => {
  secondReady = true;
  marker("KEL72_READY_SECOND");
});

// Unawaited first call shares the in-flight failing connect with the await
// below. Must not surface as unhandledRejection.
app.whenReady();

const firstErr = await app.whenReady().then(
  () => null,
  (err: unknown) => err,
);

if (!(firstErr instanceof Error) || !String(firstErr).includes("KELD-IPC-001")) {
  writeSync(2, `KEL72_FIRST_SHOULD_REJECT=${String(firstErr)}\n`);
  process.exit(1);
}

const afterFailCalls = connectCalls;

try {
  await app.whenReady();
} catch (err: unknown) {
  writeSync(2, `KEL72_RETRY_FAILED=${String(err)}\n`);
  process.exit(1);
}
marker("KEL72_READY");

if (!secondReady) {
  writeSync(2, "KEL72_SECOND_LISTENER_SKIPPED\n");
  process.exit(1);
}

if (connectCalls < afterFailCalls + 1) {
  writeSync(2, `KEL72_CONNECT_NOT_RETRIED calls=${connectCalls} afterFail=${afterFailCalls}\n`);
  process.exit(1);
}

await Promise.resolve();
await Promise.resolve();
// Bun 1.3.14 delivers `unhandledRejection` on a macrotask. Two microticks
// leave COUNT=0 even when a derived `whenReady()` promise is unhandled.
await new Promise<void>((resolve) => {
  setImmediate(resolve);
});

marker(`KEL72_CONNECT_CALLS=${connectCalls}`);
marker(`KEL72_UNHANDLED_COUNT=${unhandled}`);

if (unhandled !== 0) {
  writeSync(2, `KEL72_UNHANDLED_REJECTION count=${unhandled}\n`);
  process.exit(1);
}

process.exit(0);
