/**
 * GH-567 F07-T4: initial red-until-implemented renderer/preload corpus.
 *
 * Electron oracle: v44.4.5 (694f45852a0f1726cd23bfd379854de489cccb65),
 * docs/api/structures/web-preferences.md. The matching manifest is at
 * crates/keld-compat/fixtures/renderer-preload-corpus/corpus.json.
 *
 * These executable cases observe the current main-role entrypoint blocker.
 * They do NOT prove a native WKWebView ran, nor separately verify preload
 * timing, path admission or renderer argv. GH-484 must replace each red case
 * with a real renderer observable, then flip its verdict to pass.
 */
import { describe, expect, test } from "bun:test";

import * as electron from "./index";

/** Read either Electron-style constructor export exactly as an application does. */
function exportedBrowserWindow(): unknown {
  const named = Reflect.get(electron, "BrowserWindow") as unknown;
  const defaultExport: unknown = electron.default;
  const fallback =
    defaultExport !== null && typeof defaultExport === "object"
      ? Reflect.get(defaultExport, "BrowserWindow")
      : undefined;
  return named ?? fallback;
}

/**
 * Actually attempt configured window construction. At present there is no
 * BrowserWindow export; new throws before a renderer can launch.
 */
function attemptRendererLaunch(webPreferences: object): unknown {
  const BrowserWindow = exportedBrowserWindow() as new (options: object) => unknown;
  return new BrowserWindow({ webPreferences });
}

describe("GH-567 pending renderer preload conformance", () => {
  test("renderer.preload.before-first-inline today: no window can launch a preload", () => {
    // The proposed path would need a first-inline-script witness in a real WKWebView.
    expect(exportedBrowserWindow()).toBeUndefined();
    expect(() =>
      attemptRendererLaunch({
        contextIsolation: true,
        preload: "/gh567/renderer/preload.cjs",
      }),
    ).toThrow(TypeError);
  });

  test("renderer.preload.absolute-file-path today: no window can receive an absolute preload", () => {
    expect(exportedBrowserWindow()).toBeUndefined();
    expect(() =>
      attemptRendererLaunch({
        contextIsolation: true,
        preload: "/gh567/renderer/preload.cjs",
      }),
    ).toThrow(TypeError);
  });

  test("renderer.additional-arguments.argv today: no renderer can receive additionalArguments", () => {
    expect(exportedBrowserWindow()).toBeUndefined();
    expect(() =>
      attemptRendererLaunch({
        contextIsolation: true,
        additionalArguments: ["--gh567=first", "second arg with spaces"],
      }),
    ).toThrow(TypeError);
  });
});
