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
import { attachMirror } from "./mirror.ts";
import type { WindowSource } from "./window-source.ts";

import {
  APP_LINK_IO_DEADLINE_MS,
  ECHO_CHANNEL,
  FS_CHANNEL,
  AUTH_CHANNEL,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
  WorkerLink,
  decodeVarint,
  encodeVarint,
  kipcError,
  linkError,
  quitAndCloseLink,
  type KeldCallError,
  type WorkerReceiveTable,
} from "../../kipc/src/transport.ts";

export type LifecycleEventName = "ready" | "last-window-closed";

/** Closed lifecycle SubscribeRefusal vocabulary from keld-ipc/lifecycle.rs. */
export type WindowSubscribeRefusal = "UnknownChannel" | "NotSubscribable" | "GuardedNotSupported" | "RoleNotAdmitted";
/** Separate from raw lifecycle Ready; refusal never satisfies subscription. */
export type WindowSubscription = { readonly accepted: true } | { readonly accepted: false; readonly reason: WindowSubscribeRefusal };

/** Only primitive diagnostics survive the private subscription completion. */
type WindowSubscriptionOutcome =
  | { readonly result: WindowSubscription }
  | { readonly fault: { readonly name: string; readonly message: string; readonly code: string | number | undefined } };

function decodeWindowSubscription(payload: Uint8Array): WindowSubscription {
  const [variant, offset] = decodeVarint(payload, 0);
  if (variant === 1 && offset === payload.length) return { accepted: true };
  if (variant === 2) {
    const [reason, end] = decodeVarint(payload, offset);
    if (end === payload.length) {
      switch (reason) {
        case 0: return { accepted: false, reason: "UnknownChannel" };
        case 1: return { accepted: false, reason: "NotSubscribable" };
        case 2: return { accepted: false, reason: "GuardedNotSupported" };
        case 3: return { accepted: false, reason: "RoleNotAdmitted" };
      }
    }
  }
  throw kipcError("KELD-IPC-003", "invalid lifecycle window subscription reply; check the host lifecycle schema");
}

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

/** A concrete internal window consumer adds only its generated event channel. */
const WINDOW_RECEIVE: WorkerReceiveTable = {
  eventChannels: [LIFECYCLE_CHANNEL, WINDOW_CHANNEL],
  callReceivers: LIFECYCLE_RECEIVE.callReceivers,
};

/**
 * The role's one app-link, owned by the GH-527 transport Worker: lifecycle
 * EVENTs, host Echo CALLs, and the role's Quit. Pings are answered by the
 * Worker.
 */
export class LifecycleLink {
  readonly #link: WorkerLink;
  readonly #windowSubscriptionOutcome: Promise<WindowSubscriptionOutcome> | undefined;
  #closed = false;
  /** The error that ended the link without a local close: later quits rethrow it. */
  #dead: KeldCallError | undefined;
  #quitPromise: Promise<void> | undefined;

  private constructor(link: WorkerLink, windowSubscriptionOutcome: Promise<WindowSubscriptionOutcome> | undefined) {
    this.#link = link;
    this.#windowSubscriptionOutcome = windowSubscriptionOutcome;
  }

  /** A caller owns each completion Promise and any late materialized Error. */
  get windowSubscription(): Promise<WindowSubscription> | undefined {
    const completion = this.#windowSubscriptionOutcome?.then((outcome) => {
      if ("result" in outcome) return outcome.result;
      const { name, message, code } = outcome.fault;
      const error = typeof code === "string" ? linkError(code, "") : new Error(message);
      // Canonical linkError takes a detail. The stored message is already full.
      error.message = message;
      error.name = name;
      if (typeof code === "number") Object.defineProperty(error, "code", { value: code, enumerable: true, writable: true, configurable: true });
      throw error;
    });
    // Handle an ignored read while preserving rejection on the returned Promise.
    void completion?.catch(() => {});
    return completion;
  }

  static async connect(link: string, handlers: LifecycleHandler, source?: WindowSource): Promise<LifecycleLink> {
    const workerLink = await WorkerLink.open({ link, receive: source ? WINDOW_RECEIVE : LIFECYCLE_RECEIVE });
    let windowSubscriptionOutcome: Promise<WindowSubscriptionOutcome> | undefined;
    if (source) {
      attachMirror(workerLink, source);
      // onEnd runs only after retained records drain, including local close/Quit.
      workerLink.onEnd((error) => source.end(error));
      const channel = encodeVarint(WINDOW_CHANNEL);
      const request = new Uint8Array(1 + channel.length);
      request[0] = 1; // LifecycleRequest::Subscribe; the channel is generated.
      request.set(channel, 1);
      windowSubscriptionOutcome = workerLink.call(LIFECYCLE_CHANNEL, request, APP_LINK_IO_DEADLINE_MS).then(decodeWindowSubscription).then(
        (result): WindowSubscriptionOutcome => ({ result }),
        // Actual transport/decoder producers throw Errors. Preserve untyped codec
        // Errors and runtime Error names/numeric codes without inventing a code.
        (error: Error & { code?: string | number }): WindowSubscriptionOutcome => ({
          fault: { name: error.name, message: error.message, code: error.code },
        }),
      );
    }
    const session = new LifecycleLink(workerLink, windowSubscriptionOutcome);
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

  /** Package-private outbound FS call, admitted only while this session is open. */
  callFs(payload: Uint8Array): Promise<Uint8Array> {
    if (this.#closed || this.#quitPromise !== undefined) {
      return Promise.reject(this.#dead ?? kipcError("KELD-IPC-001", "session is closed"));
    }
    return this.#link.call(FS_CHANNEL, payload, APP_LINK_IO_DEADLINE_MS);
  }

  /** Package-private auth call with an explicit finite caller reply deadline. */
  callAuth(payload: Uint8Array, deadlineMs: number): Promise<Uint8Array> {
    if (this.#closed || this.#quitPromise !== undefined) {
      return Promise.reject(this.#dead ?? kipcError("KELD-IPC-001", "session is closed"));
    }
    return this.#link.call(AUTH_CHANNEL, payload, deadlineMs);
  }

  /** Internal window call; the future facade owns its Ready + Subscribed barrier. */
  callWindow(payload: Uint8Array): Promise<Uint8Array> {
    if (this.#closed || this.#quitPromise !== undefined) {
      return Promise.reject(this.#dead ?? kipcError("KELD-IPC-001", "session is closed"));
    }
    return this.#link.call(WINDOW_CHANNEL, payload, APP_LINK_IO_DEADLINE_MS);
  }

  /** Internal parked window call through the same Worker and finite deadline. */
  callWindowBlocking(payload: Uint8Array): Uint8Array {
    if (this.#closed || this.#quitPromise !== undefined) {
      throw this.#dead ?? kipcError("KELD-IPC-001", "session is closed");
    }
    return this.#link.callBlocking(WINDOW_CHANNEL, payload, APP_LINK_IO_DEADLINE_MS);
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#link.close();
  }
}
