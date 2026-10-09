/**
 * #653: the transport stamp (spec gh527 §4.13). Every file a transport Worker
 * may evaluate starts with one stamp line binding the SHA-256 of exactly the
 * bytes after it. `WorkerLink.open` reads its own module file and refuses an
 * unstamped or stale one before it spawns the Worker, so a bundle that carries
 * app code is never evaluated by a transport Worker, whatever its file name.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "bun:test";

import { TRANSPORT_STAMP_PREFIX, isCallError, stampTransport, verifyTransportStamp } from "./transport.ts";
import { countedOpenAndReport, runLayout, transportPath } from "../test/bundle-layout.ts";

const canonicalPath = join(import.meta.dir, "transport.ts");
const canonical = readFileSync(canonicalPath);

function unstamped(source: string): string {
  return source.slice(source.indexOf("\n") + 1);
}

function stampCode(source: Uint8Array): string {
  try {
    verifyTransportStamp(source, "transport.ts");
    return "valid";
  } catch (err) {
    return isCallError(err) ? err.code : `untyped:${String(err)}`;
  }
}

describe("the transport stamp", () => {
  test("the canonical transport is stamped over exactly its remaining bytes", () => {
    const text = canonical.toString("utf8");
    expect(text.startsWith(TRANSPORT_STAMP_PREFIX)).toBe(true);
    expect(stampTransport(unstamped(text))).toBe(text);
    expect(stampCode(canonical)).toBe("valid");
  });

  test("one changed byte, a missing or malformed stamp, or a CRLF stamp line is KELD-IPC-005", () => {
    const text = canonical.toString("utf8");
    const flipped = Buffer.from(canonical);
    flipped[flipped.length - 2] ^= 0x01;
    const firstLine = text.slice(0, text.indexOf("\n"));
    const cases: Record<string, string> = {
      "changed byte": flipped.toString("utf8"),
      "no stamp": unstamped(text),
      "uppercase digest": `${TRANSPORT_STAMP_PREFIX}${firstLine.slice(TRANSPORT_STAMP_PREFIX.length).toUpperCase()}\n${unstamped(text)}`,
      "short digest": `${firstLine.slice(0, -1)}\n${unstamped(text)}`,
      "crlf stamp line": `${firstLine}\r\n${unstamped(text)}`,
      "empty file": "",
    };
    const outcomes = Object.fromEntries(
      Object.entries(cases).map(([name, source]) => [name, stampCode(new TextEncoder().encode(source))]),
    );
    expect(outcomes).toEqual(Object.fromEntries(Object.keys(cases).map((name) => [name, "KELD-IPC-005"])));
  });

  test("the refusal names the file and the fix", () => {
    let message = "";
    try {
      verifyTransportStamp(new TextEncoder().encode(unstamped(canonical.toString("utf8"))), "transport.js");
    } catch (err) {
      message = err instanceof Error ? err.message : String(err);
    }
    expect(message).toContain("KELD-IPC-005");
    expect(message).toContain("`transport.js`");
    expect(message).toContain("keld create");
  });
});

// A transport-only build has the transport's code but not its bytes, so a
// bundler cannot carry a valid stamp by accident: the build must restamp
// what it stages (gh527 §4.13, the `keld build` rule).
describe("a transpiled transport-only build", () => {
  const opener = (file: string) =>
    `import { WorkerLink, isCallError } from "./${file}";\n` +
    "WorkerLink.open({ link: process.env.KELD_APP_LINK, receive: { eventChannels: [3], callReceivers: [] } }).then(\n" +
    '  () => console.log("main-open=opened"),\n' +
    '  (err) => console.log(`main-open=${isCallError(err) ? err.code : "untyped"}`),\n' +
    ");\n";

  async function run(restamp: boolean) {
    return runLayout({
      closeOnOpen: true,
      ready: (out) => out.includes("main-open="),
      async write(dir: string): Promise<string> {
        const built = await Bun.build({ entrypoints: [canonicalPath], target: "bun", format: "esm" });
        expect(built.success).toBe(true);
        const js = await built.outputs[0]!.text();
        await Bun.write(join(dir, "kipc-transport.js"), restamp ? stampTransport(js) : js);
        await Bun.write(join(dir, "main.js"), opener("kipc-transport.js"));
        return "main.js";
      },
    });
  }

  test("is refused unstamped, before anything connects", async () => {
    const outcome = await run(false);
    expect({ stdout: outcome.stdout.trim(), connections: outcome.connections }).toEqual({
      stdout: "main-open=KELD-IPC-005",
      connections: 0,
    });
  }, 30_000);

  test("passes the stamp once restamped, so the stamp alone decided the refusal", async () => {
    const outcome = await run(true);
    expect(outcome.connections).toBe(1);
    expect(outcome.stdout.trim()).not.toBe("main-open=KELD-IPC-005");
    expect(outcome.stdout.trim().startsWith("main-open=KELD-IPC-")).toBe(true);
  }, 30_000);
});

// The stamp binds bytes, not intent: a bundle someone restamps on purpose
// passes it. The in-Worker check (#643) still stops that bundle's app code
// from opening a nested link from inside its transport Worker.
test("a deliberately restamped app bundle cannot open a nested link from its transport Worker", async () => {
  const outcome = await runLayout({
    closeOnOpen: true,
    ready: (out) => out.includes("main-open="),
    async write(dir: string): Promise<string> {
      const entry = join(dir, "entry.ts");
      await Bun.write(entry, countedOpenAndReport);
      const built = await Bun.build({ entrypoints: [entry], target: "bun", format: "esm" });
      expect(built.success).toBe(true);
      await Bun.write(join(dir, "transport.js"), stampTransport(await built.outputs[0]!.text()));
      await Bun.write(join(dir, "main.js"), 'import "./transport.js";\n');
      return "main.js";
    },
  });
  expect(outcome.stdout).toContain("worker-open=KELD-IPC-005");
  expect(outcome.evals).toBe(2);
  expect(transportPath).toContain("transport.ts");
}, 30_000);
