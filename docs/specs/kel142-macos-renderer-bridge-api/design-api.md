# Spec: KEL-142: constraints and public API

Part of: [index](index.md)
Owns: section 4, the design constraints and the frozen public API

## 4. Design: constraints and public API

### 3. Current facts that constrain the design

1. `@keld/api` is specified but absent.
2. `@keld/electron` currently owns a direct `LifecycleLink` over the canonical
   `@keld/kipc` transport.
3. The canonical TypeScript receiver policy already admits host-originated
   nonzero-correlation CALLs on `ECHO_CHANNEL = 1` and lifecycle traffic on
   `LIFECYCLE_CHANNEL = 3`; this slice does not need new KIPC frame bytes.
4. macOS still uses Wry 0.56.1 as an interim WKWebView scaffold, while
   `keld-wv` already directly depends on the pinned `objc2` /
   `objc2-web-kit` family.
5. Wry's public initialization-script API exposes main-frame selection but not
   a `WKContentWorld` selector. WebKit documents that the legacy
   `WKUserScript initWithSource:injectionTime:forMainFrameOnly:` initializer
   is equivalent to using `WKContentWorld.pageWorld`.
6. KEL-139 requires the trusted bridge logic to live outside page JavaScript.

Therefore the KEL-139 isolation claim cannot be satisfied by calling Wry's
ordinary initialization-script helper and treating it as an isolated preload.
The macOS bridge must install its trusted script/message handler into a
non-page `WKContentWorld` through the existing direct WebKit configuration
seam, or fail the task.

External semantics receipts:

- Wry builder: <https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html>
- WebKit `WKContentWorld`:
  <https://developer.apple.com/documentation/webkit/wkcontentworld>
- WebKit content-world message handler:
  <https://developer.apple.com/documentation/webkit/wkusercontentcontroller/addscriptmessagehandler(_:contentworld:name:)>
- WebKit `WKUserScript` page-world equivalence:
  <https://github.com/WebKit/WebKit/blob/main/Source/WebKit/UIProcess/API/Cocoa/WKUserScript.h>

### 4. Public API frozen by this child

#### 4.1 Renderer surface

KEL-142 ships exactly this renderer-visible floor:

```ts
interface KeldRendererBridge {
  invoke(
    channel: number,
    payload: Uint8Array,
    opts?: undefined,
  ): Promise<Uint8Array>;
}

interface Window {
  readonly keld: KeldRendererBridge;
}
```

Rules:

- `channel` is an unsigned nonzero 16-bit channel id.
- This slice routes exactly the declared Echo channel (`1`). Any other
  renderer-selected channel is rejected before app dispatch.
- `payload.byteLength <= 4096`. The 4 KiB value is reused from landed
  KEL-75/T3 rather than introducing a second inline-control budget.
- `invoke` snapshots/copies the payload synchronously before any asynchronous
  boundary; mutating the caller's `Uint8Array` after the call cannot change
  the bytes admitted by the host.
- `opts` has **no supported semantics** in KEL-142. Passing any value other
  than `undefined` rejects locally. Principal, role, app generation,
  navigation generation, deadline, grant, token, endpoint, retained handle, or
  retry policy can never be supplied through `opts`.
- Exactly one renderer-originated invoke may be pending per WebView in this
  minimum slice. A second invoke while the first is pending fails immediately;
  there is no hidden unbounded queue.
- The host, not the page, mints the KIPC CALL correlation id.
- No renderer CALL is automatically replayed after link loss, navigation, or
  Bun generation loss.
- `window.keld` and its `invoke` function are installed before page code,
  frozen, non-writable, and non-configurable.
- This task does not ship `send`, `on`, `stream`, `meta`, a `window.ipc`
  alias, or a direct native-handler object.

Future generated `@keld/web` clients may wrap this byte-level floor with typed
methods. KEL-142 does not create the general `@keld/schema` system.

#### 4.2 Bun `@keld/api` surface

KEL-142 creates one package-level lifecycle/channel owner:

```ts
export type Unsubscribe = () => void;

export interface AppLifecycle {
  whenReady(): Promise<void>;
  isReady(): boolean;
  onLastWindowClosed(listener: () => void): Unsubscribe;
  quit(): Promise<void>;
}

export const app: AppLifecycle;

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

export const channels: Channels;
```

Contract:

- `channels.handle` registers one handler per declared channel id.
- `LIFECYCLE_CHANNEL = 3` is reserved and cannot be registered as an
  application channel.
- KEL-142's product fixture uses a generated Echo
  `AppChannel<EchoRequest, EchoResponse>` derived from the existing KEL-98
  declarations/codecs. This is a bounded fixture, not general schema codegen.
- In this slice `channels.handle` accepts only that declared Echo descriptor
  (`id = 1`); arbitrary caller-constructed ids and lifecycle channel `3`
  reject locally. A generated multi-channel table remains later schema work.
- Duplicate registration, missing handler, handler failure, and malformed
  request payload are terminal outcomes for that CALL; none may invoke a second
  handler or produce a second reply.
- A malformed declared application payload may return one correlated typed
  error with zero handler effect. Header/session-shape failures remain owned by
  KEL-133 and stay link-terminal where its contract says so.
- `app` and `channels` share one authenticated app-link session, one
  `FrameReader`, and one serialized `WriteQueue`. They must not open a second
  socket/pipe or run overlapping readers.
- Listener/handler exceptions are isolated from the session reader and from
  unrelated lifecycle waiters.
- App-link death rejects `whenReady()` waiters and pending host CALL work with
  one terminal outcome.

#### 4.3 Electron compatibility ownership

`@keld/electron` becomes a compatibility consumer:

- `app.whenReady()` delegates to `@keld/api.app.whenReady()`.
- `app.isReady()` delegates to `@keld/api.app.isReady()`.
- Electron `window-all-closed` maps to
  `@keld/api.app.onLastWindowClosed(...)`.
- `app.quit()` delegates to `@keld/api.app.quit()`.

The direct lifecycle transport implementation must not remain duplicated in
both packages. Compatibility behavior stays unchanged; only the generic owner
moves.
