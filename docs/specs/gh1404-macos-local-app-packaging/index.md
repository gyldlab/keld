# Spec: local macOS application packaging and installed boot

Status: implementing
Tracker: GH-1404
Owner: GYLDLAB
Updated: 2026-11-05
Pin: 76df3f9fa8120e599d68f190c8dcec157c58d0d0

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
| --- | --- | --- |
| [requirements](requirements.md) | References and A1–A16 acceptance | Checking inputs, approval receipts or completion obligations |
| [admission](admission.md) | Design 4.1–4.4 | Implementing config, trust or bundle admission |
| [installation](installation.md) | Design 4.5–4.8 | Implementing runtime facts, graphs, signing or future-platform boundaries |
| [delivery](delivery.md) | Boundaries, tasks, tests, performance and open gates | Planning or validating implementation and installed acceptance |

## Current blockers

none

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

