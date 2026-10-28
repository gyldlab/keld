/** Real installed API → canonical Worker → private protocol peer; no native Host claim. */
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { LifecycleLink } from "@keld/api/internal/link";
import { encodeWindowEvent } from "@keld/api/internal/window-codec";
import {
  DrainSignal,
  ECHO_CHANNEL,
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
const generation = process.argv[2];
assert.ok(generation === "initial" || generation === "recovery");
const tokenByte = generation === "initial" ? 0x72 : 0x73;
const token = new Uint8Array(32).fill(tokenByte);
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
const ref =
  generation === "initial"
    ? { id: 7, incarnation: 300n }
    : { id: 8, incarnation: 301n };
const oldRef = { id: 7, incarnation: 300n };
let facts = 0;
const connect = LifecycleLink.connect(
  `${path}#${tokenByte.toString(16).repeat(32)}`,
  {
    onReady() {},
    onLastWindowClosed() {},
    onConnected(value) {
      session = value;
    },
    onWindowFact(fact) {
      facts += 1;
      assert.equal(fact.variant, "Created");
      assert.deepEqual(fact.window, ref);
      assert.equal(session?.windowSource?.get(ref)?.window.id, ref.id);
      resolveFact();
    },
    async onApplicationCall(channel, payload) {
      assert.equal(channel, ECHO_CHANNEL);
      return payload;
    },
    onLinkDead() {
      resolveEnd();
    },
  },
);
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
  // Replay precedes the subscription reply on this generation's own link.
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
  assert.equal(
    connected.windowSource.get(ref)?.window.id,
    ref.id,
    "GENERATION_REPLAY_NOT_APPLIED",
  );
  if (generation === "recovery")
    assert.equal(connected.windowSource.get(oldRef), undefined);
  try {
    await withIoDeadline(factSeen, 1500);
  } catch (error) {
    throw new Error("SOURCE_FACT_NOT_DISPATCHED", { cause: error });
  }
  const source = connected.windowSource;
  assert.equal(source.get(ref)?.window.id, ref.id);
  const echoPayload = Uint8Array.of(0, 0);
  await writer.writeFrame(FrameKind.Call, 0, ECHO_CHANNEL, 77, echoPayload);
  const echo = await withIoDeadline(reader.readFrame(), 1500);
  assert.equal(echo.header.kind, FrameKind.Reply);
  assert.equal(echo.header.channel, ECHO_CHANNEL);
  assert.equal(echo.header.corr, 77);
  assert.deepEqual(echo.payload, echoPayload);
  assert.equal(source.get(ref)?.window.id, ref.id);
  assert.equal(facts, 1);
  socket.end();
  await withIoDeadline(ended, 1500);
  assert.throws(() => source.all(), /KELD-IPC-022/);
  await assert.rejects(
    connected.callWindow({ variant: "Destroy", window: ref }),
    /KELD-IPC-022/,
  );
  assert.equal(count, 1);
  assert.equal(facts, 1);
  console.log(`PASS installed-api-real-worker-source-end ${generation}`);
} finally {
  session?.close();
  listener.stop(true);
  rmSync(root, { recursive: true, force: true });
}
