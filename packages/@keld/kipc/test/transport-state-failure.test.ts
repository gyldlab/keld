/** Real socket/Worker boundary; hostile values stay in disposable role children. */
import { expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  ECHO_CHANNEL,
  FrameKind,
  FrameReader,
  LIFECYCLE_CHANNEL,
  encodeHeader,
} from "../src/transport.ts";

for (const mode of [
  "proxy-throw",
  "proxy-return",
  "revoked",
  "accessor",
  "typed",
  "message",
  "string",
  "object",
]) {
  test(`state failure ${mode} records terminal state without application hooks`, async () => {
    const root = mkdtempSync(join(tmpdir(), "ksf-"));
    const path = join(root, "a.s");
    const reader = new FrameReader();
    const encoder = new TextEncoder();
    const decoder = new TextDecoder();
    const calls = new Map<string, number>();
    let peer: Bun.Socket<undefined> | undefined;
    function send(
      kind: number,
      channel: number,
      corr: number,
      payload: Uint8Array,
    ) {
      const header = encodeHeader(kind, 0, channel, corr, payload.length);
      const bytes = new Uint8Array(header.length + payload.length);
      bytes.set(header);
      bytes.set(payload, header.length);
      if (peer!.write(bytes) !== bytes.length)
        throw new Error("small fixture frame was not written completely");
    }
    const socket: Bun.SocketHandler<undefined> = {
      binaryType: "uint8array",
      open(socket) {
        peer = socket;
      },
      data(_socket, bytes) {
        reader.push(bytes);
      },
      close() {
        reader.end(new Error("role socket closed"));
      },
      error(_socket, error) {
        reader.fail(error);
      },
    };
    const tcp =
      process.platform === "win32"
        ? Bun.listen<undefined>({ hostname: "127.0.0.1", port: 0, socket })
        : undefined;
    const listener = tcp ?? Bun.listen<undefined>({ unix: path, socket });
    const endpoint = tcp === undefined ? path : String(tcp.port);
    const host = (async () => {
      const hello = await reader.readFrame();
      send(FrameKind.Hello, 0, 0, hello.payload);
      for (let index = 0; index < 3; index += 1) {
        const call = await reader.readFrame();
        expect(call.header.kind).toBe(FrameKind.Call);
        expect(call.header.channel).toBe(ECHO_CHANNEL);
        calls.set(decoder.decode(call.payload), call.header.corr);
      }
      send(
        FrameKind.Reply,
        ECHO_CHANNEL,
        calls.get("earlier")!,
        encoder.encode("earlier-answer"),
      );
      send(FrameKind.Event, LIFECYCLE_CHANNEL, 0, Uint8Array.of(0));
      send(FrameKind.Event, LIFECYCLE_CHANNEL, 0, Uint8Array.of(1));
      send(
        FrameKind.Reply,
        ECHO_CHANNEL,
        calls.get("fault")!,
        encoder.encode("never-return"),
      );
    })();
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dir, "transport-state-failure.test-role.ts"),
        mode,
      ],
      {
        env: {
          ...process.env,
          KELD_APP_LINK: `${endpoint}#${"72".repeat(32)}`,
        },
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const deadline = setTimeout(() => child.kill(), 15_000);
    // A child that fails before connecting must also release the host reader.
    void child.exited.then(() => reader.end(new Error("role process exited")));
    try {
      const [exit, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
        host,
      ]);
      expect({ exit, stderr }).toEqual({
        exit: 0,
        stderr: expect.stringContaining("kipc WorkerLink:"),
      });
      const result = JSON.parse(stdout);
      expect(result).toEqual({
        mode,
        hooks: 0,
        applied: 2,
        delivered: [0],
        cause:
          mode === "typed" || mode === "message" || mode === "object"
            ? "KELD-IPC-003"
            : "KELD-IPC-005",
        detail:
          mode === "typed"
            ? "typed framework failure"
            : mode === "message"
              ? "message framework failure"
              : mode === "object"
                ? "plain object"
                : "state applier failed",
      });
    } finally {
      clearTimeout(deadline);
      child.kill();
      peer?.end();
      listener.stop(true);
      rmSync(root, { recursive: true, force: true });
    }
  }, 20_000);
}
