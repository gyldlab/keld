import { MessageChannel, receiveMessageOnPort } from "node:worker_threads";
import {
  WriteQueue,
  DrainSignal,
  FrameKind,
  HEADER_LEN,
  MAX_FRAME_LEN,
} from "../packages/@keld/kipc/src/transport.ts";
import { encodeFsRequest } from "../packages/@keld/api/src/fs.generated.ts";

const turn = () => new Promise<void>((resolve) => setImmediate(resolve));
const results: Record<string, unknown> = {
  source: "269d51528137faf574cfd3548586ee3ada1acfda",
  runtime: Bun.version,
  platform: process.platform,
  arch: process.arch,
};
const assert = (value: boolean, detail: string) => {
  if (!value) throw new Error(detail);
};

// One tiny logical payload can carry a much larger cloned ArrayBuffer.
{
  const channel = new MessageChannel();
  const backing = new Uint8Array(8 * 1024 * 1024);
  const view = backing.subarray(123, 124);
  view[0] = 65;
  channel.port1.postMessage({ payload: view });
  view[0] = 66;
  const received = receiveMessageOnPort(channel.port2)?.message
    .payload as Uint8Array;
  assert(received?.byteLength === 1, "one payload byte must arrive");
  results.subviewClone = {
    logicalBytes: received.byteLength,
    backingBytes: received.buffer.byteLength,
    byteOffset: received.byteOffset,
    receivedByte: received[0],
    producerByteAfterPost: view[0],
  };
  channel.port1.close();
  channel.port2.close();
}

// The public WriteQueue delays the snapshot until execution, not admission.
{
  const wire: Uint8Array[] = [];
  const queue = new WriteQueue(
    {
      write(bytes: Uint8Array) {
        wire.push(new Uint8Array(bytes));
        return bytes.length;
      },
      end() {},
    },
    new DrainSignal(),
  );
  const bytes = new Uint8Array([65]);
  const writing = queue.writeFrame(FrameKind.Event, 0, 3, 0, bytes);
  bytes[0] = 66;
  await writing;
  results.mutableQueuedPayload = {
    suppliedByte: 65,
    laterByte: 66,
    wireByte: wire[0]![HEADER_LEN],
  };
}

// Keep no strong payload references outside the writer; wait across jobs before GC.
{
  const count = Number(process.env.KIPC_AUDIT_FRAMES ?? 512);
  const bytesEach = 64 * 1024;
  const budget = Number(process.env.KIPC_AUDIT_BUDGET ?? 8 * 1024 * 1024);
  let open = false;
  let writes = 0;
  let settled = 0;
  const weak: WeakRef<Uint8Array>[] = [];
  const drain = new DrainSignal();
  let firstWrite!: () => void;
  const reached = new Promise<void>((resolve) => {
    firstWrite = resolve;
  });
  const queue = new WriteQueue(
    {
      write(bytes: Uint8Array) {
        writes += 1;
        firstWrite();
        return open ? bytes.length : 0;
      },
      end() {},
    },
    drain,
  );
  let last!: Promise<void>;
  for (let i = 0; i < count; i += 1) {
    const payload = new Uint8Array(bytesEach);
    payload[0] = i & 255;
    weak.push(new WeakRef(payload));
    last = queue.writeFrame(FrameKind.Event, 0, 3, 0, payload);
    void last.then(
      () => {
        settled += 1;
      },
      () => {
        settled += 1;
      },
    );
  }
  await reached;
  await turn();
  Bun.gc(true);
  await turn();
  const retained = weak.reduce((n, ref) => n + (ref.deref() ? 1 : 0), 0);
  results.stalledWriter = {
    attemptedFrames: count,
    acceptedFrames: count,
    completedBeforeRelease: settled,
    retainedPayloadObjects: retained,
    retainedLogicalPayloadBytes: retained * bytesEach,
    candidateTestBudgetBytes: budget,
    exceedsCandidateBudget: retained * bytesEach > budget,
    socketWriteAttemptsBeforeRelease: writes,
    limitation:
      "Injected zero-progress socket proves WriteQueue retention, not native socket buffering or RSS.",
  };
  assert(
    retained * bytesEach > budget,
    "unbounded current WriteQueue must exceed candidate budget",
  );
  open = true;
  drain.fire();
  await last;
  results.stalledWriterCleanup = { completedAfterRelease: settled };
}

// Size refusal happens after TextEncoder has already allocated an oversized string.
{
  const original = TextEncoder.prototype.encode;
  let largestEncoding = 0;
  TextEncoder.prototype.encode = function (value?: string): Uint8Array {
    const result = original.call(this, value);
    largestEncoding = Math.max(largestEncoding, result.byteLength);
    return result;
  };
  let error = "";
  try {
    encodeFsRequest({ variant: "Read", path: "x".repeat(MAX_FRAME_LEN + 1) });
  } catch (err) {
    error = String(err);
  } finally {
    TextEncoder.prototype.encode = original;
  }
  results.encoderAdmission = {
    maxFrameBytes: MAX_FRAME_LEN,
    largestTextEncoderAllocation: largestEncoding,
    error,
  };
  assert(
    largestEncoding > MAX_FRAME_LEN,
    "source must demonstrate oversize allocation before refusal",
  );
}
console.log(JSON.stringify(results, null, 2));
