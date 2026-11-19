// GH556's observed Windows application-menu absence.
import { hostLaunchFacts } from "@keld/kipc";
import { refuse } from "./unsupported.ts";

/** Application menu surface for the qualified already-absent Windows route. */
export class Menu {
  constructor() {
    refuse("constructor", "Menu", "GH-556");
  }

  /** Accept the exact absent application-menu fact. No native request is sent. */
  static setApplicationMenu(menu: Menu | null): void {
    if (menu !== null) refuse("method", "Menu.setApplicationMenu", "GH-556");
    if (
      hostLaunchFacts?.schema === 8 &&
      hostLaunchFacts.windowsApplicationMenu === "absent-v1"
    )
      return;

    refuse("method", "Menu.setApplicationMenu(null)", "GH-556");
  }
}
