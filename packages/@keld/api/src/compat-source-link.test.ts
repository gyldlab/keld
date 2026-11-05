/** Real socket/Worker/Source ordering; scripted peer proves no native or Guard effect. */
import { expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  COMPAT_CONTROL_CHANNEL,
  DrainSignal,
  FrameKind,
  FrameReader,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
  WriteQueue,
  encodeCallError,
  withIoDeadline,
} from "@keld/kipc";
import { freshWin32AppLinkPipe } from "../../kipc/test/app-link-host.ts";
import { encodeWindowEvent } from "./window-codec.ts";

async function exchange(
  outcome: "fresh" | "successor" | "denied",
): Promise<void> {
  const root =
    process.platform === "win32"
      ? undefined
      : mkdtempSync(join(tmpdir(), "kcb"));
  if (root) chmodSync(root, 0o700);
  const endpoint =
    root === undefined ? freshWin32AppLinkPipe() : join(root, "a.s");
  const reader = new FrameReader();
  const drain = new DrainSignal();
  let accept!: (socket: Bun.Socket<undefined>) => void;
  const accepted = new Promise<Bun.Socket<undefined>>((resolve) => {
    accept = resolve;
  });
  let connections = 0;
  const listener = Bun.listen<undefined>({
    unix: endpoint,
    socket: {
      binaryType: "uint8array",
      open(socket) {
        connections++;
        accept(socket);
      },
      data(_socket, data) {
        reader.push(data);
      },
      drain() {
        drain.fire();
      },
      close() {
        reader.end(new Error("peer closed"));
      },
      error(_socket, error) {
        reader.end(error);
      },
    },
  });
  const role = Bun.spawn(
    [
      process.execPath,
      join(import.meta.dir, "compat-source-link.test-role.ts"),
      outcome,
    ],
    {
      env: {
        ...process.env,
        KELD_APP_LINK: `${endpoint}#${"73".repeat(32)}`,
        KELD_NATIVE_WINDOW_FACADE: "1",
      },
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  try {
    const socket = await withIoDeadline(accepted);
    const writer = new WriteQueue(socket, drain);
    const hello = await withIoDeadline(reader.readFrame());
    expect(hello.header.kind).toBe(FrameKind.Hello);
    expect(hello.payload).toEqual(new Uint8Array(32).fill(0x73));
    await writer.writeFrame(FrameKind.Hello, 0, 0, 0, hello.payload);
    await writer.writeFrame(
      FrameKind.Event,
      0,
      LIFECYCLE_CHANNEL,
      0,
      Uint8Array.of(0),
    );
    const windows = await withIoDeadline(reader.readFrame());
    expect(windows.header.channel).toBe(LIFECYCLE_CHANNEL);
    expect(windows.payload).toEqual(Uint8Array.of(1, WINDOW_CHANNEL));
    const window = { id: 7, incarnation: 258n };
    await writer.writeFrame(
      FrameKind.Event,
      0,
      WINDOW_CHANNEL,
      0,
      encodeWindowEvent({
        variant: "Created",
        window,
        state: {
          bounds: { x: 0, y: 0, width: 640, height: 480 },
          focused: true,
          maximized: false,
          full_screen: false,
        },
        pending_close: null,
      }),
    );
    await writer.writeFrame(
      FrameKind.Event,
      0,
      WINDOW_CHANNEL,
      0,
      encodeWindowEvent({
        variant: "NavigationCommitted",
        window,
        document_generation: 2,
        native_url: "keld-app://actual/index.html",
      }),
    );
    await writer.writeFrame(
      FrameKind.Reply,
      0,
      LIFECYCLE_CHANNEL,
      windows.header.corr,
      Uint8Array.of(1),
    );
    const compat = await withIoDeadline(reader.readFrame());
    expect(compat.header.channel).toBe(LIFECYCLE_CHANNEL);
    expect(compat.payload).toEqual(Uint8Array.of(1, COMPAT_CONTROL_CHANNEL));
    // No RendererMessage is sent anywhere in this exchange.
    await writer.writeFrame(
      FrameKind.Event,
      0,
      COMPAT_CONTROL_CHANNEL,
      0,
      Uint8Array.of(2, 1, 2, 7, 0x82, 2),
    );
    if (outcome === "successor") {
      await writer.writeFrame(
        FrameKind.Event,
        0,
        WINDOW_CHANNEL,
        0,
        encodeWindowEvent({
          variant: "NavigationCommitted",
          window,
          document_generation: 4,
          native_url: "keld-app://actual/index.html",
        }),
      );
      await writer.writeFrame(
        FrameKind.Event,
        0,
        COMPAT_CONTROL_CHANNEL,
        0,
        Uint8Array.of(2, 1, 4, 7, 0x82, 2),
      );
      await writer.writeFrame(
        FrameKind.Event,
        0,
        COMPAT_CONTROL_CHANNEL,
        0,
        Uint8Array.of(1, 1, 2),
      );
    }
    await writer.writeFrame(
      FrameKind.Reply,
      0,
      LIFECYCLE_CHANNEL,
      compat.header.corr,
      Uint8Array.of(1),
    );
    const call = await withIoDeadline(reader.readFrame());
    expect(call.header.kind).toBe(FrameKind.Call);
    expect(call.header.channel).toBe(COMPAT_CONTROL_CHANNEL);
    // Exact DeliverToOriginalDocument: real view 1, never WindowRef.id 7.
    expect(call.payload).toEqual(
      Uint8Array.of(
        1,
        1,
        outcome === "successor" ? 4 : 2,
        10,
        105,
        115,
        77,
        111,
        100,
        105,
        102,
        105,
        101,
        100,
        1,
        0,
      ),
    );
    await writer.writeFrame(
      outcome === "denied" ? FrameKind.Err : FrameKind.Reply,
      0,
      COMPAT_CONTROL_CHANNEL,
      call.header.corr,
      outcome === "denied"
        ? encodeCallError(
            "KELD-GUARD003",
            "channel is forbidden by the final Host grant",
          )
        : Uint8Array.of(0, 0),
    );
    const [status, stdout, stderr] = await withIoDeadline(
      Promise.all([
        role.exited,
        new Response(role.stdout).text(),
        new Response(role.stderr).text(),
      ]),
    );
    expect(status).toBe(0);
    expect(stderr).toBe("");
    expect(stdout).toBe(
      "PASS bound-before-first-renderer-message " + outcome + "\n",
    );
    expect(connections).toBe(1);
  } finally {
    if (role.exitCode === null) role.kill();
    await role.exited;
    listener.stop(true);
    if (root) rmSync(root, { recursive: true, force: true });
  }
}

test("main sends from Ready over the existing link before any renderer DATA", () =>
  exchange("fresh"));
test("late A retirement cannot redirect the first link delivery from B", () =>
  exchange("successor"));
test("bound target does not turn a final Host denial into delivery success", () =>
  exchange("denied"));
