/** GH481 S1 synchronous desktop reads from the selected role's local mirror. */
import { getDesktopSnapshot } from "@keld/api/internal/app";
import { linkError, windowsDesktopSnapshotSelected } from "@keld/kipc";

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
  ? Object.freeze({
      getAllDisplays(): Display[] {
        return getDesktopSnapshot().displays.map(copyDisplay);
      },
      getPrimaryDisplay(): Display {
        const snapshot = getDesktopSnapshot();
        const primary = snapshot.displays.find(
          (display) => display.id === snapshot.primary_id,
        );
        if (primary === undefined)
          throw linkError(
            "KELD-IPC-005",
            "Admitted desktop primary identity is missing.",
          );
        return copyDisplay(primary);
      },
    })
  : undefined;

export const nativeTheme = windowsDesktopSnapshotSelected
  ? Object.freeze({
      get shouldUseDarkColors(): boolean {
        return getDesktopSnapshot().should_use_dark_colors;
      },
    })
  : undefined;
