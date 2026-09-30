/**
 * Host-lifecycle channel adapter over the canonical kipc transport (KEL-72 / KEL-136).
 *
 * Framing, HELLO, deadlines, buffering, and serialized writes live in `@keld/kipc`.
 * This file owns lifecycle postcard enums, Quit dispatch, and Electron-facing
 * listener isolation. It does not import Electron at runtime.
 */
export {
  APP_LINK_IO_DEADLINE_MS,
  DrainSignal,
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

// Same-module bindings for this adapter and for concatenated KEL-96 host
// fixtures that append `t1b_harness.ts` to this file as `src/main.ts`.
import {
  DrainSignal,
  FrameKind,
  FrameReader,
  LIFECYCLE_CHANNEL,
  RECEIVE_POLICIES,
  WriteQueue,
  connectKipcSocket,
  errorFromErrFrame,
  isWin32PipeEndpoint,
  kipcError,
  lifecycleReplyWaiter,
  parseAppLink,
  parseWin32DiagnosticPort,
  timingSafeEqual,
  validateReceivedHeader,
  withIoDeadline,
  type KipcSocket,
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
  /**
   * Read loop died after HELLO. `app.whenReady()` waiters must reject here;
   * a throw must not skip `#quitWaiter` drain.
   */
  onLinkDead: (err: Error) => void;
};

/**
 * One HELLO'd app-link that demuxes lifecycle Events vs the Quit Reply.
 */
export class LifecycleLink {
  #socket: KipcSocket;
  #reader: FrameReader;
  #writes: WriteQueue;
  #nextCorr = 1;
  #closed = false;
  #quitWaiter: { corr: number; resolve: () => void; reject: (e: Error) => void } | null = null;
  #quitPromise: Promise<void> | undefined;
  #loopStarted = false;

  private constructor(socket: KipcSocket, reader: FrameReader, writes: WriteQueue) {
    this.#socket = socket;
    this.#reader = reader;
    this.#writes = writes;
  }

  static async connect(link: string, handlers: LifecycleHandler): Promise<LifecycleLink> {
    const { endpoint, token } = parseAppLink(link);
    const reader = new FrameReader();
    const drain = new DrainSignal();
    const socket = await connectKipcSocket(endpoint, reader, drain);
    const writes = new WriteQueue(socket, drain);
    const session = new LifecycleLink(socket, reader, writes);
    try {
      await withIoDeadline(writes.writeFrame(FrameKind.Hello, 0, 0, 0, token));
      const helloReply = await withIoDeadline(reader.readFrame());
      validateReceivedHeader(RECEIVE_POLICIES.clientAwaitHello, helloReply.header);
      if (!timingSafeEqual(helloReply.payload, token)) {
        throw kipcError("KELD-IPC-007", "HELLO session token mismatch");
      }
      session.#startLoop(handlers);
      return session;
    } catch (err) {
      session.close();
      throw err;
    }
  }

  #startLoop(handlers: LifecycleHandler): void {
    if (this.#loopStarted) return;
    this.#loopStarted = true;
    const run = async (): Promise<void> => {
      for (;;) {
        const frame = await this.#reader.readFrame();
        if (frame.header.kind === FrameKind.Ping) {
          validateReceivedHeader(RECEIVE_POLICIES.lifecycleEventReceiver, frame.header);
          await withIoDeadline(
            this.#writes.writeFrame(
              FrameKind.Ping,
              0,
              frame.header.channel,
              frame.header.corr,
              new Uint8Array(),
            ),
          );
          continue;
        }
        if (frame.header.kind === FrameKind.Event) {
          validateReceivedHeader(RECEIVE_POLICIES.lifecycleEventReceiver, frame.header);
          const event = decodeEvent(frame.payload);
          if (event === "ready") handlers.onReady();
          else handlers.onLastWindowClosed();
          continue;
        }
        if (
          (frame.header.kind === FrameKind.Reply || frame.header.kind === FrameKind.Err) &&
          this.#quitWaiter !== null
        ) {
          const waiter = this.#quitWaiter;
          validateReceivedHeader(lifecycleReplyWaiter(waiter.corr), frame.header);
          this.#quitWaiter = null;
          if (frame.header.kind === FrameKind.Reply) {
            waiter.resolve();
          } else {
            waiter.reject(errorFromErrFrame(frame.payload));
          }
          continue;
        }
        validateReceivedHeader(RECEIVE_POLICIES.lifecycleEventReceiver, frame.header);
        throw kipcError("KELD-IPC-005", "frame kind is not declared by the session policy");
      }
    };
    void run().catch((err: Error) => {
      const localClose = this.#closed;
      try {
        this.close();
        if (!localClose) {
          try {
            handlers.onLinkDead(err);
          } catch {
            // Isolate: a throwing listener must not skip quit-waiter drain.
          }
        }
      } finally {
        const waiter = this.#quitWaiter;
        this.#quitWaiter = null;
        waiter?.reject(err);
      }
    });
  }

  async quit(): Promise<void> {
    if (this.#quitPromise) return this.#quitPromise;
    if (this.#closed) {
      throw kipcError("KELD-IPC-001", "session is closed");
    }
    this.#quitPromise = this.#quitOnce();
    return this.#quitPromise;
  }

  async #quitOnce(): Promise<void> {
    const corr = this.#nextCorr;
    let next = (corr + 1) >>> 0;
    if (next === 0) next = 1;
    this.#nextCorr = next;
    try {
      await withIoDeadline(
        new Promise<void>((resolve, reject) => {
          this.#quitWaiter = { corr, resolve, reject };
          void this.#writes
            .writeFrame(FrameKind.Call, 0, LIFECYCLE_CHANNEL, corr, new Uint8Array([0x00]))
            .catch((err: Error) => {
              this.#quitWaiter = null;
              reject(err);
            });
        }),
      );
    } catch (err) {
      this.#quitWaiter = null;
      throw err;
    } finally {
      this.close();
    }
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#socket.end();
  }
}
