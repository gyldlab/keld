/**
 * Electron main IPC registration and one authenticated-source delivery consumer.
 * The existing EventEmitter remains the listener owner; the separate invoke map
 * is not dispatched by this send/reply slice. The API singleton supplies native
 * facts; Core/native retain authentication, document/role fencing and final Guard.
 * Registering a listener creates no connection, principal or channel grant.
 */
import { EventEmitter } from "node:events";

import {
  stateMap,
  stateMapDelete,
  stateMapHas,
  stateMapSet,
  stateFreeze,
  stateWeakMap,
  stateWeakMapGet,
  stateWeakMapSet,
  linkError,
} from "@keld/kipc";
import type { WebContents } from "./browser-window.ts";
import type { WindowRef, WindowRequest } from "@keld/api/internal/window-codec";
import type {
  CompatControlEvent,
  CompatControlRequest,
  ConsumerDisposition,
} from "@keld/kipc/compat-control";
import {
  CompatCodecError,
  decodeEndpointArguments,
  encodeEndpointArguments,
} from "@keld/kipc/endpoint-value-codec";
import { refuse } from "./unsupported.ts";

/**
 * The `WebFrameMain` subset GH-466 names: the host-minted `url` and `origin`
 * of the native document that admitted the message; it is null after retirement.
 */
export interface WebFrameMain {
  readonly url: string;
  readonly origin: string;
}

/**
 * First-proof native-source event. senderFrame/reply retain the admitted original
 * document. processId/frameId remain typed refusals until their native owner
 * supplies actual opaque identities; view/nav/OS ids are never substituted.
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

/** Internal composition only. No method here authenticates supplied event data. */
export interface IpcMainDeliveryOwner {
  register(
    consumer: (
      event: CompatControlEvent,
      current: () => boolean,
    ) => ConsumerDisposition,
  ): void;
  findSender(ref: WindowRef): WebContents | undefined;
  send(request: CompatControlRequest, current: () => boolean): Promise<void>;
  /** Close owner awaits the exact Veto ReplyApplied where relevant, rechecks
   * current before admission and resolves only after physical Destroyed ACK.
   * A failure after admission keeps its existing unknown-effect semantics. */
  destroyOriginalDocument(
    request: Extract<WindowRequest, { variant: "DestroyOriginalDocument" }>,
    current: () => boolean,
  ): Promise<void>;
  /** Existing exact close attempt settlement; never derives a generation from
   * the latest mirror or infers Host acceptance from a local dialog Promise. */
  awaitCloseVeto(window: WindowRef, current: () => boolean): Promise<void>;
  report(error: unknown): void;
}
interface OriginalIpcDocumentOperation {
  current(): boolean;
  awaitCloseVeto(): Promise<void>;
  destroy(): Promise<void>;
}
const originalDocumentDestroy = stateWeakMap<
  IpcMainEvent,
  OriginalIpcDocumentOperation
>();
function originalCloseRefused(): Error {
  return linkError(
    "KELD-CORE-054",
    "The original IPC document is stale or unavailable. Retain the current window and wait for a new explicit current-document action.",
  );
}
/** @internal Close only the immutable original document represented by this
 * actual event; no caller field or latest WindowSource lookup selects a target.
 * Repeated calls share the first outcome, including a failure after admission. */
export function destroyOriginalIpcDocument(event: IpcMainEvent): Promise<void> {
  const destroy = stateWeakMapGet(originalDocumentDestroy, event);
  return destroy ? destroy.destroy() : Promise.reject(originalCloseRefused());
}
/** @internal Predicate only for a genuinely retained original native event. */
export function isCurrentOriginalIpcDocument(event: IpcMainEvent): boolean {
  const operation = stateWeakMapGet(originalDocumentDestroy, event);
  // Expired source observations are an ineligible event, never new authority.
  try {
    return operation?.current() === true;
  } catch {
    return false;
  }
}
/** @internal Await this original document's actual close-verdict settlement.
 * The per-window owner supplies its exact correlated Veto promise. */
export function awaitOriginalIpcCloseVeto(event: IpcMainEvent): Promise<void> {
  const operation = stateWeakMapGet(originalDocumentDestroy, event);
  return operation
    ? operation.awaitCloseVeto()
    : Promise.reject(originalCloseRefused());
}
let deliveryInstalled = false;
/** Compose the one registry with the existing authenticated API source. */
export function installIpcMainDelivery(owner: IpcMainDeliveryOwner): void {
  if (deliveryInstalled)
    throw new Error("KELD-IPC-005: ipcMain delivery is already installed");
  const consume = (
    event: CompatControlEvent,
    current: () => boolean,
  ): ConsumerDisposition => {
    if (event.variant !== "RendererMessage" || !current())
      return { variant: "NoListener" };
    const ref = { id: event.window_id, incarnation: event.window_incarnation };
    const sender = owner.findSender(ref);
    if (!sender) return { variant: "NoListener" };
    let args: unknown[];
    try {
      args = decodeEndpointArguments(event.payload);
    } catch (error) {
      const code =
        error instanceof CompatCodecError ? error.code : "KELD-COMPAT-101";
      return {
        variant: "Refused",
        error: {
          code,
          message:
            code +
            ": renderer arguments are invalid. Fix the endpoint value domain or limits before sending.",
        },
      };
    }
    const eligible = (): boolean =>
      current() && owner.findSender(ref) === sender;
    // Copy all original addressing before application code can run or await.
    const webview = event.webview,
      navigation = event.navigation;
    const url = event.url,
      origin = event.origin;
    const hostEvent: IpcMainEvent = {
      type: "frame",
      sender,
      // Native process/frame identities are absent from this first-proof DTO.
      // Never substitute a view id, navigation stamp or OS pid for those fields.
      get processId(): number {
        return refuse("property", "IpcMainEvent.processId", "GH-466");
      },
      get frameId(): number {
        return refuse("property", "IpcMainEvent.frameId", "GH-466");
      },
      get senderFrame(): WebFrameMain | null {
        return eligible() ? { url, origin } : null;
      },
      reply(topic: string, ...values: unknown[]): void {
        const payload = encodeEndpointArguments(values);
        if (!eligible()) return;
        void owner
          .send(
            {
              variant: "DeliverToOriginalDocument",
              webview,
              navigation,
              topic,
              payload,
            },
            eligible,
          )
          .catch((error: unknown) => owner.report(error));
      },
    };
    // The event capability captures native selectors before application code.
    // Weak ownership follows the retained event; it is not a native registry or
    // a successor-document authority. Generic BrowserWindow.destroy is separate.
    const originalRequest = stateFreeze({
      variant: "DestroyOriginalDocument" as const,
      window: stateFreeze({ id: ref.id, incarnation: ref.incarnation }),
      document_generation: navigation,
    });
    let teardown: Promise<void> | undefined;
    const destroy = (): Promise<void> => {
      if (teardown !== undefined) return teardown;
      if (!eligible()) teardown = Promise.reject(originalCloseRefused());
      else {
        try {
          teardown = owner.destroyOriginalDocument(originalRequest, eligible);
        } catch (error) {
          teardown = Promise.reject(error);
        }
      }
      // Do not recheck after success: genuine destruction itself retires the
      // original document. The close owner alone verifies the correlated ACK.
      return teardown;
    };
    let vetoWait: Promise<void> | undefined;
    const awaitCloseVeto = (): Promise<void> => {
      if (vetoWait !== undefined) return vetoWait;
      if (!eligible()) return Promise.reject(originalCloseRefused());
      let source: Promise<void>;
      try {
        source = owner.awaitCloseVeto(originalRequest.window, eligible);
      } catch (error) {
        return Promise.reject(error);
      }
      const waiting = source.then(
        () => {
          if (vetoWait === waiting) vetoWait = undefined;
          if (!eligible()) throw originalCloseRefused();
        },
        (error) => {
          if (vetoWait === waiting) vetoWait = undefined;
          throw error;
        },
      );
      vetoWait = waiting;
      return waiting;
    };
    stateWeakMapSet(originalDocumentDestroy, hostEvent, {
      current: eligible,
      awaitCloseVeto,
      destroy,
    });
    if (!eligible() || ipcMain.listenerCount(event.topic) === 0)
      return { variant: "NoListener" };
    try {
      // EventEmitter owns duplicates/once/reentrancy. Returned Promises are not replies.
      ipcMain.emit(event.topic, hostEvent, ...args);
      return { variant: "Consumed" };
    } catch (error) {
      owner.report(error);
      return {
        variant: "Refused",
        error: {
          code: "KELD-IPC-005",
          message: "application main listener threw during dispatch",
        },
      };
    }
  };
  owner.register(consume);
  deliveryInstalled = true;
}
