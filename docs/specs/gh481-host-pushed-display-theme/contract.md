# Contract and acceptance

Part of: [index](index.md)
Owns: section 3

## 3. Acceptance criteria

### AC-1: D1 - one authority and one subscription

- **Given:** An authenticated current-primary Bun role uses the existing GH531 window subscription.
- **When:** The Host publishes a desktop snapshot.
- **Then:** The current primary receives one snapshot on that link. No second channel, Worker or permission exists. Unsubscribed and wrong-role links receive no fact. A forged frame or old generation is refused before mirror application.
- **Negative control:**
  - **Single fault:** Broadcast one desktop EVENT to all authenticated links.
  - **At-risk precondition:** A nonprimary, app-bound or unsubscribed role has a valid HELLO.
  - **Expected RED:** The nonprimary role observes a desktop fact.

### AC-2: D2 - initial snapshot before public Ready

- **Given:** A facade boot has an authenticated current-primary Subscribe(c). Native sampling and raw Host Ready may occur later.
- **When:** The native UI Init produces a valid snapshot, in either Ready-first or Ready-last order.
- **Then:** Keep one pending correlation before the first sample. Do not set subscriber state. Do not publish a desktop/window EVENT, send Subscribed(c) or resolve public Ready. After validation, enqueue DesktopSnapshot(e), Created* and Subscribed(c). Public Ready waits for raw Ready, the applied snapshot, full replay and the accepted same-correlation reply.
  A missing or malformed sample at completed Init settles one same-correlation typed ERR. It publishes zero replay or subscriber state and rejects all whenReady waiters. isReady stays false; no ready event is emitted. A missed 15-second Init uses GH446 KELD-CORE-037. Role/link death uses the GH527 terminal path. Only a successor may retry.
- **Negative control:**
  - **Single fault:** Return an immediate ERR when Subscribe arrives before Init and no sample exists yet.
  - **At-risk precondition:** A valid sample will arrive within the approved Init bound.
  - **Expected RED:** A healthy Ready-first or Ready-last boot rejects Subscribe, hangs whenReady or emits partial replay.

### AC-3: D3 - real primary and all-displays shape

- **Given:** A native Mac provider can observe one or more physical displays.
- **When:** The Host captures a fresh desktop snapshot.
- **Then:** Return all available displays and one primary id. Each display has a unique id, finite DIP bounds/workArea rectangles and a positive finite scale factor. The primary appears once. Headless or unavailable sampling returns a typed error, not an all-zero display.
- **Negative control:**
  - **Single fault:** Accept a duplicate display id or nonfinite scale.
  - **At-risk precondition:** The native provider returns duplicate or invalid display data.
  - **Expected RED:** Invalid display data enters the mirror.

### AC-4: D4 - actual OS work areas

- **Given:** The same physical Mac, layout and scale configuration is measured by pinned Electron 44.4.5 and KELD.
- **When:** Each primary and secondary display workArea is sampled.
- **Then:** KELD returns Electron-compatible logical top-left DIP rectangles. Include a second non-primary display with a negative offset. Preserve a real toolbar, dock or menu inset. Do not assume raw AppKit visibleFrame is identical.
- **Negative control:**
  - **Single fault:** Leave AppKit's bottom-left y-coordinate unchanged.
  - **At-risk precondition:** The comparison uses a layout with a negative offset or native origin difference.
  - **Expected RED:** The paired Electron/KELD rectangle comparison fails.

### AC-5: D5 - actual effective appearance

- **Given:** The same physical Mac runs pinned Electron 44.4.5 and KELD in light and dark appearance.
- **When:** The effective native appearance is sampled and changed.
- **Then:** nativeTheme.shouldUseDarkColors returns the same boolean as Electron in both modes. It is synchronous. It is not a Promise, hardcoded false or string. This slice does not install nativeTheme.updated or change OS preferences.
- **Negative control:**
  - **Single fault:** Hardcode shouldUseDarkColors to false.
  - **At-risk precondition:** The OS is in dark effective appearance.
  - **Expected RED:** The dark-mode parity test fails.

### AC-6: D6 - zero IPC on reads

- **Given:** An admitted role has an initialized and applied desktop snapshot.
- **When:** Each synchronous display, primary, workArea and dark-mode getter is called at least 1,000 times.
- **Then:** Actual WorkerLink and Host counters show zero new app-link frames. Getters read only the applied local mirror. A mock call count or hidden transport spy is not sufficient.
- **Negative control:**
  - **Single fault:** Replace one local getter with a per-read Host CALL.
  - **At-risk precondition:** The getter is called after the snapshot is applied.
  - **Expected RED:** The independent WorkerLink/Host frame counter increases.

### AC-7: D7 - ordered updates and parked callers

- **Given:** A blocking dialog CALL is parked while the native observer reports a changed desktop snapshot.
- **When:** The observer update races with Subscribe capture or a blocking reply.
- **Then:** One Core writer serializes observer updates, Subscribe admission and EVENT/REPLY order. A fact before a blocking REPLY applies before Bun's synchronous continuation; listeners run after. If e+1 commits before capture, replay uses e+1. If it commits after capture, replay e and Subscribed(c) precede live e+1. No update is lost or sent out of order.
- **Negative control:**
  - **Single fault:** Let the native observer write outside the Core writer.
  - **At-risk precondition:** An observer update races with initial Subscribe replay or a parked caller.
  - **Expected RED:** A lost update, regressing epoch or wrong continuation value is observed.

### AC-8: D8 - lifetime and recovery

- **Given:** A native observer is active and the Host owns the latest validated snapshot.
- **When:** A Bun role restarts or the Host shuts down.
- **Then:** Observer subscriptions and Host facts survive a Bun-role restart. They retire on Host shutdown. An authenticated successor receives the current snapshot and surviving-window replay. An old role callback cannot answer or target the successor. Contradictory epochs fail closed. A legitimate Subscribe/observer race is serialized, not treated as stale.
- **Negative control:**
  - **Single fault:** Allow a callback from the retired role to target its successor.
  - **At-risk precondition:** A Bun role restarts while a native observer callback is pending.
  - **Expected RED:** The successor receives a fact from the retired role or epoch.

### AC-9: D9 - bounded representation

- **Given:** A proposed desktop snapshot contains display facts and an epoch.
- **When:** The Host validates and encodes the snapshot.
- **Then:** Accept 1 through 32 displays and at most 32 KiB encoded data. Require one primary, finite integer DIP dimensions and positive finite scale.
  Reject zero or 33 displays, duplicate ids, invalid scale, overflow, truncation, malformed tags and regressing epochs. Same-epoch same-value delivery is idempotent. Different values at one epoch are errors. Preserve GH527 bounds.
- **Negative control:**
  - **Single fault:** Accept a 33rd display or a snapshot without a primary.
  - **At-risk precondition:** An over-limit or incomplete native snapshot reaches validation.
  - **Expected RED:** The invalid snapshot is accepted or emitted.

### AC-10: D10 - default-deny role admission

- **Given:** A Host-owned window-fact subscription is associated with the current declared primary role.
- **When:** The Host considers sending desktop facts to a role.
- **Then:** Only the authenticated current primary receives passive desktop/theme facts. No FS, media, location or system-write capability is granted. No app.system literal or new permission is added. Renderer, app-bound, retired and unauthenticated roles receive no facts.
- **Negative control:**
  - **Single fault:** Add a desktop fact path to a public unguarded role.
  - **At-risk precondition:** A nonprimary or renderer role has an active connection.
  - **Expected RED:** The unadmitted role receives a desktop EVENT.

### AC-11: D11 - explicit unsupported boundaries

- **Given:** A getter is called before its role has an applied snapshot, or an unsupported Electron API is called.
- **When:** The facade evaluates the request.
- **Then:** A valid applied snapshot permits a synchronous local read. Otherwise return a stable typed not-ready error. Do not block on a synchronous Host CALL. Unsupported themeSource writes, nativeTheme.updated, cursor/listener APIs and Windows/Linux native execution remain explicit unsupported behavior. Never return fabricated values.
- **Negative control:**
  - **Single fault:** Return false or an empty array as an unqualified default.
  - **At-risk precondition:** No valid snapshot exists or the API is outside this first slice.
  - **Expected RED:** The unsupported or pre-Ready call appears to succeed with synthetic data.

### AC-12: D12 - conformance flip only alongside native proof

- **Given:** Real paired macOS native results, mirror/transport zero-frame evidence and generation/Ready fences are available.
- **When:** The implementing GH481 PR updates the GH477 compatibility corpus.
- **Then:** Flip only the four GH481-owned expected-fail cells in the same implementation diff. Preserve the frozen v44.3.0 lifecycle manifest, all other GH477 cells, GH566's single parser and full source/hosted review gates.
- **Negative control:**
  - **Single fault:** Flip an expected-fail cell using only a TypeScript mock or without native proof.
  - **At-risk precondition:** The corpus change is proposed before real macOS and mirror evidence exists.
  - **Expected RED:** The independent corpus/product-acceptance gate rejects the flip.
