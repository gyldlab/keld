# Spec: GH451 D2 local before-quit preflight

Status: approved — D2 plus the separately scoped original-menu intent appendix; full GH451 remains unapproved
Tracker: GH-451 · Owner: repository owner / root integration writer · Updated: 2026-11-01

## 1. Goal & non-goals

A genuine facade `app.quit()` synchronously emits a cancelable `before-quit`
through the existing application listener owner before terminal work. A veto or
listener exception cancels the attempt, resolves its existing `Promise<void>`
without native work and permits a later fresh attempt. Nested/concurrent calls
share the active Promise. Without cancellation, the existing COMPAT012/IPC007
failure paths remain and the existing terminal callback executes in the same turn.

Approval covers the exact pinned/tested app.ts candidate only. It does not approve
full dirty application Quit, `will-quit`, any window sweep, active-quit
window-all-closed suppression, OS/Dock/menu routing, native/process cleanup, or
the original application's async/rejection patch. GH451 remains open.

## 2. Spec refs

- Architecture 04 §4's existing nonblocking `Promise<void>` divergence and shared
  lifecycle link; architecture 02's ordered lifecycle Quit owner.
- GH531 §4.e: the existing per-window close registry is not a cross-window quit
  registry. D2 preserves that boundary and its native-window refusal.
- Pinned Electron v44.4.5 `docs/api/app.md`, snapshot
  `694f45852a0f1726cd23bfd379854de489cccb65`, before-quit lines 61–69 and app.quit
  lines 518–519. These support first notification/cancellation, not the Keld
  Promise or coalescing contract.
- GH451 comments 6381751800 (full oracle remains separate) and 6393116870
  (containment and same-turn Quit/Create fence).

## 3. Acceptance criteria

| D2 cell | Required observable | Mapped `GH451 D2 local before-quit preflight` case |
|---|---|---|
| D2-1 | Registered callback runs synchronously before app.quit returns; real cancelable event latches preventDefault; canceled request resolves | `veto` |
| D2-2 | After veto, a later explicit call gets a fresh event and Promise | `repeated-veto` |
| D2-3 | Nested app.quit shares the outer pending Promise without recursive dispatch | `reentrant-veto` |
| D2-4 | Listener throw cancels, resolves and releases the attempt; healthy retry works | `throw-retry` |
| D2-5 | Existing snapshot dispatch continues after veto; removal affects the next attempt | `listener-snapshot` |
| D2-6 | Concurrent active calls share one Promise; actual terminal failure releases it and a later call re-emits | `coalesced-terminal-failure` |
| D2-7 | Non-veto absent endpoint remains actual IPC007, never silent success | `missing-link-refusal` |
| D2-8 | Native-selected pre-Ready local veto completes without native work | `native-pre-ready-veto` |
| D2-9 | Non-veto native-selected pre-Ready request remains COMPAT012 | `native-pre-ready-refusal` |

These are scoped local conformance cells, not changes to any full-GH451, native
menu, dirty-window or corpus parity verdict. Fulfilled cancellation means the
request completed with cancellation, not that the process exited.

## 4. Design

The existing facade `emit` remains the only listener-snapshot and exception owner;
it additionally reports whether a callback threw. Other event callers ignore
that return value and retain their current behavior. One facade attempt Promise
owns reentrancy and settlement. A veto/exception resolves and releases it. A
non-veto invokes the existing guarded terminal callback synchronously, then
forwards its outcome and releases the attempt. No async function or await is
inserted before terminal admission.

No boundary change: Host/native handle ownership, crash ownership, principal
minting, authentication, Guard policy, role/link generations, I/O queues and
wire frames remain with their existing owners. This changes only the scoped
facade lifecycle behavior. No API method, dependency, manifest grant, native
callback, wire type, source-document witness or private transport is added.

| Atom | Owner / boundary and input → output | Failure / falsifiable proof | Independence |
|---|---|---|---|
| Q1 | Facade listener owner; genuine app.quit → synchronous cancelable callback before terminal work | `veto`; ignored-veto and no-op-preventDefault mutants fail | No Host Ready/window authority inferred |
| Q1a | Same facade attempt lifetime; nested/concurrent call → shared Promise; settlement → fresh later attempt | nested/concurrent/retry cases; omitted release fails | Independent of native window state |
| Q2 | Existing guarded terminal owner; non-veto request → current typed refusal or API call | actual COMPAT012/IPC007; no-op-terminal mutant fails | Does not prove native/OS Quit or full choreography |

Reuse decision: extend the existing event owner and guarded terminal body. A
notification without cancellation leaves terminal work enabled; an unconditional
successful no-op loses actual IPC errors; an await before admission reopens the
documented same-turn native Quit/Create race. These alternatives are rejected.
There is no rewrite or performance claim. Compatibility fallback is the retained
native-window/pre-Ready refusal. Host-selected native configuration capture
remains the canonical transport's immutable import-time selection.

## 5. Boundaries

Production: `packages/@keld/electron/src/app.ts` exact approved bytes.
Tests: existing `packages/@keld/electron/src/app.test.ts` subprocess harness and
`packages/@keld/electron/fixtures/quit_before_event_preflight.ts`.
No Rust/Cargo/Host/native, API terminal protocol, generated package, original app
or other owner source changes belong to D2.

## 6. Tasks

Conformance mappings above and the real-module subprocess fixture land with the
exact approved app.ts diff. Root integrates and runs the required checks. Root
publishes the scoped tracker claim/progress/approval provenance and retains all
unapproved full-GH451 work. D2 completion cannot close GH451.

## 7. Test plan

Run the nine mapped cases via `bun test src/app.test.ts --test-name-pattern
'GH451 D2 local before-quit preflight'` in `packages/@keld/electron`, then the
package suite/typecheck and repository gates. Each case is an isolated real
module child with no Host endpoint; timeouts only kill a stuck child. No stubbed
link, synthetic event, fake Ready/window fact or original application is used.
Assertions thrown inside a callback are retained separately so production
exception isolation cannot turn an oracle failure into a pass.

Retained exact-candidate evidence: nine passing real-module cases; five failing
single-fault mutants (ignored veto, ignored listener exception, omitted attempt
release, no-op preventDefault, no-op terminal callback). The production bytes
are unchanged. The canonical fixture import/path adapter and test discovery are
checked separately. Native acceptance and the existing Quit/Create fence require
the real macOS Host selector; these local cases do not qualify OS behavior.

## 8. Review gates triggered

Public API/lifecycle: yes. Unsafe: none. Permission model: none. Dependency
addition: none. Wire protocol: none. Required final independent evidence and
canonical/native checks remain root-owned; user approval is not a test result.

## 9. Perf impact

One cold Quit attempt Promise; no steady-state transport path change. No measured
speed or resource improvement is claimed.

## 10. Approval provenance and exact byte scope

The human user's message titled “KELD — External Owner Decisions” in the current
resumed Codex turn on 2026-11-01 states **APPROVE SCOPED LOCAL PREFLIGHT**. The root
retained that direct decision and the implementation child verified its wording:

> app.quit() must synchronously dispatch cancelable before-quit.
>
> preventDefault() or listener exception cancels the attempt and resolves its
> Promise. No native work occurs.
>
> Concurrent or nested calls share the active Promise. A later call starts a
> fresh attempt.
>
> Preserve existing COMPAT012 / IPC007 failure behavior.
>
> Apply only the pinned, tested app.ts candidate after source verification and
> required gates.

No platform message UUID was exposed; no identifier or GitHub approval is
invented. The direct conversation receipt is retained by the root integration
writer. This record scopes implementation authority, not independent review or
native acceptance.

- Approved base app.ts SHA256:
  `ddb7a2d9a31dbd28295b270ef35067ab43d5b456fa499094e5c1b588ffa96216`.
- Approved candidate app.ts SHA256:
  `bc17ae37acaf4787c7251be9fb56fcf699451602e503fb0536aa07f3a2a88b24`.
- Exact production forward diff SHA256:
  `4c29560a35724daa9873ab5c05ec53d8bfd9f8e152404ff0fabacdb2b845230d`.

Approval does not extend to full dirty Quit, will-quit, a window sweep or the
original async/rejection patch. The live GH451 read at preparation found an
open, unassigned issue with needs-spec/needs-info labels and four comments, no
competing Agent claim. Its broader planning/spec gate stays in force outside D2.
No blocking design question remains within the approved exact-byte D2 scope.

## 11. Scoped original native-menu intent — separately approved

GH563 comment6563533459 records the delegated local developer-demo amendment;
it does not approve full GH451, generic menu callbacks/GH556, Dock/OS termination,
multiwindow sweep or will-quit. Existing Native Quit remains nonterminating.

Append exactly the unit LifecycleEvent::QuitRequested at payload tag2 on the existing
authenticated lifecycle channel. Preserve Ready0/LastWindowClosed1 and existing frame
version/header. The owning generator emits its strict decoder; trailing/malformed
units remain IPC003. Add no channel, message counter, JS epoch, principal or grant.
Use the existing lifecycle observer map, not a new MenuSource/callback registry.

Core uses the existing installed-menu stamp/current authenticated owner, Ready and
qualified original-window selection/subscription. The final existing menu Guard must
contain actual intent publication. Generic/unqualified/pre-Ready/stale activation
keeps its refusal. A failed write preserves its cause and retires only the same
generation after releasing menu locks; it cannot affect a successor. Native never
calls terminate or force-Quit. Existing API zero-window behavior remains unchanged.

The API observer rechecks its exact captured session/link, Ready and selected source
before each callback, including reentrant retirement. The Electron facade invokes
one shared private implementation of its existing app.quit, preserving D2 synchronous
veto/exception/coalescing and the separately approved original one-window close
handoff. Public method mutation or a MenuItem click cannot replace that owner.
No success is inferred from notification or Promise cancellation.

Independent tests cover exact unit bytes, malformed units, selection/subscription/
Ready/current-caller denials, final Guard and actual socket-write failure, stale and
reentrant observer retirement, D2 veto/retry/coalescing and exact effect-unknown cause.
Real original menu/CmdQ plus unsaved decision, native physical release and natural
Host/family/window/stage exit remain Root-owned acceptance, separate from models.
The manual harness may observe app exit while retaining its real lease; the operator
keyword cannot cause Quit or make lease release count as application termination.

Public API/wire/security independent review applies to this exact supplement; no
dependency or native unsafe operation is added. It is not production qualification.
