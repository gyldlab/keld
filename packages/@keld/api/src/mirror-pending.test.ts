/** Scripted authenticated host; no native WebView or Guard effect is claimed. */
import { expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  DrainSignal,
  ECHO_CHANNEL,
  FrameKind,
  FrameReader,
  LIFECYCLE_CHANNEL,
  WriteQueue,
  withIoDeadline,
} from "@keld/kipc";
import { freshWin32AppLinkPipe } from "../../kipc/test/app-link-host.ts";

async function exercise(
  mode: "visitor-failure" | "close-during-scan",
): Promise<void> {
  const root =
    process.platform === "win32"
      ? undefined
      : mkdtempSync(join(tmpdir(), "keld-scan-"));
  if (root) chmodSync(root, 0o700);
  const endpoint =
    root === undefined ? freshWin32AppLinkPipe() : join(root, "pipe");
  const reader = new FrameReader();
  const drain = new DrainSignal();
  let accept!: (socket: Bun.Socket<undefined>) => void;
  const accepted = new Promise<Bun.Socket<undefined>>((resolve) => {
    accept = resolve;
  });
  const listener = Bun.listen<undefined>({
    unix: endpoint,
    socket: {
      binaryType: "uint8array",
      open(socket) {
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
      end(socket) {
        reader.end(new Error("peer closed"));
        socket.end();
      },
      error(_socket, error) {
        reader.end(error);
      },
    },
  });
  const child = Bun.spawn(
    [
      process.execPath,
      join(import.meta.dir, "mirror-pending.test-role.ts"),
      mode,
    ],
    {
      env: { ...process.env, KELD_APP_LINK: `${endpoint}#${"73".repeat(32)}` },
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  const stdoutRead = new Response(child.stdout).text();
  const stderrRead = new Response(child.stderr).text();
  try {
    const socket = await withIoDeadline(accepted, 10_000);
    const writer = new WriteQueue(socket, drain);
    const hello = await withIoDeadline(reader.readFrame(), 10_000);
    expect(hello.header.kind).toBe(FrameKind.Hello);
    expect(hello.payload).toEqual(new Uint8Array(32).fill(0x73));
    await writer.writeFrame(FrameKind.Hello, 0, 0, 0, hello.payload);
    for (const [name, values] of [
      ["scan-first", [1, 2]],
      ["scan-wrap", [4, 5]],
    ] as const) {
      const call = await withIoDeadline(reader.readFrame(), 10_000);
      expect(call.header.kind).toBe(FrameKind.Call);
      expect(call.header.channel).toBe(ECHO_CHANNEL);
      expect(new TextDecoder().decode(call.payload)).toBe(name);
      for (const value of values)
        await writer.writeFrame(
          FrameKind.Event,
          0,
          LIFECYCLE_CHANNEL,
          0,
          Uint8Array.of(value),
        );
      await writer.writeFrame(
        FrameKind.Reply,
        0,
        ECHO_CHANNEL,
        call.header.corr,
        Uint8Array.of(0),
      );
    }
    const [exit, stdout, stderr] = await withIoDeadline(
      Promise.all([child.exited, stdoutRead, stderrRead]),
      10_000,
    );
    expect(exit, stderr).toBe(0);
    expect(stdout).toContain("PASS pending-facts");
  } catch (error) {
    if (child.exitCode === null) child.kill();
    await child.exited;
    const [stdout, stderr] = await Promise.all([stdoutRead, stderrRead]);
    throw new Error(
      `scripted host failed: ${String(error)}\nchild stdout: ${stdout}\nchild stderr: ${stderr}`,
    );
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
    listener.stop(true);
    if (root) rmSync(root, { recursive: true, force: true });
  }
}

test(
  "existing mirror slots preserve order and terminal custody through scans",
  () => exercise("visitor-failure"),
  30_000,
);
test(
  "a link closed by the last visitor refuses the pending scan",
  () => exercise("close-during-scan"),
  30_000,
);
