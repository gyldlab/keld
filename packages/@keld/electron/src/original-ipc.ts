/** Internal Keld migration seam. Shares the facade's retained-event capability. */
export {
  isCurrentOriginalIpcDocument,
  awaitOriginalIpcCloseVeto,
  destroyOriginalIpcDocument,
} from "./ipc-main.ts";
