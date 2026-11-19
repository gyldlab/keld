# KEL-78 authority and references
Part of: [index](index.md)
Owns: section 2

## 2. Spec refs

Source baseline: 3503a8aa383427dcf50ccbf6653c4298e7fb8aad.





Historical approval provenance: on 2026-10-23 the repository owner delegated the remaining decisions and
publication of the reviewed specification to the issue agent. The Windows T3 and unverified-vocabulary
approvals did not cover the R1 schema correction. The earlier specification approval bound
`eba60d2215a7fa2543b3673e20cd90fa7dbde555`, not R1.

The [historical R1 receipt in #1120](https://github.com/gyldlab/keld/pull/1120) identifies review head
`1ea0d6a0f25c93cea2f9f9630a96cbd1000a5f24` and specification SHA-256
`7bc71cdedb79da9bf0ac51145b2a92fb0e41b5d04c4881817b3006c62a9e96af`. R1 preserved the approved
Ready PR #657 `bb43e474577297f0792740676994aa153e5df1dd` schema 2 and separated the profile-bearing
combinations in §4.4. The independent permission-model, public-API and compiled-contract review
applied to those exact historical bytes. At the recorded review, main
`f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f` implemented schema 1 only. [S21][S22]

This split revision is pending exact-content owner review. Historical approval does not waive
implementation tests, native qualification, independent review gates, or the explicit do-not-merge
instruction.

| Concern | Governing owner and source | This amendment changes |
|---|---|---|
| Profile states, strict OS admission, forfeiture | KEL-78, existing strict-profile specification | Concrete declaration and its product integration |
| Host-minted identity and broker authorization | Architecture 03; KEL-75/KEL-102 | No grant or principal semantics |
| Permissions parsing and verified snapshot | `keld-guard`; KEL-102 | One optional, presence-sensitive profile property |
| Compiled boot contract | KEL-96/KEL-102; GH #446 for window ownership | Distinct profile-bearing versions composed with renderer/facade ownership (§4.4) |
| Evidence labels and scoring | KEL-74; approved GH-532 | Bind product labels to admitted state; retain existing scoring |
| Migration output and acceptance | X02 / GH #497, #499 | Consume this location and explicit-acceptance requirement |
| Product receipts and first proof | GH #587, #500, #589 | Consume runtime provenance; no inferred legacy |

This specification PR updates the existing KEL-78 “future key” paragraph and links this amendment.
Architecture 03's manifest ownership, Architecture 06's compiled descriptor contract, and Architecture 04's
migration file-owner table point to this single definition. Historical blanket statements that all platforms
are unverified must not override later platform-specific evidence; the specific macOS T2c strict
qualification remains open. [S1][S13]
