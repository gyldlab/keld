// Compile only. Keep this consumer independent of the public Electron entry
// and direct Node imports so neither can supply the private Node reference.
import {
  awaitOriginalIpcCloseVeto,
  consumeOriginalRecoveryStartup,
  destroyOriginalIpcDocument,
  isCurrentOriginalIpcDocument,
} from "electron/internal/original-ipc";

type OriginalEvent = Parameters<typeof destroyOriginalIpcDocument>[0];
declare const event: OriginalEvent;
declare const candidate: unknown;

const current: boolean = isCurrentOriginalIpcDocument(event);
const consumeStartup: (candidate: unknown) => boolean =
  consumeOriginalRecoveryStartup;
const recoveredStartup: boolean = consumeStartup(candidate);
const veto: Promise<void> = awaitOriginalIpcCloseVeto(event);
const destroyed: Promise<void> = destroyOriginalIpcDocument(event);
const frame: { readonly url: string; readonly origin: string } | null =
  event.senderFrame;
const documentUrl: string = event.sender.getURL();
const contentsDestroyed: boolean = event.sender.isDestroyed();
event.sender.on("destroyed", () => {});
event.reply("mainResp", { success: true, reqId: 73 });
void [
  current,
  recoveredStartup,
  veto,
  destroyed,
  frame,
  documentUrl,
  contentsDestroyed,
];

// @ts-expect-error A plain object lacks the required retained-event shape.
isCurrentOriginalIpcDocument({});
// @ts-expect-error Veto accepts the event, not a caller-selected numeric ID.
awaitOriginalIpcCloseVeto(7);
// @ts-expect-error Conditional destruction requires the event argument.
destroyOriginalIpcDocument();
// @ts-expect-error The predicate is synchronous, not a Promise.
const asyncCurrent: Promise<boolean> = isCurrentOriginalIpcDocument(event);
// @ts-expect-error Veto completion is awaited, never a boolean disposition.
const immediateVeto: boolean = awaitOriginalIpcCloseVeto(event);
// @ts-expect-error Destruction must retain Promise<void>, not an unknown result.
const destroyResult: Promise<number> = destroyOriginalIpcDocument(event);
// @ts-expect-error The recovery decision is synchronous, not a Promise.
const asyncRecovery: Promise<boolean> =
  consumeOriginalRecoveryStartup(candidate);
void [asyncCurrent, immediateVeto, destroyResult, asyncRecovery];

// @ts-expect-error Installed private declarations must not introduce Bun globals.
Bun;
