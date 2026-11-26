import electron, { BrowserWindow, app } from "electron";
import main from "electron/main";
import renderer from "electron/renderer";
import common from "electron/common";
import utility from "electron/utility";

const facades: (typeof electron)[] = [main, renderer, common, utility];
const apps: (typeof app)[] = facades.map((facade) => facade.app);
declare const window: BrowserWindow;
window.on("close", () => {});
void apps;
// @ts-expect-error Installed Electron declarations must not introduce Bun globals.
Bun;
