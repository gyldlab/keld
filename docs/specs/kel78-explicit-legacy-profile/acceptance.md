# KEL-78 acceptance criteria
Part of: [index](index.md)
Owns: section 3

## 3. Acceptance criteria

### AC-1: One source
- **Given:** The permissions manifest is decoded before profile admission.
- **When:** The parser reads the root profile property.
- **Then:** only the decoded root property `/profile` in `keld.permissions.jsonc` selects a requested profile. Its accepted values are exactly `"strict"` and `"legacy"`.
- **Negative control:
  - **Single fault:** Read a second app-level profile field as a selector.
  - **At-risk precondition:** The root manifest contains a profile request.
  - **Expected RED:** The root-property and duplicate-source tests fail.

### AC-2: Presence matters
- **Given:** The manifest may contain no profile property.
- **When:** The parser reads a manifest with the property absent.
- **Then:** an absent property means no declared request and follows the existing unverified/default path. Null, wrong types, other strings, duplicate decoded keys, or invalid JSONC are errors.
- **Negative control:
  - **Single fault:** Treat an absent property as Legacy.
  - **At-risk precondition:** The root property is absent.
  - **Expected RED:** The default/unverified path changes and the absence test fails.

### AC-3: No inference
- **Given:** The app may provide Electron options, fuses, names, environment or CLI values.
- **When:** The application attempts to influence the requested profile.
- **Then:** Electron sandbox options, fuses, package/app names, platform, environment, CLI flags, import failures, missing primitives, and failed strict proofs never activate legacy.
- **Negative control:
  - **Single fault:** Infer Legacy from an Electron option or failed Strict proof.
  - **At-risk precondition:** The explicit profile property is absent or different.
  - **Expected RED:** The no-inference matrix observes an unintended state.

### AC-4: Verified consumption
- **Given:** The Host has opened the digest-verified permissions manifest.
- **When:** The Host derives the requested profile for authorization.
- **Then:** the host derives its request from the same retained, digest-verified manifest used by broker authorization. It never reopens a path or trusts a child-supplied profile.
- **Negative control:
  - **Single fault:** Reopen a path or accept a child-supplied profile.
  - **At-risk precondition:** The Host retained the verified manifest bytes and digest.
  - **Expected RED:** The provenance or child-forgery test fails.

### AC-5: Version admission
- **Given:** A manifest declares a profile and a boot schema is selected.
- **When:** The boot producer and reader validate the declared profile/version pair.
- **Then:** a profile-bearing manifest requires schema 3 for a renderer-declared boot or schema 4 for a
  facade boot. Schemas 1 and 2 retain their respective renderer/facade meanings without an explicit profile.
  A supporting host refuses declaration/version or closed-shape mismatches before resources; a reader that
  lacks profile support refuses 3 and 4 rather than ignoring their declarations (§4.4).
- **Negative control:
  - **Single fault:** Downgrade schema 3/4 to 1/2 or accept a profile on an old reader.
  - **At-risk precondition:** A profile-bearing artifact reaches a supporting or unsupported reader.
  - **Expected RED:** The version-skew test observes execution or the wrong shape.

### AC-6: Real legacy launch
- **Given:** The qualified macOS primary-role path receives an explicit Legacy request.
- **When:** The Host performs the real primary-role launch.
- **Then:** on the qualified macOS primary-role path, explicit legacy reaches the Guard-owned requested-state admission dispatcher through `admit_uncontained(Legacy)` (§4.5); common launch validation, authenticated links, guard checks, and enrolled lifecycle cleanup still run. Qualified containment paths retain the full `admit()` API.
- **Negative control:
  - **Single fault:** Call generic strict admission or bypass Guard on Legacy.
  - **At-risk precondition:** The real macOS primary role requests explicit Legacy.
  - **Expected RED:** The real child or permitted/denied call evidence fails.

### AC-7: Strict unchanged
- **Given:** A candidate requests Strict admission.
- **When:** The Host evaluates the candidate proof and primitive availability.
- **Then:** a qualified candidate uses the existing full strict admission checks. Missing, stale,
  incomplete, mismatched, or wrong-layer proof, missing primitive, or unexpected grant refuses launch. The
  known-uncontained macOS primary path instead calls `admit_uncontained(Strict)` and refuses with `KELD-
  GUARD008` before execution, without fabricated proof inputs (§4.5). No unconfined replacement starts.
- **Negative control:
  - **Single fault:** Replace missing Strict proof with Legacy or an unconfined child.
  - **At-risk precondition:** A Strict candidate lacks a required proof or primitive.
  - **Expected RED:** The refusal test observes an application marker.

### AC-8: Recovery frozen
- **Given:** A Host session may create a successor generation.
- **When:** The Host recovers or starts another generation.
- **Then:** the selected request and policy snapshot remain fixed for a host session. Every successor generation uses them. Strict proof/admission is rechecked for each candidate generation.
- **Negative control:
  - **Single fault:** Reload a mutable profile during recovery.
  - **At-risk precondition:** A successor generation is created in the same Host session.
  - **Expected RED:** The immutable snapshot test detects a changed request.

### AC-9: Fresh transitions
- **Given:** A user changes the profile for a later Host session.
- **When:** The old Host and links retire and a fresh Host starts.
- **Then:** removing legacy takes effect only after the old host session and links are retired and a fresh host captures new policy. Without successful strict admission, the new run is unverified. Setting strict requires the normal strict proof.
- **Negative control:
  - **Single fault:** Reuse the old Host session after the user changes the profile.
  - **At-risk precondition:** The old Host and links have retired.
  - **Expected RED:** The fresh schema/receipt test detects stale state.

### AC-10: Truthful evidence
- **Given:** A launch produces a product evidence receipt.
- **When:** The receipt collector records the actual admitted state.
- **Then:** product legacy receipts map to `legacy_sandbox_off`; strict primary receipts map to `strict_bun`; unverified receipts map to `unverified`. Rejected launches cannot emit successful admission or workflow evidence.
- **Negative control:
  - **Single fault:** Emit a successful receipt for a rejected launch.
  - **At-risk precondition:** A product receipt is collected for an admitted or rejected attempt.
  - **Expected RED:** The trusted receipt collector rejects false success.

### AC-11: Truthful reporting
- **Given:** Doctor, runtime output and scoreboard report the selected profile.
- **When:** Each reporting surface renders the profile state.
- **Then:** legacy output includes the exact forfeiture; doctor separates a declared request from admitted state; scoreboard derives its label from the existing accessor. Renderer status remains separate.
- **Negative control:
  - **Single fault:** Render the declared request as effective admitted state.
  - **At-risk precondition:** Doctor, runtime and scoreboard read the same session.
  - **Expected RED:** The disclosure/accessor consistency tests fail.

### AC-12: Migration acceptance
- **Given:** An automated migration tool may write a permissions manifest.
- **When:** The migration tool analyzes, asks for acceptance or writes the config.
- **Then:** automated migration writes legacy only after acceptance specific to the visible weaker-security choice. Analysis, cancellation, default answers, and generic write permission do not select legacy.
- **Negative control:
  - **Single fault:** Write Legacy after analysis, cancellation or generic write consent.
  - **At-risk precondition:** The user did not accept the weaker-security choice.
  - **Expected RED:** The migration output contains no new Legacy declaration.

### AC-13: Scope preserved
- **Given:** The first implementation targets the macOS primary Bun role.
- **When:** A non-primary role or another platform requests Legacy.
- **Then:** the first implementation qualifies only the macOS primary Bun role. Other roles and platforms cannot inherit a legacy qualification from it.
- **Negative control:
  - **Single fault:** Allow another role or platform to inherit macOS qualification.
  - **At-risk precondition:** A non-primary role or unsupported target requests Legacy.
  - **Expected RED:** The unsupported-combination refusal occurs before execution.

### AC-14: No alternate bypass
- **Given:** Diagnostic and alternate execution entry points exist.
- **When:** An alternate entry attempts to skip common profile preflight.
- **Then:** diagnostic and alternate execution entry points cannot ignore an explicit profile and launch under a falsely declared state.
- **Negative control:
  - **Single fault:** Skip profile preflight in a diagnostic or alternate entry.
  - **At-risk precondition:** An explicit profile is present and the alternate entry runs.
  - **Expected RED:** The entry refuses before spawning the app.

### AC-15: Behavioral evidence retained
- **Given:** The product evidence board may contain unverified or mixed records.
- **When:** The scorer aggregates evidence records.
- **Then:** unverified records and the approved homogeneous/mixed-board scoring rules remain unchanged.
- **Negative control:
  - **Single fault:** Promote unverified or mixed evidence to a qualified score.
  - **At-risk precondition:** The board contains unverified or mixed records.
  - **Expected RED:** The scorer preserves labels and the no-percentage rule.

### AC-16: Existing authority preserved
- **Given:** The manifest also contains existing grants and channel policies.
- **When:** A profile branch is evaluated alongside existing authority checks.
- **Then:** no profile branch expands manifest grants, bypasses channel checks, relaxes stale-generation rejection, changes WebView hardening, or enables a forbidden updater/native-addon path.
- **Negative control:
  - **Single fault:** Expand a manifest grant or bypass stale-generation checks.
  - **At-risk precondition:** The profile branch performs a privileged operation.
  - **Expected RED:** Existing Guard, channel and stale-generation controls fail.
