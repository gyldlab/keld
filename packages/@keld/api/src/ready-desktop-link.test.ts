import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "bun:test";
import {
  APP_LINK_IO_DEADLINE_MS,
  withIoDeadline,
  FrameKind,
  FrameReader,
  WriteQueue,
  DrainSignal,
  WINDOW_CHANNEL,
  LIFECYCLE_CHANNEL,
} from "@keld/kipc";
import { freshWin32AppLinkPipe } from "../../kipc/test/app-link-host.ts";

for (const mode of [
  "valid",
  "missing",
  "invalidated",
  "late-ready",
  "public-ready",
  "unavailable",
  "unselected",
  "foreign-host",
]) {
  test(`desktop Ready over the existing link: ${mode}`, async () => {
    const child = Bun.spawn(
      [
        process.execPath,
        `--define=__KELD_WINDOWS_HOST__:${mode !== "foreign-host"}`,
        ...(mode === "unselected"
          ? []
          : ["--define=__KELD_WINDOWS_DESKTOP_SNAPSHOT__:true"]),
        join(import.meta.dir, "ready-desktop-link.test-role.ts"),
        mode,
      ],
      {
        env: {
          ...process.env,
          KELD_NATIVE_WINDOW_FACADE: "1",
          __KELD_WINDOWS_DESKTOP_SNAPSHOT__: "true",
        },
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    try {
      const [exit, stdout, stderr] = await withIoDeadline(
        Promise.all([
          child.exited,
          new Response(child.stdout).text(),
          new Response(child.stderr).text(),
        ]),
        APP_LINK_IO_DEADLINE_MS,
      );
      assert.equal(exit, 0, `${stdout}\n${stderr}`);
    } finally {
      if (child.exitCode === null) child.kill();
      await child.exited;
    }
  });
}

for (const mode of ["snapshot-first", "ack-first"]) {
  test(`desktop Subscribe reads the exact reply cut in one retained batch: ${mode}`, async () => {
    const dir = mkdtempSync(join(tmpdir(), "keld-desktop-ring-"));
    const endpoint =
      process.platform === "win32" ? freshWin32AppLinkPipe() : join(dir, "s");
    const reader = new FrameReader();
    const drain = new DrainSignal();
    const { promise: opened, resolve } =
      Promise.withResolvers<Bun.Socket<undefined>>();
    const server = Bun.listen<undefined>({
      unix: endpoint,
      socket: {
        binaryType: "uint8array",
        open: resolve,
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
    const token = "72".repeat(32);
    const child = Bun.spawn(
      [
        process.execPath,
        "--no-install",
        "--define=__KELD_WINDOWS_HOST__:true",
        "--define=__KELD_WINDOWS_DESKTOP_SNAPSHOT__:true",
        join(import.meta.dir, "ready-desktop-ring.test-role.ts"),
        mode,
      ],
      {
        env: {
          ...process.env,
          KELD_NATIVE_WINDOW_FACADE: "1",
          KELD_KIPC_TEST_HOOKS: "1",
          KELD_APP_LINK: `${endpoint}#${token}`,
        },
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const stdoutDone = new Response(child.stdout).text();
    const stderrDone = new Response(child.stderr).text();
    const failedEarly = child.exited.then(async (status) => {
      throw new Error(
        `WorkerLink fixture exited before Subscribe (${status}): ${await stderrDone}`,
      );
    });
    try {
      const writer = new WriteQueue(
        await withIoDeadline(Promise.race([opened, failedEarly])),
        drain,
      );
      const hello = await withIoDeadline(reader.readFrame());
      assert.equal(hello.header.kind, FrameKind.Hello);
      await writer.writeFrame(
        FrameKind.Hello,
        0,
        0,
        0,
        Uint8Array.from(Buffer.from(token, "hex")),
      );
      const call = await withIoDeadline(
        Promise.race([reader.readFrame(), failedEarly]),
      );
      assert.equal(call.header.kind, FrameKind.Call);
      assert.equal(call.header.channel, LIFECYCLE_CHANNEL);
      assert.deepEqual([...call.payload], [1, WINDOW_CHANNEL]);
      const ack = () =>
        writer.writeFrame(
          FrameKind.Reply,
          0,
          LIFECYCLE_CHANNEL,
          call.header.corr,
          Uint8Array.of(1),
        );
      const snapshot = () =>
        writer.writeFrame(
          FrameKind.Event,
          0,
          WINDOW_CHANNEL,
          0,
          Uint8Array.from(
            Buffer.from(
              "07ac02011101110000a00b84070030a00bec06000000000000004000",
              "hex",
            ),
          ),
        );
      await writer.writeFrame(
        FrameKind.Event,
        0,
        LIFECYCLE_CHANNEL,
        0,
        Uint8Array.of(0),
      );
      if (mode === "ack-first") {
        await ack();
        await snapshot();
      } else {
        await snapshot();
        await ack();
      }
      const [exit, stdout, stderr] = await withIoDeadline(
        Promise.all([child.exited, stdoutDone, stderrDone]),
        APP_LINK_IO_DEADLINE_MS,
      );
      assert.equal(exit, 0, `${stdout}\n${stderr}`);
    } finally {
      if (child.exitCode === null) child.kill();
      await child.exited;
      server.stop(true);
      rmSync(dir, { recursive: true, force: true });
    }
  }, 10_000);
}
