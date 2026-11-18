/** Component-only controlled inputs. These tests authenticate no Host or native page. */
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { WindowSource } from "../../api/src/window-source.ts";
import { CompatSource } from "../../api/src/compat-source.ts";
import {
  encodeWindowEvent,
  type WindowEvent,
} from "../../api/src/window-codec.ts";
import vm from "node:vm";

export const tree = resolve(import.meta.dir, "../../../..");
export const sourceRoot = process.env.KELD_RECOVERY_SOURCE_ROOT ?? tree;
export const mutation = process.env.KELD_RECOVERY_MUTANT ?? "";

export type Ref = { id: number; incarnation: bigint };
export interface TestWindow extends EventEmitter {
  id: number;
  loadURL(uri: string): Promise<void>;
  destroy(): void;
  isDestroyed(): boolean;
  webContents: EventEmitter & { getURL(): string };
}
type Request = { variant: string; window?: Ref; uri?: string };
type HostFact = WindowEvent;

function transform(source: string): string {
  return new Bun.Transpiler({ loader: "ts" }).transformSync(source);
}

export function makeFixture() {
  let api = readFileSync(
    join(sourceRoot, "packages/@keld/api/src/app.ts"),
    "utf8",
  );
  const apiBegin = api.indexOf("function currentOriginalWindowSession()");
  const apiEnd = api.indexOf(
    "/** Read-only scope for the same admitted original session",
    apiBegin,
  );
  assert(apiBegin > 0 && apiEnd > apiBegin, "existing API owner slice missing");
  api = api.slice(apiBegin, apiEnd).replaceAll("export function ", "function ");
  if (mutation === "omit-source-check")
    api = api.replace(
      "currentOriginalWindowSession() !== session ||\n      session.windowSource !== source",
      "false",
    );
  let browser = readFileSync(
    join(sourceRoot, "packages/@keld/electron/src/browser-window.ts"),
    "utf8",
  );
  browser = browser
    .replace(/^import[\s\S]*?;\r?\n/gm, "")
    .replace(/\bexport\s+(?=(let|class|interface|type|function)\b)/g, "");
  if (mutation === "omit-skip")
    browser = browser.replace("        current()\n", "        false\n");
  if (mutation === "omit-consume")
    browser = browser.replace(
      "      candidate.#originalRecoveryStartup = false;\n",
      "",
    );
  if (mutation === "omit-explicit-retire")
    browser = browser.replace(
      "    this.#originalRecoveryStartup = false;\n",
      "",
    );

  let onFact: (event: HostFact) => void = () => {};
  const source = new WindowSource((event: HostFact) => onFact(event));
  const compat = new CompatSource(
    22,
    source,
    () => ({ variant: "Consumed" }),
    () => {},
  );
  const calls: Request[] = [];
  const appEvents: string[] = [];
  let nextId = 0;
  let adopt: Ref | undefined;
  let pendingLoad: (() => void) | undefined;
  let deferNextLoad = false;
  const state = {
    bounds: { x: 0, y: 0, width: 800, height: 600 },
    focused: false,
    maximized: false,
    full_screen: false,
  };
  function fact(event: HostFact) {
    const decoded = source.decode(encodeWindowEvent(event));
    source.apply(decoded);
    source.dispatch(decoded);
  }
  function hostWindow(): Ref {
    const window = { id: ++nextId, incarnation: BigInt(100 + nextId) };
    fact({ variant: "Created", window, state, pending_close: null });
    return window;
  }
  function bind(window: Ref, generation: number) {
    compat.apply({
      event: {
        variant: "DocumentBound",
        webview: window.id,
        navigation: generation,
        window_id: window.id,
        window_incarnation: window.incarnation,
      },
    });
  }
  function commit(
    window: Ref,
    generation: number,
    finished = true,
    bound = true,
  ) {
    fact({
      variant: "NavigationCommitted",
      window,
      document_generation: generation,
      native_url: "keld-original://qualified/index.html",
    });
    if (finished)
      fact({
        variant: "NavigationFinished",
        window,
        document_generation: generation,
      });
    if (bound) bind(window, generation);
  }
  const session = {
    windowSource: source,
    windowAdmissionOpen: true,
    captureCurrentWindowDocument: (ref: Ref) => compat.captureForWindow(ref),
  };
  const ctx = vm.createContext(
    {
      EventEmitter,
      source,
      session,
      calls,
      appEvents,
      setImmediate,
      clearImmediate,
      queueMicrotask,
      console,
      stateDefineProperty: Object.defineProperty,
      stateFreeze: Object.freeze,
      stateMap: () => new Map(),
      stateMapGet: (map: Map<unknown, unknown>, key: unknown) => map.get(key),
      stateMapSet: (map: Map<unknown, unknown>, key: unknown, value: unknown) =>
        map.set(key, value),
      stateMapHas: (map: Map<unknown, unknown>, key: unknown) => map.has(key),
      stateMapDelete: (map: Map<unknown, unknown>, key: unknown) =>
        map.delete(key),
      stateWeakMap: () => new WeakMap(),
      stateWeakMapGet: (map: WeakMap<object, unknown>, key: object) =>
        map.get(key),
      stateWeakMapSet: (
        map: WeakMap<object, unknown>,
        key: object,
        value: unknown,
      ) => map.set(key, value),
      linkError: (code: string, message: string) =>
        Object.assign(new Error(message), { code }),
      encodeEndpointArguments: () => new Uint8Array(),
      sendCurrentWindowEvent: () => Promise.resolve(),
      getDispatchedPendingWindowCloses: () => [],
      flushAppliedLastWindowClosedAfterDestroy: () => {},
      joinOriginalQuitBeforeClose: () => false,
      emitNativeAppEvent: (name: string) => appEvents.push(name),
      onWindowFactApplied: (listener: (event: HostFact) => void) => {
        onFact = listener;
        return () => {};
      },
      invokeWindowBlocking: (request: Request) => {
        calls.push(request);
        if (
          request.variant === "Create" ||
          request.variant === "CreateWithPreload"
        ) {
          if (adopt) {
            const window = adopt;
            adopt = undefined;
            return { variant: "Adopted", window };
          }
          return { variant: "Created", window: hostWindow() };
        }
        if (request.variant === "Destroy") {
          assert(request.window);
          // The blocking transport applies physical facts before returning its
          // reply; user dispatch (and Source retirement) happens afterward.
          const destroyed = source.decode(
            encodeWindowEvent({ variant: "Destroyed", window: request.window }),
          );
          source.apply(destroyed);
          queueMicrotask(() => source.dispatch(destroyed));
          return { variant: "Destroyed" };
        }
        throw new Error("unexpected blocking request " + request.variant);
      },
      invokeWindow: (request: Request) => {
        calls.push(request);
        if (request.variant !== "LoadUrl")
          return Promise.resolve({ variant: "ReplyApplied" });
        assert(request.window);
        const window = request.window;
        const finish = () => {
          const generation =
            (source.get(window)?.document?.generation ?? 0) + 1;
          commit(window, generation);
          return { variant: "Loaded" };
        };
        if (!deferNextLoad) return Promise.resolve(finish());
        deferNextLoad = false;
        return new Promise((resolve) => {
          pendingLoad = () => resolve(finish());
        });
      },
    },
    { codeGeneration: { strings: false, wasm: false } },
  );
  vm.runInContext(
    "let connectedLink=session, hostReady=true, linkDead; " +
      "let nativeFacadeBootSelected=true, runtimeProcessFactsSelected=true; " +
      "let hostLaunchFacts={schema:7,originalResourcePolicy:{contract:'qualified-input'}}; " +
      "const hostApp={isReady:()=>hostReady,whenReady:()=>Promise.resolve()}; " +
      "const getWindowSnapshot=ref=>hostReady?connectedLink?.windowSource?.get(ref):undefined; " +
      "const getWindowSnapshots=()=>hostReady?(connectedLink?.windowSource?.all()??[]):[];\n" +
      transform(api + "\n" + browser),
    ctx,
    { timeout: 1500 },
  );
  const run = (code: string) => vm.runInContext(code, ctx, { timeout: 500 });
  const Constructor = run("BrowserWindow") as new () => TestWindow;
  const consume = run(
    "typeof consumeOriginalRecoveryStartup==='function' ? consumeOriginalRecoveryStartup : (()=>false)",
  ) as (candidate: unknown) => boolean;
  const capture = run(
    "typeof captureOriginalWindowRecoveryDocument==='function' ? captureOriginalWindowRecoveryDocument : (()=>undefined)",
  ) as (ref: Ref) => (() => boolean) | undefined;
  return {
    calls,
    appEvents,
    source,
    session,
    compat,
    fact,
    commit,
    bind,
    hostWindow,
    consume,
    capture,
    run,
    construct: () => new Constructor(),
    enumerate: () => run("BrowserWindow.getAllWindows()") as TestWindow[],
    adoptNext: (window: Ref) => {
      adopt = window;
    },
    deferLoad: () => {
      deferNextLoad = true;
    },
    finishLoad: () => {
      assert(pendingLoad, "no pending native load input");
      const finish = pendingLoad;
      pendingLoad = undefined;
      finish();
    },
    loadCalls: () => calls.filter((call) => call.variant === "LoadUrl"),
    retire: (window: Ref, generation: number) =>
      source.retireNativeDocument(window, generation),
    endSource: () =>
      source.end(
        Object.assign(new Error("controlled source retirement"), {
          code: "KELD-IPC-022",
        }),
      ),
  };
}
