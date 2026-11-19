# KEL-78 authority and references
Part of: [index](index.md)
Owns: section 2

## 2. Spec refs

Source baseline: 3503a8aa383427dcf50ccbf6653c4298e7fb8aad.





Approval provenance: on 2026-10-23 the repository owner explicitly delegated the remaining decisions and
publication of the reviewed specification to the issue agent. This amendment records that new delegated
design decision under KEL-78; the earlier Windows T3 and unverified-vocabulary approvals did not approve
these new details. The exact committed content and independent review evidence are recorded on KEL-78 and
the specification PR. This approval does not waive implementation tests, native qualification, independent
review gates, or the explicit do-not-merge instruction.

R1 revision: the previous approval binds `eba60d2215a7fa2543b3673e20cd90fa7dbde555`, not these revised
bytes. Cross-contract reconciliation against approved Ready PR #657 at
`bb43e474577297f0792740676994aa153e5df1dd` found that both specifications assigned incompatible meanings to
schema 2. R1 preserves Ready's version and separates the profile-bearing combinations in §4.4. The narrow
uncontained-admission and receipt-dependency clarifications preserve the decisions already recorded on
KEL-78. Under the continuing owner delegation, R1's technical content was independently reviewed through the
permission-model, public-API and compiled-contract lenses with no surviving blocker; its exact committed
bytes and review receipts are recorded on #1120/KEL-78. This is a new design approval, not inherited
approval of changed bytes, runtime implementation or native proof. Current main
`f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f` still implements only schema 1. The PR remains draft and do-not-
merge. [S21][S22]

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
