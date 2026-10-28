/** Real installed API → canonical Worker → private protocol peer; no native Host claim. */
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { LifecycleLink } from "@keld/api/internal/link";
import { encodeWindowEvent } from "@keld/api/internal/window-codec";
import {
  DrainSignal,
  FrameKind,
  FrameReader,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
  WriteQueue,
  withIoDeadline,
} from "@keld/kipc";

const root = mkdtempSync(join(tmpdir(), "keld-package-host-"));
chmodSync(root, 0o700);
const path = join(root, "host.sock");
const token = new Uint8Array(32).fill(0x72);
const reader = new FrameReader();
const drain = new DrainSignal();
let accept!: (socket: Bun.Socket<undefined>) => void;
const accepted = new Promise<Bun.Socket<undefined>>((resolve) => {
  accept = resolve;
});
let count = 0;
const listener = Bun.listen<undefined>({
  unix: path,
  socket: {
    binaryType: "uint8array",
    open(socket) {
      count += 1;
      accept(socket);
    },
    data(_socket, data) {
      reader.push(data);
    },
    drain() {
      drain.fire();
    },
    error(_socket, error) {
      reader.end(error);
    },
    close() {
      reader.end(new Error("peer closed"));
    },
  },
});
let session: LifecycleLink | undefined;
let resolveFact!: () => void;
const factSeen = new Promise<void>((resolve) => {
  resolveFact = resolve;
});
let resolveEnd!: () => void;
const ended = new Promise<void>((resolve) => {
  resolveEnd = resolve;
});
const ref = { id: 7, incarnation: 300n };
const connect = LifecycleLink.connect(`${path}#${"72".repeat(32)}`, {
  onReady() {},
  onLastWindowClosed() {},
  onConnected(value) {
    session = value;
  },
  onWindowFact(fact) {
    assert.equal(fact.variant, "Created");
    assert.equal(session?.windowSource?.get(ref)?.window.id, ref.id);
    resolveFact();
  },
  onLinkDead() {
    resolveEnd();
  },
});
try {
  const socket = await withIoDeadline(accepted, 1500);
  const writer = new WriteQueue(socket, drain);
  const hello = await withIoDeadline(reader.readFrame(), 1500);
  assert.equal(hello.header.kind, FrameKind.Hello);
  assert.deepEqual(hello.payload, token);
  await writer.writeFrame(FrameKind.Hello, 0, 0, 0, token);
  const subscribe = await withIoDeadline(reader.readFrame(), 1500);
  assert.equal(subscribe.header.kind, FrameKind.Call);
  assert.equal(subscribe.header.channel, LIFECYCLE_CHANNEL);
  await writer.writeFrame(
    FrameKind.Reply,
    0,
    LIFECYCLE_CHANNEL,
    subscribe.header.corr,
    Uint8Array.of(1),
  );
  const connected = await withIoDeadline(connect, 1500);
  assert.equal(connected, session);
  assert.ok(connected.windowSource);
  await writer.writeFrame(
    FrameKind.Event,
    0,
    WINDOW_CHANNEL,
    0,
    encodeWindowEvent({
      variant: "Created",
      window: ref,
      state: {
        bounds: { x: 0, y: 0, width: 640, height: 480 },
        focused: false,
        maximized: false,
        full_screen: false,
      },
      pending_close: null,
    }),
  );
  try {
    await withIoDeadline(factSeen, 1500);
  } catch (error) {
    throw new Error("SOURCE_FACT_NOT_DISPATCHED", { cause: error });
  }
  const source = connected.windowSource;
  assert.equal(source.get(ref)?.window.id, ref.id);
  socket.end();
  await withIoDeadline(ended, 1500);
  assert.throws(() => source.all(), /KELD-IPC-022/);
  await assert.rejects(
    connected.callWindow({ variant: "Destroy", window: ref }),
    /KELD-IPC-022/,
  );
  assert.equal(count, 1);
  console.log("PASS installed-api-real-worker-source-end");
} finally {
  session?.close();
  listener.stop(true);
  rmSync(root, { recursive: true, force: true });
}
