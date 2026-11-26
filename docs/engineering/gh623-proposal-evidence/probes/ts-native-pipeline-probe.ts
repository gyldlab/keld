import { createServer, type Socket } from "node:net";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  WorkerLink,
  FrameKind,
  encodeHeader,
  HEADER_LEN,
} from "../packages/@keld/kipc/src/transport.ts";
const mode = process.argv[2] ?? "outbound";
const turn = () => new Promise<void>((resolve) => setImmediate(resolve));
const dir = mkdtempSync(join(tmpdir(), "keld-resource-audit-"));
const path = join(dir, "s.sock");
const token = "ab".repeat(32);
let peer: Socket | undefined;
let peerReadBytes = 0;
let hello = Buffer.alloc(0);
let authenticated = false;
const server = createServer((socket) => {
  peer = socket;
  socket.on("error", () => {});
  socket.on("data", (chunk) => {
    peerReadBytes += chunk.length;
    if (authenticated) return;
    hello = Buffer.concat([hello, chunk]);
    if (hello.length >= 48) {
      authenticated = true;
      socket.write(hello.subarray(0, 48));
      if (mode === "outbound") socket.pause();
    }
  });
});
await new Promise<void>((resolve) => server.listen(path, resolve));
let link: WorkerLink | undefined;
const watchdog = setTimeout(() => {
  console.error("watchdog: native audit probe did not finish");
  process.exit(2);
}, 15000);
try {
  link = await WorkerLink.open({
    link: path + "#" + token,
    receive:
      mode === "handlers"
        ? { eventChannels: [], callReceivers: [{ policy: "echoReceiver" }] }
        : { eventChannels: [3], callReceivers: [] },
    ringBytes: 65536,
    ringRecords: 8,
  });
  const ended = new Promise<string>((resolve) =>
    link!.onEnd((error) => resolve(error.code)),
  );
  if (mode === "outbound") {
    let accepted = 0;
    const payloadSize = 65536;
    for (let i = 0; i < 512; i += 1) {
      const payload = new Uint8Array(payloadSize);
      payload[0] = i & 255;
      link.sendEvent(3, payload);
      accepted += 1;
    }
    await turn();
    console.log(
      JSON.stringify({
        mode,
        source: "269d51528137faf574cfd3548586ee3ada1acfda",
        runtime: Bun.version,
        platform: process.platform,
        arch: process.arch,
        acceptedFrames: accepted,
        acceptedPayloadBytes: accepted * payloadSize,
        peerReadBytes,
        peerReadPausedAfterHello: true,
        limitation:
          "Actual WorkerLink and native Unix socket. Accepted payload bytes are not a measured split of mailbox, WriteQueue and kernel bytes.",
      }),
    );
  } else {
    let started = 0;
    let completed = 0;
    let release!: () => void;
    const barrier = new Promise<void>((resolve) => {
      release = resolve;
    });
    link.setCallHandler(1, async (payload) => {
      started += 1;
      await barrier;
      completed += 1;
      return { kind: FrameKind.Reply, payload };
    });
    const count = 512;
    for (let i = 0; i < count; i += 1) {
      const payload = new Uint8Array(64);
      const header = encodeHeader(FrameKind.Call, 0, 1, i + 1, payload.length);
      peer!.write(Buffer.concat([header, payload]));
      while (started <= i) await turn();
    }
    console.log(
      JSON.stringify({
        mode,
        source: "269d51528137faf574cfd3548586ee3ada1acfda",
        runtime: Bun.version,
        platform: process.platform,
        arch: process.arch,
        configuredRingRecords: 8,
        startedHandlers: started,
        completedHandlers: completed,
        retainedHandlerPayloadBytes: started * 64,
        observed:
          "Healthy transport drained all 512 CALLs into unresolved handler continuations despite only 8 ring slots.",
      }),
    );
    link.close();
    await ended;
    const startedAtClose = started;
    release();
    await turn();
    console.log(
      JSON.stringify({
        afterClose: true,
        startedHandlers: startedAtClose,
        completedAfterBarrierRelease: completed,
        observed:
          "Link close does not cancel application handler continuations; late responses are suppressed by terminal state.",
      }),
    );
  }
  link.close();
  peer?.destroy();
  const terminal = await ended;
  console.log(JSON.stringify({ cleanup: true, terminal }));
} finally {
  clearTimeout(watchdog);
  link?.close();
  peer?.destroy();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  rmSync(dir, { recursive: true, force: true });
}
