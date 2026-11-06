# Spec: macOS system appearance subscriptions for Zettlr

Status: draft
Linear: GH-1356 (#517) · Owner: gyldlab/keld · Updated: 2026-11-06

## 1. Goal & non-goals

Zettlr's AppearanceProvider must be able to register for a macOS system-theme change before creating a window, then react exactly once per observed system transition. Expose the three requested systemPreferences members to the admitted Electron main facade. The first slice supports the exact notification Zettlr consumes. The operating system's general notification center stays privileged.

## 2. Spec refs

[Tracker and dependency map](https://github.com/gyldlab/keld/issues/1356), [host observation #481](https://github.com/gyldlab/keld/issues/481), [effective appearance #558](https://github.com/gyldlab/keld/issues/558), [draft mirror design](https://github.com/gyldlab/keld/blob/5de312ec0c8517cbb57733baaebc63e5d9ccd343/docs/specs/gh481-host-pushed-display-theme.md).
[Compatibility policy](../architecture/04-electron-compat.md), [principal boundary](../architecture/03-security.md), [existing export surface][surface].

Upstream [Electron API][edoc] and [macOS binding][ebind] establish numeric subscription IDs, callback arguments, unsubscribe by ID and RGBA output. [Zettlr's constructor][zapp] uses the OS event and the effective-theme event for different purposes. [Its color helper][zcolor] catches failures and supplies its own fallback.

## 3. Acceptance criteria (binary, each becomes a test)

1. A conformance record for Electron 44.4.5 captures subscription, duplicate callback registration, unknown-ID removal and accent format. The KELD row links the exact notification subset, payload divergence and chosen accent behavior.
2. The unmodified macOS AppearanceProvider construction reaches the assignment after registration with zero windows. Registration returns an integer ID. Multiple registrations produce distinct live IDs. Initial snapshot installation fires zero theme-change callbacks.
3. With the app forcing light appearance, move the OS from light to dark. Each active subscriber to the one supported name fires once with that exact name. The applied OS fact is already visible to consumers before callbacks. The effective app color can remain light.
4. With the OS unchanged, change only app themeSource, reapply the same host revision, or change only a display's work area. None may fire that systemPreferences callback. A real OS transition still reaches it after these controls.
5. After removing one of two active IDs, the next OS transition reaches only the remaining registration. An unknown ID is a no-op. Replacing the role drops its callbacks and any queued old-generation delivery.
6. Other valid names register as inert handles with a declared divergence. Include a wildcard/null test. KELD never turns a user string or null into a native subscription. Invalid argument types take the validated diagnostic path.
7. The bounded accent path either returns the exact eight hex digits for an approved host fact or raises the typed unsupported error. For this initial slice select the latter. The actual Zettlr helper returns its own default. A synthesized color must fail this cell.

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

## 6. Tasks (ordered, each approximately one PR)

- [ ] T1. Land missing red conformance rows and record the pinned Electron cases. Verify link-time export ownership against PR #1366. Trace the current host fact from acquisition to a zero-window role.
- [ ] T2. Implement the bounded registry and typed accent fallback, with subscribe/unsubscribe and generation teardown as a usable vertical slice. Registering a callback must work before window construction.
- [ ] T3. Attach one projection to the approved #481/#558 apply path. Run forced-theme and reentrancy controls and the actual Zettlr constructor on macOS. Update the compatibility record with the captured divergence.

## 7. Test plan

| Criteria | Oracle and targeted defect |
|---|---|
| 1, 2 | Count host observer creation and app-link messages before and after two subscriptions. Count callbacks at seed time. Registering a second native observer or firing on seed must fail. |
| 3, 4 | Drive OS changes and app overrides separately while the other stays fixed. A test listening to effective dark mode alone must miss the forced-light case or spuriously fire on an override. |
| 5 | Register A and B. A removes B while handling a change. Register C during dispatch. Check exact callback identities on this and the subsequent change. Deliver an old queued callback after role replacement. |
| 6 | Register a distinct name and null. Inject a verified OS transition and an unrelated native notification. Assert the supported control fires and census shows zero arbitrary platform observers. |
| 7 | Run the original getSystemColors helper under the typed failure. Assert both the caught diagnostic and the exact fallback. A KELD getter hardcoded to Zettlr green would produce the same color but fail the diagnostic assertion. |

Record macOS, app/profile, Electron executable, source revisions, registration counts, OS/effective values and callback sequence. No desktop observation has been run for this proposal.

## 8. Review gates triggered

Public API and permission model. A change to the shared mirror wire shape requires its owner's independent protocol review. This document changes no runtime authority or dependencies.

## 9. Perf impact

No measurement. Count host observations per OS transition and messages per registration. Callback work is proportional to active supported subscribers. Verify cleanup returns the registration count to baseline.

## 10. Open questions

Approve the inert-null and metadata projection divergences, numeric retention budget, exhaustion behavior, and source of the system-level fact when themeSource is overridden. The actual OS event/order and native app behavior require their own receipts.

[surface]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts
[edoc]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/system-preferences.md
[ebind]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_system_preferences_mac.mm
[zapp]: https://github.com/Zettlr/Zettlr/blob/e6c7fd8ffebfddb6fa091521b5a9dea673719887/source/app/service-providers/appearance/index.ts#L77-L111
[zcolor]: https://github.com/Zettlr/Zettlr/blob/e6c7fd8ffebfddb6fa091521b5a9dea673719887/source/common/util/get-system-colors.ts#L23-L47
