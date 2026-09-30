/** Backend-independent application API (KEL-142). */
export { app, type AppLifecycle, type Unsubscribe } from "./app.ts";
export {
  channels,
  echoChannel,
  type AppChannel,
  type AppChannelHandler,
  type Channels,
} from "./channels.ts";
export type { EchoRequest, EchoResponse } from "./echo.generated.ts";
export { isCallError, type KeldCallError } from "./link.ts";
