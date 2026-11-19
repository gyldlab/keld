/** The four GH565 link-time rows needed by the Windows original main. */
import {
  stateDefineProperty,
  stateFreeze,
  stateIsArray,
  stateObjectEntries,
  stateTypeErrorConstructor,
} from "@keld/kipc";
import {
  method,
  promise,
  property,
  refuse,
  refuseLater,
  unsupportedSurface,
} from "./unsupported.ts";

const isWindows = process.platform === "win32";
const isLinux = process.platform === "linux";

/** The existing GH479 broker owns shell operations. */
export const shell = unsupportedSurface("shell", {
  showItemInFolder: method("GH-479"),
  openPath: promise("GH-479"),
  openExternal: promise("GH-479"),
  trashItem: promise("GH-479"),
  beep: method("GH-479"),
  ...(isWindows
    ? {
        writeShortcutLink: method("GH-482"),
        readShortcutLink: method("GH-482"),
      }
    : {}),
});

/** Electron v44 clipboard methods retain their documented return kind. */
export const clipboard = unsupportedSurface("clipboard", {
  readText: promise("GH-559"),
  writeText: promise("GH-559"),
  read: promise("GH-476"),
  write: promise("GH-559"),
  has: promise("GH-476"),
  clear: method("GH-476"),
  ...(isLinux ? { selection: property("GH-476") } : {}),
});

function invalidClipboardItem(): never {
  throw new stateTypeErrorConstructor(
    "ClipboardItem items must be a nonempty MIME-keyed record (Electron 44.4.5 clipboard-item.md)",
  );
}

/** A local snapshot under GH565 AC8. GH559 owns native payload access. */
export class ClipboardItem {
  readonly #payload: Readonly<Record<string, unknown>>;

  constructor(items: Readonly<Record<string, unknown>>) {
    if (items === null || typeof items !== "object" || stateIsArray(items))
      invalidClipboardItem();
    const entries = stateObjectEntries<unknown>(items);
    if (entries.length === 0) invalidClipboardItem();
    const payload = { __proto__: null } as Record<string, unknown>;
    for (let index = 0; index < entries.length; index++) {
      const entry = entries[index]!;
      const descriptor = { __proto__: null, value: entry[1], enumerable: true };
      stateDefineProperty(payload, entry[0], descriptor);
    }
    this.#payload = stateFreeze(payload);
  }

  get types(): never {
    return refuse("property", "ClipboardItem.types", "GH-559");
  }

  getType(..._type: unknown[]): Promise<never> {
    return refuseLater("ClipboardItem.getType", "GH-559");
  }
}

/** Image decoding remains with its existing native owner. */
export const nativeImage = unsupportedSurface("nativeImage", {
  createEmpty: method("GH-476"),
  ...(!isLinux ? { createThumbnailFromPath: promise("GH-476") } : {}),
  createFromPath: method("GH-476"),
  createFromBitmap: method("GH-476"),
  createFromBuffer: method("GH-476"),
  createFromDataURL: method("GH-559"),
  createFromNamedImage: method("GH-476"),
  createMenuSymbol: method("GH-476"),
});
