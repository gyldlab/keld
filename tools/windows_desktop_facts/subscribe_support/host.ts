/** Scripted peer around the canonical S1 codec. This is not Core admission. */
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

export type Notice = Record<string, unknown>;
export type Frame = {
  header: {
    kind: number;
    flags: number;
    channel: number;
    corr: number;
    len: number;
  };
  payload: Uint8Array;
};

export async function dependencies(root: string) {
  const transport = await import(
    pathToFileURL(join(root, "packages/@keld/kipc/src/transport.ts")).href
  );
  const codec = await import(
    pathToFileURL(join(root, "packages/@keld/api/src/window-codec.ts")).href
  );
  const host = await import(
    pathToFileURL(join(root, "packages/@keld/kipc/test/app-link-host.ts")).href
  );
  return { transport, codec, host };
}

class Inbox {
  readonly history: Notice[] = [];
  readonly pending: Notice[] = [];
  readonly waiters: Array<{
    match: (value: Notice) => boolean;
    resolve: (value: Notice) => void;
  }> = [];

  push(value: unknown): void {
    assert.ok(value && typeof value === "object");
    const notice = value as Notice;
    this.history.push(notice);
    const index = this.waiters.findIndex((waiter) => waiter.match(notice));
    if (index < 0) this.pending.push(notice);
    else this.waiters.splice(index, 1)[0]!.resolve(notice);
  }

  take(match: (value: Notice) => boolean): Promise<Notice> {
    const index = this.pending.findIndex(match);
    if (index >= 0) return Promise.resolve(this.pending.splice(index, 1)[0]!);
    return new Promise((resolve) => this.waiters.push({ match, resolve }));
  }
}

export async function fixture(
  root: string,
  nativeFacade = true,
  expectedStderr = "",
) {
  assert.equal(
    process.platform,
    "win32",
    "this fixture requires Windows named pipes",
  );
  const { transport: t, codec, host } = await dependencies(root);
  const endpoint = host.freshWin32AppLinkPipe();
  assert.equal(t.isWin32PipeEndpoint(endpoint), true);
  const token = randomBytes(32);
  const reader = new t.FrameReader();
  const drain = new t.DrainSignal();
  const accepted = Promise.withResolvers<Bun.Socket<undefined>>();
  let acceptedCount = 0;
  let closed = false;
  const peerClosed = Promise.withResolvers<void>();
  const listener = Bun.listen({
    unix: endpoint,
    socket: {
      binaryType: "uint8array",
      open(socket) {
        acceptedCount += 1;
        if (acceptedCount !== 1) {
          socket.end();
          reader.fail(new Error("unexpected second connection"));
          return;
        }
        accepted.resolve(socket);
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
        closed = true;
        peerClosed.resolve();
        reader.end(new Error("fixture peer closed"));
      },
    },
  });
  const inbox = new Inbox();
  const child = Bun.spawn({
    cmd: [
      process.execPath,
      "--no-install",
      "--no-env-file",
      join(import.meta.dir, "role.ts"),
    ],
    env: {
      ...process.env,
      W4_SHARED_ROOT: root,
      KELD_NATIVE_WINDOW_FACADE: nativeFacade ? "1" : "0",
      W4_TEST_LINK: endpoint + "#" + token.toString("hex"),
    },
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
    ipc(value) {
      inbox.push(value);
    },
  });
  const stdout = new Response(child.stdout).text();
  const stderr = new Response(child.stderr).text();
  let socket: Bun.Socket<undefined>;
  try {
    socket = await t.withIoDeadline(accepted.promise);
  } catch (error) {
    if (child.exitCode === null) child.kill();
    await child.exited;
    listener.stop(true);
    reader.fail(new Error("owned fixture setup failed"));
    throw new Error(
      String(error) + "\n" + (await stdout) + "\n" + (await stderr),
    );
  }
  const writer = new t.WriteQueue(socket, drain);
  const incoming: Frame[] = [];
  const outgoing: Frame[] = [];
  let commandId = 0;
  let echoCorr = 10000;
  async function read(): Promise<Frame> {
    const frame = await t.withIoDeadline(reader.readFrame());
    incoming.push(frame);
    return frame;
  }
  async function send(
    kind: number,
    channel: number,
    corr: number,
    payload: Uint8Array,
  ) {
    outgoing.push({
      header: { kind, flags: 0, channel, corr, len: payload.byteLength },
      payload,
    });
    await writer.writeFrame(kind, 0, channel, corr, payload);
  }
  async function notice(kind: string): Promise<Notice> {
    const value = await t.withIoDeadline(
      inbox.take((item) => item.kind === kind || item.kind === "fatal"),
      7000,
    );
    assert.notEqual(value.kind, "fatal", String(value.error));
    return value;
  }
  async function request(command: string, extra: Notice = {}): Promise<Notice> {
    const id = ++commandId;
    child.send({ command, id, ...extra });
    const value = await t.withIoDeadline(
      inbox.take(
        (item) =>
          (item.kind === "response" && item.id === id) || item.kind === "fatal",
      ),
    );
    assert.notEqual(value.kind, "fatal", String(value.error));
    return value;
  }
  async function hello(accept = true): Promise<void> {
    const frame = await read();
    t.validateReceivedHeader(
      t.RECEIVE_POLICIES.serverPreAuthHello,
      frame.header,
    );
    assert.deepEqual(frame.payload, new Uint8Array(token));
    await send(
      t.FrameKind.Hello,
      t.HANDSHAKE_CHANNEL,
      0,
      accept ? token : new Uint8Array(32),
    );
  }
  async function subscribe(): Promise<Frame> {
    const frame = await read();
    assert.equal(frame.header.kind, t.FrameKind.Call);
    assert.equal(frame.header.channel, t.LIFECYCLE_CHANNEL);
    assert.ok(frame.header.corr > 0);
    assert.deepEqual(
      frame.payload,
      Uint8Array.of(1, ...t.encodeVarint(t.WINDOW_CHANNEL)),
    );
    await notice("connected");
    return frame;
  }
  async function fence(): Promise<void> {
    const corr = ++echoCorr;
    const payload = randomBytes(16);
    await send(t.FrameKind.Call, t.ECHO_CHANNEL, corr, payload);
    const answer = await read();
    assert.deepEqual(
      [answer.header.kind, answer.header.channel, answer.header.corr],
      [t.FrameKind.Reply, t.ECHO_CHANNEL, corr],
      "no unexpected outbound Host call may pass the echo observation fence",
    );
    assert.deepEqual(answer.payload, new Uint8Array(payload));
  }
  async function dispose(): Promise<void> {
    let cleanupError: unknown;
    try {
      if (child.exitCode === null) {
        child.send({ command: "finish", id: ++commandId });
        const exit = await t.withIoDeadline(child.exited, 5000);
        assert.equal(exit, 0, "owned role did not exit cleanly");
      }
      await t.withIoDeadline(peerClosed.promise);
    } catch (error) {
      cleanupError = error;
    } finally {
      if (child.exitCode === null) {
        child.kill();
        await child.exited;
      }
      listener.stop(true);
      reader.fail(new Error("owned fixture disposed"));
      const [out, err] = await Promise.all([stdout, stderr]);
      if (cleanupError === undefined) {
        assert.equal(
          err,
          expectedStderr,
          "owned role stderr must match the exact declared diagnostic",
        );
        assert.match(
          out,
          /"cleanup":/,
          "role must report retirement before exit",
        );
        assert.equal(closed, true, "owned named-pipe peer must close");
      }
      if (cleanupError !== undefined) {
        throw new Error(
          "owned fixture cleanup failed: " +
            String(cleanupError) +
            "\n" +
            out +
            "\n" +
            err,
        );
      }
    }
  }
  return {
    t,
    codec,
    token,
    endpoint,
    inbox,
    incoming,
    outgoing,
    child,
    read,
    send,
    notice,
    request,
    hello,
    subscribe,
    fence,
    dispose,
    isClosed: () => closed,
  };
}

export type Fixture = Awaited<ReturnType<typeof fixture>>;

export async function withFixture(
  root: string,
  operation: (peer: Fixture) => Promise<void>,
  nativeFacade = true,
  expectedStderr = "",
): Promise<void> {
  const peer = await fixture(root, nativeFacade, expectedStderr);
  let failed = false;
  try {
    await operation(peer);
  } catch (error) {
    failed = true;
    throw error;
  } finally {
    try {
      await peer.dispose();
    } catch (cleanupError) {
      if (!failed) throw cleanupError;
      console.error("Additional owned-fixture cleanup failure:", cleanupError);
    }
  }
}
