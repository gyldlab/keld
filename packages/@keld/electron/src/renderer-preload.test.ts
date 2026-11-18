/**
 * GH-567 F07-T4 staged facade-boundary corpus.
 *
 * A test-owned authenticated app-link Host captures the actual Window Create
 * request bytes. These tests prove only that the current BrowserWindow facade
 * sends geometry and no preload/argv admission frame. They do NOT run WKWebView,
 * observe first-inline execution, admit a preload path, or inspect renderer argv.
 * All three compatibility cells remain expected-fail under GH-484.
 */
import { describe, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { freshWin32AppLinkPipe } from "../../kipc/test/app-link-host.ts";
import {
  APP_LINK_IO_DEADLINE_MS,
  DrainSignal,
  FrameKind,
  FrameReader,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
  WriteQueue,
  encodeVarint,
  withIoDeadline,
} from "../../kipc/src/transport.ts";
import {
  encodeWindowEvent,
  encodeWindowResponse,
} from "../../api/src/window-codec.ts";
import type { BrowserWindowOptions } from "./index.ts";

const TOKEN_HEX = "72".repeat(32);
const TOKEN = new Uint8Array(32).fill(0x72);
const WINDOW = { id: 7, incarnation: 300n } as const;
const CREATE_GEOMETRY_GOLDEN = Uint8Array.from([
  0x00, 0x80, 0x05, 0xe0, 0x03, 0x01, 0x05, 0x01, 0x08,
]);

async function withPreloadFixture(
  run: (preload: string) => Promise<void>,
): Promise<void> {
  const root = mkdtempSync(join(import.meta.dir, ".gh567-preload-"));
  const preload = join(root, "preload.cjs");
  try {
    writeFileSync(preload, "globalThis.__gh567_preload__ = true;\n");
    await run(preload);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

async function captureCreateRequest(
  options: BrowserWindowOptions,
): Promise<Uint8Array> {
  const root =
    process.platform === "win32"
      ? undefined
      : mkdtempSync(join(tmpdir(), "krp"));
  if (root !== undefined) chmodSync(root, 0o700);
  const endpoint =
    root === undefined ? freshWin32AppLinkPipe() : join(root, "a.s");
  const reader = new FrameReader();
  const drain = new DrainSignal();
  let resolveOpen: (socket: Bun.Socket<undefined>) => void = () => undefined;
  const opened = new Promise<Bun.Socket<undefined>>((resolveOpenPromise) => {
    resolveOpen = resolveOpenPromise;
  });
  let resolveClosed: () => void = () => undefined;
  const closed = new Promise<void>((resolveClosedPromise) => {
    resolveClosed = resolveClosedPromise;
  });
  const listener = Bun.listen<undefined>({
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
      error(_socket, error) {
        reader.end(error);
      },
      close() {
        reader.end(new Error("KELD-IPC-001: GH567 role closed the app-link"));
        resolveClosed();
      },
    },
  });
  const roleScript = join(import.meta.dir, "renderer-preload.test-role.ts");
  const role = Bun.spawn(["bun", roleScript, JSON.stringify(options)], {
    env: {
      ...process.env,
      KELD_APP_LINK: `${endpoint}#${TOKEN_HEX}`,
      KELD_NATIVE_WINDOW_FACADE: "1",
    },
    stdout: "pipe",
    stderr: "pipe",
  });
  let roleExited = false;
  try {
    const socket = await withIoDeadline(opened, APP_LINK_IO_DEADLINE_MS);
    const writer = new WriteQueue(socket, drain);
    const next = () =>
      withIoDeadline(reader.readFrame(), APP_LINK_IO_DEADLINE_MS);

    const hello = await next();
    expect(hello.header.kind).toBe(FrameKind.Hello);
    expect(hello.payload).toEqual(TOKEN);
    await writer.writeFrame(FrameKind.Hello, 0, 0, 0, TOKEN);

    const subscribe = await next();
    expect(subscribe.header.kind).toBe(FrameKind.Call);
    expect(subscribe.header.channel).toBe(LIFECYCLE_CHANNEL);
    expect(Array.from(subscribe.payload)).toEqual([
      1,
      ...encodeVarint(WINDOW_CHANNEL),
    ]);
    await writer.writeFrame(
      FrameKind.Event,
      0,
      LIFECYCLE_CHANNEL,
      0,
      Uint8Array.of(0),
    );
    await writer.writeFrame(
      FrameKind.Reply,
      0,
      LIFECYCLE_CHANNEL,
      subscribe.header.corr,
      Uint8Array.of(1),
    );

    const create = await next();
    expect(create.header.kind).toBe(FrameKind.Call);
    expect(create.header.channel).toBe(WINDOW_CHANNEL);
    const requestBytes = new Uint8Array(create.payload);

    // Independent Rust-owned golden: request-create-positioned in
    // crates/keld-ipc/tests/fixtures/window-v0.tsv.
    expect(requestBytes).toEqual(CREATE_GEOMETRY_GOLDEN);

    const state = {
      bounds: { x: -3, y: 4, width: 640, height: 480 },
      focused: true,
      maximized: false,
      full_screen: false,
    };
    await writer.writeFrame(
      FrameKind.Event,
      0,
      WINDOW_CHANNEL,
      0,
      encodeWindowEvent({
        variant: "Created",
        window: WINDOW,
        state,
        pending_close: null,
      }),
    );
    await writer.writeFrame(
      FrameKind.Reply,
      0,
      WINDOW_CHANNEL,
      create.header.corr,
      encodeWindowResponse({ variant: "Created", window: WINDOW }),
    );

    const [exitCode, stdout, stderr] = await withIoDeadline(
      Promise.all([
        role.exited,
        new Response(role.stdout as ReadableStream<Uint8Array>).text(),
        new Response(role.stderr as ReadableStream<Uint8Array>).text(),
      ]),
      APP_LINK_IO_DEADLINE_MS,
    );
    roleExited = true;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("GH567_WINDOW_CREATED id=7");
    // A warning is deliberately not an oracle; stderr is captured only to
    // prevent a child pipe from filling and hiding the process exit.
    void stderr;

    await withIoDeadline(closed, APP_LINK_IO_DEADLINE_MS);
    // No second app-link frame (including a separate preload/argv admission
    // call) was sent after Create. FrameReader retains any unconsumed bytes.
    expect(reader.bufferedBytes()).toBe(0);
    return requestBytes;
  } finally {
    if (!roleExited) {
      role.kill();
      await role.exited;
    }
    listener.stop(true);
    if (root !== undefined) rmSync(root, { recursive: true, force: true });
  }
}

describe("GH-567 pending renderer preload conformance", () => {
  test("renderer.preload.before-first-inline facade sends no preload admission frame", async () => {
    await withPreloadFixture(async (preload) => {
      const request = await captureCreateRequest({
        width: 640,
        height: 480,
        x: -3,
        y: 4,
        webPreferences: { contextIsolation: true, preload },
      });
      expect(request).toEqual(CREATE_GEOMETRY_GOLDEN);
    });
  }, 30_000);

  test("renderer.preload.absolute-file-path facade does not forward the absolute path", async () => {
    await withPreloadFixture(async (preload) => {
      const request = await captureCreateRequest({
        width: 640,
        height: 480,
        x: -3,
        y: 4,
        webPreferences: { contextIsolation: true, preload },
      });
      expect(request).toEqual(CREATE_GEOMETRY_GOLDEN);
    });
  }, 30_000);

  test("renderer.additional-arguments.argv facade does not forward ordered arguments", async () => {
    const request = await captureCreateRequest({
      width: 640,
      height: 480,
      x: -3,
      y: 4,
      webPreferences: {
        contextIsolation: true,
        additionalArguments: ["--gh567=first", "second arg with spaces"],
      },
    });
    expect(request).toEqual(CREATE_GEOMETRY_GOLDEN);
  }, 30_000);
});
