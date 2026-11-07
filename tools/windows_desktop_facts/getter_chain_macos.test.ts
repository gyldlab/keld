/**
 * GH1186 Mac code-only gap: real S1 WorkerLink -> public Ready -> real Electron
 * screen/nativeTheme getters. Scripted Unix-socket Host; no native OS capture,
 * Windows acceptance, new mirror, application RPC, or product-source edit.
 */
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { test } from "bun:test";

const PIN = "e09b520eaee1e4bd69bd0bde2137ec52f0fed18a";
const repo = join(import.meta.dir, "../..");
const role = join(import.meta.dir, "getter_chain_macos_role.ts");
const launchFacts = {
  schema: 3,
  kind: "development",
  name: "gh1186-mac-getter-chain-test",
  root: "/qualified-test-host",
  paths: {},
  locale: "en-US",
};
// These are only *source* dependencies of the unmodified public getter path.
// The existing prepare_shared.py first checks the eleven pinned mirror files.
const ADDITIONAL_S1: Record<string, string> = {
  "packages/@keld/api/package.json": "ada5d00069dfd9d48dbf2be2e80e3c5f4a509a81",
  "packages/@keld/api/src/app.ts": "25bd240eaa495545c3854cca57c0704a719dbb11",
  "packages/@keld/api/src/channels.ts":
    "236a88fd07e86d1f4bc324881c5eb04b374f27bd",
  "packages/@keld/api/src/echo.generated.ts":
    "f98c29bb1bd59f3088ffaca5104f77750f156be1",
  "packages/@keld/api/src/window-close-receipts.ts":
    "7afe7bcbee907383a51c570a9d27008a8fdd526a",
  "packages/@keld/electron/src/screen.ts":
    "03371a297f0308696cea3b1a1aa530225b1bf9ef",
  "packages/@keld/electron/src/native-theme.ts":
    "57ec366379e62324d7c27d8a72a3e620eec97b86",
  "packages/@keld/electron/src/unsupported.ts":
    "096cfd3648a5b74f03574e10a75aa3f8f5049184",
};

function run(command: string[]): Uint8Array {
  const result = Bun.spawnSync({
    cmd: command,
    stdout: "pipe",
    stderr: "pipe",
  });
  assert.equal(
    result.exitCode,
    0,
    `immutable source export failed: ${command.join(" ")}\n${result.stderr.toString()}`,
  );
  return new Uint8Array(result.stdout);
}

function stageSource(root: string, mutateReady = false): string {
  const shared = join(root, "shared");
  const receipt = JSON.parse(
    Buffer.from(
      run([
        "python3",
        "-B",
        join(import.meta.dir, "prepare_shared.py"),
        "--repo",
        repo,
        "--out",
        shared,
      ]),
    ).toString(),
  ) as { commit: string; files: number };
  assert.equal(receipt.commit, PIN);
  assert.equal(receipt.files, 11);
  for (const [name, blob] of Object.entries(ADDITIONAL_S1)) {
    const identity = Buffer.from(
      run(["git", "-C", repo, "rev-parse", `${PIN}:${name}`]),
    )
      .toString()
      .trim();
    assert.equal(
      identity,
      blob,
      `immutable S1 source identity changed: ${name}`,
    );
    const destination = join(shared, name);
    mkdirSync(dirname(destination), { recursive: true });
    writeFileSync(
      destination,
      run(["git", "-C", repo, "show", `${PIN}:${name}`]),
      { flag: "wx" },
    );
  }
  // Source-derived package resolution. Both imports point to one real API
  // singleton; the role asserts that equality before starting its link.
  symlinkSync(
    join(shared, "packages/@keld/api"),
    join(shared, "node_modules/@keld/api"),
    "dir",
  );
  // Bun resolves bare imports from the child script's own path. Stage this
  // test-only child beside the verified package links, outside the checkout.
  writeFileSync(
    join(shared, "getter_chain_macos_role.ts"),
    readFileSync(role),
    { flag: "wx" },
  );
  if (mutateReady) {
    // Opt-in temporary failure-first control, never a tracked source edit.
    const file = join(shared, "packages/@keld/api/src/app.ts");
    const original = readFileSync(file, "utf8");
    const guard = "else if (!connectedLink.windowSource.desktop()) {";
    assert.equal(
      original.split(guard).length,
      2,
      "expected one exact S1 Ready predicate",
    );
    writeFileSync(file, original.replace(guard, "else if (false) {"));
  }
  if (process.env.W4_GETTER_FAULT === "block-restoration") {
    // Test-only negative control: initial and changed snapshots still work,
    // but an invalidated mirror cannot recover from the next valid snapshot.
    // The immutable source and production files are never changed.
    const file = join(shared, "packages/@keld/api/src/window-source.ts");
    const original = readFileSync(file, "utf8");
    const assignment = "this.#desktop = next;";
    assert.equal(
      original.split(assignment).length,
      2,
      "expected one exact S1 desktop restoration assignment",
    );
    writeFileSync(
      file,
      original.replace(
        assignment,
        "if (this.#desktopEpoch === 0n || this.#desktop !== undefined || fact.snapshot === null) this.#desktop = next;",
      ),
    );
  }
  return shared;
}

type Notice = Record<string, unknown>;
async function withPeer(
  operation: (peer: Awaited<ReturnType<typeof startPeer>>) => Promise<void>,
  mutateReady = false,
): Promise<void> {
  const root = mkdtempSync(join(tmpdir(), "w4-getter-"));
  let peer: Awaited<ReturnType<typeof startPeer>> | undefined;
  let failure: unknown;
  try {
    const shared = stageSource(root, mutateReady);
    peer = await startPeer(shared, join(root, "link.sock"));
    await operation(peer);
  } catch (error) {
    failure = error;
  } finally {
    try {
      await peer?.dispose(failure !== undefined);
    } catch (error) {
      if (failure === undefined) failure = error;
      else console.error("owned getter-chain cleanup also failed:", error);
    }
    rmSync(root, { recursive: true, force: true });
  }
  if (failure !== undefined) throw failure;
}

async function startPeer(shared: string, endpoint: string) {
  const t = await import(
    pathToFileURL(join(shared, "packages/@keld/kipc/src/transport.ts")).href
  );
  const codec = await import(
    pathToFileURL(join(shared, "packages/@keld/api/src/window-codec.ts")).href
  );
  const token = randomBytes(32);
  const reader = new t.FrameReader();
  const drain = new t.DrainSignal();
  const acceptedSocket = Promise.withResolvers<Bun.Socket<undefined>>();
  const peerClosed = Promise.withResolvers<void>();
  let connections = 0;
  let didClose = false;
  let sawChildFatal = false;
  const listener = Bun.listen<undefined>({
    unix: endpoint,
    socket: {
      binaryType: "uint8array",
      open(socket) {
        connections += 1;
        if (connections !== 1) {
          socket.end();
          reader.fail(new Error("unexpected second app link"));
        } else acceptedSocket.resolve(socket);
      },
      data(_socket, bytes) {
        reader.push(bytes);
      },
      drain() {
        drain.fire();
      },
      error(_socket, error) {
        reader.fail(error);
      },
      close() {
        didClose = true;
        peerClosed.resolve();
        reader.end(new Error("owned scripted Host closed"));
      },
    },
  });
  const notices: Notice[] = [];
  const waiters: Array<{
    match: (notice: Notice) => boolean;
    resolve: (notice: Notice) => void;
  }> = [];
  function noticeMatching(match: (notice: Notice) => boolean): Promise<Notice> {
    const index = notices.findIndex(
      (notice) => match(notice) || notice.kind === "fatal",
    );
    if (index >= 0) return Promise.resolve(notices.splice(index, 1)[0]!);
    return new Promise((resolve) => waiters.push({ match, resolve }));
  }
  const child = Bun.spawn({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      '--define=process.env.KELD_NATIVE_WINDOW_FACADE:"1"',
      `--define=__KELD_HOST_LAUNCH_FACTS__:${JSON.stringify(launchFacts)}`,
      `--preload=${join(shared, "packages/@keld/kipc/src/transport.ts")}`,
      join(shared, "getter_chain_macos_role.ts"),
    ],
    env: {
      ...process.env,
      W4_SHARED_ROOT: shared,
      KELD_APP_LINK: `${endpoint}#${token.toString("hex")}`,
      KELD_NATIVE_WINDOW_FACADE: "1",
    },
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
    ipc(input) {
      assert.ok(input && typeof input === "object");
      const value = input as Notice;
      if (value.kind === "fatal") sawChildFatal = true;
      const i = waiters.findIndex(
        (waiter) => waiter.match(value) || value.kind === "fatal",
      );
      if (i < 0) notices.push(value);
      else waiters.splice(i, 1)[0]!.resolve(value);
    },
  });
  const stdout = new Response(child.stdout).text();
  const stderr = new Response(child.stderr).text();
  let socket: Bun.Socket<undefined>;
  try {
    socket = await t.withIoDeadline(acceptedSocket.promise, 5000);
  } catch (error) {
    if (child.exitCode === null) child.kill();
    await child.exited;
    listener.stop(true);
    reader.fail(new Error("setup aborted"));
    throw new Error(
      `getter-chain child could not connect: ${String(error)}\n${await stdout}\n${await stderr}`,
    );
  }
  const writer = new t.WriteQueue(socket, drain);
  const incoming: Array<{
    header: { kind: number; channel: number; corr: number };
    payload: Uint8Array;
  }> = [];
  let nextId = 0;
  let nextCorrelation = 10000;
  async function read() {
    const frame = await t.withIoDeadline(reader.readFrame(), 5000);
    incoming.push(frame);
    return frame;
  }
  async function send(
    kind: number,
    channel: number,
    corr: number,
    payload: Uint8Array,
  ) {
    await writer.writeFrame(kind, 0, channel, corr, payload);
  }
  async function notice(kind: string): Promise<Notice> {
    const value = await t.withIoDeadline(
      noticeMatching((n) => n.kind === kind),
      5000,
    );
    assert.notEqual(value.kind, "fatal", String(value.error));
    return value;
  }
  async function request(
    command: string,
    extra: Record<string, unknown> = {},
  ): Promise<Notice> {
    const id = ++nextId;
    child.send({ command, id, ...extra });
    const value = await t.withIoDeadline(
      noticeMatching((n) => n.kind === "response" && n.id === id),
      5000,
    );
    assert.notEqual(value.kind, "fatal", String(value.error));
    return value;
  }
  async function handshake(): Promise<number> {
    const hello = await read();
    t.validateReceivedHeader(
      t.RECEIVE_POLICIES.serverPreAuthHello,
      hello.header,
    );
    assert.deepEqual(hello.payload, new Uint8Array(token));
    await send(t.FrameKind.Hello, t.HANDSHAKE_CHANNEL, 0, token);
    const subscribe = await read();
    assert.equal(subscribe.header.kind, t.FrameKind.Call);
    assert.equal(subscribe.header.channel, t.LIFECYCLE_CHANNEL);
    assert.ok(subscribe.header.corr > 0);
    assert.deepEqual(
      subscribe.payload,
      Uint8Array.of(1, ...t.encodeVarint(t.WINDOW_CHANNEL)),
    );
    return subscribe.header.corr;
  }
  async function desktop(
    epoch: bigint,
    snapshot: typeof initial | null,
  ): Promise<void> {
    await send(
      t.FrameKind.Event,
      t.WINDOW_CHANNEL,
      0,
      codec.encodeWindowEvent({ variant: "DesktopSnapshot", epoch, snapshot }),
    );
  }
  async function created(): Promise<void> {
    await send(
      t.FrameKind.Event,
      t.WINDOW_CHANNEL,
      0,
      codec.encodeWindowEvent({
        variant: "Created",
        window: { id: 7, incarnation: 300n },
        state: {
          bounds: { x: 20, y: 30, width: 640, height: 480 },
          focused: false,
          maximized: false,
          full_screen: false,
        },
        pending_close: null,
      }),
    );
  }
  async function accepted(correlation: number): Promise<void> {
    await send(
      t.FrameKind.Reply,
      t.LIFECYCLE_CHANNEL,
      correlation,
      Uint8Array.of(1),
    );
  }
  async function ready(): Promise<void> {
    await send(t.FrameKind.Event, t.LIFECYCLE_CHANNEL, 0, Uint8Array.of(0));
  }
  async function fence(): Promise<void> {
    const corr = ++nextCorrelation;
    const message = t.encodePostcardString("W4 getter fence", "echo message");
    const count = t.encodeVarint(19);
    const payload = Uint8Array.of(...message, ...count);
    await send(t.FrameKind.Call, t.ECHO_CHANNEL, corr, payload);
    const frame = await read();
    assert.deepEqual(
      [frame.header.kind, frame.header.channel, frame.header.corr],
      [t.FrameKind.Reply, t.ECHO_CHANNEL, corr],
    );
    assert.deepEqual(frame.payload, payload);
  }
  async function dispose(allowExpectedFault = false): Promise<void> {
    let cleanupError: unknown;
    try {
      let exit: number | null = child.exitCode;
      if (child.exitCode === null) {
        if (allowExpectedFault && sawChildFatal) {
          // An intentionally broken staged source already terminated the role.
          // Never send finish to a dead peer and obscure the semantic failure
          // with a secondary five-second IPC timeout.
          exit = await t.withIoDeadline(child.exited, 5000);
        } else {
          await request("finish");
          exit = await t.withIoDeadline(child.exited, 5000);
        }
      }
      if (allowExpectedFault && sawChildFatal) {
        assert.notEqual(exit, 0, "deliberately mutated role must fail");
      } else {
        assert.equal(
          exit,
          0,
          "owned getter-chain child must exit successfully",
        );
      }
      await t.withIoDeadline(peerClosed.promise, 5000);
    } catch (error) {
      cleanupError = error;
    } finally {
      if (child.exitCode === null) {
        child.kill();
        try {
          await t.withIoDeadline(child.exited, 5000);
        } catch (error) {
          cleanupError ??= error;
        }
      }
      listener.stop(true);
      reader.fail(new Error("fixture disposed"));
      const [out, err] = await Promise.all([stdout, stderr]);
      if (cleanupError === undefined) {
        if (allowExpectedFault && sawChildFatal) {
          assert.match(err, /KELD-CORE-049/);
        } else {
          assert.equal(
            err,
            "",
            "owned child must not emit unexplained diagnostics",
          );
          assert.match(out, /"cleanup":true/);
        }
        assert.equal(didClose, true);
      } else {
        throw new Error(
          `getter-chain cleanup failed: ${String(cleanupError)}\n${out}\n${err}`,
        );
      }
    }
  }
  return {
    t,
    incoming,
    handshake,
    desktop,
    created,
    accepted,
    ready,
    fence,
    notice,
    request,
    dispose,
  };
}

const initial = {
  primary_id: 17,
  displays: [
    {
      id: 17,
      bounds: { x: 0, y: 0, width: 1440, height: 900 },
      work_area: { x: 0, y: 24, width: 1440, height: 876 },
      scale_factor: 2,
    },
  ],
  should_use_dark_colors: false,
};
const changed = {
  primary_id: 17,
  displays: [
    {
      id: 17,
      bounds: { x: 0, y: 0, width: 1440, height: 900 },
      work_area: { x: 0, y: 32, width: 1440, height: 868 },
      scale_factor: 2,
    },
  ],
  should_use_dark_colors: true,
};
const expectedInitial = {
  all: [
    {
      id: 17,
      bounds: { x: 0, y: 0, width: 1440, height: 900 },
      workArea: { x: 0, y: 24, width: 1440, height: 876 },
      scaleFactor: 2,
    },
  ],
  primary: {
    id: 17,
    bounds: { x: 0, y: 0, width: 1440, height: 900 },
    workArea: { x: 0, y: 24, width: 1440, height: 876 },
    scaleFactor: 2,
  },
  dark: false,
};
const expectedChanged = {
  all: [
    {
      id: 17,
      bounds: { x: 0, y: 0, width: 1440, height: 900 },
      workArea: { x: 0, y: 32, width: 1440, height: 868 },
      scaleFactor: 2,
    },
  ],
  primary: {
    id: 17,
    bounds: { x: 0, y: 0, width: 1440, height: 900 },
    workArea: { x: 0, y: 32, width: 1440, height: 868 },
    scaleFactor: 2,
  },
  dark: true,
};

for (const order of [
  "sample-before-raw-ready",
  "raw-ready-before-sample",
] as const) {
  test.skipIf(process.platform !== "darwin")(
    `W4 real S1 transport -> public Electron getters: ${order}`,
    async () => {
      await withPeer(async (peer) => {
        const corr = await peer.handshake();
        const pending = await peer.request("checkpoint");
        assert.equal(pending.ready, false);
        assert.equal(pending.readyOutcome, "pending");
        assert.equal(pending.getterError, "KELD-CORE-040");
        assert.deepEqual(pending.getterErrors, {
          all: "KELD-CORE-040",
          primary: "KELD-CORE-040",
          dark: "KELD-CORE-040",
        });
        if (order === "raw-ready-before-sample") await peer.ready();
        await peer.fence();
        const stillPending = await peer.request("checkpoint");
        assert.equal(stillPending.ready, false);
        assert.equal(stillPending.readyOutcome, "pending");
        await peer.desktop(1n, initial);
        await peer.created();
        const first = await peer.notice("fact");
        const replay = await peer.notice("fact");
        assert.deepEqual(
          [first.variant, first.epoch, first.publicReady],
          ["DesktopSnapshot", "1", false],
        );
        assert.deepEqual(
          [replay.variant, replay.publicReady],
          ["Created", false],
        );
        await peer.fence();
        const beforeAck = await peer.request("checkpoint");
        assert.equal(beforeAck.ready, false);
        assert.equal(beforeAck.readyOutcome, "pending");
        assert.deepEqual(beforeAck.factOrder, ["DesktopSnapshot", "Created"]);
        await peer.accepted(corr);
        if (order === "sample-before-raw-ready") await peer.ready();
        const result = await peer.notice("ready-result");
        assert.equal(result.outcome, "ready");
        assert.equal(result.ready, true);
        const beforeCalls = peer.incoming.filter(
          (frame) => frame.header.kind === peer.t.FrameKind.Call,
        ).length;
        assert.equal(
          (
            await peer.request("getters", {
              expected: expectedInitial,
              count: 1000,
            })
          ).hostCalls,
          0,
        );
        await peer.fence();
        assert.equal(
          peer.incoming.filter(
            (frame) => frame.header.kind === peer.t.FrameKind.Call,
          ).length,
          beforeCalls,
        );
        await peer.desktop(2n, changed);
        const update = await peer.notice("fact");
        assert.deepEqual(
          [update.variant, update.epoch],
          ["DesktopSnapshot", "2"],
        );
        // The replay's Created and e2 facts must also be observed by the real consumer.
        const checkpoint = await peer.request("checkpoint");
        assert.deepEqual(checkpoint.factOrder, [
          "DesktopSnapshot",
          "Created",
          "DesktopSnapshot",
        ]);
        assert.equal(checkpoint.ready, true);
        assert.equal(
          (
            await peer.request("getters", {
              expected: expectedChanged,
              count: 1000,
            })
          ).hostCalls,
          0,
        );
        await peer.fence();
        assert.equal(
          peer.incoming.filter(
            (frame) => frame.header.kind === peer.t.FrameKind.Call,
          ).length,
          beforeCalls,
        );
        await peer.desktop(3n, null);
        const invalidation = await peer.notice("fact");
        assert.deepEqual(
          [invalidation.variant, invalidation.epoch],
          ["DesktopSnapshot", "3"],
        );
        const unavailable = await peer.request("checkpoint");
        assert.equal(unavailable.ready, true);
        assert.equal(unavailable.getterError, "KELD-CORE-049");
        assert.deepEqual(unavailable.getterErrors, {
          all: "KELD-CORE-049",
          primary: "KELD-CORE-049",
          dark: "KELD-CORE-049",
        });
        const failure = await peer.request("check-failure", {
          expected: { code: "KELD-CORE-049" },
        });
        assert.equal(failure.readyOutcome, "ready");
        assert.equal(failure.getterError, "KELD-CORE-049");
        // A later valid Host observation can restore the same ready role.
        // This is not a new Ready or an app-side recovery RPC.
        await peer.desktop(4n, initial);
        const restored = await peer.notice("fact");
        assert.deepEqual(
          [restored.variant, restored.epoch, restored.publicReady],
          ["DesktopSnapshot", "4", true],
        );
        const afterRestore = await peer.request("checkpoint");
        assert.equal(afterRestore.ready, true);
        assert.equal(afterRestore.readyOutcome, "ready");
        assert.deepEqual(afterRestore.factOrder, [
          "DesktopSnapshot",
          "Created",
          "DesktopSnapshot",
          "DesktopSnapshot",
          "DesktopSnapshot",
        ]);
        assert.equal(
          (
            await peer.request("getters", {
              expected: expectedInitial,
              count: 1000,
            })
          ).hostCalls,
          0,
          "post-invalidation public getters must use restored local snapshot",
        );
        await peer.fence();
        assert.equal(
          peer.incoming.filter(
            (frame) => frame.header.kind === peer.t.FrameKind.Call,
          ).length,
          beforeCalls,
          "getter recovery must not issue additional Host calls",
        );
      });
    },
    15000,
  );
}

test.skipIf(process.platform !== "darwin")(
  "W4 missing first sample rejects public Ready and cannot revive the role",
  async () => {
    await withPeer(async (peer) => {
      const corr = await peer.handshake();
      assert.equal((await peer.request("checkpoint")).ready, false);
      await peer.ready();
      await peer.accepted(corr);
      const outcome = await peer.notice("ready-result");
      assert.deepEqual(
        [outcome.outcome, outcome.code, outcome.ready],
        ["rejected", "KELD-CORE-049", false],
      );
      const failed = await peer.request("check-failure", {
        expected: { code: "KELD-CORE-049" },
      });
      assert.equal(failed.readyOutcome, "rejected");
      assert.equal(failed.ready, false);
    }, process.env.W4_GETTER_FAULT === "skip-current-snapshot-check");
  },
  15000,
);
