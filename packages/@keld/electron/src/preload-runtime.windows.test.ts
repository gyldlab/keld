import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import * as runtime from "./preload-runtime";
import {
  encodeEndpointArguments,
  decodeEndpointArguments,
} from "@keld/kipc/endpoint-value-codec";

// GH1184 conformance: pure carrier/runtime evidence only. This port model does
// not prove native identity, grant checks, first-inline timing or post retirement.
const originalSource = JSON.parse(
  readFileSync(
    resolve(
      import.meta.dir,
      "../../../../crates/keld-wv/tests/fixtures/drawio-original-preload.json",
    ),
    "utf8",
  ),
) as string;
const token = "0123456789abcdef0123456789abcdef";
const stale = "ffffffffffffffffffffffffffffffff";
const facts: runtime.PreloadFacts = {
  argv: ["--initial-adaptive-colors=auto"],
  versions: { keld: "captured-host-facts", bun: "1.4.4" },
};
type Message = Record<string, unknown>;
type Original = Parameters<typeof runtime.installApp>[1];
// The baseline has no Windows entry point. The same tests are retained after
// adding it; negative controls separately falsify actual transport behavior.
const windows = runtime as unknown as {
  installWindows: (facts: runtime.PreloadFacts, original: Original) => void;
  decodeHostRendererMessage: (text: unknown) => unknown;
};
const carrier = (kind: string, fields: Message = {}, document = token) =>
  JSON.stringify({ v: 2, document, kind, ...fields });

function fixture(
  options: { frame?: boolean; missingPort?: boolean; numeric?: boolean } = {},
) {
  const previous = globalThis.window;
  const dom = new EventTarget();
  const lifecycle = new EventTarget();
  const posted: Message[] = [];
  const receivedReplies: unknown[] = [];
  let postFailure = false;
  const stored = new Map<string, string>();
  const callbacks = new Set<
    (event: { data: unknown; source: unknown }) => void
  >();
  const port = {
    postMessage(text: string) {
      if (postFailure)
        throw new Error("actual captured port refused this post");
      posted.push(JSON.parse(text) as Message);
    },
    addEventListener(
      _name: string,
      fn: (event: { data: unknown; source: unknown }) => void,
    ) {
      callbacks.add(fn);
    },
    removeEventListener(
      _name: string,
      fn: (event: { data: unknown; source: unknown }) => void,
    ) {
      callbacks.delete(fn);
    },
  };
  const page = {
    document: dom,
    addEventListener: lifecycle.addEventListener.bind(lifecycle),
    localStorage: {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => stored.set(key, value),
    },
    chrome: options.missingPort ? undefined : { webview: port },
    __keldBridgeNativeResult: options.numeric
      ? (value: unknown) => receivedReplies.push(value)
      : undefined,
  } as unknown as Window & Record<string, unknown>;
  Object.defineProperty(page, "top", { value: options.frame ? {} : page });
  Object.defineProperty(globalThis, "window", {
    value: page,
    writable: true,
    configurable: true,
  });
  let entered = 0;
  let originalProcess: Record<string, unknown> | undefined;
  const body = new Function(
    "require",
    "process",
    "exports",
    "module",
    "Buffer",
    "global",
    "setImmediate",
    "clearImmediate",
    originalSource,
  ) as Original;
  const original: Original = (...args) => {
    entered++;
    originalProcess = args[1] as Record<string, unknown>;
    body(...args);
  };
  const receive = (text: unknown, source: unknown = port) => {
    for (const fn of callbacks) fn({ data: text, source });
  };
  const restore = () =>
    Object.defineProperty(globalThis, "window", {
      value: previous,
      writable: true,
      configurable: true,
    });
  return {
    page,
    port,
    dom,
    posted,
    stored,
    receivedReplies,
    receive,
    restore,
    pagehide: () => lifecycle.dispatchEvent(new Event("pagehide")),
    setPostFailure: (value: boolean) => {
      postFailure = value;
    },
    install: () => windows.installWindows(facts, original),
    entered: () => entered,
    process: () => originalProcess,
    bind: () => receive(carrier("bound", { ready: true })),
    register: () => receive(carrier("app-registered")),
    call(name: string, ...args: unknown[]) {
      (page.electron as Record<string, (...values: unknown[]) => void>)[name]!(
        ...args,
      );
      const call = posted.filter((value) => value.kind === "call").at(-1)!;
      receive(
        carrier("app-call", {
          call: call.call,
          method: call.method,
          args: call.args,
        }),
      );
      return call;
    },
    event(channel: string, args: unknown[], delivery = 1, document = token) {
      receive(
        carrier(
          "event",
          {
            delivery,
            channel,
            payload: Array.from(encodeEndpointArguments(args)),
          },
          document,
        ),
      );
    },
  };
}

describe("GH1184 explicit Windows Legacy preload transport", () => {
  test("top-frame guard precedes original entry and facade publication", () => {
    const f = fixture({ frame: true });
    try {
      f.install();
      expect(f.entered()).toBe(0);
      expect(Object.hasOwn(f.page, "electron")).toBe(false);
      expect(Object.hasOwn(f.page, "process")).toBe(false);
      expect(f.posted).toEqual([]);
    } finally {
      f.restore();
    }
  });
  test("the existing Mac entry point retains its isolated realm fact", () => {
    const f = fixture();
    try {
      f.page.webkit = { messageHandlers: { __keld_app_world: f.port } };
      let isolated: unknown;
      runtime.installApp(facts, (_require, process) => {
        isolated = (process as { contextIsolated: unknown }).contextIsolated;
      });
      expect(isolated).toBe(true);
    } finally {
      f.restore();
    }
  });
  test("missing Windows port refuses before publishing a facade", () => {
    const f = fixture({ missingPort: true });
    try {
      expect(f.install).toThrow("KELD-COMPAT-103");
      expect(f.entered()).toBe(0);
      expect(Object.hasOwn(f.page, "electron")).toBe(false);
    } finally {
      f.restore();
    }
  });
  test("unchanged original runs once with explicit Legacy facts and current registration", () => {
    const f = fixture();
    try {
      f.install();
      expect(f.entered()).toBe(1);
      expect(f.process()?.contextIsolated).toBe(false);
      expect(f.process()?.argv).toEqual(facts.argv);
      expect(
        JSON.parse(f.stored.get(".configuration")!).defaultAdaptiveColors,
      ).toBe("auto");
      expect(f.posted).toEqual([{ v: 2, kind: "bind" }]);
      const early = (
        f.page.electron as { request: (...args: unknown[]) => void }
      ).request;
      early(
        { action: "isFullscreen" },
        () => {},
        () => {},
      );
      expect(f.posted.filter((value) => value.kind === "call")).toHaveLength(0);
      f.bind();
      expect(f.posted.at(-1)).toEqual({
        v: 1,
        kind: "registered",
        document: token,
        methods: [
          "request",
          "registerMsgListener",
          "sendMessage",
          "listenOnce",
        ],
      });
      const before = f.posted.length;
      f.bind();
      f.receive(carrier("app-registered", {}, stale));
      early(
        { action: "isFullscreen" },
        () => {},
        () => {},
      );
      expect(f.posted).toHaveLength(before);
      f.register();
      let value: unknown;
      const original = { action: "isFullscreen" };
      f.call(
        "request",
        original,
        (answer: unknown) => {
          value = answer;
        },
        () => {},
      );
      expect(Object.hasOwn(original, "reqId")).toBe(false);
      const sent = f.posted.find((item) => item.kind === "send")!;
      const request = decodeEndpointArguments(
        new Uint8Array(sent.payload as number[]),
      )[0] as { reqId: number };
      expect(sent.document).toBe(token);
      expect(sent.channel).toBe("rendererReq");
      f.event("mainResp", [{ reqId: request.reqId, data: false }]);
      expect(value).toBe(false);
      expect(f.posted.at(-1)).toEqual({
        v: 2,
        kind: "consumed",
        document: token,
        delivery: 1,
        disposition: { status: "consumed" },
      });
      expect(
        f.posted.filter((item) => item.kind === "call-finished"),
      ).toHaveLength(1);
    } finally {
      f.restore();
    }
  });
  test("captured transport and handlers survive page-hook replacement; stale carriers have no effect", () => {
    const f = fixture();
    try {
      f.install();
      f.bind();
      f.register();
      f.port.postMessage = () => {
        throw new Error("replaced transport must not be called");
      };
      f.page.__keldAppNativeEvent = () => {
        throw new Error("replaced page hook");
      };
      f.page.__keldAppNativeCall = () => {
        throw new Error("replaced page hook");
      };
      const values: unknown[] = [];
      f.call("registerMsgListener", "local", (value: unknown) =>
        values.push(value),
      );
      const before = f.posted.length;
      f.event("local", ["stale"], 1, stale);
      f.receive(carrier("route-state", { ready: false }, stale));
      f.receive(
        carrier("event", { delivery: 1, channel: "local", payload: [] }),
        {},
      );
      expect(values).toEqual([]);
      expect(f.posted).toHaveLength(before);
      f.event("local", ["current"], 2);
      expect(values).toEqual(["current"]);
      f.receive(
        carrier("send-result", {
          message: 1,
          disposition: { status: "route-retired", certainty: "not-dispatched" },
        }),
      );
      f.event("local", ["retired"], 3);
      expect(values).toEqual(["current"]);
      expect((f.posted.at(-1)?.disposition as Message).status).toBe("refused");
    } finally {
      f.restore();
    }
  });
  test("completed Windows calls release callback capacity and refuse replayed original responses", () => {
    const f = fixture();
    try {
      f.install();
      f.bind();
      f.register();
      let callbacks = 0;
      let latest = 0;
      for (let i = 0; i < 100; i++) {
        f.call(
          "request",
          { action: "isFullscreen" },
          () => {
            callbacks++;
          },
          () => {},
        );
        const sent = f.posted.filter((item) => item.kind === "send").at(-1)!;
        latest = (
          decodeEndpointArguments(
            new Uint8Array(sent.payload as number[]),
          )[0] as { reqId: number }
        ).reqId;
        f.event("mainResp", [{ reqId: latest, data: false }], i + 1);
      }
      expect(callbacks).toBe(100);
      expect(
        f.posted.filter((item) => item.kind === "call-finished"),
      ).toHaveLength(100);
      f.event("mainResp", [{ reqId: latest, data: true }], 101);
      expect(callbacks).toBe(100);
      expect((f.posted.at(-1)?.disposition as Message).status).toBe("refused");
    } finally {
      f.restore();
    }
  });
  test("numeric replies use only the captured existing consumer and current nonce", () => {
    const f = fixture({ numeric: true });
    try {
      f.install();
      f.bind();
      f.register();
      f.page.__keldBridgeNativeResult = () => {
        throw new Error("replacement hook");
      };
      f.receive(
        carrier("reply", { request: 9, ok: true, payload: [1, 2] }, stale),
      );
      expect(f.receivedReplies).toEqual([]);
      f.receive(carrier("reply", { request: 9, ok: true, payload: [1, 2] }));
      f.receive(
        carrier("reply", {
          request: 10,
          ok: false,
          code: "KELD-IPC-001",
          detail: "refused",
        }),
      );
      expect(f.receivedReplies).toEqual([
        { request: 9, ok: true, payload: [1, 2] },
        { request: 10, ok: false, code: "KELD-IPC-001", detail: "refused" },
      ]);
    } finally {
      f.restore();
    }
    const absent = fixture();
    try {
      absent.install();
      absent.bind();
      absent.register();
      const before = absent.posted.length;
      absent.receive(carrier("reply", { request: 1, ok: true, payload: [] }));
      expect(absent.posted).toHaveLength(before);
    } finally {
      absent.restore();
    }
  });
});

describe("GH1184 closed HostRendererMessage carrier", () => {
  const decode = (text: unknown) => windows.decodeHostRendererMessage(text);
  test("all nine closed rows preserve exact JSON values", () => {
    const rows = [
      ["bound", { ready: false }],
      ["event", { delivery: 7, channel: "mainResp", payload: [0, 255] }],
      ["send-result", { message: 2, disposition: { status: "consumed" } }],
      ["route-state", { ready: true }],
      ["app-registered", {}],
      ["app-refused", { call: 3 }],
      [
        "app-call",
        {
          call: 4,
          method: "listenOnce",
          args: ["local", { k: "callback", id: 1 }],
        },
      ],
      ["reply", { request: 3, ok: true, payload: [1] }],
      [
        "reply",
        { request: 3, ok: false, code: "KELD-IPC-001", detail: "refused" },
      ],
    ] as const;
    for (const [kind, fields] of rows) {
      const raw = carrier(kind, fields);
      expect(decode(raw)).toEqual(JSON.parse(raw));
      expect(decode(raw.slice(0, -1) + ',"extra":null}')).toBeUndefined();
      expect(decode(raw.replace('"v":2', '"v":null'))).toBeUndefined();
      expect(decode(raw.replace(token, "not-a-native-epoch"))).toBeUndefined();
    }
  });
  test("duplicates, aliases, nulls, foreign kinds and malformed Local graphs refuse", () => {
    for (const raw of [
      '{"v":2,"v":2,"document":"' + token + '","kind":"bound","ready":true}',
      '{"v":2,"\\u0076":2,"document":"' +
        token +
        '","kind":"bound","ready":true}',
      carrier("bound", { ready: null }),
      carrier("bound", { ready: true, payload: [] }),
      carrier("bind"),
      carrier("invoke", { request: 1, channel: 1, payload: [] }),
      carrier("app-call", {
        call: 1,
        method: "request",
        args: [{ k: "undefined", extra: null }],
      }),
      carrier("app-call", {
        call: 1,
        method: "request",
        args: [
          {
            k: "object",
            entries: [
              ["x", 1],
              ["x", 2],
            ],
          },
        ],
      }),
      carrier("app-call", {
        call: 1,
        method: "request",
        args: [{ k: "callback", id: 0 }],
      }),
      carrier("app-call", { call: 1, method: "request", args: [[1]] }),
      carrier("app-call", { call: 1, method: "eval", args: [] }),
      carrier("event", { delivery: 0, channel: "x", payload: [] }),
      carrier("event", { delivery: 1, channel: "*", payload: [] }),
      carrier("event", { delivery: 1, channel: "x", payload: [256] }),
      carrier("reply", {
        request: 1,
        ok: false,
        code: "KELD-IPC-001",
        detail: null,
      }),
      carrier("reply", { request: 1, ok: true, payload: [], detail: "extra" }),
      carrier("send-result", {
        message: 1,
        disposition: { status: "consumed", code: null },
      }),
      carrier("send-result", {
        message: 1,
        disposition: { status: "route-retired", certainty: "maybe" },
      }),
      carrier("send-result", {
        message: 1,
        disposition: { status: "invented" },
      }),
    ])
      expect(decode(raw)).toBeUndefined();
    expect(decode({ v: 2 })).toBeUndefined();
    expect(decode("")).toBeUndefined();
    expect(
      decode(
        carrier("app-call", {
          call: 1,
          method: "request",
          args: ['"v":2,{[]}'],
        }),
      ),
    ).toBeDefined();
  });
  test("payload, control and Local bounds are independent", () => {
    expect(
      decode(
        carrier("event", {
          delivery: 1,
          channel: "x",
          payload: Array(4096).fill(255),
        }),
      ),
    ).toBeDefined();
    expect(
      decode(
        carrier("event", {
          delivery: 1,
          channel: "x",
          payload: Array(4097).fill(0),
        }),
      ),
    ).toBeUndefined();
    expect(
      decode(
        carrier("reply", {
          request: 1,
          ok: false,
          code: "KELD-IPC-001",
          detail: "x".repeat(4096),
        }),
      ),
    ).toBeUndefined();
    expect(
      decode(
        carrier("app-call", {
          call: 1,
          method: "request",
          args: ["x".repeat(65536)],
        }),
      ),
    ).toBeDefined();
    expect(
      decode(
        carrier("app-call", {
          call: 1,
          method: "request",
          args: ["x".repeat(65537)],
        }),
      ),
    ).toBeUndefined();
    expect(
      decode(
        carrier("app-call", {
          call: 1,
          method: "request",
          args: Array(17).fill(null),
        }),
      ),
    ).toBeUndefined();
  });
});

type NumericFacade = {
  invoke(
    channel: unknown,
    payload: unknown,
    opts?: unknown,
  ): Promise<Uint8Array>;
};
const numericFacade = (f: ReturnType<typeof fixture>) =>
  f.page.keld as NumericFacade;

describe("Windows fixed numeric Echo facade", () => {
  test("queues one copied Echo request until accepted binding and settles only its closed reply", async () => {
    const f = fixture({ numeric: true });
    try {
      f.install();
      const facade = numericFacade(f);
      expect(Object.isFrozen(facade)).toBe(true);
      expect(Object.isFrozen(facade.invoke)).toBe(true);
      expect(Object.keys(facade)).toEqual(["invoke"]);
      expect(Object.getOwnPropertyDescriptor(f.page, "keld")).toMatchObject({
        writable: false,
        configurable: false,
        enumerable: true,
      });
      expect(Object.hasOwn(f.page, "__keldBridgeNativeBind")).toBe(false);
      const input = new Uint8Array([4, 5]);
      const first = facade.invoke(1, input);
      input[0] = 99;
      expect(f.posted.filter((value) => value.kind === "invoke")).toHaveLength(
        0,
      );
      await expect(facade.invoke(1, new Uint8Array())).rejects.toThrow(
        "already pending",
      );
      f.bind();
      const request = f.posted.find((value) => value.kind === "invoke")!;
      expect(request).toEqual({
        v: 2,
        kind: "invoke",
        document: token,
        request: 1,
        channel: 1,
        payload: [4, 5],
      });
      expect(
        f.posted.findIndex((value) => value.kind === "registered"),
      ).toBeLessThan(f.posted.indexOf(request));
      f.page.__keldBridgeNativeResult = () => {
        throw new Error("late global replacement");
      };
      f.receive(
        carrier("reply", { request: 1, ok: true, payload: [8] }, stale),
      );
      f.receive(carrier("reply", { request: 1, ok: true, payload: [8] }), {});
      f.receive(carrier("reply", { request: 2, ok: true, payload: [8] }));
      await expect(facade.invoke(1, new Uint8Array())).rejects.toThrow(
        "already pending",
      );
      f.receive(carrier("reply", { request: 1, ok: true, payload: [7, 6] }));
      expect(await first).toEqual(new Uint8Array([7, 6]));
      expect(f.receivedReplies.at(-1)).toEqual({
        request: 1,
        ok: true,
        payload: [7, 6],
      });

      const second = facade.invoke(1, new Uint8Array([3]));
      expect(f.posted.at(-1)?.request).toBe(2);
      f.receive(carrier("reply", { request: 1, ok: true, payload: [0] }));
      await expect(facade.invoke(1, new Uint8Array())).rejects.toThrow(
        "already pending",
      );
      f.receive(
        carrier("reply", {
          request: 2,
          ok: false,
          code: "KELD-GUARD001",
          detail: "denied",
        }),
      );
      await expect(second).rejects.toThrow("KELD-GUARD001: denied");
    } finally {
      f.restore();
    }
  });

  test("refuses channels, options, payload types and overflow without native invocation", async () => {
    const f = fixture();
    try {
      f.install();
      const facade = numericFacade(f);
      for (const channel of [0, 2, 65536, "1", NaN])
        await expect(facade.invoke(channel, new Uint8Array())).rejects.toThrow(
          "not declared",
        );
      await expect(facade.invoke(1, [1])).rejects.toThrow("Uint8Array");
      await expect(facade.invoke(1, new Uint8Array(4097))).rejects.toThrow(
        "4096",
      );
      await expect(facade.invoke(1, new Uint8Array(), {})).rejects.toThrow(
        "opts",
      );
      expect(f.posted.filter((value) => value.kind === "invoke")).toHaveLength(
        0,
      );
      f.bind();
      const result = facade.invoke(1, new Uint8Array(4096).fill(7));
      const request = f.posted.at(-1)!;
      expect((request.payload as number[]).length).toBe(4096);
      f.receive(
        carrier("reply", {
          request: 1,
          ok: true,
          payload: new Array(4097).fill(0),
        }),
      );
      await expect(facade.invoke(1, new Uint8Array())).rejects.toThrow(
        "already pending",
      );
      f.receive(carrier("reply", { request: 1, ok: true, payload: [] }));
      expect(await result).toEqual(new Uint8Array());
    } finally {
      f.restore();
    }
  });

  test("pagehide rejects queued or dispatched work and prevents late binding or replies", async () => {
    for (const bound of [false, true]) {
      const f = fixture();
      try {
        f.install();
        if (bound) f.bind();
        const facade = numericFacade(f);
        const pending = facade.invoke(1, new Uint8Array([2]));
        f.pagehide();
        await expect(pending).rejects.toThrow("document navigated");
        const calls = f.posted.filter(
          (value) => value.kind === "invoke",
        ).length;
        f.bind();
        f.receive(carrier("reply", { request: 1, ok: true, payload: [9] }));
        await expect(facade.invoke(1, new Uint8Array())).rejects.toThrow(
          "document navigated",
        );
        expect(
          f.posted.filter((value) => value.kind === "invoke"),
        ).toHaveLength(calls);
      } finally {
        f.restore();
      }
    }
  });

  test("a native post refusal releases only that request and never reuses its identifier", async () => {
    const f = fixture();
    try {
      f.install();
      f.bind();
      const facade = numericFacade(f);
      f.setPostFailure(true);
      await expect(facade.invoke(1, new Uint8Array([1]))).rejects.toThrow(
        "native post failed",
      );
      f.setPostFailure(false);
      const next = facade.invoke(1, new Uint8Array([2]));
      expect(f.posted.at(-1)?.request).toBe(2);
      f.receive(carrier("reply", { request: 1, ok: true, payload: [0] }));
      await expect(facade.invoke(1, new Uint8Array())).rejects.toThrow(
        "already pending",
      );
      f.receive(carrier("reply", { request: 2, ok: true, payload: [2] }));
      expect(await next).toEqual(new Uint8Array([2]));
    } finally {
      f.restore();
    }
  });
});
