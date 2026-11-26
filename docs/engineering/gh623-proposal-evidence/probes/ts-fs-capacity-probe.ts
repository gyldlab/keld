import { createServer, type Socket } from "node:net";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  WorkerLink,
  FrameKind,
  encodeHeader,
  encodeVarint,
  DEFAULT_RING_BYTES,
} from "../packages/@keld/kipc/src/transport.ts";
import { decodeFsResponse } from "../packages/@keld/api/src/fs.generated.ts";
const sourcePin = "269d51528137faf574cfd3548586ee3ada1acfda";
const dir = mkdtempSync(join(tmpdir(), "keld-fs-capacity-audit-"));
const path = join(dir, "s.sock");
let peer: Socket | undefined;
let incoming = Buffer.alloc(0);
let helloDone = false;
const fileBytes = 2 * 1024 * 1024;
const response = Buffer.concat([
  Uint8Array.of(0),
  encodeVarint(fileBytes),
  new Uint8Array(fileBytes),
]);
const decoded = decodeFsResponse(response);
if (decoded.variant !== "Read" || decoded.bytes.length !== fileBytes)
  throw new Error("fixture is not a valid FS Read response");
const server = createServer((socket) => {
  peer = socket;
  socket.on("error", () => {});
  socket.on("data", (bytes) => {
    incoming = Buffer.concat([incoming, bytes]);
    if (!helloDone) {
      if (incoming.length < 48) return;
      socket.write(incoming.subarray(0, 48));
      incoming = incoming.subarray(48);
      helloDone = true;
    }
    if (incoming.length < 16) return;
    const view = new DataView(
      incoming.buffer,
      incoming.byteOffset,
      incoming.byteLength,
    );
    const len = view.getUint32(12, true);
    if (incoming.length < 16 + len) return;
    const corr = view.getUint32(8, true);
    socket.write(
      Buffer.concat([
        encodeHeader(FrameKind.Reply, 0, 2, corr, response.length),
        response,
      ]),
    );
    incoming = incoming.subarray(16 + len);
  });
});
await new Promise<void>((resolve) => server.listen(path, resolve));
const watchdog = setTimeout(() => process.exit(2), 15000);
let link: WorkerLink | undefined;
try {
  link = await WorkerLink.open({
    link: path + "#" + "ab".repeat(32),
    receive: { eventChannels: [], callReceivers: [] },
  });
  let code = "";
  try {
    await link.call(2, Uint8Array.of(0, 1, 120), 5000);
    code = "resolved";
  } catch (error) {
    code = String((error as { code?: string }).code ?? error);
  }
  console.log(
    JSON.stringify({
      source: sourcePin,
      runtime: Bun.version,
      platform: process.platform,
      arch: process.arch,
      defaultRingBytes: DEFAULT_RING_BYTES,
      fileBytes,
      nativeFsContentCapBytes: 8 * 1024 * 1024,
      validFsPayloadBytes: response.length,
      result: code,
      limitation:
        "Real WorkerLink/UDS with independently generated valid FS response; not a real filesystem Host run.",
    }),
  );
  if (code !== "KELD-IPC-026")
    throw new Error("expected current default-ring capacity failure");
} finally {
  clearTimeout(watchdog);
  link?.close();
  peer?.destroy();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  rmSync(dir, { recursive: true, force: true });
}
