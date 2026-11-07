/** Exact scoped selector vectors; native admission and physical release are separate owners. */
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import {
  encodeWindowRequest,
  decodeWindowRequest,
  encodeWindowResponse,
  decodeWindowResponse,
  type WindowRequest,
} from "./window-codec.ts";
import { renderWindowCodecBinding } from "../../kipc/scripts/echo-codegen.ts";
const window = { id: 7, incarnation: 258n };
function original(document_generation: number): WindowRequest {
  return {
    variant: "DestroyOriginalDocument",
    window,
    document_generation,
  } as WindowRequest;
}
test("request8 retains the exact original generation independently of same-window successor", () => {
  const a = Uint8Array.of(8, 7, 130, 2, 172, 2),
    b = Uint8Array.of(8, 7, 130, 2, 173, 2);
  expect(encodeWindowRequest(original(300))).toEqual(a);
  expect(decodeWindowRequest(a)).toEqual(original(300));
  expect(encodeWindowRequest(original(301))).toEqual(b);
  expect(decodeWindowRequest(b)).toEqual(original(301));
  expect(a).not.toEqual(b);
  // Generic Destroy and genuine physical result retain their established bytes.
  expect(encodeWindowRequest({ variant: "Destroy", window })).toEqual(
    Uint8Array.of(5, 7, 130, 2),
  );
  expect(encodeWindowResponse({ variant: "Destroyed" })).toEqual(
    Uint8Array.of(7),
  );
  expect(decodeWindowResponse(Uint8Array.of(7))).toEqual({
    variant: "Destroyed",
  });
});
test("original selector is a nonzero exact native u32 and rejects malformed/trailing bytes", () => {
  const max = Uint8Array.of(8, 7, 130, 2, 255, 255, 255, 255, 15);
  expect(encodeWindowRequest(original(0xffffffff))).toEqual(max);
  expect(decodeWindowRequest(max)).toEqual(original(0xffffffff));
  for (const generation of [0, -1, 1.5, 0x100000000, NaN, Infinity])
    expect(() => encodeWindowRequest(original(generation))).toThrow(
      "KELD-IPC-003",
    );
  for (const bytes of [
    [8, 7, 130, 2],
    [8, 7, 130, 2, 0],
    [8, 7, 130, 2, 1, 0],
    [8, 7, 130, 2, 255, 255, 255, 255, 16],
  ])
    expect(() => decodeWindowRequest(Uint8Array.from(bytes))).toThrow(
      "KELD-IPC-003",
    );
});
test("same owning generator binds request8 and refuses selector type, order or ordinal drift", () => {
  const rust = readFileSync(
    new URL("../../../../crates/keld-ipc/src/window.rs", import.meta.url),
    "utf8",
  );
  const adapter = readFileSync(
    new URL("./window-codec.ts", import.meta.url),
    "utf8",
  );
  expect(rust).toContain("    DestroyOriginalDocument {");
  expect(renderWindowCodecBinding(rust, adapter)).toBe(adapter);
  const type = rust.replace(
    /(DestroyOriginalDocument \{[\s\S]*?document_generation:) u32,/,
    "$1 u64,",
  );
  const order = rust.replace(
    /(DestroyOriginalDocument \{[\s\S]*?)window: WindowRef,([\s\S]*?)document_generation: u32,/,
    "$1document_generation: u32,$2window: WindowRef,",
  );
  for (const changed of [
    type,
    order,
    rust.replace("    DestroyOriginalDocument {", "    DestroyOtherDocument {"),
  ])
    expect(() => renderWindowCodecBinding(changed, adapter)).toThrow(
      "echo codegen",
    );
});
