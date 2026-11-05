/** Installed facade identity/component proof, not native authentication or release proof. */
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomBytes } from "node:crypto";
const directory = mkdtempSync(join(tmpdir(), "koi-"));
const endpoint =
  process.platform === "win32"
    ? String.raw`\\.\pipe\keld-${randomBytes(32).toString("hex")}`
    : join(directory, "s");
process.env.KELD_NATIVE_WINDOW_FACADE = "1";
process.env.KELD_APP_LINK = endpoint + "#" + "73".repeat(32);
const kipc = await import("@keld/kipc");
const windowCodec = await import("@keld/api/internal/window-codec");
const compat = await import("@keld/kipc/compat-control");
const endpointCodec = await import("@keld/kipc/endpoint-value-codec");
const reader = new kipc.FrameReader(),
  drain = new kipc.DrainSignal();
let resolveOpen;
const opened = new Promise((resolve) => {
  resolveOpen = resolve;
});
const server = Bun.listen({
  unix: endpoint,
  socket: {
    binaryType: "uint8array",
    open(socket) {
      resolveOpen(socket);
    },
    data(_socket, data) {
      reader.push(data);
    },
    drain() {
      drain.fire();
    },
    close() {
      reader.end(new Error("fixture peer closed"));
    },
    error() {},
  },
});
let socket;
try {
  // Load the private alias first: it must share the facade's later minted events.
  const helper = await import("electron/internal/original-ipc");
  const electron = await import("electron");
  const ready = electron.app.whenReady();
  socket = await kipc.withIoDeadline(opened);
  const writer = new kipc.WriteQueue(socket, drain);
  const next = () => kipc.withIoDeadline(reader.readFrame());
  const hello = await next();
  assert.equal(hello.header.kind, kipc.FrameKind.Hello);
  await writer.writeFrame(
    kipc.FrameKind.Hello,
    0,
    0,
    0,
    new Uint8Array(32).fill(0x73),
  );
  const window = { id: 7, incarnation: 300n };
  const event = (payload) =>
    writer.writeFrame(
      kipc.FrameKind.Event,
      0,
      kipc.WINDOW_CHANNEL,
      0,
      windowCodec.encodeWindowEvent(payload),
    );
  for (const channel of [kipc.WINDOW_CHANNEL, kipc.COMPAT_CONTROL_CHANNEL]) {
    const request = await next();
    assert.equal(request.header.kind, kipc.FrameKind.Call);
    assert.equal(request.header.channel, kipc.LIFECYCLE_CHANNEL);
    assert.deepEqual([...request.payload], [1, channel]);
    if (channel === kipc.WINDOW_CHANNEL) {
      await event({
        variant: "Created",
        window,
        state: {
          bounds: { x: 0, y: 0, width: 800, height: 600 },
          focused: true,
          maximized: false,
          full_screen: false,
        },
        pending_close: null,
      });
      await event({
        variant: "NavigationCommitted",
        window,
        document_generation: 5,
        native_url: "keld-resource://fixture/index.html",
      });
      await event({
        variant: "NavigationFinished",
        window,
        document_generation: 5,
      });
    }
    await writer.writeFrame(
      kipc.FrameKind.Reply,
      0,
      kipc.LIFECYCLE_CHANNEL,
      request.header.corr,
      Uint8Array.of(1),
    );
  }
  await writer.writeFrame(
    kipc.FrameKind.Event,
    0,
    kipc.LIFECYCLE_CHANNEL,
    0,
    Uint8Array.of(0),
  );
  await ready;
  const owned = electron.BrowserWindow.getAllWindows()[0];
  assert(owned);
  owned.on("close", (event) => {
    event.preventDefault();
  });
  await event({ variant: "CloseRequested", window, close_seq: 55n });
  const veto = await next();
  assert.equal(veto.header.channel, kipc.WINDOW_CHANNEL);
  assert.deepEqual(windowCodec.decodeWindowRequest(veto.payload), {
    variant: "CloseReply",
    window,
    close_seq: 55n,
    verdict: "Veto",
  });
  let vetoAccepted = false;
  let captured,
    resolveOriginal,
    rejectOriginal,
    settled = false;
  const original = new Promise((resolve, reject) => {
    resolveOriginal = resolve;
    rejectOriginal = reject;
  });
  void original.catch(() => {}); // Preserve the exact failure for the peer-stage race.
  electron.ipcMain.once("original-fixture", (event) => {
    captured = event;
    try {
      assert.equal(
        helper.isCurrentOriginalIpcDocument(event),
        true,
        "same installed retained-event capability",
      );
      assert.equal(
        helper.isCurrentOriginalIpcDocument(Object.create(event)),
        false,
      );
      void helper
        .awaitOriginalIpcCloseVeto(event)
        .then(() => helper.destroyOriginalIpcDocument(event))
        .then(() => {
          settled = true;
          resolveOriginal();
        }, rejectOriginal);
    } catch (error) {
      rejectOriginal(error);
    }
  });
  await writer.writeFrame(
    kipc.FrameKind.Event,
    0,
    kipc.COMPAT_CONTROL_CHANNEL,
    0,
    compat.encodeCompatControlEvent({
      variant: "RendererMessage",
      webview: 17,
      navigation: 5,
      message: 1,
      window_id: 7,
      window_incarnation: 300n,
      url: "keld-resource://fixture/index.html",
      origin: "keld-resource://fixture",
      topic: "original-fixture",
      payload: endpointCodec.encodeEndpointArguments([]),
    }),
  );
  let physical = false;
  while (!physical) {
    const request = await Promise.race([
      next(),
      original.then(() => {
        throw new Error("teardown resolved before physical ACK");
      }),
    ]);
    assert.equal(request.header.kind, kipc.FrameKind.Call);
    if (request.header.channel === kipc.COMPAT_CONTROL_CHANNEL) {
      assert.deepEqual(compat.decodeCompatControlRequest(request.payload), {
        variant: "ReleaseConsumed",
        webview: 17,
        navigation: 5,
        message: 1,
        disposition: { variant: "Consumed" },
      });
      await writer.writeFrame(
        kipc.FrameKind.Reply,
        0,
        request.header.channel,
        request.header.corr,
        compat.encodeCompatControlResponse({ variant: "Released" }),
      );
      // This actual correlated Veto receipt is deliberately held until the
      // original event dispatched and physically left its shared ring.
      assert.equal(
        settled,
        false,
        "local callback completion is not Veto acceptance",
      );
      await writer.writeFrame(
        kipc.FrameKind.Reply,
        0,
        veto.header.channel,
        veto.header.corr,
        windowCodec.encodeWindowResponse({ variant: "ReplyApplied" }),
      );
      vetoAccepted = true;
    } else {
      assert.equal(request.header.channel, kipc.WINDOW_CHANNEL);
      assert.equal(
        vetoAccepted,
        true,
        "conditional teardown cannot precede actual Veto receipt",
      );
      assert.deepEqual(windowCodec.decodeWindowRequest(request.payload), {
        variant: "DestroyOriginalDocument",
        window,
        document_generation: 5,
      });
      assert.equal(settled, false, "queued request is not physical ACK");
      if (process.argv[2] !== "missing-destroyed") {
        await event({ variant: "Destroyed", window });
        await writer.writeFrame(
          kipc.FrameKind.Event,
          0,
          kipc.LIFECYCLE_CHANNEL,
          0,
          Uint8Array.of(1),
        );
      }
      await writer.writeFrame(
        kipc.FrameKind.Reply,
        0,
        request.header.channel,
        request.header.corr,
        windowCodec.encodeWindowResponse({ variant: "Destroyed" }),
      );
      physical = true;
    }
  }
  await kipc.withIoDeadline(original);
  assert.equal(settled, true);
  assert.equal(helper.isCurrentOriginalIpcDocument(captured), false);
  assert.equal(captured.sender.isDestroyed(), true);
  const quitting = electron.app.quit();
  while (true) {
    const request = await next();
    if (request.header.channel === kipc.COMPAT_CONTROL_CHANNEL) {
      await writer.writeFrame(
        kipc.FrameKind.Reply,
        0,
        request.header.channel,
        request.header.corr,
        compat.encodeCompatControlResponse({ variant: "Released" }),
      );
      continue;
    }
    assert.equal(request.header.channel, kipc.LIFECYCLE_CHANNEL);
    assert.deepEqual([...request.payload], [0]);
    await writer.writeFrame(
      kipc.FrameKind.Reply,
      0,
      request.header.channel,
      request.header.corr,
      Uint8Array.of(0),
    );
    break;
  }
  await quitting;
  console.log("INSTALLED_ORIGINAL_IPC_SHARED_CAPABILITY");
} finally {
  socket?.end();
  server.stop(true);
  rmSync(directory, { recursive: true, force: true });
}
