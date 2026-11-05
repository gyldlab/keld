// A real Electron facade role. Imports of app, BrowserWindow and the existing
// package-private window adapter are prepended by the Windows stage fixture.
import { createConnection } from "node:net";
import { createInterface } from "node:readline";

const control = createConnection({
  host: "127.0.0.1", port: Number(process.env.KELD_T1B_CONTROL),
});
const report = (kind: string, value: unknown = {}) => control.write(
  kind + " " + JSON.stringify(value, (_key, item) =>
    typeof item === "bigint" ? item.toString() : item) + "\n",
);
const code = (error: unknown) =>
  /KELD-[A-Z]+-\d+/.exec(String(error))?.[0] ?? "untyped:" + String(error);
const creationEvents: string[] = [];
let readyEvents = 0;
let allClosed = 0;
const order: string[] = [];
app.on("ready", () => { readyEvents += 1; });
app.on("web-contents-created", () => { creationEvents.push("contents"); });
app.on("browser-window-created", () => { creationEvents.push("window"); });
app.on("window-all-closed", () => { allClosed += 1; order.push("all"); });

type Tracked = {
  win: InstanceType<typeof BrowserWindow>;
  ref: { id: number; incarnation: bigint };
  contents: ReturnType<typeof contentsOf>;
  veto: boolean;
  close: number;
  closed: number;
  destroyed: number;
  closeSeq: bigint | null;
  insideClosedContents: boolean | null;
};
function contentsOf(win: InstanceType<typeof BrowserWindow>) { return win.webContents; }
const tracked = new Map<number, Tracked>();
const initialStates = new Map<number, unknown>();
const factCounts = new Map<number, { focusTrue: number; focusFalse: number; maximized: number; bounds: number }>();
onWindowFactApplied((fact) => {
  const count = factCounts.get(fact.window.id) ?? { focusTrue: 0, focusFalse: 0, maximized: 0, bounds: 0 };
  if (fact.variant === "Created") initialStates.set(fact.window.id, fact.state);
  if (fact.variant === "Focus") {
    if (fact.focused) count.focusTrue += 1; else count.focusFalse += 1;
  }
  if (fact.variant === "Maximized") count.maximized += 1;
  if (fact.variant === "Bounds") count.bounds += 1;
  factCounts.set(fact.window.id, count);
});
function attach(win: InstanceType<typeof BrowserWindow>): Tracked {
  const existing = tracked.get(win.id);
  if (existing) return existing;
  const snapshot = getWindowSnapshots().find((item) =>
    item.window.id === win.id && !item.host_destroyed);
  if (!snapshot) throw new Error("public window has no live canonical Host snapshot");
  const value: Tracked = {
    win, ref: { ...snapshot.window }, contents: win.webContents,
    veto: true, close: 0, closed: 0, destroyed: 0, closeSeq: null,
    insideClosedContents: null,
  };
  tracked.set(win.id, value);
  value.contents.on("destroyed", () => {
    value.destroyed += 1;
    order.push("contents:" + win.id);
  });
  win.on("close", (event: { preventDefault(): void }) => {
    value.close += 1;
    value.closeSeq = getWindowSnapshots().find((item) =>
      item.window.id === value.ref.id)?.pending_close ?? null;
    if (value.veto) event.preventDefault();
    order.push("close:" + win.id);
    report("CLOSE_EVENT", { id: win.id, count: value.close, seq: value.closeSeq });
  });
  win.on("closed", () => {
    value.closed += 1;
    value.insideClosedContents = value.contents.isDestroyed();
    order.push("closed:" + win.id);
    report("CLOSED", {
      id: win.id, count: value.closed, destroyed: win.isDestroyed(),
      contentsDestroyed: value.contents.isDestroyed(),
    });
  });
  return value;
}
function item(id: string): Tracked {
  const previous = tracked.get(Number(id));
  if (previous) return previous;
  const replay = BrowserWindow.getAllWindows().find((window) => window.id === Number(id));
  if (!replay) throw new Error("unknown fixture window " + id);
  return attach(replay);
}
function state(value: Tracked) {
  const snapshot = getWindowSnapshots().find((candidate) =>
    candidate.window.id === value.ref.id);
  let sizeError: string | null = null;
  let size: number[] | null = null;
  try { size = value.win.getSize(); } catch (error) { sizeError = code(error); }
  return {
    id: value.ref.id, generation: value.ref.incarnation,
    live: !value.win.isDestroyed(), contentsLive: !value.contents.isDestroyed(),
    close: value.close, closed: value.closed, contentsDestroyed: value.destroyed,
    insideClosedContents: value.insideClosedContents,
    pending: snapshot?.pending_close ?? null, hostDestroyed: snapshot?.host_destroyed ?? null,
    liveCount: BrowserWindow.getAllWindows().length, size, sizeError, allClosed, order,
    position: value.win.isDestroyed() ? null : value.win.getPosition(),
    maximized: value.win.isDestroyed() ? null : value.win.isMaximized(),
    fullScreen: value.win.isDestroyed() ? null : value.win.isFullScreen(),
    focusedId: BrowserWindow.getFocusedWindow()?.id ?? null,
    createdState: initialStates.get(value.ref.id) ?? null,
    facts: factCounts.get(value.ref.id) ?? null,
  };
}
async function refused(request: Parameters<typeof invokeWindow>[0]): Promise<string> {
  try { await invokeWindow(request); return "returned"; } catch (error) { return code(error); }
}

await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve); control.once("error", reject);
});
report("BOUND", { pid: process.pid, link: process.env.KELD_APP_LINK });
try {
  await app.whenReady();
  const recovered = BrowserWindow.getAllWindows().map((win) => state(attach(win)));
  report("READY", { ready: app.isReady(), readyEvents, windows: recovered });
  for await (const command of createInterface({ input: control })) {
    const [verb, first, second] = command.split(" ");
    if (verb === "CREATE" || verb === "CREATE_CHECKED") {
      if (verb === "CREATE_CHECKED") report("CREATE_ATTEMPT", { pid: process.pid });
      const started = performance.now();
      const before = creationEvents.length;
      try {
        const win = new BrowserWindow({
          width: 640, height: 480, x: 37, y: 91,
          webPreferences: { contextIsolation: true, nodeIntegration: false, webSecurity: true },
        });
        const value = attach(win);
        report("PUBLIC_CREATED", {
          ...state(value), creation: creationEvents.slice(before),
          roundTripMs: performance.now() - started,
        });
      } catch (error) {
        // Base #1400 answers CORE-042 here. The test's first desired Create assertion
        // fails on this actual response, with no timeout or unhandled process crash.
        let windows: number | null = null;
        let mirrorError: string | null = null;
        try { windows = BrowserWindow.getAllWindows().length; }
        catch (mirror) { mirrorError = code(mirror); }
        report("CREATE_ERROR", {
          code: code(error), windows, mirrorError,
          roundTripMs: performance.now() - started,
        });
      }
    } else if (verb === "REPLACE") {
      const previous = item(first);
      const gapMs = Number(second);
      if (![0, 150, 400].includes(gapMs)) throw new Error("unapproved browser-rebind gap");
      previous.win.destroy();
      const gapStart = performance.now();
      if (gapMs !== 0) await Bun.sleep(gapMs);
      const actualGapMs = performance.now() - gapStart;
      const started = performance.now();
      try {
        const successor = new BrowserWindow({
          width: 640, height: 480, x: 37, y: 91,
          webPreferences: { contextIsolation: true, nodeIntegration: false, webSecurity: true },
        });
        report("REPLACED", {
          previous: previous.ref.id, previousGeneration: previous.ref.incarnation,
          gapMs, actualGapMs, ...state(attach(successor)),
          roundTripMs: performance.now() - started,
        });
      } catch (error) {
        report("REPLACE_ERROR", {
          previous: previous.ref.id, gapMs, code: code(error),
          roundTripMs: performance.now() - started,
        });
      }
    } else if (verb === "ALLOW") {
      item(first).veto = false;
      report("ALLOW", { id: Number(first) });
    } else if (verb === "VERIFY_VETO") {
      const value = item(first);
      if (value.closeSeq === null) throw new Error("veto has no canonical close sequence");
      const stale = await invokeWindow({
        variant: "CloseReply", window: value.ref, close_seq: value.closeSeq, verdict: "Allow",
      });
      report("VETO_VERIFIED", {
        reply: stale.variant, ...state(value),
        dispatched: getDispatchedPendingWindowCloses().length,
      });
    } else if (verb === "MAXIMIZE") {
      item(first).win.maximize();
      report("MAXIMIZE_SENT", { id: Number(first) });
    } else if (verb === "FULLSCREEN") {
      if (second !== "true" && second !== "false") throw new Error("invalid full-screen flag");
      item(first).win.setFullScreen(second === "true");
      report("FULLSCREEN_SENT", { id: Number(first), value: second === "true" });
    } else if (verb === "STATE") {
      report("STATE", state(item(first)));
    } else if (verb === "DESTROY") {
      const value = item(first);
      value.win.destroy();
      order.push("return:" + value.ref.id);
      report("DESTROY_RETURN", state(value));
      await Promise.resolve();
      report("DESTROY_FINISHED", state(value));
    } else if (verb === "REPEAT") {
      const value = item(first);
      value.win.close(); value.win.close();
      value.win.destroy(); value.win.destroy();
      const retired = await refused({ variant: "Destroy", window: value.ref });
      report("REPEATED", { retired, ...state(value) });
    } else if (verb === "STALE") {
      const retired = item(first);
      const successor = item(second);
      const forged = await refused({
        variant: "Destroy",
        window: { id: successor.ref.id, incarnation: retired.ref.incarnation },
      });
      const future = await refused({
        variant: "SetMaximized",
        window: { id: successor.ref.id, incarnation: successor.ref.incarnation + 1n },
        maximized: true,
      });
      report("STALE", { forged, future, ...state(successor) });
    } else if (verb === "RAW_CREATE") {
      const token = first;
      const pending = invokeWindow({ variant: "Create", width: 640, height: 480, x: 37, y: 91 });
      report("RAW_SENT", { token });
      void pending.then(
        (reply) => report("RAW_CREATED", { token, reply }),
        (error) => report("RAW_ERROR", { token, code: code(error) }),
      );
    } else if (verb === "RAW_DESTROY") {
      const value = item(first);
      const pending = invokeWindow({ variant: "Destroy", window: value.ref });
      report("RAW_DESTROY_SENT", { id: value.ref.id });
      void pending.then(
        (reply) => report("DESTROY_REPLY", { id: value.ref.id, reply: reply.variant }),
        (error) => report("RAW_ERROR", { token: first, code: code(error) }),
      );
    } else if (verb === "CRASH") {
      await new Promise<void>((resolve, reject) => control.write(
        "CRASHING " + JSON.stringify({ pid: process.pid }) + "\n",
        (error) => error ? reject(error) : resolve(),
      ));
      process.exit(86);
    } else if (verb === "QUIT") {
      await app.quit();
      report("QUIT_ACCEPTED");
      control.end();
      break;
    } else if (verb === "QUIT_RAW") {
      await hostApp.quit();
      report("QUIT_ACCEPTED");
      control.end();
      break;
    } else {
      throw new Error("unknown facade fixture command " + command);
    }
  }
} catch (error) {
  report("ROLE_ERROR", { code: code(error), detail: String(error) });
  throw error;
}
