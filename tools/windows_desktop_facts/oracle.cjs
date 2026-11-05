// GH-1186 W4 Electron behavior oracle. Read-only: no theme/display/settings writes.
// Usage: electron.exe <abs path to this file> --out=<abs json path>
"use strict";
const fs = require("fs");
const path = require("path");
const { app, nativeTheme, screen, BrowserWindow } = require("electron");

const outArg = process.argv.find((a) => a.startsWith("--out="));
const outPath = outArg ? outArg.slice("--out=".length) : null;

// Cooperative 60-second timer. The runner separately enforces a 65-second
// process-family deadline and a fixed cleanup budget if this event loop stalls.
const kill = setTimeout(() => {
  process.stderr.write("KILL_SWITCH_60S\n");
  app.exit(3);
}, 60000);
kill.unref();

// Keep profile state out of the user's %APPDATA%.
if (outPath) {
  app.setPath(
    "userData",
    path.join(path.dirname(outPath), "userdata-" + process.versions.electron),
  );
}

function display(d) {
  return {
    id: d.id,
    label: d.label,
    bounds: d.bounds,
    workArea: d.workArea,
    size: d.size,
    workAreaSize: d.workAreaSize,
    scaleFactor: d.scaleFactor,
    rotation: d.rotation,
    internal: d.internal,
    nativeOrigin: d.nativeOrigin,
    displayFrequency: d.displayFrequency,
    colorDepth: d.colorDepth,
    allKeys: Object.keys(d).sort(),
  };
}

function sample(label) {
  const primary = screen.getPrimaryDisplay();
  const s = {
    label,
    tMs: Date.now(),
    windowsOpen: BrowserWindow.getAllWindows().length,
    shouldUseDarkColors: nativeTheme.shouldUseDarkColors,
    shouldUseHighContrastColors: nativeTheme.shouldUseHighContrastColors,
    shouldUseInvertedColorScheme: nativeTheme.shouldUseInvertedColorScheme,
    inForcedColorsMode: nativeTheme.inForcedColorsMode,
    shouldUseDarkColorsForSystemIntegratedUI:
      nativeTheme.shouldUseDarkColorsForSystemIntegratedUI,
    prefersReducedTransparency: nativeTheme.prefersReducedTransparency,
    themeSource: nativeTheme.themeSource,
    primary: display(primary),
    all: screen.getAllDisplays().map(display),
  };
  // Windows-only DIP<->physical helpers, applied to the primary work area.
  if (typeof screen.dipToScreenRect === "function") {
    s.primaryWorkAreaDipToScreenRect = screen.dipToScreenRect(
      null,
      primary.workArea,
    );
    s.primaryBoundsDipToScreenRect = screen.dipToScreenRect(
      null,
      primary.bounds,
    );
  }
  return s;
}

const result = {
  oracle: "gh1186-w4-electron",
  electronVersion: process.versions.electron,
  chromeVersion: process.versions.chrome,
  nodeVersion: process.versions.node,
  processType: process.type,
  platform: process.platform,
  arch: process.arch,
  electronRunAsNodeEnv:
    process.env.ELECTRON_RUN_AS_NODE === undefined
      ? null
      : process.env.ELECTRON_RUN_AS_NODE,
  samples: [],
};

app.whenReady().then(async () => {
  // draw.io order: read nativeTheme + screen after ready, before the first BrowserWindow.
  result.samples.push(sample("afterReady_beforeAnyWindow"));
  // Control: a hidden (show:false) window, to see whether first-window creation changes facts.
  const w = new BrowserWindow({ show: false, width: 200, height: 200 });
  result.samples.push(sample("afterHiddenWindowConstructed"));
  w.destroy();
  // Flat top-level fields requested by the brief (from the pre-window sample).
  const pre = result.samples[0];
  result.shouldUseDarkColors = pre.shouldUseDarkColors;
  result.shouldUseHighContrastColors = pre.shouldUseHighContrastColors;
  result.themeSource = pre.themeSource;
  result.primary = pre.primary;
  result.all = pre.all;
  const json = JSON.stringify(result, null, 2);
  if (outPath) fs.writeFileSync(outPath, json + "\n");
  process.stdout.write(json + "\n");
  app.quit();
});

app.on("window-all-closed", () => {});
