/**
 * The link-time bindings GH-565 adds and the member table behind them
 * (`docs/specs/gh565-link-time-electron-bindings.md` §4.3).
 *
 * Each binding exists so draw.io's `import { Menu, shell, dialog, session, screen,
 * ClipboardItem, clipboard, nativeImage, nativeTheme, ipcMain, app, BrowserWindow } from
 * "electron"` (`src/main/electron.js:5-6` at 2edf9fb9) links. Every member the pinned
 * Electron v44.4.5 page (694f45852a0f1726cd23bfd379854de489cccb65) documents at module or
 * static level refuses with `KELD-COMPAT-013` naming the issue that owns it; the `Returns
 * Promise<…>` lines of those pages decide which members reject instead of throwing.
 * Members no ticket covers yet name the F06 epic #476 (§ Not yet specified).
 *
 * An implementing ticket moves its module out of this file into its own module and
 * deletes the rows it implements in the same change that flips its corpus cells. Instance
 * members of a refusing constructor (`menu.popup`, `image.toPNG`) are unreachable and are
 * not declared.
 */
import {
  method,
  promise,
  property,
  refuse,
  refuseLater,
  unsupportedSurface,
  writableProperty,
} from "./unsupported";

/** Pinned `shell.md`: five members for the GH-479 broker; the Windows shortcut pair is a GH-482 scoreboard row. */
export const shell = unsupportedSurface("shell", {
  showItemInFolder: method("GH-479"),
  openPath: promise("GH-479"),
  openExternal: promise("GH-479"),
  trashItem: promise("GH-479"),
  beep: method("GH-479"),
  writeShortcutLink: method("GH-482"),
  readShortcutLink: method("GH-482"),
});

/** Pinned `clipboard.md` (W3C promise shape): draw.io's three members are GH-559's; the rest are unspecified on #476. */
export const clipboard = unsupportedSurface("clipboard", {
  readText: promise("GH-559"),
  writeText: promise("GH-559"),
  read: promise("GH-476"),
  write: promise("GH-559"),
  has: promise("GH-476"),
  clear: method("GH-476"),
  selection: property("GH-476"),
});

/**
 * Pinned `clipboard-item.md`: `new ClipboardItem(items)` takes a record keyed by MIME type
 * and constructs locally (GH-565 AC8); its payload members belong to GH-559.
 */
export class ClipboardItem {
  constructor(items: Readonly<Record<string, unknown>>) {
    if (items === null || typeof items !== "object") {
      throw new TypeError(
        "ClipboardItem items must be an object whose keys are MIME types (Electron 44.4.5 clipboard-item.md)",
      );
    }
  }

  get types(): never {
    return refuse("property", "ClipboardItem.types", "GH-559");
  }

  getType(..._type: unknown[]): Promise<never> {
    return refuseLater("ClipboardItem.getType", "GH-559");
  }
}

/** Pinned `native-image.md` module methods: `createFromDataURL` is GH-559's; the rest are unspecified on #476. */
export const nativeImage = unsupportedSurface("nativeImage", {
  createEmpty: method("GH-476"),
  createThumbnailFromPath: promise("GH-476"),
  createFromPath: method("GH-476"),
  createFromBitmap: method("GH-476"),
  createFromBuffer: method("GH-476"),
  createFromDataURL: method("GH-559"),
  createFromNamedImage: method("GH-476"),
  createMenuSymbol: method("GH-476"),
});

/** Pinned `native-theme.md`: `shouldUseDarkColors` is the GH-481 mirror fact; the rest, and the `updated` event, are GH-558's. */
export const nativeTheme = unsupportedSurface("nativeTheme", {
  shouldUseDarkColors: property("GH-481"),
  themeSource: writableProperty("GH-558"),
  shouldUseHighContrastColors: property("GH-558"),
  shouldUseDarkColorsForSystemIntegratedUI: property("GH-558"),
  shouldUseInvertedColorScheme: property("GH-558"),
  inForcedColorsMode: property("GH-558"),
  prefersReducedTransparency: property("GH-558"),
  shouldDifferentiateWithoutColor: property("GH-558"),
  on: method("GH-558"),
  once: method("GH-558"),
  off: method("GH-558"),
  addListener: method("GH-558"),
  removeListener: method("GH-558"),
  removeAllListeners: method("GH-558"),
});

/** Pinned `screen.md`: the two draw.io getters are GH-481 mirror facts; the rest, and the display events, are GH-558's. */
export const screen = unsupportedSurface("screen", {
  getCursorScreenPoint: method("GH-558"),
  getPrimaryDisplay: method("GH-481"),
  getAllDisplays: method("GH-481"),
  getDisplayNearestPoint: method("GH-558"),
  getDisplayMatching: method("GH-558"),
  screenToDipPoint: method("GH-558"),
  dipToScreenPoint: method("GH-558"),
  screenToDipRect: method("GH-558"),
  dipToScreenRect: method("GH-558"),
  on: method("GH-558"),
  once: method("GH-558"),
  off: method("GH-558"),
  addListener: method("GH-558"),
  removeListener: method("GH-558"),
  removeAllListeners: method("GH-558"),
});

/**
 * Pinned `menu.md` statics. draw.io's `buildFromTemplate` and `setApplicationMenu` are
 * GH-480's; construction, `getApplicationMenu` and item state are GH-556's;
 * `sendActionToFirstResponder` is unspecified on #476. `MenuItem` is deliberately not
 * exported: its appearance is the GH-480 transition gate in `native-surface.test.ts`.
 */
export class Menu {
  constructor() {
    refuse("constructor", "Menu", "GH-556");
  }

  static buildFromTemplate(..._template: unknown[]): never {
    return refuse("method", "Menu.buildFromTemplate", "GH-480");
  }

  static setApplicationMenu(..._menu: unknown[]): never {
    return refuse("method", "Menu.setApplicationMenu", "GH-480");
  }

  static getApplicationMenu(): never {
    return refuse("method", "Menu.getApplicationMenu", "GH-556");
  }

  static sendActionToFirstResponder(..._action: unknown[]): never {
    return refuse("method", "Menu.sendActionToFirstResponder", "GH-476");
  }
}

/** Pinned `dialog.md`: the three async dialogs are GH-478's broker, the sync prompt GH-564's, the remainder GH-561's. */
export const dialog = unsupportedSurface("dialog", {
  showOpenDialogSync: method("GH-561"),
  showOpenDialog: promise("GH-478"),
  showSaveDialogSync: method("GH-561"),
  showSaveDialog: promise("GH-478"),
  showMessageBoxSync: method("GH-564"),
  showMessageBox: promise("GH-478"),
  showErrorBox: method("GH-561"),
  showCertificateTrustDialog: promise("GH-561"),
});

/**
 * Pinned `session.md` module members. `defaultSession` refuses on read naming GH-553,
 * which owns the stable default-session object and its `webRequest` lane rules; the two
 * factories belong to GH-474 (one Session = the KEL-135 profile).
 */
export const session = unsupportedSurface("session", {
  fromPartition: method("GH-474"),
  fromPath: method("GH-474"),
  defaultSession: property("GH-553"),
});

/** Pinned `ipc-main.md`: the listener family is GH-466's registry; the `handle` family is GH-547's. */
export const ipcMain = unsupportedSurface("ipcMain", {
  on: method("GH-466"),
  off: method("GH-466"),
  once: method("GH-466"),
  addListener: method("GH-466"),
  removeListener: method("GH-466"),
  removeAllListeners: method("GH-466"),
  handle: method("GH-547"),
  handleOnce: method("GH-547"),
  removeHandler: method("GH-547"),
});
