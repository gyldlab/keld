import { expect, test } from "bun:test";
import * as codec from "./endpoint-value-codec.ts";

function accessorIndex(
  prototype: object,
  key: string,
  write: PropertyKey,
): void {
  Object.defineProperty(prototype, key, {
    configurable: true,
    set(this: Record<PropertyKey, unknown>, value: unknown) {
      // Redirect the store instead of dropping it: a swallowed write and a
      // moved write both corrupt a value that trusted [[Set]].
      this[write] = value;
    },
  });
}

function withoutIndex(prototype: object, key: string, run: () => void): void {
  accessorIndex(prototype, key, "redirected");
  try {
    run();
  } finally {
    delete (prototype as Record<string, unknown>)[key];
  }
}

function refusal(run: () => unknown, code: string): void {
  let caught: unknown;
  try {
    run();
  } catch (error) {
    caught = error;
  }
  expect(caught).toBeInstanceOf(codec.CompatCodecError);
  expect((caught as codec.CompatCodecError).code).toBe(code);
}

test("refusal checks the error from exactly one invocation", () => {
  let calls = 0;
  const typed = new codec.CompatCodecError("MalformedValue", "test refusal");
  refusal(() => {
    calls += 1;
    throw typed;
  }, "KELD-COMPAT-101");
  expect(calls).toBe(1);
  expect(() => refusal(() => undefined, "KELD-COMPAT-101")).toThrow();

  calls = 0;
  expect(() =>
    refusal(() => {
      calls += 1;
      if (calls === 1) throw new Error("KELD-COMPAT-101");
      throw typed;
    }, "KELD-COMPAT-101"),
  ).toThrow();
  expect(calls).toBe(1);
});

test("an inherited array index setter installed by a trap cannot change the wire", () => {
  let installed = false;
  const target = { a: undefined };
  const value = new Proxy(target, {
    ownKeys(trapped) {
      installed = true;
      accessorIndex(Array.prototype, "6", "7");
      return Reflect.ownKeys(trapped);
    },
  });
  try {
    expect(Array.from(codec.encodeEndpointValue(value))).toEqual([
      7, 1, 0, 1, 0, 97, 0,
    ]);
  } finally {
    delete (Array.prototype as unknown as Record<string, unknown>)["6"];
  }
  expect(installed).toBe(true);
});

test("an inherited array index setter during decode cannot drop an element", () => {
  const wire = codec.encodeEndpointValue([false]);
  withoutIndex(Array.prototype, "0", () => {
    const decoded = codec.decodeEndpointValue(wire) as unknown[];
    expect(Array.isArray(decoded)).toBe(true);
    expect(decoded.length).toBe(1);
    expect(Object.hasOwn(decoded, "0")).toBe(true);
    expect(decoded[0]).toBe(false);
  });
});

test("a drifting proxy length cannot authorize a seventeenth argument", () => {
  const target: unknown[] = Array(16).fill(null);
  let lengthReads = 0;
  const args = new Proxy(target, {
    getOwnPropertyDescriptor(trapped, key) {
      if (key === "length") {
        lengthReads += 1;
        // Growing the target after the first answer used to let the sender
        // serialize 17 arguments that the receiver then refused.
        if (lengthReads > 1) trapped.push(null);
      }
      return Reflect.getOwnPropertyDescriptor(trapped, key);
    },
  });
  const wire = codec.encodeEndpointArguments(args);
  expect(wire[1]).toBe(16);
  expect(wire[2]).toBe(0);
  expect(codec.decodeEndpointArguments(wire).length).toBe(16);
});

test("a stable argument vector keeps its reviewed boundary", () => {
  expect(
    codec.decodeEndpointArguments(
      codec.encodeEndpointArguments(Array(16).fill(null)),
    ).length,
  ).toBe(16);
  refusal(
    () => codec.encodeEndpointArguments(Array(17).fill(null)),
    "KELD-COMPAT-102",
  );
  refusal(
    () => codec.encodeEndpointArguments({ 0: null, length: 1 } as never),
    "KELD-COMPAT-101",
  );
});

test("native values with a replaced prototype still refuse", () => {
  const natives: [string, object][] = [
    ["Map", new Map([[1, 2]])],
    ["Set", new Set([1])],
    ["WeakMap", new WeakMap()],
    ["WeakSet", new WeakSet()],
    ["ArrayBuffer", new ArrayBuffer(4)],
  ];
  for (const [label, value] of natives) {
    Object.setPrototypeOf(value, null);
    refusal(() => codec.encodeEndpointValue(value), "KELD-COMPAT-100");
    expect(label).toBeTruthy();
  }
  const date = Object.setPrototypeOf(new Date(1_700_000_000_000), null);
  const decoded = codec.decodeEndpointValue(codec.encodeEndpointValue(date));
  expect(decoded).toBeInstanceOf(Date);
  expect((decoded as Date).getTime()).toBe(1_700_000_000_000);
});

test("supported values keep round-tripping exactly after the brand change", () => {
  const value = {
    date: new Date(1_700_000_000_000),
    error: new Error("boom"),
    list: [null, true, 1.5, "text", [undefined]],
  };
  const decoded = codec.decodeEndpointValue(
    codec.encodeEndpointValue(value),
  ) as typeof value;
  expect(decoded.date.getTime()).toBe(1_700_000_000_000);
  expect(decoded.error).toBeInstanceOf(Error);
  expect(decoded.error.message).toBe("boom");
  expect(decoded.list).toEqual([null, true, 1.5, "text", [undefined]]);
});

test("a proxied TypedArray cannot turn a native slot into record data", () => {
  refusal(
    () => codec.encodeEndpointValue(new Proxy(new Uint8Array([1, 2]), {})),
    "KELD-COMPAT-100",
  );
  refusal(
    () => codec.encodeEndpointValue(new Uint8Array([1, 2])),
    "KELD-COMPAT-100",
  );
});

test("a forged codec-error prototype cannot escape the typed contract", () => {
  const forged = new Error("RAW ERROR");
  Object.setPrototypeOf(forged, codec.CompatCodecError.prototype);
  refusal(
    () =>
      codec.encodeEndpointValue(
        new Proxy(
          {},
          {
            getPrototypeOf() {
              throw forged;
            },
          },
        ),
      ),
    "KELD-COMPAT-101",
  );
  refusal(
    () =>
      codec.encodeEndpointValue(
        new Proxy(
          {},
          {
            getPrototypeOf() {
              throw new Proxy(
                {},
                {
                  getPrototypeOf() {
                    throw new RangeError("unwrapped classifier");
                  },
                },
              );
            },
          },
        ),
      ),
    "KELD-COMPAT-101",
  );
});

test("genuine refusals keep their own kind and code", () => {
  refusal(() => codec.encodeEndpointValue(() => null), "KELD-COMPAT-100");
  refusal(() => codec.encodeEndpointValue("\ud800"), "KELD-COMPAT-101");
  refusal(
    () => codec.decodeEndpointValue(Uint8Array.from([0, 0])),
    "KELD-COMPAT-101",
  );
});

test("a revoked argument proxy is a typed refusal, not a raw TypeError", () => {
  const revoked = Proxy.revocable([], {});
  revoked.revoke();
  refusal(
    () => codec.encodeEndpointArguments(revoked.proxy as unknown[]),
    "KELD-COMPAT-101",
  );
  refusal(
    () => codec.decodeEndpointValue(revoked.proxy as unknown as Uint8Array),
    "KELD-COMPAT-101",
  );
});

test("individually small arrays cannot amplify descriptor work without bound", () => {
  let probes = 0;
  const blocks: unknown[] = [];
  for (let block = 0; block < 80; block += 1) {
    const inner = [null];
    for (let index = 0; index < 250; index += 1)
      Object.defineProperty(inner, `p${index}`, {
        value: index,
        enumerable: false,
      });
    blocks.push(
      new Proxy(inner, {
        getOwnPropertyDescriptor(trapped, key) {
          probes += 1;
          return Reflect.getOwnPropertyDescriptor(trapped, key);
        },
      }),
    );
  }
  refusal(() => codec.encodeEndpointValue(blocks), "KELD-COMPAT-102");
  // The refusal is deterministic and early: previously this same value
  // encoded in 323 bytes after 20,240 descriptor inspections.
  expect(probes).toBeLessThan(2_000);
});

test("one array near the permitted boundary still encodes, and decodes", () => {
  // 40 elements plus 200 non-enumerable own keys stay inside the reviewed
  // 257 own-key domain and inside the probe budget.
  const value = Array(40).fill(null);
  for (let index = 0; index < 200; index += 1)
    Object.defineProperty(value, `p${index}`, {
      value: index,
      enumerable: false,
    });
  const decoded = codec.decodeEndpointValue(
    codec.encodeEndpointValue([value]),
  ) as unknown[][];
  expect(decoded).toEqual([value]);
});
