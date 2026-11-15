# Spec: Explicit legacy compatibility profile: requirements

Part of: [index](index.md)
Owns: Source authority, provenance and acceptance obligations

## 2. Spec refs

Historical approval and source context:

Approval provenance: on 2026-10-23 the repository owner explicitly delegated the remaining decisions and publication of the reviewed specification to the issue agent. This amendment records that new delegated design decision under KEL-78; the earlier Windows T3 and unverified-vocabulary approvals did not approve these new details. The exact
committed content and independent review evidence are recorded on KEL-78 and the specification PR. This approval does not waive implementation tests, native qualification, independent review gates, or the explicit do-not-merge instruction.

R1 revision: the previous approval binds `eba60d2215a7fa2543b3673e20cd90fa7dbde555`, not these revised bytes. Cross-contract reconciliation against approved Ready PR #657 at `bb43e474577297f0792740676994aa153e5df1dd` found that both specifications assigned incompatible meanings to schema 2. R1 preserves Ready's version and separates the
profile-bearing combinations in §4.4. The narrow uncontained-admission and receipt-dependency clarifications preserve the decisions already recorded on KEL-78. Under the continuing owner delegation, R1's technical content was independently reviewed through the permission-model, public-API and compiled-contract lenses with no surviving blocker;
its exact committed bytes and review receipts are recorded on #1120/KEL-78. This is a new design approval, not inherited approval of changed bytes, runtime implementation or native proof. Current main `f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f` still implements only schema 1. The PR remains draft and do-not-merge. [S21][S22]

| Concern | Governing owner and source | This amendment changes |
|---|---|---|
| Profile states, strict OS admission, forfeiture | KEL-78, existing strict-profile specification | Concrete declaration and its product integration |
| Host-minted identity and broker authorization | Architecture 03; KEL-75/KEL-102 | No grant or principal semantics |
| Permissions parsing and verified snapshot | `keld-guard`; KEL-102 | One optional, presence-sensitive profile property |
| Compiled boot contract | KEL-96/KEL-102; GH #446 for window ownership | Distinct profile-bearing versions composed with renderer/facade ownership (§4.4) |
| Evidence labels and scoring | KEL-74; approved GH-532 | Bind product labels to admitted state; retain existing scoring |
| Migration output and acceptance | X02 / GH #497, #499 | Consume this location and explicit-acceptance requirement |
| Product receipts and first proof | GH #587, #500, #589 | Consume runtime provenance; no inferred legacy |

This specification PR updates the existing KEL-78 “future key” paragraph and links this amendment. Architecture 03's manifest ownership, Architecture 06's compiled descriptor contract, and Architecture 04's migration file-owner table point to this single definition. Historical blanket statements that all platforms are unverified must not
override later platform-specific evidence; the specific macOS T2c strict qualification remains open. [S1][S13]

## 3. Acceptance criteria

Each criterion has a falsifier in section 7.

1. **One source:** only the decoded root property `/profile` in `keld.permissions.jsonc` selects a requested profile. Its accepted values are exactly `"strict"` and `"legacy"`.
2. **Presence matters:** an absent property means no declared request and follows the existing unverified/default path. Null, wrong types, other strings, duplicate decoded keys, or invalid JSONC are errors.
3. **No inference:** Electron sandbox options, fuses, package/app names, platform, environment, CLI flags, import failures, missing primitives, and failed strict proofs never activate legacy.
4. **Verified consumption:** the host derives its request from the same retained, digest-verified manifest used by broker authorization. It never reopens a path or trusts a child-supplied profile.
5. **Version admission:** a profile-bearing manifest requires schema 3 for a renderer-declared boot or schema 4 for a facade boot. Schemas 1 and 2 retain their respective renderer/facade meanings without an explicit profile. A supporting host refuses declaration/version or closed-shape mismatches before resources; a reader that lacks profile
   support refuses 3 and 4 rather than ignoring their declarations (§4.4).
6. **Real legacy launch:** on the qualified macOS primary-role path, explicit legacy reaches the Guard-owned requested-state admission dispatcher through `admit_uncontained(Legacy)` (§4.5); common launch validation, authenticated links, guard checks, and enrolled lifecycle cleanup still run. Qualified containment paths retain the full `admit()` API.
7. **Strict unchanged:** a qualified candidate uses the existing full strict admission checks. Missing, stale, incomplete, mismatched, or wrong-layer proof, missing primitive, or unexpected grant refuses launch. The known-uncontained macOS primary path instead calls `admit_uncontained(Strict)` and refuses with `KELD-GUARD008` before execution,
   without fabricated proof inputs (§4.5). No unconfined replacement starts.
8. **Recovery frozen:** the selected request and policy snapshot remain fixed for a host session. Every successor generation uses them. Strict proof/admission is rechecked for each candidate generation.
9. **Fresh transitions:** removing legacy takes effect only after the old host session and links are retired and a fresh host captures new policy. Without successful strict admission, the new run is unverified. Setting strict requires the normal strict proof.
10. **Truthful evidence:** product legacy receipts map to `legacy_sandbox_off`; strict primary receipts map to `strict_bun`; unverified receipts map to `unverified`. Rejected launches cannot emit successful admission or workflow evidence.
11. **Truthful reporting:** legacy output includes the exact forfeiture; doctor separates a declared request from admitted state; scoreboard derives its label from the existing accessor. Renderer status remains separate.
12. **Migration acceptance:** automated migration writes legacy only after acceptance specific to the visible weaker-security choice. Analysis, cancellation, default answers, and generic write permission do not select legacy.
13. **Scope preserved:** the first implementation qualifies only the macOS primary Bun role. Other roles and platforms cannot inherit a legacy qualification from it.
14. **No alternate bypass:** diagnostic and alternate execution entry points cannot ignore an explicit profile and launch under a falsely declared state.
15. **Behavioral evidence retained:** unverified records and the approved homogeneous/mixed-board scoring rules remain unchanged.
16. **Existing authority preserved:** no profile branch expands manifest grants, bypasses channel checks, relaxes stale-generation rejection, changes WebView hardening, or enables a forbidden updater/native-addon path.

### AC-1: Explicit Legacy preserves the truthful authority disclosure

- **Given:** The root permissions manifest declares the legacy profile explicitly.
- **When:** The current Guard parser and runtime admit the Legacy role.
- **Then:** The report states the requested Legacy profile and the canonical security-forfeit line without asserting Strict containment.
- **Negative control:** Change only the root profile declaration to an unqualified strict request.
- **Single fault:** Replace legacy with strict while leaving qualified strict proof unavailable.
- **At-risk precondition:** Static configuration is valid but runtime strict admission has not been observed.
- **Expected RED:** The admission/report test refuses the unqualified strict claim and never emits a strict-verified result.
