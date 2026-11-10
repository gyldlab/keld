/** Child-only mutation: desktop decode/apply must not enter application hooks. */
import assert from "node:assert/strict";
import { stateApply } from "@keld/kipc";

const mode = process.argv[2];
const define = Object.defineProperty;
const descriptor = Object.getOwnPropertyDescriptor;
const apply = Reflect.apply;
let armed = false;
let hits = 0;
let restore = () => {};
function replaceMethod(owner: object, name: PropertyKey): void {
  const original = descriptor(owner, name);
  assert.ok(original && typeof original.value === "function");
  const target = original.value as (...args: unknown[]) => unknown;
  define(owner, name, {
    ...original,
    value: function (this: unknown, ...args: unknown[]) {
      if (armed) hits += 1;
      return apply(target, this, args);
    },
  });
  restore = () => define(owner, name, original);
}
function replaceGetter(owner: object, name: PropertyKey): void {
  const original = descriptor(owner, name);
  assert.ok(original?.get);
  const target = original.get;
  define(owner, name, {
    ...original,
    get(this: unknown) {
      if (armed) hits += 1;
      return apply(target, this, []);
    },
  });
  restore = () => define(owner, name, original);
}

if (mode === "early-data-view") {
  const Original = DataView;
  const original = descriptor(globalThis, "DataView")!;
  define(globalThis, "DataView", {
    ...original,
    value: function DataViewHook(
      buffer: ArrayBufferLike,
      offset?: number,
      length?: number,
    ) {
      if (armed) hits += 1;
      return new Original(buffer, offset, length);
    },
  });
  restore = () => define(globalThis, "DataView", original);
}
const { WindowSource } = await import("./window-source.ts");
const { attachMirror } = await import("./mirror.ts");
let state: (bytes: Uint8Array) => void = () => {
  throw new Error("missing applier");
};
let dispatch = () => {
  throw new Error("missing dispatch");
};
let callbacks = 0;
const source = new WindowSource(() => {
  callbacks += 1;
});
attachMirror(
  {
    stateFactCapacity: 2,
    setStateApplier(_channel: number, callback: typeof state) {
      state = callback;
    },
    onEvent(_channel: number, callback: typeof dispatch) {
      dispatch = callback;
    },
  } as unknown as Parameters<typeof attachMirror>[0],
  source,
);
// Independent Rust-owned literal: primary17, 1440x900, work y24 h876, scale2.
const bytes = Uint8Array.from(
  Buffer.from(
    "07ac02011101110000a00b84070030a00bec06000000000000004000",
    "hex",
  ),
);
const typedPrototype = Object.getPrototypeOf(Uint8Array.prototype);
switch (mode) {
  case "control":
  case "early-data-view":
    break;
  case "map":
    replaceMethod(Array.prototype, "map");
    break;
  case "push":
    replaceMethod(Array.prototype, "push");
    break;
  case "iterator":
    replaceMethod(Array.prototype, Symbol.iterator);
    break;
  case "array-brand":
    replaceMethod(Array, "isArray");
    break;
  case "integer":
    replaceMethod(Number, "isInteger");
    break;
  case "finite":
    replaceMethod(Number, "isFinite");
    break;
  case "set-has":
    replaceMethod(Set.prototype, "has");
    break;
  case "set-add":
    replaceMethod(Set.prototype, "add");
    break;
  case "float-read":
    replaceMethod(DataView.prototype, "getFloat64");
    break;
  case "buffer":
    replaceGetter(typedPrototype, "buffer");
    break;
  case "offset":
    replaceGetter(typedPrototype, "byteOffset");
    break;
  default:
    throw new Error("unrecognized desktop mutation mode");
}
try {
  armed = true;
  state(bytes);
} finally {
  armed = false;
  restore();
}
assert.equal(
  hits,
  0,
  "desktop state invoked an application-replaceable intrinsic",
);
assert.equal(callbacks, 0, "state phase dispatched application callbacks");
const observed = source.desktop();
assert.equal(observed?.primary_id, 17);
assert.deepEqual(observed?.displays[0]?.work_area, {
  x: 0,
  y: 24,
  width: 1440,
  height: 876,
});
assert.equal(observed?.displays[0]?.scale_factor, 2);
dispatch();
assert.equal(callbacks, 1);
assert.equal(
  typeof stateApply,
  "function",
  "the same preloaded provider remains present",
);
console.log(
  JSON.stringify({ mode, hits, callbacks, primary: observed.primary_id }),
);
