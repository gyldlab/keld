// GH451 / GH531 criterion 34: RED source-only control against the original
// quit-facade functions. This is NOT a Host, AppKit or signed-app acceptance.
// The physical Host steps are controlled inputs; no native milestone is forged.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(new URL("./app.ts", import.meta.url), "utf8");
function region(startText: string, endText: string): string {
  const start = source.indexOf(startText);
  const end = source.indexOf(endText, start);
  expect(start).toBeGreaterThan(-1);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}
const sourceBodies = [
  region(
    "function emitCancelable(",
    "\n/** Electron event names and application listeners",
  ),
  region("function onLastWindowClosed(): void {", "\n// One facade attempt"),
  region(
    "interface QuitAttempt {",
    "\napiApp.onLastWindowClosed(onLastWindowClosed);",
  ).replace(
    "export function joinOriginalQuitBeforeClose",
    "function joinOriginalQuitBeforeClose",
  ),
].join("\n");
const executable = new Bun.Transpiler({ loader: "ts" }).transformSync(
  sourceBodies,
);

function fixture() {
  const ctx = vm.createContext(Object.create(null), {
    codeGeneration: { strings: false, wasm: false },
  });
  vm.runInContext(
    `
// Source-only Host model: one real-window identity, current CloseRequested
// sequence, independently observed Destroyed/LastWindowClosed, and optional
// replacement. A CloseRequested alone NEVER proves Destroyed or Allow.
const trace = [];
let nativeAlive = true;
let replacementAlive = false;
let pendingClose = null;
let requestCalls = 0;
let terminalCalls = 0;
let resolveClose;
let rejectClose;
const selectedWindow = { id: 7, incarnation: 258n };
const stateDefineProperty = Object.defineProperty;
const nativeFacadeBootSelected = true;
const apiApp = {
  isReady: () => true,
  quit: () => { terminalCalls++; trace.push("terminal"); return Promise.resolve(); },
};
function emit(name) { trace.push(name); return false; }
function hasListeners(name) { return name === "window-all-closed"; }
function liveWindowSnapshots() {
  return [...(nativeAlive ? [{window: selectedWindow, host_destroyed: false}] : []),
          ...(replacementAlive ? [{window: {id: 8, incarnation: 259n}, host_destroyed: false}] : [])];
}
const scope = {
  window: selectedWindow,
  isCurrentPending(ref, sequence) {
    return nativeAlive && ref.id === selectedWindow.id &&
      ref.incarnation === selectedWindow.incarnation && pendingClose === sequence;
  },
  dispatchedPendingClose() { return null; },
  requestClose() {
    requestCalls++;
    return new Promise((resolve, reject) => {
      resolveClose = resolve;
      rejectClose = reject;
    });
  },
  failAfterHandoff(error) { throw error; },
};
function captureOriginalWindowQuitScope(ref) {
  return nativeAlive && ref.id === selectedWindow.id &&
    ref.incarnation === selectedWindow.incarnation ? scope : undefined;
}
function linkError(code, detail) { return new Error(code + ": " + detail); }
function ignoreIfUnawaited() {}
function observeReady() { return Promise.resolve(); }
${executable}
    `,
    ctx,
    { timeout: 500 },
  );
  return {
    run<T = unknown>(code: string): T {
      return vm.runInContext(code, ctx, { timeout: 500 }) as T;
    },
    read<T = unknown>(expression: string): T {
      return JSON.parse(
        vm.runInContext(`JSON.stringify(${expression})`, ctx, {
          timeout: 500,
        }),
      ) as T;
    },
  };
}

test("GH531-34 RED: matching CloseRequested cannot lift quit suppression before Host Destroyed", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  expect(f.read("trace")).toEqual(["before-quit"]);
  expect(f.read("requestCalls")).toBe(1);
  expect(f.read("terminalCalls")).toBe(0);

  // Authenticated Host has only requested a close. It has NOT received the
  // app's Allow/Veto reply and has NOT issued its actual Destroyed receipt.
  f.run("pendingClose = 3n");
  expect(f.run("joinOriginalQuitBeforeClose(selectedWindow, 3n)")).toBe(true);
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;

  // Host CloseReply(Allow) -> physical Destroyed -> LastWindowClosed.
  f.run("nativeAlive = false; onLastWindowClosed()");
  expect({
    trace: f.read("trace"),
    terminalCalls: f.read("terminalCalls"),
  }).toEqual({ trace: ["before-quit"], terminalCalls: 0 });
  // When full GH451 choreography is approved, a separate owner must verify
  // will-quit, terminal Quit and native teardown; none is claimed here.
});

test("GH531-34 RED: RequestClose RPC success alone is not native Destroyed", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  // An RPC reply says Host accepted/requested the close, not that a physical
  // release occurred. Deliberately do NOT dispatch the native CloseRequested
  // callback first: the two evidence channels have separate delivery order.
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  expect(f.read("requestCalls")).toBe(1);
  expect(f.read("terminalCalls")).toBe(0);
  f.run("nativeAlive = false; onLastWindowClosed()");
  expect({
    trace: f.read("trace"),
    terminalCalls: f.read("terminalCalls"),
  }).toEqual({ trace: ["before-quit"], terminalCalls: 0 });
});

test("Host rejection before CloseRequested leaves no false terminal and permits retry", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("rejectClose(new Error('controlled Host RequestClose denial'))");
  await expect(first).rejects.toThrow("controlled Host RequestClose denial");
  expect(f.read("trace")).toEqual(["before-quit"]);
  expect(f.read("terminalCalls")).toBe(0);
  const second = f.run<Promise<void>>("quit()");
  expect(second).not.toBe(first);
  f.run("rejectClose(new Error('second controlled denial'))");
  await expect(second).rejects.toThrow("second controlled denial");
  expect(f.read("trace")).toEqual(["before-quit", "before-quit"]);
});

test("controlled Host Veto keeps dirty window; later user close may emit window-all-closed", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("pendingClose = 3n");
  expect(f.run("joinOriginalQuitBeforeClose(selectedWindow, 3n)")).toBe(true);
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  // A dirty-document preventDefault causes Host CloseReply(Veto). No Destroyed
  // and no LastWindowClosed may be manufactured at this point.
  f.run("pendingClose = null");
  expect(f.read("trace")).toEqual(["before-quit"]);
  expect(f.read("terminalCalls")).toBe(0);
  // Only a separate successful user close, outside the vetoed Quit, creates
  // the actual Host Destroyed and LastWindowClosed used here.
  f.run("nativeAlive = false; onLastWindowClosed()");
  expect(f.read("trace")).toEqual(["before-quit", "window-all-closed"]);
});

test("a replacement window blocks stale LastWindowClosed and terminal Quit", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("pendingClose = 3n");
  expect(f.run("joinOriginalQuitBeforeClose(selectedWindow, 3n)")).toBe(true);
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  f.run("nativeAlive = false; replacementAlive = true; onLastWindowClosed()");
  expect(f.read("trace")).toEqual(["before-quit"]);
  expect(f.read("terminalCalls")).toBe(0);
});
