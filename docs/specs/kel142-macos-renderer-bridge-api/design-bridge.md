# Spec: KEL-142: bridge, ordering and errors

Part of: [index](index.md)
Owns: section 4, the trust boundary, ordering, backpressure and error registry

## 4. Design: bridge, ordering and errors

### 5. macOS bridge trust boundary

#### 5.1 Two worlds, one reviewed facade

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

#### 5.2 Host-bound document identity

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

#### 5.3 Private wv-link envelope

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

### 6. Ordering, backpressure, disconnect

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

### 7. Errors

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

**Current-behaviour correction (GH-1095, 2026-10-22).** The `KELD-WV-011`
bullet above is not true of the code at `516644b0`. Two emitters return
`KELD-WV-011` when the call may already have run:

- the host, after the CALL frame write was attempted: a failed write ("application
  call is unavailable or already pending") or a link end after dispatch
  ("application link ended before the renderer reply");
- the page facade, on `pagehide` while a call is pending ("renderer document
  navigated before reply").

The same `KELD-API-001` bytes also cover both "no handler registered" (no app
code ran) and "handler failed" (it may have run).

Until GH-1095 is approved and implemented:

- callers MUST treat every `KELD-WV-011` and every `KELD-API-001` as outcome
  unknown, without branching on the detail string;
- callers MUST NOT resend those calls automatically.

The draft contract revision
[`gh1095-request-error-codes.md`](../gh1095-request-error-codes.md) narrows
`KELD-WV-011` back to pre-write refusals and adds `KELD-WV-012` (definitely not
dispatched), `KELD-WV-013` (outcome unknown) and `KELD-API-002` (no handler
registered), and it keeps `KELD-WV-011` as "never resend automatically". It
supersedes this section only once approved.
