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
  appReady?: boolean;
  observedHostReady?: boolean;
  payload?: number[];
};

/** An independent raw WorkerLink EVENT oracle, without API Ready mediation. */
async function provePackedRawEvent(
  stage: string,
  withPriorCalls = false,
  fullReceiveTable = false,
): Promise<void> {
  const local = mkdtempSync(join(tmpdir(), "krp-packed-raw-"));
  const endpoint =
    process.platform === "win32"
      ? freshWin32AppLinkPipe()
      : join(local, "raw.sock");
  const reader = new FrameReader();
  const drain = new DrainSignal();
  let accept!: (socket: Bun.Socket<undefined>) => void;
  const opened = new Promise<Bun.Socket<undefined>>((resolve) => {
    accept = resolve;
  });
  const server = Bun.listen<undefined>({
    unix: endpoint,
    socket: {
      binaryType: "uint8array",
      open(socket) {
        accept(socket);
      },
      data(_socket, bytes) {
        reader.push(bytes);
      },
      drain() {
        drain.fire();
      },
      close() {
        reader.end(new Error("raw peer closed"));
      },
      error() {},
    },
  });
  const rawEntry = join(stage, "raw-event.mjs");
  writeFileSync(
    rawEntry,
    `import {
  WorkerLink,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
  COMPAT_CONTROL_CHANNEL,
} from "@keld/kipc";
const fullReceive = process.env.KELD_RAW_FULL_RECEIVE === "1";
const link = await WorkerLink.open({
  link: process.env.KELD_APP_LINK,
  receive: {
    eventChannels: fullReceive
      ? [LIFECYCLE_CHANNEL, WINDOW_CHANNEL, COMPAT_CONTROL_CHANNEL]
      : [LIFECYCLE_CHANNEL],
    callReceivers: fullReceive ? [{ policy: "echoReceiver" }] : [],
  },
});
link.onEvent(LIFECYCLE_CHANNEL, (bytes) => {
  process.send?.({ kind: "raw-event", payload: [...bytes] });
});
if (process.env.KELD_RAW_PRIOR_CALLS === "1") {
  for (const value of [41, 42]) {
    const reply = await link.call(LIFECYCLE_CHANNEL, Uint8Array.of(value), 5000);
    if (reply.length !== 1 || reply[0] !== value) {
      throw new Error("raw packed call reply was not identical");
    }
  }
}
process.on("message", message => {
  if (message?.kind === "finish") {
    link.close();
    process.exit(0);
  }
});
process.send?.({ kind: "listening" });
`,
  );
  let listening!: (message: Message) => void;
  let evented!: (message: Message) => void;
  const listeningReady = new Promise<Message>((resolve) => {
    listening = resolve;
  });
  const eventReady = new Promise<Message>((resolve) => {
    evented = resolve;
  });
  const child = Bun.spawn(
    [process.execPath, "--no-env-file", "--no-install", rawEntry],
    {
      cwd: stage,
      env: {
        ...process.env,
        KELD_APP_LINK: endpoint + "#" + "72".repeat(32),
        KELD_RAW_PRIOR_CALLS: withPriorCalls ? "1" : "0",
        KELD_RAW_FULL_RECEIVE: fullReceiveTable ? "1" : "0",
      },
      stdout: "pipe",
      stderr: "pipe",
      ipc(message) {
        const value = message as Message;
        if (value.kind === "listening") listening(value);
        if (value.kind === "raw-event") evented(value);
      },
    },
  );
  let socket: Bun.Socket<undefined> | undefined;
  let phase = "hello";
  try {
    socket = await withIoDeadline(opened);
    const writer = new WriteQueue(socket, drain);
    const hello = await withIoDeadline(reader.readFrame());
    expect(hello.header.kind).toBe(FrameKind.Hello);
    await writer.writeFrame(
      FrameKind.Hello,
      0,
      0,
      0,
      new Uint8Array(32).fill(0x72),
    );
    if (withPriorCalls) {
      for (const expected of [41, 42]) {
        phase = `call-${expected}`;
        const call = await withIoDeadline(reader.readFrame());
        expect(call.header.kind).toBe(FrameKind.Call);
        expect(call.header.channel).toBe(LIFECYCLE_CHANNEL);
        expect([...call.payload]).toEqual([expected]);
        await writer.writeFrame(
          FrameKind.Reply,
          0,
          LIFECYCLE_CHANNEL,
          call.header.corr,
          Uint8Array.of(expected),
        );
      }
    }
    phase = "listen";
    expect((await withIoDeadline(listeningReady)).kind).toBe("listening");
    phase = "raw-event";
    await writer.writeFrame(
      FrameKind.Event,
      0,
      LIFECYCLE_CHANNEL,
      0,
      Uint8Array.of(0),
    );
    const received = await withIoDeadline(eventReady);
    expect(received.kind).toBe("raw-event");
    expect(received.payload).toEqual([0]);
    phase = "exit";
    child.send({ kind: "finish" });
    expect(await withIoDeadline(child.exited)).toBe(0);
  } catch (error) {
    child.kill();
    await child.exited;
    const stderr = await new Response(child.stderr).text();
    throw new Error(
      `packed raw WorkerLink phase=${phase}, error=${String(error)}, child_stderr=${stderr.slice(-1800)}`,
      { cause: error },
    );
  } finally {
    child.kill();
    await child.exited;
    socket?.end();
    server.stop(true);
    rmSync(local, { recursive: true, force: true });
  }
}

async function provePackedHandshake(
  stage: string,
  wrapper: string,
  entry: string,
  inheritCwd = false,
  preloadByAlias = false,
  preloadRelative = false,
  singleGraphBootstrap = false,
  rejectInitAck = false,
  verifiedOriginalEntry?: string,
): Promise<void> {
  const socketDir = mkdtempSync(join(tmpdir(), "krp-packed-live-"));
  const endpoint =
    process.platform === "win32"
      ? freshWin32AppLinkPipe()
      : join(socketDir, "live.sock");
  const reader = new FrameReader();
  const secondaryReader = new FrameReader();
  const drain = new DrainSignal();
  const secondaryDrain = new DrainSignal();
  const timeline: string[] = [];
  let primarySocket: Bun.Socket<undefined> | undefined;
  let secondarySocket: Bun.Socket<undefined> | undefined;
  let secondaryClassification: Promise<string> | undefined;
  const classifySecondary = async (socket: Bun.Socket<undefined>) => {
    try {
      // Observe the extra link while its own HELLO deadline is still live.
      // Never advance its application lifecycle or send a synthetic Ready.
      const hello = await withIoDeadline(secondaryReader.readFrame());
      expect(hello.header).toMatchObject({
        kind: FrameKind.Hello,
        channel: 0,
        corr: 0,
        flags: 0,
      });
      expect(hello.payload).toEqual(new Uint8Array(32).fill(0x72));
      const writer = new WriteQueue(socket, secondaryDrain);
      await writer.writeFrame(
        FrameKind.Hello,
        0,
        0,
        0,
        new Uint8Array(32).fill(0x72),
      );
      const first = await withIoDeadline(secondaryReader.readFrame());
      expect(first.header).toMatchObject({
        kind: FrameKind.Call,
        channel: LIFECYCLE_CHANNEL,
        corr: 1,
        flags: 0,
      });
      if (
        first.payload.length === 2 &&
        first.payload[0] === 1 &&
        first.payload[1] === WINDOW_CHANNEL
      ) {
        return "second-link-subscribe-without-runtime-init";
      }
      try {
        decodeRuntimeProcessInit(first.payload);
        return "second-link-runtime-process-init";
      } catch {
        return `unrecognized-lifecycle-call-len=${first.payload.length}`;
      }
    } catch (error) {
      return `secondary-probe-error=${String(error)}`;
    }
  };
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
        timeline.push(`socket-open-${connections}`);
        if (connections === 1) {
          primarySocket = socket;
          accept(socket);
        } else {
          secondarySocket = socket;
          if (connections === 2)
            secondaryClassification = classifySecondary(socket);
        }
      },
      data(socket, bytes) {
        if (socket === primarySocket) reader.push(bytes);
        else secondaryReader.push(bytes);
      },
      drain() {
        drain.fire();
        secondaryDrain.fire();
      },
      close() {
        reader.end(new Error("packed test peer closed"));
      },
      error() {},
    },
  });
  const messages: Message[] = [];
  let pending: ((message: Message) => void) | undefined;
  let pendingProbe: ((message: Message) => void) | undefined;
  const child = Bun.spawn(
    [
      process.execPath,
      "--no-env-file",
      "--no-install",
      "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
      ...(singleGraphBootstrap
        ? []
        : [
            "--preload",
            preloadRelative
              ? "./node_modules/@keld/api/runtime-preload.js"
              : preloadByAlias
                ? "@keld/api/runtime-preload"
                : wrapper,
          ]),
      ...(verifiedOriginalEntry === undefined ? [] : ["run"]),
      entry,
      ...(verifiedOriginalEntry === undefined ? [] : [verifiedOriginalEntry]),
    ],
    {
      ...(inheritCwd ? {} : { cwd: stage }),
      env: {
        ...process.env,
        KELD_APP_LINK: endpoint + "#" + "72".repeat(32),
        KELD_NATIVE_WINDOW_FACADE: "1",
      },
      stdout: "pipe",
      stderr: "pipe",
      ipc(message) {
        const value = message as Message;
        timeline.push(`child-${value.kind}`);
        if (value.kind === "probe-ready") {
          const resolve = pendingProbe;
          pendingProbe = undefined;
          resolve?.(value);
          return;
        }
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
  async function probeReadyState(): Promise<Message> {
    if (pendingProbe !== undefined)
      throw new Error("only one diagnostic probe may be in flight");
    const outcome = new Promise<Message>((resolve) => {
      pendingProbe = resolve;
    });
    child.send({ kind: "probe-ready" });
    try {
      return await withIoDeadline(outcome, 1_500);
    } finally {
      pendingProbe = undefined;
    }
  }
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
      Uint8Array.of(rejectInitAck ? 1 : 3),
    );
    if (rejectInitAck) {
      // A host refusal must retire the one link before any user entry. No
      // subscriptions, Ready, second socket or successfully-loaded user code.
      step = "rejected-init";
      expect(await withIoDeadline(child.exited)).not.toBe(0);
      expect(await new Response(child.stderr).text()).toContain(
        "Runtime facts require",
      );
      expect(messages.some((value) => value.kind === "entry")).toBe(false);
      expect(connections).toBe(1);
      return;
    }
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
    // Windows named-pipe writes can remain queued after an otherwise
    // successful socket.write. Test whether a real flush exposes the Ready
    // record without a second frame, timer delay or forged readiness.
    if (process.platform === "win32") socket.flush();
    step = "entry";
    const entered = await withIoDeadline(Promise.resolve(next()));
    expect(entered.kind).toBe("entry");
    expect(entered.before).toEqual(captured.versions);
    // Test-only IPC introspection must itself respond before a failure is
    // trusted; it never mutates Host state or supplies a synthetic Ready.
    const observation = await probeReadyState();
    expect(typeof observation.appReady).toBe("boolean");
    expect(typeof observation.observedHostReady).toBe("boolean");
    step = "ready";
    expect((await withIoDeadline(Promise.resolve(next()))).kind).toBe("ready");
    expect(connections).toBe(1);
    step = "clean exit";
    child.send({ kind: "finish" });
    expect(await withIoDeadline(child.exited)).toBe(0);
  } catch (error) {
    let observed = "unavailable";
    if (step === "ready" && child.exitCode === null) {
      try {
        const state = await probeReadyState();
        observed = `appReady=${String(state.appReady)}, observedHostReady=${String(state.observedHostReady)}`;
      } catch (probeError) {
        observed = `probe-error=${String(probeError)}`;
      }
    }
    let second = "none";
    if (secondaryClassification) second = await secondaryClassification;
    child.kill();
    const exitCode = await child.exited;
    const stderr = await new Response(child.stderr).text();
    const stdout = await new Response(child.stdout).text();
    throw new Error(
      `packed app-link mode=${verifiedOriginalEntry === undefined ? (singleGraphBootstrap ? "bootstrap" : "preload") : "root-signed-bootstrap"}, preload=${preloadRelative ? "relative" : preloadByAlias ? "export" : "absolute"}, cwd=${inheritCwd ? "inherited" : "stage"}, stage=${step}, host_connections=${connections}, order=${timeline.join(">")}, secondary_first=${second}, child_exit=${exitCode}, observed=${observed}, cause=${String(error)}; child_stdout=${stdout.slice(-1000)}; child_stderr=${stderr.slice(-2500)}`,
      { cause: error },
    );
  } finally {
    child.kill();
    await child.exited;
    socket?.end();
    secondarySocket?.end();
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

    // Isolate Bun's process-phase module cache without any Host socket or
    // patched release module. A preloaded app object and entry-owned app
    // object must be identical, regardless of ESM/CommonJS loading form.
    // Collect all four outcomes before asserting, so one Windows failure
    // cannot hide whether a different loader form shares the same owner.
    const esmPreload = join(stage, "cache-preload.mjs");
    const cjsPreload = join(stage, "cache-preload.cjs");
    const esmEntry = join(stage, "cache-entry.mjs");
    const cjsEntry = join(stage, "cache-entry.cjs");
    writeFileSync(
      esmPreload,
      'import { app } from "@keld/api/internal/app"; globalThis.__keld_cache_probe_app = app;\n',
    );
    writeFileSync(
      cjsPreload,
      'globalThis.__keld_cache_probe_app = require("@keld/api/internal/app").app;\n',
    );
    writeFileSync(
      esmEntry,
      'import { app } from "@keld/api"; console.log(JSON.stringify({ same: app === globalThis.__keld_cache_probe_app }));\n',
    );
    writeFileSync(
      cjsEntry,
      'const { app } = require("@keld/api"); console.log(JSON.stringify({ same: app === globalThis.__keld_cache_probe_app }));\n',
    );
    const cacheIdentity: Record<string, unknown> = {};
    for (const [label, preload, script] of [
      ["esm-esm", esmPreload, esmEntry],
      ["commonjs-esm", cjsPreload, esmEntry],
      ["esm-commonjs", esmPreload, cjsEntry],
      ["commonjs-commonjs", cjsPreload, cjsEntry],
    ] as const) {
      const checked = Bun.spawnSync(
        [
          process.execPath,
          "--no-env-file",
          "--no-install",
          "--preload",
          preload,
          script,
        ],
        { cwd: stage, stdout: "pipe", stderr: "pipe", timeout: 9_000 },
      );
      if (checked.exitCode !== 0) {
        cacheIdentity[label] = {
          exit: checked.exitCode,
          stderr: checked.stderr.toString().slice(-400),
        };
      } else {
        try {
          cacheIdentity[label] = JSON.parse(
            checked.stdout.toString().trim(),
          ).same;
        } catch {
          cacheIdentity[label] = "invalid-cached-identity-output";
        }
      }
    }
    // Only test-owned booleans and typed module-load failures are logged.
    console.info(
      "PACKED_CROSS_PHASE_APP_IDENTITY",
      JSON.stringify(cacheIdentity),
    );
    expect(cacheIdentity).toEqual({
      "esm-esm": true,
      "commonjs-esm": true,
      "esm-commonjs": true,
      "commonjs-commonjs": true,
    });

    // Bun must deduplicate the preload's relative app.js import and the
    // installed package's public + internal export paths on every OS.
    // Otherwise Ready is published on one app singleton but awaited on another.
    const identity = join(stage, "module-identity.mjs");
    writeFileSync(
      identity,
      `import { app as publicApp } from "@keld/api";
import { app as internalApp } from "@keld/api/internal/app";
import { app as relativeApp } from "./node_modules/@keld/api/app.js";
console.log(JSON.stringify([
  publicApp === internalApp,
  publicApp === relativeApp,
  internalApp === relativeApp,
]));`,
    );
    const checkedIdentity = Bun.spawnSync(
      [process.execPath, "--no-env-file", "--no-install", identity],
      { cwd: stage, stdout: "pipe", stderr: "pipe", timeout: 9_000 },
    );
    expect(checkedIdentity.exitCode, checkedIdentity.stderr.toString()).toBe(0);
    expect(checkedIdentity.stdout.toString()).toBe("[true,true,true]\n");
    expect(checkedIdentity.stderr.toString()).toBe("");

    await provePackedRawEvent(stage);
    await provePackedRawEvent(stage, true);
    await provePackedRawEvent(stage, true, true);
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
      )
        .replace('"./app.ts"', '"@keld/api"')
        .replace(
          "await app.whenReady();",
          `import { hasObservedHostReady } from "@keld/api/internal/app";
process.on("message", message => {
  if (message?.kind === "probe-ready") {
    process.send?.({
      kind: "probe-ready",
      appReady: app.isReady(),
      observedHostReady: hasObservedHostReady(),
    });
  }
});
await app.whenReady();`,
        ),
    );
    // The exact same checked-in release wrapper is available through the
    // installed exports map. Compare package-canonical versus absolute
    // preload naming without changing its bytes or emitting synthetic Ready.
    // The Host fixes the staged working directory. A root-relative preload
    // must resolve the same installed bytes as the package entry without
    // Windows drive-letter or URL-escaping cache-key differences.
    await provePackedHandshake(stage, wrapper, entry, false, false, true);
    await provePackedHandshake(stage, wrapper, entry, false, true);
    await provePackedHandshake(stage, wrapper, entry);
    // Match the original packed runtime test's inherited working directory.
    // It is a separate end-to-end acceptance case, not an assertion bypass.
    await provePackedHandshake(stage, wrapper, entry, true);
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}, 30_000);

test("single-graph entry boots the installed API before user code on one Host link", async () => {
  const stage = mkdtempSync(join(tmpdir(), "krp-single-graph-"));
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

    const entry = join(stage, "real-user-entry.mjs");
    writeFileSync(
      entry,
      readFileSync(
        join(import.meta.dir, "runtime-preload-entry.fixture.mjs"),
        "utf8",
      )
        .replace('"./app.ts"', '"@keld/api"')
        .replace(
          "await app.whenReady();",
          `import { hasObservedHostReady } from "@keld/api/internal/app";
process.on("message", message => {
  if (message?.kind === "probe-ready") {
    process.send?.({
      kind: "probe-ready",
      appReady: app.isReady(),
      observedHostReady: hasObservedHostReady(),
    });
  }
});
await app.whenReady();`,
        ),
    );
    const bootstrap = join(stage, "host-selected-bootstrap.mjs");
    writeFileSync(
      bootstrap,
      `// A candidate Host-owned entry: no app selected path or privileged global.
import { bootHostSelectedEntry } from "@keld/api/internal/app";
await bootHostSelectedEntry(() => import("./real-user-entry.mjs"));
`,
    );
    const wrapper = join(stage, "node_modules/@keld/api/runtime-preload.js");

    // An app-selected invocation without the Host's immutable launch define
    // must be refused before executing even the first user dependency.
    const unselected = Bun.spawnSync(
      [process.execPath, "--no-env-file", "--no-install", bootstrap],
      {
        cwd: stage,
        env: {
          ...process.env,
          KELD_APP_LINK: "not-a-qualified-host-link",
          KELD_NATIVE_WINDOW_FACADE: "1",
        },
        stdout: "pipe",
        stderr: "pipe",
        timeout: 9_000,
      },
    );
    expect(
      unselected.exitCode,
      unselected.stdout.toString() + unselected.stderr.toString(),
    ).toBe(1);
    expect(unselected.stdout.toString()).toBe("");
    expect(unselected.stderr.toString()).toContain("KELD-IPC-005");

    // Invalid loader arguments must be rejected locally *before* the trusted
    // bootstrap can contact its endpoint or evaluate a user module.
    const invalidBootstrap = join(stage, "invalid-bootstrap.mjs");
    writeFileSync(
      invalidBootstrap,
      'import { bootHostSelectedEntry } from "@keld/api/internal/app";\nawait bootHostSelectedEntry(null);\n',
    );
    const invalidLoader = Bun.spawnSync(
      [
        process.execPath,
        "--no-env-file",
        "--no-install",
        "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
        invalidBootstrap,
      ],
      {
        cwd: stage,
        env: {
          ...process.env,
          KELD_APP_LINK: "not-a-qualified-host-link",
          KELD_NATIVE_WINDOW_FACADE: "1",
        },
        stdout: "pipe",
        stderr: "pipe",
        timeout: 9_000,
      },
    );
    expect(
      invalidLoader.exitCode,
      invalidLoader.stdout.toString() + invalidLoader.stderr.toString(),
    ).toBe(1);
    expect(invalidLoader.stdout.toString()).toBe("");
    expect(invalidLoader.stderr.toString()).toContain("KELD-IPC-005");

    // This fail-closed bootstrap must NOT evaluate the user entry when its
    // Host-minted endpoint is absent. No locally fabricated Ready or session.
    const missingEndpoint =
      process.platform === "win32"
        ? freshWin32AppLinkPipe()
        : join(stage, "unbound.sock");
    const refused = Bun.spawnSync(
      [
        process.execPath,
        "--no-env-file",
        "--no-install",
        "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
        bootstrap,
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
      refused.exitCode,
      refused.stdout.toString() + refused.stderr.toString(),
    ).toBe(1);
    expect(refused.stdout.toString()).toBe("");
    expect(refused.stderr.toString()).toContain("KELD-IPC-001");

    await provePackedHandshake(
      stage,
      wrapper,
      bootstrap,
      false,
      false,
      false,
      true,
    );
    // The same supported entry path must never start user code when Host Init
    // is rejected; do not turn an ACK-denial into a local Ready or retry.
    await provePackedHandshake(
      stage,
      wrapper,
      bootstrap,
      false,
      false,
      false,
      true,
      true,
    );
    // Use the EXACT source bytes compiled into Core for installed Pack output.
    // Unlike a generated SDK package-local entry, this module is at the app
    // root beside the original app, and is separately admitted by Guardian.
    const installedBootstrap = join(stage, "keld-installed-runtime-entry.mjs");
    writeFileSync(
      installedBootstrap,
      readFileSync(join(import.meta.dir, "installed-runtime-entry.mjs")),
    );
    await provePackedHandshake(
      stage,
      wrapper,
      installedBootstrap,
      false,
      false,
      false,
      true,
      false,
      entry,
    );
    await provePackedHandshake(
      stage,
      wrapper,
      installedBootstrap,
      false,
      false,
      false,
      true,
      true,
      entry,
    );

    // Exact physical filenames containing # and % must never be interpreted
    // as URL fragment/escape syntax. Execute that actual file after Init ACK.
    const specialEntry = join(stage, "original#percent%.mjs");
    writeFileSync(specialEntry, readFileSync(entry));
    await provePackedHandshake(
      stage,
      wrapper,
      installedBootstrap,
      false,
      false,
      false,
      true,
      false,
      specialEntry,
    );

    // %2e%2e is a literal admissible directory name, not an encoded parent
    // traversal. It must load the exact graph-selected in-root file or fail;
    // it may never escape the app root to a different module.
    const encodedParentLiteral = join(stage, "%2e%2e");
    mkdirSync(encodedParentLiteral, { recursive: true });
    const encodedEntry = join(encodedParentLiteral, "literal-original.mjs");
    writeFileSync(encodedEntry, readFileSync(entry));
    await provePackedHandshake(
      stage,
      wrapper,
      installedBootstrap,
      false,
      false,
      false,
      true,
      false,
      encodedEntry,
    );

    // Bun 1.4.4 cannot import a physical '?' filename on Unix (and Windows
    // forbids the name). Refuse it with a typed error before touching Host IPC,
    // rather than silently resolving a URL query to a different original.
    const unsupportedEntry = join(stage, "original?query.mjs");
    if (process.platform !== "win32") {
      writeFileSync(unsupportedEntry, readFileSync(entry));
    }
    const refusedQuestion = Bun.spawnSync(
      [
        process.execPath,
        "--no-env-file",
        "--no-install",
        "--define=__KELD_RUNTIME_PROCESS_FACTS__:true",
        "run",
        installedBootstrap,
        unsupportedEntry,
      ],
      {
        cwd: stage,
        env: {
          ...process.env,
          KELD_APP_LINK: "not-a-qualified-host-link",
          KELD_NATIVE_WINDOW_FACADE: "1",
        },
        stdout: "pipe",
        stderr: "pipe",
        timeout: 9_000,
      },
    );
    expect(refusedQuestion.exitCode).toBe(1);
    expect(refusedQuestion.stdout.toString()).toBe("");
    expect(refusedQuestion.stderr.toString()).toContain("KELD-IPC-005");

    // An unselected or malformed invocation of the exact generated module
    // must not load arbitrary original code or start an app-link handshake.
    const outsideRoot = join(tmpdir(), "not-host-admitted.mjs");
    for (const [name, hasHostSelector, selected, extra] of [
      ["without-host-selector", false, entry, []],
      ["relative-original", true, "./real-user-entry.mjs", []],
      ["outside-app-root", true, outsideRoot, []],
      ["extra-argument", true, entry, ["another-app.mjs"]],
    ] as const) {
      const denied = Bun.spawnSync(
        [
          process.execPath,
          "--no-env-file",
          "--no-install",
          ...(hasHostSelector
            ? ["--define=__KELD_RUNTIME_PROCESS_FACTS__:true"]
            : []),
          "run",
          installedBootstrap,
          selected,
          ...extra,
        ],
        {
          cwd: stage,
          env: {
            ...process.env,
            KELD_APP_LINK: "not-a-qualified-host-link",
            KELD_NATIVE_WINDOW_FACADE: "1",
          },
          stdout: "pipe",
          stderr: "pipe",
          timeout: 9_000,
        },
      );
      expect(
        denied.exitCode,
        `${name}: ${denied.stdout.toString()} ${denied.stderr.toString()}`,
      ).toBe(1);
      expect(denied.stdout.toString()).toBe("");
      expect(denied.stderr.toString()).toContain("KELD-IPC-005");
    }
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}, 30_000);
