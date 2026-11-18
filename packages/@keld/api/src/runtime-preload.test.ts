import { expect, test } from "bun:test";
import {
  existsSync,
  mkdtempSync,
  rmSync,
  readFileSync,
  writeFileSync,
  mkdirSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  FrameKind,
  FrameReader,
  WriteQueue,
  DrainSignal,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
  COMPAT_CONTROL_CHANNEL,
  withIoDeadline,
} from "@keld/kipc";
import { decodeRuntimeProcessInit } from "@keld/kipc/runtime-process-facts";

// This allowance covers process startup in the packed integration test only.
// Frame assembly and production WorkerLink deadlines remain at 5 seconds.
const TEST_STARTUP_ALLOWANCE_MS = 15_000;
const TEST_CLEANUP_ALLOWANCE_MS = 5_000;
function withTestIoDeadline<T>(
  promise: Promise<T>,
  allowanceMs = TEST_STARTUP_ALLOWANCE_MS,
): Promise<T> {
  return withIoDeadline(promise, allowanceMs);
}

import { freshWin32AppLinkPipe } from "../../kipc/test/app-link-host.ts";

type Message = {
  kind: string;
  entered?: boolean;
  before?: Record<string, string>;
  after?: Record<string, string>;
};
async function launch(
  observer: boolean,
  mutant = false,
  installed?: {
    wrapper: string;
    entry: string;
    runAsMainGraph?: boolean;
    startupAllowanceMs?: number;
  },
) {
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
  let wrapper =
    installed?.wrapper ?? join(import.meta.dir, "runtime-preload.ts");
  if (mutant) {
    wrapper = join(import.meta.dir, "runtime-preload-no-await.fixture.ts");
    writeFileSync(
      wrapper,
      readFileSync(join(import.meta.dir, "runtime-preload.ts"), "utf8").replace(
        "await preloadRuntimeProcessFacts(process.versions)",
        "void preloadRuntimeProcessFacts(process.versions)",
      ),
    );
  }
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
    // Windows packaged startup uses one Bun main ESM graph for wrapper and app.
    // Keep the separate preload route for the other existing fixture cases.
    ...(installed?.runAsMainGraph ? [] : ["--preload", wrapper]),
    installed?.entry ??
      join(import.meta.dir, "runtime-preload-entry.fixture.mjs"),
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
  const childExit = proc.exited.then(async (code) => {
    const [stdout, stderr] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
    ]);
    return { code, stdout, stderr };
  });
  let socket: Bun.Socket<undefined> | undefined;
  let hostClosed = false;
  function closeHost() {
    if (hostClosed) return;
    hostClosed = true;
    try {
      socket?.end();
    } finally {
      server.stop(true);
    }
  }
  let disposePromise: Promise<void> | undefined;
  function disposeChild(): Promise<void> {
    if (disposePromise) return disposePromise;
    disposePromise = (async () => {
      try {
        closeHost();
      } finally {
        if (proc.exitCode === null) proc.kill();
      }
      try {
        await withIoDeadline(childExit, TEST_CLEANUP_ALLOWANCE_MS);
      } catch (error) {
        throw new Error(
          `Bun child did not exit during test cleanup; fixture retained at ${dir}`,
          { cause: error },
        );
      }
      rmSync(dir, { recursive: true, force: true });
      if (mutant) rmSync(wrapper, { force: true });
    })();
    return disposePromise;
  }
  async function awaitChild<T>(label: string, promise: Promise<T>): Promise<T> {
    try {
      return await withTestIoDeadline(
        Promise.race([
          promise,
          childExit.then(({ code, stdout, stderr }) => {
            throw new Error(
              `Bun child exited while waiting for ${label} (exit ${code}); fixture=${JSON.stringify(dir)}; stdout=${stdout}; stderr=${stderr}`,
            );
          }),
        ]),
        installed?.startupAllowanceMs,
      );
    } catch (error) {
      pending = undefined;
      if (error instanceof Error && error.message.includes("KELD-IPC-006")) {
        await disposeChild();
        const exited = await childExit;
        throw new Error(
          `Timed out waiting for ${label}; Bun child exit=${exited.code}; fixture=${JSON.stringify(dir)}; stdout=${exited.stdout}; stderr=${exited.stderr}`,
          { cause: error },
        );
      }
      throw error;
    }
  }
  async function message(label = "runtime event"): Promise<Message> {
    const next = Promise.resolve(
      messages.shift() ??
        new Promise<Message>((resolve) => {
          pending = resolve;
        }),
    );
    return awaitChild(label, next);
  }
  try {
    const connectedSocket = await awaitChild("app-link socket open", opened);
    socket = connectedSocket;
    const writer = new WriteQueue(connectedSocket, drain);
    const hello = await awaitChild("initial Hello frame", reader.readFrame());
    expect(hello.header.kind).toBe(FrameKind.Hello);
    await writer.writeFrame(
      FrameKind.Hello,
      0,
      0,
      0,
      new Uint8Array(32).fill(0x72),
    );
    const init = await awaitChild("runtime Init call", reader.readFrame());
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
      childExit,
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
        for (const channel of [WINDOW_CHANNEL, COMPAT_CONTROL_CHANNEL]) {
          const call = await awaitChild(
            `subscription request for channel ${channel}`,
            reader.readFrame(),
          );
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
      },
      async dispose() {
        await disposeChild();
      },
    };
  } catch (error) {
    await disposeChild();
    throw error;
  }
}

test("one real API preload holds entry through Init ACK then reuses one WorkerLink", async () => {
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
}, 20000);

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
}, 20000);

test("dropping wrapper await is detected by the same main-realm entry probe", async () => {
  const run = await launch(true, true);
  try {
    expect((await run.message()).kind).toBe("entry");
    run.proc.send({ kind: "probe" });
    expect(await run.message()).toEqual({ kind: "probe", entered: true });
    await run.ack();
    await run.subscribe();
    expect((await run.message()).kind).toBe("ready");
  } finally {
    await run.dispose();
  }
}, 20000);

test("wrong Init acknowledgement closes the link before application entry", async () => {
  const run = await launch(true);
  try {
    run.proc.send({ kind: "probe" });
    expect(await run.message()).toEqual({ kind: "probe", entered: false });
    await run.ack(1);
    const exited = await withTestIoDeadline(run.childExit);
    expect(exited.code).not.toBe(0);
    const stderr = exited.stderr;
    expect(stderr).toContain("Runtime facts require");
    expect(run.connections()).toBe(1);
  } finally {
    await run.dispose();
  }
}, 20000);

test("startup failures reap the Bun child and remove the listener fixture", async () => {
  async function expectFixtureRemoved(error: unknown) {
    expect(error).toBeInstanceOf(Error);
    const match = (error as Error).message.match(/fixture=("(?:\\.|[^"\\])*")/);
    expect(match).not.toBeNull();
    expect(existsSync(JSON.parse(match![1]!))).toBe(false);
  }

  let earlyExit: unknown;
  try {
    await launch(false, false, {
      wrapper: join(tmpdir(), `krp-missing-${process.pid}-${Date.now()}.mjs`),
      entry: join(import.meta.dir, "runtime-preload-entry.fixture.mjs"),
    });
  } catch (error) {
    earlyExit = error;
  }
  expect(earlyExit).toBeInstanceOf(Error);
  expect((earlyExit as Error).message).toContain(
    "Bun child exited while waiting for app-link socket open",
  );
  await expectFixtureRemoved(earlyExit);

  const stallRoot = mkdtempSync(join(tmpdir(), "krp-stall-"));
  const stallWrapper = join(stallRoot, "stall.mjs");
  writeFileSync(
    stallWrapper,
    "setInterval(() => {}, 60_000);\nawait new Promise(() => {});\n",
  );
  let startupTimeout: unknown;
  try {
    await launch(false, false, {
      wrapper: stallWrapper,
      entry: join(import.meta.dir, "runtime-preload-entry.fixture.mjs"),
      startupAllowanceMs: 250,
    });
  } catch (error) {
    startupTimeout = error;
  } finally {
    rmSync(stallRoot, { recursive: true, force: true });
  }
  expect(startupTimeout).toBeInstanceOf(Error);
  expect((startupTimeout as Error).message).toContain(
    "Timed out waiting for app-link socket open",
  );
  await expectFixtureRemoved(startupTimeout);
}, 10000);

test("actual packed API wrapper captures and ACKs over its same stamped installed transport", async () => {
  const stage = mkdtempSync(join(tmpdir(), "krp-sdk-"));
  try {
    const producer = join(
      import.meta.dir,
      "../../kipc/scripts/package-artifacts.ts",
    );
    const built = Bun.spawnSync([process.execPath, producer, stage], {
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(
      built.exitCode,
      built.stdout.toString() + built.stderr.toString(),
    ).toBe(0);
    for (const name of ["api", "kipc"]) {
      const target = join(stage, "node_modules/@keld", name);
      mkdirSync(target, { recursive: true });
      const version = JSON.parse(
        readFileSync(
          join(import.meta.dir, `../../${name}/package.json`),
          "utf8",
        ),
      ).version;
      const unpacked = Bun.spawnSync(
        [
          "tar",
          "-xzf",
          `keld-${name}-${version}.tgz`,
          "--strip-components=1",
          "-C",
          `node_modules/@keld/${name}`,
        ],
        { cwd: stage, stdout: "pipe", stderr: "pipe" },
      );
      expect(unpacked.exitCode, unpacked.stderr.toString()).toBe(0);
    }
    const entry = join(stage, "entry.mjs");
    writeFileSync(
      entry,
      readFileSync(
        join(import.meta.dir, "runtime-preload-entry.fixture.mjs"),
        "utf8",
      ).replace('"./app.ts"', '"@keld/api"'),
    );
    const wrapper = join(stage, "node_modules/@keld/api/runtime-preload.js");
    const text = readFileSync(wrapper, "utf8");
    expect(text).toContain('from "@keld/kipc"');
    expect(text).toContain('from "./app.js"');
    // Bun 1.4.4 on Windows can split singleton identity between --preload
    // and the main module. The actual Windows Host enters one main ESM graph.
    // Dynamic imports preserve the required wrapper-before-entry ordering.
    const runAsMainGraph = process.platform === "win32";
    const mainEntry = join(stage, "packed-main-graph.mjs");
    if (runAsMainGraph) {
      writeFileSync(
        mainEntry,
        'await import("./node_modules/@keld/api/runtime-preload.js");\nawait import("./entry.mjs");\n',
      );
    }
    const run = await launch(false, false, {
      wrapper,
      entry: runAsMainGraph ? mainEntry : entry,
      runAsMainGraph,
    });
    try {
      await run.ack();
      await run.subscribe();
      const entered = await run.message("packed API entry after Init ACK");
      expect(entered.kind).toBe("entry");
      expect(run.captured.versions).toEqual(entered.before!);
      expect(run.captured.versions.__keld_late).toBeUndefined();
      expect((await run.message("packed API Ready event")).kind).toBe("ready");
      expect(run.connections()).toBe(1);
    } finally {
      await run.dispose();
    }
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}, 30000);
