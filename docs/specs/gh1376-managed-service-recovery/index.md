# Spec: Managed service and recovery authority

- Status: draft
- Tracker: GH-1376
- Owner: Amisha, specification author. Existing runtime, security and state owners retain approval authority.
- Updated: 2026-11-16
- Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

## 1. Goal & non-goals

PROPOSED: Give managed OpenCode one accountable service owner before replacing upstream lifecycle operations.
The owner must prevent concurrent recovery and preserve uncertain effect outcomes.
Keep the complete product denominator from #1377.
D-5 through D-10 record selected proposals and their detailed contract candidates.
Q-1 through Q-3 retain the exact mechanism and approval dependencies stated in validation.
This draft does not authorize implementation. B-1, B-2 and B-3 remain open.

Non-goals:

- Add another supervisor, permission authority, update agent or database.
- Implement KEL-76 while its STOP remains active.
- Change the required OpenCode commit or accept reduced functionality.
- Treat a controller handoff, endpoint or PID as process custody.
- Claim native-trusted code satisfies a broker-constrained profile.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [contract](contract.md) | Source references and AC-1 through AC-36. | Defining or checking required behavior and negative controls. |
| [design](design.md) | D-1 through D-7 and architectural alternatives. | Reviewing the selected architecture and its alternatives. |
| [platform](platform.md) | D-8 platform registration, authentication and custody candidates. | Reviewing installed root and client lifetime. |
| [admission](admission.md) | D-9 proposed identities, interfaces, grants and release ordering. | Reviewing admission and bounded transport. |
| [outcomes](outcomes.md) | D-10 state schema, transactions, retention, storage limits and edit boundaries. | Reviewing durable outcomes and recovery. |
| [validation](validation.md) | Tasks, evidence, performance, open questions and blockers. | Scheduling implementation, reviewing proof or deciding exposure. |

## Current blockers

- [B-1](validation.md#b-1-protected-implementation-approval-remains-absent)
- [B-2](validation.md#b-2-independent-custody-has-no-complete-native-qualification)
- [B-3](validation.md#b-3-pinned-recovery-permits-repeated-external-effects)

## 8. Review gates triggered

Public API, permission model and wire protocol review apply to the proposed contract.
The exact implementation may also trigger unsafe or dependency review.
This documentation change admits no dependency, wire field or capability.
Named owner reviews and human specification approval remain required.
The specification PR is do-not-merge until its approval conditions are satisfied.
