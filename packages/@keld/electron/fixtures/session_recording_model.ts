/** Local facade model only. Mocks deliberately bypass producer trust; never native proof. */
import { mock } from "bun:test";
const phase = process.argv[2];
let ready = phase !== "pre-ready";
let sourceReads = 0;
let listenerCalls = 0;
const diagnostics: string[] = [];
console.warn = (value: unknown) => {
  if (phase === "diagnostic-fault")
    throw Object.assign(new Error("diagnostic sink failed"), {
      code: "fixture-diagnostic-sink",
    });
  diagnostics.push(String(value));
};
const claim = Object.freeze(
  Object.assign(Object.create(null), {
    contract: "gh473-original-editor-offline/v1",
    graphSha256: `sha256:${"1".repeat(64)}`,
    policySha256: `sha256:${"2".repeat(64)}`,
    windowKey: "editor",
    selector: "original-editor-offline",
  }),
);
mock.module("@keld/kipc", () => ({
  hostLaunchFacts:
    phase === "missing"
      ? { schema: 4 }
      : phase === "missing-descriptor"
        ? { schema: 5 }
        : { schema: 5, originalResourcePolicy: claim },
}));
mock.module("@keld/api/internal/app", () => ({
  app: { isReady: () => ready },
  isDesktopS1Selected: () => true,
  getDesktopSnapshot() {
    sourceReads++;
    if (phase === "dead-source")
      throw Object.assign(new Error("source ended"), { code: "KELD-IPC-005" });
    return { primary_id: 1 };
  },
}));
const { session } = await import("../src/session.ts");
function caught(action: () => unknown): unknown {
  try {
    return {
      status: "returned",
      value: action() === undefined ? "undefined" : "object",
    };
  } catch (error) {
    if (!(error instanceof Error)) throw error;
    return {
      status: "threw",
      code: Reflect.get(error, "code"),
      member: Reflect.get(error, "member"),
      ticket: Reflect.get(error, "ticket"),
    };
  }
}
if (phase === "pre-ready") {
  console.log(
    JSON.stringify({
      result: caught(() => session.defaultSession),
      diagnostics,
      sourceReads,
    }),
  );
} else {
  const actual = session.defaultSession;
  const request = actual.webRequest;
  const weak: WeakRef<(...args: never[]) => unknown>[] = [];
  const results: unknown[] = [];
  if (phase === "invalid") {
    let getters = 0;
    const filter = Object.defineProperty({}, "urls", {
      get() {
        getters++;
        return [];
      },
      enumerable: true,
    });
    results.push(
      caught(() => Reflect.apply(request.onHeadersReceived, request, [17])),
      caught(() =>
        Reflect.apply(request.onBeforeRequest, request, [
          filter,
          () => {
            listenerCalls++;
          },
        ]),
      ),
    );
    console.log(
      JSON.stringify({ results, getters, listenerCalls, diagnostics }),
    );
    process.exit(0);
  }
  if (phase === "null") {
    results.push(
      caught(() => request.onHeadersReceived(null)),
      caught(() => request.onBeforeRequest({ urls: ["file://*"] }, null)),
    );
  } else {
    (() => {
      const headers = (..._args: never[]) => {
        listenerCalls++;
      };
      const before = (..._args: never[]) => {
        listenerCalls++;
      };
      weak.push(new WeakRef(headers), new WeakRef(before));
      results.push(
        caught(() => request.onHeadersReceived(headers)),
        caught(() => request.onBeforeRequest({ urls: ["file://*"] }, before)),
      );
    })();
  }
  const unsupported: Record<string, unknown> = {};
  for (const method of [
    "onBeforeSendHeaders",
    "onSendHeaders",
    "onResponseStarted",
    "onBeforeRedirect",
    "onCompleted",
    "onErrorOccurred",
  ] as const)
    unsupported[method] = caught(() => request[method]());
  await new Promise<void>((resolve) => setImmediate(resolve));
  Bun.gc(true);
  await new Promise<void>((resolve) => setImmediate(resolve));
  Bun.gc(true);
  console.log(
    JSON.stringify({
      stable:
        actual === session.defaultSession && request === actual.webRequest,
      results,
      unsupported,
      listenerCalls,
      retained: weak.map((reference) => reference.deref() !== undefined),
      diagnostics,
      sourceReads,
    }),
  );
}
