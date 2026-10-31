/**
 * Electron `ipcMain` registration facade: the GH-466 registration slice.
 *
 * This module owns two tables and nothing else: the main-process listener
 * registry (`on`, `once`, `addListener`, `off`, `removeListener`,
 * `removeAllListeners`) and the invoke-handler table (`handle`, `handleOnce`,
 * `removeHandler`). It delivers no message. No `IpcMainEvent` or
 * `IpcMainInvokeEvent` is minted here; there is no sender, no reply, no
 * `el:<channel>` grant evaluation and no invoke dispatch. A registered
 * listener or handler is therefore never invoked by Keld today, not because
 * it is dropped but because no sender exists yet. Delivery over the
 * host-routed grants is the GH-466 delivery slice (#541/#552 grants,
 * #542 navigation identity); invoke dispatch is GH-547.
 * Spec: docs/specs/gh466-ipcmain-registration.md.
 *
 * Reuse (Electron v44.4.5 `lib/browser/ipc-main-impl.ts` at
 * 694f45852a0f1726cd23bfd379854de489cccb65): Electron's own `IpcMainImpl`
 * is `EventEmitter` plus one `Map` of invoke handlers and a no-op `error`
 * listener. The listener registry is the runtime EventEmitter the
 * BrowserWindow facade already sits on; the handler table is the
 * framework-owned `stateMap` the app facade already uses. Neither
 * `@keld/api`'s single-handler channel map nor a second listener table is
 * introduced (GH540 §4.10).
 */
import { EventEmitter } from "node:events";

import {
  stateMap,
  stateMapDelete,
  stateMapHas,
  stateMapSet,
} from "../../kipc/src/transport.ts";
import type { WebContents } from "./browser-window.ts";

/**
 * The `WebFrameMain` subset GH-466 names: the host-minted `url` and `origin`
 * of the navigation that admitted a message. No value of this type exists yet.
 */
export interface WebFrameMain {
  readonly url: string;
  readonly origin: string;
}

/**
 * Electron `IpcMainEvent` (`docs/api/structures/ipc-main-event.md` at the
 * pin), exported as a type only: this slice mints no instance and exports no
 * value binding. `processId` and `frameId` are host-provenance opaque ids,
 * never OS process ids. `returnValue` (GH-548) and `ports` (GH-468) belong to
 * other tickets and are deliberately absent until they land.
 */
export interface IpcMainEvent {
  readonly type: "frame";
  readonly processId: number;
  readonly frameId: number;
  readonly sender: WebContents;
  readonly senderFrame: WebFrameMain | null;
  reply(channel: string, ...args: unknown[]): void;
}

/**
 * Electron `IpcMainInvokeEvent` (`docs/api/structures/ipc-main-invoke-event.md`
 * at the pin); a type only, minted by GH-547.
 */
export interface IpcMainInvokeEvent {
  readonly type: "frame";
  readonly processId: number;
  readonly frameId: number;
  readonly sender: WebContents;
  readonly senderFrame: WebFrameMain | null;
}

/**
 * A main-process message listener. Message arguments stay `unknown` until the
 * GH-544 endpoint codec names the supported value domain.
 */
export type IpcMainListener = (event: IpcMainEvent, ...args: unknown[]) => void;

/** An invoke handler. Its return value becomes a reply only once GH-547 dispatches invokes. */
export type IpcMainInvokeHandler = (
  event: IpcMainInvokeEvent,
  ...args: unknown[]
) => unknown;

/** The public `ipcMain` surface (`docs/api/ipc-main.md` at the pin). */
export interface IpcMain extends EventEmitter {
  on(channel: string, listener: IpcMainListener): this;
  once(channel: string, listener: IpcMainListener): this;
  addListener(channel: string, listener: IpcMainListener): this;
  off(channel: string, listener: IpcMainListener): this;
  removeListener(channel: string, listener: IpcMainListener): this;
  removeAllListeners(channel?: string): this;
  handle(channel: string, listener: IpcMainInvokeHandler): void;
  handleOnce(channel: string, listener: IpcMainInvokeHandler): void;
  removeHandler(channel: string): void;
}

class IpcMainImpl extends EventEmitter implements IpcMain {
  /** One invoke handler per channel, separate from the listener registry. */
  readonly #invokeHandlers = stateMap<string, IpcMainInvokeHandler>();

  constructor() {
    super();
    // Electron: a message on the channel named "error" must not raise the
    // EventEmitter unhandled-error exception in the main process.
    this.on("error", () => {});
  }

  // Own properties, as in Electron, so `const { handle } = ipcMain` stays bound.
  handle = (channel: string, listener: IpcMainInvokeHandler): void => {
    // Electron checks for a second handler before it checks the type.
    if (stateMapHas(this.#invokeHandlers, channel)) {
      throw new Error(
        `KELD-COMPAT-098: Attempted to register a second handler for '${channel}'. Call ipcMain.removeHandler('${channel}') before registering a replacement; Electron keeps one invoke handler per channel.`,
      );
    }
    if (typeof listener !== "function") {
      throw new TypeError(
        `KELD-COMPAT-099: Expected handler to be a function, but found type '${typeof listener}'. Pass the invoke handler function as the second argument of ipcMain.handle(channel, handler).`,
      );
    }
    stateMapSet(this.#invokeHandlers, channel, listener);
  };

  handleOnce = (channel: string, listener: IpcMainInvokeHandler): void => {
    // Pinned Electron wraps without checking `listener`; the wrapper removes
    // itself before the first call. Nothing invokes it in this slice.
    this.handle(channel, (event, ...args) => {
      this.removeHandler(channel);
      return listener(event, ...args);
    });
  };

  removeHandler(channel: string): void {
    stateMapDelete(this.#invokeHandlers, channel);
  }
}

/** The one main-process `ipcMain`, shared by the named and default `electron` exports. */
export const ipcMain: IpcMain = new IpcMainImpl();
