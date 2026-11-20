/** GH481 S1: synchronous read of the admitted, already applied desktop mirror. */
import {
  getDesktopSnapshot,
  isDesktopS1Selected,
  type DesktopSnapshot,
} from "@keld/api/internal/app";
import { method, refuse, unsupportedSurface } from "./unsupported.ts";

/** Electron's documented logical rectangle; isolated from Host mirror storage. */
export interface DisplayRectangle {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The three properties required by original draw.io first-window placement. */
export interface Display {
  id: number;
  bounds: DisplayRectangle;
  workArea: DisplayRectangle;
  scaleFactor: number;
}

function copyDisplay(display: DesktopSnapshot["displays"][number]): Display {
  return {
    id: display.id,
    bounds: { ...display.bounds },
    workArea: { ...display.work_area },
    scaleFactor: display.scale_factor,
  };
}

const remaining = unsupportedSurface("screen", {
  getCursorScreenPoint: method("GH-558"),
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

export const screen = Object.freeze({
  ...remaining,
  getAllDisplays(): Display[] {
    if (!isDesktopS1Selected())
      return refuse("method", "screen.getAllDisplays", "GH-481");
    const snapshot = getDesktopSnapshot();
    return snapshot.displays.map(copyDisplay);
  },
  getPrimaryDisplay(): Display {
    if (!isDesktopS1Selected())
      return refuse("method", "screen.getPrimaryDisplay", "GH-481");
    const snapshot = getDesktopSnapshot();
    // The one Host validation owner has already required exactly one primary
    // display. Never fabricate a default if that invariant is violated.
    const primary = snapshot.displays.find(
      (display) => display.id === snapshot.primary_id,
    );
    if (primary === undefined)
      throw new Error(
        "KELD-IPC-005: admitted desktop primary identity is missing",
      );
    return copyDisplay(primary);
  },
});
