/**
 * Host-lifecycle channel adapter over the canonical kipc transport (KEL-72 / KEL-136).
 *
 * The link is the GH-527 `WorkerLink`: its transport Worker owns the socket,
 * framing, HELLO, deadlines and writes. This file owns lifecycle postcard
 * enums, the Quit, and Electron-facing listener isolation. It does not import
 * Electron at runtime.
 */
export {
  APP_LINK_IO_DEADLINE_MS,
  DrainSignal,
  ECHO_CHANNEL,
  FLAG_RAW,
  FrameKind,
  FrameReader,
  LIFECYCLE_CHANNEL,
  RECEIVE_POLICIES,
  WriteQueue,
  decodeCallError,
  echoReplyWaiter,
  errorFromErrFrame,
  encodeHeader,
  isCallError,
  isWin32PipeEndpoint,
  lifecycleReplyWaiter,
  parseAppLink,
  parseWin32Port,
  primaryAppReceiver,
  privilegedCallReceiver,
  validateReceivedHeader,
  withIoDeadline,
  type KeldCallError,
  type ReceivePolicy,
} from "../../kipc/src/transport.ts";

import { resolveEchoCall } from "./echo-call.ts";

import {
  APP_LINK_IO_DEADLINE_MS,
  ECHO_CHANNEL,
  LIFECYCLE_CHANNEL,
  WorkerLink,
  kipcError,
  quitAndCloseLink,
  type KeldCallError,
  type WorkerReceiveTable,
} from "../../kipc/src/transport.ts";

export type LifecycleEventName = "ready" | "last-window-closed";

function decodeUnitEnum(bytes: Uint8Array): number {
  if (bytes.length !== 1) {
    throw kipcError("KELD-IPC-003", "lifecycle enum must be a single postcard varint byte");
  }
  return bytes[0];
}

function decodeEvent(bytes: Uint8Array): LifecycleEventName {
  const disc = decodeUnitEnum(bytes);
  if (disc === 0) return "ready";
  if (disc === 1) return "last-window-closed";
  throw kipcError("KELD-IPC-003", `unknown LifecycleEvent discriminant ${disc}`);
}

export type LifecycleHandler = {
  onReady: () => void;
  onLastWindowClosed: () => void;
  onApplicationCall?: (channel: number, payload: Uint8Array) => Promise<Uint8Array>;
  /**
   * The link ended after HELLO without a local `close()` or `quit()`.
   * `app.whenReady()` waiters must reject here.
   */
  onLinkDead: (err: Error) => void;
};

/** What the host may send this role besides replies to its own calls (GH-527 §4.7). */
const LIFECYCLE_RECEIVE: WorkerReceiveTable = {
  eventChannels: [LIFECYCLE_CHANNEL],
  callReceivers: [{ policy: "echoReceiver" }],
};

/**
 * The role's one app-link, owned by the GH-527 transport Worker: lifecycle
 * EVENTs, host Echo CALLs, and the role's Quit. Pings are answered by the
 * Worker.
 */
export class LifecycleLink {
  readonly #link: WorkerLink;
  #closed = false;
  /** The error that ended the link without a local close: later quits rethrow it. */
  #dead: KeldCallError | undefined;
  #quitPromise: Promise<void> | undefined;

  private constructor(link: WorkerLink) {
    this.#link = link;
  }

  static async connect(link: string, handlers: LifecycleHandler): Promise<LifecycleLink> {
    const workerLink = await WorkerLink.open({ link, receive: LIFECYCLE_RECEIVE });
    const session = new LifecycleLink(workerLink);
    // A lifecycle EVENT this role cannot decode ends the link (a throwing
    // applier, §4.6) before any listener runs; the listener then dispatches it.
    workerLink.setStateApplier(LIFECYCLE_CHANNEL, (payload) => {
      decodeEvent(payload);
    });
    workerLink.onEvent(LIFECYCLE_CHANNEL, (payload) => {
      if (decodeEvent(payload) === "ready") handlers.onReady();
      else handlers.onLastWindowClosed();
    });
    workerLink.setCallHandler(ECHO_CHANNEL, (payload) =>
      resolveEchoCall(ECHO_CHANNEL, payload, handlers.onApplicationCall),
    );
    workerLink.onEnd((err: KeldCallError) => {
      if (session.#closed) return;
      session.#closed = true;
      session.#dead = err;
      try {
        handlers.onLinkDead(err);
      } catch {
        // Isolate: a throwing listener must not escape the transport's task.
      }
    });
    return session;
  }

  /**
   * Sends the role's Quit without parking (PANEL-P2: `app.quit` returns
   * immediately) and closes the link once its REPLY or typed failure settles
   * (GH-527 §4.9). Resolves on `LifecycleResponse::Quit`; a host ERR rejects
   * with that error, any other REPLY with `KELD-IPC-003`. Concurrent calls
   * share one Quit.
   */
  quit(): Promise<void> {
    if (this.#quitPromise) return this.#quitPromise;
    if (this.#closed) {
      return Promise.reject(this.#dead ?? kipcError("KELD-IPC-001", "session is closed"));
    }
    this.#closed = true;
    this.#quitPromise = quitAndCloseLink(this.#link, APP_LINK_IO_DEADLINE_MS);
    return this.#quitPromise;
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#link.close();
  }
}
