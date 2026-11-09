/**
 * Cross-platform installed-artifact boundary: a genuinely packed API/KIPC
 * distribution must resolve its bare exports and enter the real stamped
 * transport before reporting a missing Host link. This independent regression
 * provides subprocess stderr when Windows packaging fails before HELLO; it
 * does not replace the existing positive same-link Init/ACK oracle.
 */
import { expect, test } from "bun:test";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { freshWin32AppLinkPipe } from "../../kipc/test/app-link-host.ts";
import {
  DrainSignal,
  FrameKind,
  FrameReader,
  WriteQueue,
  withIoDeadline,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
  COMPAT_CONTROL_CHANNEL,
} from "@keld/kipc";
import { decodeRuntimeProcessInit } from "@keld/kipc/runtime-process-facts";

type Message = {
  kind: string;
  before?: Record<string, string>;
};

async function provePackedHandshake(
  stage: string,
  wrapper: string,
  entry: string,
): Promise<void> {
  const socketDir = mkdtempSync(join(tmpdir(), "krp-packed-live-"));
  const endpoint =
    process.platform === "win32"
      ? freshWin32AppLinkPipe()
      : join(socketDir, "live.sock");
  const reader = new FrameReader();
  const drain = new DrainSignal();
  let connections = 0;
  let accept!: (socket: Bun.Socket<undefined>) => void;
  const accepted = new Promise<Bun.Socket<undefined>>((resolve) => {
    accept = resolve;
  });
  const server = Bun.listen<undefined>({
    unix: endpoint,
    socket: {
      binaryType: "uint8array",
      open(socket) {
        connections++;
        accept(socket);
      },
      data(_socket, bytes) {
        reader.push(bytes);
      },
      drain() {
        drain.fire();
      },
      close() {
        reader.end(new Error("packed test peer closed"));
      },
      error() {},
    },
  });
  const messages: Message[] = [];
  let pending: ((message: Message) => void) | undefined;
  const child = Bun.spawn(
    [
      process.execPath,
      "--no-env-file",
      "--no-install",
      "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
      "--preload",
      wrapper,
      entry,
    ],
    {
      cwd: stage,
      env: {
        ...process.env,
        KELD_APP_LINK: endpoint + "#" + "72".repeat(32),
        KELD_NATIVE_WINDOW_FACADE: "1",
      },
      stdout: "pipe",
      stderr: "pipe",
      ipc(message) {
        const value = message as Message;
        if (pending) {
          const resolve = pending;
          pending = undefined;
          resolve(value);
        } else {
          messages.push(value);
        }
      },
    },
  );
  const next = () =>
    messages.shift() ??
    new Promise<Message>((resolve) => {
      pending = resolve;
    });
  let socket: Bun.Socket<undefined> | undefined;
  let step = "accept";
  try {
    socket = await Promise.race([
      withIoDeadline(accepted),
      child.exited.then((code) => {
        throw new Error(`child exited before Host accepted it: ${code}`);
      }),
    ]);
    const writer = new WriteQueue(socket, drain);
    step = "hello";
    const hello = await withIoDeadline(reader.readFrame());
    expect(hello.header.kind).toBe(FrameKind.Hello);
    await writer.writeFrame(
      FrameKind.Hello,
      0,
      0,
      0,
      new Uint8Array(32).fill(0x72),
    );
    step = "init";
    const init = await withIoDeadline(reader.readFrame());
    expect(init.header).toMatchObject({
      kind: FrameKind.Call,
      channel: LIFECYCLE_CHANNEL,
      corr: 1,
      flags: 0,
    });
    const captured = decodeRuntimeProcessInit(init.payload);
    await writer.writeFrame(
      FrameKind.Reply,
      0,
      LIFECYCLE_CHANNEL,
      1,
      Uint8Array.of(3),
    );
    for (const channel of [WINDOW_CHANNEL, COMPAT_CONTROL_CHANNEL]) {
      step = `subscribe-${channel}`;
      const call = await withIoDeadline(reader.readFrame());
      expect(call.header.kind).toBe(FrameKind.Call);
      expect(call.header.channel).toBe(LIFECYCLE_CHANNEL);
      expect([...call.payload]).toEqual([1, channel]);
      await writer.writeFrame(
        FrameKind.Reply,
        0,
        LIFECYCLE_CHANNEL,
        call.header.corr,
        Uint8Array.of(1),
      );
    }
    await writer.writeFrame(
      FrameKind.Event,
      0,
      LIFECYCLE_CHANNEL,
      0,
      Uint8Array.of(0),
    );
    step = "entry";
    const entered = await withIoDeadline(Promise.resolve(next()));
    expect(entered.kind).toBe("entry");
    expect(entered.before).toEqual(captured.versions);
    step = "ready";
    expect((await withIoDeadline(Promise.resolve(next()))).kind).toBe("ready");
    expect(connections).toBe(1);
    step = "clean exit";
    child.send({ kind: "finish" });
    expect(await withIoDeadline(child.exited)).toBe(0);
  } catch (error) {
    child.kill();
    const exitCode = await child.exited;
    const stderr = await new Response(child.stderr).text();
    const stdout = await new Response(child.stdout).text();
    throw new Error(
      `packed app-link stage=${step}, child_exit=${exitCode}, cause=${String(error)}; child_stdout=${stdout.slice(-1000)}; child_stderr=${stderr.slice(-2500)}`,
      { cause: error },
    );
  } finally {
    child.kill();
    await child.exited;
    socket?.end();
    server.stop(true);
    rmSync(socketDir, { recursive: true, force: true });
  }
}

test("packed SDK resolves, fails closed unbound and authenticates over its stamped link", async () => {
  const stage = mkdtempSync(join(tmpdir(), "krp-packed-startup-"));
  try {
    const producer = join(
      import.meta.dir,
      "../../kipc/scripts/package-artifacts.ts",
    );
    const packed = Bun.spawnSync([process.execPath, producer, stage], {
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(
      packed.exitCode,
      packed.stdout.toString() + packed.stderr.toString(),
    ).toBe(0);

    for (const name of ["api", "kipc"]) {
      const manifest = JSON.parse(
        readFileSync(
          join(import.meta.dir, `../../${name}/package.json`),
          "utf8",
        ),
      );
      mkdirSync(join(stage, "node_modules", "@keld", name), {
        recursive: true,
      });
      const extracted = Bun.spawnSync(
        [
          "tar",
          "-xzf",
          `keld-${name}-${manifest.version}.tgz`,
          "--strip-components=1",
          "-C",
          `node_modules/@keld/${name}`,
        ],
        { cwd: stage, stdout: "pipe", stderr: "pipe" },
      );
      expect(extracted.exitCode, extracted.stderr.toString()).toBe(0);
    }

    const entry = join(stage, "entry.mjs");
    writeFileSync(
      entry,
      'import {app} from "@keld/api"; console.log("API_IMPORT", typeof app.whenReady);\n',
    );
    const importResult = Bun.spawnSync(
      [process.execPath, "--no-env-file", "--no-install", entry],
      { cwd: stage, stdout: "pipe", stderr: "pipe", timeout: 9_000 },
    );
    expect(importResult.exitCode, importResult.stderr.toString()).toBe(0);
    expect(importResult.stdout.toString()).toBe("API_IMPORT function\n");
    expect(importResult.stderr.toString()).toBe("");

    const wrapper = join(stage, "node_modules/@keld/api/runtime-preload.js");
    const missingEndpoint =
      process.platform === "win32"
        ? freshWin32AppLinkPipe()
        : join(stage, "unbound.sock");
    const preloadResult = Bun.spawnSync(
      [
        process.execPath,
        "--no-env-file",
        "--no-install",
        "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
        "--preload",
        wrapper,
        entry,
      ],
      {
        cwd: stage,
        env: {
          ...process.env,
          KELD_APP_LINK: missingEndpoint + "#" + "72".repeat(32),
          KELD_NATIVE_WINDOW_FACADE: "1",
        },
        stdout: "pipe",
        stderr: "pipe",
        timeout: 9_000,
      },
    );
    expect(
      preloadResult.exitCode,
      preloadResult.stdout.toString() + preloadResult.stderr.toString(),
    ).toBe(1);
    expect(preloadResult.stdout.toString()).toBe("");
    expect(preloadResult.stderr.toString()).toContain("KELD-IPC-001");

    // Keep the authenticated positive path as strict as the existing
    // owner-controlled ACK test, but include stage-specific child stderr.
    // This proves whether Windows fails before connection, at Init, or later.
    writeFileSync(
      entry,
      readFileSync(
        join(import.meta.dir, "runtime-preload-entry.fixture.mjs"),
        "utf8",
      ).replace('"./app.ts"', '"@keld/api"'),
    );
    await provePackedHandshake(stage, wrapper, entry);
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}, 30_000);
