# Spec: macOS minimum renderer bridge and `@keld/api` lifecycle/channel

**Status:** approved
**Linear:** KEL-142
**Parent contract:** KEL-139
**Updated:** 2026-09-30
**Approval:** active user session · approved content head `2e8da1b7ba19783eceb671093620419664537415` · Linear comment `53510bc6-66e0-441c-8992-010b6a8719e6` · decision SHA-256 `8e97d6daf9fe516b3844827bf0138af66a8343e37892ea278387f4f7c503a4a7`

{"schema":"keld.kel142-approval/v1","decision":"approved","approved_content_head":"2e8da1b7ba19783eceb671093620419664537415","linear_comment_id":"53510bc6-66e0-441c-8992-010b6a8719e6","source":"active-user-voice-session-2026-09-30"}

## 1. Goal

Ship the smallest real macOS renderer-to-app slice required by KEL-139:

```text
main-frame page
  -> window.keld.invoke(...)
  -> host-owned WKWebView bridge
  -> current admitted app-link generation
  -> @keld/api typed handler in Bun
  -> matching reply
  -> same document
```

The same `@keld/api` package also becomes the single generic owner for the
already-live host lifecycle semantics: Ready, LastWindowClosed, and Quit.

This task does **not** add filesystem authority, a second app-link transport,
general schema codegen, broad Electron compatibility, recovery/replay, Windows
or Linux qualification, `send/on/stream/meta`, or a bulk plane.

## 2. Authority and exact predecessors

This child consumes, rather than redefines:

- KEL-139 / PR #341, merged as
  `32093e1d789febda666b3fc04a5836999d429e8c`: approved minimum macOS
  product-spine contract.
- KEL-136: the canonical TypeScript app-link transport in
  `packages/@keld/kipc/src/transport.ts`.
- KEL-133: centralized receiver semantics, nonzero CALL correlation, absolute
  deadlines, and hostile transcript rules.
- KEL-75/T3 corrected lifecycle-routed virtual-port owner, including follow-up
  PR #59 merged as
  `756b6fb76030666f7b4ce56e423cdd9d1b32bff3`. Its already-published inline
  bound is `MAX_PORT_MESSAGE_LEN = 4096`.
- KEL-72: live `@keld/electron` lifecycle behavior that must be extracted,
  not copied.
- KEL-98: the bounded EchoRequest/EchoResponse declaration and codec fixture.
- KEL-80 comment "Product-spine task-level slice" as **scope/acceptance
  authority** for this first renderer slice.

KEL-80 is still Backlog and no separate passed `KEL-80/T*` execution artifact
exists. KEL-142 therefore owns the first implementation of this explicitly
authorized narrow slice. It must not claim that a nonexistent KEL-80 execution
artifact was consumed, and it must not mark broader KEL-80 acceptance complete.

## 3. Current facts that constrain the design

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

## 4. Public API frozen by this child

### 4.1 Renderer surface

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

### 4.2 Bun `@keld/api` surface

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

### 4.3 Electron compatibility ownership

`@keld/electron` becomes a compatibility consumer:

- `app.whenReady()` delegates to `@keld/api.app.whenReady()`.
- `app.isReady()` delegates to `@keld/api.app.isReady()`.
- Electron `window-all-closed` maps to
  `@keld/api.app.onLastWindowClosed(...)`.
- `app.quit()` delegates to `@keld/api.app.quit()`.

The direct lifecycle transport implementation must not remain duplicated in
both packages. Compatibility behavior stays unchanged; only the generic owner
moves.

## 5. macOS bridge trust boundary

### 5.1 Two worlds, one reviewed facade

The macOS WKWebView configuration installs before first navigation:

1. a **main-frame-only page-world facade** whose only public effect is frozen
   `window.keld.invoke`; and
2. a trusted script plus native message handler in a non-page
   `WKContentWorld` (named or `defaultClientWorld`).

The page world must not be able to access the native message-handler object,
its handler name, the app-link endpoint/token, the Bun process identity, the
current app generation, or the host's document binding.

A raw Wry `window.ipc` bridge is not accepted as evidence for this contract.

`WKContentWorld` is available on macOS 11+. If the required content-world
APIs are unavailable, KEL-142 fails construction with the typed bridge error;
it must not silently fall back to a page-world-only bridge merely to preserve
older hello-window availability.

The page-to-isolated-world handoff is not an authority boundary. If the
implementation uses a page-callable DOM/`postMessage` relay, that relay must
accept no broader shape than `window.keld.invoke`, carry no endpoint/token/
principal/generation/document nonce, and a forged relay message must be unable
to produce any host effect that the public facade could not already request.

### 5.2 Host-bound document identity

Each main-frame document performs an internal bind through the isolated world:

1. document-start isolated script asks the host to bind;
2. host verifies the message came from the expected WebView/main frame;
3. host mints a fresh opaque document nonce and records it against
   `(WebviewId, navigation generation)`;
4. isolated script retains the nonce outside the page world;
5. every native invoke carries that host-minted nonce;
6. provisional/new navigation invalidates the previous nonce before the new
   document may dispatch.

The page-world `invoke` payload contains only channel + application payload.
It never supplies the document nonce or any app authority.

Old nonce, wrong WebView, subframe-owned message, message after destruction, or
message from a retired navigation fails before app dispatch.

### 5.3 Private wv-link envelope

The isolated-world -> native control message is a bounded private
`keld.wv-link/v1` JSON envelope. It is **not** KIPC and cannot contain app
authority.

Accepted shapes are limited to:

```text
bind:
  { v: 1, kind: "bind" }

invoke:
  {
    v: 1,
    kind: "invoke",
    document: <host-minted nonce>,
    request: <nonzero u32 local request id>,
    channel: <nonzero u16>,
    payload: <array of 0..255 bytes, length <= 4096>
  }
```

Unknown fields/kinds, duplicate JSON keys, non-integers, zero request/channel,
payload elements outside 0..255, over-budget payload, malformed UTF-8/JSON, or
trailing non-whitespace fail closed with zero app dispatch.

The host does not reuse `request` as the app-link correlation id. It mints a
fresh nonzero KIPC correlation and retains only the one permitted pending map
entry.

The native reply returns the same local request id plus either response bytes
or a typed error. The isolated script resolves/rejects the page promise. Page
scripts may fake their own DOM state, but they cannot cause a native/app effect
without a host-accepted invoke.

## 6. Ordering, backpressure, disconnect

For one WebView:

```text
bind document
  -> renderer invoke admitted
  -> host mints app corr
  -> host sends one KIPC CALL
  -> Bun @keld/api handler runs once
  -> one REPLY or ERR
  -> host retires pending corr
  -> one renderer promise terminal outcome
```

Rules:

- no second renderer CALL is admitted while one is pending;
- slow Bun/app-link consumer cannot grow a renderer queue;
- navigation/close invalidates document authority immediately;
- a pending call invalidated by navigation gets one terminal renderer error;
  a later app reply is discarded as stale and cannot settle the promise again;
- app-link disconnect retires the pending call exactly once;
- no automatic replay occurs after reconnect/recovery;
- Quit/quiesce prevents new renderer dispatch before link teardown;
- no UI thread blocks on Bun, app-link I/O, or handler completion.

KEL-143 owns successor-generation recovery and a successful post-recovery
second guarded call. KEL-142 only makes the renderer/app call retirement
semantics safe enough for that later task.

## 7. Errors

Implementation registers, at minimum:

- `KELD-WV-011` — renderer bridge request rejected before app dispatch
  (malformed, stale document, wrong frame/view, over-budget, busy, undeclared
  renderer channel, unsupported `opts`, or destroyed bridge). The structured
  detail must distinguish these reasons without exposing endpoint/token or
  authority material.
- `KELD-API-001` — application channel registration/handler contract failure
  (duplicate/reserved/missing handler or handler failure), with no stack or
  secret returned to renderer.

Existing `KELD-IPC-001..007` keep their KEL-133 meanings. Do not allocate a
new KIPC error code for a webview-local validation failure.

## 8. Fuzzing and deterministic hostile tests

KEL-142 introduces a new untrusted decoder: `keld.wv-link/v1`. Therefore
fuzzing **does apply**.

Required:

- a raw-byte `cargo-fuzz` target for the pure renderer-envelope decoder;
- accepted fuzz inputs must satisfy every version/kind/integer/bounds/no-extra
  invariant;
- no panic, unbounded allocation, or accepted identity/authority field;
- every discovered bug becomes a permanent deterministic regression.

Fuzzing does not replace navigation-generation tests, real WKContentWorld
isolation tests, slow-consumer/pending-call tests, subprocess/app-link
disconnect tests, or real OS-visible click evidence.

## 9. Acceptance

### AC1 — package ownership

- `packages/@keld/api` exists and exports exactly the approved lifecycle/channel
  floor.
- `@keld/electron` delegates lifecycle behavior to it instead of maintaining a
  second transport owner.
- `tsc --strict`, no-public-`any`, Bun tests, and the canonical hostile KIPC
  corpus pass.

### AC2 — trusted macOS injection

On a real macOS WKWebView:

- page code observes `window.keld.invoke` before its first script runs;
- the facade is frozen/non-replaceable;
- an iframe has no own `window.keld`;
- popup/new-window escape is not created by this slice;
- page world cannot access the isolated native handler;
- raw endpoint/token/principal/generation/document nonce are absent from page
  globals and request arguments.

Deleting content-world isolation or changing the trusted script to page-world
only must fail a named test.

### AC3 — one real renderer -> Bun roundtrip

Using an OS-visible pointer event, not `element.click()`:

1. user activates the fixture button;
2. same document calls `window.keld.invoke(1, encodedEchoRequest)`;
3. host observes exactly one renderer admission;
4. host mints exactly one nonzero KIPC correlation;
5. one admitted Bun `@keld/api.channels.handle` handler receives the decoded
   EchoRequest;
6. one matching reply returns;
7. the same document renders the typed EchoResponse.

No stdout-grep or JS-only click stands in for any limb.

### AC4 — host lifecycle

The same `@keld/api` session proves:

- `app.whenReady()` does not resolve before real host Ready;
- `app.isReady()` reflects that state;
- `app.onLastWindowClosed` subscribes/unsubscribes without duplicate callbacks;
- `app.quit()` sends one correlated Quit and resolves only from host Reply;
- `@keld/electron` conformance remains behaviorally unchanged.

### AC5 — malformed/forged/stale input

Each independently proves zero application-handler effect:

- malformed JSON/envelope;
- unknown or extra field;
- zero/out-of-range request/channel;
- channel other than declared renderer Echo channel;
- unsupported `opts`;
- caller mutates the original `Uint8Array` after `invoke` (host still sees
  the snapshotted original bytes);
- >4096-byte payload;
- forged internal relay message cannot widen the facade contract;
- wrong/forged document nonce;
- old nonce after navigation begins;
- subframe/native-handler attempt;
- message after view destruction.

### AC6 — slow consumer and disconnect

- Hold the one admitted Bun handler pending.
- A second renderer invoke fails immediately as busy; memory/queue count does not
  grow with repeated attempts.
- Navigation or app-link disconnect settles the first renderer promise exactly
  once with a terminal error.
- A late old reply cannot settle it again or reach the new document.
- A fresh ordinary call succeeds on a fresh non-recovery session afterward.

### AC7 — fuzz boundary

The renderer-envelope cargo-fuzz target runs under the repository fuzz gate;
its retained corpus includes malformed UTF-8/JSON, duplicate/unknown fields,
boundary integers, 4096/4097 payloads, and stale-shape samples.

### AC8 — real-device evidence

Record for the physical Mac run:

- exact Keld SHA/tree;
- exact macOS build, architecture, and WKWebView/WebKit facts available to the
  fixture;
- exact Bun version;
- WebView/process identity;
- document nonce only as a redacted/hashable observation, never raw in logs;
- OS-visible input evidence;
- one renderer admission, one KIPC corr, one Bun handler, one reply;
- negative-control outcomes from AC2/AC5/AC6.

This macOS row says nothing about Windows/Linux parity.

## 10. Review gates

| Gate | Applies | Reason |
|---|---|---|
| Public API | **Yes** | freezes `window.keld.invoke`, `@keld/api.app`, `channels.handle`, and channel descriptor types |
| Wire protocol | **Yes, wv-link only** | first private `keld.wv-link/v1` envelope; KIPC frame/version bytes stay unchanged |
| Permission model | **No** | no privileged/native operation or grant representation is added |
| Dependency addition | **No new package** | reuse existing `serde_json`, `getrandom`, `objc2`, `objc2-web-kit`; feature widening is reviewed in implementation diff |
| Unsafe | **Potentially yes** | direct WebKit content-world/delegate plumbing stays inside `keld-wv`; every required unsafe block needs a local `// SAFETY:` proof and review |

Human approval is required on the exact technical spec revision before source
implementation begins.

## 11. Non-goals

- KEL-140 guarded filesystem implementation;
- KEL-143 Bun recovery/restart;
- KEL-144 Rust-quarantine product acceptance;
- `window.keld.send/on/stream/meta`;
- broad KEL-80 replay/PTY/shared-memory work;
- general `@keld/schema` or `keld gen`;
- BrowserWindow or wider Electron Tier 1;
- Windows/Linux renderer bridge qualification;
- background automatic request replay;
- a second app-link socket, frame parser, HELLO implementation, deadline owner,
  lifecycle reader, or WebView identity owner.

## 12. First implementation slices after approval

1. Extract one singleton lifecycle/app-link session into `@keld/api`; make
   `@keld/electron` consume it; keep current lifecycle tests green.
2. Add the declared Echo `AppChannel` descriptor and one `channels.handle`
   path over that same reader/writer.
3. Add pure `keld.wv-link/v1` decoder + deterministic hostile table + fuzz
   target.
4. Install page facade + isolated WKContentWorld handler in the existing macOS
   WebView configuration before navigation.
5. Route one bounded host-minted CALL through the current admitted app-link,
   with one pending entry and stale-document retirement.
6. Run focused unit/hostile tests, full repository gates, then the real
   macOS OS-visible acceptance matrix.

If any step requires page access to the raw native handler, a second app-link
transport, caller-selected identity, an unbounded pending queue, or a KIPC
frame/version change, stop and revise this spec instead of widening
implementation ad hoc.
