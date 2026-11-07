import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { installApp, installPage, type PreloadFacts } from "./preload-runtime";
import {
  encodeEndpointArguments,
  decodeEndpointArguments,
} from "@keld/kipc/endpoint-value-codec";

// Pure runtime/copy observations. This fake native port cannot prove WK realm,
// Host identity, grants, credits, physical retirement or product acceptance.
const source = JSON.parse(
  readFileSync(
    resolve(
      import.meta.dir,
      "../../../../crates/keld-wv/tests/fixtures/drawio-original-preload.json",
    ),
    "utf8",
  ),
) as string;
const token = "0123456789abcdef0123456789abcdef";
const facts: PreloadFacts = {
  argv: ["--initial-adaptive-colors=auto"],
  versions: { keld: "canonical-test-facts", bun: "1.4.4" },
};
type Message = Record<string, unknown>;
function fixture(seed?: string, bound = true, routeReady = true) {
  const dom = new EventTarget();
  const appMessages: Message[] = [];
  const pageMessages: Message[] = [];
  const stored = new Map<string, string>();
  if (seed !== undefined) stored.set(".configuration", seed);
  const port = (receive: (value: Message) => void) => ({
    postMessage: (text: string) => receive(JSON.parse(text) as Message),
  });
  const common = {
    document: dom,
    addEventListener: dom.addEventListener.bind(dom),
    removeEventListener: dom.removeEventListener.bind(dom),
    dispatchEvent: dom.dispatchEvent.bind(dom),
  };
  const page = { ...common } as unknown as Window & Record<string, unknown>;
  const app = {
    ...common,
    localStorage: {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => {
        stored.set(key, value);
      },
    },
    webkit: {
      messageHandlers: {
        __keld_app_world: port((value) => {
          appMessages.push(value);
          if (value.kind === "call") pageMessages.push(value);
          if (value.kind === "release-call")
            (page.__keldPageRelease as (id: number) => void)(
              value.call as number,
            );
        }),
      },
    },
  } as unknown as Window & Record<string, unknown>;
  const originalWindow = globalThis.window;
  Object.defineProperty(globalThis, "window", {
    value: page,
    configurable: true,
    writable: true,
  });
  installPage(facts);
  Object.defineProperty(globalThis, "window", {
    value: app,
    configurable: true,
    writable: true,
  });
  const original = new Function(
    "require",
    "process",
    "exports",
    "module",
    "Buffer",
    "global",
    "setImmediate",
    "clearImmediate",
    source,
  ) as Parameters<typeof installApp>[1];
  Object.defineProperty(app, "__keldHostDocumentCredential", {
    value: token,
    writable: false,
    configurable: false,
  });
  installApp(facts, original);
  if (bound) {
    (app.__keldAppNativeBind as (v: unknown) => void)({
      document: token,
      ready: routeReady,
    });
    (app.__keldAppNativeRegistered as (document: string) => void)(token);
    (page.__keldPageReady as (v: boolean) => void)(true);
  }
  const call = (name: string, ...args: unknown[]) => {
    (page.electron as Record<string, (...values: unknown[]) => void>)[name]!(
      ...args,
    );
    const value = pageMessages.at(-1)!;
    (app.__keldAppNativeCall as (v: unknown) => void)(value);
    return value;
  };
  const sends = () => appMessages.filter((value) => value.kind === "send");
  const event = (topic: string, args: unknown[], delivery = 1) => {
    (app.__keldAppNativeEvent as (v: unknown) => void)({
      document: token,
      delivery,
      channel: topic,
      payload: Array.from(encodeEndpointArguments(args)),
    });
    return appMessages.at(-1)!;
  };
  const restore = () =>
    Object.defineProperty(globalThis, "window", {
      value: originalWindow,
      configurable: true,
      writable: true,
    });
  return {
    page,
    app,
    stored,
    appMessages,
    pageMessages,
    call,
    sends,
    event,
    restore,
  };
}

describe("original preload runtime only", () => {
  test("unavailable current event is explicitly refused and only current readiness permits listener entry", () => {
    const f = fixture(undefined, true, false);
    try {
      let entered = 0;
      f.call("registerMsgListener", "local", () => {
        entered++;
      });
      const unavailable = f.event("local", ["blocked"]);
      expect(unavailable.kind).toBe("consumed");
      expect(unavailable.disposition).toEqual({
        status: "refused",
        code: "KELD-COMPAT-103",
        message: "current renderer route is unavailable",
      });
      expect(entered).toBe(0);
      (f.app.__keldAppRouteState as (document: string, ready: boolean) => void)(
        "ffffffffffffffffffffffffffffffff",
        true,
      );
      expect(
        (f.event("local", ["stale"], 2).disposition as { status: string })
          .status,
      ).toBe("refused");
      expect(entered).toBe(0);
      (f.app.__keldAppRouteState as (document: string, ready: boolean) => void)(
        token,
        true,
      );
      expect(f.event("local", ["allowed"], 3).disposition).toEqual({
        status: "consumed",
      });
      expect(entered).toBe(1);
      (f.app.__keldAppRouteState as (document: string, ready: boolean) => void)(
        token,
        false,
      );
      expect(
        (f.event("local", ["retired"], 4).disposition as { status: string })
          .status,
      ).toBe("refused");
      expect(entered).toBe(1);
    } finally {
      f.restore();
    }
  });
  test("app install without an isolated native port refuses before original entry", () => {
    const previous = globalThis.window;
    let entered = false;
    Object.defineProperty(globalThis, "window", {
      value: { document: new EventTarget() },
      configurable: true,
      writable: true,
    });
    try {
      expect(() =>
        installApp(facts, () => {
          entered = true;
        }),
      ).toThrow("KELD-COMPAT-103: isolated native app port is unavailable");
      expect(entered).toBe(false);
    } finally {
      Object.defineProperty(globalThis, "window", {
        value: previous,
        configurable: true,
        writable: true,
      });
    }
  });
  test("original body seeds preference, registers real functions and mutates only copied request", () => {
    const f = fixture();
    try {
      expect(
        JSON.parse(f.stored.get(".configuration")!).defaultAdaptiveColors,
      ).toBe("auto");
      expect(f.appMessages.some((value) => value.kind === "registered")).toBe(
        true,
      );
      expect(Object.keys(f.page.electron as object)).toEqual([
        "request",
        "registerMsgListener",
        "sendMessage",
        "listenOnce",
      ]);
      expect(Object.isFrozen(f.page.process)).toBe(true);
      const values: unknown[] = [];
      const msg = { action: "isFullscreen" };
      f.call(
        "request",
        msg,
        (value: unknown) => values.push(value),
        () => {},
      );
      expect(Object.hasOwn(msg, "reqId")).toBe(false);
      const first = decodeEndpointArguments(
        new Uint8Array(f.sends()[0]!.payload as number[]),
      )[0] as { reqId: number };
      expect(first.reqId).toBe(1);
      f.event("mainResp", [{ reqId: first.reqId, data: false }]);
      expect(values).toEqual([false]);
      f.call(
        "request",
        msg,
        (value: unknown) => values.push(value),
        () => {},
      );
      const second = decodeEndpointArguments(
        new Uint8Array(f.sends()[1]!.payload as number[]),
      )[0] as { reqId: number };
      expect(second.reqId).toBe(2);
      expect(second.reqId).not.toBe(first.reqId);
    } finally {
      f.restore();
    }
  });
  test("original nested watch callback survives acknowledgement and two event values", () => {
    const f = fixture();
    try {
      const changes: unknown[][] = [];
      const listener = (...args: unknown[]) => changes.push(args);
      const original = {
        action: "watchFile",
        path: "/test/owned.xml",
        listener,
      };
      f.call(
        "request",
        original,
        () => {},
        () => {},
      );
      expect(original.listener).toBe(listener);
      const sent = decodeEndpointArguments(
        new Uint8Array(f.sends()[0]!.payload as number[]),
      )[0] as { reqId: number; listener?: unknown };
      expect(Object.hasOwn(sent, "listener")).toBe(false);
      f.event("mainResp", [{ reqId: sent.reqId, data: true }]);
      f.event(
        "fileChanged",
        [{ path: original.path, curr: { size: 1 }, prev: { size: 0 } }],
        2,
      );
      f.event(
        "fileChanged",
        [{ path: original.path, curr: { size: 2 }, prev: { size: 1 } }],
        3,
      );
      expect(changes).toEqual([
        [{ size: 1 }, { size: 0 }],
        [{ size: 2 }, { size: 1 }],
      ]);
    } finally {
      f.restore();
    }
  });
  test("local registration has no send dependency and once removes before reentry", () => {
    const f = fixture();
    try {
      const values: unknown[] = [];
      f.call("registerMsgListener", "local", (value: unknown) =>
        values.push(value),
      );
      f.call("listenOnce", "once", (value: unknown) => values.push(value));
      expect(f.sends()).toHaveLength(0);
      f.event("local", ["a"]);
      f.event("once", ["b"], 2);
      f.event("once", ["c"], 3);
      expect(values).toEqual(["a", "b"]);
      expect(f.appMessages.at(-1)!.disposition).toEqual({
        status: "no-listener",
      });
    } finally {
      f.restore();
    }
  });
  test("a throwing original request callback retains its actual request entry", () => {
    const f = fixture();
    try {
      let entered = 0;
      f.call(
        "request",
        { action: "isFullscreen" },
        () => {
          entered++;
          throw new Error("page callback failure");
        },
        () => {},
      );
      const request = decodeEndpointArguments(
        new Uint8Array(f.sends()[0]!.payload as number[]),
      )[0] as { reqId: number };
      f.event("mainResp", [{ reqId: request.reqId, data: true }]);
      expect(
        (f.appMessages.at(-1)!.disposition as { status: string }).status,
      ).toBe("refused");
      f.event("mainResp", [{ reqId: request.reqId, data: false }], 2);
      expect(entered).toBe(2); // original deletes reqInfo only after normal callback completion
      expect(
        f.appMessages.filter((value) => value.kind === "release-call"),
      ).toHaveLength(0);
    } finally {
      f.restore();
    }
  });
  test("completed original requests reuse bounded callback capacity without synthetic cleanup", () => {
    const f = fixture();
    try {
      let callbacks = 0;
      for (let i = 0; i < 100; i++) {
        f.call(
          "request",
          { action: "isFullscreen" },
          () => {
            callbacks++;
          },
          () => {},
        );
        const sent = decodeEndpointArguments(
          new Uint8Array(f.sends().at(-1)!.payload as number[]),
        )[0] as { reqId: number };
        f.event("mainResp", [{ reqId: sent.reqId, data: false }], i + 1);
      }
      expect(callbacks).toBe(100);
      expect(f.sends()).toHaveLength(100);
    } finally {
      f.restore();
    }
  });
  test("native fixture payload vectors are the sole endpoint codec output", () => {
    expect(Array.from(encodeEndpointArguments(["old"]))).toEqual([
      6, 1, 0, 5, 3, 0, 111, 108, 100,
    ]);
    expect(Array.from(encodeEndpointArguments(["fresh"]))).toEqual([
      6, 1, 0, 5, 5, 0, 102, 114, 101, 115, 104,
    ]);
  });
  test("stale bootstrap before the fresh reply cannot poison native-installed recipient binding", () => {
    const f = fixture(undefined, false);
    try {
      const stale = "ffffffffffffffffffffffffffffffff";
      (f.app.__keldAppNativeBind as (v: unknown) => void)({
        document: stale,
        ready: true,
      });
      (f.app.__keldAppNativeRegistered as (document: string) => void)(stale);
      expect(
        f.appMessages.filter((value) => value.kind === "registered"),
      ).toHaveLength(0);
      (f.app.__keldAppNativeBind as (v: unknown) => void)({
        document: token,
        ready: true,
      });
      (f.app.__keldAppNativeRegistered as (document: string) => void)(token);
      f.call(
        "request",
        { action: "isFullscreen" },
        () => {},
        () => {},
      );
      expect(f.sends()).toHaveLength(1);
      const sent = decodeEndpointArguments(
        new Uint8Array(f.sends()[0]!.payload as number[]),
      )[0] as { reqId: number };
      expect(sent.reqId).toBe(1);
    } finally {
      f.restore();
    }
  });
  test("old private call and route controls cannot enter a successor original function", () => {
    const f = fixture();
    try {
      const values: unknown[] = [];
      const first = f.call(
        "request",
        { action: "isFullscreen" },
        (value: unknown) => values.push(value),
        () => {},
      );
      const before = f.appMessages.length;
      const stale = "ffffffffffffffffffffffffffffffff";
      (f.app.__keldAppNativeCall as (v: unknown) => void)({
        ...first,
        document: stale,
      });
      (f.app.__keldAppRouteState as (document: string, ready: boolean) => void)(
        stale,
        false,
      );
      expect(f.appMessages).toHaveLength(before);
      expect(f.sends()).toHaveLength(1);
      f.call(
        "request",
        { action: "isFullscreen" },
        (value: unknown) => values.push(value),
        () => {},
      );
      const sent = decodeEndpointArguments(
        new Uint8Array(f.sends()[1]!.payload as number[]),
      )[0] as { reqId: number };
      expect(sent.reqId).toBe(2);
      f.event("mainResp", [{ reqId: sent.reqId, data: false }]);
      expect(values).toEqual([false]);
    } finally {
      f.restore();
    }
  });
  test("early page invocation is void and creates no retained callback registration", () => {
    const f = fixture(undefined, false);
    try {
      (f.page.__keldPageReady as (v: boolean) => void)(false);
      const fn = () => {
        throw new Error("early callback must not run");
      };
      expect(
        (f.page.electron as { request: (...args: unknown[]) => void }).request(
          { action: "isFullscreen" },
          fn,
          fn,
        ),
      ).toBeUndefined();
      const call = f.pageMessages.at(-1);
      expect(call?.args ?? []).toEqual([]);
      expect(f.sends()).toHaveLength(0);
    } finally {
      f.restore();
    }
  });
});
