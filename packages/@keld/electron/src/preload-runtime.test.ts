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
// Built, not written as a literal: an all-zero 32-byte hex string next to a
// `token` name is indistinguishable from a credential to secret scanners.
const token = "0".repeat(32);
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
  test("an acknowledged unwatch releases its watch owner but a failed unwatch preserves it", () => {
    const f = fixture();
    try {
      const path = "/test/watch-lifecycle.xml";
      const released: number[] = [];
      f.page.document.addEventListener(
        "__keld_page_refusal_data_v1",
        (event) => {
          released.push((event as CustomEvent<number>).detail);
        },
      );
      let events = 0;
      function request(action: string, listener?: () => void) {
        const call = f.call(
          "request",
          { action, path, listener },
          () => {},
          () => {},
        );
        const { reqId } = decodeEndpointArguments(
          new Uint8Array(f.sends().at(-1)!.payload as number[]),
        )[0] as { reqId: number };
        return { call: call.call as number, reqId };
      }
      const watch = request("watchFile", () => events++);
      f.event("mainResp", [{ reqId: watch.reqId, data: true }]);
      const denied = request("unwatchFile");
      f.event("mainResp", [{ reqId: denied.reqId, error: "not stopped" }], 2);
      expect(released).not.toContain(watch.call);
      f.event("fileChanged", [{ path, curr: 1, prev: 0 }], 3);
      expect(events).toBe(1);
      const stopped = request("unwatchFile");
      f.event("mainResp", [{ reqId: stopped.reqId, data: true }], 4);
      expect(released.filter((call) => call === watch.call)).toHaveLength(1);
    } finally {
      f.restore();
    }
  });

  test("an acknowledged stop retires the watcher when its original callback throws", () => {
    const f = fixture();
    try {
      const path = "/test/watch-callback-failure.xml";
      const released: number[] = [];
      f.page.document.addEventListener(
        "__keld_page_refusal_data_v1",
        (event) => {
          released.push((event as CustomEvent<number>).detail);
        },
      );
      function request(
        action: string,
        success: () => void,
        failure: () => void,
      ) {
        const call = f.call(
          "request",
          action === "watchFile"
            ? { action, path, listener: () => {} }
            : { action, path },
          success,
          failure,
        );
        const { reqId } = decodeEndpointArguments(
          new Uint8Array(f.sends().at(-1)!.payload as number[]),
        )[0] as { reqId: number };
        return { call: call.call as number, reqId };
      }
      const throwing = () => {
        throw new Error("page callback failure");
      };
      const noop = () => {};
      const watch = request("watchFile", noop, noop);
      f.event("mainResp", [{ reqId: watch.reqId, data: true }]);
      const failedStop = request("unwatchFile", noop, throwing);
      expect(
        f.event(
          "mainResp",
          [{ reqId: failedStop.reqId, error: "not stopped" }],
          2,
        ).disposition,
      ).toMatchObject({ status: "refused", message: "page callback failure" });
      expect(released).not.toContain(watch.call);
      const stopped = request("unwatchFile", throwing, noop);
      expect(
        f.event("mainResp", [{ reqId: stopped.reqId, data: true }], 3)
          .disposition,
      ).toMatchObject({ status: "refused", message: "page callback failure" });
      expect(released).toEqual([watch.call, stopped.call]);
    } finally {
      f.restore();
    }
  });

  test("an acknowledged stop does not retire a newer watch on the same path", () => {
    const f = fixture();
    try {
      const path = "/test/watch-renewed.xml";
      const released: number[] = [];
      f.page.document.addEventListener(
        "__keld_page_refusal_data_v1",
        (event) => {
          released.push((event as CustomEvent<number>).detail);
        },
      );
      function request(action: string) {
        const call = f.call(
          "request",
          action === "watchFile"
            ? { action, path, listener: () => {} }
            : { action, path },
          () => {},
          () => {},
        );
        const { reqId } = decodeEndpointArguments(
          new Uint8Array(f.sends().at(-1)!.payload as number[]),
        )[0] as { reqId: number };
        return { call: call.call as number, reqId };
      }
      const pending = request("watchFile");
      const stopped = request("unwatchFile");
      const newer = request("watchFile");
      f.event("mainResp", [{ reqId: newer.reqId, data: true }]);
      f.event("mainResp", [{ reqId: stopped.reqId, data: true }], 2);
      expect(released).not.toContain(pending.call);
      expect(released).not.toContain(newer.call);
      f.event("mainResp", [{ reqId: pending.reqId, data: true }], 3);
      expect(released.filter((call) => call === pending.call)).toHaveLength(1);
      expect(
        f.event("fileChanged", [{ path, curr: 1, prev: 0 }], 4).disposition,
      ).toEqual({
        status: "consumed",
      });
    } finally {
      f.restore();
    }
  });
  test("replacing completed same-path watches retires prior callbacks without losing other paths", () => {
    const f = fixture();
    try {
      const path = "/test/owned.xml";
      const other = "/test/other.xml";
      let oldEvents = 0;
      let latestEvents = 0;
      let otherEvents = 0;
      const acknowledge = (delivery: number) => {
        const send = f.sends().at(-1)!;
        const request = decodeEndpointArguments(
          new Uint8Array(send.payload as number[]),
        )[0] as { reqId: number };
        f.event("mainResp", [{ reqId: request.reqId, data: true }], delivery);
      };
      f.call(
        "request",
        { action: "watchFile", path: other, listener: () => otherEvents++ },
        () => {},
        () => {},
      );
      acknowledge(1);
      for (let i = 0; i < 85; i++) {
        const before = f.sends().length;
        f.call(
          "request",
          {
            action: "watchFile",
            path,
            listener: i === 84 ? () => latestEvents++ : () => oldEvents++,
          },
          () => {},
          () => {},
        );
        expect(f.sends()).toHaveLength(before + 1);
        acknowledge(i + 2);
      }
      f.event(
        "fileChanged",
        [{ path, curr: { size: 1 }, prev: { size: 0 } }],
        100,
      );
      f.event(
        "fileChanged",
        [{ path: other, curr: { size: 2 }, prev: { size: 1 } }],
        101,
      );
      expect(oldEvents).toBe(0);
      expect(latestEvents).toBe(1);
      expect(otherEvents).toBe(1);
      const before = f.sends().length;
      f.call(
        "request",
        { action: "isFullscreen" },
        () => {},
        () => {},
      );
      expect(f.sends()).toHaveLength(before + 1);
    } finally {
      f.restore();
    }
  });
  test("a replaced watch retains its response callbacks until the older response completes", () => {
    const f = fixture();
    try {
      const path = "/test/pending.xml";
      let oldResponse = 0;
      let oldEvents = 0;
      let latestEvents = 0;
      f.call(
        "request",
        { action: "watchFile", path, listener: () => oldEvents++ },
        () => oldResponse++,
        () => {},
      );
      const first = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      f.call(
        "request",
        { action: "watchFile", path, listener: () => latestEvents++ },
        () => {},
        () => {},
      );
      const second = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      f.event("mainResp", [{ reqId: second.reqId, data: true }], 1);
      f.event("mainResp", [{ reqId: first.reqId, data: true }], 2);
      expect(oldResponse).toBe(1);
      f.event(
        "fileChanged",
        [{ path, curr: { size: 3 }, prev: { size: 2 } }],
        3,
      );
      expect(oldEvents).toBe(0);
      expect(latestEvents).toBe(1);
    } finally {
      f.restore();
    }
  });
  test("denied watches release callbacks and leave later requests usable", () => {
    const f = fixture();
    try {
      for (let i = 0; i < 70; i++) {
        const path = `/test/denied-${i}.xml`;
        const before = f.sends().length;
        f.call(
          "request",
          { action: "watchFile", path, listener: () => {} },
          () => {},
          () => {},
        );
        expect(f.sends()).toHaveLength(before + 1);
        const sent = decodeEndpointArguments(
          new Uint8Array(f.sends().at(-1)!.payload as number[]),
        )[0] as { reqId: number };
        expect(
          f.event("mainResp", [{ reqId: sent.reqId, error: "denied" }], i + 1)
            .disposition,
        ).toEqual({ status: "consumed" });
        expect(
          (
            f.event("fileChanged", [{ path }], 100 + i).disposition as {
              status: string;
            }
          ).status,
        ).toBe("refused");
      }
      const observed: unknown[] = [];
      f.call(
        "request",
        { action: "isFullscreen" },
        (value: unknown) => observed.push(value),
        () => {},
      );
      const sent = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      f.event("mainResp", [{ reqId: sent.reqId, data: false }], 200);
      expect(observed).toEqual([false]);
      expect(f.sends()).toHaveLength(71);
    } finally {
      f.restore();
    }
  });
  test("denied same-path replacement retains the acknowledged callback owner", () => {
    const f = fixture();
    try {
      const path = "/test/keep-existing.xml";
      const released: number[] = [];
      f.page.document.addEventListener(
        "__keld_page_refusal_data_v1",
        (event) => {
          released.push((event as CustomEvent<number>).detail);
        },
      );
      let earlierEvents = 0;
      let deniedEvents = 0;
      const earlier = f.call(
        "request",
        { action: "watchFile", path, listener: () => earlierEvents++ },
        () => {},
        () => {},
      );
      const first = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      f.event("mainResp", [{ reqId: first.reqId, data: true }], 1);
      const denied = f.call(
        "request",
        { action: "watchFile", path, listener: () => deniedEvents++ },
        () => {},
        () => {},
      );
      const replacement = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      f.event("mainResp", [{ reqId: replacement.reqId, error: "denied" }], 2);
      expect(released).toContain(denied.call as number);
      expect(released).not.toContain(earlier.call as number);
      // An unacknowledged same-path replacement MUST NOT overwrite the
      // callback of the last acknowledged watch. Test delivery before another
      // successful registration can conceal the stale original map.
      expect(
        f.event(
          "fileChanged",
          [{ path, curr: { size: 1 }, prev: { size: 0 } }],
          3,
        ).disposition,
      ).toEqual({ status: "consumed" });
      expect(earlierEvents).toBe(1);
      expect(deniedEvents).toBe(0);
      let newestEvents = 0;
      f.call(
        "request",
        { action: "watchFile", path, listener: () => newestEvents++ },
        () => {},
        () => {},
      );
      const latest = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      f.event("mainResp", [{ reqId: latest.reqId, data: true }], 4);
      expect(
        f.event(
          "fileChanged",
          [{ path, curr: { size: 1 }, prev: { size: 0 } }],
          5,
        ).disposition,
      ).toEqual({ status: "consumed" });
      expect(newestEvents).toBe(1);
    } finally {
      f.restore();
    }
  });
  test("acknowledged watch replacement changes event owner despite a throwing response callback", () => {
    const f = fixture();
    try {
      const path = "/test/throwing-replacement.xml";
      let earlierEvents = 0;
      let replacementEvents = 0;
      f.call(
        "request",
        { action: "watchFile", path, listener: () => earlierEvents++ },
        () => {},
        () => {},
      );
      const first = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      expect(
        f.event("mainResp", [{ reqId: first.reqId, data: true }], 1)
          .disposition,
      ).toEqual({ status: "consumed" });
      f.call(
        "request",
        { action: "watchFile", path, listener: () => replacementEvents++ },
        () => {
          throw new Error("controlled original response callback failure");
        },
        () => {},
      );
      const second = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      expect(
        f.event("mainResp", [{ reqId: second.reqId, data: true }], 2)
          .disposition,
      ).toMatchObject({ status: "refused", code: "KELD-COMPAT-103" });
      // The Host's successful registration is committed even when the page's
      // response callback throws. The prior acknowledged callback is retired.
      expect(
        f.event(
          "fileChanged",
          [{ path, curr: { size: 2 }, prev: { size: 1 } }],
          3,
        ).disposition,
      ).toEqual({ status: "consumed" });
      expect(earlierEvents).toBe(0);
      expect(replacementEvents).toBe(1);
    } finally {
      f.restore();
    }
  });
  test("page-facing methods refuse Date and Map brands without erasing values", () => {
    const f = fixture();
    try {
      const page = f.page.electron as Record<
        string,
        (...args: unknown[]) => void
      >;
      const map = new Map([["preserved", 3]]);
      const erasedMap = new Map([["preserved", 4]]);
      Object.setPrototypeOf(erasedMap, Object.prototype);
      for (const method of [
        "request",
        "registerMsgListener",
        "sendMessage",
        "listenOnce",
      ]) {
        for (const input of [new Date(0), map, erasedMap]) {
          const before = f.pageMessages.length;
          expect(() => page[method]!(input)).toThrow("KELD-COMPAT-103");
          expect(f.pageMessages).toHaveLength(before);
        }
      }
      const guarded = Object.create(null) as object;
      let getterCalls = 0;
      Object.defineProperty(guarded, Symbol.toStringTag, {
        get() {
          getterCalls++;
          throw new Error("hostile tag getter");
        },
      });
      expect(() => page.request!(guarded)).toThrow("KELD-COMPAT-103");
      expect(getterCalls).toBe(0);
      page.sendMessage!({ plain: [1, null, { ok: true }] });
      expect(f.pageMessages.at(-1)?.args).toEqual([
        {
          k: "object",
          entries: [
            [
              "plain",
              {
                k: "array",
                values: [1, null, { k: "object", entries: [["ok", true]] }],
              },
            ],
          ],
        },
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
  test("sync original request failures release each call without retiring pending replies or acknowledged watches", () => {
    const f = fixture();
    const refused: number[] = [];
    f.page.document.addEventListener("__keld_page_refusal_data_v1", (event) => {
      refused.push((event as CustomEvent<number>).detail);
    });
    const previousError = console.error;
    let originalThrows = 0;
    console.error = (...values: unknown[]) => {
      if (values[0] === "KELD-COMPAT-103: original preload call refused")
        originalThrows++;
      else previousError(...values);
    };
    try {
      const pageErrors: string[] = [];
      for (let i = 0; i < 70; i++) {
        try {
          f.call(
            "request",
            null,
            () => {},
            () => {},
          );
        } catch (error) {
          pageErrors.push(String(error));
        }
      }
      expect(pageErrors).toEqual([]);
      expect(originalThrows).toBe(70); // Actual unmodified draw.io request(null) threw.
      expect(refused).toHaveLength(70);
      expect(new Set(refused).size).toBe(70);
      expect(f.sends()).toHaveLength(0);
      expect(
        f.appMessages.filter((value) => value.kind === "call-finished"),
      ).toHaveLength(70);

      const observed: unknown[] = [];
      f.call(
        "request",
        { action: "isFullscreen" },
        (value: unknown) => {
          observed.push(value);
        },
        () => {},
      );
      const first = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      f.event("mainResp", [{ reqId: first.reqId, data: false }], 1);
      expect(observed).toEqual([false]);

      const pending: unknown[] = [];
      f.call(
        "request",
        { action: "isFullscreen" },
        (value: unknown) => {
          pending.push(value);
        },
        () => {},
      );
      const pendingReq = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      const watchPath = "/test/preserved-after-throw.xml";
      let watchEvents = 0;
      f.call(
        "request",
        { action: "watchFile", path: watchPath, listener: () => watchEvents++ },
        () => {},
        () => {},
      );
      const watchReq = decodeEndpointArguments(
        new Uint8Array(f.sends().at(-1)!.payload as number[]),
      )[0] as { reqId: number };
      f.event("mainResp", [{ reqId: watchReq.reqId, data: true }], 2);

      f.call(
        "request",
        null,
        () => {},
        () => {},
      );
      expect(originalThrows).toBe(71);
      expect(refused).toHaveLength(72); // Valid response and failed call both release.
      expect(f.sends()).toHaveLength(3); // No native send for 71 failed calls.
      f.event("mainResp", [{ reqId: pendingReq.reqId, data: true }], 3);
      f.event("fileChanged", [{ path: watchPath, curr: 1, prev: 0 }], 4);
      expect(pending).toEqual([true]);
      expect(watchEvents).toBe(1);
    } finally {
      console.error = previousError;
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
  test("forged A2 page carrier cannot mint native authority before app binding", () => {
    const f = fixture(undefined, false);
    try {
      const before = f.appMessages.length;
      f.page.document.dispatchEvent(
        new CustomEvent("__keld_page_call_data_v1", {
          detail: JSON.stringify({
            v: 1,
            kind: "call",
            call: 900,
            method: "sendMessage",
            args: ["rendererReq", { action: "isFullscreen" }],
            document: "f".repeat(32),
          }),
        }),
      );
      expect(f.appMessages).toHaveLength(before);
      expect(f.pageMessages).toHaveLength(0);
      expect(f.sends()).toHaveLength(0);
      expect(Object.hasOwn(f.page, "__keldHostDocumentCredential")).toBe(false);
    } finally {
      f.restore();
    }
  });
  test("forged A2 carrier cannot send during Bun retirement and healthy recovery works", () => {
    const f = fixture(undefined, true, false);
    try {
      const spoofedDocument = "f".repeat(32);
      const dispatch = (value: unknown) =>
        f.page.document.dispatchEvent(
          new CustomEvent("__keld_page_call_data_v1", {
            detail: JSON.stringify(value),
          }),
        );
      const forged = {
        v: 1,
        kind: "call",
        call: 901,
        method: "request",
        args: [{ k: "object", entries: [["action", "isFullscreen"]] }],
        document: spoofedDocument,
      };
      const before = f.pageMessages.length;
      dispatch({ ...forged, method: "__keld_app_world" });
      dispatch({ ...forged, args: {} });
      expect(f.pageMessages).toHaveLength(before);

      // A2 is untrusted data, not a document token. The app world supplies its
      // own binding to native; native role admission is still a separate gate.
      dispatch(forged);
      expect(f.pageMessages).toHaveLength(before + 1);
      const offered = f.pageMessages.at(-1)!;
      expect(offered.document).toBe(token);
      expect(offered.document).not.toBe(spoofedDocument);
      expect(f.sends()).toHaveLength(0);

      // This fake invokes the private callback directly; production must
      // additionally check native frame/role/credit before invoking it.
      (f.app.__keldAppNativeCall as (value: unknown) => void)(offered);
      expect(f.sends()).toHaveLength(0);

      (f.app.__keldAppRouteState as (document: string, ready: boolean) => void)(
        spoofedDocument,
        true,
      );
      expect(f.sends()).toHaveLength(0);
      (f.app.__keldAppRouteState as (document: string, ready: boolean) => void)(
        token,
        true,
      );
      const values: unknown[] = [];
      f.call(
        "request",
        { action: "isFullscreen" },
        (value: unknown) => values.push(value),
        () => {},
      );
      expect(f.sends()).toHaveLength(1);
      const sent = decodeEndpointArguments(
        new Uint8Array(f.sends()[0]!.payload as number[]),
      )[0] as { reqId: number };
      f.event("mainResp", [{ reqId: sent.reqId, data: false }]);
      expect(values).toEqual([false]);
    } finally {
      f.restore();
    }
  });
});
