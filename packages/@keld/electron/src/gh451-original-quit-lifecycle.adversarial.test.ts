// GH451 / GH531 criterion 34: failure-first source-only control against the
// original quit-facade functions. This is NOT signed-app acceptance.
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
let sourceBodies = [
  region(
    "function emitCancelable(",
    "\n/** Electron event names and application listeners",
  ),
  region("function onLastWindowClosed(): void {", "\n// One facade attempt"),
  region(
    "interface QuitAttempt {",
    "\napiApp.onLastWindowClosed(onLastWindowClosed);",
  ).replaceAll("export function ", "function "),
  region("onWindowFactApplied((fact) => {", "\nonNativeQuitRequested(() => {"),
].join("\n");
if (process.env.KELD_GH451_MUTANT === "drop-close-suppression")
  sourceBodies = sourceBodies.replace(
    "const original = pendingOriginalQuitClose;",
    "const original = undefined;",
  );
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
let generationCurrent = true;
let documentCurrent = true;
let vetoWillWithTemporaryClose = false;
let vetoBeforeQuit = false;
let enqueueRetryAfterWillVeto = false;
let queuedRetry = null;
let requestCalls = 0;
let terminalCalls = 0;
let resolveClose;
let rejectClose;
const selectedWindow = { id: 7, incarnation: 258n };
let windowFactCallback = () => {};
function onWindowFactApplied(callback) { windowFactCallback = callback; }
const stateDefineProperty = Object.defineProperty;
const nativeFacadeBootSelected = true;
function observeWindowCreateActivity() { return { epoch: 0n, pending: 0 }; }
const apiApp = {
  isReady: () => true,
  quit: () => { terminalCalls++; trace.push("terminal"); return Promise.resolve(); },
};
function emit(name, event) {
  trace.push(name);
  if (name === "before-quit" && vetoBeforeQuit) {
    if (event.defaultPrevented !== false) throw new Error("forged defaultPrevented before veto");
    event.preventDefault();
    if (event.defaultPrevented !== true) throw new Error("forged defaultPrevented after veto");
  }
  if (name === "will-quit" && vetoWillWithTemporaryClose) {
    replacementAlive = true;
    replacementAlive = false;
    onLastWindowClosed();
    event.preventDefault();
  }
  if (name === "will-quit" && enqueueRetryAfterWillVeto) {
    enqueueRetryAfterWillVeto = false;
    Promise.resolve().then(() => { queuedRetry = quit(); });
    event.preventDefault();
  }
  return false;
}
function hasListeners(name) { return name === "window-all-closed"; }
function liveWindowSnapshots() {
  return [...(nativeAlive ? [{window: selectedWindow, host_destroyed: false}] : []),
          ...(replacementAlive ? [{window: {id: 8, incarnation: 259n}, host_destroyed: false}] : [])];
}
const scope = {
  window: selectedWindow,
  ownsCurrentGeneration() { return generationCurrent && documentCurrent; },
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

test("GH531-34: matching CloseRequested cannot lift quit suppression before Host Destroyed", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  expect(f.read<string[]>("trace")).toEqual(["before-quit"]);
  expect(f.read<number>("requestCalls")).toBe(1);
  expect(f.read<number>("terminalCalls")).toBe(0);

  // Authenticated Host has only requested a close. It has NOT received the
  // app's Allow/Veto reply and has NOT issued its actual Destroyed receipt.
  f.run("pendingClose = 3n");
  expect(
    f.run<boolean>("joinOriginalQuitBeforeClose(selectedWindow, 3n)"),
  ).toBe(true);
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;

  // Host CloseReply(Allow) -> physical Destroyed -> LastWindowClosed.
  f.run("nativeAlive = false; onLastWindowClosed()");
  expect({
    trace: f.read<string[]>("trace"),
    terminalCalls: f.read<number>("terminalCalls"),
  }).toEqual({ trace: ["before-quit"], terminalCalls: 0 });
  // When full GH451 choreography is approved, a separate owner must verify
  // will-quit, terminal Quit and native teardown; none is claimed here.
});

test("GH531-34: RequestClose RPC success alone is not native Destroyed", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  // An RPC reply says Host accepted/requested the close, not that a physical
  // release occurred. Deliberately do NOT dispatch the native CloseRequested
  // callback first: the two evidence channels have separate delivery order.
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  expect(f.read<number>("requestCalls")).toBe(1);
  expect(f.read<number>("terminalCalls")).toBe(0);
  f.run("nativeAlive = false; onLastWindowClosed()");
  expect({
    trace: f.read<string[]>("trace"),
    terminalCalls: f.read<number>("terminalCalls"),
  }).toEqual({ trace: ["before-quit"], terminalCalls: 0 });
});

test("Host rejection before CloseRequested leaves no false terminal and permits retry", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("rejectClose(new Error('controlled Host RequestClose denial'))");
  await expect(first).rejects.toThrow("controlled Host RequestClose denial");
  expect(f.read<string[]>("trace")).toEqual(["before-quit"]);
  expect(f.read<number>("terminalCalls")).toBe(0);
  const second = f.run<Promise<void>>("quit()");
  expect(second).not.toBe(first);
  f.run("rejectClose(new Error('second controlled denial'))");
  await expect(second).rejects.toThrow("second controlled denial");
  expect(f.read<string[]>("trace")).toEqual(["before-quit", "before-quit"]);
});

test("inherited descriptor getter cannot strand Quit and block a later retry", async () => {
  const f = fixture();
  // Poison the app's own VM realm after preload, not the test runner's realm.
  // Without null-prototype descriptors, Object.defineProperty throws before
  // the active attempt's cleanup handler is installed.
  f.run(
    "Object.prototype.get = function injectedGetter() { return () => {}; }",
  );
  const first = f.run<Promise<void>>("quit()");
  expect(f.read<string[]>("trace")).toEqual(["before-quit"]);
  f.run("rejectClose(new Error('controlled Host denial under poison'))");
  await expect(first).rejects.toThrow("controlled Host denial under poison");

  f.run("delete Object.prototype.get");
  const retry = f.run<Promise<void>>("quit()");
  expect(retry).not.toBe(first);
  expect(f.read<string[]>("trace")).toEqual(["before-quit", "before-quit"]);
  f.run("rejectClose(new Error('retry remains available'))");
  await expect(retry).rejects.toThrow("retry remains available");
});

test("inherited getter cannot spoof defaultPrevented or swallow a vetoed Quit retry", async () => {
  const f = fixture();
  f.run(
    "Object.prototype.get = function injectedGetter() { return 'forged'; }; vetoBeforeQuit = true",
  );
  const vetoed = f.run<Promise<void>>("quit()");
  await vetoed;
  expect(f.read<string[]>("trace")).toEqual(["before-quit"]);
  expect(f.read<number>("requestCalls")).toBe(0);

  f.run("delete Object.prototype.get; vetoBeforeQuit = false");
  const retry = f.run<Promise<void>>("quit()");
  expect(retry).not.toBe(vetoed);
  expect(f.read<string[]>("trace")).toEqual(["before-quit", "before-quit"]);
  f.run("rejectClose(new Error('retry after veto remains available'))");
  await expect(retry).rejects.toThrow("retry after veto remains available");
});

test("controlled Host Veto keeps dirty window; later user close may emit window-all-closed", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("pendingClose = 3n");
  expect(
    f.run<boolean>("joinOriginalQuitBeforeClose(selectedWindow, 3n)"),
  ).toBe(true);
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  // A dirty-document preventDefault causes Host CloseReply(Veto). No Destroyed
  // and no LastWindowClosed may be manufactured at this point.
  f.run("pendingClose = null");
  f.run("noteOriginalQuitVetoApplied(selectedWindow, 3n)");
  expect(f.read<string[]>("trace")).toEqual(["before-quit"]);
  expect(f.read<number>("terminalCalls")).toBe(0);
  // Only a separate successful user close, outside the vetoed Quit, creates
  // the actual Host Destroyed and LastWindowClosed used here.
  f.run("nativeAlive = false; onLastWindowClosed()");
  expect(f.read<string[]>("trace")).toEqual([
    "before-quit",
    "window-all-closed",
  ]);
});

test("a replacement window blocks stale LastWindowClosed and terminal Quit", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("pendingClose = 3n");
  expect(
    f.run<boolean>("joinOriginalQuitBeforeClose(selectedWindow, 3n)"),
  ).toBe(true);
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  f.run(
    "nativeAlive = false; replacementAlive = true; noteOriginalQuitWindowDestroyed(selectedWindow); onLastWindowClosed()",
  );
  expect(f.read<string[]>("trace")).toEqual(["before-quit"]);
  expect(f.read<number>("terminalCalls")).toBe(0);
  f.run("replacementAlive = false; onLastWindowClosed()");
  expect(f.read<string[]>("trace")).toEqual([
    "before-quit",
    "window-all-closed",
  ]);
});

test("retired G1 suppression cannot swallow a CURRENT G2 LastWindowClosed", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  f.run("generationCurrent = false; nativeAlive = false; onLastWindowClosed()");
  expect(f.read<string[]>("trace")).toEqual([
    "before-quit",
    "window-all-closed",
  ]);
  expect(f.read<number>("terminalCalls")).toBe(0);
});

test("pending original Quit coalesces until Host Veto ReplyApplied, then retry emits before-quit", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("pendingClose = 3n");
  expect(
    f.run<boolean>("joinOriginalQuitBeforeClose(selectedWindow, 3n)"),
  ).toBe(true);
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  expect(f.run<Promise<void>>("quit()")).toBe(first);
  f.run("noteOriginalQuitVetoApplied(selectedWindow, 2n)");
  expect(f.run<Promise<void>>("quit()")).toBe(first);
  f.run("pendingClose = null; noteOriginalQuitVetoApplied(selectedWindow, 3n)");
  const again = f.run<Promise<void>>("quit()");
  expect(again).not.toBe(first);
  expect(f.read<string[]>("trace")).toEqual(["before-quit", "before-quit"]);
  f.run("rejectClose(new Error('retry refused by Host'))");
  await expect(again).rejects.toThrow("retry refused by Host");
});

test("premature LastWindowClosed cannot consume suppression before native Destroyed", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("pendingClose = 3n; joinOriginalQuitBeforeClose(selectedWindow, 3n)");
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  f.run("onLastWindowClosed()");
  expect(f.read<string[]>("trace")).toEqual(["before-quit"]);
  expect(f.run<Promise<void>>("quit()")).toBe(first);
  f.run("nativeAlive = false; onLastWindowClosed()");
  expect(f.read<string[]>("trace")).toEqual(["before-quit"]);
  expect(f.read<number>("terminalCalls")).toBe(0);
});

test("unbound AlreadyClosing answer fails closed and releases a retryable attempt", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("resolveClose({variant:'AlreadyClosing'})");
  await expect(first).rejects.toThrow("KELD-CORE-054");
  expect(f.read<string[]>("trace")).toEqual(["before-quit"]);
  const retry = f.run<Promise<void>>("quit()");
  expect(retry).not.toBe(first);
  f.run("rejectClose(new Error('retry refused'))");
  await expect(retry).rejects.toThrow("retry refused");
});

test("recovered document G2 in the SAME native WindowRef retires G1's pending Quit suppression", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>("quit()");
  f.run("pendingClose=3n; joinOriginalQuitBeforeClose(selectedWindow,3n)");
  f.run("resolveClose({variant:'CloseRequested',close_seq:3n})");
  await first;
  f.run(
    "documentCurrent=false; windowFactCallback({variant:'DocumentRecovered',window:selectedWindow})",
  );
  f.run("nativeAlive=false; onLastWindowClosed()");
  expect(f.read<string[]>("trace")).toEqual([
    "before-quit",
    "window-all-closed",
  ]);
  expect(f.read<number>("terminalCalls")).toBe(0);
});

test("native Veto and Destroyed hooks stay behind actual Host receipts", () => {
  const code = readFileSync(
    new URL("./browser-window.ts", import.meta.url),
    "utf8",
  );
  const vetoStart = code.indexOf(
    "if (prevented) {",
    code.indexOf("  #nativeClose(close_seq: bigint)"),
  );
  const authenticated = code.indexOf(
    'response.variant !== "ReplyApplied"',
    vetoStart,
  );
  const notifyVeto = code.indexOf(
    "noteOriginalQuitVetoApplied(this.#ref, close_seq);",
    vetoStart,
  );
  const callbackStart = code.indexOf("  #closed(fromDestroy: boolean)");
  const userClosed = code.indexOf('this.emit("closed");', callbackStart);
  const notifyDestroyed = code.indexOf(
    "noteOriginalQuitWindowDestroyed(this.#ref);",
    callbackStart,
  );
  expect(vetoStart).toBeGreaterThan(0);
  expect(authenticated).toBeGreaterThan(vetoStart);
  expect(notifyVeto).toBeGreaterThan(authenticated);
  expect(notifyDestroyed).toBeGreaterThan(userClosed);
});

test("GH531 criterion34 suppresses last-window notification while will-quit is active", async () => {
  const f = fixture();
  f.run("nativeAlive=false; vetoWillWithTemporaryClose=true");
  await f.run<Promise<void>>("quit()");
  expect(f.read<string[]>("trace")).toEqual(["before-quit", "will-quit"]);
  expect(f.read<number>("terminalCalls")).toBe(0);
});

test("will-quit veto clears D2 before an already-queued next-turn Quit retry", async () => {
  const f = fixture();
  const first = f.run<Promise<void>>(
    "nativeAlive=false; enqueueRetryAfterWillVeto=true; quit()",
  );
  await first;
  await Promise.resolve();
  const retry = f.run<Promise<void> | null>("queuedRetry");
  expect(retry).not.toBeNull();
  expect(retry).not.toBe(first);
  await retry;
  expect(f.read<string[]>("trace")).toEqual([
    "before-quit",
    "will-quit",
    "before-quit",
    "will-quit",
    "terminal",
  ]);
  expect(f.read<number>("terminalCalls")).toBe(1);
});
