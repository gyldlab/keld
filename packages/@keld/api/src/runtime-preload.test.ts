import { expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  FrameKind,
  FrameReader,
  WriteQueue,
  DrainSignal,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
  withIoDeadline,
} from "@keld/kipc";
import { decodeRuntimeProcessInit } from "@keld/kipc/runtime-process-facts";

import { freshWin32AppLinkPipe } from "../../kipc/test/app-link-host.ts";

type Message = {
  kind: string;
  entered?: boolean;
  before?: Record<string, string>;
  after?: Record<string, string>;
};
async function launch(observer: boolean) {
  const dir = mkdtempSync(join(tmpdir(), "krp-"));
  const endpoint =
    process.platform === "win32" ? freshWin32AppLinkPipe() : join(dir, "s");
  const reader = new FrameReader();
  const drain = new DrainSignal();
  let connections = 0;
  let open!: (socket: Bun.Socket<undefined>) => void;
  const opened = new Promise<Bun.Socket<undefined>>((resolve) => {
    open = resolve;
  });
  const server = Bun.listen<undefined>({
    unix: endpoint,
    socket: {
      binaryType: "uint8array",
      open(socket) {
        connections++;
        open(socket);
      },
      data(_socket, data) {
        reader.push(data);
      },
      drain() {
        drain.fire();
      },
      close() {
        reader.end(new Error("test peer closed"));
      },
      error() {},
    },
  });
  const messages: Message[] = [];
  let pending: ((message: Message) => void) | undefined;
  const command = [
    process.execPath,
    "--no-env-file",
    "--no-install",
    "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
    ...(observer
      ? [
          "--preload",
          join(import.meta.dir, "runtime-preload-observer.fixture.mjs"),
        ]
      : []),
    join(import.meta.dir, "host-selected-runtime.fixture.mjs"),
  ];
  const proc = Bun.spawn(command, {
    env: {
      ...process.env,
      KELD_APP_LINK: endpoint + "#" + "72".repeat(32),
      KELD_NATIVE_WINDOW_FACADE: "1",
    },
    stdout: "pipe",
    stderr: "pipe",
    ipc(message: Message) {
      if (pending) {
        const resolve = pending;
        pending = undefined;
        resolve(message);
      } else messages.push(message);
    },
  });
  async function message(): Promise<Message> {
    return (
      messages.shift() ??
      withIoDeadline(
        new Promise<Message>((resolve) => {
          pending = resolve;
        }),
      )
    );
  }
  const socket = await withIoDeadline(opened);
  const writer = new WriteQueue(socket, drain);
  const hello = await withIoDeadline(reader.readFrame());
  expect(hello.header.kind).toBe(FrameKind.Hello);
  await writer.writeFrame(
    FrameKind.Hello,
    0,
    0,
    0,
    new Uint8Array(32).fill(0x72),
  );
  const init = await withIoDeadline(reader.readFrame());
  expect(init.header).toMatchObject({
    kind: FrameKind.Call,
    channel: LIFECYCLE_CHANNEL,
    corr: 1,
    flags: 0,
  });
  const captured = decodeRuntimeProcessInit(init.payload);
  const saved = init.payload.slice();
  return {
    proc,
    reader,
    writer,
    captured,
    saved,
    message,
    connections: () => connections,
    async ack(value = 3) {
      await writer.writeFrame(
        FrameKind.Reply,
        0,
        LIFECYCLE_CHANNEL,
        1,
        Uint8Array.of(value),
      );
    },
    async subscribe() {
      const call = await withIoDeadline(reader.readFrame());
      expect(call.header.kind).toBe(FrameKind.Call);
      expect(call.header.channel).toBe(LIFECYCLE_CHANNEL);
      expect([...call.payload]).toEqual([1, WINDOW_CHANNEL]);
      await writer.writeFrame(
        FrameKind.Reply,
        0,
        LIFECYCLE_CHANNEL,
        call.header.corr,
        Uint8Array.of(1),
      );
      await writer.writeFrame(
        FrameKind.Event,
        0,
        LIFECYCLE_CHANNEL,
        0,
        Uint8Array.of(0),
      );
    },
    async dispose() {
      proc.kill();
      await proc.exited;
      socket.end();
      server.stop(true);
      rmSync(dir, { recursive: true, force: true });
    },
  };
}

test("Host-selected original holds entry through Init ACK then reuses one WorkerLink", async () => {
  const run = await launch(false);
  try {
    await run.ack();
    await run.subscribe();
    const entered = await run.message();
    expect(entered.kind).toBe("entry");
    expect(run.captured.versions).toEqual(entered.before!);
    expect(entered.after!.__keld_late).toBe("changed-after-entry");
    expect(run.captured.versions.__keld_late).toBeUndefined();
    expect(decodeRuntimeProcessInit(run.saved)).toEqual(run.captured);
    expect((await run.message()).kind).toBe("ready");
    expect(run.connections()).toBe(1);
  } finally {
    await run.dispose();
  }
}, 15000);

test("independent main-realm probe proves entry cannot run while ACK is held", async () => {
  const run = await launch(true);
  try {
    run.proc.send({ kind: "probe" });
    expect(await run.message()).toEqual({ kind: "probe", entered: false });
    await run.ack();
    await run.subscribe();
    expect((await run.message()).kind).toBe("entry");
    expect((await run.message()).kind).toBe("ready");
  } finally {
    await run.dispose();
  }
}, 15000);

test("wrong Init acknowledgement closes the link before application entry", async () => {
  const run = await launch(true);
  try {
    run.proc.send({ kind: "probe" });
    expect(await run.message()).toEqual({ kind: "probe", entered: false });
    await run.ack(1);
    expect(await withIoDeadline(run.proc.exited)).not.toBe(0);
    const stderr = await new Response(run.proc.stderr).text();
    expect(stderr).toContain("Runtime facts require");
    expect(run.connections()).toBe(1);
  } finally {
    await run.dispose();
  }
}, 15000);
