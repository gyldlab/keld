/** Child-only mutations: the independent oracle is hook count plus exact fact. */
import { attachMirror } from "./mirror.ts";
import { WindowSource } from "./window-source.ts";
import { encodeWindowEvent, type WindowEvent } from "./window-codec.ts";
import {
  WorkerLink,
  linkError,
  encodeCallError,
  decodeCallError,
} from "../../kipc/src/transport.ts";
const mode = process.argv[2];
let state: (bytes: Uint8Array) => void = () => {
  throw new Error("missing applier");
};
let dispatch: () => void = () => {
  throw new Error("missing dispatcher");
};
let fact: WindowEvent | undefined;
const link = {
  stateFactCapacity: 2,
  setStateApplier(_channel: number, callback: typeof state) {
    state = callback;
  },
  onEvent(_channel: number, callback: () => void) {
    dispatch = callback;
  },
} as unknown as WorkerLink;
const source = new WindowSource((value) => {
  fact = value;
});

const bytes = encodeWindowEvent({
  variant: "Created",
  window: { id: 7, incarnation: 300n },
  state: {
    bounds: { x: -3, y: 4, width: 640, height: 480 },
    focused: true,
    maximized: false,
    full_screen: true,
  },
  pending_close: null,
});
const errMode = mode.startsWith("err-");
const errBytes = errMode ? encodeCallError("KELD-CORE-045", "denied") : bytes;
let decodedError: { code: string; message: string } | undefined;
const failing = mode.startsWith("failure-");
if (failing) bytes[0] = 0xff;
let hits = 0;
if (mode === "lifecycle-length") {
  const { mock } = await import("bun:test");
  const canonical = await import("../../kipc/src/transport.ts");
  mock.module("../../kipc/src/transport.ts", () => ({
    ...canonical,
    WorkerLink: {
      open: async () => ({
        ...link,
        setCallHandler() {},
        onEnd() {},
        close() {},
      }),
    },
  }));
  process.env.KELD_NATIVE_WINDOW_FACADE = "0";
  const { LifecycleLink } = await import("./link.ts");
  const session = await LifecycleLink.connect("controlled", {
    onReady() {},
    onLastWindowClosed() {},
    onLinkDead() {},
  });
  const prototype = Object.getPrototypeOf(Uint8Array.prototype);
  const descriptor = Object.getOwnPropertyDescriptor(prototype, "length")!;
  Object.defineProperty(prototype, "length", {
    ...descriptor,
    get() {
      hits += 1;
      return Reflect.apply(descriptor.get!, this, []);
    },
  });
  try {
    state(Uint8Array.of(0));
    state(Uint8Array.of(1));
  } finally {
    Object.defineProperty(prototype, "length", descriptor);
  }
  if (hits !== 0 || !session.consumeAppliedLastWindowClosed())
    throw new Error(`lifecycle state getter calls=${hits}`);
  console.log(JSON.stringify({ mode, hits, variant: "Created" }));
  process.exit(0);
}
if (mode === "view-custody") {
  const nativeUint8 = Uint8Array;
  const nativeInt32 = Int32Array;
  const sab = new SharedArrayBuffer(64 + 4096 + 65536);
  globalThis.Uint8Array = new Proxy(nativeUint8, {
    construct(target, args, newTarget) {
      if (args[0] === sab) hits += 1;
      return Reflect.construct(target, args, newTarget);
    },
  });
  globalThis.Int32Array = new Proxy(nativeInt32, {
    construct(target, args, newTarget) {
      if (args[0] === sab) hits += 1;
      return Reflect.construct(target, args, newTarget);
    },
  });
  // Only the real constructor's view ownership is exercised. The inert worker
  // supplies registration, and is never claimed as Worker/process acceptance.
  const Constructor = WorkerLink as unknown as new (
    ...args: unknown[]
  ) => WorkerLink;
  const owned = new Constructor(
    sab,
    { replyBytes: 4096, ringBytes: 65536, ringRecords: 2, table: undefined },
    { on() {} },
    undefined,
    undefined,
  );
  globalThis.Uint8Array = nativeUint8;
  globalThis.Int32Array = nativeInt32;
  if (hits !== 0 || owned.stateFactCapacity !== 2)
    throw new Error(`private shared views escaped: ${hits}`);
  console.log(JSON.stringify({ mode, hits, variant: "Created" }));
  process.exit(0);
}
if (
  mode === "escape-values" ||
  mode === "escape-clear" ||
  mode === "escape-iterator"
) {
  attachMirror(link, source);
  state(bytes);
  dispatch();
  let escaped: Map<number, unknown> | undefined;
  const originalValues = Map.prototype.values;
  const originalClear = Map.prototype.clear;
  if (mode === "escape-values") {
    Map.prototype.values = function () {
      escaped = this;
      return Reflect.apply(originalValues, this, []);
    };
    try {
      source.all();
    } finally {
      Map.prototype.values = originalValues;
    }
    if (escaped !== undefined) {
      const prior = source.get({ id: 7, incarnation: 300n })!;
      escaped.set(7, {
        ...prior,
        window: {
          id: 7,
          get incarnation() {
            hits += 1;
            return 300n;
          },
        },
      });
    }
    state(
      encodeWindowEvent({
        variant: "Focus",
        window: { id: 7, incarnation: 300n },
        focused: false,
      }),
    );
    dispatch();
  } else if (mode === "escape-iterator") {
    const prototype = Object.getPrototypeOf(new Map().values());
    const next = prototype.next;
    prototype.next = function () {
      hits += 1;
      return Reflect.apply(next, this, []);
    };
    try {
      source.all();
    } finally {
      prototype.next = next;
    }
  } else {
    Map.prototype.clear = function () {
      escaped = this;
      return Reflect.apply(originalClear, this, []);
    };
    try {
      source.end(linkError("KELD-IPC-022", "after drain"));
    } finally {
      Map.prototype.clear = originalClear;
    }
  }
  if (escaped !== undefined || hits !== 0)
    throw new Error(
      `private Source Map escaped: ${mode}; getter calls=${hits}`,
    );
  console.log(JSON.stringify({ mode, hits, variant: "Created" }));
  process.exit(0);
}
const hook = () => {
  hits += 1;
};
let restore: () => void;
if (mode === "push") {
  const original = Array.prototype.push;
  Array.prototype.push = function <T>(this: T[], ...values: T[]) {
    hook();
    return Reflect.apply(original, this, values);
  };
  restore = () => {
    Array.prototype.push = original;
  };
} else if (mode === "freeze") {
  const original = Object.freeze;
  Object.freeze = ((value: object) => {
    hook();
    return original(value);
  }) as typeof Object.freeze;
  restore = () => {
    Object.freeze = original;
  };
} else if (mode === "get") {
  const original = Map.prototype.get;
  Map.prototype.get = function (key: unknown) {
    hook();
    return Reflect.apply(original, this, [key]);
  };
  restore = () => {
    Map.prototype.get = original;
  };
} else if (mode === "iterator" || mode === "err-iterator") {
  const original = Array.prototype[Symbol.iterator];
  Array.prototype[Symbol.iterator] = function () {
    hook();
    return Reflect.apply(original, this, []);
  };
  restore = () => {
    Array.prototype[Symbol.iterator] = original;
  };
} else if (mode === "integer") {
  const original = Number.isInteger;
  Number.isInteger = (value: unknown) => {
    hook();
    return original(value);
  };
  restore = () => {
    Number.isInteger = original;
  };
} else if (mode === "bigint") {
  const original = BigInt;
  globalThis.BigInt = ((value: string | number | bigint | boolean) => {
    hook();
    return original(value);
  }) as BigIntConstructor;
  restore = () => {
    globalThis.BigInt = original;
  };
} else if (mode === "byteLength") {
  const proto = Object.getPrototypeOf(Uint8Array.prototype);
  const original = Object.getOwnPropertyDescriptor(proto, "byteLength")!;
  Object.defineProperty(proto, "byteLength", {
    ...original,
    get() {
      hook();
      return Reflect.apply(original.get!, this, []);
    },
  });
  restore = () => {
    Object.defineProperty(proto, "byteLength", original);
  };
} else if (mode === "brand") {
  Object.defineProperty(Uint8Array, Symbol.hasInstance, {
    configurable: true,
    value(value: unknown) {
      hook();
      return Reflect.apply(Function.prototype[Symbol.hasInstance], this, [
        value,
      ]);
    },
  });
  restore = () => {
    delete (Uint8Array as unknown as Record<symbol, unknown>)[
      Symbol.hasInstance
    ];
  };
} else if (mode === "failure-error") {
  const original = Error;
  globalThis.Error = new Proxy(original, {
    construct(target, args, newTarget) {
      hook();
      return Reflect.construct(target, args, newTarget);
    },
  });
  restore = () => {
    globalThis.Error = original;
  };
} else if (mode === "failure-define") {
  const original = Object.defineProperty;
  Object.defineProperty = ((
    ...args: Parameters<typeof Object.defineProperty>
  ) => {
    hook();
    return Reflect.apply(original, Object, args);
  }) as typeof Object.defineProperty;
  restore = () => {
    Object.defineProperty = original;
  };
} else if (mode === "failure-startsWith" || mode === "err-startsWith") {
  const original = String.prototype.startsWith;
  String.prototype.startsWith = function (...args) {
    hook();
    return Reflect.apply(original, this, args);
  };
  restore = () => {
    String.prototype.startsWith = original;
  };
} else if (mode === "err-subarray") {
  const original = Uint8Array.prototype.subarray;
  Uint8Array.prototype.subarray = function (...args) {
    hook();
    return Reflect.apply(original, this, args);
  };
  restore = () => {
    Uint8Array.prototype.subarray = original;
  };
} else if (mode === "err-length") {
  const prototype = Object.getPrototypeOf(Uint8Array.prototype);
  const descriptor = Object.getOwnPropertyDescriptor(prototype, "length")!;
  Object.defineProperty(prototype, "length", {
    ...descriptor,
    get() {
      hook();
      return Reflect.apply(descriptor.get!, this, []);
    },
  });
  restore = () => {
    Object.defineProperty(prototype, "length", descriptor);
  };
} else if (mode === "err-textdecoder") {
  const original = TextDecoder;
  globalThis.TextDecoder = new Proxy(original, {
    construct(target, args, newTarget) {
      hook();
      return Reflect.construct(target, args, newTarget);
    },
  });
  restore = () => {
    globalThis.TextDecoder = original;
  };
} else if (mode === "err-decode") {
  const original = TextDecoder.prototype.decode;
  TextDecoder.prototype.decode = function (...args) {
    hook();
    return Reflect.apply(original, this, args);
  };
  restore = () => {
    TextDecoder.prototype.decode = original;
  };
} else if (mode === "invocation") {
  const call = Function.prototype.call;
  const apply = Function.prototype.apply;
  const bind = Function.prototype.bind;
  Function.prototype.call = function (...args: unknown[]) {
    hook();
    return Reflect.apply(call, this, args);
  };
  Function.prototype.apply = function (...args: unknown[]) {
    hook();
    return Reflect.apply(apply, this, args);
  };
  Function.prototype.bind = function (...args: unknown[]) {
    hook();
    return Reflect.apply(bind, this, args);
  } as typeof Function.prototype.bind;
  restore = () => {
    Function.prototype.call = call;
    Function.prototype.apply = apply;
    Function.prototype.bind = bind;
  };
} else if (mode === "inherited-slot") {
  const original = Object.getOwnPropertyDescriptor(Array.prototype, "0");
  Object.defineProperty(Array.prototype, "0", {
    configurable: true,
    set() {
      hook();
    },
  });
  restore = () => {
    if (original) Object.defineProperty(Array.prototype, "0", original);
    else delete Array.prototype[0];
  };
} else throw new Error("unknown mutation");
let error: unknown;
try {
  if (errMode) decodedError = decodeCallError(errBytes);
  else {
    attachMirror(link, source);
    state(bytes);
  }
} catch (caught) {
  error = caught;
} finally {
  restore();
}
if (errMode) {
  if (error !== undefined) throw error;
  if (
    hits !== 0 ||
    decodedError?.code !== "KELD-CORE-045" ||
    decodedError.message !== "denied"
  )
    throw new Error(JSON.stringify({ mode, hits, decodedError }));
  console.log(JSON.stringify({ mode, hits, variant: "Created" }));
  process.exit(0);
}
if (failing) {
  if (
    hits !== 0 ||
    !(error instanceof Error) ||
    !error.message.startsWith("KELD-IPC-005:")
  )
    throw new Error(JSON.stringify({ mode, hits, error }));
  console.log(JSON.stringify({ mode, hits, variant: "rejected" }));
  process.exit(0);
}
dispatch();
if (error !== undefined) throw error;
if (
  hits !== 0 ||
  fact?.variant !== "Created" ||
  fact.window.id !== 7 ||
  fact.state.bounds.width !== 640
)
  throw new Error(JSON.stringify({ mode, hits, fact }));
console.log(JSON.stringify({ mode, hits, variant: fact.variant }));
