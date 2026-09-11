/** Reproducible fragmented-frame scaling receipt for KEL-136. */
import { FrameKind, FrameReader, encodeHeader } from "../src/transport.ts";

const CHUNK_BYTES = 256;
const SAMPLE_COUNT = 3;
const MIB = 1024 * 1024;

type Sample = {
  payloadBytes: number;
  fragmentCount: number;
  elapsedMs: number;
};

async function measure(payloadBytes: number): Promise<Sample> {
  const reader = new FrameReader();
  const pending = reader.readFrame();
  reader.push(encodeHeader(FrameKind.Reply, 0, 1, 1, payloadBytes));
  const fullChunk = new Uint8Array(CHUNK_BYTES).fill(0x5a);
  const started = performance.now();
  for (let offset = 0; offset < payloadBytes; offset += CHUNK_BYTES) {
    const remaining = payloadBytes - offset;
    reader.push(remaining >= CHUNK_BYTES ? fullChunk : fullChunk.subarray(0, remaining));
  }
  const frame = await pending;
  const elapsedMs = performance.now() - started;
  if (frame.payload.byteLength !== payloadBytes || reader.bufferedBytes() !== 0) {
    throw new Error("KELD-IPC-005: fragmented benchmark did not consume exactly one frame");
  }
  return {
    payloadBytes,
    fragmentCount: Math.ceil(payloadBytes / CHUNK_BYTES),
    elapsedMs,
  };
}

function median(samples: readonly Sample[]): number {
  const ordered = samples.map((sample) => sample.elapsedMs).sort((a, b) => a - b);
  return ordered[Math.floor(ordered.length / 2)]!;
}

await measure(MIB);
const groups = [];
for (const payloadBytes of [8 * MIB, 16 * MIB]) {
  const samples: Sample[] = [];
  for (let sample = 0; sample < SAMPLE_COUNT; sample += 1) {
    samples.push(await measure(payloadBytes));
  }
  groups.push({
    payloadBytes,
    fragmentCount: samples[0]!.fragmentCount,
    elapsedMs: samples.map((sample) => sample.elapsedMs),
    medianMs: median(samples),
  });
}

console.log(
  JSON.stringify(
    {
      schema: "keld.kipc.fragmented-frame-benchmark/v1",
      runtime: { name: "bun", version: Bun.version, revision: Bun.revision },
      platform: { os: process.platform, arch: process.arch },
      chunkBytes: CHUNK_BYTES,
      sampleCount: SAMPLE_COUNT,
      groups,
      medianScalingRatio: groups[1]!.medianMs / groups[0]!.medianMs,
    },
    null,
    2,
  ),
);
