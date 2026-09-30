import {
  ECHO_CHANNEL,
  LIFECYCLE_CHANNEL,
  decodePostcardStringAt,
  decodeVarint,
  encodePostcardString,
  encodeVarint,
} from "../../kipc/src/transport.ts";
import type { EchoRequest, EchoResponse } from "./echo.generated.ts";
import type { Unsubscribe } from "./app.ts";

export interface AppChannel<Request, Response> {
  readonly id: number;
  decodeRequest(payload: Uint8Array): Request;
  encodeResponse(value: Response): Uint8Array;
}

export type AppChannelHandler<Request, Response> =
  (request: Request) => Response | Promise<Response>;

export interface Channels {
  handle<Request, Response>(
    channel: AppChannel<Request, Response>,
    handler: AppChannelHandler<Request, Response>,
  ): Unsubscribe;
}

function apiError(detail: string): Error {
  return new Error(`KELD-API-001: ${detail}`);
}

function decodeEchoRequest(payload: Uint8Array): EchoRequest {
  const [message, afterMessage] = decodePostcardStringAt(payload, 0);
  const [count, end] = decodeVarint(payload, afterMessage);
  if (end !== payload.length) {
    throw apiError("trailing bytes after EchoRequest");
  }
  return { message, count };
}

function encodeEchoResponse(value: EchoResponse): Uint8Array {
  const message = encodePostcardString(value.message, "echo response message");
  const count = encodeVarint(value.count);
  const out = new Uint8Array(message.length + count.length);
  out.set(message, 0);
  out.set(count, message.length);
  return out;
}

export const echoChannel: AppChannel<EchoRequest, EchoResponse> = Object.freeze({
  id: ECHO_CHANNEL,
  decodeRequest: decodeEchoRequest,
  encodeResponse: encodeEchoResponse,
});

let echoHandler: AppChannelHandler<EchoRequest, EchoResponse> | undefined;

export const channels: Channels = {
  handle<Request, Response>(
    channel: AppChannel<Request, Response>,
    handler: AppChannelHandler<Request, Response>,
  ): Unsubscribe {
    if (channel.id === LIFECYCLE_CHANNEL) {
      throw apiError("lifecycle channel 3 is reserved");
    }
    if (channel !== echoChannel) {
      throw apiError("channel descriptor is not declared by this Keld build");
    }
    if (typeof handler !== "function") {
      throw apiError("application channel handler must be a function");
    }
    if (echoHandler !== undefined) {
      throw apiError("Echo channel already has a handler");
    }

    const registered =
      handler as unknown as AppChannelHandler<EchoRequest, EchoResponse>;
    echoHandler = registered;
    let active = true;
    return () => {
      if (!active) return;
      active = false;
      if (echoHandler === registered) echoHandler = undefined;
    };
  },
};

export async function dispatchApplicationCall(
  channel: number,
  payload: Uint8Array,
): Promise<Uint8Array> {
  if (channel !== ECHO_CHANNEL) {
    throw apiError(`undeclared application channel ${channel}`);
  }
  const handler = echoHandler;
  if (handler === undefined) {
    throw apiError("Echo channel has no registered handler");
  }
  const request = echoChannel.decodeRequest(payload);
  const response = await handler(request);
  return echoChannel.encodeResponse(response);
}

export type { EchoRequest, EchoResponse } from "./echo.generated.ts";
