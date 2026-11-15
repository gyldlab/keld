/** KEL-140 private test caller; framework imports expose only the public API. */
// Fixed fd0/fd1 calls control this private fixture; target-path I/O uses public fs.
import { readSync, writeSync } from "node:fs";
import {
  app,
  fs,
  isCallError,
} from "../../../../../../packages/@keld/api/src/index.ts";

function report(key: string, value: string | number | boolean): void {
  console.log(`KELD_WL ${key}=${value}`);
}

function readControlLine(): unknown {
  const line = new Uint8Array(4096);
  for (let offset = 0; offset < line.length; offset += 1) {
    if (readSync(0, line, offset, 1, null) !== 1) {
      throw new Error("occupied fixture control ended before its JSON line");
    }
    if (line[offset] === 10) {
      return JSON.parse(new TextDecoder().decode(line.subarray(0, offset)));
    }
  }
  throw new Error("occupied fixture control line exceeds its bounded input");
}

const ready = app.whenReady();
const scenario = process.argv[2] ?? "retirement";
if (
  scenario !== "retirement" &&
  scenario !== "reply-wins" &&
  scenario !== "healthy" &&
  scenario !== "ring-fit" &&
  scenario !== "ring-over" &&
  scenario !== "ring-occupied"
) {
  throw new Error("invalid private public-FS fixture scenario");
}
const occupied = scenario === "ring-occupied";
// A synchronous control read must not block Worker creation or HELLO. Root has
// authenticated and supplied the line before it publishes this real Ready.
if (occupied) await ready;
const control: unknown = occupied
  ? readControlLine()
  : JSON.parse(await Bun.stdin.text());
if (
  typeof control !== "object" ||
  control === null ||
  !("target" in control) ||
  typeof control.target !== "string" ||
  !("lateTarget" in control) ||
  typeof control.lateTarget !== "string"
) {
  throw new Error("invalid private public-FS fixture control");
}
await ready;
// This counts one public Promise handler, not wire ERRs or native syscall entries.
let settlements = 0;
let retainedEvents = 0;
let eventBeforeRelease = false;
let parkReleased = false;
const unsubscribe = occupied
  ? app.onLastWindowClosed(() => {
      retainedEvents += 1;
      eventBeforeRelease ||= !parkReleased;
    })
  : undefined;
const ringRead =
  scenario === "ring-fit" || scenario === "ring-over" || occupied;
const pending = ringRead
  ? fs.read(control.target)
  : fs.write(control.target, new Uint8Array([116, 50]));
if (occupied) {
  // Let the adapter's admission microtasks post the real FS CALL. Root also
  // requires its independent held-job receipt before queuing any event/reply.
  await new Promise<void>((resolve) => setImmediate(resolve));
  const marker = "KELD_WL park-enter=true\n";
  if (writeSync(1, marker) !== marker.length) {
    throw new Error(
      "occupied fixture did not publish its complete park marker",
    );
  }
  // No await separates the marker from this blocking control read. The main
  // realm cannot dispatch the retained event until Root supplies release byte1.
  const release = new Uint8Array(1);
  const count = readSync(0, release, 0, 1, null);
  report("release-count", count);
  report("release-byte", release[0]);
  if (count !== 1 || release[0] !== 1) {
    throw new Error("occupied fixture did not consume its exact release byte");
  }
  parkReleased = true;
  report("park-release", true);
}
await pending.then(
  (bytes) => {
    settlements += 1;
    if (ringRead) {
      if (!(bytes instanceof Uint8Array))
        throw new Error("public ring read returned no bytes");
      report("read-length", bytes.length);
      report(
        "read-sha256",
        new Bun.CryptoHasher("sha256").update(bytes).digest("hex"),
      );
    }
    report("call-returned", true);
  },
  (error: unknown) => {
    settlements += 1;
    if (!isCallError(error)) throw error;
    report("call-code", error.code);
    report("call-returned", false);
  },
);
if (occupied) {
  unsubscribe?.();
  report("retained-events", retainedEvents);
  report("event-before-release", eventBeforeRelease);
}
if (scenario === "retirement") {
  try {
    await fs.write(control.lateTarget, new Uint8Array([108, 97, 116, 101]));
    throw new Error("post-retirement public write unexpectedly succeeded");
  } catch (error: unknown) {
    if (!isCallError(error)) throw error;
    report("after-code", error.code);
  }
} else if (scenario === "healthy") {
  const bytes = await fs.read(control.target);
  report(
    "read-hex",
    Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(""),
  );
  report("read-returned", true);
}
report("settlements", settlements);
// No Quit, reconnect, or extra request in reply-wins/ring modes. Let the canonical
// Worker reach the parent's terminal EOF and flush diagnostics before natural exit.
