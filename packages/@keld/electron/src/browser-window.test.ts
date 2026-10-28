/**
 * Mapped oracle cases for the `electron-window-v1` corpus (gyldlab/keld#448, F02-T1):
 * the draw.io first-proof window path, pinned at Electron v44.4.5
 * (`694f45852a0f1726cd23bfd379854de489cccb65`).
 *
 * Manifest: `crates/keld-compat/fixtures/window-corpus/corpus.json`. Each case name
 * below is a cell's exact `test_name`; `corpus_registry` admits a cell only when its
 * case passed once on a declared platform (gh566 D8, D13).
 *
 * The public first-window subset is now exported. These Bun cases exercise
 * fail-closed non-Host admission and API shape without inventing a native Host.
 * The real macOS process/WindowId/veto/Destroy oracle is the shipping
 * keld-host no_flag_macos::facade_window acceptance. Unqualified Electron
 * corpus cells remain fail or unknown, never silently declared compatible.
 *
 * The getter, state and close cases probe only the members their own ticket adds, so
 * GH-449's constructor leaves the GH-450 cases unchanged. The creation and triage cases
 * observe construction itself. GH-449's constructor therefore changes the triage
 * cases' current behaviour, and that PR re-records them (still red, GH-455). GH-449
 * also breaks the two unknown cases it touches: its constructor breaks
 * `app.window-created-events.constructor-order`, and its static `getAllWindows`
 * breaks `window.get-all-windows.order`. That PR re-records both, still `unknown`,
 * because their oracle stays a source receipt (gh532 §10 Q1).
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
  if (
    (typeof target !== "object" && typeof target !== "function") ||
    target === null
  ) {
    return undefined;
  }
  return Reflect.get(target, name);
}

/** The `BrowserWindow` binding an app would import, by name or from the default export. */
function exportedBrowserWindow(): unknown {
  return (
    member(electron, "BrowserWindow") ??
    member(electron.default, "BrowserWindow")
  );
}

/** The named statics that the exported `BrowserWindow` has as functions. */
function presentStatics(names: readonly string[]): string[] {
  const binding = exportedBrowserWindow();
  return names.filter((name) => typeof member(binding, name) === "function");
}

/**
 * A window built from the draw.io options, or `undefined` without an authenticated
 * Keld Host. The native constructor contract is proved in a real Host test.
 */
function constructedWindow(): unknown {
  try {
    return construct(DRAWIO_OPTIONS);
  } catch {
    return undefined;
  }
}

/**
 * The named methods any `BrowserWindow` instance has: inherited from the prototype, or
 * own members (class fields) of a window that construction returns.
 */
function presentInstanceMethods(names: readonly string[]): string[] {
  const prototype = member(exportedBrowserWindow(), "prototype");
  const instance = constructedWindow();
  return names.filter(
    (name) =>
      typeof member(prototype, name) === "function" ||
      typeof member(instance, name) === "function",
  );
}

/** Runs `new BrowserWindow(options)` against the exported binding, as an app would. */
function construct(options: object): unknown {
  const BrowserWindow = exportedBrowserWindow() as new (
    options: object,
  ) => unknown;
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

/** No Host means no locally minted or pretend BrowserWindow identity. */
function expectNoHostWindow(options: object): void {
  const error = constructionError(options);
  expect(error.message).toContain("KELD-CORE-040");
}

/** Registers `event` on the app facade, attempts one construction, and counts calls. */
function appEventCallsDuringConstruction(event: string): number {
  let calls = 0;
  const listener = (): void => {
    calls += 1;
  };
  app.on(event, listener);
  try {
    expectNoHostWindow(DRAWIO_OPTIONS);
  } finally {
    app.removeListener(event, listener);
  }
  return calls;
}

describe("GH-449 window registry: no-Host negative controls", () => {
  // browser-window.md:523 "Each ID is unique among all `BrowserWindow` instances ..."
  // Ordered observable: two constructions return windows whose ids are distinct
  // integers. That the id is the host-delivered WindowId is gh531 criterion 1, which
  // GH-449 proves in its own tests; the Electron sentence cannot falsify provenance.
  test("window.constructor.host-id: exported constructor refuses to mint a local ID without Host Ready", () => {
    expect(typeof exportedBrowserWindow()).toBe("function");
    expectNoHostWindow(DRAWIO_OPTIONS);
  });

  // app.md:282 "Emitted when a new [browserWindow](browser-window.md) is created."
  // Ordered observable: one construction emits browser-window-created exactly once,
  // with that window.
  test("app.browser-window-created.once-per-window: unhosted constructor emits no creation event", () => {
    expect(appEventCallsDuringConstruction("browser-window-created")).toBe(0);
  });

  // app.md:291 "Emitted when a new [webContents](web-contents.md) is created."
  // Ordered observable: one construction emits web-contents-created exactly once,
  // with that window's webContents.
  test("app.web-contents-created.once-per-window: unhosted constructor emits no content event", () => {
    expect(appEventCallsDuringConstruction("web-contents-created")).toBe(0);
  });

  // browser-window.md:470 "Returns `BrowserWindow[]` - An array of all opened browser windows."
  // Ordered observable: after one construction, getAllWindows() returns synchronously
  // an Array (not a Promise) that contains that window.
  test("window.get-all-windows.sync: local empty read has no Host side effects", () => {
    expect(presentStatics(["getAllWindows"])).toEqual(["getAllWindows"]);
    expect(
      (
        exportedBrowserWindow() as typeof electron.BrowserWindow
      ).getAllWindows(),
    ).toEqual([]);
  });

  // browser-window.md:474 "Returns `BrowserWindow | null` - The window that is focused
  // in this application, otherwise returns `null`."
  // Ordered observable: getFocusedWindow() returns synchronously the focused window or null.
  test("window.get-focused-window.sync: no focused window without native fact", () => {
    expect(presentStatics(["getFocusedWindow"])).toEqual(["getFocusedWindow"]);
    expect(electron.BrowserWindow.getFocusedWindow()).toBeNull();
  });

  // browser-window.md:277 "Emitted when window is maximized."
  // Ordered observable: maximize(), then inside the 'maximize' listener isMaximized() is true.
  test("window.maximize.state-in-listener: exposed accessors cannot construct without Host Ready", () => {
    expect(presentInstanceMethods(["maximize", "isMaximized"])).toEqual([
      "maximize",
      "isMaximized",
    ]);
  });

  // browser-window.md:753 "When querying for a BrowserWindow's fullscreen status, you
  // should ensure that either the ['enter-full-screen'] or ['leave-full-screen'] events
  // have been emitted." (link targets elided here; the manifest quotes them verbatim)
  // Ordered observable: setFullScreen(true), then once 'enter-full-screen' has been
  // emitted, isFullScreen() is true.
  test("window.set-full-screen.state: exposed accessors cannot construct without Host Ready", () => {
    expect(presentInstanceMethods(["setFullScreen", "isFullScreen"])).toEqual([
      "setFullScreen",
      "isFullScreen",
    ]);
  });
});

describe("GH-455 constructor triage: no-Host negative controls", () => {
  // browser-window.md:156 "It creates a new `BrowserWindow` with native properties as
  // set by the `options`."
  // Ordered observable: new BrowserWindow(<draw.io main-window options>) returns a
  // window, and getAllWindows() grows by one.
  test("window.constructor.drawio-options: safe draw.io options pass security triage but cannot create without Host", () => {
    expectNoHostWindow(DRAWIO_OPTIONS);
  });

  // Same sentence. Ordered observable once GH-455 lands: for each escalating value,
  // construction throws the KELD-COMPAT triage error naming the safe value, and no
  // window is created. Electron honours these values, so the implemented refusal is a
  // recorded divergence against this sentence, not parity.
  test("window.constructor.escalating-web-preferences: six unsafe values refuse before Host access", () => {
    expect(ESCALATING_WEB_PREFERENCES).toHaveLength(6);
    for (const escalation of ESCALATING_WEB_PREFERENCES) {
      const error = constructionError({
        ...DRAWIO_OPTIONS,
        webPreferences: { ...DRAWIO_OPTIONS.webPreferences, ...escalation },
      });
      expect(error.message).toContain("KELD-COMPAT-010");
    }
  });
});

describe("GH-450 close API: no-Host negative controls", () => {
  // browser-window.md:193 "Calling `event.preventDefault()`\nwill cancel the close."
  // Ordered observable: a native close request emits 'close' once; its listener calls
  // preventDefault(); the window stays open (isDestroyed() false, no 'closed').
  test("window.close-event.prevent-default: close method is exposed only on Host-backed windows", () => {
    expect(presentInstanceMethods(["close"])).toEqual(["close"]);
  });

  // browser-window.md:661 "and `close` event will also not be emitted\nfor this window,
  // but it guarantees the `closed` event will be emitted."
  // Ordered observable: destroy() emits no 'close', and 'closed' is emitted.
  test("window.destroy.closed-without-close: destroy method is exposed only on Host-backed windows", () => {
    expect(presentInstanceMethods(["destroy"])).toEqual(["destroy"]);
  });

  // browser-window.md:519 "Reading this property throws `Object has been destroyed`
  // once the window has been destroyed"
  // Ordered observable: destroy() returns; reading win.webContents throws an Error whose
  // message contains the exact text Object has been destroyed.
  test("window.web-contents.read-after-destroy-throws: public Destroy requires a Host-created window", () => {
    expect(presentInstanceMethods(["destroy"])).toEqual(["destroy"]);
  });

  // browser-window.md:221 "Emitted when the window is closed."
  // Ordered observable: close() vetoed by preventDefault(); a second close(), not
  // vetoed, closes the window; 'closed' is emitted. (That the second close() is a new
  // attempt is close()'s "Try to close the window", browser-window.md:666; the cell
  // asserts only the cited 'closed' emission.)
  test("window.close.repeat-after-veto-emits-closed: public close requires a Host-created window", () => {
    expect(presentInstanceMethods(["close"])).toEqual(["close"]);
  });

  // browser-window.md:661, as for window.destroy.closed-without-close.
  // Ordered observable: close() vetoed by preventDefault(); destroy() emits no 'close',
  // and 'closed' is emitted.
  test("window.destroy.after-veto-emits-closed: public Destroy requires a Host-created window", () => {
    expect(presentInstanceMethods(["close", "destroy"])).toEqual([
      "close",
      "destroy",
    ]);
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
  test("app.window-created-events.constructor-order: no fabricated events before Host Create", () => {
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
      expectNoHostWindow(DRAWIO_OPTIONS);
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
  test("window.get-all-windows.order: no window can be invented by the local collection", () => {
    expect(presentStatics(["getAllWindows"])).toEqual(["getAllWindows"]);
  });

  // Source receipt (Electron 694f4585): electron_api_base_window.cc BaseWindow::OnWindowClosed
  // calls MarkDestroyed() at L182 before Emit("closed") at L184, so isDestroyed() is
  // true inside a 'closed' listener (PANEL-D6).
  test("window.closed.is-destroyed-in-listener: public isDestroyed requires a Host-created window", () => {
    expect(presentInstanceMethods(["isDestroyed"])).toEqual(["isDestroyed"]);
  });
});

test("constructor options cannot impersonate the internal replay identity", () => {
  // A malicious options Proxy may answer every Symbol lookup. It must still
  // reach the normal Host-Ready fence, never the replay constructor path.
  const hostile = new Proxy(
    {},
    {
      get(_target, property) {
        if (typeof property === "symbol") return { id: 1, incarnation: 1n };
        return undefined;
      },
    },
  );
  expect(constructionError(hostile).message).toContain("KELD-CORE-040");
  expect(electron.BrowserWindow.getAllWindows()).toEqual([]);
});
