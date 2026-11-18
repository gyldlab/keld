# Spec: Explicit legacy compatibility profile

Status: approved — R1 owner-delegated compiled-boot composition correction; implementation pending

Linear: KEL-78 · Existing owner: GYLDLAB · Specification: GH #586 · Implementation: GH #588

Updated: 2026-10-24 · Source baseline: `3503a8aa383427dcf50ccbf6653c4298e7fb8aad`

Approval provenance: on 2026-10-23 the repository owner explicitly delegated the remaining decisions and publication of the reviewed specification to the issue agent. This amendment records that new delegated design decision under KEL-78; the earlier Windows T3 and unverified-vocabulary approvals did not approve these new details. The exact committed content and independent review evidence are recorded on KEL-78 and the specification PR. This approval does not waive implementation tests, native qualification, independent review gates, or the explicit do-not-merge instruction.

R1 revision: the previous approval binds `eba60d2215a7fa2543b3673e20cd90fa7dbde555`, not these revised bytes. Cross-contract reconciliation against approved Ready PR #657 at `bb43e474577297f0792740676994aa153e5df1dd` found that both specifications assigned incompatible meanings to schema 2. R1 preserves Ready's version and separates the profile-bearing combinations in §4.4. The narrow uncontained-admission and receipt-dependency clarifications preserve the decisions already recorded on KEL-78. Under the continuing owner delegation, R1's technical content was independently reviewed through the permission-model, public-API and compiled-contract lenses with no surviving blocker; its exact committed bytes and review receipts are recorded on #1120/KEL-78. This is a new design approval, not inherited approval of changed bytes, runtime implementation or native proof. Current main `f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f` still implements only schema 1. The PR remains draft and do-not-merge. [S21][S22]

## 1. Goal and non-goals

Enable the macOS draw.io first proof through one explicit Keld legacy profile while preserving the existing strict-profile admission contract. A successful legacy launch remains authenticated and subject to Keld's broker authorization, but forfeits the claim that the operating system confines the Bun role to zero ambient authority. Ambient authority means access inherited from the operating-system user or process rather than granted through Keld.

The observable result is a declared profile parsed from one reviewed permissions manifest, consumed by the actual host launch path, retained across recovery, and reported accurately in runtime receipts, doctor, compatibility evidence, and eventually build/release output. A configuration value is a request. It is not evidence of successful admission.

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

The underlying contract already exists in approved KEL-78: explicit legacy, authenticated and guarded operation, unsandboxed Bun, visible forfeiture, separate renderer status, and fail-closed strict admission. The missing parts are the authoritative key location, parser integration, product launch integration, compatibility/version admission, and reporting. At the source baseline, the KEL-78 spec still described the key as future work and #586/#588 remained open. No implementing or key-spec PR was located before this amendment in their timelines or the current open-PR review. [S1][S2][S4]

Current `PermissionsManifest` ignores unknown top-level keys. `admit()` already has `Unverified`, `Legacy`, and `Strict` branches; its legacy return is a policy-state result, not proof that a child was authenticated, guarded, or supervised. macOS development already captures and stages the project's real permissions bytes. The host retains the digest-verified manifest for its session. These are the reuse points. [S5][S6][S7][S8]

`keld build` and `keld migrate` are reserved commands returning `KELD-CLI-045`, exit 2. Doctor is live, but has no security-profile finding. The public compatibility page remains a placeholder and the documented committed product-corpus list is empty. [S9][S10][S11]

### 1.2 Superseded planning text

The older #586/#588 statements that `unverified` has no evidence enum and must produce no scoreable record are superseded.

KEL-78's owner decision on 2026-10-07 selected an explicit `unverified` evidence value. Approved GH-532 and merged PRs #612/#638 implement that decision. Preserve unverified behavioral runs; never relabel them strict or legacy. A homogeneous unverified board can retain its behavioral percentage when the existing denominator, identity, and panel rules permit one. It cannot qualify as the declared-legacy draw.io proof or a strict-security proof. Mixed authority profiles still withhold percentage and completion. [S3][S12]

The #586 specification PR and refreshed owning issue briefs record this reconciliation explicitly. It must not restore the obsolete enum ban or add a blanket unverified-scoring exclusion.

## 2. Spec references and authority

| Concern | Governing owner and source | This amendment changes |
|---|---|---|
| Profile states, strict OS admission, forfeiture | KEL-78, existing strict-profile specification | Concrete declaration and its product integration |
| Host-minted identity and broker authorization | Architecture 03; KEL-75/KEL-102 | No grant or principal semantics |
| Permissions parsing and verified snapshot | `keld-guard`; KEL-102 | One optional, presence-sensitive profile property |
| Compiled boot contract | KEL-96/KEL-102; GH #446 for window ownership | Distinct profile-bearing versions composed with renderer/facade ownership (§4.4) |
| Evidence labels and scoring | KEL-74; approved GH-532 | Bind product labels to admitted state; retain existing scoring |
| Migration output and acceptance | X02 / GH #497, #499 | Consume this location and explicit-acceptance requirement |
| Product receipts and first proof | GH #587, #500, #589 | Consume runtime provenance; no inferred legacy |

This specification PR updates the existing KEL-78 “future key” paragraph and links this amendment. Architecture 03's manifest ownership, Architecture 06's compiled descriptor contract, and Architecture 04's migration file-owner table point to this single definition. Historical blanket statements that all platforms are unverified must not override later platform-specific evidence; the specific macOS T2c strict qualification remains open. [S1][S13]

## 3. Acceptance criteria

Each criterion has a falsifier in section 7.

1. **One source:** only the decoded root property `/profile` in `keld.permissions.jsonc` selects a requested profile. Its accepted values are exactly `"strict"` and `"legacy"`.
2. **Presence matters:** an absent property means no declared request and follows the existing unverified/default path. Null, wrong types, other strings, duplicate decoded keys, or invalid JSONC are errors.
3. **No inference:** Electron sandbox options, fuses, package/app names, platform, environment, CLI flags, import failures, missing primitives, and failed strict proofs never activate legacy.
4. **Verified consumption:** the host derives its request from the same retained, digest-verified manifest used by broker authorization. It never reopens a path or trusts a child-supplied profile.
5. **Version admission:** a profile-bearing manifest requires schema 3 for a renderer-declared boot or schema 4 for a facade boot. Schemas 1 and 2 retain their respective renderer/facade meanings without an explicit profile. A supporting host refuses declaration/version or closed-shape mismatches before resources; a reader that lacks profile support refuses 3 and 4 rather than ignoring their declarations (§4.4).
6. **Real legacy launch:** on the qualified macOS primary-role path, explicit legacy reaches the Guard-owned requested-state admission dispatcher through `admit_uncontained(Legacy)` (§4.5); common launch validation, authenticated links, guard checks, and enrolled lifecycle cleanup still run. Qualified containment paths retain the full `admit()` API.
7. **Strict unchanged:** a qualified candidate uses the existing full strict admission checks. Missing, stale, incomplete, mismatched, or wrong-layer proof, missing primitive, or unexpected grant refuses launch. The known-uncontained macOS primary path instead calls `admit_uncontained(Strict)` and refuses with `KELD-GUARD008` before execution, without fabricated proof inputs (§4.5). No unconfined replacement starts.
8. **Recovery frozen:** the selected request and policy snapshot remain fixed for a host session. Every successor generation uses them. Strict proof/admission is rechecked for each candidate generation.
9. **Fresh transitions:** removing legacy takes effect only after the old host session and links are retired and a fresh host captures new policy. Without successful strict admission, the new run is unverified. Setting strict requires the normal strict proof.
10. **Truthful evidence:** product legacy receipts map to `legacy_sandbox_off`; strict primary receipts map to `strict_bun`; unverified receipts map to `unverified`. Rejected launches cannot emit successful admission or workflow evidence.
11. **Truthful reporting:** legacy output includes the exact forfeiture; doctor separates a declared request from admitted state; scoreboard derives its label from the existing accessor. Renderer status remains separate.
12. **Migration acceptance:** automated migration writes legacy only after acceptance specific to the visible weaker-security choice. Analysis, cancellation, default answers, and generic write permission do not select legacy.
13. **Scope preserved:** the first implementation qualifies only the macOS primary Bun role. Other roles and platforms cannot inherit a legacy qualification from it.
14. **No alternate bypass:** diagnostic and alternate execution entry points cannot ignore an explicit profile and launch under a falsely declared state.
15. **Behavioral evidence retained:** unverified records and the approved homogeneous/mixed-board scoring rules remain unchanged.
16. **Existing authority preserved:** no profile branch expands manifest grants, bypasses channel checks, relaxes stale-generation rejection, changes WebView hardening, or enables a forbidden updater/native-addon path.

## 4. Design

### 4.1 First-principles decomposition and reuse

A principal is an identity minted by the host for a specific role or WebView generation. Authentication establishes which principal owns a link. Authorization decides whether that principal may perform a brokered operation. OS containment separately restricts direct system calls. These are independent checks; none substitutes for another.

| Decision-bearing unit | Owner and boundary | Input → output | Failure and first observable |
|---|---|---|---|
| Declaration | Guard parser; manifest bytes | Unique root string → declared request | Invalid/null/duplicate fails parsing |
| Artifact/config identity | Existing boot/KEL-102 loader | Retained bytes + expected digest → VerifiedManifest | Byte substitution fails before resources |
| Runtime capability | Boot compiler and reader | Window ownership + verified declaration + supported version → admitted contract | Old reader rejects profile-bearing 3/4; shape or presence mismatch refuses |
| Identity/authentication | Existing host/role supervisor | Host-owned enrollment and handshake → bound link | Forged or foreign link rejected |
| Broker authorization | Guard/native dispatch | Principal + channel + arguments → allow/deny | Denied request has no handler side effect |
| OS containment | KEL-78 strict admission | Exact artifact/profile/platform + proof → Strict or error | Missing/wrong-layer proof prevents execution |
| Lifecycle/revocation | Existing supervisor/guardian | Generation state → teardown/recovery | Stale generation rejected; enrolled process retirement observed |
| Evidence | KEL-74/GH-532 consumer | Runtime receipt + behavioral result → correct label | Receipt/record mismatch rejected |
| Migration acceptance | X02 | Proposed diff + affirmative decision → explicit key | Cancelled/noninteractive default produces no key |

The dependency edges are explicit: declaration comes from verified policy; evidence comes from actual admitted state; recovery consumes the original policy. A passing guard test does not prove containment. A passing cleanup test does not prove hostile descendants cannot escape. A valid configuration does not prove a run occurred.

Reuse `parse_manifest`, decoded-key uniqueness, `VerifiedManifest`, `GuardSnapshot`, `admit()`, the current supervisor/guardian, and KEL-74 accessors. Do not create a second JSONC decoder, profile-state machine, grant evaluator, process reaper, or scoring map. No performance claim motivates this work.

### 4.2 Exact authoring contract

**Authoritative location:** root `/profile` in `keld.permissions.jsonc`.

Minimal declaration fragment:

```jsonc
{
  "profile": "legacy",
  "app": {}
}
```

This fragment demonstrates profile syntax only. It is not a complete draw.io permissions manifest and grants no channel or system capability.

Logical grammar:

```text
profile-member = "profile" : ("strict" | "legacy")
presence       = absent | exactly one profile-member
absent         = no explicit request
```

The grammar is applied to JSON-decoded strings. Standard JSON escapes are allowed: an escaped spelling that decodes to `profile` is the same key and participates in duplicate rejection. There is no case folding, Unicode normalization, trimming, boolean coercion, array selection, or last-key-wins behavior.

| Input | Required outcome |
|---|---|
| No root profile | No declared request; unverified/default path |
| `"profile": "legacy"` | Requested Legacy |
| `"profile": "strict"` | Requested Strict, subject to full admission |
| `null`, booleans, numbers, arrays, objects | Parse error |
| `""`, `"Legacy"`, `" strict"`, `"strict "`, `"unverified"` | Parse error |
| Duplicate root key, even identical values | Parse error |
| Literal key plus escaped-equivalent key | Parse error |
| Recognized misplaced security-profile key under app/window/role configuration | Configuration-location error; never a fallback |
| Profile in `keld.config.ts`, `keld.build.ts`, or `keld.compat.ts` | Unsupported authoring location; never used for selection |
| Electron `sandbox: false` without root key | No declared request; never Legacy |
| Missing entire manifest on current macOS dev path | Only existing ENOENT all-deny fallback; never Legacy |
| Unreadable, malformed, oversized or integrity-mismatched manifest | Existing typed failure; never treated as absent |

Reserve the security-profile property at the known alternate policy locations `/app/profile`, `/windows/<window>/profile`, and `/roles/<role>/profile` so an apparent per-role override is not silently accepted. Do not recursively prohibit unrelated authentication-provider or persistent-WebView profile concepts. Window and role identifiers named “profile” are not themselves overrides.

The boot producer rejects a literal root security-profile declaration in its supported app-config syntax with guidance to move it to the permissions manifest. It must reuse the existing root-property scanning owner rather than execute TypeScript or add line-prefix security parsing. Dynamic/unsupported config syntax cannot become another profile source.

The existing 64 KiB manifest limit, JSONC comment handling, recursive decoded-key uniqueness, UTF-8 rules, and unrelated unknown-key behavior remain intact. The document must be a JSON object; positional array deserialization is not another manifest syntax. An implementation must distinguish missing from explicit null; a naive optional-enum decoder that treats null as missing violates this contract. [S5][S14]

Required type shape, owned by `keld-guard`:

```rust
pub enum DeclaredProfile {
    Strict,
    Legacy,
}

impl PermissionsManifest {
    pub fn declared_profile(&self) -> Option<DeclaredProfile>;
}
```

The spelling and location are public API. The accessor returns policy data, not a live permission or effective admission state. `ProfileState::Unverified` remains an internal/reporting state; it is not a third configurable value.

### 4.3 Request, effective state, and evidence

| Configuration and launch result | Requested state | Effective result | Product evidence label |
|---|---|---|---|
| Valid manifest, no declaration; existing default launch succeeds | Unverified/no explicit selection | Unverified | `unverified` |
| Explicit legacy; common checks and macOS launch succeed | Legacy | Legacy | `legacy_sandbox_off` |
| Explicit strict; complete exact admission succeeds | Strict | Strict | `strict_bun` for this primary-role scope |
| Explicit strict; admission fails | Strict remains requested | Rejected; no replacement | No successful-run label |
| Invalid declaration or unsupported boot/runtime pairing | None accepted | Rejected before app execution | Diagnostic failure only |
| Legacy key removed; fresh valid no-profile session | Unverified/no explicit selection | Unverified | `unverified` |
| Legacy changed to strict; no valid strict proof | Strict | Rejected | No strict or legacy success |

A failed attempt can be retained as failure diagnostics by the relevant evidence contract. It must not invent an admitted legacy or strict role or a completed application workflow.

`AuthorityProfile` describes evidence provenance; it does not grant runtime authority. Leave `sandboxed_addon_worker` and `user_approved_tool_child` under their existing owners. A top-level legacy declaration is not permission to start either class or to load arbitrary native code.

The request is application-session policy. The first admitted product receipt covers only macOS's primary Bun role. A future separately declared role must consume the same request through its owning admission path, or refuse an unsupported combination before execution. No per-role override syntax or mixed-profile application is approved here. A primary receipt cannot qualify another role.

### 4.4 Boot-version compatibility and downgrade prevention

This is a required part of #588. Older hosts ignore unknown manifest properties, including `profile`. The manifest key alone is therefore insufficient to make those hosts respect a strict request. [S5]

Use the existing `keld.boot.json` version discriminator. Window ownership and the explicit security request are independent inputs. Ready's approved schema 2 is already a closed, windowless facade shape; it must not acquire the renderer-bearing meaning from the earlier revision of this amendment. Preserve it and assign the two profile-bearing combinations distinct versions. [S21]

| Window ownership selected by the existing config owner | Verified root profile | Exact compiled version | Exact closed shape |
|---|---|---|---|
| Renderer-declared | Absent | 1 | Existing `schema`, `name`, `entry`, `renderer`, `permissions`, optional `auth` |
| Facade (`initialWindow: "none"`, GH #446) | Absent | 2 | GH #446's `schema`, `name`, `entry`, `permissions`; no `renderer` or `auth` |
| Renderer-declared | Valid strict or legacy | 3 | Schema 1's fields and validation, including optional `auth`, with `schema: 3` |
| Facade (`initialWindow: "none"`, GH #446) | Valid strict or legacy | 4 | Schema 2's fields and validation, with `schema: 4`; no `renderer` or `auth` |

No descriptor carries a second `profile` value. The existing verified permissions bytes remain the sole selection source. The compiler selects one exact row from the captured configuration and permissions; it does not execute TypeScript, infer facade mode from a missing renderer, or infer a security request from the version alone. A malformed declaration fails before version selection.

Supporting readers retain the exact parsed version through verified-policy preflight and enforce all of these predicates before creating an application child, listener, or window:

- Versions 1/2 require absence of a root profile; an explicit declaration is a version mismatch, not an ignored extension.
- Versions 3/4 require a valid explicit declaration; absence is inconsistent compiled input.
- Versions 1/3 require a renderer and keep its existing validation; versions 2/4 forbid the renderer field even when null. The optional auth carrier remains available only in 1/3. Unknown or duplicate decoded fields, non-object documents, invalid UTF-8, and oversized input remain errors.
- Dispatch uses an exact supported-version set, not a numeric range or `schema >= 3`. A future version cannot enter a current profile or Ready branch by ordering alone.
- A recognized shape is not proof that its platform, window mode or role is implemented. Refuse unsupported combinations before resources. Reuse the boot-schema error for unsupported versions/mismatches and the owning unsupported-platform error for a valid but unavailable mode.

The current schema-1-only reader already rejects `schema != 1` with `KELD-CORE-035`. A Ready-only reader supporting exactly 1/2 must likewise reject 3/4. Thus a profile-bearing facade cannot silently run under an old Ready implementation that ignores unknown manifest keys. Tests must bind both reader capability sets, with an actual previous artifact where available; a hypothetical 1/2 reader model is a compatibility contract test, not an executed historical artifact. [S15][S21]

**Partial rollout:** #588 may prepare and qualify the renderer/profile row 3 before GH #446 implements facade startup. That producer emits only supported rows and must refuse a requested facade/profile combination; it must never manufacture a renderer or emit 3 for it. Neither 2 nor 4 is executable until the complete GH #446 Ready/window-ownership consumer exists. The later facade/profile integration emits and consumes 4 atomically with profile preflight, mandatory disclosure and GH #446's unchanged Ready/recovery/custody rules. Reserving 4 is not permission to add an accepting stub or to claim draw.io activation. Removing a profile from a fresh, fully supported facade session selects 2, not 1; window ownership must not change as a side effect of the profile edit.

If GH #446's schema-2 implementation is integrated first, its producer and verified host preflight must already recognize-and-refuse an explicit root security profile. It need not activate profiles or support 3/4, but it cannot silently accept a declaration that it does not enforce. This is an integration prerequisite for newly introduced schema-2 support, not a claim that old, unmodified schema-1 artifacts can be repaired retroactively.

**Rejected alternatives:** repurpose 2 (conflicts with its approved closed shape); permit explicit profiles in 2 (a Ready-only old reader could ignore Strict); infer mode from field presence or try a different parser after an error (changes ownership on malformed input); add a second manifest profile value to the descriptor (two authority sources); use one extensible new union for every combination (unneeded rewrite of both closed contracts). Versions 3/4 reuse the existing shapes with a deliberate compatibility barrier. There is no automatic downgrade from 3/4 to 1/2, no declaration stripping, and no retry in another mode.

Migration and generated artifacts must pin a supporting compiler and host pair. New code cannot retroactively make an old compiler plus old host understand a future key. Hand-editing the key into such an old installation is unsupported. Build/release producers inherit this requirement.

Every execution entry point must use common profile preflight. In particular, `run_unprivileged` currently discards the permissions descriptor for a diagnostic session. It must refuse explicit-profile versions 3/4 and any explicit declaration presented through 1/2; its renderer-only diagnostic path also cannot execute facade versions 2/4. For a supported profile-free diagnostic input, retain the same verified manifest integrity check. The high-level `HostOwnedHelloSession` diagnostic entry used directly by CLI dev helpers must also refuse an explicit project profile before creating a listener or child: use the existing bounded manifest loader for a direct project input, and the retained digest-verifying loader when a compiled descriptor supplies a digest. Never invent verified provenance for a direct input without one. Missing-file behavior stays with the existing entry's contract; malformed/unreadable policy is not absence.

Valid profile-free diagnostic execution remains separately labelled unverified. Shared preflight intentionally moves malformed or digest-mismatched diagnostic policy from an ignored input to a refusal. The compiler also parses captured manifest bytes to choose the schema, so malformed input may fail during compilation instead of later host startup; tests must preserve refusal while documenting this earlier error phase. [S16]

This is a narrowly scoped compiled-contract version amendment requiring KEL-96/KEL-102 review. It is not a public kipc frame change.

### 4.5 Runtime admission and policy lifetime

1. Validate the selected artifact, package/stage identity, and boot descriptor through the existing owners.
2. Load the permissions through the existing retained-handle, digest-verifying loader. Parse the same bytes once.
3. Check boot version against declaration presence. Reject invalid or unsupported combinations before creating an application child, listener, or window.
4. Freeze the requested state in the existing host-session policy snapshot. Retain the permissions digest as its identity.
5. For explicit Legacy on the known-uncontained macOS primary path, call `admit_uncontained(Legacy)`, which shares the existing requested-state dispatcher; full candidate/proof paths keep `admit()`. Do not fabricate proof identities to call the full API, and do not treat either pure state result as proof of a successful launch.
6. For Strict on a qualified containment path, gather the actual candidate artifact/platform/profile/proof inputs and call the unchanged full `admit()` checks. On the known-uncontained macOS primary path, call `admit_uncontained(Strict)` and refuse with the existing missing-primitive error before execution. Either failure ends the attempt; there is no unsandboxed retry or fabricated proof identity.
7. Emit the canonical legacy forfeiture on the real launch path before releasing application execution, explicitly marked as a requested profile until admission succeeds. Use the existing role/supervisor path for enrollment, link authentication, guard-before-handler dispatch, and generation revocation. Only after the actual required launch/bind events may the host expose an admitted receipt.
8. On recovery, retain the same requested profile and permissions snapshot. Revalidate candidate-specific strict admission and proof freshness. Never reread user-editable configuration to downgrade a running strict session.
9. To change profile or grants, retire the full host session and links, then capture new policy in a fresh host. A renderer reload or Bun restart alone cannot switch policy.

Separate digests: the permissions content digest identifies configuration bytes; the strict proof's profile digest identifies its OS containment policy; the artifact digest identifies the executable under proof. One is not a substitute for another.

**Known-uncontained primary path:** the current macOS launcher has no qualified Strict containment or complete candidate proof identity. It must not manufacture artifact/profile digests merely to call the full proof API. A narrow Guard-owned `admit_uncontained(requested: ProfileState) -> Result<ProfileState, AdmissionError>` may share the existing requested-state dispatcher and return only Unverified or Legacy. Strict must fail with the existing missing-primitive error (`KELD-GUARD008`); if the primitive check ever stops rejecting, absent proof must still fail with `KELD-GUARD011`, never admit Strict. This entry describes a positively known uncontained path, not a fallback after `admit()` fails. The full qualified-path `admit()` and its ordered proof checks remain unchanged. Recovery reuses the frozen request through the same admission owner. This adds no successful Strict mode and supplies no runtime or product receipt by itself.

No declaration must not disable an existing Windows/Linux containment mechanism. The existing default runtime behavior remains intact. The new declared-profile reporting path maps omission to Unverified; it never invents a strict claim from platform choice. Existing separately qualified strict platform fixtures and their evidence are not relabelled by this amendment.

The initial explicit-legacy implementation is qualified on macOS only. Unsupported target/role combinations fail before app execution with an actionable unsupported-profile diagnostic. A non-macOS compiler must not discard an explicit project profile by staging its historical fixed empty-policy fallback; it must detect and reject unsupported explicit-profile input. Existing Windows/Linux strict mechanisms stay with their owners and are not removed to make legacy start.

Existing strict failures remain `KELD-GUARD008` (primitive unavailable), `009` (unexpected grant), `010` (host-handle leak), `011` (proof missing), `012` (proof stale), `013` (identity mismatch), `014` (incomplete catalog), and `015` (wrong evidence layer/oracle). Use the existing error types and fixes; do not invent replacement runtime codes. Manifest syntax/type errors remain in the existing parser error category, and boot/version inconsistency uses the boot-contract category. [S6]

### 4.6 Authority and security-forfeit matrix

“Preserved” below describes existing framework mechanisms where implemented. Required future controls are explicitly marked pending. Neither wording promises resistance to arbitrary same-user native attacks when OS containment is absent.

| Surface | Strict contract | Explicit legacy contract |
|---|---|---|
| Host-minted identities and authenticated links | Required | Still required; no child-selected identity |
| Guard-before-handler on privileged Keld calls | Required | Still required; identical grants and denial semantics |
| Literal renderer channel grants and navigation generations | Separate required controls under #541/#552/#542; integration pending | Same requirements, with no legacy exemption; not claimed implemented by this amendment |
| Direct filesystem access outside role storage | OS-denied by qualified profile | Keld confinement guarantee forfeited |
| Direct network calls | OS-denied even if broker has a net grant | Keld confinement guarantee forfeited |
| Secrets, keychain/credentials, devices | Qualified OS restrictions | Keld confinement guarantee forfeited; ordinary OS/privacy controls may still deny |
| Host/sibling process control or memory/token access | Qualified hostile-process denial | No Keld guarantee against ambient same-user native access |
| Native syscalls and code loading | Qualified profile plus independent addon policy | No syscall-containment claim; addon/host-plugin approval rules unchanged |
| Inherited privileged handles | Strict allowlist and hostile proof | Existing handle hygiene retained; no new handles granted and no full containment claim |
| Descendants | Equally confined; no escape under qualified contract | Existing enrolled cleanup retained; detached hostile descendants are not claimed contained |
| Crash/restart and generation revocation | Required independently of containment | Same supervisor/guardian mechanisms; qualification remains per tested path |
| Renderer sandbox/CSP/media/navigation policy | Separate security boundary | Unchanged and reported separately |
| Package identity and signature verification | Required for relevant path | Still required; not a claim that ambient native code cannot tamper with user-writable assets |
| Updater authority and state protection | Existing independently qualified rules | No new authority; existing legacy same-user direct-update refusal remains |
| Zero ambient OS authority | Claim permitted only after exact strict qualification | Explicitly forfeited |

Legacy does not grant administrator/root privileges, remove operating-system permissions, turn off macOS privacy controls, or ensure every direct operation succeeds. It removes Keld's strict containment claim for the selected role.

The material application risk is that compromised Bun/dependency/native code can exercise ambient user privileges without taking a Keld broker path. Consequently, a narrow manifest cannot be advertised as a complete restriction on such code in legacy mode.

Canonical required line:

```text
legacy — zero-authority claim forfeited
```

Supporting disclosure:

```text
Keld-mediated privileged calls remain authenticated and permission-checked.
The Bun role is not admitted under Keld's strict OS-containment profile.
```

Do not turn the forfeit into a blanket “security disabled” message. Do not call legacy sandboxed because the renderer is sandboxed. [S1][S13]

### 4.7 Reporting contract

Use the existing typed request/effective-state owners. Reports consume their result; they do not infer it from app identity or clone a state machine.

| Surface | Required behavior | Current availability |
|---|---|---|
| Runtime/dev launch | Source location, requested profile, actual admitted state, forfeiture for legacy | #588 must add real path |
| Doctor text | Validated declaration and whether runtime admission was observed; never claim strict from config alone | Live command, new finding needed |
| Doctor JSON and MCP | Keep existing top-level array of `DoctorFinding`; add security-profile finding, use existing typed error envelope | Existing shape must remain compatible |
| Build | Declare request; attach qualified effective state only when supported by exact artifact/platform evidence; print legacy forfeiture | Reserved today; acceptance remains pending until real build exists |
| Release metadata | Bind declaration/permissions digest, platform/artifact, effective state and matching receipt; renderer separate | KEL-78 T6 integration remains open |
| Compatibility scoreboard | Use `Scoreboard::authority_profile()` and `AuthorityProfile::as_str()`; print legacy forfeiture beside legacy result | Existing scorer, product qualification pending |

Doctor's profile finding is labelled `security_profile`. A valid static Legacy declaration can report `ok: true` for configuration validity only, with detail that includes the forfeiture and “runtime admission not performed.” A strict declaration without matching admission/proof must not produce an overall “strict verified” finding. When proof is unavailable, the finding reports failure to verify with the existing actionable error structure. With no declaration, the static finding says that no profile was selected, a default run would be unverified, and runtime admission was not performed. It does not invent an observed runtime state or silently select legacy.

Do not change the JSON array into a wrapper object or make MCP use a different parser. Renderer sandbox facts are a separate finding, and unknown renderer status stays unknown.

For product evidence, the run receipt binds the selected declaration/policy snapshot, actual effective state, role generation, artifact/runtime identity, platform/engine, and behavioral evidence. A strict receipt additionally binds the existing strict ProofIdentity: artifact digest, OS ProfileDigest, OS, archive identity, and policy generation/freshness or its matching immutable archive reference. The permissions digest never substitutes for this containment proof. Use the existing corpus receipt owner; do not create a competing universal ledger or expose link tokens. The KEL-74 record schema stays `keld.compat.evidence/v1`.

**Unresolved collection dependency:** GH #587 owns the product receipt format and trusted collection boundary. Application stdout/stderr is forwarded into host diagnostics, so an application can print a receipt-shaped line there. Such a line is not authoritative host evidence. The #587 consumer must distinguish an actual host-origin admitted result from forged application output and bind it to the exact role/runtime/artifact. Until that contract and reader exist, do not introduce an unused private receipt structure, label diagnostics as receipts, emit synthetic successful corpus records, or declare P1 receipt/P2 acceptance complete. This is a named dependency, not a waiver of the receipt criteria. [S23]

Scoring rules remain:

- Preserve a genuine unverified run as unverified.
- Require explicit declared-legacy receipt provenance for product `legacy_sandbox_off`.
- Reject record labels that disagree with the receipt.
- Do not aggregate mixed authority profiles into one percentage or complete board.
- A homogeneous unverified behavioral board can score under existing rules; it still proves no containment.
- The empty committed-product list currently prevents product percentages. A profile declaration does not register a corpus.
- Preserve the separately governed conformance-harness legacy label; it is not proof of a declared product launch.

A shared formatter test is not proof that the reserved build command works. Keep its existing error until the real build implementation consumes this contract. Never return success from a profile-only build stub. [S3][S9][S10][S11]

### 4.8 Migration and user acceptance

The hand-authored declaration is the operator's explicit selection. No second runtime “accept legacy” flag or token is introduced.

Automated migration must show the exact profile diff and the forfeiture before it writes `"profile": "legacy"`. Accepting file writes in general is not acceptance of weaker containment. Cancellation, no answer, unattended defaults, and compatibility detection alone produce no new legacy key. The exact interactive/noninteractive UX remains #497-owned, but any machine option must name legacy acceptance specifically.

Preserve existing explicit strict. A failed strict run may offer an explanation of the explicit legacy option, but cannot rewrite policy, retry unsandboxed, or automatically accept the choice. An existing valid explicit legacy declaration may be retained without a redundant prompt; any new authority change must be shown.

Removing legacy does not enable strict. It returns a fresh session to unverified/default behavior. Changing it to strict requires strict admission and its proof. Recompilation creates the corresponding descriptor version. Do not delete application data or reinterpret stored WebView profiles during this change.

A profile edit changes the exact permissions digest. Future frozen-permissions checks must detect it, including removal and semantically equivalent byte changes under their existing byte-identity policy.

The migration analyzer's static findings are not compatibility scores. The planned draw.io proof uses the reviewed hand-authored fixture first; completing the general migration command is not a prerequisite to testing the runtime seam.

### 4.9 Draw.io-specific consumers

The existing first-proof source receipt [S20] pins desktop commit `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e` and webapp commit `24b76c2cbd55d88e354042e8d329a2e4708972bc`. It selects a macOS “configuration plus one edit” route: replace the close-with-unsaved synchronous message box with the approved awaited asynchronous call inside the existing async handler. This is a scoped plan with remaining falsifiers, not a current Keld compatibility result. [S17]

#497/#499 consume:

- Exactly one legacy declaration in the permissions manifest.
- App entry `src/main/electron.js`, declared role environment `DRAWIO_DISABLE_UPDATE=true`, and separately owned single-instance/session facts.
- Literal IPC channel grants from the security/channel owners; no wildcard authority.
- One `electron` package identity resolving to `@keld/electron`, plus the inert updater adapter.
- Updated scripts that no longer launch Electron/electron-builder.

They do not invent `keld.build.ts`, `keld.compat.ts`, bunfig aliases, or `tsconfig paths.electron` for this first proof. The draw.io plan's rule not to seed filesystem grants under legacy is application migration policy; it does not change the guard's general filesystem grammar. [S18]

| Dependent owner | Required draw.io integration |
|---|---|
| [#452](https://github.com/gyldlab/keld/issues/452) | Session facts, legacy paths and declared role environment |
| [#453](https://github.com/gyldlab/keld/issues/453) | Single-instance declaration and host-owned lock |
| [#446](https://github.com/gyldlab/keld/issues/446) | Facade Ready and app-owned first window; compose its schema-2 mode with explicit profile as schema 4 under §4.4 |
| [#491](https://github.com/gyldlab/keld/issues/491) | Inert updater adapter |
| [#541](https://github.com/gyldlab/keld/issues/541), [#552](https://github.com/gyldlab/keld/issues/552), [#542](https://github.com/gyldlab/keld/issues/542) | Permission amendment, exact channel enforcement and host-owned navigation generations; implementation/integration pending |
| [#497](https://github.com/gyldlab/keld/issues/497), [#499](https://github.com/gyldlab/keld/issues/499) | Output/key-owner contract and hand-authored fixture |
| [#587](https://github.com/gyldlab/keld/issues/587), [#500](https://github.com/gyldlab/keld/issues/500), [#589](https://github.com/gyldlab/keld/issues/589) | Product receipts, corpus cells and real-Mac workflow evidence |

The independent legacy-primary fixture may proceed before the renderer bridge exists. Exposing draw.io's bridge must wait for its separately owned channel and generation controls; an unchanged test suite cannot prove missing functionality.

Remaining observed integration risks include sender-URL matching against the application's expected file origin, synchronous screen/theme/locale/window getters, and an experiment that did not itself validate real Electron 44 equivalence. Profile selection does not solve these.

Doctor currently assumes `src/main.ts` for project layout. The draw.io entry is different. Route a focused configured-entry fix through the migration/config owner; a green profile finding must not hide that failure. [S10][S17]

## 5. Boundaries

The approved design below is the implementation boundary. Publishing this specification does not implement it; #588 still requires an integrated specification base, a winning implementation claim, and the independent gates below.

| Area | Expected files/owners |
|---|---|
| Profile parse/accessor and parser tests | `crates/keld-guard/src/lib.rs`, existing JSONC/unique-key tests |
| Strict-state reuse | `crates/keld-guard/src/admit.rs`; preserve strict checks |
| Verified policy lifetime | `crates/keld-guard/src/verified_manifest.rs`, `crates/keld-core/src/app_session.rs`; no second loader |
| Boot version and all-entry preflight | `crates/keld-core/src/app_session/boot_contract.rs`; owning host/boot tests |
| macOS descriptor producer | `crates/keld-cli/src/boot.rs`; reuse exact-byte capture |
| Launch integration | Existing macOS app-session and supervisor/guardian owners; no second reaper |
| Doctor and shared disclosure | `crates/keld-cli/src/doctor.rs`; existing JSON/MCP/error contracts |
| Receipt/scoreboard integration | Existing KEL-74/GH-532 corpus receipt/report owners |
| Governing docs | KEL-78 amendment, relevant Architecture 03/06 clauses, key-owner/reporting references |

Do not change public kipc frames, per-channel grant grammar, renderer generation ownership, updater authority, native-addon policy, persistent WebView profiles, role declaration grammar, or OS sandbox mechanisms. The descriptor version is the sole intentional boot-contract change.

No new production unsafe operation or dependency is required by this design. If implementation reveals one is required, it needs its own explicit review rather than being smuggled into profile parsing.

## 6. Implementation PR sequence

| Order | Owner | Vertical deliverable | Exit observable |
|---|---|---|---|
| P0 | #586 / KEL-78, with KEL-96/KEL-102/KEL-74 review | Exact-content spec; single location; unambiguous version/mode/profile matrix; stale unverified wording reconciled | Approved spec and consistent governing references |
| P1 | #588 | Parser, boot version, verified host selection, macOS primary receipt, and mandatory runtime/dev plus doctor disclosure on all already-live applicable surfaces, landed together | Real legacy launch prints forfeiture before app execution; old-host, invalid-input, strict-refusal and no-inference controls pass |
| P2 | #588 with KEL-78 T6/KEL-74 consumers | Downstream receipt-to-scoreboard integration and its report tests | Correct receipt binding and forfeiture; no relabelling/mixed aggregation; existing unverified scoring preserved |
| P3 | #497/#499/#587/#500 owners | Hand-authored draw.io artifact, accepted declaration, receipt contract and initially honest corpus cells | Exact fixture reproducible; unimplemented/unknown cells remain visible |
| P4 | #589 | Actual macOS activation and workflow evidence; corpus registration only with qualifying evidence | Per-cell evidence under explicit legacy; unresolved install remains unknown |
| Later independent integration | Build/release and X02 migration owners | Real build/release disclosure and migration acceptance UX | Actual commands consume contract; no stub accepted as completion |

P1 is atomic across producer, consumer, and disclosure: do not ship parser acceptance while the runtime silently ignores the request, or an active legacy mode before its required visible forfeiture exists. P1 and P2 may be separate reviewed PRs, but the macOS runtime portion of #588 is not complete until both land and the live required surfaces pass. Neither #588 nor KEL-78 T6 may claim its original full build/reporting acceptance complete while build remains reserved. Any approved milestone split must be recorded explicitly on the owning tracker; a runtime milestone is not silent closure of the deferred command requirement.

A coherent launch/admission/disclosure draft can be prepared and tested while #587 is pending, but it remains draft/do-not-merge and cannot satisfy full P1 or P2. The renderer/profile proof does not qualify schema 4: facade/profile activation additionally needs the accepted GH #446 consumer and the cross-combination tests in §4.4. No parser-only, receipt-shaped log, or placeholder collector completes either boundary.

The profile-dependent macOS activation work need not wait for macOS strict T2c or the complete build/migrate product. It must wait for honest explicit legacy admission. Full first-proof completion cannot claim an install pass from a development stage; KEL-141's qualifying prebuilt artifact remains the install dependency. [S19]

## 7. Parsing, admission, and negative-control tests

Failure-first means the intended observable fails on the pre-feature implementation. A negative control is a deliberate, narrowly scoped defect that must make the corrected test fail again. A constant printed label, source inspection, or a mock claiming launch is not a native-runtime oracle.

### 7.1 Parser and compiled-input matrix

| ID | Input/condition | Required result | Deliberate negative control |
|---|---|---|---|
| P01 | Absent profile | None/Unverified, no legacy request | Default absence to Legacy |
| P02 | Valid legacy and strict strings | Exact distinct requested states | Swap or collapse enum mapping |
| P03 | Null, bool, number, array, object | Typed parse rejection | Treat null as None or coerce type |
| P04 | Empty/case/whitespace/unknown strings | Rejection with accepted values and location | Trim/case-fold or accept unverified |
| P05 | Duplicate profile, same/conflicting values | Rejection | Last/first value wins |
| P06 | Escaped-equivalent duplicate keys | Rejection | Compare only raw key spelling |
| P07 | One escaped key/value decoding to accepted spelling | Valid, exact-byte digest retained | Inconsistent second profile parser |
| P08 | Misplaced security profile | Location error, no alternate authority source | Accept nested/app-config fallback |
| P09 | Malformed JSONC, unterminated comment, oversized bytes, invalid UTF-8 | Existing specific boundary failures | Bypass common parser/ceiling |
| P10 | Profile changed after staged digest | Integrity failure before app resources | Reopen by path or hash different bytes |
| P11 | Profile-bearing 3/4 on a schema-1-only reader and a Ready-only 1/2 reader | Unsupported-version refusal before application marker/PID; distinguish actual old artifact from a 1/2 contract model | Downgrade 3→1 or 4→2 automatically |
| P12 | All four §4.4 rows, both explicit values, wrong presence, forbidden renderer/auth, duplicate/non-object input, unknown version, and unsupported facade/profile rollout | Each supported pair selects its exact version/shape; all invalid or unsupported combinations refuse before resources | Independently swap the producer's mode mapping, omit the consumer presence check, accept a numeric range, or fabricate a renderer |
| P13 | Profile-free schema1 and, after GH #446 qualification, profile-free schema2 | Existing mode and semantics preserved; fresh profile removal maps 3→1 and 4→2 without changing window ownership | Require a new declaration unconditionally or turn profile removal into a mode change |
| P14 | Existing missing-file dev fallback vs unreadable file | Only ENOENT uses old all-deny path | Treat every read failure as absence |

Parser fixtures include unchanged unrelated manifest grants and unknown fields to prove the profile addition does not silently tighten or loosen unrelated grammar.

### 7.2 Runtime and security matrix

| ID | Scenario | Independent observable | Deliberate negative control |
|---|---|---|---|
| A01 | Explicit legacy on real macOS primary path | Host observed Legacy request; real bound child; admitted receipt and guarded call | Print legacy but omit actual admission wiring |
| A02 | No key with Electron sandbox false/appSandbox off | Unverified receipt, never declared legacy | Read Electron option as selection |
| A03 | App name, fuse, environment, CLI flag, failed import | No change to request/effective state | Add one hidden fallback branch |
| A04 | Strict missing/stale/mismatched/incomplete/wrong-layer proof; missing primitive | Existing exact typed refusal; no app execution and no unconfined replacement | Replace error with Legacy/Unverified start |
| A05 | Strict restart after proof becomes invalid | Request remains Strict; successor application marker absent | Reset request on recovery |
| A06 | Legacy denied broker operation plus allowed control | Denied handler side effect absent; permitted operation succeeds | Skip guard when Legacy |
| A07 | Forged/foreign/stale link and generation | Existing rejection; healthy next operation succeeds | Trust payload profile/principal or stale link |
| A08 | Source manifest changes while host is alive | Current generation/recovery use original digest and request | Live reread or mutable profile cache |
| A09 | Remove legacy, retire host, start fresh | No cached legacy receipt; correct newly compiled schema | Reuse prior session state |
| A10 | Change legacy to strict without proof | Strict refusal; no fallback | Treat prior legacy acceptance as permission |
| A11 | Diagnostic/alternate entry with explicit profile | Refusal before app execution | Drop permissions and call unprivileged spawn |
| A12 | Existing guardian/enrolled cleanup and generation revocation | Retained mechanism tests pass, no duplicate reaper | Bypass shared owner on legacy branch |
| A13 | Unsupported target/role combination, including a compiler with fixed fallback permissions | Actionable refusal before execution; explicit profile never discarded | Claim all platforms from macOS pass or replace declared policy with empty bytes |
| A14 | Direct safe temp-file/socket probe on a legacy fixture | Record actual ambient outcome separately from broker denial | Count a broker/shim deny as OS containment |
| A15 | Renderer channels/generations and native-addon/updater policies | Existing regressions preserved; #541/#552/#542 integration controls required before exposing the draw.io bridge | Treat a pending control as passed, or skip it for legacy |

A14 runs only against disposable test-owned files, local endpoints, and child processes. It establishes disclosure accuracy, not that every ambient operation must succeed. Do not read real secrets or infer containment from an environmental denial.

Strict variants retain the full current negative matrix: artifact, profile digest, OS, archive identity, freshness, catalog completeness, and OS-oracle layer. A JS shim, guard denial, cleanup success, or resource-limit hit cannot satisfy strict OS evidence.

**Test ownership and OS scope:** strict proof permutations and recovery request retention use the existing pure admission/state-preparer tests with synthetic proof tuples; these make no OS-containment claim. Real macOS tests prove that an invalid strict attempt does not execute application code or start an unconfined replacement. A successful native Strict launch followed by proof invalidation/restart is a platform-owner acceptance test after that platform has qualified Strict. In particular, #588 does not wait for macOS T2c to run its pure A05 regression and real refusal controls. Real explicit-legacy activation, guard side effects, immutable policy, and enrolled cleanup remain required for the macOS slice.

### 7.3 Reporting, evidence, and migration matrix

| ID | Scenario | Required result | Deliberate negative control |
|---|---|---|---|
| R01 | Legacy runtime/doctor/reporter | Canonical forfeit present | Remove formatter call |
| R02 | Strict configuration without admission | Requested strict; admission not performed/not verified; no effective runtime state asserted | Use raw config as effective state or invent an Unverified run |
| R03 | Doctor JSON/MCP | Same finding array and actionable errors | New wrapper or divergent parser |
| R04 | Unverified product receipt | Record preserved as unverified | Drop run or label legacy/strict |
| R05 | Legacy product record without explicit admitted receipt, including an application-printed receipt lookalike | Rejected by the GH #587 trusted collector/reader | Trust record's self-label or a line from forwarded app output |
| R06 | Homogeneous unverified board | Existing behavior and label retained | Blanket score exclusion or promotion |
| R07 | Mixed authority board | No percentage, not complete, no single profile | Select first record's label |
| R08 | Reserved build today | Existing typed failure | Return success from formatter stub |
| R09 | Future real build/release | Correct declaration and exact evidence-qualified state | Attach another artifact's proof |
| M01 | Analyze/default/cancelled migration | No new legacy declaration | Infer acceptance from app detection |
| M02 | Generic write permission | Does not alone authorize legacy selection | Treat --write as security acceptance |
| M03 | Accepted legacy diff | Exactly one root declaration; no new grants | Add duplicate source or wildcard |
| M04 | Existing explicit strict | Preserved or change separately accepted | Auto-rewrite after strict failure |
| M05 | Configured draw.io entry | Honest doctor result for configured path | Hardcode src/main.ts |
| M06 | Dev-stage launch used for install cell | No install pass | Relabel dev activation as installation |
| M07 | Product corpus not registered/denominator incomplete | No product percentage | Register or complete by declaration alone |

Use table-driven parser tests, real temporary-file/digest integration, isolated real macOS process tests, and existing shared corpus/scorer tests. Timing tests use existing event/deadline observers, not sleeps. Prove denied side effects and process absence independently of log text. Replay each mutation against its named test and retain the observed red result.

No product/native tests or mutation experiments were run while drafting this specification. The tables are the required implementation acceptance plan, not executed results.

## 8. Review gates triggered

| Gate | Applies? | Required review |
|---|---|---|
| Permission model | Yes | KEL-78 owner; explicit forfeiture, no new grant, no fallback |
| Public API | Yes | Manifest grammar/accessor, reporting, compatibility behavior |
| Wire/compiled protocol | Yes, private boot descriptor | KEL-96/KEL-102 exact 1/2/3/4 composition, producer/consumer and version-skew refusal |
| Unsafe | None expected | Reuse current platform mechanisms; new unsafe is separately gated |
| Dependency | None expected | Reuse current parser/runtime/scorer |

The specification and implementation PRs each require named independent security/architecture evidence on their exact final diff. The delegated design approval is recorded in the header and section 10; it does not replace these gates or constitute native proof. The specification PR is explicitly do-not-merge.

## 9. Performance impact

One optional field decode, one version/declaration check, and one immutable selected-state value belong on the cold startup path. The existing exact-byte permissions digest is reused. No per-message profile parsing, manifest read, additional runtime, or new reaper is permitted.

No speedup or numeric overhead is claimed. If the integration materially changes launch allocation or process setup, compare the existing attributed macOS startup path using the repository's established budget rules. Strict and legacy behavior must be semantically correct before comparing timings.

## 10. Open questions, approved decisions, and handoff

### 10.1 Prior approval and R1 decisions

Under the repository owner's continuing 2026-10-23 delegation, this revision approves the following target after independent review of the corrected technical content. The new R1 approval, not the old eba60d22 approval, covers the schema correction and the added public admission entry:

1. Root `keld.permissions.jsonc.profile` is the sole authoring location.
2. Only strict/legacy are configurable; absence remains unverified/default and never implies legacy.
3. Schema 2 remains GH #446's no-profile facade contract. Explicit profiles use 3 for renderer boots and 4 for facade boots; profile-free renderer inputs remain 1. Unsupported combinations refuse (§4.4).
4. Strict admission and all independent guard/identity/lifecycle/renderer/native/updater owners remain intact.
5. Legacy explicitly forfeits OS containment and is qualified first for the macOS primary role.
6. KEL-74's current unverified evidence/scoring contract is preserved; stale #586/#588 exclusions are superseded.
7. The staged PR sequence and test matrix govern implementation; build/migrate integration is reported pending until those commands are real.

No further user choice is requested. KEL-78 remains the profile owner; GH #446 remains the facade Ready owner. The independent review found and resolved the earlier literal-API obligations that contradicted `admit_uncontained`; AC6/AC7 and §4.5 now state the same routing rule. The reviewed content digest and new delegated approval are published on #1120/KEL-78, with the matrix correction handed to #446/#588. Each implementation PR still requires independent review of its exact final public contract, permission boundary and compiled-boot seam. Design approval neither changes the explicit no-merge instruction nor supplies any implementation or OS evidence.

### 10.2 Verified blockers and first completion checks

| Blocker | Why it matters | Owner and next action | First observable completion |
|---|---|---|---|
| #586 integration pending | Approved design is not yet integrated into main | KEL-78/#586 publish the exact reviewed amendment and reconcile old briefs; preserve do-not-merge | Repository PR and consistent tracker decision receipts; main integration remains separately gated |
| Key currently ignored | Merely editing permissions does not select Legacy/Strict | #588 extend existing parser and actual host preflight atomically | Explicit declaration changes real admitted receipt |
| Old host ignores profile | New strict request could be ignored on unsupported runtime | #588 plus KEL-96/KEL-102 implement 3/4 pairing without repurposing Ready's 2 | Previous host refuses new artifact before app execution |
| Competing schema-2 meanings | Renderer/profile and facade/no-profile cannot share a closed version | #586 corrects its own assignment; #446 keeps Ready ownership; independent review covers §4.4 | Reviewed exact four-row matrix and consistent producer/consumer tests |
| Product receipt collector absent | Forwarded application output can forge a diagnostic receipt lookalike | #587 defines trusted host-origin collection and exact artifact binding, then #588 consumes it | Forged-app-output negative control fails while a real bound-host receipt succeeds |
| Generic admit is not a live proof | Returning Legacy alone proves no launch/auth/guard behavior | #588 wire existing host/session path | Real macOS bound child plus permitted/denied calls |
| macOS strict T2c unqualified | Explicit strict cannot be promised for draw.io today | Existing KEL-78 strict owner continues signed-helper hostile proof separately | Exact artifact/profile archive passes strict checks |
| Build/migrate reserved | Their future output cannot be claimed complete now | Existing command owners consume this contract; use hand-authored fixture first | Real commands execute and report honestly |
| Draw.io dependency graph still open | Profile alone does not provide IPC/window/preload/dialog behavior or pending renderer controls | #499/#589 plus #541/#552/#542 and API owners | Actual Mac workflow with denied-channel/stale-generation controls |
| Install/product denominator incomplete | Development launch cannot establish installed/product compatibility | #589/KEL-141 and corpus owners | Qualifying install evidence and committed denominator |
| Doctor entry assumption | A valid profile does not fix layout validation for draw.io | #497/config owner | Configured src/main/electron.js passes intended layout check |

The specification and delegated decisions are published through the #586 PR and KEL-78/#588 handoffs. Runtime implementation, native qualification, reserved-command integration, and merging remain open. Issue comments and PR records carry execution status; this document defines the approved target contract.

### 10.3 Evidence ledger

All source links below are pinned to the inspected commit. Main advanced from `442f3ebc` during research; the direct comparison to `3503a8aa383427dcf50ccbf6653c4298e7fb8aad` changed only Windows foreign-app admission tests and the KEL-254 spec. The profile/parser/reporting findings are unchanged.

[S1]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/kel78-strict-profile-sandbox.md
[S2]: https://linear.app/gyldlab-keld/issue/KEL-78/spec-strict-profile-os-sandbox-for-bun-roles-and-native-addon-workers
[S3]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/gh532-first-proof-evidence-rules.md
[S4]: https://github.com/gyldlab/keld/issues/586
[S5]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/lib.rs
[S6]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/admit.rs
[S7]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/boot.rs
[S8]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session.rs
[S9]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/verb.rs
[S10]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/doctor.rs
[S11]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/engineering/compat-scoreboard.md
[S12]: https://github.com/gyldlab/keld/pull/638
[S13]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/architecture/03-security.md
[S14]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/verified_manifest.rs
[S15]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session/boot_contract.rs
[S16]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session.rs#L1846-L1888
[S17]: https://github.com/gyldlab/keld/issues/420#issuecomment-6034980014
[S18]: https://github.com/gyldlab/keld/issues/497#issuecomment-6026217139
[S19]: https://github.com/gyldlab/keld/issues/589
[S20]: https://github.com/gyldlab/keld/issues/420#issuecomment-6022061800
[S21]: https://github.com/gyldlab/keld/blob/bb43e474577297f0792740676994aa153e5df1dd/docs/specs/gh446-facade-boot-ready.md
[S22]: https://github.com/gyldlab/keld/compare/3503a8aa383427dcf50ccbf6653c4298e7fb8aad...f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f
[S23]: https://github.com/gyldlab/keld/issues/587#issuecomment-6371500644

Additional owning links: [GH #588](https://github.com/gyldlab/keld/issues/588), [GH #499](https://github.com/gyldlab/keld/issues/499), [GH #587](https://github.com/gyldlab/keld/issues/587), [GH #500](https://github.com/gyldlab/keld/issues/500), [approved vocabulary PR #612](https://github.com/gyldlab/keld/pull/612), [direct-update legacy refusal](https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/architecture/06-runtime-and-tooling.md#L1098-L1103).

Decision history: KEL-78 owner comment `99767a07-b061-497d-89c1-2381032bad95`, 2026-10-07, selects unverified evidence option (b). The tracker returned no comment-specific URL; the KEL-78 issue link and the approved merged spec provide the verifiable references.
