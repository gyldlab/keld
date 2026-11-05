// A real Electron facade role. Imports of app, BrowserWindow and the existing
// package-private window adapter are prepended by the Windows stage fixture.
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";

const control = createConnection({
  host: "127.0.0.1",
  port: Number(process.env.KELD_T1B_CONTROL),
});
const report = (kind: string, value: unknown = {}) =>
  control.write(
    kind +
      " " +
      JSON.stringify(value, (_key, item) =>
        typeof item === "bigint" ? item.toString() : item,
      ) +
      "\n",
  );
const code = (error: unknown) =>
  /KELD-[A-Z]+-\d+/.exec(String(error))?.[0] ?? "untyped:" + String(error);
const creationEvents: string[] = [];
let readyEvents = 0;
let allClosed = 0;
const order: string[] = [];
app.on("ready", () => {
  readyEvents += 1;
});
app.on("web-contents-created", () => {
  creationEvents.push("contents");
});
app.on("browser-window-created", () => {
  creationEvents.push("window");
});
if (lastWindowAction === "hold") {
  app.on("window-all-closed", () => {
    allClosed += 1;
    order.push("all");
  });
} else if (lastWindowAction === "drawio") {
  app.on("window-all-closed", () => {
    allClosed += 1;
    order.push("all");
    report("LAST_WINDOW_QUIT", {
      action: lastWindowAction,
      platform: process.platform,
      liveCount: BrowserWindow.getAllWindows().length,
      count: allClosed,
    });
    // The pinned draw.io listener requests the public facade Quit on non-macOS.
    if (process.platform !== "darwin") {
      void app.quit().then(
        () => report("AUTO_QUIT_ACCEPTED"),
        (error) => report("AUTO_QUIT_ERROR", { code: code(error) }),
      );
    }
  });
} else if (lastWindowAction !== "default") {
  throw new Error("unrecognized fixed last-window fixture action");
}
// The default arm deliberately registers no window-all-closed listener.

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

// Fixture-only rendezvous. The real callback still sends its ordinary verdict
// only after returning. No Host policy, native hook or permission is added.
let heldCloseId: number | null = null;
let closeHoldUsed = false;
const closePark = new Int32Array(new SharedArrayBuffer(4));
function holdRecord(name: string, value: unknown) {
  const path = join(closeHoldRoot, name + ".json");
  const temporary = path + ".tmp";
  writeFileSync(temporary, JSON.stringify(value));
  renameSync(temporary, path);
}
function holdCloseDecision(value: Tracked) {
  if (heldCloseId !== value.ref.id) return;
  heldCloseId = null;
  const deadline = performance.now() + closeHoldDeadlineMs;
  try {
    holdRecord("pending", {
      id: value.ref.id,
      generation: String(value.ref.incarnation),
      sequence: String(value.closeSeq),
      count: value.close,
    });
    let observed = false;
    while (!existsSync(join(closeHoldRoot, "release"))) {
      if (performance.now() >= deadline)
        throw new Error("fixture close-decision hold expired");
      const drain = join(closeHoldRoot, "drain.json");
      if (!observed && existsSync(drain)) {
        const target = JSON.parse(readFileSync(drain, "utf8"));
        if (!Number.isFinite(target.x) || !Number.isFinite(target.y))
          throw new Error("invalid owned native move target");
        // A harmless existing setter lets the one transport apply retained
        // state records while this user callback remains synchronous. It does
        // not request another close, send a verdict, or dispatch user listeners.
        const reply = invokeWindowBlocking({
          variant: "SetFullScreen",
          window: value.ref,
          full_screen: false,
        });
        if (reply.variant !== "Accepted")
          throw new Error("fixture drain setter was refused");
        const snapshot = getWindowSnapshots().find(
          (item) =>
            item.window.id === value.ref.id &&
            item.window.incarnation === value.ref.incarnation,
        );
        const bounds = snapshot?.state.bounds;
        if (
          bounds &&
          Math.abs(bounds.x - target.x) <= 1 &&
          Math.abs(bounds.y - target.y) <= 1
        ) {
          observed = true;
          holdRecord("observed", {
            id: value.ref.id,
            generation: String(value.ref.incarnation),
            pending:
              snapshot?.pending_close === null
                ? null
                : String(snapshot?.pending_close),
            count: value.close,
            bounds,
          });
        }
      }
      Atomics.wait(closePark, 0, 0, 10);
    }
  } catch (error) {
    holdRecord("error", { code: code(error), detail: String(error) });
    throw error;
  }
}

function contentsOf(win: InstanceType<typeof BrowserWindow>) {
  return win.webContents;
}
const tracked = new Map<number, Tracked>();
const initialStates = new Map<number, unknown>();
const factCounts = new Map<
  number,
  { focusTrue: number; focusFalse: number; maximized: number; bounds: number }
>();
onWindowFactApplied((fact) => {
  const count = factCounts.get(fact.window.id) ?? {
    focusTrue: 0,
    focusFalse: 0,
    maximized: 0,
    bounds: 0,
  };
  if (fact.variant === "Created") initialStates.set(fact.window.id, fact.state);
  if (fact.variant === "Focus") {
    if (fact.focused) count.focusTrue += 1;
    else count.focusFalse += 1;
  }
  if (fact.variant === "Maximized") count.maximized += 1;
  if (fact.variant === "Bounds") count.bounds += 1;
  factCounts.set(fact.window.id, count);
});
function attach(win: InstanceType<typeof BrowserWindow>): Tracked {
  const existing = tracked.get(win.id);
  if (existing) return existing;
  const snapshot = getWindowSnapshots().find(
    (item) => item.window.id === win.id && !item.host_destroyed,
  );
  if (!snapshot)
    throw new Error("public window has no live canonical Host snapshot");
  const value: Tracked = {
    win,
    ref: { ...snapshot.window },
    contents: win.webContents,
    veto: true,
    close: 0,
    closed: 0,
    destroyed: 0,
    closeSeq: null,
    insideClosedContents: null,
  };
  tracked.set(win.id, value);
  value.contents.on("destroyed", () => {
    value.destroyed += 1;
    order.push("contents:" + win.id);
  });
  win.on("close", (event: { preventDefault(): void }) => {
    value.close += 1;
    value.closeSeq =
      getWindowSnapshots().find((item) => item.window.id === value.ref.id)
        ?.pending_close ?? null;
    if (value.veto) event.preventDefault();
    holdCloseDecision(value);
    order.push("close:" + win.id);
    report("CLOSE_EVENT", {
      id: win.id,
      count: value.close,
      seq: value.closeSeq,
    });
  });
  win.on("closed", () => {
    value.closed += 1;
    value.insideClosedContents = value.contents.isDestroyed();
    order.push("closed:" + win.id);
    report("CLOSED", {
      id: win.id,
      count: value.closed,
      destroyed: win.isDestroyed(),
      contentsDestroyed: value.contents.isDestroyed(),
    });
  });
  return value;
}
function item(id: string): Tracked {
  const previous = tracked.get(Number(id));
  if (previous) return previous;
  const replay = BrowserWindow.getAllWindows().find(
    (window) => window.id === Number(id),
  );
  if (!replay) throw new Error("unknown fixture window " + id);
  return attach(replay);
}
function state(value: Tracked) {
  const snapshot = getWindowSnapshots().find(
    (candidate) => candidate.window.id === value.ref.id,
  );
  let sizeError: string | null = null;
  let size: number[] | null = null;
  try {
    size = value.win.getSize();
  } catch (error) {
    sizeError = code(error);
  }
  return {
    id: value.ref.id,
    generation: value.ref.incarnation,
    live: !value.win.isDestroyed(),
    contentsLive: !value.contents.isDestroyed(),
    close: value.close,
    closed: value.closed,
    contentsDestroyed: value.destroyed,
    insideClosedContents: value.insideClosedContents,
    pending: snapshot?.pending_close ?? null,
    hostDestroyed: snapshot?.host_destroyed ?? null,
    liveCount: BrowserWindow.getAllWindows().length,
    size,
    sizeError,
    allClosed,
    order,
    position: value.win.isDestroyed() ? null : value.win.getPosition(),
    maximized: value.win.isDestroyed() ? null : value.win.isMaximized(),
    fullScreen: value.win.isDestroyed() ? null : value.win.isFullScreen(),
    focusedId: BrowserWindow.getFocusedWindow()?.id ?? null,
    createdState: initialStates.get(value.ref.id) ?? null,
    facts: factCounts.get(value.ref.id) ?? null,
  };
}
async function initialCreatedWasDispatched(id: number): Promise<void> {
  if (initialStates.has(id)) return;
  await new Promise<void>((resolve) => {
    const stop = onWindowFactApplied((fact) => {
      if (fact.variant === "Created" && fact.window.id === id) {
        stop();
        resolve();
      }
    });
  });
}
async function refused(
  request: Parameters<typeof invokeWindow>[0],
): Promise<string> {
  try {
    await invokeWindow(request);
    return "returned";
  } catch (error) {
    return code(error);
  }
}

await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
report("BOUND", { pid: process.pid, link: process.env.KELD_APP_LINK });
try {
  await app.whenReady();
  const recovered = BrowserWindow.getAllWindows().map((win) =>
    state(attach(win)),
  );
  report("READY", {
    ready: app.isReady(),
    readyEvents,
    windows: recovered,
    lastWindowAction,
  });
  for await (const command of createInterface({ input: control })) {
    const [verb, first, second] = command.split(" ");
    if (["CREATE", "CREATE_CHECKED", "CREATE_X", "CREATE_Y"].includes(verb)) {
      if (verb === "CREATE_CHECKED")
        report("CREATE_ATTEMPT", { pid: process.pid });
      const started = performance.now();
      const before = creationEvents.length;
      try {
        const position =
          verb === "CREATE_X"
            ? { x: 37 }
            : verb === "CREATE_Y"
              ? { y: 91 }
              : { x: 37, y: 91 };
        const win = new BrowserWindow({
          width: 640,
          height: 480,
          ...position,
          webPreferences: {
            contextIsolation: true,
            nodeIntegration: false,
            webSecurity: true,
          },
        });
        const roundTripMs = performance.now() - started;
        const value = attach(win);
        await initialCreatedWasDispatched(win.id);
        report("PUBLIC_CREATED", {
          ...state(value),
          creation: creationEvents.slice(before),
          roundTripMs,
        });
      } catch (error) {
        // Base #1400 answers CORE-042 here. The test's first desired Create assertion
        // fails on this actual response, with no timeout or unhandled process crash.
        let windows: number | null = null;
        let mirrorError: string | null = null;
        try {
          windows = BrowserWindow.getAllWindows().length;
        } catch (mirror) {
          mirrorError = code(mirror);
        }
        report("CREATE_ERROR", {
          code: code(error),
          windows,
          mirrorError,
          roundTripMs: performance.now() - started,
        });
      }
    } else if (verb === "REPLACE") {
      const previous = item(first);
      const gapMs = Number(second);
      if (![0, 150, 400].includes(gapMs))
        throw new Error("unapproved browser-rebind gap");
      previous.win.destroy();
      const gapStart = performance.now();
      if (gapMs !== 0) await Bun.sleep(gapMs);
      const actualGapMs = performance.now() - gapStart;
      const started = performance.now();
      try {
        const successor = new BrowserWindow({
          width: 640,
          height: 480,
          x: 37,
          y: 91,
          webPreferences: {
            contextIsolation: true,
            nodeIntegration: false,
            webSecurity: true,
          },
        });
        const roundTripMs = performance.now() - started;
        await initialCreatedWasDispatched(successor.id);
        report("REPLACED", {
          previous: previous.ref.id,
          previousGeneration: previous.ref.incarnation,
          gapMs,
          actualGapMs,
          ...state(attach(successor)),
          roundTripMs,
        });
      } catch (error) {
        report("REPLACE_ERROR", {
          previous: previous.ref.id,
          gapMs,
          code: code(error),
          roundTripMs: performance.now() - started,
        });
      }
    } else if (verb === "HOLD_CLOSE") {
      const value = item(first);
      if (closeHoldUsed) throw new Error("fixture close hold is one-shot");
      mkdirSync(closeHoldRoot, { recursive: true });
      const probe = join(closeHoldRoot, "access-probe");
      writeFileSync(probe, "owned fixture access", { flag: "wx" });
      if (readFileSync(probe, "utf8") !== "owned fixture access")
        throw new Error("owned fixture access readback failed");
      unlinkSync(probe);
      heldCloseId = value.ref.id;
      closeHoldUsed = true;
      report("CLOSE_HOLD_ARMED", { id: value.ref.id });
    } else if (verb === "ALLOW") {
      item(first).veto = false;
      report("ALLOW", { id: Number(first) });
    } else if (verb === "VERIFY_VETO") {
      const value = item(first);
      if (value.closeSeq === null)
        throw new Error("veto has no canonical close sequence");
      const stale = await invokeWindow({
        variant: "CloseReply",
        window: value.ref,
        close_seq: value.closeSeq,
        verdict: "Allow",
      });
      report("VETO_VERIFIED", {
        reply: stale.variant,
        ...state(value),
        dispatched: getDispatchedPendingWindowCloses().length,
      });
    } else if (verb === "MAXIMIZE") {
      item(first).win.maximize();
      report("MAXIMIZE_SENT", { id: Number(first) });
    } else if (verb === "FULLSCREEN") {
      if (second !== "true" && second !== "false")
        throw new Error("invalid full-screen flag");
      item(first).win.setFullScreen(second === "true");
      report("FULLSCREEN_SENT", {
        id: Number(first),
        value: second === "true",
      });
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
      value.win.close();
      value.win.close();
      value.win.destroy();
      value.win.destroy();
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
        window: {
          id: successor.ref.id,
          incarnation: successor.ref.incarnation + 1n,
        },
        maximized: true,
      });
      report("STALE", { forged, future, ...state(successor) });
    } else if (verb === "RAW_CREATE") {
      const token = first;
      const pending = invokeWindow({
        variant: "Create",
        width: 640,
        height: 480,
        x: 37,
        y: 91,
      });
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
        (reply) =>
          report("DESTROY_REPLY", { id: value.ref.id, reply: reply.variant }),
        (error) => report("RAW_ERROR", { token: first, code: code(error) }),
      );
    } else if (verb === "CRASH") {
      await new Promise<void>((resolve, reject) =>
        control.write(
          "CRASHING " + JSON.stringify({ pid: process.pid }) + "\n",
          (error) => (error ? reject(error) : resolve()),
        ),
      );
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
