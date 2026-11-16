# Failure, recovery and platform design
Part of: [index](index.md)
Owns: Startup failure, generation recovery, lifecycle wire and platform support rules.

## 4. Design: failure, recovery and platforms

### D-6: Failure and timeout behaviour

**Decision.**

- **Before `Ready`.** KEL-96 §4.4's rules are unchanged. Any failure is a typed startup
  failure with one attempt and no successor; a schema-2 boot never has a window to
  close. The `HELLO` deadline is unchanged (`APP_LINK_IO_DEADLINE`, 5 s). So a main
  that never touches the `app` lifecycle within 5 s fails startup with today's typed
  error. Whether `@keld/electron` should connect eagerly is the facade's decision, not
  this spec's.
- **Waiting for `Init` is bounded.**
  - **The wait.** The coordinator waits for `HostInitialized` with `recv_timeout`. The
    deadline is set when `run_app` enters the schema-2 loop, the same moment the
    schema-1 navigation deadline starts today.
  - **The bound.** `INITIAL_MILESTONE_DEADLINE`, 15 s: today's
    `INITIAL_NAVIGATION_DEADLINE`, renamed in T-1 with the same value. One constant
    bounds the first host milestone of both boot kinds. Its documented sizing rule
    (`keld-wv/src/lib.rs`) is at least 3× margin over every observed healthy
    start, "while every test harness that waits for `Ready` allows longer". Schema 2's
    milestone (`Init`) precedes the navigation milestone within the proposed loop.
    Retaining the bound avoids changing existing timeout policy; that ordering alone
    is not empirical proof of the latency distribution of a new startup path.
  - **The bound's evidence.** Every sample behind that sizing rule is from Windows
    (WebView2 on Windows 10.0.26300 and hosted `windows-latest` runners, #370). UNKNOWN:
    the macOS distribution of time to `Init`. Until it is measured, the bound is
    an existing policy value rather than a macOS measurement. The
    first measurement action is to record time to `Init` in T-1's AC-4 run on the
    physical Mac.
  - **On expiry.**
    1. Construct `KELD-CORE-037` with the detail "the UI loop did not deliver its first
       turn within 15 s". No `Ready` is written.
    2. Deny recovery and invoke the existing bounded router/guardian paths to retire
       authority, close the link and endpoint, and terminate/reap Bun. Those owners
       do not need the UI thread. Report their actual receipts; an attempted cleanup
       or eventual process exit is not evidence that physical drain succeeded.
    3. Call the host terminal function once.
  - **Why the UI thread cannot do it.** tao delivers proxy commands only after
    `launched` sets ready (A5), so `Fatal` would wait for the very `Init` that never
    came.
  - **What the exit leaves behind: host death for the profile store.** Before `Init`,
    the engine already exists, and `WkWebViewEngine::new` opened the profile store
    (`crates/keld-wv/src/wkwebview/macos_profile.rs`, `MacProfileOwner::new`). Releasing it,
    `clean_shutdown`, needs the AppKit main thread, which is
    the stuck thread. So no clean-shutdown path exists here, and this spec claims none.
    - In a persistent-profile boot, the terminal function's `exit(1)` is host death for
      the profile. The store's lifecycle record remains `Running`, which
      `PersistentProfile::open` committed before engine construction returned
      (`macos_profile.rs`, successful-open lifecycle write). The next launch in
      the same boot finds a dead non-idle owner and quarantines the store, and a
      same-boot quarantine refuses with `KELD-WV-009` until the next reboot. This is
      the existing profile-lock recovery path (`crates/keld-wv/src/profile/lifecycle.rs`,
      `next_lifecycle_action`), unchanged here. An implementation child fixture must
      inspect `lifecycle.v1` before relaunch: quarantine alone cannot distinguish
      Starting from Running, because both are non-idle states.
    - An ephemeral-dev profile keeps no durable profile state (`MacProfileOwner::new`).
  - **The terminal function and current custody contract.** The ordinary
    `run_guarded` result remains `Result<(), HostRunFailure>`. Its retained macOS
    payload stays `!Send`; the ordinary `main` tail reports by borrow, skips stage
    deletion when `retains_main_custody()` is true, and retains that result through
    failed-process exit. The new stalled-Init path receives only a binary-owned
    `fn(&HostAppError) -> !`: it borrows the plain worker error, performs best-effort
    non-unwinding diagnostic output, and exits with failure. It does not own, move,
    flatten or drop the engine, presentation service, profile owner or a
    `HostRunFailure`. Those remain in the blocked main-thread stack through exit.
    The callback must not delete the dev stage, because main-owner release has not
    been proved. It does not reuse a cleanup tail that assumes the UI loop returned.
    No successful-drain or clean-shutdown claim follows from that exit (AC-20).
- **Schema-1 exposure before `Init` (not changed here).** UNKNOWN: whether AppKit can
  withhold `applicationDidFinishLaunching:` indefinitely. Per A5, schema 1's in-loop
  navigation deadline is inert before `Init`. The only exception is a non-user event
  such as `Opened`, which runs the closure early. That is a FACT read from tao source,
  and no test exercises it. Recorded with owner KEL-96.
- **A failed install or a failed `Ready` write.** Both are startup failures with
  `deny_recovery` (D-4 rule 3; today's write-failure path).
- **After `Ready`.** A session with zero windows is valid and has no timer, as in
  Electron (tray-style apps). It ends by Quit, by host death, or by a role exit under
  the KEL-96/KEL-116 runtime rules. The milestone deadline is disarmed at `Init` (AC-7).

**Rejected:**

- *No bound on `Init`.* A boot could hang with no typed outcome.
- *The in-loop deadline as the bound.* It is inert before `Init` (A5).
- *`process::exit` inside keld-core.* Exit authority belongs to the binary.
- *Flatten the ordinary error to `HostAppError`, or delete a stage while the UI is
  stuck.* Either loses the current retained-custody guarantee.
- *A "first window within N seconds" timer.* Electron has none, and a timer would kill
  valid windowless apps.

**Falsifiers, each with a test:**

- A schema-2 boot that hangs before `Ready` with no typed outcome (AC-15).
- A healthy schema-2 session ended by a host timer after `Init` (AC-7 unit).
- A failed install that writes `Ready` (AC-14).

### D-7: Recovered schema-2 generations

**Decision.**

- **Host side, decided here.** The host rule is the same for every boot kind. Recovery
  is armed after the first successful `Ready` write, and g≥2 gets `Ready` replayed
  after its fresh `HELLO`. In a schema-2 boot the host creates no window in any
  generation on its own (AC-8). This is GH-531 D-6 Option B, which sets the host crash
  ownership of windows for every boot kind and needs a successor for its handover
  (criterion 30). It is consistent with KEL-75, whose `primary` owner stops at session
  stop, not at a window.
- **Adoption invariant:** a successor `Create` with an eligible surviving window must
  adopt it without allocating another native window. The GH-531 registry owns this
  rule; it is not a global one-window cap or a URL-deduplication rule. Native identity
  retention covers every survivor, including windows not adopted by a constructor.
  The facade complies (GH-531 D-6 scope, amended by #657):
  - The keld-core registry owns the rule. It keys on per-link subscription and
    per-window state, never on boot kind. Renderer boots never populate the registry
    (GH-531 criterion 15).
  - **Create before Subscribe.** A `window` CALL from a link not subscribed to
    `window` is refused with `KELD-CORE-045` and changes no state. A blocking `Create`
    that hits this surfaces as a thrown constructor error (gh508 c16: a supervised
    restart starts unsubscribed). So transfer has one point, `Subscribe` (GH-531
    criterion 46).
  - **Adoptable windows.** Each `Open` window transferred at `Subscribe` is marked
    `adoptable`. The mark is cleared by adoption, by the owner's first
    window-addressed CALL naming that window, or by `Destroyed`.
  - **Adoption.** A successor's `Create` adopts the adoptable window with the lowest
    `WindowId`, answering `REPLY Adopted { window }` with no EVENT and no state
    change. With no adoptable window left, `Create` creates a new one.
    - Ids are minted in `Create` admission order, the same order as `getAllWindows`.
    - In T-1, `Create` carries no renderer-authority option, so there is nothing to
      match. #455 adds the rule: adopt the lowest-id adoptable window whose triaged
      renderer-authority options equal the request's; if none matches, create. There
      is no refusal code.
  - **Identity.** The replayed `Created` builds the mirror entry and its
    `BrowserWindow` object, and on `Adopted` the constructor returns that same object,
    so `fromId(win.id) === win`. `browser-window-created` and `web-contents-created`
    fire once, at adoption, never at replay. Bounds and title come from the replay.
  - **Unclaimed survivors.** They stay `Open`, owned by the successor and counted by
    GH-531 T-11's last-window check. They can be closed natively, and they stay adoptable until
    claimed. A later constructor can therefore bind a forgotten survivor. That is
    deterministic, avoids an extra allocation for that eligible survivor, and never
    makes the host navigate;
    it is recorded as a ▲ divergence (cell `app.window.adopted-survivor`, #449).
  - **Scoped recovery amendment.** Renderer-declared schema-1 recovery preserves its
    existing native-window and document-state contract. In schema 2, every surviving
    native window keeps its identity; its document remains unchanged until recovered
    application code explicitly navigates that window. Adoption does not itself
    navigate or discard the document. Existing navigation authorization, generation
    isolation and retired-filesystem quiescence remain required.

  Transfer, the `Created` replay at `Subscribe` (criterion 30 without `pending_close`),
  `adoptable`, criterion 46, `WindowResponse::Adopted` and `KELD-CORE-045` land with
  #449 (GH-531 T-1). `pending_close` and closes during
  the recovery gap stay with #450 (GH-531 T-2).
- **The compat cell.** `app.ready.recovered-generation` is `unknown` (AC-9). It is
  uncited, and its NC names #449's keld-core adoption test.
- **Scoring fact, which carries no weight here.** The first proof scores only "install
  + activation + a 4-step primary_workflow" (#432), so no scored path crosses
  generation loss. The invariant holds regardless.

This spec is safe in its own landing window. No `Create` exists until #449, so no
schema-2 generation can have a window.

**Rejected:**

- *A boot-kind-scoped crash rule:* schema-2 boots that do not arm recovery, so role
  loss ends the session. Criterion 30's transfer and replay would then have no
  consumer, and the host would carry two lifecycle policies keyed on the descriptor.
  It would also give up supervised recovery for every facade app. Electron 44.4.5 has
  no main-gone semantics to copy, so "Electron-faithful" would only mean that the app
  dies.
- *A facade-owned no-second-window rule.* It cannot be unit-tested without a GUI host,
  and a host refusal beside it would give one policy two owners.
- *Suppressing a same-URL `loadURL` after adoption.* It changes Electron semantics, and
  it stalls `did-finish-load` waiters.
- *Counting pre-transfer windows, so `Create` can adopt before `Subscribe`.* It adds a
  second transfer path, and a `REPLY Created` ahead of its replayed EVENT breaks the
  O1 ordering.
- *A `Ready` or time cutoff for adoption.* Apps create windows after async `whenReady`
  work, and timers are nondeterministic.
- *X06-D1(c)'s session-ending default.* It was a research correction proposal that was
  never filed (it is absent from the adopted decisions), so it cannot override the
  owner's later D-6. It is superseded.
- *Withhold `Ready(g2)`.* The successor's `whenReady` would hang with no typed outcome.

**Falsifiers:**

- A host-created window in any schema-2 generation (AC-8).
- A restart alone allocates a native application window.
- A `Create` builds a window while an eligible adoptable one exists (GH-531 criterion 46,
  #449). Its NCs: deleting the adoption arm builds on the first `Create`, and deleting
  the 045 check builds before `Subscribe`.
- The independent native identity observer sees any retained survivor replaced,
  including one the successor has not adopted. A facade-local id that happens to equal
  a host id is not that evidence (AC-19).
- A survivor's document changes without an explicit authorized application navigation,
  or schema-1 document continuity changes (§4.10).
- GH-531 D-6's own falsifier: a successor cannot safely adopt a window it did not create.

### D-8: Wire

**Decision.** No Ready wire change. `LifecycleEvent::Ready` stays `0x00` on channel 3,
and its doc already reads "ready for the app process to create windows / run".
Schema-2 boots do not use the legacy keld-wv
`AppWindowEvent::LastWindowClosed` UI milestone. The GH-531 registry owns the
last-window decision and causes the host to write the distinct wire
`LifecycleEvent::LastWindowClosed`, ordered after the final `Destroyed` according to
GH-531 criterion 32, ordering rule 5 and GH-531 T-11, including its deferred-last-window rule.
An initially empty facade boot produces no synthetic last-window notification.
The existing facade `window-all-closed` and default-Quit contract is preserved.

**Rejected:** *A new `HostInitialized` lifecycle variant.* It has no consumer, and
Electron has one `ready`.

**Falsifier.** A design step that needs a new frame.

### D-9: Platforms

**Decision.** macOS implements schema 2. On Linux and Windows, the descriptor is valid
but the platform cannot run it, so `ResolveBoot` refuses it with `KELD-CORE-034` ("No-flag
application boot is unavailable on this platform"). That happens before any
application resource exists (KEL-96: "Failure in steps 1–3 leaves no application
resource"). `KELD-CORE-035` remains for invalid descriptors.

No `WebEngine` trait method is added; the new run method exists only on
`WkWebViewEngine`. The one new cell, `app.ready.recovered-generation`, declares macOS.
On Windows and Linux it is an unrun lane, reported `unknown` (GH-566 D-13).

**Rejected:**

- *Implementing Windows and Linux now.* Out of the first-proof milestone, and their
  emission points are unknown.
- *A trait method with stub backends.* KEL-96 §4.4 forbids hollow methods.
- *`KELD-CORE-035` for the refusal.* The document is valid; 035 means "invalid descriptor".
