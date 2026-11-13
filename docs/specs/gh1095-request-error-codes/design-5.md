# Design 5: Bun-side outbound calls (role → host)

Part of: [index](index.md)
Owns: Design 5: Bun-side outbound calls (role → host)

## 4. Design: Bun-side outbound calls (role → host)

No new code is reserved or emitted; this section only records each code's class.

| Code | Class | Note |
|---|---|---|
| `KELD-IPC-004`, `KELD-IPC-005` local (deadline, frame bound, blocking call in flight) | Not sent, deterministic | thrown before any post |
| `KELD-IPC-022`, `025`, `026`, `027` | **Unknown** | the same code is thrown before posting (`#throwIfTerminal`) and for a call pending at close (F8); callers MUST NOT infer "not sent" from it |
| `KELD-IPC-023` | Unknown | already registered so |
| `KELD-IPC-006` | Unknown | the host may still run the call |
| `KELD-IPC-024` | Not executed | receiver-side: the host ran nothing behind an accepted Quit |
| `KELD-IPC-005` capacity | Not executed | receiver-side admission refusal |

A Bun crash kills the Bun caller, so splitting `KELD-IPC-022` would help only
with Worker death and host retirement while Bun is alive. That is not a Phase 2
need (YAGNI).

GH-527 said that a call still pending at a Quit-close "is treated as
`KELD-IPC-024` (neither ran)". That is unproven for a call the role wrote before
its `Quit`. Owner decision Q-3 (§10) applies:

- T-1 corrects the three GH-527 passages that make this claim. A role-observed
  `KELD-IPC-022` at a Quit-close is **Unknown** unless AT-B3 proves otherwise.
- The independently proven non-execution case is kept: `KELD-IPC-024`. The
  host writes it for a CALL it read behind the accepted `Quit` and ran none of
  it. This is a host-side proof, tested by `guarded_router_and_quit.rs`.
- The GH-527 falsifier "a caller or contract that must tell `KELD-IPC-024`
  apart from `KELD-IPC-022`" is now met, and T-1 records it as met in GH-527.
  T-1 does not change the close-on-REPLY design. Its cost is now explicit: a
  call the host answered `KELD-IPC-024` is observed by the role as
  `KELD-IPC-022`, which is Unknown. Whether to deliver that `024` is a decision
  for GH-527's owner.
- GH-531's O11 row ("the facade treats the two alike") gets a matching interim
  note.
- The matching comment at `transport.ts:2571-2572` is production source, so
  T-1 leaves it unchanged. Correcting it is a recorded T-5 follow-up.
