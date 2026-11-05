import {
  encodeMenuRequest,
  decodeMenuInstallReceipt,
  type MenuRequestV1,
  type MenuInstallReceipt,
} from "./menu.generated.ts";
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
} from "@keld/kipc";

import {
  decodeRuntimeInitialized,
  decodeLifecycleEvent as decodeEvent,
} from "@keld/kipc/runtime-process-facts";
import { resolveEchoCall } from "./echo-call.ts";
import {
  decodeDialogReply,
  encodeDialogRequest,
  type DialogReplyV1,
  type DialogRequestV1,
  type MessageOptions,
  type OpenDialogOptions,
  type SaveDialogOptions,
} from "./dialog.generated.ts";
import type { WindowRef } from "./window-codec.ts";
import { attachMirror } from "./mirror.ts";
import { WindowSource } from "./window-source.ts";
import { CompatSource, type CompatConsumer } from "./compat-source.ts";
import {
  decodeCompatControlResponse,
  encodeCompatControlRequest,
  type CompatControlRequest,
  type CompatControlResponse,
} from "@keld/kipc/compat-control";
import {
  decodeWindowResponse,
  encodeWindowRequest,
  type WindowEvent,
  type WindowRequest,
  type WindowResponse,
} from "./window-codec.ts";

import {
  APP_LINK_IO_DEADLINE_MS,
  ECHO_CHANNEL,
  FS_CHANNEL,
  AUTH_CHANNEL,
  COMPAT_CONTROL_CHANNEL,
  DIALOG_CHANNEL,
  MENU_CHANNEL,
  LIFECYCLE_CHANNEL,
  WINDOW_CHANNEL,
  WorkerLink,
  encodeVarint,
  kipcError,
  linkError,
  nativeFacadeBootSelected,
  quitAndCloseLink,
  type KeldCallError,
  type WorkerReceiveTable,
} from "@keld/kipc";

export type { LifecycleEventName } from "@keld/kipc/runtime-process-facts";

export type LifecycleHandler = {
  /** Endpoint consumer of native-stamped facts; never an authority minter. */
  onCompatEvent?: CompatConsumer;
  onCompatError?: (error: unknown) => void;
  onReady: () => void;
  onLastWindowClosed: () => void;
  /** Scoped Host intent; the captured live session is rechecked by the consumer. */
  onQuitRequested?: (session: LifecycleLink) => void;
  /** The singleton belongs to @keld/api before a Ready callback may construct. */
  onConnected?: (session: LifecycleLink) => void;
  /** Host-owned window facts, dispatched only after the mirror applied them. */
  onWindowFact?: (fact: WindowEvent) => void;
  onApplicationCall?: (
    channel: number,
    payload: Uint8Array,
  ) => Promise<Uint8Array>;
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
  readonly #windowSource: WindowSource | undefined;
  #compatSource: CompatSource | undefined;
  #closed = false;
  /** The error that ended the link without a local close: later quits rethrow it. */
  #dead: Error | undefined;
  #quitPromise: Promise<void> | undefined;
  /** The one authenticated WorkerLink generation alone allocates nonces. */
  #nextDialogNonce = 1n;
  /** Authenticated lifecycle apply/dispatch ordinals across a parked CALL. */
  #appliedLastWindowClosed = 0;
  #dispatchedLastWindowClosed = 0;
  #handledLastWindowClosed = 0;

  private constructor(link: WorkerLink, windowSource?: WindowSource) {
    this.#link = link;
    this.#windowSource = windowSource;
  }

  /** Only a completed, accepted replay makes the native window mirror observable. */
  get windowSource(): WindowSource | undefined {
    return this.#windowSource;
  }

  /** @internal The existing role/link owns this observation, not a new generation mint. */
  get windowAdmissionOpen(): boolean {
    return (
      !this.#closed &&
      this.#dead === undefined &&
      this.#quitPromise === undefined &&
      this.#windowSource !== undefined &&
      this.#link.callAdmissionOpen
    );
  }

  /** @internal Retire only this original link with its actual post-handoff cause. */
  retireWindowHandoffFailure(error: Error): void {
    this.#dead ??= error;
    this.#closed = true;
    // Existing onEnd still disposes Sources after the earlier ring records drain.
    this.#link.close();
  }

  /**
   * Claim authenticated Host LastWindowClosed milestones pre-applied during
   * synchronous Destroy, after JS `closed` but before Destroy returns. The
   * cumulative watermark coalesces older pending milestones into one public
   * callback; their later ring dispatch cannot duplicate the notification.
   */
  consumeAppliedLastWindowClosed(): boolean {
    if (this.#closed) return false;
    if (
      this.#appliedLastWindowClosed <=
      Math.max(this.#dispatchedLastWindowClosed, this.#handledLastWindowClosed)
    )
      return false;
    this.#handledLastWindowClosed = this.#appliedLastWindowClosed;
    return true;
  }

  static async connect(
    link: string,
    handlers: LifecycleHandler,
    runtimeInitPayload?: Uint8Array,
  ): Promise<LifecycleLink> {
    // A verified schema-two Host sets the role hint before the Guardian spawns
    // Bun. This hint does not grant window authority: the existing authenticated
    // primary link, host Subscribe admission and per-window owner still do that.
    const facadeWindows = nativeFacadeBootSelected;
    const workerLink = await WorkerLink.open({
      link,
      receive: facadeWindows
        ? {
            ...LIFECYCLE_RECEIVE,
            eventChannels: [
              LIFECYCLE_CHANNEL,
              WINDOW_CHANNEL,
              COMPAT_CONTROL_CHANNEL,
            ],
          }
        : LIFECYCLE_RECEIVE,
    });
    const windowSource = facadeWindows
      ? new WindowSource((fact) => handlers.onWindowFact?.(fact))
      : undefined;
    const session = new LifecycleLink(workerLink, windowSource);
    if (windowSource) attachMirror(workerLink, windowSource);
    if (windowSource) {
      const compatSource = new CompatSource(
        COMPAT_CONTROL_CHANNEL,
        windowSource,
        (event, current) =>
          handlers.onCompatEvent?.(event, current) ?? { variant: "NoListener" },
        (request) => {
          if (session.#closed) return;
          void workerLink
            .call(
              COMPAT_CONTROL_CHANNEL,
              encodeCompatControlRequest(request),
              APP_LINK_IO_DEADLINE_MS,
            )
            .then((raw) => {
              if (decodeCompatControlResponse(raw).variant !== "Released")
                throw linkError(
                  "KELD-IPC-003",
                  "compat release received a delivery terminal",
                );
            })
            .catch((error: unknown) => {
              // Unconfirmed release never refunds Host capacity or retries.
              session.close();
              try {
                handlers.onCompatError?.(error);
              } catch {
                /* diagnostic only */
              }
            });
        },
      );
      session.#compatSource = compatSource;
      attachMirror(workerLink, compatSource);
    }
    let rawReady = false;
    let replayComplete = !facadeWindows;
    let readyPublished = false;
    const maybePublishReady = (): void => {
      if (!rawReady || !replayComplete || readyPublished) return;
      readyPublished = true;
      handlers.onReady();
    };
    // A lifecycle EVENT this role cannot decode ends the link (a throwing
    // applier, §4.6) before any listener runs; the listener then dispatches it.
    workerLink.setStateApplier(LIFECYCLE_CHANNEL, (payload) => {
      if (decodeEvent(payload) === "last-window-closed") {
        session.#appliedLastWindowClosed += 1;
      }
    });
    workerLink.onEvent(LIFECYCLE_CHANNEL, (payload) => {
      const event = decodeEvent(payload);
      if (event === "ready") {
        rawReady = true;
        maybePublishReady();
      } else if (event === "quit-requested") {
        // A buffered old/terminal event cannot become a successor's app effect.
        // The API owner additionally selects the exact original composition.
        if (readyPublished && session.windowAdmissionOpen)
          handlers.onQuitRequested?.(session);
      } else {
        session.#dispatchedLastWindowClosed += 1;
        // A native Destroy may already have claimed this Host milestone
        // before returning to JS. Mark an ordinary event handled *before*
        // application callbacks, which may reenter Destroy or Quit.
        if (
          session.#dispatchedLastWindowClosed > session.#handledLastWindowClosed
        ) {
          session.#handledLastWindowClosed =
            session.#dispatchedLastWindowClosed;
          handlers.onLastWindowClosed();
        }
      }
    });
    workerLink.setCallHandler(ECHO_CHANNEL, (payload) =>
      resolveEchoCall(ECHO_CHANNEL, payload, handlers.onApplicationCall),
    );
    workerLink.onEnd((err: KeldCallError) => {
      session.#compatSource?.end();
      // onEnd follows the earlier-record drain even when local Quit already
      // marked the session closed. Always release the retained source here.
      windowSource?.end(err);
      if (session.#closed) return;
      session.#closed = true;
      session.#dead = err;
      try {
        handlers.onLinkDead(err);
      } catch {
        // Isolate: a throwing listener must not escape the transport's task.
      }
    });
    handlers.onConnected?.(session);
    if (runtimeInitPayload !== undefined) {
      try {
        // The ordinary allocator yields correlation 1: no Subscribe or app call
        // precedes this private admission handshake on the same WorkerLink.
        decodeRuntimeInitialized(
          await workerLink.call(
            LIFECYCLE_CHANNEL,
            runtimeInitPayload,
            APP_LINK_IO_DEADLINE_MS,
          ),
        );
      } catch (error) {
        session.close();
        throw error;
      }
    }
    if (windowSource) {
      try {
        // Postcard LifecycleRequest::Subscribe { channel } discriminant=1.
        // The channel id itself comes only from the generated table. The Host
        // writes every replayed Created fact before its Subscribed reply on
        // this same authenticated link; call() applies them before settling.
        const request = Uint8Array.of(1, ...encodeVarint(WINDOW_CHANNEL));
        const reply = await workerLink.call(
          LIFECYCLE_CHANNEL,
          request,
          APP_LINK_IO_DEADLINE_MS,
        );
        if (reply.length !== 1 || reply[0] !== 1) {
          throw reply[0] === 2 && reply.length === 2
            ? linkError(
                "KELD-CORE-045",
                `Host refused native window subscription (reason ${reply[1]}). Do not publish facade Ready.`,
              )
            : linkError(
                "KELD-IPC-003",
                "Host returned an invalid native window subscription reply",
              );
        }
        const compatReply = await workerLink.call(
          LIFECYCLE_CHANNEL,
          Uint8Array.of(1, ...encodeVarint(COMPAT_CONTROL_CHANNEL)),
          APP_LINK_IO_DEADLINE_MS,
        );
        if (compatReply.length !== 1 || compatReply[0] !== 1)
          throw linkError(
            "KELD-CORE-045",
            "Host refused the compat subscription",
          );
        replayComplete = true;
        maybePublishReady();
      } catch (error) {
        session.close();
        throw error;
      }
    }
    return session;
  }

  /** Existing single-link carriage; native/role/Guard admission remains Host-owned. */
  async callCompat(
    request: CompatControlRequest,
    current: () => boolean,
    payload = encodeCompatControlRequest(request),
  ): Promise<CompatControlResponse> {
    if (
      this.#closed ||
      this.#quitPromise !== undefined ||
      !this.#compatSource ||
      !current()
    )
      throw (
        this.#dead ??
        linkError(
          "KELD-CORE-043",
          "compat original target is no longer current",
        )
      );
    if (!current())
      throw linkError(
        "KELD-CORE-043",
        "compat target retired before link admission",
      );
    const raw = await this.#link.call(
      COMPAT_CONTROL_CHANNEL,
      payload,
      APP_LINK_IO_DEADLINE_MS,
    );
    const response = decodeCompatControlResponse(raw);
    const expected =
      request.variant === "ReleaseConsumed" ? "Released" : "Deliver";
    if (response.variant !== expected) {
      this.close();
      throw linkError(
        "KELD-IPC-003",
        "compat reply variant does not match its operation",
      );
    }
    return response;
  }

  /** Target an existing native document from this link's single Host-fact map.
   * The outbound call carries the exact captured view/navigation; a newer
   * window or document cannot inherit this event after an async link yield.
   */
  sendToCurrentWindow(
    ref: Readonly<WindowRef>,
    topic: string,
    payload: Uint8Array,
    sessionCurrent: () => boolean,
  ): Promise<void> {
    const source = this.#compatSource;
    const selected = source?.captureForWindow(ref);
    if (
      !selected ||
      this.#closed ||
      this.#quitPromise !== undefined ||
      !sessionCurrent()
    ) {
      throw linkError(
        "KELD-CORE-043",
        "no unique current Host-authenticated renderer target",
      );
    }
    const { webview, navigation } = selected;
    const request: CompatControlRequest = {
      variant: "DeliverToOriginalDocument",
      webview,
      navigation,
      topic,
      payload,
    };
    // Encode before yielding and before returning to the original caller.
    // An invalid channel or endpoint cannot become an unhandled async reject.
    const encoded = encodeCompatControlRequest(request);
    const current = (): boolean =>
      !this.#closed &&
      this.#quitPromise === undefined &&
      this.#compatSource === source &&
      sessionCurrent() &&
      selected.current();
    return this.callCompat(request, current, encoded).then((reply) => {
      if (
        reply.variant !== "Deliver" ||
        reply.disposition.variant !== "Consumed"
      ) {
        throw linkError(
          "KELD-CORE-043",
          "original renderer event was not confirmed consumed by its current document",
        );
      }
    });
  }

  /** Synchronous Electron constructor/Destroy over the existing authenticated link. */
  callWindowBlocking(request: WindowRequest): WindowResponse {
    if (this.#closed || !this.#windowSource) {
      throw (
        this.#dead ??
        linkError(
          "KELD-CORE-045",
          "this role has no live native window subscription",
        )
      );
    }
    return decodeWindowResponse(
      this.#link.callBlocking(
        WINDOW_CHANNEL,
        encodeWindowRequest(request),
        APP_LINK_IO_DEADLINE_MS,
      ),
    );
  }

  /** Non-blocking state changes and close verdicts share the same window owner. */
  callWindow(request: WindowRequest): Promise<WindowResponse> {
    if (this.#closed || !this.#windowSource) {
      return Promise.reject(
        this.#dead ??
          linkError(
            "KELD-CORE-045",
            "this role has no live native window subscription",
          ),
      );
    }
    return this.#link
      .call(
        WINDOW_CHANNEL,
        encodeWindowRequest(request),
        APP_LINK_IO_DEADLINE_MS,
      )
      .then(decodeWindowResponse);
  }

  /** One bounded guarded install, with the actual Host generation as its receipt. */
  callMenu(request: MenuRequestV1): Promise<MenuInstallReceipt> {
    if (this.#closed || this.#quitPromise !== undefined) {
      return Promise.reject(
        this.#dead ??
          linkError("KELD-IPC-005", "menu installation link has ended"),
      );
    }
    const payload = encodeMenuRequest(request);
    return this.#link
      .call(MENU_CHANNEL, payload, APP_LINK_IO_DEADLINE_MS)
      .then(decodeMenuInstallReceipt);
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
      return Promise.reject(
        this.#dead ?? kipcError("KELD-IPC-001", "session is closed"),
      );
    }
    this.#closed = true;
    this.#quitPromise = quitAndCloseLink(this.#link, APP_LINK_IO_DEADLINE_MS);
    return this.#quitPromise;
  }

  /** Package-private outbound FS call, admitted only while this session is open. */
  callFs(payload: Uint8Array): Promise<Uint8Array> {
    if (this.#closed || this.#quitPromise !== undefined) {
      return Promise.reject(
        this.#dead ?? kipcError("KELD-IPC-001", "session is closed"),
      );
    }
    return this.#link.call(FS_CHANNEL, payload, APP_LINK_IO_DEADLINE_MS);
  }

  /** Package-private auth call with an explicit finite caller reply deadline. */
  callAuth(payload: Uint8Array, deadlineMs: number): Promise<Uint8Array> {
    if (this.#closed || this.#quitPromise !== undefined) {
      return Promise.reject(
        this.#dead ?? kipcError("KELD-IPC-001", "session is closed"),
      );
    }
    return this.#link.call(AUTH_CHANNEL, payload, deadlineMs);
  }

  /** Async native Message uses the existing authenticated WorkerLink. A
   * timeout posts a fresh Cancel CALL(c_cancel); its acknowledgement never
   * substitutes for the original terminal CALL(c_original). */
  async callMessageDialog(
    parent: WindowRef | null,
    options: MessageOptions,
  ): Promise<DialogReplyV1> {
    return this.#callNativeDialog(
      (nonce) => ({
        variant: "Message",
        nonce,
        parent,
        options,
      }),
      "Message",
    );
  }

  /** Both file operations use the same role nonce high-water, Guard and
   * WorkerLink correlations as Message; no second file-dialog channel. */
  async callFileDialog(
    variant: "Open" | "Save",
    parent: WindowRef | null,
    options: OpenDialogOptions | SaveDialogOptions,
  ): Promise<DialogReplyV1> {
    return this.#callNativeDialog(
      (nonce) =>
        variant === "Open"
          ? { variant, nonce, parent, options: options as OpenDialogOptions }
          : { variant, nonce, parent, options: options as SaveDialogOptions },
      variant,
    );
  }

  async #callNativeDialog(
    request: (nonce: bigint) => DialogRequestV1,
    expected: "Message" | "Open" | "Save",
  ): Promise<DialogReplyV1> {
    if (this.#closed || this.#quitPromise !== undefined) {
      throw this.#dead ?? kipcError("KELD-IPC-001", "session is closed");
    }
    if (this.#nextDialogNonce > 0xffff_ffff_ffff_ffffn) {
      throw kipcError(
        "KELD-IPC-003",
        "dialog generation nonce exhausted; restart the role",
      );
    }
    const nonce = this.#nextDialogNonce++;
    const payload = encodeDialogRequest(request(nonce));
    try {
      const raw = await this.#link.call(DIALOG_CHANNEL, payload, 600_000);
      const value = decodeDialogReply(raw);
      if (value.variant !== expected) {
        throw kipcError(
          "KELD-IPC-003",
          "native dialog original correlation received a different operation's reply",
        );
      }
      return value;
    } catch (error) {
      // A role-local timeout does not prove whether the Host opened a sheet.
      // Never replay the original effect, and do not await Cancel from a
      // synchronous caller or overwrite its original typed failure.
      if (
        !this.#closed &&
        (error as { code?: unknown } | null)?.code === "KELD-IPC-006"
      ) {
        const cancel = encodeDialogRequest({ variant: "Cancel", nonce });
        void this.#link
          .call(DIALOG_CHANNEL, cancel, APP_LINK_IO_DEADLINE_MS)
          .then(decodeDialogReply)
          .catch(() => {
            // Link failure independently retires the generation. The Host
            // cleanup is not claimed from this best-effort relay.
          });
      }
      throw error;
    }
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#compatSource?.end();
    this.#link.close();
  }
}
