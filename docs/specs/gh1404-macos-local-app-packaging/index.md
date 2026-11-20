# Spec: local macOS application packaging and installed boot

- Status: approved
- Tracker: GH-1404
- Owner: GYLDLAB
- Updated: 2026-11-20
- Pin: d7b273299bc31ff7d3d67d79f6ea2339632cfc6a

Tracker of record: [GH-1404](https://github.com/gyldlab/keld/issues/1404).
Future design only: [Windows GH-1405](https://github.com/gyldlab/keld/issues/1405)
and [Linux GH-1406](https://github.com/gyldlab/keld/issues/1406).
The amended B-1 specification was approved for implementation at
[PR #1407 commit `76f891c8`](https://github.com/gyldlab/keld/commit/76f891c806d419f6772a1ad44beca54b48b45660).
The earlier delegated approval at `3c490596ceb483254551d37041d25e7e110dfa41`
covered only the prior specification, before the module-resolution amendment.
The B-1 approval does not establish implementation or installed-app acceptance.
The user's delivery objective remains open until installed macOS acceptance passes.
Independent implementation review, native acceptance and merge clearance remain separate gates.

## 1. Goal & non-goals

Produce KELD's first framework-owned, reusable macOS application bundle and DMG.
The app runs from an independent installation with the real Rust Host, bundled Bun,
original application resources and existing Host-enforced permissions. Local ad-hoc
signing is the initial artifact class. The original draw.io migration is the first
full acceptance consumer; a separate minimal app proves framework independence.

Target macOS first, subject to the open input and acceptance gates below.
Design Windows/Linux now; implement them later.

Non-goals: Windows/Linux code, installers, adapters, traits, dependencies or pipelines;
universal or Intel Mac qualification; public Developer ID/notarized distribution;
an updater; a new policy engine, supervisor or trust service; arbitrary Electron
build-config translation; a general TypeScript evaluator; file associations or
Launch Services open-document delivery; a test harness as the application launcher;
automatic merges. No paid Apple membership is required for this local artifact.

The first native cell is `macos-arm64`, the existing target vocabulary. A second Mac
and all other platform cells remain unverified until actually exercised.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [References](01-references.md) | Section 2 | Check source and platform claims |
| [Acceptance](02-acceptance.md) | Section 3 | Implement or verify an acceptance obligation |
| [Bundle design](03-design.md) | Section 4, ownership and bundle | Change capture or admission |
| [Runtime design](04-runtime.md) | Section 4, runtime and outputs | Change launch, graph or signing |
| [Execution](05-execution.md) | Sections 5–7, 9–10 | Plan tasks, tests or unblock approval |

## Current blockers

- B-2
- B-3

## 8. Review gates triggered

- **unsafe:** installed Apple code/resource verification or native retained-object
  operations need exact owning-path/operation review; no binary-wide unsafe grant.
- **public API:** Pack/build request, additive config extraction/ID predicate and
  installed facade facts.
- **permission model:** local provenance must not become publisher/Strict/update
  authority; existing Guard rules and graph admission remain the only policy owners.
- **dependency addition:** none planned; any necessity reopens this gate.
- **wire protocol:** no public kipc change proposed; the private launch-fact record
  handed off by the GH-452/GH-570 owners (B-2/B-3) needs a versioned closed extension
  and independent protocol review.

Architecture/security review is independent of the author and covers the final
implementation diff. B-1 specification approval does not replace that review.
