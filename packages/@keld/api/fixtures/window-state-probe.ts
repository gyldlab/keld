const invoke = Reflect.apply;
let restoreRegistration: () => void = () => {};
let applierChannel: number | undefined;
const push = Array.prototype.push;
const get = Map.prototype.get;
const set = Map.prototype.set;
const NativeMap = Map;
const NativeUint8 = Uint8Array;
const NativeInt32 = Int32Array;
const NativeShared = SharedArrayBuffer;
const typedPrototype = Object.getPrototypeOf(Uint8Array.prototype);
const lengthDescriptor = Object.getOwnPropertyDescriptor(
  typedPrototype,
  "length",
)!;
let sharedCustody = 0;
const constructed: Map<unknown, unknown>[] = [];
let custody = 0;
const freeze = Object.freeze;
let applying = false;
let waking = false;
let calls = 0;
let hooks = 0;
export function observeStateAppliers(
  Link: {
    prototype: {
      setStateApplier(
        channel: number,
        applier: (payload: Uint8Array) => void,
      ): void;
      callBlocking?(
        channel: number,
        payload: Uint8Array,
        deadline: number,
      ): Uint8Array;
    };
  },
  channel: number,
  kinds?: { readonly Event: number; readonly Err: number },
) {
  applierChannel = channel;
  const register = Link.prototype.setStateApplier;
  Link.prototype.setStateApplier = function (channel, applier) {
    return invoke(register, this, [
      channel,
      (payload: Uint8Array) => {
        applying = true;
        calls += 1;
        try {
          applier(payload);
        } finally {
          applying = false;
        }
      },
    ]);
  };
  const blocking = Link.prototype.callBlocking;
  if (blocking)
    Link.prototype.callBlocking = function (...args) {
      waking = true;
      try {
        return invoke(blocking, this, args);
      } finally {
        waking = false;
      }
    };
  const restoreKinds: (() => void)[] = [];
  if (kinds)
    for (const key of ["Event", "Err"] as const) {
      const descriptor = Object.getOwnPropertyDescriptor(kinds, key)!;
      const value = kinds[key];
      Object.defineProperty(kinds, key, {
        configurable: true,
        enumerable: true,
        get() {
          if (waking || applying) hooks += 1;
          return value;
        },
      });
      restoreKinds.push(() => {
        Object.defineProperty(kinds, key, descriptor);
      });
    }
  restoreRegistration = () => {
    Link.prototype.setStateApplier = register;
    if (blocking) Link.prototype.callBlocking = blocking;
    for (const restore of restoreKinds) restore();
  };
}
globalThis.Uint8Array = new Proxy(NativeUint8, {
  construct(target, args, newTarget) {
    if (args[0] instanceof NativeShared) sharedCustody += 1;
    return Reflect.construct(target, args, newTarget);
  },
});
globalThis.Int32Array = new Proxy(NativeInt32, {
  construct(target, args, newTarget) {
    if (args[0] instanceof NativeShared) sharedCustody += 1;
    return Reflect.construct(target, args, newTarget);
  },
});
globalThis.SharedArrayBuffer = new Proxy(NativeShared, {
  construct(target, args, newTarget) {
    sharedCustody += 1;
    return Reflect.construct(target, args, newTarget);
  },
});
globalThis.Map = new Proxy(NativeMap, {
  construct(target, args, newTarget) {
    const map = Reflect.construct(target, args, newTarget);
    invoke(push, constructed, [map]);
    return map;
  },
});
Map.prototype.set = function (key, value) {
  if (key === applierChannel && typeof value === "function") custody += 1;
  return invoke(set, this, [key, value]);
};
Object.defineProperty(typedPrototype, "length", {
  ...lengthDescriptor,
  get() {
    if (applying) hooks += 1;
    return invoke(lengthDescriptor.get!, this, []);
  },
});
Array.prototype.push = function (...values) {
  if (applying) hooks += 1;
  return invoke(push, this, values);
};
Map.prototype.get = function (key) {
  if (applying) hooks += 1;
  return invoke(get, this, [key]);
};
Object.freeze = function (value) {
  if (applying) hooks += 1;
  return freeze(value);
};
export function assertStatePhase() {
  restoreRegistration();
  for (const map of constructed)
    if (typeof invoke(get, map, [applierChannel]) === "function") custody += 1;
  globalThis.Map = NativeMap;
  globalThis.Uint8Array = NativeUint8;
  globalThis.Int32Array = NativeInt32;
  globalThis.SharedArrayBuffer = NativeShared;
  Map.prototype.set = set;
  Array.prototype.push = push;
  Map.prototype.get = get;
  Object.freeze = freeze;
  Object.defineProperty(typedPrototype, "length", lengthDescriptor);
  if (calls === 0 || hooks !== 0 || custody !== 0 || sharedCustody !== 0)
    throw new Error(
      `state phase calls=${calls} application hooks=${hooks} callback custody=${custody} shared custody=${sharedCustody}`,
    );
}
