# Spec: exact-literal WebView channel grants and single guarded Host dispatch (GH552)

Status: draft
Tracker: GH-552
Owner: GYLDLAB
Updated: 2026-11-17
Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

**Approval boundary:** This is a specification-only proposal for GH552.
It authorizes no production implementation and no merge.
Amisha must complete independent permission-model and public-API review.
The owner must record exact-content approval and set this spec to approved before source work.
GH540/GH541 approval and green documentation CI do not satisfy this gate.

## 1. Goal & non-goals

Allow only a Host-authenticated current main-frame document in a Host-bound window
to send an application topic. The decoded exact `el:<topic>` literal must appear
in that window's immutable `windows.<window>.channels` list.
Missing authority returns `ChannelForbidden / KELD-GUARD003` before any Bun app-link
frame or application handler effect.

Keep the existing Rust Guard policy and `dispatch_privileged` as the only
authority-bearing dispatch boundary.
A bounded typed Core adapter may call it only after GH542 authenticates identity
and GH540/465 establishes the canonical one-link v2 carriage.
The warmed policy/dispatcher Allow decision performs zero heap allocations.
Transport serialization is outside that measurement.

The first independent implementation candidate is narrower than the full goal.
It covers strict policy parsing, pure typed query/evaluation and counted no-effect callbacks.
This work may start only after this spec is approved.
Pure tests cannot prove real Host/Bun zero-write or lifecycle acceptance.

Non-goals:

- No source changes in this PR. No WebKit patch, shared navigation counter,
  policy bypass or parallel compatibility implementation.
- No generic `windows` capabilities, native FS/media permission for a WebView,
  role/window registry replacement, OS confinement claim, live policy reload,
  wildcard channel language or trusted page-supplied authority.
- No second authorization evaluator, channel decoder, IPC link, credit ledger,
  transport reader, facade-local guard or new numeric KIPC channel id.
- No interpretation of app endpoint value bytes by `keld-core`, `keld-ipc`
  or `keld-guard`.
- No change to GH544 codec, GH1095 error/settlement, GH449 subscriptions,
  GH657 readiness or GH1171/GH1181.
- No Electron parity or draw.io product acceptance claim from a pure Guard test,
  native fixture-only check or documentation CI result.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [contract.md](contract.md) | Sections 2 and 3. | Reviewing upstream contracts or acceptance rules. |
| [design.md](design.md) | Section 4. | Reviewing trust, policy, dispatch, lifecycle or diagnostics. |
| [delivery.md](delivery.md) | Sections 5–7, 9 and 10. | Planning implementation, tests, performance or blockers. |

## Current blockers

- B-1
- B-2
- B-3
- B-4
- B-5

## 8. Review gates triggered

- **Permission model:** Amisha must independently review decoded policy syntax,
  whole-policy rejection, immutable Host window binding, native capability separation,
  default deny, error precedence and the single evaluator.
- **Public API:** Amisha must review the proposed Rust query/dispatcher API and
  hypothetical MCP selector. Preserve legacy callers and frozen GUARD003/CallError text.
  The hypothetical window must never become a live Host binding.
- **Wire protocol:** GH540/GH465 owners review any later wire or source change.
  This spec consumes v2 shapes only. It adds no channel entry or serializer.
- **Unsafe/native:** GH542/WebKit owners review later native identity and injected handlers.
  This document contains no unsafe or macOS native source.
- **Dependencies:** None are added by this specification PR.
- **KEL-102 shared dispatch:** The sole-evaluator claim remains gated on KEL-102 T-4 media evidence.
  No bypass or policy-copy exception is approved.

Review this draft at its exact PR commit.
Do not infer approval from GH540 or GH541 labels.
Amisha's review is requested, not assumed.
