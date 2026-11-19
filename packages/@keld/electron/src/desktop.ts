/** GH481 S1 synchronous desktop reads from the selected role's local mirror. */
import { getDesktopSnapshot } from "@keld/api/internal/app";
import {
  linkError,
  stateDefineProperty,
  stateFreeze,
  windowsDesktopSnapshotSelected,
} from "@keld/kipc";

/** Electron logical desktop coordinates. */
export interface DisplayRectangle {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The bounded first-window placement subset of Electron Display. */
export interface Display {
  id: number;
  bounds: DisplayRectangle;
  workArea: DisplayRectangle;
  scaleFactor: number;
}

function copyDisplay(
  display: ReturnType<typeof getDesktopSnapshot>["displays"][number],
): Display {
  return {
    id: display.id,
    bounds: { ...display.bounds },
    workArea: { ...display.work_area },
    scaleFactor: display.scale_factor,
  };
}

export const screen = windowsDesktopSnapshotSelected
  ? stateFreeze({
      getAllDisplays(): Display[] {
        const { displays } = getDesktopSnapshot();
        const copies: Display[] = [];
        for (let index = 0; index < displays.length; index += 1) {
          // Do not call application-replaceable array methods or indexed setters
          // with the retained mirror or its copies.
          stateDefineProperty(copies, index, {
            __proto__: null,
            value: copyDisplay(displays[index]!),
            enumerable: true,
            configurable: true,
            writable: true,
          } as PropertyDescriptor);
        }
        return copies;
      },
      getPrimaryDisplay(): Display {
        const snapshot = getDesktopSnapshot();
        for (let index = 0; index < snapshot.displays.length; index += 1) {
          const display = snapshot.displays[index]!;
          if (display.id === snapshot.primary_id) return copyDisplay(display);
        }
        throw linkError(
          "KELD-IPC-005",
          "Admitted desktop primary identity is missing.",
        );
      },
    })
  : undefined;

export const nativeTheme = windowsDesktopSnapshotSelected
  ? stateFreeze({
      get shouldUseDarkColors(): boolean {
        return getDesktopSnapshot().should_use_dark_colors;
      },
    })
  : undefined;
