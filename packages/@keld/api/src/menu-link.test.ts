/** Real one-Worker/socket adapter tests; scripted peer proves no AppKit/Guard effect. */
import { expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  DrainSignal,
  FrameKind,
  FrameReader,
  LIFECYCLE_CHANNEL,
  MENU_CHANNEL,
  WriteQueue,
  encodeCallError,
  withIoDeadline,
} from "@keld/kipc";
import { freshWin32AppLinkPipe } from "../../kipc/test/app-link-host.ts";

async function exchange(
  outcome: "receipt" | "malformed" | "denied" | "setter",
): Promise<void> {
  const root =
    process.platform === "win32"
      ? undefined
      : mkdtempSync(join(tmpdir(), "km"));
  if (root) chmodSync(root, 0o700);
  const endpoint =
    root === undefined ? freshWin32AppLinkPipe() : join(root, "a.s");
  const reader = new FrameReader();
  const drain = new DrainSignal();
  let connections = 0;
  let accept!: (socket: Bun.Socket<undefined>) => void;
  const accepted = new Promise<Bun.Socket<undefined>>((resolve) => {
    accept = resolve;
  });
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
      join(import.meta.dir, "menu-link.test-role.ts"),
      outcome,
    ],
    {
      env: {
        ...process.env,
        KELD_APP_LINK: `${endpoint}#${"73".repeat(32)}`,
        KELD_NATIVE_WINDOW_FACADE: "",
      },
      stdin: "pipe",
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
      new Uint8Array([0]),
    );
    const call = await withIoDeadline(reader.readFrame());
    expect(call.header.kind).toBe(FrameKind.Call);
    expect(call.header.channel).toBe(MENU_CHANNEL);
    expect([...call.payload]).toEqual([1, 0, 0, 1, 88, 1, 0, 0, 8]);
    if (outcome === "denied")
      await writer.writeFrame(
        FrameKind.Err,
        0,
        MENU_CHANNEL,
        call.header.corr,
        encodeCallError("KELD-GUARD001", "Add menu at /app/system"),
      );
    else
      await writer.writeFrame(
        FrameKind.Reply,
        0,
        MENU_CHANNEL,
        call.header.corr,
        new Uint8Array(
          outcome === "receipt" || outcome === "setter" ? [172, 2] : [0],
        ),
      );
    if (outcome === "setter") {
      role.stdin.write(new Uint8Array([1]));
      await role.stdin.flush();
    }
    const [status, stdout, stderr] = await withIoDeadline(
      Promise.all([
        role.exited,
        new Response(role.stdout).text(),
        new Response(role.stderr).text(),
      ]),
    );
    expect(status).toBe(0);
    expect(stderr).toBe("");
    expect(connections).toBe(1);
    expect(stdout).toBe(
      outcome === "setter"
        ? "returned=undefined\nsetter-done\n"
        : outcome === "receipt"
          ? "installed-generation=300\n"
          : outcome === "malformed"
            ? "refused=KELD-IPC-003\n"
            : "refused=KELD-GUARD001\n",
    );
  } finally {
    if (role.exitCode === null) role.kill();
    await role.exited;
    listener.stop(true);
    if (root) rmSync(root, { recursive: true, force: true });
  }
}
test("GH480 menu install sends exact wire bytes over the existing authenticated WorkerLink and observes the receipt", () =>
  exchange("receipt"));
test("GH480 malformed native menu receipt is rejected instead of claiming installation", () =>
  exchange("malformed"));
test("GH480 guard ERR remains typed on the one existing menu call", () =>
  exchange("denied"));

test("GH480 public void setter sends an immutable role-over-click snapshot over its existing app owner", () =>
  exchange("setter"));
