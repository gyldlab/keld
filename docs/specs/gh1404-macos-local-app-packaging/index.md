# Spec: Local macOS application packaging and installed boot

- Status: implementing
- Tracker: GH-1404
- Owner: GYLDLAB
- Updated: 2026-11-05
- Pin: 151776c72f3615f6c87d8a03ccbab00a0c08e955

Tracker of record: [GH-1404](https://github.com/gyldlab/keld/issues/1404).
Future design only: [Windows GH-1405](https://github.com/gyldlab/keld/issues/1405)
and [Linux GH-1406](https://github.com/gyldlab/keld/issues/1406).
The user authorized implementation of the exact reviewed contract at
`3c490596ceb483254551d37041d25e7e110dfa41` on 2026-11-05. The assistant recorded
that authorization through the connected non-author account in
[review 5787262030](https://github.com/gyldlab/keld/pull/1407#pullrequestreview-5787262030),
after verifying the specification and independent review findings. This is the
[workflow step 2](../../agents/workflow.md) approval record, not native implementation
or installed-artifact acceptance. The approved behavioral contract below is unchanged.
The user's delivery objective remains open until installed macOS acceptance passes.
Implementation note: the first local installed cell requires an explicitly declared
Legacy permission profile; an absent profile is refused rather than inferred as
consent to uncontained execution. Existing development schemas and their historical
profile behavior are unchanged. The local Guardian reuses the authenticated private
role with a closed `local-bundle-v1` suffix carrying the exact Bun, boot and policy
digests; those expected values are data, not an admission capability. Installed
origin is independently derived from current code before each generation. These
mechanical choices do not qualify the original application graph or final artifact.
Original owner record: GH-1404, GYLDLAB, updated 2026-11-05.

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
| [contract.md](contract.md) | Architecture/source requirements and ACs | Approving packaging behavior |
| [design-build.md](design-build.md) | Authoring, tooling and build path | Building original macOS artifacts |
| [design-runtime.md](design-runtime.md) | Installed boot, signing, runtime and original graph | Reviewing security and application runtime |
| [design-distribution.md](design-distribution.md) | DMG, reproducibility, future targets | Qualifying local distribution and platform boundaries |
| [implementation.md](implementation.md) | Boundaries, tasks and verification | Implementing or validating a release slice |
| [review.md](review.md) | Performance and remaining evidence | Reviewing incomplete native acceptance |

## Current blockers

B-1


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
