# Spec: local macOS application packaging and installed boot

- Status: implementing
- Tracker: GH-1404
- Owner: GYLDLAB
- Updated: 2026-11-19
- Pin: d98675c1a3b2eabb19e3000705e178f84fd169b4

## 1. Goal & non-goals

Produce KELD's first framework-owned, reusable macOS application bundle and DMG.
The app runs from an independent installation with the real Rust Host, bundled Bun,
original application resources and existing Host-enforced permissions. Local ad-hoc
signing is the initial artifact class. The original draw.io migration is the first
full acceptance consumer; a separate minimal app proves framework independence.

Implement macOS now. Design Windows/Linux now; implement them later.

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
|---|---|---|
| [refs.md](refs.md) | Section 2: architecture and owner contracts | Reviewing inputs and authority |
| [acceptance.md](acceptance.md) | Section 3: AC-1 to AC-16 | Implementing and reviewing acceptance |
| [design.md](design.md) | Section 4: design and packaging order | Changing CLI, Pack, boot or installed runtime |
| [delivery.md](delivery.md) | Sections 5–7, 9–10: boundaries, tasks, tests and blockers | Planning build and installed qualification |

## Current blockers

- B-1
- B-2

## 8. Review gates triggered

- **unsafe:** installed Apple code/resource verification or native retained-object
  operations need exact owning-path/operation review; no binary-wide unsafe grant.
- **public API:** Pack/build request, additive config extraction/ID predicate and
  installed facade facts.
- **permission model:** local provenance must not become publisher/Strict/update
  authority; existing Guard rules and graph admission remain the only policy owners.
- **dependency addition:** none planned; any necessity reopens this gate.
- **wire protocol:** no public kipc change proposed; the existing private launch-fact
  record needs a versioned closed extension and independent protocol review.

Architecture/security review is independent of the author and covers the final
diff. It does not replace the separate exact-content feature-spec approval.
