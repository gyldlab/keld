import {
  isCurrentOriginalIpcDocument,
  awaitOriginalIpcCloseVeto,
  destroyOriginalIpcDocument,
} from "electron/internal/original-ipc";
declare const original: Parameters<typeof isCurrentOriginalIpcDocument>[0];
const current: boolean = isCurrentOriginalIpcDocument(original);
const veto: Promise<void> = awaitOriginalIpcCloseVeto(original);
const destroyed: Promise<void> = destroyOriginalIpcDocument(original);
void current;
void veto;
void destroyed;
// @ts-expect-error No registry leaks through the private three-helper wrapper.
import { ipcMain } from "electron/internal/original-ipc";
// @ts-expect-error Internal declarations must not introduce Bun globals.
Bun;
