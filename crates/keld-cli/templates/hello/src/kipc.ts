/**
 * Hello echo adapter over the canonical kipc transport (KEL-136 / KEL-30).
 *
 * The app-link is the GH-527 `WorkerLink`: its transport Worker, whose entry is
 * `kipc-transport.ts` itself, owns the socket, HELLO, framing, deadlines and
 * writes, and answers host Pings. This file owns the echo postcard codecs, the
 * lifecycle EVENT decoding and `AppLinkSession`; `echo.generated.ts` owns only
 * the Rust-derived compile-time payload declarations. `keld create`
 * concatenates this file with `main-body.ts` into `src/main.ts` and stages the
 * transport beside it, never bundled into it (the Worker would otherwise load
 * the whole app).
 */
export * from "./kipc-transport.ts";
export type { EchoRequest, EchoResponse } from "./echo.generated.ts";

import type { EchoRequest, EchoResponse } from "./echo.generated.ts";

import {
  APP_LINK_IO_DEADLINE_MS,
  ECHO_CHANNEL,
  LIFECYCLE_CHANNEL,
  WorkerLink,
  decodePostcardStringAt,
  decodeVarint,
  encodePostcardString,
  encodeVarint,
  kipcError,
  quitAndCloseLink,
  type KeldCallError,
} from "./kipc-transport.ts";

/** Postcard encoding of `EchoRequest`: struct-as-tuple, field order = declaration order. */
export function encodeEchoRequest(req: EchoRequest): Uint8Array {
  const message = encodePostcardString(req.message, "echo message");
  const count = encodeVarint(req.count);
  const out = new Uint8Array(message.length + count.length);
  out.set(message, 0);
  out.set(count, message.length);
  return out;
}

/** Postcard decoding of `EchoResponse`. Rejects trailing bytes (mirrors `keld_ipc::codec::decode`). */
export function decodeEchoResponse(bytes: Uint8Array): EchoResponse {
  const [message, afterMessage] = decodePostcardStringAt(bytes, 0);
  const [count, afterCount] = decodeVarint(bytes, afterMessage);
  if (afterCount !== bytes.length) {
    throw kipcError("KELD-IPC-003", "trailing bytes after EchoResponse");
  }
  return { message, count };
}

/** A host lifecycle EVENT (`keld_ipc::LifecycleEvent`). */
export type LifecycleEventName = "ready" | "last-window-closed";

/** Postcard decoding of `LifecycleEvent`: one unit-enum byte. */
export function decodeLifecycleEvent(payload: Uint8Array): LifecycleEventName {
  if (payload.length !== 1) {
    throw kipcError("KELD-IPC-003", "lifecycle event must be one postcard enum byte");
  }
  if (payload[0] === 0) return "ready";
  if (payload[0] === 1) return "last-window-closed";
  throw kipcError("KELD-IPC-003", `unknown LifecycleEvent discriminant ${payload[0]}`);
}

/**
 * The role's one HELLO'd app-link (GH-527 `WorkerLink`). Echo CALLs share it
 * without another handshake; lifecycle EVENTs reach `onLifecycleEvent`
 * listeners; `quit()` is the link's last call. Mirrors
 * `keld_ipc::{handshake_client, echo_invoke}`.
 *
 * `echoRoundtrip` is the one-shot wrapper (connect, one CALL, close).
 */
export class AppLinkSession {
  readonly #link: WorkerLink;

  private constructor(link: WorkerLink) {
    this.#link = link;
  }

  /**
   * Connects and completes the v2 `HELLO` handshake. A realm opens one
   * app-link (GH-527 §4.2).
   *
   * @throws on I/O, protocol, or auth failure — messages carry `KELD-IPC-*`.
   */
  static async connect(link: string): Promise<AppLinkSession> {
    const workerLink = await WorkerLink.open({
      link,
      receive: { eventChannels: [LIFECYCLE_CHANNEL], callReceivers: [] },
    });
    // An EVENT this app cannot decode ends the link before any listener runs.
    workerLink.setStateApplier(LIFECYCLE_CHANNEL, (payload) => {
      decodeLifecycleEvent(payload);
    });
    return new AppLinkSession(workerLink);
  }

  /**
   * One echo `Call`/`Reply` on this connection. Does not handshake again.
   *
   * @throws on I/O, protocol, deadline, or codec error — `KELD-IPC-*`.
   */
  async echo(request: EchoRequest): Promise<EchoResponse> {
    const reply = await this.#link.call(
      ECHO_CHANNEL,
      encodeEchoRequest(request),
      APP_LINK_IO_DEADLINE_MS,
    );
    return decodeEchoResponse(reply);
  }

  /** Runs `listener` for each host lifecycle EVENT. Returns its removal. */
  onLifecycleEvent(listener: (event: LifecycleEventName) => void): () => void {
    return this.#link.onEvent(LIFECYCLE_CHANNEL, (payload) => {
      listener(decodeLifecycleEvent(payload));
    });
  }

  /** Runs `listener` once when the link ends, with its `KELD-IPC-*` error. */
  onEnd(listener: (error: KeldCallError) => void): () => void {
    return this.#link.onEnd(listener);
  }

  /**
   * Sends `Quit`, the link's last call, and closes the link on its REPLY
   * (GH-527 §4.9). Returns the host's `LifecycleResponse::Quit` bytes.
   */
  quit(): Uint8Array {
    return quitAndCloseLink(this.#link, APP_LINK_IO_DEADLINE_MS);
  }

  /** Ends the link. Safe to call more than once. */
  close(): void {
    this.#link.close();
  }
}

/**
 * Performs one echo round-trip: connect, `HELLO` handshake, one `Call`/`Reply`.
 *
 * One-shot wrapper over [`AppLinkSession`]. `link` is the `KELD_APP_LINK`
 * value (`<endpoint>#<64 hex chars>`).
 *
 * @throws on I/O failure, protocol mismatch, auth failure, or codec error —
 * error messages carry the matching `KELD-IPC-*` code from `keld-ipc`.
 */
export async function echoRoundtrip(link: string, request: EchoRequest): Promise<EchoResponse> {
  const session = await AppLinkSession.connect(link);
  try {
    return await session.echo(request);
  } finally {
    session.close();
  }
}
