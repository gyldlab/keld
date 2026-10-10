/** Public adapter controls over the real canonical WorkerLink; no native FS authority claim. */
import { describe, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  DEFAULT_RING_BYTES,
  DrainSignal,
  ECHO_CHANNEL,
  FS_CHANNEL,
  FrameKind,
  FrameReader,
  HEADER_LEN,
  LIFECYCLE_CHANNEL,
  WriteQueue,
  decodeHeader,
  encodePostcardString,
  encodeVarint,
  withIoDeadline,
} from "../../kipc/src/transport.ts";
import { fs } from "./index.ts";

const TOKEN = new Uint8Array(32).fill(0x72);

async function rejection(promise: Promise<unknown>): Promise<Error> {
  try { await promise; } catch (error) {
    if (error instanceof Error) return error;
    throw error;
  }
  throw new Error("operation unexpectedly succeeded");
}

test("local argument and Unicode failures always return rejected Promises", async () => {
  for (const call of [
    () => fs.read(7 as unknown as string),
    () => fs.read("\ud800"),
    () => fs.write("/p", [1] as unknown as Uint8Array),
    () => fs.write("\udfff", new Uint8Array()),
  ]) {
    let promise: Promise<unknown> | undefined;
    expect(() => { promise = call(); }).not.toThrow();
    expect(promise).toBeInstanceOf(Promise);
    expect((await rejection(promise!)).message).toStartWith("KELD-IPC-003");
  }
});

test("public index exports typed fs without private call or codec seams", async () => {
  const api = await import("./index.ts");
  expect(Object.keys(api.fs).sort()).toEqual(["read", "write"]);
  for (const internal of ["invokeFs", "LifecycleLink", "encodeFsRequest", "decodeFsResponse"]) {
    expect(Object.hasOwn(api, internal)).toBe(false);
  }
});

interface Host {
  next(): ReturnType<FrameReader["readFrame"]>;
  send(kind: number, channel: number, corr: number, payload: Uint8Array): Promise<void>;
  reported(key: string): Promise<void>;
  finish(): Promise<string>;
  connections(): number;
  receivedChannels(): number[];
  close(): void;
  awaitClose(): Promise<void>;
  releasePark(): void;
  dispose(): Promise<void>;
}

async function start(scenario: string): Promise<Host> {
  const root = mkdtempSync(join(tmpdir(), "kf"));
  chmodSync(root, 0o700);
  const path = join(root, "a.s");
  const reader = new FrameReader();
  const drain = new DrainSignal();
  let connectionCount = 0;
  const received: number[] = [];
  let buffered = new Uint8Array();
  let open!: (socket: Bun.Socket<undefined>) => void;
  const opened = new Promise<Bun.Socket<undefined>>((resolve) => { open = resolve; });
  let close!: () => void;
  const closed = new Promise<void>((resolve) => { close = resolve; });
  const listener = Bun.listen<undefined>({ unix: path, socket: {
    binaryType: "uint8array",
    open(socket) { connectionCount += 1; open(socket); },
    data(_socket, data) {
      const combined = new Uint8Array(buffered.length + data.length);
      combined.set(buffered); combined.set(data, buffered.length); buffered = combined;
      while (buffered.length >= HEADER_LEN) {
        const header = decodeHeader(buffered.subarray(0, HEADER_LEN));
        const size = HEADER_LEN + header.len;
        if (buffered.length < size) break;
        received.push(header.channel);
        buffered = buffered.slice(size);
      }
      reader.push(data);
    },
    drain() { drain.fire(); },
    error(_socket, error) { reader.end(error); },
    close() { reader.end(new Error("host observed role close")); close(); },
  } });
  const role = Bun.spawn(["bun", join(import.meta.dir, "fs.test-role.ts"), scenario], {
    env: { ...process.env, KELD_APP_LINK: `${path}#${"72".repeat(32)}` },
    stdin: "pipe", stdout: "pipe", stderr: "pipe",
  });
  let text = "";
  const waiters: Array<{ key: string; resolve: () => void; reject: (error: Error) => void }> = [];
  const stdout = (async () => {
    const stream = role.stdout.getReader();
    const decoder = new TextDecoder();
    for (;;) {
      const part = await stream.read();
      if (part.done) {
        for (const waiter of waiters.splice(0)) {
          waiter.reject(new Error(`role exited before reporting ${waiter.key}; output=${text}`));
        }
        return text;
      }
      text += decoder.decode(part.value, { stream: true });
      for (let index = waiters.length - 1; index >= 0; index -= 1) {
        if (text.includes(`${waiters[index]!.key}=`)) waiters.splice(index, 1)[0]!.resolve();
      }
    }
  })();
  const stderr = new Response(role.stderr).text();
  try {
    const socket = await withIoDeadline(opened);
    const writer = new WriteQueue(socket, drain);
    const send = (kind: number, channel: number, corr: number, payload: Uint8Array) =>
      writer.writeFrame(kind, 0, channel, corr, payload);
    const hello = await withIoDeadline(reader.readFrame());
    expect(hello.header.kind).toBe(FrameKind.Hello);
    expect(hello.payload).toEqual(TOKEN);
    await send(FrameKind.Hello, 0, 0, TOKEN);
    await send(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([0]));
    return {
      next: () => withIoDeadline(reader.readFrame()), send,
      reported: (key) => withIoDeadline(new Promise<void>((resolve, reject) => {
        waiters.push({ key, resolve, reject });
        if (text.includes(`${key}=`)) waiters.splice(waiters.length - 1, 1)[0]!.resolve();
      })),
      async finish() {
        const [code, output, errors] = await withIoDeadline(Promise.all([role.exited, stdout, stderr]));
        expect(code).toBe(0);
        if (scenario === "dead") expect(errors).toContain("KELD-IPC-022");
        else if (scenario.startsWith("capacity-") && scenario !== "capacity-fit") {
          expect({ errors, output }).toEqual({
            errors: expect.stringContaining("KELD-IPC-026"),
            output: expect.stringContaining('overflow="KELD-IPC-026"'),
          });
        } else expect(errors).toBe("");
        expect(output).toContain("done=true");
        return output;
      },
      connections: () => connectionCount,
      receivedChannels: () => received,
      close: () => socket.end(),
      awaitClose: () => withIoDeadline(closed),
      releasePark: () => { role.stdin.write(new Uint8Array([1])); role.stdin.flush(); },
      async dispose() {
        if (role.exitCode === null) role.kill();
        await role.exited;
        listener.stop(true); rmSync(root, { recursive: true, force: true });
      },
    };
  } catch (error) {
    if (role.exitCode === null) role.kill();
    await role.exited; listener.stop(true); rmSync(root, { recursive: true, force: true });
    throw error;
  }
}

async function replyQuit(host: Host): Promise<void> {
  const quit = await host.next();
  expect(quit.header.channel).toBe(LIFECYCLE_CHANNEL);
  expect(quit.payload).toEqual(new Uint8Array([0]));
  await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, quit.header.corr, new Uint8Array([0]));
}

function readResponse(length: number): Uint8Array {
  const size = encodeVarint(length);
  const payload = new Uint8Array(1 + size.length + length);
  payload.set(size, 1);
  return payload;
}

function fitReadLength(): number {
  let size = DEFAULT_RING_BYTES - HEADER_LEN;
  while (HEADER_LEN + readResponse(size).length > DEFAULT_RING_BYTES) size -= 1;
  return size;
}

describe.skipIf(process.platform === "win32")("public FS on the existing singleton", () => {
  test("write snapshots call-entry bytes and lifecycle, Echo and FS share one connection", async () => {
    const host = await start("snapshot");
    try {
      await host.send(FrameKind.Call, ECHO_CHANNEL, 77, new Uint8Array([0, 0]));
      let write: Awaited<ReturnType<Host["next"]>> | undefined;
      let echo = false;
      while (write === undefined || !echo) {
        const frame = await host.next();
        if (frame.header.channel === ECHO_CHANNEL) {
          expect(frame.header.kind).toBe(FrameKind.Reply);
          expect(frame.header.corr).toBe(77);
          expect(frame.payload).toEqual(new Uint8Array([0, 0]));
          echo = true;
        } else {
          expect(frame.header.channel).toBe(FS_CHANNEL);
          expect(frame.payload).toEqual(new Uint8Array([1, 2, 47, 112, 3, 0, 255, 65]));
          write = frame;
        }
      }
      await host.send(FrameKind.Reply, FS_CHANNEL, write.header.corr, new Uint8Array([1]));
      const read = await host.next();
      expect(read.header.channel).toBe(FS_CHANNEL);
      expect(read.payload).toEqual(new Uint8Array([0, 2, 47, 112]));
      await host.send(FrameKind.Reply, FS_CHANNEL, read.header.corr, new Uint8Array([0, 3, 0, 255, 65]));
      await replyQuit(host);
      expect(await host.finish()).toContain("read=[0,255,65]");
      expect(host.connections()).toBe(1);
    } finally { await host.dispose(); }
  });

  test("wrong variants, unknown/truncated/trailing codec replies reject without retiring the link", async () => {
    const host = await start("variants");
    try {
      for (const response of [[1], [0, 0, 0], [0, 2, 5], [127], [0, 0], [0, 3, 0, 255, 65], [1]]) {
        const call = await host.next();
        expect(call.header.channel).toBe(FS_CHANNEL);
        await host.send(FrameKind.Reply, FS_CHANNEL, call.header.corr, new Uint8Array(response));
      }
      await replyQuit(host);
      expect(await host.finish()).toContain("read=[0,255,65]");
      expect(host.connections()).toBe(1);
    } finally { await host.dispose(); }
  });

  test("native ERR preserves its original code, message and fix", async () => {
    const host = await start("host-error");
    try {
      const call = await host.next();
      const message = "KELD-GUARD002: outside scope. Fix: choose an allowed path.";
      const code = encodePostcardString("KELD-GUARD002");
      const detail = encodePostcardString(message);
      const payload = new Uint8Array(code.length + detail.length);
      payload.set(code); payload.set(detail, code.length);
      await host.send(FrameKind.Err, FS_CHANNEL, call.header.corr, payload);
      await replyQuit(host);
      expect(await host.finish()).toContain(`error=${JSON.stringify({ code: "KELD-GUARD002", message })}`);
    } finally { await host.dispose(); }
  });

  test("pending Quit after Ready rejects public calls before its reply and emits zero FS CALLs", async () => {
    const host = await start("pending-quit");
    try {
      const quit = await host.next();
      expect(quit.header.channel).toBe(LIFECYCLE_CHANNEL);
      // Keep Quit's reply held. A removed admission guard must fail on the
      // actual forbidden frame immediately, rather than waiting for a call deadline.
      const unexpected = host.next().then(
        (frame) => ({ outcome: "unexpected-frame", kind: frame.header.kind,
          channel: frame.header.channel, corr: frame.header.corr }),
        (error: unknown) => ({ outcome: "unexpected-close", error: String(error) }),
      );
      const rejectedBeforeReply = host.reported("rejected-before-quit-reply")
        .then(() => ({ outcome: "rejected-before-reply" }));
      expect(await Promise.race([unexpected, rejectedBeforeReply]))
        .toEqual({ outcome: "rejected-before-reply" });
      await host.send(FrameKind.Reply, LIFECYCLE_CHANNEL, quit.header.corr, new Uint8Array([0]));
      await host.finish();
      expect(host.receivedChannels().filter((channel) => channel === FS_CHANNEL)).toEqual([]);
    } finally { await host.dispose(); }
  });

  test("explicit private lifecycle close retains ordinary closed-error shape and emits no FS", async () => {
    const host = await start("close");
    try {
      expect(await host.finish()).toContain("closed=true");
      expect(host.receivedChannels().filter((channel) => channel === FS_CHANNEL)).toEqual([]);
    } finally { await host.dispose(); }
  });

  test("external link death retains the exact recorded error object", async () => {
    const host = await start("dead");
    try {
      expect((await host.next()).header.channel).toBe(FS_CHANNEL);
      host.close();
      expect(await host.finish()).toContain('"code":"KELD-IPC-022"');
    } finally { await host.dispose(); }
  });

  for (const scenario of ["capacity-fit", "capacity-over", "capacity-occupied"]) {
    test(`async read ${scenario} keeps the canonical ring boundary`, async () => {
      const host = await start(scenario);
      try {
        const call = await host.next();
        expect(call.header.kind).toBe(FrameKind.Call);
        expect(call.header.channel).toBe(FS_CHANNEL);
        const size = fitReadLength();
        expect(HEADER_LEN + readResponse(size).length).toBe(DEFAULT_RING_BYTES);
        const response = readResponse(size + (scenario === "capacity-over" ? 1 : 0));
        if (scenario === "capacity-occupied") {
          await host.reported("parked");
          // Retain a HEADER_LEN+1 event while main is blocked. The following
          // exact-full-ring reply must overflow those occupied bytes.
          await host.send(FrameKind.Event, LIFECYCLE_CHANNEL, 0, new Uint8Array([1]));
        }
        await host.send(FrameKind.Reply, FS_CHANNEL, call.header.corr, response);
        if (scenario === "capacity-fit") {
          await replyQuit(host);
          expect(await host.finish()).toContain(`length=${size}`);
        } else {
          if (scenario === "capacity-occupied") {
            // A canonical Ping is answered by the Worker only after processing
            // preceding frames, without draining the parked realm's ring. If
            // pressure is removed, its reply directly proves the frame fit.
            const probeWrite = host.send(FrameKind.Ping, 0, 95, new Uint8Array())
              .then(() => undefined, (error: unknown) => error);
            const processed = host.next().then(
              (frame) => ({ outcome: "worker-processed-without-overflow",
                kind: frame.header.kind, channel: frame.header.channel, corr: frame.header.corr }),
              async () => { await host.awaitClose(); return { outcome: "socket-closed" }; },
            );
            const outcome = await Promise.race([
              processed,
              host.awaitClose().then(() => ({ outcome: "socket-closed" })),
            ]);
            host.releasePark();
            await probeWrite;
            expect(outcome).toEqual({ outcome: "socket-closed" });
          }
          expect(await host.finish()).toContain('overflow="KELD-IPC-026"');
        }
      } finally { await host.dispose(); }
    }, 15_000);
  }
});
