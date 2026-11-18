import { expect, test } from "bun:test";
import {
  mkdtempSync,
  rmSync,
  readFileSync,
  writeFileSync,
  mkdirSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
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
  installed?: { wrapper: string; entry: string; startupProbe?: boolean },
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
    "--preload",
    wrapper,
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
  async function message(): Promise<Message> {
    return (
      messages.shift() ??
      withIoDeadline(
        new Promise<Message>((resolve) => {
          pending = resolve;
        }),
      )
    );
  }
  // Child startup failures must not masquerade as a silent peer timeout.
  // This race keeps the existing per-operation I/O deadline unchanged.
  const exited = proc.exited.then((code): never => {
    throw new Error(`preload child exited with code ${code}`);
  });
  void exited.catch(() => undefined);
  async function startup<T>(stage: string, operation: Promise<T>): Promise<T> {
    try {
      if (installed?.startupProbe) proc.send({ kind: "startup-stage", stage });
      return await withIoDeadline(Promise.race([operation, exited]));
    } catch (error) {
      throw new Error(`preload startup ${stage}: ${String(error)}`);
    }
  }
  let socket: Bun.Socket<undefined>;
  let writer: WriteQueue;
  let captured: ReturnType<typeof decodeRuntimeProcessInit>;
  let saved: Uint8Array;
  try {
    socket = await startup("connect", opened);
    writer = new WriteQueue(socket, drain);
    const hello = await startup("HELLO", reader.readFrame());
    expect(hello.header.kind).toBe(FrameKind.Hello);
    await writer.writeFrame(
      FrameKind.Hello,
      0,
      0,
      0,
      new Uint8Array(32).fill(0x72),
    );
    const init = await startup("Init", reader.readFrame());
    expect(init.header).toMatchObject({
      kind: FrameKind.Call,
      channel: LIFECYCLE_CHANNEL,
      corr: 1,
      flags: 0,
    });
    captured = decodeRuntimeProcessInit(init.payload);
    saved = init.payload.slice();
  } catch (error) {
    let stderr = "";
    let cleanupFailure = "";
    try {
      proc.kill();
      await withIoDeadline(proc.exited);
      stderr = await withIoDeadline(new Response(proc.stderr).text());
    } catch (cleanupError) {
      cleanupFailure = `\npreload cleanup: ${String(cleanupError)}`;
    } finally {
      for (const clean of [
        () => server.stop(true),
        () => rmSync(dir, { recursive: true, force: true }),
        () => {
          if (mutant) rmSync(wrapper, { force: true });
        },
      ]) {
        try {
          clean();
        } catch (cleanupError) {
          cleanupFailure += `\npreload cleanup: ${String(cleanupError)}`;
        }
      }
    }
    throw new Error(
      `${String(error)}\npreload child stderr:\n${stderr}${cleanupFailure}`,
    );
  }
  return {
    proc,
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
    },
    async dispose() {
      proc.kill();
      await proc.exited;
      socket.end();
      server.stop(true);
      rmSync(dir, { recursive: true, force: true });
      if (mutant) rmSync(wrapper, { force: true });
    },
  };
}

test("preload startup reports an early child exit instead of a peer timeout", async () => {
  const stage = mkdtempSync(join(tmpdir(), "krp-missing-"));
  try {
    let failure: unknown;
    try {
      const run = await launch(false, false, {
        wrapper: join(stage, "missing-preload.mjs"),
        entry: join(import.meta.dir, "runtime-preload-entry.fixture.mjs"),
      });
      await run.dispose();
    } catch (error) {
      failure = error;
    }
    expect(String(failure)).toContain("preload startup connect:");
    expect(String(failure)).toContain("preload child exited with code");
    expect(String(failure)).toContain("missing-preload.mjs");
    expect(String(failure)).not.toContain("I/O deadline exceeded");
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}, 15000);

for (const phase of ["HELLO", "Init"] as const) {
  test(`preload startup reports child failure during ${phase}`, async () => {
    const stage = mkdtempSync(join(tmpdir(), "krp-exit-"));
    try {
      const wrapper = join(stage, "empty-preload.mjs");
      const entry = join(stage, "exit-entry.mjs");
      writeFileSync(
        wrapper,
        "// No API preload: this child is the failing protocol peer.\n",
      );
      const transport = pathToFileURL(
        Bun.resolveSync("@keld/kipc", import.meta.dir),
      ).href;
      writeFileSync(
        entry,
        `
import { encodeHeader, FrameKind } from ${JSON.stringify(transport)};
const phase = ${JSON.stringify(phase)};
const stop = () => { console.error("intentional peer failure " + phase); process.exit(23); };
process.on("message", (message) => {
  if (phase === "HELLO" && message?.kind === "startup-stage" && message.stage === "HELLO") stop();
});
await Bun.connect({
  unix: process.env.KELD_APP_LINK.split("#")[0],
  socket: {
    open(socket) {
      if (phase === "HELLO") return;
      const header = encodeHeader(FrameKind.Hello, 0, 0, 0, 32);
      const hello = new Uint8Array(header.length + 32);
      hello.set(header); hello.fill(0x72, header.length);
      socket.write(hello);
    },
    data() { stop(); },
    error() { process.exit(24); },
  },
});
`,
      );
      let failure: unknown;
      try {
        const run = await launch(false, false, {
          wrapper,
          entry,
          startupProbe: true,
        });
        await run.dispose();
      } catch (error) {
        failure = error;
      }
      expect(String(failure)).toContain(`preload startup ${phase}:`);
      expect(String(failure)).toContain(`intentional peer failure ${phase}`);
      expect(String(failure)).not.toContain("I/O deadline exceeded");
    } finally {
      rmSync(stage, { recursive: true, force: true });
    }
  }, 15000);
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
}, 15000);

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
}, 15000);

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
}, 15000);

test("wrong Init acknowledgement closes the link before application entry", async () => {
  const run = await launch(true);
  try {
    run.proc.send({ kind: "probe" });
    expect(await run.message()).toEqual({ kind: "probe", entered: false });
    await run.ack(1);
    expect(await withIoDeadline(run.proc.exited)).not.toBe(0);
    const stderr = await new Response(run.proc.stderr).text();
    expect(stderr).toContain("Runtime facts require");
    expect(run.connections()).toBe(1);
  } finally {
    await run.dispose();
  }
}, 15000);

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
    const run = await launch(false, false, { wrapper, entry });
    try {
      await run.ack();
      await run.subscribe();
      const entered = await run.message();
      expect(entered.kind).toBe("entry");
      expect(run.captured.versions).toEqual(entered.before!);
      expect(run.captured.versions.__keld_late).toBeUndefined();
      expect((await run.message()).kind).toBe("ready");
      expect(run.connections()).toBe(1);
    } finally {
      await run.dispose();
    }
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}, 15000);
