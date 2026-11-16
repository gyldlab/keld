# Spec: Explicit legacy compatibility profile

- Status: approved
- Tracker: KEL-78
- Owner: GYLDLAB
- Updated: 2026-10-24
- Pin: 151776c72f3615f6c87d8a03ccbab00a0c08e955

Approval provenance: on 2026-10-23 the repository owner explicitly delegated the remaining decisions and publication of the reviewed
  specification to the issue agent. This amendment records that new delegated design decision under KEL-78; the earlier Windows T3 and
  unverified-vocabulary approvals did not approve these new details. The exact committed content and independent review evidence are recorded on
  KEL-78 and the specification PR. This approval does not waive implementation tests, native qualification, independent review gates, or the
  explicit do-not-merge instruction.
R1 revision: the previous approval binds `eba60d2215a7fa2543b3673e20cd90fa7dbde555`, not these revised bytes. Cross-contract reconciliation
  against approved Ready PR #657 at `bb43e474577297f0792740676994aa153e5df1dd` found that both specifications assigned incompatible meanings to
  schema 2. R1 preserves Ready's version and separates the profile-bearing combinations in §4.4. The narrow uncontained-admission and
  receipt-dependency clarifications preserve the decisions already recorded on KEL-78. Under the continuing owner delegation, R1's technical
  content was independently reviewed through the permission-model, public-API and compiled-contract lenses with no surviving blocker; its exact
  committed bytes and review receipts are recorded on #1120/KEL-78. This is a new design approval, not inherited approval of changed bytes,
  runtime implementation or native proof. Current main `f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f` still implements only schema 1. The PR remains
  draft and do-not-merge. [S21][S22]
Historical owning tracker: KEL-78, specification GH #586, implementation GH #588.
Original source baseline: `3503a8aa383427dcf50ccbf6653c4298e7fb8aad`.

The pinned S1–S23 references used below resolve in [the source and approval ledger](review-and-evidence.md).

## 1. Goal & non-goals
Enable the macOS draw.io first proof through one explicit Keld legacy profile while preserving the existing strict-profile admission contract. A
  successful legacy launch remains authenticated and subject to Keld's broker authorization, but forfeits the claim that the operating system
  confines the Bun role to zero ambient authority. Ambient authority means access inherited from the operating-system user or process rather than
  granted through Keld.

The observable result is a declared profile parsed from one reviewed permissions manifest, consumed by the actual host launch path, retained
  across recovery, and reported accurately in runtime receipts, doctor, compatibility evidence, and eventually build/release output. A
  configuration value is a request. It is not evidence of successful admission.

This amendment follows KEL-78 ownership. It does not create another authority model. KEL-74 remains the evidence/scoring owner; KEL-75 owns role identity and lifecycle; KEL-96/KEL-102 own boot and verified policy loading. [S1][S2][S3]

Non-goals:

- Implementing macOS strict App Sandbox qualification or weakening any strict check.
- Adding per-role profile overrides, mixed strict/legacy applications, addon approval, in-host plugin authority, renderer sandbox controls, or updater privileges.
- Completing Electron IPC, BrowserWindow, preload, dialog/menu, or draw.io behavior.
- Implementing the entire build or migration command to deliver this profile slice.
- Changing the evidence schema, denominator rules, existing authority vocabulary, or conformance-harness provenance.
- Claiming Windows/Linux legacy support from a macOS test.
- Merging a PR or treating design approval as implemented runtime behavior.

### 1.1 Verified starting point

The underlying contract already exists in approved KEL-78: explicit legacy, authenticated and guarded operation, unsandboxed Bun, visible
  forfeiture, separate renderer status, and fail-closed strict admission. The missing parts are the authoritative key location, parser
  integration, product launch integration, compatibility/version admission, and reporting. At the source baseline, the KEL-78 spec still
  described the key as future work and #586/#588 remained open. No implementing or key-spec PR was located before this amendment in their
  timelines or the current open-PR review. [S1][S2][S4]

Current `PermissionsManifest` ignores unknown top-level keys. `admit()` already has `Unverified`, `Legacy`, and `Strict` branches; its legacy
  return is a policy-state result, not proof that a child was authenticated, guarded, or supervised. macOS development already captures and
  stages the project's real permissions bytes. The host retains the digest-verified manifest for its session. These are the reuse points.
  [S5][S6][S7][S8]

`keld build` and `keld migrate` are reserved commands returning `KELD-CLI-045`, exit 2. Doctor is live, but has no security-profile finding. The public compatibility page remains a placeholder and the documented committed product-corpus list is empty. [S9][S10][S11]

### 1.2 Superseded planning text

The older #586/#588 statements that `unverified` has no evidence enum and must produce no scoreable record are superseded.

KEL-78's owner decision on 2026-10-07 selected an explicit `unverified` evidence value. Approved GH-532 and merged PRs #612/#638 implement that
  decision. Preserve unverified behavioral runs; never relabel them strict or legacy. A homogeneous unverified board can retain its behavioral
  percentage when the existing denominator, identity, and panel rules permit one. It cannot qualify as the declared-legacy draw.io proof or a
  strict-security proof. Mixed authority profiles still withhold percentage and completion. [S3][S12]

The #586 specification PR and refreshed owning issue briefs record this reconciliation explicitly. It must not restore the obsolete enum ban or add a blanket unverified-scoring exclusion.

## Parts

| Part | Owns | Load when |
|---|---|---|
| [contract.md](contract.md) | Profile declarations, boot acceptance | Reading Guard/compiler requirements |
| [design-policy.md](design-policy.md) | Policy declaration and boot versions | Implementing parsed policy and boot |
| [design-runtime.md](design-runtime.md) | Runtime admission, evidence and migration | Implementing live Legacy and Strict behavior |
| [implementation.md](implementation.md) | Boundaries, tasks and failure-first cases | Building and testing profile integration |
| [review-and-evidence.md](review-and-evidence.md) | Performance, blockers and original provenance | Reviewing unresolved acceptance and sources |

## Current blockers

B-1

B-2

B-3

B-4

B-5

B-6

B-7

B-8

B-9

B-10

B-11


## 8. Review gates triggered
| Gate | Applies? | Required review |
|---|---|---|
| Permission model | Yes | KEL-78 owner; explicit forfeiture, no new grant, no fallback |
| Public API | Yes | Manifest grammar/accessor, reporting, compatibility behavior |
| Wire/compiled protocol | Yes, private boot descriptor | KEL-96/KEL-102 exact 1/2/3/4 composition, producer/consumer and version-skew refusal |
| Unsafe | None expected | Reuse current platform mechanisms; new unsafe is separately gated |
| Dependency | None expected | Reuse current parser/runtime/scorer |

The specification and implementation PRs each require named independent security/architecture evidence on their exact final diff. The delegated design approval is recorded in the header and section 10; it does not replace these gates or constitute native proof. The specification PR is explicitly do-not-merge.
