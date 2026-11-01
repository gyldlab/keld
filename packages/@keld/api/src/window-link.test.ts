/** Real socket consumer contracts; no native Init, facade readiness or authority claim. */
import { chmodSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, test } from "bun:test";
import {
  APP_LINK_IO_DEADLINE_MS, DrainSignal, ECHO_CHANNEL, FS_CHANNEL, FrameKind, FrameReader,
  LIFECYCLE_CHANNEL, WINDOW_CHANNEL, WriteQueue, encodeCallError, withIoDeadline,
} from "../../kipc/src/transport.ts";

const testQualifiedHardware = test.skipIf(process.platform !== "linux" || process.arch !== "x64");

const TOKEN = new Uint8Array(32).fill(0x49);
const TIMEOUT = APP_LINK_IO_DEADLINE_MS * 3;
const createdHex = readFileSync(join(import.meta.dir, "../../../../crates/keld-ipc/tests/fixtures/window-v0.tsv"), "utf8").split("\n").find((row) => row.startsWith("event-created\tevent\t"))?.split("\t")[2];
if (!createdHex) throw new Error("missing Rust-owned Created literal");
const CREATED = Uint8Array.from(Buffer.from(createdHex, "hex"));
const OTHER_CREATED = CREATED.slice();
OTHER_CREATED[1] = 3; OTHER_CREATED[2] = 0xad; // Host pair (3,301), same pinned state.

interface Host {
  next(): ReturnType<FrameReader["readFrame"]>;
  send(kind: number, channel: number, corr: number, payload: Uint8Array): Promise<void>;
  reported(key: string): Promise<void>;
  releasePark(): void;
  closePeer(): void;
  finish(): Promise<string>;
  dispose(): Promise<void>;
}
async function start(scenario: string): Promise<Host> {
  const root = mkdtempSync(join(tmpdir(), "kw"));
  chmodSync(root, 0o700);
  const path = join(root, "a.s");
  const reader = new FrameReader();
  const drain = new DrainSignal();
  let open!: (socket: Bun.Socket<undefined>) => void;
  const opened = new Promise<Bun.Socket<undefined>>((resolve) => { open = resolve; });
  let close!: () => void;
  const closed = new Promise<void>((resolve) => { close = resolve; });
  let connections = 0;
  const listener = Bun.listen<undefined>({ unix: path, socket: {
    binaryType: "uint8array",
    open(socket) { connections += 1; open(socket); },
    data(_socket, bytes) { reader.push(bytes); },
    drain() { drain.fire(); },
    error(_socket, error) { reader.end(error); },
    close() { reader.end(new Error("host observed role close")); close(); },
  } });
  const role = Bun.spawn(["bun", join(import.meta.dir, "window-link.test-role.ts"), scenario], {
    env: { ...process.env, KELD_APP_LINK: `${path}#${"49".repeat(32)}` },
    stdin: "pipe", stdout: "pipe", stderr: "pipe",
  });
  let output = "";
  const waiters: Array<{ key: string; resolve: () => void; reject: (error: Error) => void }> = [];
  const stdout = (async () => {
    const stream = role.stdout.getReader();
    const decoder = new TextDecoder();
    for (;;) {
      const part = await stream.read();
      if (part.done) {
        for (const waiter of waiters.splice(0)) waiter.reject(new Error(`role exited before ${waiter.key}; output=${output}`));
        return output;
      }
      output += decoder.decode(part.value, { stream: true });
      for (let index = waiters.length - 1; index >= 0; index -= 1) {
        if (output.includes(`${waiters[index]!.key}=`)) waiters.splice(index, 1)[0]!.resolve();
      }
    }
  })();
  const stderr = new Response(role.stderr).text();
  try {
    const socket = await withIoDeadline(opened, TIMEOUT);
    const writer = new WriteQueue(socket, drain);
    const hello = await withIoDeadline(reader.readFrame(), TIMEOUT);
    expect(hello.header.kind).toBe(FrameKind.Hello);
    expect(hello.payload).toEqual(TOKEN);
    await writer.writeFrame(FrameKind.Hello, 0, 0, 0, TOKEN);
    return {
      next: () => withIoDeadline(reader.readFrame(), TIMEOUT),
      send: (kind, channel, corr, payload) => writer.writeFrame(kind, 0, channel, corr, payload),
      reported: (key) => withIoDeadline(new Promise<void>((resolve, reject) => {
        if (output.includes(`${key}=`)) resolve();
        else waiters.push({ key, resolve, reject });
      }), TIMEOUT),
      releasePark() { role.stdin.write(Uint8Array.of(1)); role.stdin.flush(); },
      closePeer() { socket.end(); },
      async finish() {
        const [code, text, errors] = await withIoDeadline(Promise.all([role.exited, stdout, stderr]), TIMEOUT);
        if (scenario.startsWith("ignored-")) console.log(`child-observation=${JSON.stringify({ scenario, code, text, errors })}`);
        expect({ code, text, errors }).toEqual({ code: 0, text: expect.stringContaining("done=true"), errors: scenario === "earlier-answer" ? expect.stringContaining("KELD-IPC-005") : scenario === "subscription-peer-close" ? expect.stringContaining("KELD-IPC-022") : "" });
        await withIoDeadline(closed, TIMEOUT);
        // No hidden Subscribe, getter CALL or post-close Window operation remains.
        await expect(withIoDeadline(reader.readFrame(), TIMEOUT)).rejects.toThrow("host observed role close");
        expect(connections).toBe(1);
        return text;
      },
      async dispose() {
        if (role.exitCode === null) role.kill();
        await role.exited;
        listener.stop(true);
        rmSync(root, { recursive: true, force: true });
      },
    };
  } catch (error) {
    if (role.exitCode === null) role.kill();
    await role.exited;
    listener.stop(true); rmSync(root, { recursive: true, force: true });
    throw error;
  }
}
async function subscribe(host: Host): Promise<number> {
  const request = await host.next();
  expect(request.header.kind).toBe(FrameKind.Call);
  expect(request.header.channel).toBe(LIFECYCLE_CHANNEL);
  expect(request.header.corr).toBeGreaterThan(0);
  expect(request.payload).toEqual(Uint8Array.of(1, WINDOW_CHANNEL));
  return request.header.corr;
}
async function replay(host: Host, corr: number, empty = false): Promise<void> {
  if (!empty) {
    await host.send(FrameKind.Event, WINDOW_CHANNEL, 0, CREATED);
    await host.send(FrameKind.Event, WINDOW_CHANNEL, 0, OTHER_CREATED);
  }
  await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, corr, Uint8Array.of(1));
}
async function fsBarrier(host: Host): Promise<void> {
  const request = await host.next();
  expect(request.header.kind).toBe(FrameKind.Call);
  expect(request.header.channel).toBe(FS_CHANNEL);
  await host.send(FrameKind.Reply, FS_CHANNEL, request.header.corr, Uint8Array.of(1));
}

describe.skipIf(process.platform === "win32")("LifecycleLink window consumer over a Unix host", () => {
test("default lifecycle consumer sends no Subscribe and preserves lifecycle-only behavior", async () => {
  const host = await start("default");
  try {
    await host.send(FrameKind.Event, LIFECYCLE_CHANNEL, 0, Uint8Array.of(0));
    await fsBarrier(host);
    await host.finish();
  } finally { await host.dispose(); }
});

for (const scenario of ["replay", "empty", "subscribed-before-ready"]) {
  test(`window subscription: ${scenario} uses a separate completed-replay barrier`, async () => {
    const host = await start(scenario);
    try {
      const corr = await subscribe(host);
      await host.reported("connected"); // connect cannot await its Subscribe answer.
      if (scenario !== "subscribed-before-ready") {
        await host.send(FrameKind.Event, LIFECYCLE_CHANNEL, 0, Uint8Array.of(0));
        await fsBarrier(host);
        await host.reported("ready-without-subscription");
      }
      if (scenario === "replay") {
        await host.send(FrameKind.Call, ECHO_CHANNEL, 90, Uint8Array.of(0, 0));
        const echo = await host.next();
        expect(echo.header.kind).toBe(FrameKind.Reply);
        expect(echo.header.channel).toBe(ECHO_CHANNEL);
        expect(echo.header.corr).toBe(90);
        expect(echo.payload).toEqual(Uint8Array.of(0, 0));
      }
      await replay(host, corr, scenario === "empty");
      if (scenario === "subscribed-before-ready") {
        await host.reported("subscribed-without-ready");
        await host.send(FrameKind.Event, LIFECYCLE_CHANNEL, 0, Uint8Array.of(0));
      }
      const output = await host.finish();
      expect(output).toContain('subscription={"accepted":true}');
      expect(output).toContain(scenario === "empty" ? "windows=[]" : "windows=[3,7]");
    } finally { await host.dispose(); }
  }, TIMEOUT);
}

for (const [reason, byte] of [["UnknownChannel", 0], ["NotSubscribable", 1], ["GuardedNotSupported", 2], ["RoleNotAdmitted", 3]] as const) {
  test(`subscription refusal ${reason} remains typed and does not close the source early`, async () => {
    const host = await start(`refusal-${byte}`);
    try {
      const corr = await subscribe(host);
      await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, corr, Uint8Array.of(2, byte));
      expect(await host.finish()).toContain(`subscription={"accepted":false,"reason":"${reason}"}`);
    } finally { await host.dispose(); }
  }, TIMEOUT);
}

for (const payload of [[], [0], [1, 0], [2], [2, 4], [2, 0, 0], [3], [0x80], [0xff, 0xff, 0xff, 0xff, 0x10]]) {
  test(`malformed or wrong subscription reply ${JSON.stringify(payload)} rejects without early source end`, async () => {
    const host = await start("malformed");
    try {
      const corr = await subscribe(host);
      await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, corr, Uint8Array.from(payload));
      await fsBarrier(host);
      const output = await host.finish();
      const detail = payload.length === 0 || JSON.stringify(payload) === "[2]" || JSON.stringify(payload) === "[128]"
        ? "truncated varint"
        : payload.length === 5 ? "u32 varint exceeds five bytes or overflows its fifth byte"
        : "invalid lifecycle window subscription reply; check the host lifecycle schema";
      expect(output).toContain(`subscription-error=${JSON.stringify(`KELD-IPC-003: ${detail}`)}`);
      expect(output).toContain('subscription-error-shape={"name":"Error","hasCode":false}');
    } finally { await host.dispose(); }
  }, TIMEOUT);
}

for (const scenario of ["subscription-err", "subscription-peer-close", "subscription-deadline"]) {
  test(`${scenario} preserves the existing error code`, async () => {
    const host = await start(scenario);
    try {
      const corr = await subscribe(host);
      if (scenario === "subscription-err") await host.send(FrameKind.Err, LIFECYCLE_CHANNEL, corr, encodeCallError("KELD-CORE-042", "KELD-CORE-042: native path unavailable; select supported host"));
      else if (scenario === "subscription-peer-close") host.closePeer();
      const output = await host.finish();
      expect(output).toContain(scenario === "subscription-err" ? '"code":"KELD-CORE-042"' : scenario === "subscription-peer-close" ? '"code":"KELD-IPC-022"' : '"code":"KELD-IPC-006"');
      expect(output).toContain("exact-raw-diagnostic=true");
    } finally { await host.dispose(); }
  }, TIMEOUT);
}

test("parked window call applies host facts before return and local close drains listeners before source end", async () => {
  const host = await start("blocking-close");
  try {
    const corr = await subscribe(host);
    await host.send(FrameKind.Event, WINDOW_CHANNEL, 0, CREATED);
    await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, corr, Uint8Array.of(1));
    await host.reported("park-ready"); host.releasePark();
    const request = await host.next();
    expect(request.header.channel).toBe(WINDOW_CHANNEL);
    expect(request.header.corr).not.toBe(corr);
    await host.send(FrameKind.Event, WINDOW_CHANNEL, 0, Uint8Array.of(2, 7, 0xac, 2, 0));
    await host.send(FrameKind.Reply, WINDOW_CHANNEL, request.header.corr, Uint8Array.of(1));
    expect(await host.finish()).toContain('drain=["Created","Focus","end"]');
  } finally { await host.dispose(); }
}, TIMEOUT);

test("pending Quit refuses further Window calls and ends source only after retained facts drain", async () => {
  const host = await start("quit");
  try {
    const corr = await subscribe(host);
    await host.send(FrameKind.Event, WINDOW_CHANNEL, 0, CREATED);
    await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, corr, Uint8Array.of(1));
    const quit = await host.next();
    expect(quit.header.channel).toBe(LIFECYCLE_CHANNEL);
    expect(quit.payload).toEqual(Uint8Array.of(0));
    await host.reported("quit-guarded");
    await host.send(FrameKind.Event, WINDOW_CHANNEL, 0, Uint8Array.of(2, 7, 0xac, 2, 0));
    await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, quit.header.corr, Uint8Array.of(0));
    expect(await host.finish()).toContain('drain=["Created","Focus","end"]');
  } finally { await host.dispose(); }
}, TIMEOUT);

test("failed Window source preserves earlier answers and existing unanswered-caller error priority", async () => {
  const host = await start("earlier-answer");
  try {
    const corr = await subscribe(host);
    await host.send(FrameKind.Event, WINDOW_CHANNEL, 0, CREATED);
    await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, corr, Uint8Array.of(1));
    const first = await host.next();
    const pending = await host.next();
    expect(first.header.channel).toBe(WINDOW_CHANNEL);
    expect(pending.header.channel).toBe(WINDOW_CHANNEL);
    expect(pending.header.corr).not.toBe(first.header.corr);
    await host.send(FrameKind.Reply, WINDOW_CHANNEL, first.header.corr, Uint8Array.of(9));
    await host.send(FrameKind.Event, WINDOW_CHANNEL, 0, Uint8Array.of(7));
    expect(await host.finish()).toContain("earlier-answer=true");
  } finally { await host.dispose(); }
}, TIMEOUT);

for (const scenario of ["release-close", "release-quit"]) {
  testQualifiedHardware(`${scenario}: real post-drain end releases source Map snapshots while source and external alias survive`, async () => {
    const host = await start(scenario);
    try {
      const corr = await subscribe(host);
      await replay(host, corr);
      await host.reported("park-ready"); host.releasePark();
      const request = await host.next();
      expect(request.header.kind).toBe(FrameKind.Call);
      expect(request.header.channel).toBe(WINDOW_CHANNEL);
      await host.send(FrameKind.Event, WINDOW_CHANNEL, 0, Uint8Array.of(2, 7, 0xac, 2, 0));
      await host.send(FrameKind.Reply, WINDOW_CHANNEL, request.header.corr, Uint8Array.of(1));
      if (scenario === "release-quit") {
        const quit = await host.next();
        expect(quit.header.channel).toBe(LIFECYCLE_CHANNEL);
        expect(quit.payload).toEqual(Uint8Array.of(0));
        await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, quit.header.corr, Uint8Array.of(0));
      }
      const output = await host.finish();
      console.log(output.trim()); // Primitive release observation only, no heap output.
      expect(output).toContain('release-observation={"live":0,"sourceAlive":true,"externalAlive":true,"sequence":["Created","Created","Focus","transport-end","end"]}');
    } finally { await host.dispose(); }
  }, TIMEOUT);
}


testQualifiedHardware("original actual subscription RPC Error releases while Link and Source remain healthy", async () => {
  const host = await start("subscription-original-release");
  try {
    const corr = await subscribe(host);
    await host.reported("connected");
    await host.send(FrameKind.Err, LIFECYCLE_CHANNEL, corr, encodeCallError("KELD-CORE-042", "KELD-CORE-042: native path unavailable; select supported host"));
    await host.reported("raw-rpc-error");
    await fsBarrier(host);
    await host.reported("original-error-release");
    await host.reported("completion-observation");
    await fsBarrier(host);
    const output = await host.finish();
    console.log(output.trim()); // Actual primitive observations only.
    expect(output).toContain('"originalErrorAlive":false,"linkAlive":true,"sourceAlive":true,"externalErrorAlive":true');
    expect(output).toContain('"samePromise":false,"sameError":false,"ownProperty":false,"prototypeGetter":true,"enumerable":false,"configurable":true');
    expect(output).toContain('"firstMessage":"KELD-CORE-042: native path unavailable; select supported host","secondMessage":"KELD-CORE-042: native path unavailable; select supported host"');
  } finally { await host.dispose(); }
}, TIMEOUT);


for (const scenario of ["ignored-before-err", "ignored-after-err", "ignored-presence-guard"]) {
  test(`${scenario}: discarded completion preserves exact awaited rejection and child exit0`, async () => {
    const host = await start(scenario);
    try {
      const corr = await subscribe(host);
      await host.reported("connected");
      if (scenario !== "ignored-after-err") await host.reported("ignored-read");
      await host.send(FrameKind.Err, LIFECYCLE_CHANNEL, corr, encodeCallError("KELD-CORE-042", "KELD-CORE-042: native path unavailable; select supported host"));
      if (scenario === "ignored-after-err") {
        await fsBarrier(host);
        await host.reported("ignored-read");
      }
      await fsBarrier(host);
      const output = await host.finish();
      expect(output).toContain('awaited-rejection={"code":"KELD-CORE-042","message":"KELD-CORE-042: native path unavailable; select supported host"}');
      expect(output).toContain("healthy-followup=true");
      expect(output).toContain("survived-next-turn=true");
    } finally { await host.dispose(); }
  }, TIMEOUT);
}
});
