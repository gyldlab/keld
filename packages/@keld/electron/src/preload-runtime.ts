/** GH467/GH484 original-only runtime. Native owns all document/role authority. */
import {
  encodeEndpointArguments,
  decodeEndpointArguments,
} from "@keld/kipc/endpoint-value-codec";

type Local =
  | null
  | boolean
  | number
  | string
  | { k: "undefined" }
  | { k: "callback"; id: number }
  | { k: "array"; values: Local[] }
  | { k: "object"; entries: [string, Local][] };
type Method = "request" | "registerMsgListener" | "sendMessage" | "listenOnce";
type Listener = (...args: unknown[]) => void;
interface IpcRendererFacade {
  send(topic: string, ...args: unknown[]): void;
  on(topic: string, fn: Listener): IpcRendererFacade;
  addListener(topic: string, fn: Listener): IpcRendererFacade;
  once(topic: string, fn: Listener): IpcRendererFacade;
  off(topic: string, fn: Listener): IpcRendererFacade;
  removeListener(topic: string, fn: Listener): IpcRendererFacade;
  removeAllListeners(topic?: string): IpcRendererFacade;
}
interface NativePort {
  postMessage(text: string): void;
}
interface WindowsNativePort extends NativePort {
  addEventListener(
    name: "message",
    receive: (event: { data: unknown; source: unknown }) => void,
  ): void;
}
type TerminalDisposition =
  | { status: "consumed" | "no-listener" | "stale" }
  | { status: "refused"; code: string; message: string }
  | {
      status: "route-retired";
      certainty: "not-dispatched" | "outcome-unknown";
    };
type NumericReply =
  | { request: number; ok: true; payload: number[] }
  | { request: number; ok: false; code: string; detail: string };
type HostRendererMessage = { v: 2; document: string } & (
  | { kind: "bound" | "route-state"; ready: boolean }
  | { kind: "event"; delivery: number; channel: string; payload: number[] }
  | { kind: "send-result"; message: number; disposition: TerminalDisposition }
  | { kind: "app-registered" }
  | { kind: "app-refused"; call: number }
  | { kind: "app-call"; call: number; method: Method; args: Local[] }
  | ({ kind: "reply" } & NumericReply)
);
interface AppTransport extends NativePort {
  readonly legacy: boolean;
  readonly numericReply?: (reply: NumericReply) => void;
  subscribe?(receive: (message: HostRendererMessage) => void): void;
}
interface RuntimeWindow extends Window {
  readonly chrome?: { readonly webview?: WindowsNativePort };
  __keldBridgeNativeResult?: (reply: NumericReply) => void;
  readonly webkit?: {
    readonly messageHandlers?: Readonly<Record<string, NativePort>>;
  };
  readonly __keldHostDocumentCredential?: string;
  __keldAppNativeBind?: (value: { document: string; ready: boolean }) => void;
  __keldAppTryBind?: () => void;
  __keldAppNativeRegistered?: (document: string) => void;
  __keldAppNativeRefusal?: (document: string, call: number) => void;
  __keldAppNativeCall?: (call: NativeCall) => void;
  __keldAppNativeEvent?: (value: NativeEvent) => void;
  __keldAppRouteState?: (document: string, ready: boolean) => void;
  __keldAppNativeTerminal?: (value: {
    document: string;
    disposition: { status: string; code?: string; message?: string };
  }) => void;
  __keldPageReady?: (ready: boolean) => void;
  __keldPageRelease?: (call: number) => void;
}
interface NativeCall {
  document: string;
  call: number;
  method: Method;
  args: Local[];
}
interface NativeEvent {
  document: string;
  delivery: number;
  channel: string;
  payload: number[];
}
export interface PreloadFacts {
  argv: string[];
  versions: Record<string, string>;
}
const METHODS: readonly Method[] = [
  "request",
  "registerMsgListener",
  "sendMessage",
  "listenOnce",
];
const READY_EVENT = "__keld_app_ready_data_v1";
const READY_QUERY = "__keld_page_ready_query_v1";
const PAGE_CALL_EVENT = "__keld_page_call_data_v1";
const PAGE_REFUSAL_EVENT = "__keld_page_refusal_data_v1";
const CALLBACK_EVENT = "__keld_app_callback_data_v1";
const CALLBACK_RESULT = "__keld_page_callback_result_v1";
const MAX_RETAINED = 524_288;
const MAX_KEYS = 256;
const MAX_CALLBACKS = 60; // Four fixed method proxies share the 64 endpoint cap.
const apply = Reflect.apply;
const ownKeys = Reflect.ownKeys;
const descriptor = Object.getOwnPropertyDescriptor;
const define = Object.defineProperty;
const freeze = Object.freeze;
const isArray = Array.isArray;
const stringify = JSON.stringify;
const parse = JSON.parse;
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const encodeText = TextEncoder.prototype.encode;
const decodeText = TextDecoder.prototype.decode;
function refuse(detail: string): never {
  throw new Error("KELD-COMPAT-103: " + detail);
}
function boundedText(value: string): number {
  if (value.length > 65_536) refuse("local crossing string bound exceeded");
  const encoded = apply(encodeText, encoder, [value]) as Uint8Array;
  if (apply(decodeText, decoder, [encoded]) !== value)
    refuse("local crossing contains invalid Unicode scalar data");
  const bytes = encoded.byteLength;
  if (bytes > 65_536) refuse("local crossing UTF-8 bound exceeded");
  return bytes;
}
function method(value: unknown): value is Method {
  return typeof value === "string" && METHODS.includes(value as Method);
}
function frozenFacts(facts: PreloadFacts): {
  type: "renderer";
  versions: Readonly<Record<string, string>>;
} {
  const versions: Record<string, string> = Object.create(null);
  for (const key of Object.keys(facts.versions))
    define(versions, key, { value: facts.versions[key], enumerable: true });
  return freeze({ type: "renderer", versions: freeze(versions) });
}

function closed(
  value: unknown,
  fields: readonly string[],
): value is Record<string, unknown> {
  if (!value || typeof value !== "object" || isArray(value)) return false;
  const keys = Object.keys(value);
  return (
    keys.length === fields.length && keys.every((key) => fields.includes(key))
  );
}
function nonzeroU32(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value > 0 &&
    value <= 0xffffffff
  );
}
function payloadBytes(value: unknown): value is number[] {
  return (
    isArray(value) &&
    value.length <= 4096 &&
    value.every(
      (byte) =>
        typeof byte === "number" &&
        Number.isInteger(byte) &&
        byte >= 0 &&
        byte <= 255,
    )
  );
}

/** Validate the existing Local graph once before copy or original method entry. */
function localArguments(value: unknown): {
  args: Local[];
  callbacks: Set<number>;
} {
  if (!isArray(value) || value.length > 16)
    refuse("local argument count bound exceeded");
  let keys = 0,
    bytes = 0;
  const callbacks = new Set<number>();
  function visit(item: unknown, depth: number): void {
    if (depth > 16 || ++keys > MAX_KEYS)
      refuse("local argument graph bound exceeded");
    if (item === null || typeof item === "boolean") return;
    if (typeof item === "number") {
      if (!Number.isFinite(item)) refuse("nonfinite local number");
      return;
    }
    if (typeof item === "string") {
      bytes += boundedText(item);
      if (bytes > 65_536) refuse("local byte bound exceeded");
      return;
    }
    if (!item || typeof item !== "object" || isArray(item))
      refuse("malformed local argument");
    const row = item as Record<string, unknown>;
    switch (row.k) {
      case "undefined":
        if (!closed(row, ["k"])) refuse("invalid undefined descriptor");
        return;
      case "callback":
        if (!closed(row, ["k", "id"]) || !nonzeroU32(row.id))
          refuse("invalid local callback slot");
        callbacks.add(row.id);
        return;
      case "array":
        if (
          !closed(row, ["k", "values"]) ||
          !isArray(row.values) ||
          row.values.length > 256
        )
          refuse("local array admission refused");
        for (const child of row.values) visit(child, depth + 1);
        return;
      case "object": {
        if (
          !closed(row, ["k", "entries"]) ||
          !isArray(row.entries) ||
          row.entries.length > 256
        )
          refuse("local object admission refused");
        const names = new Set<string>();
        for (const pair of row.entries) {
          if (
            !isArray(pair) ||
            pair.length !== 2 ||
            typeof pair[0] !== "string"
          )
            refuse("invalid local data pair");
          const key = pair[0];
          if (
            ++keys > MAX_KEYS ||
            names.has(key) ||
            key === "__proto__" ||
            key === "constructor" ||
            key === "prototype"
          )
            refuse("local key refused");
          names.add(key);
          bytes += boundedText(key);
          if (bytes > 65_536) refuse("local byte bound exceeded");
          visit(pair[1], depth + 1);
        }
        return;
      }
      default:
        refuse("unknown local value tag");
    }
  }
  for (const item of value) visit(item, 1);
  return { args: value as Local[], callbacks };
}

/** JSON.parse owns syntax and number/string decoding; this bounded lexical pass
 * retains duplicate-key evidence that JSON.parse otherwise discards. It is not
 * an endpoint-value codec or a second Local representation. */
function noDuplicateJsonKeys(text: string): boolean {
  const stack: (Set<string> | undefined)[] = [];
  let keys = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"') {
      const start = i++;
      for (; i < text.length; i++) {
        if (text[i] === "\\") i++;
        else if (text[i] === '"') break;
      }
      if (i === text.length) return false;
      let next = i + 1;
      while (next < text.length && /[\t\n\r ]/.test(text[next]!)) next++;
      if (text[next] === ":") {
        const names = stack.at(-1);
        if (!names || ++keys > 1024) return false;
        const key = parse(text.slice(start, i + 1)) as string;
        if (names.has(key)) return false;
        names.add(key);
      }
    } else if (c === "{" || c === "[") {
      if (stack.length >= 64) return false;
      stack.push(c === "{" ? new Set<string>() : undefined);
    } else if (c === "}" || c === "]") stack.pop();
  }
  return true;
}

function terminal(value: unknown): boolean {
  if (!value || typeof value !== "object") return false;
  const row = value as Record<string, unknown>;
  switch (row.status) {
    case "consumed":
    case "no-listener":
    case "stale":
      return closed(row, ["status"]);
    case "route-retired":
      return (
        closed(row, ["status", "certainty"]) &&
        (row.certainty === "not-dispatched" ||
          row.certainty === "outcome-unknown")
      );
    case "refused":
      return (
        closed(row, ["status", "code", "message"]) &&
        typeof row.code === "string" &&
        typeof row.message === "string" &&
        boundedText(row.code) <= 4096 &&
        boundedText(row.message) <= 4096
      );
    default:
      return false;
  }
}

/** Sole closed decoder for the GH1184 Host-to-page JSON carrier. No permission
 * or native-origin authority is derived from its output. */
export function decodeHostRendererMessage(
  text: unknown,
): HostRendererMessage | undefined {
  if (typeof text !== "string" || text.length > 512 * 1024) return;
  try {
    const encoded = apply(encodeText, encoder, [text]) as Uint8Array;
    if (
      encoded.length > 512 * 1024 ||
      apply(decodeText, decoder, [encoded]) !== text ||
      !noDuplicateJsonKeys(text)
    )
      return;
    const value = parse(text) as Record<string, unknown>;
    if (
      !value ||
      typeof value !== "object" ||
      isArray(value) ||
      value.v !== 2 ||
      typeof value.document !== "string" ||
      !/^[0-9a-f]{32}$/.test(value.document) ||
      typeof value.kind !== "string"
    )
      return;
    const fields = (extra: readonly string[]) =>
      closed(value, ["v", "document", "kind", ...extra]);
    let maximum = 4096;
    switch (value.kind) {
      case "bound":
      case "route-state":
        if (!fields(["ready"]) || typeof value.ready !== "boolean") return;
        break;
      case "event":
        maximum = 64 * 1024;
        if (
          !fields(["delivery", "channel", "payload"]) ||
          !nonzeroU32(value.delivery) ||
          typeof value.channel !== "string" ||
          value.channel.length === 0 ||
          boundedText(value.channel) > 256 ||
          /[\u0000-\u001f\u007f-\u009f*]/.test(value.channel) ||
          !payloadBytes(value.payload)
        )
          return;
        break;
      case "send-result":
        if (
          !fields(["message", "disposition"]) ||
          !nonzeroU32(value.message) ||
          !terminal(value.disposition)
        )
          return;
        break;
      case "app-registered":
        if (!fields([])) return;
        break;
      case "app-refused":
        if (!fields(["call"]) || !nonzeroU32(value.call)) return;
        break;
      case "app-call":
        maximum = 512 * 1024;
        if (
          !fields(["call", "method", "args"]) ||
          !nonzeroU32(value.call) ||
          !method(value.method)
        )
          return;
        localArguments(value.args);
        break;
      case "reply":
        if (!nonzeroU32(value.request)) return;
        if (value.ok === true) {
          maximum = 64 * 1024;
          if (
            !fields(["request", "ok", "payload"]) ||
            !payloadBytes(value.payload)
          )
            return;
        } else if (value.ok === false) {
          if (
            !fields(["request", "ok", "code", "detail"]) ||
            typeof value.code !== "string" ||
            typeof value.detail !== "string" ||
            boundedText(value.code) > 4096 ||
            boundedText(value.detail) > 4096
          )
            return;
        } else return;
        break;
      default:
        return;
    }
    if (encoded.length > maximum) return;
    return value as HostRendererMessage;
  } catch {
    return;
  }
}

/** The sole page handler carries untrusted copied values; it has no credential. */
export function installPage(facts: PreloadFacts): void {
  const w = window as RuntimeWindow;
  if (Object.hasOwn(w, "electron") || Object.hasOwn(w, "process"))
    refuse("page publication conflicts with an existing property");
  const dom = w.document; // Captured actual Document; never re-read a WindowProxy after navigation.
  const post = (text: string) =>
    dom.dispatchEvent(new CustomEvent(PAGE_CALL_EVENT, { detail: text }));
  const callbacks = new Map<number, Listener>();
  const functionIds = new WeakMap<Listener, number>();
  const calls = new Map<number, number[]>();
  let serial = 0,
    callbackSerial = 0,
    ready = false;
  w.__keldPageReady = (value) => {
    ready = value === true;
  };
  dom.addEventListener(READY_EVENT, () => {
    ready = true;
  });
  const releasePageCall = (call: number) => {
    const ids = calls.get(call) ?? [];
    calls.delete(call);
    for (const id of ids)
      if (![...calls.values()].some((owned) => owned.includes(id)))
        callbacks.delete(id);
  };
  w.__keldPageRelease = releasePageCall;
  function snapshot(values: unknown[], call: number): Local[] {
    let keys = 0,
      bytes = 0;
    const seen = new WeakSet<object>();
    if (calls.size >= 64)
      refuse("document callback-registration limit exhausted");
    const owned: number[] = [];
    const newlyOwned: number[] = [];
    function copy(value: unknown, depth: number): Local {
      if (depth > 16 || ++keys > MAX_KEYS)
        refuse("local crossing depth/key bound exceeded");
      if (value === undefined) return { k: "undefined" };
      if (value === null || typeof value === "boolean") return value;
      if (typeof value === "number") {
        if (!Number.isFinite(value)) refuse("nonfinite local number");
        return value;
      }
      if (typeof value === "string") {
        bytes += boundedText(value);
        if (bytes > 65_536) refuse("local crossing data bound exceeded");
        return value;
      }
      if (typeof value === "function") {
        const fn = value as Listener;
        const existing = functionIds.get(fn);
        if (existing !== undefined && callbacks.has(existing)) {
          if (!owned.includes(existing)) owned.push(existing);
          return { k: "callback", id: existing };
        }
        if (callbacks.size >= MAX_CALLBACKS || callbackSerial === 0xffffffff)
          refuse("local callback limit exhausted");
        const id = ++callbackSerial;
        callbacks.set(id, fn);
        functionIds.set(fn, id);
        owned.push(id);
        newlyOwned.push(id);
        return { k: "callback", id };
      }
      if (typeof value !== "object" || seen.has(value))
        refuse("unsupported or cyclic local crossing");
      seen.add(value);
      const names = ownKeys(value);
      if (names.length > MAX_KEYS - keys) refuse("local key bound exceeded");
      if (isArray(value)) {
        const length = descriptor(value, "length")?.value as unknown;
        if (
          typeof length !== "number" ||
          length > 256 ||
          names.some(
            (key) =>
              key !== "length" &&
              (typeof key !== "string" || !/^(0|[1-9][0-9]*)$/.test(key)),
          )
        )
          refuse("invalid local array");
        const result: Local[] = [];
        for (let i = 0; i < length; i++) {
          const d = descriptor(value, String(i));
          if (!d || !("value" in d)) refuse("sparse or accessor local array");
          result.push(copy(d.value, depth + 1));
        }
        return { k: "array", values: result };
      }
      const entries: [string, Local][] = [];
      for (const key of names) {
        if (
          typeof key !== "string" ||
          key === "__proto__" ||
          key === "constructor" ||
          key === "prototype"
        )
          refuse("unsupported local key");
        if (++keys > MAX_KEYS) refuse("local key bound exceeded");
        bytes += boundedText(key);
        const d = descriptor(value, key);
        if (!d || !("value" in d)) refuse("local accessors are unsupported");
        entries.push([key, copy(d.value, depth + 1)]);
      }
      return { k: "object", entries };
    }
    try {
      const copied = values.map((value) => copy(value, 1));
      calls.set(call, owned);
      return copied;
    } catch (error) {
      for (const id of newlyOwned) callbacks.delete(id);
      throw error;
    }
  }
  const pageApi = Object.create(null) as Record<
    Method,
    (...args: unknown[]) => void
  >;
  for (const name of METHODS)
    define(pageApi, name, {
      enumerable: true,
      value: freeze((...args: unknown[]): void => {
        if (serial === 0xffffffff) refuse("local call sequence exhausted");
        const call = ++serial;
        // Early calls create no persistent Function slots or owned argument queue.
        if (!ready) {
          post(stringify({ v: 1, kind: "call", call, method: name, args: [] }));
          return;
        }
        const copied = snapshot(args, call);
        post(
          stringify({ v: 1, kind: "call", call, method: name, args: copied }),
        );
      }),
    });
  dom.addEventListener(PAGE_REFUSAL_EVENT, (event: Event) => {
    const call = (event as CustomEvent<unknown>).detail;
    if (typeof call === "number") releasePageCall(call);
  });
  dom.addEventListener(CALLBACK_EVENT, (event: Event) => {
    const text = (event as CustomEvent<unknown>).detail;
    if (typeof text !== "string" || text.length > 65_536) return;
    let response: { id: number; ok: boolean; message?: string } | undefined;
    try {
      const data = parse(text) as { id: number; payload: number[] };
      const fn = callbacks.get(data.id);
      if (!fn || !isArray(data.payload) || data.payload.length > 4096) return;
      const args = decodeEndpointArguments(new Uint8Array(data.payload));
      try {
        apply(fn, undefined, args);
        response = { id: data.id, ok: true };
      } catch (error) {
        response = {
          id: data.id,
          ok: false,
          message:
            error instanceof Error
              ? error.message.slice(0, 512)
              : "page callback threw",
        };
      }
    } catch {
      return;
    }
    if (response)
      dom.dispatchEvent(
        new CustomEvent(CALLBACK_RESULT, { detail: stringify(response) }),
      );
  });
  define(w, "electron", {
    value: freeze(pageApi),
    enumerable: true,
    writable: false,
    configurable: false,
  });
  define(w, "process", {
    value: frozenFacts(facts),
    enumerable: true,
    writable: false,
    configurable: false,
  });
  dom.dispatchEvent(new CustomEvent(READY_QUERY));
}

type OriginalPreload = (
  require: (name: string) => unknown,
  process: object,
  exports: object,
  module: object,
  Buffer: unknown,
  global: unknown,
  setImmediate: unknown,
  clearImmediate: unknown,
) => void;

/** Preserve the actual macOS isolated-world entry point and isolation fact. */
export function installApp(
  facts: PreloadFacts,
  original: OriginalPreload,
): void {
  const w = window as RuntimeWindow;
  const native = w.webkit?.messageHandlers?.__keld_app_world;
  const send = native?.postMessage;
  if (!native || typeof send !== "function")
    refuse("isolated native app port is unavailable");
  installAppRuntime(facts, original, {
    legacy: false,
    postMessage: (text) => {
      apply(send, native, [text]);
    },
  });
}

/** Explicit approved Windows Legacy adapter. Native still owns the source,
 * document nonce, grants and all effects; page-visible data grants no authority. */
export function installWindows(
  facts: PreloadFacts,
  original: OriginalPreload,
): void {
  const w = window as RuntimeWindow;
  // The fixed native bootstrap also checks its qualified source before install.
  // This guard must precede both facade publication and original source entry.
  if (w.top !== w) return;
  const native = w.chrome?.webview;
  const send = native?.postMessage;
  const subscribe = native?.addEventListener;
  if (!native || typeof send !== "function" || typeof subscribe !== "function")
    refuse("Windows native app port is unavailable");
  const numeric = descriptor(w, "__keldBridgeNativeResult")?.value as unknown;
  const transport: AppTransport = {
    legacy: true,
    postMessage: (text) => {
      apply(send, native, [text]);
    },
    numericReply:
      typeof numeric === "function"
        ? (reply) => {
            apply(numeric, w, [reply]);
          }
        : undefined,
    subscribe: (receive) => {
      apply(subscribe, native, [
        "message",
        (event: { data: unknown; source: unknown }) => {
          if (event.source !== native) return;
          const value = decodeHostRendererMessage(event.data);
          if (value) receive(value);
        },
      ]);
    },
  };
  installPage(facts);
  installAppRuntime(facts, original, transport);
}

/** One original runtime owns callbacks, copies and accounting for both adapters. */
function installAppRuntime(
  facts: PreloadFacts,
  original: OriginalPreload,
  native: AppTransport,
): void {
  const w = window as RuntimeWindow;
  const dom = w.document;
  let credential: string | undefined,
    ready = false,
    message = 0,
    charged = 0;
  let registry: Record<Method, Listener> | undefined;
  let processPublished = false,
    nativeRegistered = false;
  const listeners = new Map<
    string,
    { fn: Listener; once: boolean; call?: number }[]
  >();
  const callbackProxies = new Map<number, Listener>();
  const callProxies = new Map<number, Set<number>>();
  const localCharges = new Map<number, number>();
  let copyingCall: number | undefined;
  function releaseCall(call: number): void {
    const ids = callProxies.get(call);
    callProxies.delete(call);
    if (ids)
      for (const id of ids)
        if (![...callProxies.values()].some((owned) => owned.has(id)))
          callbackProxies.delete(id);
    dom.dispatchEvent(new CustomEvent(PAGE_REFUSAL_EVENT, { detail: call }));
  }
  const requests = new Map<
    number,
    {
      call: number;
      bytes: number;
      active: boolean;
      message: number;
      watchPath?: string;
      unwatchPath?: string;
    }
  >();
  const watches = new Map<string, number>();
  const activeWatches = new Set<string>();
  let invoking: { call: number; bytes: number; watch: boolean } | undefined;
  const post = (value: unknown) => native.postMessage(stringify(value));
  function reserve(bytes: number): void {
    if (bytes > MAX_RETAINED - charged)
      refuse(
        "document retention limit exhausted; explicitly close or reload this document to reclaim retained state",
      );
    charged += bytes;
  }
  function listenerCount(): number {
    let count = 0;
    for (const rows of listeners.values()) count += rows.length;
    return count;
  }
  const ipcRenderer: IpcRendererFacade = {
    send(topic: string, ...args: unknown[]): void {
      if (!credential || !ready)
        refuse("current document app binding or Bun role unavailable");
      if (typeof topic !== "string" || encoder.encode(topic).length > 256)
        refuse("invalid literal IPC topic");
      const payload = encodeEndpointArguments(args);
      if (message === 0xffffffff) refuse("named send sequence exhausted");
      // Observe original bookkeeping after it was created by the unchanged Function.
      if (invoking && topic === "rendererReq") {
        const value = args[0] as {
          reqId?: unknown;
          action?: unknown;
          path?: unknown;
        };
        if (typeof value?.reqId !== "number" || !Number.isInteger(value.reqId))
          refuse("original request did not assign an id");
        requests.set(value.reqId, {
          call: invoking.call,
          bytes: invoking.bytes,
          active: true,
          message: message + 1,
          watchPath:
            invoking.watch && typeof value.path === "string"
              ? value.path
              : undefined,
          unwatchPath:
            value.action === "unwatchFile" && typeof value.path === "string"
              ? value.path
              : undefined,
        });
        if (invoking.watch && typeof value.path === "string")
          watches.set(value.path, invoking.call);
      }
      post({
        v: 2,
        kind: "send",
        document: credential,
        message: ++message,
        channel: topic,
        payload: Array.from(payload),
      });
    },
    on(topic: string, fn: Listener) {
      add(topic, fn, false);
      return ipcRenderer;
    },
    addListener(topic: string, fn: Listener) {
      add(topic, fn, false);
      return ipcRenderer;
    },
    once(topic: string, fn: Listener) {
      add(topic, fn, true);
      return ipcRenderer;
    },
    off(topic: string, fn: Listener) {
      remove(topic, fn);
      return ipcRenderer;
    },
    removeListener(topic: string, fn: Listener) {
      remove(topic, fn);
      return ipcRenderer;
    },
    removeAllListeners(topic?: string) {
      if (topic === undefined) listeners.clear();
      else listeners.delete(topic);
      return ipcRenderer;
    },
  };
  function add(topic: string, fn: Listener, once: boolean): void {
    if (
      typeof topic !== "string" ||
      typeof fn !== "function" ||
      listenerCount() >= 64
    )
      refuse("bounded renderer listener registration refused");
    const rows = listeners.get(topic) ?? [];
    rows.push({ fn, once, call: invoking?.call });
    listeners.set(topic, rows);
  }
  function remove(topic: string, fn: Listener): void {
    const rows = listeners.get(topic);
    if (!rows) return;
    for (let i = rows.length - 1; i >= 0; i--)
      if (rows[i]?.fn === fn) {
        rows.splice(i, 1);
        break;
      }
    if (!rows.length) listeners.delete(topic);
  }
  function revive(
    value: Local,
    depth = 1,
    state = { keys: 0, bytes: 0 },
  ): unknown {
    if (depth > 16 || ++state.keys > 256)
      refuse("local argument graph bound exceeded");
    if (
      value === null ||
      typeof value === "boolean" ||
      typeof value === "number"
    )
      return value;
    if (typeof value === "string") {
      state.bytes += boundedText(value);
      if (state.bytes > 65_536) refuse("local byte bound exceeded");
      return value;
    }
    if (!value || typeof value !== "object") refuse("malformed local argument");
    if (value.k === "undefined") return undefined;
    if (value.k === "callback") {
      if (!Number.isInteger(value.id) || value.id <= 0)
        refuse("invalid local callback slot");
      if (copyingCall !== undefined)
        callProxies.get(copyingCall)?.add(value.id);
      const existing = callbackProxies.get(value.id);
      if (existing) return existing;
      if (callbackProxies.size >= MAX_CALLBACKS)
        refuse("local callback slot limit exhausted");
      const callback: Listener = (...args: unknown[]) => {
        const payload = Array.from(encodeEndpointArguments(args));
        let result: { id: number; ok: boolean; message?: string } | undefined;
        const receive = (event: Event) => {
          const text = (event as CustomEvent<unknown>).detail;
          if (typeof text !== "string" || text.length > 2_048) return;
          try {
            const data = parse(text) as typeof result;
            if (data?.id === value.id) result = data;
          } catch {
            return;
          }
        };
        dom.addEventListener(CALLBACK_RESULT, receive);
        try {
          dom.dispatchEvent(
            new CustomEvent(CALLBACK_EVENT, {
              detail: stringify({ id: value.id, payload }),
            }),
          );
        } finally {
          dom.removeEventListener(CALLBACK_RESULT, receive);
        }
        if (!result?.ok)
          throw new Error(
            result?.message ??
              "KELD-COMPAT-103: page callback did not complete",
          );
      };
      callbackProxies.set(value.id, callback);
      return callback;
    }
    if (value.k === "array") {
      if (value.values.length > 256) refuse("local array bound exceeded");
      return value.values.map((item) => revive(item, depth + 1, state));
    }
    if (value.k === "object") {
      const result: Record<string, unknown> = {};
      for (const [key, item] of value.entries) {
        if (
          ++state.keys > 256 ||
          key === "__proto__" ||
          key === "constructor" ||
          key === "prototype"
        )
          refuse("local key refused");
        state.bytes += boundedText(key);
        if (state.bytes > 65_536) refuse("local byte bound exceeded");
        define(result, key, {
          value: revive(item, depth + 1, state),
          enumerable: true,
          writable: true,
          configurable: true,
        });
      }
      return result;
    }
    refuse("unknown local value tag");
  }
  const contextBridge = {
    exposeInMainWorld(key: string, api: unknown): void {
      if (key === "process") {
        const value = api as { type?: unknown; versions?: unknown };
        if (
          processPublished ||
          value?.type !== "renderer" ||
          value.versions !== process.versions
        )
          refuse("original process publication mismatched canonical facts");
        processPublished = true;
        return;
      }
      if (key !== "electron" || registry || !api || typeof api !== "object")
        refuse("only one exact original electron registration is admitted");
      const names = ownKeys(api);
      if (names.length !== 4 || names.some((name) => !method(name)))
        refuse("original electron registry shape mismatch");
      const own = Object.create(null) as Record<Method, Listener>;
      for (const name of METHODS) {
        const d = descriptor(api, name);
        if (!d || !("value" in d) || typeof d.value !== "function")
          refuse("original method was not a Function");
        own[name] = d.value;
      }
      registry = freeze(own);
    },
  };
  const unsupported = (): never =>
    refuse("renderer member is outside the approved first-proof subset");
  const unavailable = new Proxy(freeze({}), { get: unsupported });
  const electron = freeze({
    contextBridge: freeze(contextBridge),
    ipcRenderer: freeze(ipcRenderer),
    crashReporter: unavailable,
    nativeImage: unavailable,
    sharedTexture: unavailable,
    webFrame: unavailable,
    webUtils: unavailable,
  });
  // These named sandbox module entries are bounded pure-JS objects, with unsupported operations explicit.
  const events = freeze({
    EventEmitter: class {
      constructor() {
        unsupported();
      }
    },
  });
  const timers = freeze({
    setTimeout: unsupported,
    setInterval: unsupported,
    setImmediate: unsupported,
    clearTimeout: unsupported,
    clearInterval: unsupported,
    clearImmediate: unsupported,
  });
  const url = freeze({ URL, URLSearchParams });
  const modules = new Map<string, unknown>([
    ["electron", electron],
    ["electron/common", electron],
    ["electron/renderer", electron],
    ["events", events],
    ["node:events", events],
    ["timers", timers],
    ["node:timers", timers],
    ["url", url],
    ["node:url", url],
  ]);
  const require = (name: string): unknown => {
    if (!modules.has(name)) throw new Error("module not found: " + name);
    return modules.get(name);
  };
  const process = freeze({
    ...frozenFacts(facts),
    sandboxed: true,
    contextIsolated: !native.legacy,
    argv: freeze([...facts.argv]),
    env: freeze(Object.create(null)),
  });
  dom.addEventListener(PAGE_CALL_EVENT, (event: Event) => {
    const text = (event as CustomEvent<unknown>).detail;
    if (typeof text !== "string" || text.length > 512 * 1024) return;
    let data: NativeCall & { v?: unknown; kind?: unknown };
    try {
      data = parse(text) as typeof data;
    } catch {
      return;
    }
    if (
      !data ||
      data.v !== 1 ||
      data.kind !== "call" ||
      !method(data.method) ||
      !Number.isInteger(data.call) ||
      data.call <= 0 ||
      !isArray(data.args)
    )
      return;
    if (!credential || !registry || !processPublished || !nativeRegistered) {
      console.error(
        "KELD-COMPAT-103: early page request refused without retention",
      );
      dom.dispatchEvent(
        new CustomEvent(PAGE_REFUSAL_EVENT, { detail: data.call }),
      );
      return;
    }
    // DOM data is untrusted. This appends only the currently bound nonce.
    // Mac keeps it private; the approved Windows Legacy page may read/forge it.
    // Neither representation replaces native source, identity or grant checks.
    post({
      v: 1,
      kind: "call",
      document: credential,
      call: data.call,
      method: data.method,
      args: data.args,
    });
  });
  dom.addEventListener(READY_QUERY, () => {
    if (nativeRegistered) dom.dispatchEvent(new CustomEvent(READY_EVENT));
  });
  const nativeRefusal = (w.__keldAppNativeRefusal = (document, call) => {
    if (document === credential)
      dom.dispatchEvent(new CustomEvent(PAGE_REFUSAL_EVENT, { detail: call }));
  });
  const nativeRegisteredReceipt = (w.__keldAppNativeRegistered = (document) => {
    if (document === credential && credential && registry && processPublished) {
      nativeRegistered = true;
      dom.dispatchEvent(new CustomEvent(READY_EVENT));
    }
  });
  const tryBind = (w.__keldAppTryBind = () => post({ v: 2, kind: "bind" }));
  const nativeBind = (w.__keldAppNativeBind = (value) => {
    if (
      credential !== undefined ||
      (!native.legacy && value.document !== w.__keldHostDocumentCredential) ||
      !registry ||
      !processPublished ||
      typeof value.document !== "string" ||
      !/^[0-9a-f]{32}$/.test(value.document)
    )
      return;
    credential = value.document;
    ready = value.ready === true;
    // Registration is world/native authenticated and document-bound only now.
    post({ v: 1, kind: "registered", document: credential, methods: METHODS });
  });
  const nativeRouteState = (w.__keldAppRouteState = (document, value) => {
    if (!credential || document !== credential) return;
    ready = value === true;
    if (!ready) {
      activeWatches.clear();
      for (const record of requests.values()) record.active = false;
    }
  });
  const nativeTerminal = (w.__keldAppNativeTerminal = (value) => {
    if (value.document !== credential) return;
    if (value.disposition.status === "route-retired")
      nativeRouteState(value.document, false);
    else if (value.disposition.status !== "consumed")
      for (const record of requests.values())
        if (record.message === (value as { message?: number }).message)
          record.active = false;
    if (value.disposition.status === "refused")
      console.error(
        (value.disposition.code ?? "KELD-COMPAT-103") +
          ": " +
          (value.disposition.message ?? "one-way send refused"),
      );
    // A terminal one-way result never fabricates mainResp or deletes original maps.
  });
  const nativeCall = (w.__keldAppNativeCall = (call) => {
    if (
      !credential ||
      call.document !== credential ||
      !registry ||
      !nativeRegistered ||
      !method(call.method)
    )
      return;
    let bytes = 0,
      extra = 0,
      reserved = 0,
      entered = false;
    const proxiesBefore = new Set(callbackProxies.keys());
    try {
      const { callbacks: ids } = localArguments(call.args);
      bytes = stringify(call.args).length * 2 + ids.size * 2_048 + 1_024;
      const local =
        call.method === "registerMsgListener" || call.method === "listenOnce";
      const first = call.args[0];
      const watch =
        call.method === "request" &&
        first !== null &&
        typeof first === "object" &&
        first.k === "object" &&
        first.entries.some(
          ([key, value]) => key === "action" && value === "watchFile",
        );
      extra = watch ? 1_024 : 0;
      reserve(bytes + extra);
      reserved = bytes + extra;
      if (callProxies.size >= 64)
        refuse("document registration limit exhausted");
      callProxies.set(call.call, new Set());
      copyingCall = call.call;
      const state = { keys: 0, bytes: 0 };
      const args = call.args.map((value) => revive(value, 1, state));
      copyingCall = undefined;
      invoking = { call: call.call, bytes: watch ? 1_024 : bytes, watch };
      entered = true;
      if (local) localCharges.set(call.call, bytes);
      apply(registry[call.method], undefined, args);
      if (call.method === "sendMessage") {
        charged -= bytes;
        releaseCall(call.call);
      }
    } catch (error) {
      if (!entered) {
        charged -= reserved;
        for (const id of callbackProxies.keys())
          if (!proxiesBefore.has(id)) callbackProxies.delete(id);
        releaseCall(call.call);
      }
      console.error("KELD-COMPAT-103: original preload call refused", error);
    } finally {
      copyingCall = undefined;
      invoking = undefined;
      post({
        v: 1,
        kind: "call-finished",
        document: credential,
        call: call.call,
      });
    }
  });
  const nativeEvent = (w.__keldAppNativeEvent = (event) => {
    if (!credential || event.document !== credential) return;
    if (!ready) {
      post({
        v: 2,
        kind: "consumed",
        document: credential,
        delivery: event.delivery,
        disposition: {
          status: "refused",
          code: "KELD-COMPAT-103",
          message: "current renderer route is unavailable",
        },
      });
      return;
    }
    let disposition: {
      status: "consumed" | "no-listener" | "refused";
      code?: string;
      message?: string;
    } = { status: "no-listener" };
    try {
      const topic = event.channel;
      const args = decodeEndpointArguments(new Uint8Array(event.payload));
      if (topic === "mainResp") {
        const response = args[0] as { reqId?: unknown };
        if (
          typeof response?.reqId !== "number" ||
          !requests.get(response.reqId)?.active
        )
          refuse("response belongs to an absent or retired original request");
      }
      if (topic === "fileChanged") {
        const response = args[0] as { path?: unknown };
        if (
          typeof response?.path !== "string" ||
          !activeWatches.has(response.path)
        )
          refuse("file event belongs to a retired or unacknowledged watch");
      }
      const rows = [...(listeners.get(topic) ?? [])];
      if (rows.length) {
        for (const row of rows) {
          if (row.once) remove(topic, row.fn);
          try {
            apply(row.fn, undefined, [
              freeze({ sender: ipcRenderer, ports: freeze([]) }),
              ...args,
            ]);
          } finally {
            if (row.once && row.call !== undefined) {
              charged -= localCharges.get(row.call) ?? 0;
              localCharges.delete(row.call);
              releaseCall(row.call);
            }
          }
        }
        disposition = { status: "consumed" };
        if (topic === "mainResp") {
          const response = args[0] as { reqId?: unknown; error?: unknown };
          const req =
            typeof response?.reqId === "number"
              ? requests.get(response.reqId)
              : undefined;
          if (req && typeof response.reqId === "number") {
            requests.delete(response.reqId);
            charged -= req.bytes;
            if (!response.error && req.watchPath)
              activeWatches.add(req.watchPath);
            if (!response.error && req.unwatchPath)
              activeWatches.delete(req.unwatchPath);
            if (![...watches.values()].includes(req.call))
              releaseCall(req.call);
          }
        }
      }
    } catch (error) {
      disposition = {
        status: "refused",
        code: "KELD-COMPAT-103",
        message:
          error instanceof Error
            ? error.message.slice(0, 512)
            : "original handler threw",
      };
    }
    post({
      v: 2,
      kind: "consumed",
      document: credential,
      delivery: event.delivery,
      disposition,
    });
  });
  native.subscribe?.((value) => {
    if (value.kind === "bound") {
      nativeBind(value);
      return;
    }
    if (!credential || value.document !== credential) return;
    switch (value.kind) {
      case "event":
        nativeEvent(value);
        break;
      case "send-result":
        nativeTerminal(value);
        break;
      case "route-state":
        nativeRouteState(value.document, value.ready);
        break;
      case "app-registered":
        nativeRegisteredReceipt(value.document);
        break;
      case "app-refused":
        nativeRefusal(value.document, value.call);
        break;
      case "app-call":
        nativeCall(value);
        break;
      case "reply":
        if (value.ok)
          native.numericReply?.({
            request: value.request,
            ok: true,
            payload: value.payload,
          });
        else
          native.numericReply?.({
            request: value.request,
            ok: false,
            code: value.code,
            detail: value.detail,
          });
        break;
    }
  });
  const exports = {};
  const module = { exports };
  try {
    original(
      require,
      process,
      exports,
      module,
      unsupported,
      globalThis,
      unsupported,
      unsupported,
    );
  } catch (error) {
    console.error("Unable to load preload script:", error);
  }
  if (registry && processPublished) tryBind();
}

// This factory is embedded in a native-owned closure and removed immediately.
Object.defineProperty(globalThis, "__keldOriginalPreloadRuntime", {
  value: Object.freeze({ installApp, installPage, installWindows }),
  configurable: true,
});
