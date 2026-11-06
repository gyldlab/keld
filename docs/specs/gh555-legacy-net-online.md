# Spec: synchronous Legacy connectivity reads on macOS

- Status: draft
- Tracker: GH-555
- Owner: @0monish
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

A macOS app admitted under explicit Legacy can read `net.online` or call `net.isOnline()` after public Ready. Both return the same Boolean for a given host observation. A false result identifies a reported unavailable route. True leaves remote reachability unresolved. Zettlr uses this hint to decide whether to attempt its own update check.

Scope is the main role and these two synchronous reads. Strict-profile support, HTTP clients, DNS lookups, interface enumeration, a public change event and a working updater are outside this pack. No endpoint is contacted to produce this value.

## 2. Spec refs

- [KELD compatibility policy](../architecture/04-electron-compat.md), [principal ownership](../architecture/03-security.md) and [existing host-to-app topology](../architecture/02-ipc.md). The [import surface on main][surface] currently exports app and BrowserWindow.
- [Owner #555](https://github.com/gyldlab/keld/issues/555) and [conformance owner #554](https://github.com/gyldlab/keld/issues/554). #554 owns the red-before-implementation record. [Session facts PR #1362](https://github.com/gyldlab/keld/pull/1362) and [display mirror PR #1164](https://github.com/gyldlab/keld/pull/1164) are related proposals with distinct scopes and approval boundaries.
- Electron 44.4.5 [public contract][edoc], [JavaScript exports][ejs] and [native implementation][ecpp]. Zettlr [UpdateProvider.check at the application pin][zettlr]. Apple-authored macOS SDK [path status][path] and [monitor callback lifecycle][monitor] in a versioned SDK mirror.

FACT: The current surface exports app and BrowserWindow. `packages/@keld/electron/src/index.ts:26`.

FACT: The pinned Electron wrapper binds both reads to the same function. Its exact source line is `exports.isOnline = isOnline;`. [Upstream wrapper][ejs]. Sources retrieved 2026-11-06.

## 3. Acceptance criteria

### AC-1: Record both readiness outcomes

- **Given:** The pinned main process starts with a pre-Ready listener.
- **When:** Read each API before, inside and after Ready in fresh processes.
- **Then:** Record the ordered result type, value or exact error and the binary digest.
- **Negative control:**
  - **Single fault:** Move the early read into the Ready handler.
  - **At-risk precondition:** Register the listener while the process still reports pre-Ready.
  - **Expected RED:** The timing assertion rejects the supposed early sample.

### AC-2: Use one host observation for both reads

- **Given:** An admitted Legacy role has applied a satisfied path.
- **When:** Read both forms, then deliver and apply an unsatisfied path and repeat.
- **Then:** Both report true then false. No getter sends a frame, starts a network request or adds a monitor.
- **Negative control:**
  - **Single fault:** Make the property return true unconditionally.
  - **At-risk precondition:** Deliver a valid unsatisfied update on the current generation.
  - **Expected RED:** The property disagrees with both its oracle and the method.

### AC-3: Keep uncertainty distinct from a broken carrier

- **Given:** The current generation has an authenticated seed and no first OS sample.
- **When:** Exercise pending, invalid and satisfiable states. Separately remove the carrier.
- **Then:** Admitted uncertain states project to true. Missing or corrupt admission yields the existing diagnostic.
- **Negative control:**
  - **Single fault:** Treat a missing seed as authenticated pending.
  - **At-risk precondition:** Withhold the initial seed while leaving the link alive.
  - **Expected RED:** The missing-seed control unexpectedly obtains a Boolean.

### AC-4: Update after each observable transition

- **Given:** Both phases use the same monitor and role generation.
- **When:** Deliver satisfied, unsatisfied, then satisfied with an apply barrier between them.
- **Then:** Reads alternate true, false and true. Duplicates leave the snapshot unchanged.
- **Negative control:**
  - **Single fault:** Freeze the first seed.
  - **At-risk precondition:** Keep the subscription open across the unsatisfied phase.
  - **Expected RED:** The second read stays true.

### AC-5: Reject stale callbacks and role deliveries

- **Given:** A replacement role or monitor has a valid current offline sample.
- **When:** Release an old queued online observation after its replacement has applied offline.
- **Then:** The replacement stays offline. Its first read can use the Host's current sample.
- **Negative control:**
  - **Single fault:** Drop the source monitor lifetime check.
  - **At-risk precondition:** Keep the role unchanged. Queue a late callback from the preceding monitor.
  - **Expected RED:** The valid offline snapshot changes to online.

### AC-6: Exercise Zettlr's actual branch

- **Given:** Updates are enabled. The original updater has a prior-state sentinel and a captured request boundary.
- **When:** Call check under offline and online snapshots.
- **Then:** Offline returns before work and preserves the sentinel. Online reaches the instrumented continuation.
- **Negative control:**
  - **Single fault:** Remove the offline part of the condition.
  - **At-risk precondition:** Keep the build-time disable flag off.
  - **Expected RED:** The offline invocation reaches the continuation.

### AC-7: Contain the signal to the named role

- **Given:** A Legacy control is subscribed and a Strict role lacks this contract.
- **When:** Request the same observation from both. Attempt to seed it from application data.
- **Then:** Only the eligible role receives authenticated facts. Strict remains unqualified.
- **Negative control:**
  - **Single fault:** Remove the recipient eligibility check.
  - **At-risk precondition:** Use a valid authenticated Strict connection with the same app identity.
  - **Expected RED:** The recipient census includes an ineligible role.

## 4. Design

**Readiness.** The pinned Electron docs apply a Ready restriction to the module. The pinned JS explicitly checks Ready on request(), while both connectivity reads call the native binding. The native function negates Chromium's offline status. Neither source proves what an executable returns before Ready. Cell 1 is an implementation gate.

The supported KELD scope starts after Ready. Module import must not arm monitoring or throw as a side effect. For an early read, use the observed Electron error if
#554 records one. If Electron returns a Boolean early, keep the issue's post-Ready boundary and publish a conformance divergence with the existing compatibility
diagnostic and a wait-for-Ready remedy. Exact content approval must include that outcome.

**Owner and state.** The Host owns one macOS default-path monitor per active host observation lifetime. Give each monitor a distinct lifetime token. Install the handler on its serial queue before start. Copy only the path status into the Host owner. No OS object, interface address, network name or endpoint crosses the link.

| Host observation | Public result once admitted | Reason |
|---|---|---|
| Unsatisfied | false | The observed path currently has no usable route. |
| Satisfied | true | A route exists. A server, captive portal or DNS failure can still defeat a request. |
| Satisfiable | true | An attempted connection may bring up a route. The getter never attempts it. |
| Pending, invalid or unmapped SDK status | true | No affirmative offline observation exists. This optimistic projection is a declared limitation. |

**Transport.** Admit only the intended primary Legacy role using host-owned profile and connection generation. Stage a bounded initial status, including explicit
pending, before exposing a usable snapshot. Reuse an approved ordered host-to-app facts owner for seed and subsequent updates. Distinguish stale subscription epochs
from stale monitor callbacks. A sequence within an epoch prevents an old seed replacing a newer update.

The current main export supplies no such fact. #1362's spawn facts alone cannot carry transitions and #1164's window subscription may exclude a windowless app.
Before code, demonstrate a carrier that works with zero windows, preserves ordering, and is authorized for this role. If its payload must change, get the owner's
generated-contract and wire review first. Reusing a channel name does not grant a new payload.

**Read and revoke.** The role keeps one bounded scalar and its generation metadata. Each getter reads that scalar synchronously without allocation proportional to
previous changes. On role detach, revoke the subscription before discarding its snapshot. A shared Host monitor can survive role restart. On monitor replacement,
invalidate its token before cancel, and retire callbacks safely through cancellation completion.

**Alternatives.** A constant true bypasses the offline branch. Polling per getter introduces I/O and races between equivalent reads. A one-shot spawn value becomes
stale on network transitions. An active internet probe adds network authority and measures one destination. The default-path observer has the narrowest input that
can express the required offline branch. No performance claim accompanies this choice.

## 5. Boundaries

Implement on the owning macOS Host/native observation path, the existing authorized app-link facts carrier, the canonical `@keld/electron` export and the #554 cell. Preserve profile admission, public Ready ownership, renderer isolation and unrelated updater APIs. No manifest grant for networking follows from reading this scalar. Linux and Windows remain separate cells.

## 6. Tasks

### T-1: Record the executable comparison

- **Blocked by:** none

Land #554's executable comparison and capture the Zettlr offline/eligible branches. Record a separate pre-Ready disposition for each form and confirm the selected carrier with its owner.

### T-2: Deliver an authorized Host observation

- **Blocked by:** T-1, B-1

At the approved carrier revision, publish the signed-off schema, generation binding and a bounded Host seed/update vertical slice. Exercise a windowless role and stale delivery across restart. Hold if schema or callback ownership requires a new approval.

### T-3: Qualify synchronous facade reads

- **Blocked by:** T-2

Export the property and method from the canonical module. Connect them to the admitted snapshot, preserve the specified readiness outcome and run the paired conformance cells on native macOS. Attach actual output and exact pins to #555.


## 7. Test plan & evidence

| Criteria | Independent observation | Fault that must break the test |
|---|---|---|
| 1 | Same pinned Electron binary in fresh processes at each readiness checkpoint | Move the supposedly early read into a Ready callback. Reject its timestamp/order. |
| 2–3 | Independent status fixture drives every table row. Count host registrations and calls. | Return true always, return false always, or dispatch a call from each getter. |
| 4 | Native observer receipt and acknowledged apply barrier for each transition | Freeze the spawn seed or deliver a previous revision after the new one. |
| 5 | Pause an old delivery, reattach the replacement, then release it with distinct old and new values | Drop the epoch check while keeping sequence and authentication valid. Separately replace the monitor while keeping the same role to challenge its token check. |
| 6 | Actual updater function, updates enabled, previous state sentinel and isolated request observer | Ignore the offline gate. Ensure the online control reaches the instrumented branch. |
| 7 | Host-side recipient/registration census and absent Strict route | Subscribe a Strict role or accept an app-supplied sample as a Host fact. |

Native receipts record macOS/SDK, executable and revision, declared profile, role generation, zero-window state and monotonic observation order. Use owner-controlled callbacks or explicit barriers. Sleeping, a live internet site or toggling Wi-Fi without excluding other routes cannot provide the oracle.

### E-1: Electron, Zettlr and SDK source comparison

- **Run:** [Electron wrapper][ejs], [native binding][ecpp] and [application gate][zettlr]
- **n:** Two public connectivity reads and one application gate
- **Environment:** Pinned GitHub sources on 2026-11-06
- **Limits:** Source inspection cannot establish executable timing, real network state or native parity.

## 8. Review gates triggered

Public API. Wire protocol if an approved carrier must change. Permission-model review if eligibility or recipient selection changes. Native unsafe and dependency gates apply to the eventual implementation's chosen SDK binding. This document introduces no channel, dependency or permission.

## 9. Perf impact

No measured delta. In the implementation, count monitors per Host lifetime, retained snapshots per admitted generation and messages per getter. Prove no growth with repeated reads and no unbounded queue behind a slow recipient.

## 10. Open questions, blockers & unknowns

### B-1: Resolve readiness and the fact carrier

- **Status:** open
- **Owner:** The #554 conformance and host facts owners
- **Impact:** The unresolved recording and carrier block parity and runtime work.
- **Check:** Attach both pre-Ready outcomes and an approved zero-window seed/update trace.

### Q-1: Define pre-Ready divergence and the exact carrier

#554's executable pre-Ready result, the exact authorized seed/update carrier and native callback/lifetime acceptance are open. Record these on #555 before approving the relevant implementation slice.

[surface]: https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/index.ts
[edoc]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/net.md
[ejs]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/net.ts
[ecpp]: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/common/api/electron_api_net.cc
[zettlr]: https://github.com/Zettlr/Zettlr/blob/e6c7fd8ffebfddb6fa091521b5a9dea673719887/source/app/service-providers/updates/index.ts#L296-L303
[path]: https://github.com/phracker/MacOSX-SDKs/blob/041600eda65c6a668f66cb7d56b7d1da3e8bcc93/MacOSX11.3.sdk/System/Library/Frameworks/Network.framework/Versions/A/Headers/path.h
[monitor]: https://github.com/phracker/MacOSX-SDKs/blob/041600eda65c6a668f66cb7d56b7d1da3e8bcc93/MacOSX11.3.sdk/System/Library/Frameworks/Network.framework/Versions/A/Headers/path_monitor.h
