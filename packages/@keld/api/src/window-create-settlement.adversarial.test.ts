// Source-bound Host Opening controls for the production API/Link Create path.
// These are not signed installed native Quit acceptance.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const apiSource = readFileSync(new URL("./app.ts", import.meta.url), "utf8");
const linkSource = readFileSync(new URL("./link.ts", import.meta.url), "utf8");
const electronSource = readFileSync(
  new URL("../../electron/src/app.ts", import.meta.url),
  "utf8",
);

function section(source: string, start: string, end: string): string {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from);
  expect(from).toBeGreaterThan(-1);
  expect(to).toBeGreaterThan(from);
  return source.slice(from, to);
}

// Keep the actual production branch, settlement, counter and Quit-race code.
// Only the Host/socket and canonical codec are model inputs.
const linkWindow = section(
  linkSource,
  "  callWindow(request: WindowRequest): Promise<WindowResponse> {",
  "\n  /** One bounded guarded install",
)
  .replace("  callWindow(", "function linkWindow(")
  .replaceAll("this.#closed", "false")
  .replaceAll("this.#windowSource", "windowSource")
  .replaceAll("this.#dead", "undefined")
  .replaceAll("this.#link", "innerLink");

const createRegion = section(
  apiSource,
  "let windowCreateAttemptEpoch = 0n;",
  "/** Package-private FS invocation;",
).replaceAll("export function ", "function ");
const quitGuard = section(
  electronSource,
  "function createRacedWithQuit(",
  "\nfunction quit(): Promise<void>",
);
const code = new Bun.Transpiler({ loader: "ts" }).transformSync(
  [
    // These captures run at module evaluation, before application callbacks.
    "const trustedWindowLinkThen = Promise.prototype.then;",
    "const trustedWindowPromiseThen = Promise.prototype.then;",
    linkWindow,
    createRegion,
    quitGuard,
  ].join("\n"),
);

function fixture() {
  const context = vm.createContext(Object.create(null), {
    codeGeneration: { strings: false, wasm: false },
  });
  vm.runInContext(
    `
const stateApply = Reflect.apply;
const stateFreeze = Object.freeze;
const hostReady = true;
const WINDOW_CHANNEL = 9;
const APP_LINK_IO_DEADLINE_MS = 5000;
const windowSource = { acceptHostCloseReply() { return true; } };
const deliveredWindowCloses = { acceptHostCloseReply() {} };
function linkError(code, detail) { return new Error(code + ": " + detail); }
function encodeWindowRequest(value) { return JSON.stringify(value); }
function decodeWindowRequest(bytes) { return JSON.parse(bytes); }
function decodeWindowResponse(bytes) { return JSON.parse(bytes); }
let hostOpening = false;
let hostCalls = 0;
let spoofedThenCalls = 0;
let resolveRawHost;
let rejectRawHost;
const innerLink = {
  call() {
    hostCalls++;
    hostOpening = true;
    return new Promise((resolve, reject) => {
      resolveRawHost = (reply) => {
        hostOpening = false;
        resolve(JSON.stringify(reply));
      };
      rejectRawHost = (error) => {
        hostOpening = false;
        reject(error);
      };
    });
  },
};
${code}
const connectedLink = {
  windowSource,
  callWindow: linkWindow,
  callWindowBlocking() { throw new Error("not exercised"); },
};
const quitStartEpoch = observeWindowCreateActivity().epoch;
const createRequest = {
  variant: "Create", width: 640, height: 480, x: null, y: null,
};
`,
    context,
    { timeout: 1000 },
  );
  return {
    run<T>(source: string): T {
      return vm.runInContext(source, context, { timeout: 500 }) as T;
    },
  };
}

function drainReactions(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}

test("post-preload Promise.then cannot forge Create completion before Host reply", async () => {
  const f = fixture();
  f.run(`
    Promise.prototype.then = function(onFulfilled) {
      spoofedThenCalls++;
      if (typeof onFulfilled === "function")
        onFulfilled(JSON.stringify({ variant: "Created", window: { id: 7 } }));
      return Promise.resolve({ variant: "Created", window: { id: 7 } });
    };
    const outstandingCreate = invokeWindow(createRequest);
  `);
  expect(f.run<number>("hostCalls")).toBe(1);
  expect(f.run<boolean>("hostOpening")).toBe(true);
  expect(f.run<number>("spoofedThenCalls")).toBe(0);
  expect(f.run<number>("observeWindowCreateActivity().pending")).toBe(1);
  expect(f.run<boolean>("createRacedWithQuit(quitStartEpoch)")).toBe(true);

  f.run('resolveRawHost({ variant: "Created", window: { id: 7 } })');
  await drainReactions();
  expect(f.run<number>("observeWindowCreateActivity().pending")).toBe(0);
  expect(f.run<boolean>("hostOpening")).toBe(false);
  expect(f.run<number>("spoofedThenCalls")).toBe(0);
});

test("duplicate forged Promise callbacks cannot over-release a Create", async () => {
  const f = fixture();
  f.run(`
    Promise.prototype.then = function(onFulfilled) {
      spoofedThenCalls++;
      if (typeof onFulfilled === "function") {
        onFulfilled(JSON.stringify({ variant: "Created", window: { id: 7 } }));
        onFulfilled(JSON.stringify({ variant: "Created", window: { id: 7 } }));
      }
      return Promise.resolve({ variant: "Created", window: { id: 7 } });
    };
    const outstandingCreate = invokeWindow(createRequest);
  `);
  expect(f.run<number>("observeWindowCreateActivity().pending")).toBe(1);
  expect(f.run<number>("spoofedThenCalls")).toBe(0);
  f.run('resolveRawHost({ variant: "Created", window: { id: 7 } })');
  await drainReactions();
  expect(f.run<number>("observeWindowCreateActivity().pending")).toBe(0);
  expect(f.run<boolean>("createRacedWithQuit(quitStartEpoch)")).toBe(true);
});

test("genuine failed Host Create releases the reservation exactly once", async () => {
  const f = fixture();
  f.run(`
    const outstandingCreate = invokeWindow(createRequest);
    stateApply(trustedWindowPromiseThen, outstandingCreate, [
      () => { throw new Error("unexpected Create success"); },
      () => {},
    ]);
  `);
  expect(f.run<number>("observeWindowCreateActivity().pending")).toBe(1);
  f.run('rejectRawHost(new Error("Host refused native Create"))');
  await drainReactions();
  expect(f.run<number>("observeWindowCreateActivity().pending")).toBe(0);
  expect(f.run<boolean>("hostOpening")).toBe(false);
});
