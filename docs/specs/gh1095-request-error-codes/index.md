# Spec: request error codes split by dispatch proof

- Status: draft
- Tracker: GH-1095
- Owner: repository owner
- Updated: 2026-11-13
- Pin: 54f3d943ad6d0c6700866066f4a576eb78670d53

## 1. Goal & non-goals

A rejected `window.keld.invoke` call or Bun application call has to tell the
app one thing: whether it is safe to send the request again. Today it cannot.

- `KELD-WV-011` is registered as "rejected before application dispatch". The
  host also returns it after the CALL frame was written, when the app link ends
  or the page navigates while a call is pending.
- `KELD-API-001` covers both "no handler is registered", where no app code ran,
  and "the handler threw", where the handler may have run.

The app therefore cannot tell "never sent" from "may have run".

This revision splits the codes by what Keld can prove:

| Code | Class | Meaning |
|---|---|---|
| `KELD-WV-011` (narrowed) | Not sent, not retryable as is | the renderer request was refused before the host attempted any write: invalid, busy, stale, undeclared, or the app is quitting |
| `KELD-WV-012` (new) | **Definitely not dispatched** | no admitted application generation existed, so the host attempted no write |
| `KELD-WV-013` (new) | **Outcome unknown** | the host attempted the CALL write, or the page navigated while the call was pending; the handler may have run |
| `KELD-API-002` (new) | **Not executed** (asserted by `@keld/api`) | the call reached Bun, but no handler was registered, so no app code ran |
| `KELD-API-001` (narrowed) | Outcome unknown | the call reached the application stage and failed: decode, handler throw, encode, or oversize reply |

Observable outcome: each decided renderer-invoke outcome settles exactly once
with one of these codes or a reply. A hung handler can remain pending (S17);
this contract does not add a reply deadline. No layer of Keld ever replays a request. The rules for an
app resend are in [historical §4.4](design-4.md):

- WV-012 or API-002 permits at most one app-initiated *automatic* resend
  under its specified trigger and stop rules;
- WV-011 is never resent automatically, whatever its detail says;
- WV-013 and API-001 also permit an app-initiated resend of a non-idempotent
  action, including an automatic resend, only with the [historical §4.4](design-4.md)
  durable duplicate suppression or authoritative proof of no past or future
  commit. An absent status snapshot alone is not proof.

The app makes every resend; Keld makes none.

Non-goals:

- navigation policy, the trusted-page predicate, and which documents may hold
  the bridge (separate work);
- the restart notification's design, delivery and ordering ([#1098](https://github.com/gyldlab/keld/issues/1098);
  once it ships, this spec consumes it as the WV-012 restart trigger, [historical §4.4](design-4.md));
- the secure app origin (#1090, KEL-79);
- a host-side dedup store, a "last dispatched id", or a Keld status-query API
  (all rejected, [historical §4.1](design-1.md));
- a reply deadline for a hung Bun ([historical §4.7](design-7.md); liveness, not classification);
- Windows or Linux renderer bridges (none exist; [historical §4.6](design-6.md));
- new KIPC codes for Bun-side outbound calls ([historical §4.5](design-5.md)).

## Parts

| Part | Owns | Load when |
|---|---|---|
| [references.md](references.md) | References and revision history | Read revision authority and source provenance |
| [acceptance.md](acceptance.md) | Dispatch and retry acceptance | Read all acceptance outcomes and controls |
| [design-1.md](design-1.md) | Design 1: First principles, reuse and rejected alternatives | Read historical section 4.1 |
| [design-2.md](design-2.md) | Design 2: Failure stages and codes (renderer → host → Bun, macOS) | Read historical section 4.2 |
| [design-3.md](design-3.md) | Design 3: Backward compatibility | Read historical section 4.3 |
| [design-4.md](design-4.md) | Design 4: Retry rules (normative for apps; Keld itself never resends) | Read historical section 4.4 |
| [design-5.md](design-5.md) | Design 5: Bun-side outbound calls (role → host) | Read historical section 4.5 |
| [design-6.md](design-6.md) | Design 6: Platform notes | Read historical section 4.6 |
| [design-7.md](design-7.md) | Design 7: Exactly-once settlement and no replay | Read historical section 4.7 |
| [design-8.md](design-8.md) | Design 8: Remaining design items | Read historical section 4.8 |
| [design-9.md](design-9.md) | Design 9: Structured error representation (audit and the scoped fix) | Read historical section 4.9 |
| [boundaries.md](boundaries.md) | Implementation boundaries | Read source ownership and non-goals |
| [tasks.md](tasks.md) | Implementation sequence | Read dependency-ordered implementation work |
| [tests.md](tests.md) | Test oracles and evidence limits | Read all planned real-platform and mutation controls |
| [performance.md](performance.md) | Performance impact | Read unchanged performance boundaries |
| [decisions.md](decisions.md) | Owner decisions and outstanding approval | Read decisions without mistaking draft r3 for approval |

## Current blockers

- [B-1](decisions.md#b-1-exact-r3-content-approval)

## 8. Review gates triggered


- Gate: Public API
- Applies: **Yes**
- Reason: three new page- and app-visible codes; narrowed meanings for `KELD-WV-011` and `KELD-API-001`; normative retry guidance; an additive `code` property on
  `window.keld.invoke` rejections ([historical §4.9](design-9.md)); KEL-142 §7 revised

- Gate: Wire protocol
- Applies: **Yes** (values, plus one private field; no version bump)
- Reason: KIPC: `KELD-API-002` is a new value crossing the Bun→host link in `CallError.code`, and the host now rewrites `KELD-WV-*` values arriving there (S10b). The frame and the
  `CallError` schema are unchanged, and receivers treat an unknown code as Unknown, so no version bump. `keld.wv-link/v1`: the native→isolated result gains `document`
  ([historical §4.7](design-7.md)). Both ends ship in one keld-wv build, and the isolated→native envelope and its decoder are unchanged, so no version bump; the reviewer confirms
  this

- Gate: Permission model
- Applies: No
- Reason: no grant, capability or guard path

- Gate: Dependency addition
- Applies: No
- Reason: none

- Gate: Unsafe
- Applies: No
- Reason: no FFI change
