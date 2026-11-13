# Design 7: Exactly-once settlement and no replay

Part of: [index](index.md)
Owns: Design 7: Exactly-once settlement and no replay

## 4. Design: Exactly-once settlement and no replay

- Three layers hold at most one pending entry and empty it **before** they
  deliver a terminal. A second terminal finds the slot empty and is dropped.
  The three layers are:
  - page facade `pending`, matched on request id (`macos_bridge.rs:49-53`);
  - native `BridgeState.pending`, matched on `(navigation, request)`
    (`:425-436`);
  - host `pending_echo`, matched on `(attempt, correlation)`.
- **Today**, the isolated world's `inflight` flag is not a settlement guard.
  `__keldBridgeNativeResult` (`:137-147`) clears it and relays every result
  without checking the request id. Duplicates within one document are dropped
  by the native layer and the facade, but a result can cross documents (F10).
- **Settlement fix (T-4), document-scoped results:**
  - The isolated world records the in-flight request id, not a boolean.
  - Every native→isolated result carries `document`. The field is a new
    `document: Option<String>` on the private `PageOutcome` struct
    (`macos_bridge.rs:653`), serialized into the JSON passed to
    `__keldBridgeNativeResult`. It is filled in one of three ways:
    - **Immediate refusals** (`reply_immediate_error`, `:801-822`) copy it from
      the `document` field of the envelope being refused. An S3b stale nonce
      therefore carries the old nonce, and the successor drops it.
    - **Dispatch outcomes.** `PendingRequest` (keld-wv `BridgeState`) gains
      `document: String`, set by `admit_invoke` (`:393-431`) to the nonce the
      request was admitted under. `BridgeState::settle` (`:425`) returns that
      nonce (`Option<String>`, not `bool`) when it removes the matching
      `(navigation, request)` entry. `deliver` (`:633`) writes it into
      `PageOutcome.document`.
    - The host round trip is unchanged. `RendererBridgeRequest` and
      `RendererBridgeOutcome` still identify the call by `(webview,
      navigation, request)`, and keld-core never sees the nonce.
  - `__keldBridgeNativeResult` settles only when `document` equals the current
    `documentNonce` and `request` equals the in-flight id. Otherwise it drops
    the result and leaves `inflight` unchanged.
  - The relay to the page world never includes `document`.
  - The nonce already lives in the isolated world, so echoing it back exposes
    nothing new. The page world cannot read it, which keeps KEL-142 §5.1.
- **The resulting gap:** a stale invoke whose document is gone gets no
  terminal. That is correct, because its own `pagehide` already settled it
  with WV-013 (S16).
- **Navigation and bridge reset (r2).** A navigation can reach the bridge
  in two ways:
  - **Page-initiated main-frame navigation.** This is the only kind that can
    occur while the app runs.
    - In wry 0.56.1, `Started` fires only when a navigation commits (wry
      [src/wkwebview/navigation.rs lines 17–25](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/wkwebview/navigation.rs#L17-L25),
      `did_commit_navigation`). So the
      bridge resets only on commit, and a navigation the page cancels never
      resets it.
    - On commit, the old document has already rejected its pending call with
      WV-013 on `pagehide` (S16).
    - Any later result for that call is dropped: either by native `settle`,
      because the entry was cleared, or by the successor's `document` check.
    - Exactly once holds.
    - Main has no navigation-policy handler. #460 owns that policy, and it MUST
      keep this property.
  - **Host-initiated navigation (`WebEngine::navigate`).** It resets the bridge
    eagerly, before `load_html`/`load_url` (`crates/keld-wv/src/wkwebview/mod.rs:687-690@516644b0e1c086baa08ff8e8b52667f4bc97144a`). But
    it is **unreachable while the app runs**:
    - `run_app_until_quit` holds `&mut self` and moves the views and bridges
      into the event loop (`crates/keld-wv/src/wkwebview/mod.rs:302@516644b0e1c086baa08ff8e8b52667f4bc97144a`, `:391`);
    - `AppWindowCommand` has no navigate command.

    A reset without a commit therefore cannot occur today, and nothing in this
    spec requires a delivery for it.
  - **Future owner:** #459 (the webContents facade with host `loadURL`). It
    adds the first running-app host navigation. When it does, it MUST settle a
    call cleared by a reset whose load never commits: exactly once,
    `KELD-WV-013`, to the original document, scoped by `document`. It MUST
    also add AT-S arm (d) with that oracle and a committing-load control. It
    MAY reuse a reset-delivery helper, or it MAY reset only on commit; either
    way it MUST NOT leave a live document's call pending.
  - **Code that cannot be reached today.** A reset-delivery path in keld-wv
    cannot be exercised end to end until #459 exists. T-4 MUST NOT claim
    end-to-end coverage for it. It may keep the path only behind a unit test
    that calls `navigation_started` directly, and it must say so.
  - `destroy()` needs nothing, because the document dies with the view.
- The dispatch thread sends exactly one outcome per renderer request.
- A reply that arrives after a retirement is refused by the correlation and
  attempt check, and the reader that would deliver it is already inert.
- Each stage's code is assigned at exactly one site.
- **No replay:**
  - no layer keeps the payload after the write;
  - no layer re-sends a CALL frame;
  - a successor generation never receives a predecessor's call;
  - Keld's only re-delivery is the app's own new invoke under [historical §4.4](design-4.md).
- **Exception (existing behaviour, kept):** at session shutdown, the dispatch
  thread can stop without an outcome while the document is being destroyed.
  "Exactly once" therefore applies to documents still alive when the outcome is
  decided.
- **Hung Bun:** a call to a live Bun that never answers has no terminal (S17).
  This is a liveness defect, outside this classification spec (Q-5, §10; #1097).
