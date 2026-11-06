# Spec: macOS system appearance subscriptions for Zettlr

- Status: draft
- Tracker: GH-1356
- Owner: @0monish
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Zettlr's AppearanceProvider must be able to register for a macOS system-theme change before creating a window, then react exactly once per observed system transition. Expose the three requested systemPreferences members to the admitted Electron main facade. The first slice supports the exact notification Zettlr consumes. The operating system's general notification center stays privileged.

## 2. Spec refs

[Tracker and dependency map](https://github.com/gyldlab/keld/issues/1356), [host observation #481](https://github.com/gyldlab/keld/issues/481), [effective appearance #558](https://github.com/gyldlab/keld/issues/558), [draft mirror design](https://github.com/gyldlab/keld/blob/5de312ec0c8517cbb57733baaebc63e5d9ccd343/docs/specs/gh481-host-pushed-display-theme.md).
[Compatibility policy](../architecture/04-electron-compat.md), [principal boundary](../architecture/03-security.md), [existing export surface][surface].

Upstream [Electron API][edoc] and [macOS binding][ebind] establish numeric subscription IDs, callback arguments, unsubscribe by ID and RGBA output. [Zettlr's constructor][zapp] uses the OS event and the effective-theme event for different purposes. [Its color helper][zcolor] catches failures and supplies its own fallback.

FACT: The pinned macOS binding starts with an empty sender string and uses a dictionary for user information. [Electron binding][ebind].

FACT: Zettlr subscribes to `AppleInterfaceThemeChangedNotification` separately from `nativeTheme.on('updated', ...)`. [Consumer][zapp]. Sources retrieved 2026-11-06.

## 3. Acceptance criteria

### AC-1: Record the reference behavior

- **Given:** Electron 44.4.5 runs a main-process fixture.
- **When:** Register two callbacks, remove an unknown ID, and read accent color.
- **Then:** Record numeric IDs, callback shape, unsubscribe behavior and RGBA output per tested operation.
- **Negative control:**
  - **Single fault:** Store only the first of two subscriptions in the reference fixture.
  - **At-risk precondition:** Register the same callback twice under distinct IDs.
  - **Expected RED:** The second expected registration is missing from the census.

### AC-2: Register before the first window

- **Given:** The real AppearanceProvider constructor runs on Darwin with zero windows.
- **When:** Install its subscription, then deliver the initial system-appearance seed.
- **Then:** Construction succeeds. Each registration has a distinct live integer ID. Seeding triggers zero change callbacks.
- **Negative control:**
  - **Single fault:** Emit on seed.
  - **At-risk precondition:** Register a counting callback before the first snapshot arrives.
  - **Expected RED:** The counter changes before any OS transition.

### AC-3: Separate OS transitions from app appearance

- **Given:** The app forces light. The system is light and the baseline is applied.
- **When:** Switch the OS to dark while the effective app stays light.
- **Then:** Deliver exactly one callback per supported subscriber with the exact event name.
- **Negative control:**
  - **Single fault:** Drive subscriptions from effective app appearance only.
  - **At-risk precondition:** Keep the app pinned light across the real system transition.
  - **Expected RED:** The OS transition has zero callbacks.

### AC-4: Suppress unrelated updates

- **Given:** Keep the OS appearance fixed on a live admitted generation.
- **When:** Change app themeSource, update display geometry and replay the same revision.
- **Then:** Each control fires zero OS-theme callbacks. A following genuine OS transition fires one.
- **Negative control:**
  - **Single fault:** Publish the event on every nativeTheme update.
  - **At-risk precondition:** Change only the app override while OS appearance stays fixed.
  - **Expected RED:** The observer records a spurious OS notification.

### AC-5: Revoke per subscription and generation

- **Given:** Two subscriptions and a queued old-generation delivery exist.
- **When:** Unsubscribe one live ID and an unknown ID. Reattach a successor and release the queued event.
- **Then:** Only the surviving live registration is called. The successor receives no previous-generation callback.
- **Negative control:**
  - **Single fault:** Leave a removed ID eligible during dispatch.
  - **At-risk precondition:** Schedule removal before dispatch reaches that still-referenced callback.
  - **Expected RED:** A removed registration appears in the callback log.

### AC-6: Close the supported notification set

- **Given:** The admitted facade exposes one supported system-theme name.
- **When:** Register another string and null. Drive a known unrelated notification and a genuine theme change.
- **Then:** Unsupported names are inert and explicitly divergent. No general native observer is created.
- **Negative control:**
  - **Single fault:** Forward a null name to the distributed notification center.
  - **At-risk precondition:** Hold a null registration live when the unrelated notification occurs.
  - **Expected RED:** The platform registration census shows a wildcard observer.

### AC-7: Preserve the app's accent fallback

- **Given:** The first slice has no approved RGBA host fact.
- **When:** Call getAccentColor directly and through Zettlr's original color helper.
- **Then:** The direct call raises the typed unsupported error. Zettlr supplies its own fallback.
- **Negative control:**
  - **Single fault:** Return a hardcoded Zettlr accent.
  - **At-risk precondition:** Run the direct getter before the application helper.
  - **Expected RED:** The direct error assertion fails even if the UI color looks identical.

## 4. Design

The input is the OS appearance transition already observed by the single Host owner. The app's effective dark mode can differ when it overrides themeSource. Compare
underlying system appearance across two valid revisions. Do not attach the subscriber directly to every `nativeTheme.updated`. Zettlr uses the system signal
specifically to escape a forced override. If #481/#558 expose only the effective value, hold delivery until the mirror owner supplies an approved distinction.

Keep one role-local table keyed by an opaque integer. Registration records a callback and a supported/inert selector. An identical callback subscribed twice gets two
entries. Subscribe/unsubscribe issue zero host calls. Use the existing facade lifecycle to retire registrations on disconnect and prevent ID collision within the
same live generation. Validate before allocation. Bound active entries with the facade's approved retention policy; exact budget and exhaustion diagnostic belong in
approval.

An authenticated Host seed establishes the baseline. A subsequent genuine OS change takes a snapshot of eligible IDs in registration order. Recheck membership before
invoking each ID. New registrations during dispatch wait for the next transition. Removing the current or a later registration is safe. Preserve the existing facade
error-reporting policy if a callback throws, so one callback cannot prevent delivery to unrelated registrations.

The supported callback carries the exact event name. Never forward a distributed userInfo dictionary or arbitrary sender. Use a fresh empty dictionary and empty
string in the other two argument positions as a documented projected payload. Do not claim full parity for listeners that inspect metadata. For null, the pinned
Electron binding supports all senders/names. Return an inert handle and record the divergence; never grant a wildcard.

Choose the canonical unsupported-member error for getAccentColor until an independently approved accent fact exists. The Zettlr source calls this inside a try/catch and owns the fallback value. Fabricating a color in KELD would hide missing support. An eventual native RGBA cell must verify byte order, alpha, and absence of a hash prefix.

No new native observer, framework-level configuration or persisted registration. The admitted main role owns handlers. The Host owns system facts, role eligibility, and the existing stream. There are no new renderer or generic notification permissions. A second Cocoa observer or separate app-link subscription for each subscriber fails the single-owner contract.

## 5. Boundaries

Implement in the canonical Electron module, the process-local subscriber registry, and conformance cells. Coordinate any missing raw OS fact with #481/#558. Other systemPreferences members remain with #482. Linux and Windows have independent platform rows. This spec does not enable posting notifications, arbitrary native names, or a new sender metadata channel.

## 6. Tasks

### T-1: Record supported and divergent cells

- **Blocked by:** none

Land missing red conformance rows and record the pinned Electron cases. Verify link-time export ownership against PR #1366. Trace the current host fact from acquisition to a zero-window role.

### T-2: Implement local registration and fallback

- **Blocked by:** T-1, B-1

Implement the bounded registry and typed accent fallback, with subscribe/unsubscribe and generation teardown as a usable vertical slice. Registering a callback must work before window construction.

### T-3: Qualify raw OS appearance delivery

- **Blocked by:** T-2

Attach one projection to the approved #481/#558 apply path. Run forced-theme and reentrancy controls and the actual Zettlr constructor on macOS. Update the compatibility record with the captured divergence.


## 7. Test plan & evidence

| Criteria | Oracle and targeted defect |
|---|---|
| 1, 2 | Count host observer creation and app-link messages before and after two subscriptions. Count callbacks at seed time. Registering a second native observer or firing on seed must fail. |
| 3, 4 | Drive OS changes and app overrides separately while the other stays fixed. A test listening to effective dark mode alone must miss the forced-light case or spuriously fire on an override. |
| 5 | Register A and B. A removes B while handling a change. Register C during dispatch. Check exact callback identities on this and the subsequent change. Deliver an old queued callback after role replacement. |
| 6 | Register a distinct name and null. Inject a verified OS transition and an unrelated native notification. Assert the supported control fires and census shows zero arbitrary platform observers. |
| 7 | Run the original getSystemColors helper under the typed failure. Assert both the caught diagnostic and the exact fallback. A KELD getter hardcoded to Zettlr green would produce the same color but fail the diagnostic assertion. |

Record macOS, app/profile, Electron executable, source revisions, registration counts, OS/effective values and callback sequence. No desktop observation has been run for this proposal.

### E-1: Zettlr and Electron source trace

- **Run:** [AppearanceProvider][zapp], [color helper][zcolor] and [native implementation][ebind]
- **n:** One unconditionally registered system name and one accent read protected by try/catch
- **Environment:** Pinned GitHub sources on 2026-11-06
- **Limits:** No OS notification, registration or executable app construction was exercised.

## 8. Review gates triggered

Public API and permission model. A change to the shared mirror wire shape requires its owner's independent protocol review. This document changes no runtime authority or dependencies.

## 9. Perf impact

No measurement. Count host observations per OS transition and messages per registration. Callback work is proportional to active supported subscribers. Verify cleanup returns the registration count to baseline.

## 10. Open questions, blockers & unknowns

### B-1: Qualify system appearance under override

- **Status:** open
- **Owner:** The #481 and #558 mirror owners
- **Impact:** An effective-theme-only mirror cannot implement the actual Zettlr callback.
- **Check:** Observe an OS change while the app forces light and identify its authenticated underlying source.

### Q-1: Approve the bounded subscription contract

Approve the inert-null and metadata projection divergences, numeric retention budget, exhaustion behavior, and source of the system-level fact when themeSource is overridden. The actual OS event/order and native app behavior require their own receipts.

[surface]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts
[edoc]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/system-preferences.md
[ebind]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_system_preferences_mac.mm
[zapp]: https://github.com/Zettlr/Zettlr/blob/e6c7fd8ffebfddb6fa091521b5a9dea673719887/source/app/service-providers/appearance/index.ts#L77-L111
[zcolor]: https://github.com/Zettlr/Zettlr/blob/e6c7fd8ffebfddb6fa091521b5a9dea673719887/source/common/util/get-system-colors.ts#L23-L47
