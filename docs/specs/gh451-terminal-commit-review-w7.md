# GH-451 conditional terminal/Create gate — review-only decision candidate

Status: **draft, unapproved, local review only**. This appendix is a proposed amendment to the published GH-451 spec, not an additional approved source of truth.
Owner decision: GH-451 / KEL-96 lifecycle and GH-531 registry owners, GH-443 macOS termination owner, KIPC/API public-contract reviewers.
Publication anchor: PR #1432 at `04f65fcfd3cf82fdf1fbf68cee27ab937b4949de`, **published AC1–AC9 only**.
Runtime comparison: DRAFT PR #1581 at `46b47a8596e49640e789d40bb6ce38544b20692a`.
The separate spec owner's uncommitted AC10–AC14/T3a is a review input, **not adopted or approved** here. This file assigns no new canonical AC number and changes no production policy.

## 1. Goal & non-goals

Permit a qualified original draw.io Quit only after the current Host registry has recorded physical destruction of every relevant window and no live, Closing or Opening reservation survives. An in-flight Create and terminal commitment must receive one atomic decision. A conditional refusal must preserve the same authenticated app link for a later Save, Cancel, Veto or retry.

Keep the approved GH-531 close state machine, existing forceful/legacy `LifecycleRequest::Quit`, the scoped D2 Promise interface, and B-1 packaged-artifact acceptance unchanged. This proposal does not implement KEL-96, GH-443, GH-563, signing, notarization, generic-app native Quit admission or installed V3 acceptance. It adds no timer, authority registry or permission grant.

## 2. Spec refs and independently observed baseline

- Published governing proposal: `docs/specs/gh451-cancelable-two-phase-quit.md` at the publication SHA above, especially AC2–AC9 and section 10. It remains **draft**, including its four open owner decisions.
- Approved registry: `docs/specs/gh531-window-registry-close-state-machine.md`, which owns GH-531 RequestClose, CloseReply(Veto/Allow), physical Destroyed and retained Opening. Its non-goal expressly leaves app Quit sequencing to GH-451.
- Current Host: `crates/keld-core/src/app_session/window_registry.rs:1702` `has_live_windows` treats every phase except Destroyed as live; `window_router.rs:1180` admits Create under the existing transition/current/windows ordering. A JS Source.all() list does not expose a Create that is still Opening.
- Current forceful terminal: `crates/keld-core/src/app_session.rs:7844` `lifecycle_quit` reserves shutdown and writes the Quit reply before complete window teardown. Existing test `window_router_accepted_quit_reply_precedes_teardown_failure_with_pending_create` intentionally exercises this behavior. Do not invert that legacy test.
- Current API: `packages/@keld/api/src/link.ts:546` marks `#closed = true` and invokes `quitAndCloseLink`. `packages/@keld/kipc/src/transport.ts:3928` closes the link in `finally` for any reply or error. A conditional refusal through this method is **not retryable**. `api/src/app.ts:422` intentionally begins terminal admission in the same turn to prevent a Create-after-Quit race.
- Current macOS adapter: `crates/keld-wv/src/wkwebview/macos_termination.rs:should_terminate` returns `NSTerminateCancel`, posts current `NativeQuitRequested`; Core `menu_router.rs:native_os_quit_requested` authorizes only Ready, subscribed, selected current original role. It does not prove AppleEvent success or terminal cleanup.
- Independent Mac native component evidence: at test-only `d74261ebffed1c2f010bfb541b5d0095b8a58414`, `gh451_draft_ac9_opening_create_fences_terminal_quit` **RED 0/1** because pending Opening reached accepted terminal; `gh451_draft_ac9_empty_selected_original_allows_terminal_quit` **PASS 1/1**. Both exercise today's legacy `lifecycle_quit`; target tests must call the new conditional path.
- Independent source evidence: `4635f001bb3aa185f8470c4c1658829460f57b4d` on current #1581 ancestry: 18/20 pass, two intentionally RED (clean Destroyed-to-terminal and hidden Opening in `will-quit`); 104/104 native-quit intent Bun fixtures pass. W5 unpublished `b2426a9d7` source-model terminal tests 12/12 pass, but an independent hidden-Opening
  negative still submits terminal. None is installed-app acceptance.

- Official AppKit baseline (checked 2026-11-21): Apple states that `NSTerminateLater` runs `NSModalPanelRunLoopMode` until `replyToApplicationShouldTerminate:` receives YES or NO. The asynchronous reply is mandatory after returning Later, and `applicationWillTerminate:` is the relevant native cleanup notification. These contracts establish API
behavior, **not** progress of Tao's specific callbacks or KELD's shutdown workers. Sources: [NSApplication terminate:](https://developer.apple.com/documentation/appkit/nsapplication/terminate%28_%3A%29), [replyToApplicationShouldTerminate:](https://developer.apple.com/documentation/appkit/nsapplication/reply%28toapplicationshouldterminate%3A%29).

## 3. Proposed binary acceptance decisions (review keys, not canonical ACs)

**R1 — Qualified selected original only.** Given the current authenticated Ready+Subscribed original role, a conditional prepare may reach its Host owner. Given an unselected/retired/different generation, there is no `will-quit` authorization, terminal commit, window release or successor authority. Negative: replace exact owner identity with stale G1; the current G2 remains active.

**R2 — Live or Opening refusal is retryable and precedes will-quit.** Given the current original with an admitted Create still Opening, `PrepareOriginalWillQuit` returns a *typed retained/no-effect* result before any `will-quit` listener, shutdown reservation, Guardian preparation or link close. The same link receives a later authentic window event
and retries after physical close. Negative: run legacy `quitAndCloseLink` on retained; the retry oracle fails.

**R3 — Reversible prepare and atomic terminal cut.** Given current authoritative registry zero, Host issues an attempt-bound reversible `Prepared` receipt before `will-quit`. An admitted Create during the callback invalidates the prepare token and blocks later terminal commitment. If final Commit wins the existing Core transition/registry lock, it
reserves the terminal cause and subsequent Create fails before native construction. If Create wins, it becomes Opening before the Commit check and Commit returns retryable retained. Negative: skip token invalidation or reserve outside the lock; assert orphan/early termination.

**R4 — Genuine destruction before will-quit and terminal.** Given one or two clean windows, only Host-applied `Destroyed` receipts after GH-531 CloseReply(Allow)/actual release and Core's verified registry zero permit `Prepared` and entry into `will-quit`. `CloseRequested`, `Merged`, user-facing `closed`, delayed `LastWindowClosed` or Promise
resolution cannot authorize `Prepared`. Negative: substitute CloseRequested for Destroyed; no `will-quit` or terminal calls.

**R5 — Full veto/retry.** Given dirty Cancel or close Veto, preserve document bytes, file hash, visible native window, current birth-PIDs, role/link and new Quit eligibility. Given asynchronous Save or Discard and later actual Destroyed, a later `app.quit` emits a distinct `before-quit` and can progress through `will-quit`. Negative: keep the
vetoed attempt's suppression latch; second `before-quit` disappears.

**R6 — Multiwindow and merged close.** Given W2 then W1, request both closes newest-first even if one vetoes. If a later Quit receives GH-531 `Merged {close_seq}`, it observes that exact close-sequence settlement without borrowing an older app attempt's terminal authority. Test held Veto and Allow separately. This coordination rule **requires
explicit owner approval**; no local outcome is presumed. Negative: drop the merged result and require bounded settlement evidence.

**R7 — OS success only after cleanup.** Given a pinned signed app and a prearmed kernel witness, OS Quit must reach the same guarded app owner. With cleanup held, no native success, zero-exit or forged process-family-clean receipt is allowed. Only after successful Host, Bun and renderer cleanup can the approved AppKit strategy publish success.
Negative: publish native success before a held sender joins; the OS oracle fails.

**R8 — No success on failed cleanup.** Given controlled post-commit resource/worker failure, no success reply or zero-exit claim; report a bounded error with the actual failure and custody. The exact failure deadline, OS response and process outcome remain decisions, not approved constants. Negative: treat lost cleanup receipt as success.

## 4. Candidate design and explicit decisions

### 4.1 One conditional terminal admission owner

**Recommended scope:** one Host-owned, two-stage *prepare for will-quit / conditionally commit terminal* transaction for the selected original. Do not overload the existing forceful `LifecycleRequest::Quit`. `PrepareOriginalWillQuit`, `CommitPreparedOriginalQuit` and `CancelPreparedOriginalQuit` are illustrative, unallocated operation names; KIPC
owner chooses final wire shape, schema version and compatibility policy. Core derives the sole role authority from the authenticated app link, not a caller-provided principal or generation.

**Prepare:** under the existing `shutdown.transition_guard`, current owner and WindowRegistry lock order, verify selected/Ready/current generation, registry-wide non-Destroyed entries and outstanding admitted native work. Opening, Open, ClosePending and Closing all block. Refuse with `Retained` while nonempty; no user `will-quit` callback may run.
If empty, record **one reversible prepare token** in the existing Core lifecycle owner, bound to role generation and exact application Quit attempt, then reply `Prepared`. This token is not terminal shutdown or a second window registry. Missing owner, poisoned lock or unknown native completion is never proof of zero.

**While prepared:** `will-quit` runs on the already authenticated app role; a veto or thrown callback issues idempotent `CancelPreparedOriginalQuit` and keeps the role/link live. A Host-admitted Create after `Prepared` is still permitted, but **atomically invalidates the prepare token before an Opening reservation is minted**. This preserves
callback-created replacement windows and makes the later Commit retryably fail. The Host revokes tokens on role/generation loss and link retirement; no application-supplied token can mint a new owner.

**Commit:** after `will-quit` returns without veto, `CommitPreparedOriginalQuit` rechecks the exact token, authenticated role and authoritative registry under the **same** transition/current/windows lock order. If Create won, return typed `Retained` with no shutdown effect. If Commit won, reserve the existing terminal cause before releasing that
guard, thereby refusing later Creates before native construction. Only then enter the one existing Core terminal tail. A clean old Host snapshot is insufficient at either cut; only the current registry and prepared token decide. A terminal commit cannot be rolled back on late cleanup failure.

**Do not hold a WindowRegistry mutex while waiting for renderer, guardian, transport, native UI or sender joins.** Only the admission predicate and cause reservation are atomic; cleanup runs after dropping locks through the single existing shutdown owner. Reuse the existing `end_window_session_guarded` and Guardian tail, not a parallel registry or second shutdown timer.

### 4.2 Proposed retryable link contract

Recommended *separate* internal API for `prepareOriginalWillQuit`, `commitPreparedOriginalQuit` and `cancelPreparedOriginalQuit`, with one validated generation-bound token and correlated `WorkerLink.call` at each cut. No `#closed`, `#quitPromise` latch or `quitAndCloseLink` is applied **before Host's final Commit response**. The Prepare starts
synchronously from the already-connected role; the Host, never a JS census, orders Create against Prepare and Commit.

- `Retained` from Prepare: explicit **nonexecution**; no `will-quit` event was emitted and the live role accepts a later attempt after Save/Veto/pending Create completion.
- `Prepared`: explicit reversible permission to enter `will-quit`, **not** permission to terminate. Only one current token exists per role and native attempt. A concurrent Create revokes it; a veto must cancel it. No terminal event, shutdown reserve or process cleanup occurs here.
- `Retained` from Commit: the token was invalidated (for example by a newly admitted Create), no terminal committed, and a new `before-quit` can start later on the same live role. A veto or thrown `will-quit` callback cancels without attempting Commit.
- `Accepted` from Commit: Core already reserved terminal before its correlated reply. Only now can API fence local admission and close the link with the existing accepted terminal reply/EOF ordering. This proves **commit**, not successful OS cleanup or clean kernel exit.
- Lost link, timeout or malformed/unknown reply: **effect unknown**, do not retry a potentially committed terminal or reopen local admission on a guess. Preserve existing KIPC exact-error and role retirement behavior. A generic `ERR` is never silently converted to `Retained`.
- Unsupported older Host/schema: refuse capability/handshake and remain fail-closed; never fall back from the conditional operation to forceful `Quit`. Explicit KIPC schema/version and codegen approval is required.

**Rejected alternatives:** (a) check `WindowSource.all().length===0` then call existing `apiApp.quit()`, because Opening is invisible; (b) return typed refusal inside existing `LifecycleLink.quit()`, whose transport `finally` closes the socket; (c) emit `will-quit` and only afterwards try one conditional terminal call, because an undispatched
Opening may exist **before** `will-quit` (violates AC8); (d) reserve irreversible terminal before calling `will-quit`, because its veto must preserve the app; (e) unreserved query-then-forceful-Quit, because Create races between calls; (f) change approved legacy forceful Quit.

### 4.3 Facade, veto and generation ordering

The selected original owns one D2 Promise and the current attempt's GH-531 `close_seq` receipts. It may resolve the public scoped Promise on `CloseRequested` without declaring physical release. The native `BrowserWindow` owner delivers actual Host `Destroyed` after user `closed` callbacks; pending original suppression is retired on authenticated
ReplyApplied(Veto), role/document-generation retirement, replacement or terminal completion. The completed current sweep requests **Host Prepare** before emitting `will-quit`; a false Source zero cannot authorize the callback.

A will-quit listener that creates/reopens a window can still race Host admission after the JavaScript listener returns; that Create atomically invalidates the reversible Prepare and causes later Commit to retain the app. Veto cancels Prepare without invoking terminal Commit. A committed terminal remains terminal; the `quit` event follows the
accepted Core terminal receipt per the approved public contract, never as proof of OS cleanup. Existing forceful/legacy Quit and D2 return contract remain unchanged pending GH-451 approval.

### 4.4 AppKit strategy — recommended experiment-gated choice

**Candidate owner decision: prefer `NSTerminateLater` with a single matching main-thread `replyToApplicationShouldTerminate:` after the native/Host cleanup result.** A dirty Veto or pre-commit retained result replies **NO**; a successfully completed and independently observed cleanup replies **YES**, once, for the same native attempt. Multiple
Dock/AppleEvent/Cmd-Q requests coalesce; wrong/retired attempts never reply to a successor. A source-only mock cannot prove Tao AppKit callback progress while work is held.

The existing `NSTerminateCancel` + app-driven shutdown remains the **fail-closed fallback during experimentation**, but does not count as an OS-successful Quit or satisfy AppleEvent success. Never substitute `NSTerminateNow`, direct `NSApp.terminate:`, `performClose:` or an unguarded destroy. If a real main-thread experiment shows that
`NSTerminateLater` deadlocks, suppresses cleanup callbacks or violates Tao's delegate ownership, the proposed strategy is **rejected** until the owner explicitly approves a different native protocol.

The exact post-irreversible-teardown failure outcome is a second owner decision: no YES or successful zero-exit is permissible; preserve the recorded error, avoid indefinite pending AppKit reply and prove a bounded non-success/fatal disposition. Do not invent a cleanup timeout or promise to restore already destroyed windows. The sender/Guardian/Bun
join dependency graph and failure bound must be approved and tested before enabling the delayed reply.

**Nested modal-loop falsifier:** Apple's documented `NSTerminateLater` enters `NSModalPanelRunLoopMode`. The native experiment must prove that Tao's window Destroyed notifications, app-link proxy, required CFRunLoop modes, main-thread cleanup and final reply still make progress in that mode. Hold an owned sender and confirm the main thread can
drain it without a circular dependency on a callback that only runs after the modal reply. If this fails, **reject delayed reply**, keep current cancel-only path as a safety fallback, and do not report OS Quit success. No speculative run-loop workaround or new unsafe selector is authorized here.

### 4.5 Source and capability boundaries

Proposed implementation sites after approval: `crates/keld-ipc/src/lifecycle.rs` plus generated KIPC TypeScript codecs/fixtures; `crates/keld-core/src/app_session.rs` guarded terminal owner and `app_session/window_router.rs`/`window_registry.rs` authoritative current registry predicate; `packages/@keld/api/src/link.ts` and `app.ts` conditional
method; `packages/@keld/electron/src/app.ts` phase/veto/sweep; `crates/keld-wv/src/wkwebview/macos_termination.rs` only if GH-443 native experiment qualifies. Keep `keld-guard`, manifest permissions, B-1 signed/installed packaging, unselected legacy and protected CI unchanged.

No public wire enum allocation, new unsafe native call, transport close-order change, deadline, performance claim or native release code is approved by this review appendix.

## 5. Boundaries

This is a **candidate amendment to the existing #1432 GH-451 document only**. Its distinct local review path avoids overwriting the owner-writable unpublished AC10–AC14. It does not replace those drafts, approve their proposed cleanup deadline or merged-close disposition, or create a second normative spec. On owner acceptance, cherry-pick the
relevant semantics into the canonical spec with one reconciled numbering/ownership review; then retire this review appendix.

B-1/PR #1407 approved Pack/Core specification text remains byte-identical. The B-2 original app, B-3 signed provenance, G0–G12 installed runtime proof, generic minimal-app Quit, Apple publisher/notarization and security reviews remain NO-GO until independent, exact-source receipts.

**Minimal canonical merge map for the owner:** clarify GH-451 section 4.1 so the existing forceful Quit cause is not mistaken for a qualified conditional fence; place reversible Prepare/Cancel/Commit there. Add the link's typed retained/prepared/accepted semantics to section 4.3, and experiment-gated AppKit strategy to 4.4. Attach C1–C17 to section
7 only after assigning canonical AC numbers against the separate AC10–AC14 owner draft. Add Q1–Q7 to section 10 and reconcile with existing decisions. Preserve published AC1–AC9, never edit the owner worktree, and retire this local appendix after incorporation.

## 6. Ordered tasks for the approved owner, not execution authorization

1. Resolve GH-451 section 10, GH-443 delayed vs canceled AppKit and KEL-96 public return/recovery decisions. Approve the reversible Prepare / Cancel / conditional Commit protocol, its retained/prepared/accepted responses and fail-closed older-schema rule.
2. Add independent **RED** Host/IPC tests for current original Opening **before Prepare**, Create invalidating a pending Prepared token **during will-quit**, and Create-vs-Commit orders. Retain legacy forceful-Quit and selected-original empty controls. Prove negative-control mutations fail before changing Core.
3. Under the existing transition/registry lock, implement reversible Prepare token ownership, its invalidation by Create, explicit veto Cancel and conditional Commit. Preserve legacy forceful path. Prove `Retained` leaves the same authenticated app link operational and `Accepted` makes Create fail before a native build.
4. Integrate the admitted facade's before-quit, per-window GH-531 sweep/ReplyApplied(Veto), actual Destroyed and final LastWindowClosed, then **Host Prepare before will-quit**, will-quit veto Cancel, conditional Commit and generation fencing. Resolve the held merged-close later-attempt result with the GH-531 owner.
5. Qualify the AppKit delayed-reply experiment and cleanup/sender graph with exact PID birth, OS response, healthy relaunch and failure/timeout controls on an independently pinned disposable signed original and generic app; do not manipulate installed V3.

## 7. Failure-first test matrix and evidence standard

Each target must have a truly separate witness from the layer being tested. Use real Host app-link frames and native state for authority tests; use Bun source extraction only for ordering; use Mac AppKit/window/kernel for OS claims.

| Key | Test/negative oracle | Owner and currently available evidence |
|---|---|---|
| C1 | Prepare with actual Create Opening: correlated `Retained`, **no will-quit**, no PrepareAcceptedShutdown or Quit reply; same live link can receive a valid window fact and retry. Mutant `quitAndCloseLink` on refusal must fail retry | Core + KIPC/API; W5 legacy `gh451_draft_ac9_opening_create_fences_terminal_quit` at `d74261ebf` was RED **0/1**, but invokes legacy Quit and must be retargeted |
| C2 | No pending Create, exact selected original: `Prepared`, then no-veto `Commit` `Accepted` and native clean exit; mutant stale generation/selection must deny | Core + API; W5 legacy empty-selected control `d74261ebf` PASS **1/1**, not the proposed conditional request |
| C3 | Create obtains lock before Prepare: Opening blocks `Prepared`; later actual WindowBuilt/close/Veto and retry; mutant stale JS zero must fail | Core/window registry and live app-link |
| C4 | Prepare first, then Create during will-quit: Core admits Create, invalidates exact token before Opening, later Commit `Retained` with link alive; mutant keeping token must fail | Core/Host app link; **critical second lock ordering**, no current red/green Host test |
| C4b | Commit first with valid Prepared token: terminal cause reserved before guard release, later Create typed refusal with **zero** CreateWindow commands/IDs/native windows; mutant release lock before reserve must fail | Core/window registry, no implementation run |
| C5 | Opening build completes or fails: deferred LastWindowClosed only after remaining live native entries retire; role loss/Closing(None) never falsely appears empty | Existing GH-531 tests `released_last_window_defers_while_an_admitted_create_is_opening`; extend to conditional cut |
| C6 | CloseRequested and `Merged` cannot substitute for actual Destroyed; physical receipt for each W2/W1, newest-first, before will-quit; mutant reply-only completion must fail | Electron/Host; `4635f001bb3aa185f8470c4c1658829460f57b4d`: 18/20 Bun, 2 intentional RED |
| C7 | Dirty Cancel, Save, Discard, Veto after one or two windows: retain document hash and process births on veto; later second `before-quit`, `will-quit`, terminal after release; mutant remembered suppression must fail | Electron/real draw.io macOS oracle; model-only cases are not native proof |
| C8 | Held Veto reply while second sweep is pending: later `Merged(close_seq)` must settle on exact current sequence, independently for Allow/Veto without adopting older app-attempt terminal authority; mutant dropped merge must fail bounded oracle | GH-531 + Electron; owner strategy unapproved |
| C9 | Stale G1 AppleEvent/Host buffer and same WindowRef new document G2: no G1 terminal, current G2 remains eligible; mutate attempt/document identity and fail | Core + AppKit + Electron |
| C10 | A Host Create is Opening but no Created fact exists **before** Prepare: `will-quit` must not run. If a Create wins **after** Prepare during the callback, invalidated token makes Commit retryable; mutant source-only check must fail | Core + KIPC; `4635f001` hidden Opening source oracle intentionally RED, still model-only |
| C11 | Qualified real Dock, app-menu, Command-Q and AppleEvent: same before-quit/veto owner, no direct terminate; unqualified attempts cancel; native callback/role birth pinned | AppKit on disposable signed exact bundle; current source adapter cancels, no full OS pass |
| C12 | Native reply held through actual UI/renderer/Guardian/Bun cleanup; NO/YES reply once for same attempt, no success before clean kernel/owned-worker receipts; mutant early YES must fail | GH-443/AppKit + release owner, requires experiment |
| C12b | With `NSTerminateLater` and a held native sender, show that Tao UI/renderer Destroyed, Core app wake, Guardian/Bun join and reply callbacks actually progress in Apple's nested `NSModalPanelRunLoopMode`; if the modal mode starves any required callback, Later is NO-GO | GH-443/Tao native Mac only; Apple documents the modal run-loop requirement |
| C13 | Held sender crosses approved cleanup bound: measured non-success, no normal zero-exit or forged cleanup; healthy relaunch after failure; mutant convert failure to success must fail | Core + release, deadline/failure decision pending |
| C14 | Existing `LifecycleRequest::Quit` with pending Create retains its previously approved forceful behavior and reply/error ordering; mutant sharing conditional refusal with legacy must fail | Core: `window_router_accepted_quit_reply_precedes_teardown_failure_with_pending_create` |
| C15 | Prepare, Cancel and Commit wire discriminants + typed Retained/Prepared/Accepted responses pass schemas, stale-token, malformed and older-peer negatives; old enum bytes unchanged and unsupported peer never force-quits | KIPC/API codegen and independent schema review |
| C16 | Prepared token abandoned by link loss, stopped generation or callback exception cannot later be spent by successor; no leaked quiescence or timer; mutant retain after role-loss must fail | Core/WorkerLink; no proof yet |
| C17 | A prepared role stops responding while its link stays connected: the OS pending reply must reach the approved bounded non-success disposition and never publish normal cleanup success. A hanging `will-quit` callback is not a valid success receipt | AppKit/Core, bound and custody decision pending |

For actual native acceptance retain OS name/version, exact source/commit and bundle hashes, signing team, recorded AppKit request ID/reply, captured Host/Guardian/Bun PID+birth, artifact provenance, installed original draw.io and generic paths, document/disk hash, kernel exit status, and fresh relaunch. Do not declare real native success from Bun,
a mocked process census, mere AppleEvent dispatch or an unqualified ad-hoc signature.

## 8. Review gates

- **Architecture/public API and wire:** required for the conditional operation, schema version, compatibility and D2 return semantics; require a distinct security/architecture reviewer and exact diff.
- **Native unsafe/AppKit:** required for any `NSTerminateLater` / `replyToApplicationShouldTerminate:` binding, current Tao delegate preservation and cleanup-after-reply mechanism; independent real Mac review.
- **Permission/Guard:** no change proposed. **Dependency:** none. **Protected CI:** none.
- **Independent adversarial review:** target C1 (lost-link retry and premature will-quit), C3/C4/C4b (all lock orderings), C7/C8 (veto/merged close), C9/C16 (generation/token retention), C12/C13 (sender cleanup). An unreviewed local draft cannot satisfy this gate.

## 9. Performance

No performance change is claimed. The proposed registry scan is bounded by the existing cumulative-window ceiling and must occur under the current lock only for terminal admission; obtain an exact retained-object/lock-hold and UI dispatch measurement before accepting any budget claim. No polling loop or new deadline is introduced.

## 10. Owner decisions, alternatives and blockers

**Q1 — Wire/API shape (blocking):** Approve a selected-original two-stage `PrepareWillQuit` / `CommitPreparedQuit` with idempotent `CancelPreparedQuit`, correlated `Retained` / `Prepared` / `Accepted`, and retryable same link on `Retained`; preserve forceful Quit unchanged. A single post-will conditional call violates published AC8 when an
unpublished Opening exists. KIPC owner supplies final enum/schema IDs, token identity and typed error code; illustrative names are **not allocated**.

**Q2 — Client and failure semantics (blocking):** Specify D2 Promise settlement, event ordering and role-loss behavior around `Retained`, `Prepared`, `Cancel`, `Accepted` and indeterminate transport failure. Recommended: CloseRequested Promise remains nonterminal; only Host `Prepared` permits will-quit, while a Create during the callback
invalidates the token and Commit retains a live link. On will-quit Veto/throw, Cancel is explicit and retryable. Unknown commit effect may not replay. Prove same-turn admission, token revocation on role loss, and no leaked coalescing latch.

**Q3 — AppKit method (blocking):** Select the *experiment-gated* `NSTerminateLater` plus one main-thread delayed reply as the candidate target; accept only after real Tao+AppKit proof. If it cannot make cleanup progress, retain current cancellation and label OS native Quit NO-GO until a different reviewed strategy is chosen. Generic app native Quit needs separate admission evidence.

**Q4 — Post-commit cleanup failure (blocking):** Select an explicit bounded non-success reply/exit/custody outcome before implementation. After irrevocable teardown there is no promise to restore windows; lost sender/Guardian join cannot be silently treated as success. A deferred reply must never be left pending indefinitely by design.

**Q5 — Overlapping merged closes (blocking):** Approve how a later app attempt observes a GH-531 merged close sequence from a vetoed earlier attempt; test both delayed Allow and Veto, with no transfer of terminal authority.

**Q6 — B-1/GH-1404 release scope (blocking):** Keep approved B-1 source content unchanged, and require independent B-2/B-3 original plus generic signed-installed artifact provenance and G0–G12 native/kernel completion. No source-model test or doc approval authorizes a signed release.

**Q7 — Prepared request with no JS progress (blocking):** If Bun enters `will-quit` but remains alive without sending Cancel or Commit, neither a ready-made cleanup receipt nor link EOF exists. The native owner must select and verify a bounded non-success outcome without inventing a deadline, allowing an indefinitely pending OS response or
pretending a blocked callback can release its own token. Prove this separately from a held sender after terminal commit.

Owner-ready admission is **NO-GO until Q1–Q7 are answered and independently reviewed**. This local candidate is neither approval nor a request to merge PR #1432 or #1581.
