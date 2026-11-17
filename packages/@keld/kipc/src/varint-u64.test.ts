import { describe, expect, test } from "bun:test";
import * as transport from "./transport.ts";

function encodeU64(value: bigint): Uint8Array {
  const encoder: unknown = Reflect.get(transport, "encodeU64Varint");
  expect(typeof encoder).toBe("function");
  return (encoder as (input: bigint) => Uint8Array)(value);
}

function expectCodecError(action: () => unknown): void {
  let failure: unknown;
  try {
    action();
  } catch (error) {
    failure = error;
  }
  expect(failure).toBeInstanceOf(Error);
  expect((failure as Error).message.startsWith("KELD-IPC-003:")).toBe(true);
}

describe("exact unsigned varint encoding", () => {
  test("exports the exact u64 encoder", () => {
    expect(typeof Reflect.get(transport, "encodeU64Varint")).toBe("function");
  });

  test("pins literal u64 bytes including values beyond number precision", () => {
    const cases: readonly (readonly [bigint, readonly number[]])[] = [
      [0n, [0]],
      [1n, [1]],
      [127n, [0x7f]],
      [128n, [0x80, 1]],
      [300n, [0xac, 2]],
      [
        9_007_199_254_740_993n,
        [0x81, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x10],
      ],
      [
        18_446_744_073_709_551_615n,
        [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1],
      ],
    ];
    for (const [value, expected] of cases) {
      const bytes = encodeU64(value);
      expect(bytes).toEqual(new Uint8Array(expected));
      expect(transport.decodeU64Varint(bytes, 0)).toEqual([
        value,
        expected.length,
      ]);
    }
  });

  test("rejects out-of-range and non-bigint u64 inputs without coercion", () => {
    let coerced = false;
    const object = {
      [Symbol.toPrimitive](): never {
        coerced = true;
        throw new Error("invalid input must never be coerced");
      },
    };
    for (const value of [
      -1n,
      18_446_744_073_709_551_616n,
      0,
      1,
      0.5,
      Number.NaN,
      Number.POSITIVE_INFINITY,
      "1",
      true,
      null,
      undefined,
      Symbol("invalid"),
      object,
    ]) {
      expectCodecError(() => encodeU64(value as bigint));
    }
    expect(coerced).toBe(false);
  });

  test("u32 malformed scalar errors do not coerce rejected inputs", () => {
    let coerced = false;
    const object = {
      toString(): never {
        coerced = true;
        throw new Error("invalid input must never be coerced");
      },
    };
    for (const value of [Symbol("invalid"), object]) {
      expectCodecError(() =>
        transport.encodeVarint(value as unknown as number),
      );
    }
    expect(coerced).toBe(false);
  });
});
