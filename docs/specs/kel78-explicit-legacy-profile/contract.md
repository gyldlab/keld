# Legacy declaration and acceptance
Part of: [index](index.md)
Owns: Profile and boot contract

## 2. Spec refs
| Concern | Governing owner and source | This amendment changes |
|---|---|---|
| Profile states, strict OS admission, forfeiture | KEL-78, existing strict-profile specification | Concrete declaration and its product integration |
| Host-minted identity and broker authorization | Architecture 03; KEL-75/KEL-102 | No grant or principal semantics |
| Permissions parsing and verified snapshot | `keld-guard`; KEL-102 | One optional, presence-sensitive profile property |
| Compiled boot contract | KEL-96/KEL-102; GH #446 for window ownership | Distinct profile-bearing versions composed with renderer/facade ownership (§4.4) |
| Evidence labels and scoring | KEL-74; approved GH-532 | Bind product labels to admitted state; retain existing scoring |
| Migration output and acceptance | X02 / GH #497, #499 | Consume this location and explicit-acceptance requirement |
| Product receipts and first proof | GH #587, #500, #589 | Consume runtime provenance; no inferred legacy |

This specification PR updates the existing KEL-78 “future key” paragraph and links this amendment. Architecture 03's manifest ownership,
  Architecture 06's compiled descriptor contract, and Architecture 04's migration file-owner table point to this single definition. Historical
  blanket statements that all platforms are unverified must not override later platform-specific evidence; the specific macOS T2c strict
  qualification remains open. [S1][S13]

## 3. Acceptance criteria
Each criterion has a falsifier in section 7.


### AC-1: One source
- **Given:** A verified permissions manifest exists.
- **When:** The root profile is selected.
- **Then:** only the decoded root property `/profile` in `keld.permissions.jsonc` selects a requested profile. Its accepted values are exactly `"strict"` and `"legacy"`.
- **Negative control:**
  - **Single fault:** Move the profile request to a non-root source.
  - **At-risk precondition:** The parser has an admitted root manifest.
  - **Expected RED:** The request must not be selected.

### AC-2: Presence matters
- **Given:** An optional profile declaration is read.
- **When:** The parser examines its presence and type.
- **Then:** an absent property means no declared request and follows the existing unverified/default path. Null, wrong types, other strings, duplicate decoded keys, or invalid JSONC are errors.
- **Negative control:**
  - **Single fault:** Set root profile to null.
  - **At-risk precondition:** The profile key exists in an otherwise valid policy.
  - **Expected RED:** The parser must refuse the malformed declaration.

### AC-3: No inference
- **Given:** An application has ambient platform configuration.
- **When:** The host selects its runtime profile.
- **Then:** Electron sandbox options, fuses, package/app names, platform, environment, CLI flags, import failures, missing primitives, and failed strict proofs never activate legacy.
- **Negative control:**
  - **Single fault:** Set an Electron sandbox hint without a profile.
  - **At-risk precondition:** Verified policy has no profile request.
  - **Expected RED:** The host must not infer Legacy.

### AC-4: Verified consumption
- **Given:** Host policy snapshot and digest exist.
- **When:** The host consumes the manifest.
- **Then:** the host derives its request from the same retained, digest-verified manifest used by broker authorization. It never reopens a path or trusts a child-supplied profile.
- **Negative control:**
  - **Single fault:** Replace the retained manifest bytes after digest binding.
  - **At-risk precondition:** The host has the verified snapshot.
  - **Expected RED:** Admission must refuse the substituted bytes.

### AC-5: Version admission
- **Given:** A closed boot and a profile declaration exist.
- **When:** An older or mismatched boot reader loads it.
- - **Then:** a profile-bearing manifest requires schema 3 for a renderer-declared boot or schema 4 for a facade boot. Schemas 1 and 2 retain
  their respective renderer/facade meanings without an explicit profile. A supporting host refuses declaration/version or closed-shape mismatches
  before resources; a reader that lacks profile support refuses 3 and 4 rather than ignoring their declarations (§4.4).
- **Negative control:**
  - **Single fault:** Encode explicit profile inside schema 2.
  - **At-risk precondition:** The consumer supports only profile-free schema 2.
  - **Expected RED:** Boot admission must fail before resources.

### AC-6: Real legacy launch
- **Given:** A macOS primary role requests Legacy.
- **When:** The uncontained admission path starts Bun.
- **Then:** on the qualified macOS primary-role path, explicit legacy reaches the Guard-owned requested-state admission dispatcher through `admit_uncontained(Legacy)` (§4.5); common launch validation, authenticated links, guard checks, and enrolled lifecycle cleanup still run. Qualified containment paths retain the full `admit()` API.
- **Negative control:**
  - **Single fault:** Skip the Guard-owned admission dispatcher.
  - **At-risk precondition:** The host has valid authenticated role enrollment.
  - **Expected RED:** The launch cannot claim authenticated Legacy.

### AC-7: Strict unchanged
- **Given:** An explicitly Strict role lacks a qualified containment proof.
- **When:** Runtime tries to admit it.
- - **Then:** a qualified candidate uses the existing full strict admission checks. Missing, stale, incomplete, mismatched, or wrong-layer proof,
  missing primitive, or unexpected grant refuses launch. The known-uncontained macOS primary path instead calls `admit_uncontained(Strict)` and
  refuses with `KELD-GUARD008` before execution, without fabricated proof inputs (§4.5). No unconfined replacement starts.
- **Negative control:**
  - **Single fault:** Substitute fabricated proof input.
  - **At-risk precondition:** The macOS primary path is uncontained.
  - **Expected RED:** Guard must refuse execution with the strict error.

### AC-8: Recovery frozen
- **Given:** An admitted host starts a successor generation.
- **When:** Bun is replaced or recovered.
- **Then:** the selected request and policy snapshot remain fixed for a host session. Every successor generation uses them. Strict proof/admission is rechecked for each candidate generation.
- **Negative control:**
  - **Single fault:** Reload a different policy snapshot for the successor.
  - **At-risk precondition:** Original host policy remains retained.
  - **Expected RED:** The policy identity check must fail.

### AC-9: Fresh transitions
- **Given:** A previous Legacy host has active links.
- **When:** A changed manifest is offered.
- **Then:** removing legacy takes effect only after the old host session and links are retired and a fresh host captures new policy. Without successful strict admission, the new run is unverified. Setting strict requires the normal strict proof.
- **Negative control:**
  - **Single fault:** Reinterpret the active session profile.
  - **At-risk precondition:** The old host still owns links.
  - **Expected RED:** Fresh profile must not take effect before retirement.

### AC-10: Truthful evidence
- **Given:** A runtime receipt is generated.
- **When:** The effective profile is reported.
- **Then:** product legacy receipts map to `legacy_sandbox_off`; strict primary receipts map to `strict_bun`; unverified receipts map to `unverified`. Rejected launches cannot emit successful admission or workflow evidence.
- **Negative control:**
  - **Single fault:** Label an unverified run as strict.
  - **At-risk precondition:** The effective state is unverified.
  - **Expected RED:** The evidence mapping must reject false Strict.

### AC-11: Truthful reporting
- **Given:** Doctor and product reporting inspect a request.
- **When:** They display declared and admitted status.
- **Then:** legacy output includes the exact forfeiture; doctor separates a declared request from admitted state; scoreboard derives its label from the existing accessor. Renderer status remains separate.
- **Negative control:**
  - **Single fault:** Mark declared legacy as admitted without a run.
  - **At-risk precondition:** Only static manifest evidence exists.
  - **Expected RED:** The display must not claim runtime admission.

### AC-12: Migration acceptance
- **Given:** Automated migration proposes weaker containment.
- **When:** It requests user consent.
- **Then:** automated migration writes legacy only after acceptance specific to the visible weaker-security choice. Analysis, cancellation, default answers, and generic write permission do not select legacy.
- **Negative control:**
  - **Single fault:** Cancel the explicit Legacy confirmation.
  - **At-risk precondition:** A visible profile diff was presented.
  - **Expected RED:** The manifest must remain unchanged.

### AC-13: Scope preserved
- **Given:** The macOS primary role has qualified Legacy.
- **When:** An unsupported role or OS requests the same.
- **Then:** the first implementation qualifies only the macOS primary Bun role. Other roles and platforms cannot inherit a legacy qualification from it.
- **Negative control:**
  - **Single fault:** Reuse the macOS result for a different role.
  - **At-risk precondition:** No OS-specific acceptance exists there.
  - **Expected RED:** The unsupported role must refuse qualification.

### AC-14: No alternate bypass
- **Given:** An alternate diagnostic launch path exists.
- **When:** It consumes an explicit profile.
- **Then:** diagnostic and alternate execution entry points cannot ignore an explicit profile and launch under a falsely declared state.
- **Negative control:**
  - **Single fault:** Omit profile preflight on that path.
  - **At-risk precondition:** The manifest requests a profile.
  - **Expected RED:** The path must refuse or use the same admission contract.

### AC-15: Behavioral evidence retained
- **Given:** A behavioral corpus includes unverified runs.
- **When:** The scoreboard aggregates records.
- **Then:** unverified records and the approved homogeneous/mixed-board scoring rules remain unchanged.
- **Negative control:**
  - **Single fault:** Relabel unverified records as Legacy.
  - **At-risk precondition:** The corpus contains valid unverified evidence.
  - **Expected RED:** The scorer must preserve original authority labels.

### AC-16: Existing authority preserved
- **Given:** Guard enforces channel and generation state.
- **When:** A profile variant processes a denied request.
- **Then:** no profile branch expands manifest grants, bypasses channel checks, relaxes stale-generation rejection, changes WebView hardening, or enables a forbidden updater/native-addon path.
- **Negative control:**
  - **Single fault:** Skip the channel permission check for Legacy.
  - **At-risk precondition:** The request is from an authenticated role.
  - **Expected RED:** A forbidden operation must have no effect.


[S1]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/kel78-strict-profile-sandbox.md
[S13]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/architecture/03-security.md
