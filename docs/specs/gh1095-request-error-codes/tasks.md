# Implementation sequence

Part of: [index](index.md)
Owns: Implementation sequence

## 6. Tasks

Each task is approximately one PR; the original ordered plan is retained.

Task ID migration: original T0 → T-1; T1 → T-2; T2 → T-3; T3 → T-4; T4 → T-5; T5 → T-6.

Only the explicit original task dependency is recorded below. All production work remains subject to exact-content approval in B-1.

### T-1: Documentation

- **Blocked by:** none

docs only. It includes:
- this spec;
- the KEL-142 §7 current-behaviour correction;
- the interim `KELD-WV-011` registry text;
- the GH-527 Q-3 correction, and the matching GH-531 O11 interim note.

### T-2: Characterisation tests

- **Blocked by:** none

test-only characterisation. It pins today's codes at S8–S16 and
S10b with negative controls, writing the AT-12a/12d/13a/13b/13c arms against
today's strings. It also pins the cross-document settlement today (AT-S arm
c). Arm (d) is deferred to #459 (r2).

### T-3: keld-core

- **Blocked by:** T-2

keld-core. It includes:
- the typed refusal;
- the S8 → WV-012, S9/S10 → WV-013 and S10b rewrite;
- the WV-012/WV-013 registry headings;
- T-2's assertions flipped to the target codes.

### T-4: keld-wv

- **Blocked by:** T-2

keld-wv. It includes:
- `pagehide` → WV-013;
- the document-scoped result ([historical §4.7](design-7.md)), which flips T-2's AT-S arm c. The
  uncommitted host-navigation settlement belongs to #459 (r2);
- the `WvError::RendererBridge` fix text;
- the narrowed WV-011 registry text;
- the `code` property on every invoke rejection ([historical §4.9](design-9.md), AT-EC).

### T-5: API and KIPC

- **Blocked by:** T-2

`@keld/api` and `@keld/kipc`. It includes:
- S11 → API-002, its registry heading, and the unit and hosted API-002 tests;
- the follow-up fix to the `transport.ts:2571-2572` comment, matching Q-3 and
  the AT-B3 verdict;
- AT-API2a's decode-failure arm (S12), which no current test covers.

### T-6: Developer guidance

- **Blocked by:** none

developer guidance for [historical §4.4](design-4.md): the atomic idempotency pattern, the
GH-1132 ban on status-only retries, and registering before `whenReady`.
The X12-style example app comes only when a consumer needs it.

T-3–T-5 are independent once T-2 lands. WV-012 does not wait for the restart
notification (Q-1, [#1098](https://github.com/gyldlab/keld/issues/1098)).
