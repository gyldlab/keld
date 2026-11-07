/** Internal Keld migration seam. Shares the facade's retained-event capability. */
export {
  isCurrentOriginalIpcDocument,
  awaitOriginalIpcCloseVeto,
  destroyOriginalIpcDocument,
} from "./ipc-main.ts";
/** Internal one-shot original startup decision; public loadURL stays unchanged. */
export { consumeOriginalRecoveryStartup } from "./browser-window.ts";
