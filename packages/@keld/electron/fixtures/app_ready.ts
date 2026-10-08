/**
 * Sticky connect-failure probe for `app.ts` over the real link path (KEL-72,
 * GH-528 T3).
 *
 * Oracles:
 * - A real `LifecycleLink.connect` that fails (no listener at the app-link
 *   endpoint) rejects `whenReady()` with its own typed code, `KELD-IPC-001`.
 * - The failure is sticky: a later `whenReady()` and `quit()` reject with the
 *   same error object, and connect is not attempted again (a role opens one
 *   link per realm, GH-527 §4.2; a retry would only surface `KELD-IPC-005`).
 * - An unawaited first `whenReady()` must not become `unhandledRejection`.
 *
 * Nothing is stubbed: the counter wraps the real `connect`. Listener isolation
 * is `ready_listener_only.ts`.
 */
import { writeSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { LifecycleLink } from "../src/link.ts";

function marker(line: string): void {
  writeSync(1, `${line}\n`);
}

// An app-link endpoint with no listener, so the real connect fails at once:
// beside the test's minted Unix socket, or a fresh pipe name on Windows.
const minted = process.env.KELD_APP_LINK;
const token = minted?.split("#")[1] ?? "a".repeat(64);
const endpoint = minted?.split("#")[0];
const absent =
  process.platform === "win32"
    ? String.raw`\\.\pipe\keld-` + Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString("hex")
    : join(
        endpoint !== undefined ? dirname(endpoint) : tmpdir(),
        `keld-kel72-absent-${process.pid}.sock`,
      );
process.env.KELD_APP_LINK = `${absent}#${token}`;

let connectCalls = 0;
const realConnect = LifecycleLink.connect.bind(LifecycleLink);
LifecycleLink.connect = ((link, handlers) => {
  connectCalls += 1;
  return realConnect(link, handlers);
}) as typeof LifecycleLink.connect;

const { app } = await import("../src/app.ts");

let unhandled = 0;
process.on("unhandledRejection", (reason) => {
  unhandled += 1;
  marker(`KEL72_UNHANDLED=${String(reason)}`);
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

const again = await app.whenReady().then(
  () => null,
  (err: unknown) => err,
);
const quitErr = await app.quit().then(
  () => null,
  (err: unknown) => err,
);
if (again !== firstErr || quitErr !== firstErr) {
  writeSync(2, `KEL72_FAILURE_NOT_STICKY again=${String(again)} quit=${String(quitErr)}\n`);
  process.exit(1);
}
marker("KEL72_FAILURE_STICKY");

await new Promise<void>((resolve) => {
  setImmediate(resolve);
});

marker(`KEL72_CONNECT_CALLS=${connectCalls}`);
marker(`KEL72_UNHANDLED_COUNT=${unhandled}`);
if (connectCalls !== 1) {
  writeSync(2, `KEL72_CONNECT_RETRIED calls=${connectCalls}\n`);
  process.exit(1);
}
if (unhandled !== 0) {
  writeSync(2, `KEL72_UNHANDLED_REJECTION count=${unhandled}\n`);
  process.exit(1);
}

process.exit(0);
