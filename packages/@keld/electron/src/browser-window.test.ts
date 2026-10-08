/**
 * Mapped oracle cases for the `electron-window-v1` corpus (gyldlab/keld#448, F02-T1):
 * the draw.io first-proof window path, pinned at Electron v44.4.5
 * (`694f45852a0f1726cd23bfd379854de489cccb65`).
 *
 * Manifest: `crates/keld-compat/fixtures/window-corpus/corpus.json`. Each case name
 * below is a cell's exact `test_name`; `corpus_registry` admits a cell only when its
 * case passed once on a declared platform (gh566 D8, D13).
 *
 * Every cell is red today. `@keld/electron` exports no `BrowserWindow`, so each case
 * asserts that current behaviour and passes (gh532 rule 3). A cited cell records
 * `expected_verdict: fail` with the ticket that implements it (GH-449, GH-450 or
 * GH-455). That ticket's change inverts the case to assert the cited sentence and
 * flips the cell in the same PR.
 *
 * The getter, state and close cases probe only the members their own ticket adds, so
 * GH-449's constructor leaves the GH-450 cases unchanged. The creation and triage cases
 * observe construction itself. GH-449's constructor therefore changes the triage
 * cases' current behaviour, and that PR re-records them (still red, GH-455).
 *
 * Three cells are `unknown` because their oracle is a v44.4.5 source receipt, not a
 * doc sentence, and gh532 §10 Q1 admits doc citations only. Each receipt is quoted
 * above its case.
 */
import { describe, expect, test } from "bun:test";

import * as electron from "./index";

const { app } = electron;

/** `target[name]`, read the way an app reads it (inherited members included). */
function member(target: unknown, name: string): unknown {
  if ((typeof target !== "object" && typeof target !== "function") || target === null) {
    return undefined;
  }
  return Reflect.get(target, name);
}

/** The `BrowserWindow` binding an app would import, by name or from the default export. */
function exportedBrowserWindow(): unknown {
  return member(electron, "BrowserWindow") ?? member(electron.default, "BrowserWindow");
}

/** The named statics that the exported `BrowserWindow` has as functions. */
function presentStatics(names: readonly string[]): string[] {
  const binding = exportedBrowserWindow();
  return names.filter((name) => typeof member(binding, name) === "function");
}

/** The named instance methods that every `BrowserWindow` instance would inherit. */
function presentInstanceMethods(names: readonly string[]): string[] {
  const prototype = member(exportedBrowserWindow(), "prototype");
  return names.filter((name) => typeof member(prototype, name) === "function");
}

/** Runs `new BrowserWindow(options)` against the exported binding, as an app would. */
function construct(options: object): unknown {
  const BrowserWindow = exportedBrowserWindow() as new (options: object) => unknown;
  return new BrowserWindow(options);
}

/** The error `construct(options)` throws, or a failure if it returns. */
function constructionError(options: object): Error {
  try {
    construct(options);
  } catch (error) {
    if (error instanceof Error) return error;
    throw new Error(`construction threw a non-Error value: ${String(error)}`);
  }
  throw new Error("construction returned a window; this cell is no longer red");
}

/**
 * draw.io's main-window options (source receipt: jgraph/drawio-desktop@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e,
 * `src/main/electron.js` lines 715-748, sha256 55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d):
 * the same keys and value types, with draw.io's light-theme and first-run defaults.
 */
const DRAWIO_OPTIONS = {
  backgroundColor: "#F1F3F4",
  width: 1200,
  height: 800,
  x: 0,
  y: 0,
  icon: "images/drawlogo256.png",
  webPreferences: {
    preload: "electron-preload.js",
    spellcheck: true,
    contextIsolation: true,
    nodeIntegration: false,
    webviewTag: false,
    webSecurity: true,
    disableBlinkFeatures: "Auxclick",
    additionalArguments: [] as string[],
  },
} as const;

/** The six `webPreferences` values that construction refuses once GH-455 lands (gh531 §4.f). */
const ESCALATING_WEB_PREFERENCES = [
  { nodeIntegration: true },
  { nodeIntegrationInWorker: true },
  { nodeIntegrationInSubFrames: true },
  { contextIsolation: false },
  { webSecurity: false },
  { allowRunningInsecureContent: true },
] as const;

/** Asserts today's construction outcome: a TypeError because no constructor is exported. */
function expectNoConstructor(options: object): void {
  const error = constructionError(options);
  expect(error).toBeInstanceOf(TypeError);
  expect(error.message).not.toContain("KELD-");
}

/** Registers `event` on the app facade, attempts one construction, and counts calls. */
function appEventCallsDuringConstruction(event: string): number {
  let calls = 0;
  const listener = (): void => {
    calls += 1;
  };
  app.on(event, listener);
  try {
    expectNoConstructor(DRAWIO_OPTIONS);
  } finally {
    app.removeListener(event, listener);
  }
  return calls;
}

describe("GH-449 window registry cells, red until implemented", () => {
  // browser-window.md:523 "Each ID is unique among all `BrowserWindow` instances ..."
  // Ordered observable: new BrowserWindow(options) returns; win.id equals the WindowId
  // in the host's Created event.
  test("window.constructor.host-id today: @keld/electron exports no BrowserWindow constructor, so no window id exists", () => {
    expect(typeof exportedBrowserWindow()).toBe("undefined");
    expectNoConstructor(DRAWIO_OPTIONS);
  });

  // app.md:282 "Emitted when a new [browserWindow](browser-window.md) is created."
  // Ordered observable: one construction emits browser-window-created exactly once,
  // with that window.
  test("app.browser-window-created.once-per-window today: construction throws, so a browser-window-created listener never runs", () => {
    expect(appEventCallsDuringConstruction("browser-window-created")).toBe(0);
  });

  // app.md:291 "Emitted when a new [webContents](web-contents.md) is created."
  // Ordered observable: one construction emits web-contents-created exactly once,
  // with that window's webContents.
  test("app.web-contents-created.once-per-window today: construction throws, so a web-contents-created listener never runs", () => {
    expect(appEventCallsDuringConstruction("web-contents-created")).toBe(0);
  });

  // browser-window.md:470 "Returns `BrowserWindow[]` - An array of all opened browser windows."
  // Ordered observable: after one construction, getAllWindows() returns synchronously
  // an Array (not a Promise) that contains that window.
  test("window.get-all-windows.sync today: BrowserWindow.getAllWindows does not exist", () => {
    expect(presentStatics(["getAllWindows"])).toEqual([]);
  });

  // browser-window.md:474 "Returns `BrowserWindow | null` - The window that is focused
  // in this application, otherwise returns `null`."
  // Ordered observable: getFocusedWindow() returns synchronously the focused window or null.
  test("window.get-focused-window.sync today: BrowserWindow.getFocusedWindow does not exist", () => {
    expect(presentStatics(["getFocusedWindow"])).toEqual([]);
  });

  // browser-window.md:277 "Emitted when window is maximized."
  // Ordered observable: maximize(), then inside the 'maximize' listener isMaximized() is true.
  test("window.maximize.state-in-listener today: BrowserWindow has no maximize or isMaximized", () => {
    expect(presentInstanceMethods(["maximize", "isMaximized"])).toEqual([]);
  });

  // browser-window.md:753 "When querying for a BrowserWindow's fullscreen status, you
  // should ensure that either the ['enter-full-screen'] or ['leave-full-screen'] events
  // have been emitted." (link targets elided here; the manifest quotes them verbatim)
  // Ordered observable: setFullScreen(true), then once 'enter-full-screen' has been
  // emitted, isFullScreen() is true.
  test("window.set-full-screen.state today: BrowserWindow has no setFullScreen or isFullScreen", () => {
    expect(presentInstanceMethods(["setFullScreen", "isFullScreen"])).toEqual([]);
  });
});

describe("GH-455 constructor triage cells, red until implemented", () => {
  // browser-window.md:156 "It creates a new `BrowserWindow` with native properties as
  // set by the `options`."
  // Ordered observable: new BrowserWindow(<draw.io main-window options>) returns a
  // window, and getAllWindows() grows by one.
  test("window.constructor.drawio-options today: the draw.io main-window options create no window and throw TypeError, not a triage error", () => {
    expectNoConstructor(DRAWIO_OPTIONS);
  });

  // Same sentence. Ordered observable once GH-455 lands: for each escalating value,
  // construction throws the KELD-COMPAT triage error naming the safe value, and no
  // window is created. Electron honours these values, so the implemented refusal is a
  // recorded divergence against this sentence, not parity.
  test("window.constructor.escalating-web-preferences today: each escalating value throws TypeError, not a KELD-COMPAT triage refusal", () => {
    expect(ESCALATING_WEB_PREFERENCES).toHaveLength(6);
    for (const escalation of ESCALATING_WEB_PREFERENCES) {
      expectNoConstructor({
        ...DRAWIO_OPTIONS,
        webPreferences: { ...DRAWIO_OPTIONS.webPreferences, ...escalation },
      });
    }
  });
});

describe("GH-450 close state machine cells, red until implemented", () => {
  // browser-window.md:193 "Calling `event.preventDefault()`\nwill cancel the close."
  // Ordered observable: a native close request emits 'close' once; its listener calls
  // preventDefault(); the window stays open (isDestroyed() false, no 'closed').
  test("window.close-event.prevent-default today: BrowserWindow has no close, so no close event can be vetoed", () => {
    expect(presentInstanceMethods(["close"])).toEqual([]);
  });

  // browser-window.md:661 "and `close` event will also not be emitted\nfor this window,
  // but it guarantees the `closed` event will be emitted."
  // Ordered observable: destroy() emits no 'close' and exactly one 'closed'.
  test("window.destroy.closed-without-close today: BrowserWindow has no destroy", () => {
    expect(presentInstanceMethods(["destroy"])).toEqual([]);
  });

  // browser-window.md:519 "Reading this property throws `Object has been destroyed`
  // once the window has been destroyed"
  // Ordered observable: destroy() returns; reading win.webContents throws an Error whose
  // message contains the exact text Object has been destroyed.
  test("window.web-contents.read-after-destroy-throws today: BrowserWindow has no destroy, so no window can be read after destruction", () => {
    expect(presentInstanceMethods(["destroy"])).toEqual([]);
  });

  // browser-window.md:666 "Try to close the window. This has the same effect as a user
  // manually clicking\nthe close button of the window."
  // Ordered observable: close() vetoed by preventDefault(); a second close() is not
  // vetoed; 'closed' fires exactly once.
  test("window.close.repeat-after-veto-closes-once today: BrowserWindow has no close", () => {
    expect(presentInstanceMethods(["close"])).toEqual([]);
  });

  // browser-window.md:661, as for window.destroy.closed-without-close.
  // Ordered observable: close() vetoed by preventDefault(); destroy() emits no 'close'
  // and exactly one 'closed'.
  test("window.destroy.after-veto-closes-once today: BrowserWindow has neither close nor destroy", () => {
    expect(presentInstanceMethods(["close", "destroy"])).toEqual([]);
  });
});

describe("unknown cells: v44.4.5 source receipts, not doc citations", () => {
  // Source receipt (Electron 694f4585): electron_api_browser_window.cc L69-70 creates the
  // WebContentsView; web_contents_view.cc L302-303 -> web_contents.cc L5359
  // WebContents::New -> L5260 calls _init -> lib/browser/api/web-contents.ts L866 emits
  // 'web-contents-created'. Then electron_api_browser_window.cc L88 InitWithArgs ->
  // gin_helper/wrappable.cc L70-72 calls _init -> lib/browser/api/browser-window.ts L94
  // emits 'browser-window-created'. Both run inside the constructor, so the observed
  // order is web-contents-created, browser-window-created, constructor return.
  test("app.window-created-events.constructor-order today: no creation event and no constructor return exist to order", () => {
    const order: string[] = [];
    const onWindow = (): void => {
      order.push("browser-window-created");
    };
    const onContents = (): void => {
      order.push("web-contents-created");
    };
    app.on("browser-window-created", onWindow);
    app.on("web-contents-created", onContents);
    try {
      expectNoConstructor(DRAWIO_OPTIONS);
    } finally {
      app.removeListener("browser-window-created", onWindow);
      app.removeListener("web-contents-created", onContents);
    }
    expect(order).toEqual([]);
  });

  // Source receipt (Electron 694f4585): electron_api_base_window.cc L1440 binds
  // getAllWindows to BaseWindow::GetAll; gin_helper/trackable_object.h L84-86 returns
  // weak_map_->Values(); key_weak_map.h L73 stores them in a std::unordered_map, so
  // Electron's order is unspecified and Keld's creation order is ▲-stronger, never parity.
  test("window.get-all-windows.order today: no BrowserWindow.getAllWindows exists whose order could be observed", () => {
    expect(presentStatics(["getAllWindows"])).toEqual([]);
  });

  // Source receipt (Electron 694f4585): electron_api_base_window.cc BaseWindow::OnWindowClosed
  // calls MarkDestroyed() at L182 before Emit("closed") at L184, so isDestroyed() is
  // true inside a 'closed' listener (PANEL-D6).
  test("window.closed.is-destroyed-in-listener today: BrowserWindow has no isDestroyed", () => {
    expect(presentInstanceMethods(["isDestroyed"])).toEqual([]);
  });
});
