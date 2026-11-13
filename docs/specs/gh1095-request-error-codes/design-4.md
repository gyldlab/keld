# Design 4: Retry rules (normative for apps; Keld itself never resends)

Part of: [index](index.md)
Owns: Design 4: Retry rules (normative for apps; Keld itself never resends)

## 4. Design: Retry rules (normative for apps; Keld itself never resends)

**Default:** a code the app does not recognize means Outcome Unknown.

| Code | Rule |
|---|---|
| WV-011 | **Never resend automatically**, whatever the detail. A new invoke made from a later user action is a new request, not a resend. The detail helps a developer diagnose the refusal; code MUST NOT branch on it. Until the [historical §4.7](design-7.md) document-scoped result lands (T-4), WV-011 proves "never sent" only for the document that issued the call (F10). |
| WV-012 | **One resend** after the first (startup) or successor Bun generation is ready (see the rules below). |
| WV-013 | **Never resend automatically**, unless the app enforces durable duplicate suppression or proves that the original attempt can never commit ([historical §4.4](design-4.md)). A negative status snapshot alone never permits resend. |
| API-002 | **No app code ran.** Fix the registration order so the error cannot recur, then the page MAY resend once. |
| API-001 | Same as WV-013: the handler may have started effects that outlive its error; an absent status record is not proof of permanent non-execution. |

WV-012 resend rules:

- The resend is a new `invoke`, with a new local request id and a new host
  correlation. It is never a replay of the old frame.
- WV-012 ships independently of the restart notification ([#1098](https://github.com/gyldlab/keld/issues/1098);
  owner decision Q-1, §10). Keld emits it only when non-dispatch is proven (S8).
- No blind retries, no timer or backoff retry loops, and no Keld replay.
- Until restart readiness exists ([#1098](https://github.com/gyldlab/keld/issues/1098)), the one resend is allowed
  only after a later user action or a reliable app signal. This applies at startup as well as
  after a restart.
- Once it ships, the trusted restart notification ([#1098](https://github.com/gyldlab/keld/issues/1098)) becomes the
  restart trigger.
- Keld does not claim automatic recovery: a WV-012 call is recovered only by
  the app's own resend.
- A second WV-012, or an API-002, on the resend is surfaced to the user and is
  not resent again. The limit is at most one automatic resend per logical
  action.

An Outcome Unknown request (`KELD-WV-013` or `KELD-API-001`) is safe to
resend **only if the app establishes at least one of these conditions**:

- **(a) Atomic, durable duplicate suppression.** The page supplies a stable
  idempotency key (for example `crypto.randomUUID()` per *logical action*) and
  preserves it across attempts. The handler must atomically claim that key and
  record its effect/result in the same durable transaction, or use a protocol
  that provides an equivalent atomic guarantee. A second attempt with the same
  key never performs the effect again; it waits for or returns the recorded
  outcome. An external side effect also needs deduplication by the receiving
  service using that key, or a separately proven transactional protocol.
  Merely recording the key before/after a nontransactional external call is
  **not** an at-most-once guarantee.
- **(b) Authoritative no-future-commit proof.** Before resending, the app must
  prove that the preceding attempt is permanently unable to commit, including
  any spawned tasks, in-flight external calls, and predecessor generations.
  An acknowledged cancellation with an enforceable execution fence is one
  possible app-specific proof. A link closure, an error terminal, or an
  app-defined status query that finds no durable effect **does not** establish
  this condition on its own.

**Unsafe former rule (GH-1132):** a status handler reads an empty record while
the first handler's deferred effect is still running, reports "not executed",
and authorizes a second attempt. Both can later commit. A status query is
useful for diagnostics/reconciliation, but it cannot be a stand-alone resend
permit without (a) or (b). If neither proof exists, the app must surface the
Unknown outcome and avoid the resend; later user action does not make replay
of that same logical operation safe.

An in-memory map dies with a crashed generation, and an isolated status read
can race a late commit. Keld ships no key store, dedup helper or status API
([historical §4.1](design-1.md)). It carries the app's opaque payload but does not interpret, store, or
manage the app's idempotency key.

API-002 guidance: register every `channels.handle` handler synchronously at
module top level, **before** the first `app.whenReady()` and before any `fs` or
`auth` call. F6 proves this ordering closes the window.

```ts
import { app, channels, echoChannel } from "@keld/api";

channels.handle(echoChannel, handleEcho); // first: no link exists yet
await app.whenReady();                    // then: opens the link
```

A handler registered inside `whenReady().then(...)` leaves a window after every
start and restart in which calls get API-002.
