// Private bounded tool protocol, not a package/boot/provenance format.
export const INPUT_MAGIC = new TextEncoder().encode("KELDCIN3");
export const OUTPUT_MAGIC = new TextEncoder().encode("KELDCOT2");
export const MAX_SOURCE = 16 * 1024 * 1024;
export const MAX_APP = 256 * 1024 * 1024;
export const MAX_REQUEST = MAX_APP + 4096 * 108 + 120;
export const MAX_RESPONSE = MAX_APP + MAX_SOURCE + 48;

export function decodeInputs(bytes: Uint8Array): {
  entry: string;
  modules: Map<string, Uint8Array>;
  renderer: Uint8Array;
} {
  if (bytes.byteLength > MAX_REQUEST) throw new Error("Compiler input bound");
  let offset = 0;
  function take(count: number): Uint8Array {
    if (count > bytes.length - offset)
      throw new Error("Truncated compiler input");
    const result = bytes.subarray(offset, offset + count);
    offset += count;
    return result;
  }
  function number(): number {
    const b = take(4);
    return new DataView(b.buffer, b.byteOffset, 4).getUint32(0, true);
  }
  if (!take(8).every((byte, i) => byte === INPUT_MAGIC[i]))
    throw new Error("Compiler input version");
  const entryLength = number();
  if (entryLength === 0 || entryLength > 100)
    throw new Error("Compiler entry bound");
  const entry = new TextDecoder("utf-8", { fatal: true }).decode(
    take(entryLength),
  );
  const count = number();
  if (count === 0 || count > 4096) throw new Error("Compiler member count");
  const files = new Map<string, Uint8Array>();
  let total = 0;
  for (let i = 0; i < count; i++) {
    const nameLength = number();
    if (nameLength === 0 || nameLength > 100)
      throw new Error("Compiler name bound");
    const name = new TextDecoder("utf-8", { fatal: true }).decode(
      take(nameLength),
    );
    if (files.has(name)) throw new Error("Duplicate compiler member");
    const length = number();
    total += length;
    if (length > MAX_SOURCE || total > MAX_APP)
      throw new Error("Compiler source bound");
    files.set(name, take(length));
  }
  const rendererLength = number();
  if (
    rendererLength === 0 ||
    rendererLength > MAX_SOURCE ||
    total + rendererLength > MAX_APP
  )
    throw new Error("Renderer input bound");
  const renderer = take(rendererLength);
  if (offset !== bytes.length) throw new Error("Trailing compiler input");
  if (!files.has(entry))
    throw new Error("Declared entry absent from captured inventory");
  return { entry, modules: files, renderer };
}

export function encodeOutputs(
  main: Uint8Array,
  transport: Uint8Array,
  rendererSha256: Uint8Array,
): Uint8Array {
  if (rendererSha256.length !== 32) throw new Error("Renderer digest width");
  if (main.length > MAX_APP || transport.length > MAX_SOURCE)
    throw new Error("Compiler result bound");
  const result = new Uint8Array(48 + main.length + transport.length);
  result.set(OUTPUT_MAGIC);
  result.set(rendererSha256, 8);
  new DataView(result.buffer).setUint32(40, main.length, true);
  result.set(main, 44);
  new DataView(result.buffer).setUint32(
    44 + main.length,
    transport.length,
    true,
  );
  result.set(transport, 48 + main.length);
  return result;
}

export async function readBoundedInput(
  stream: ReadableStream<Uint8Array>,
): Promise<Uint8Array> {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      total += value.length;
      if (total > MAX_REQUEST) {
        await reader.cancel();
        throw new Error("Compiler input bound");
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const result = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}
