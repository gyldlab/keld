// GH556's observed Windows application-menu absence.
import { hostLaunchFacts, linkError } from "@keld/kipc";

function refuseMenu(member: "Menu" | "Menu.setApplicationMenu"): never {
  throw linkError(
    "KELD-COMPAT-013",
    `${member} is unsupported here. GH-556 permits only setApplicationMenu(null) with verified Windows menu absence.`,
  );
}

/** Application menu surface for the qualified already-absent Windows route. */
export class Menu {
  constructor() {
    refuseMenu("Menu");
  }

  /** Accept the exact absent application-menu fact. No native request is sent. */
  static setApplicationMenu(menu: Menu | null): void {
    if (
      menu === null &&
      hostLaunchFacts?.schema === 8 &&
      hostLaunchFacts.windowsApplicationMenu === "absent-v1"
    )
      return;

    refuseMenu("Menu.setApplicationMenu");
  }
}
