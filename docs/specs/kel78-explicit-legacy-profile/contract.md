# Contract and execution model

Part of: [index](index.md)
Owns: contract and execution model.

## 2. Spec refs
Original section scope: Spec references and authority.

### Original metadata and approval provenance
Original status: approved — R1 owner-delegated compiled-boot composition correction; implementation pending

Linear: KEL-78 · Existing owner: GYLDLAB · Specification: GH #586 · Implementation: GH #588

Original update: 2026-10-24 · Source baseline: `3503a8aa383427dcf50ccbf6653c4298e7fb8aad`

Approval provenance: on 2026-10-23 the repository owner explicitly delegated the remaining decisions and
publication of the reviewed specification to the issue agent. This amendment records that new delegated design
decision under KEL-78; the earlier Windows T3 and unverified-vocabulary approvals did not approve these new
details. The exact committed content and independent review evidence are recorded on KEL-78 and the
specification PR. This approval does not waive implementation tests, native qualification, independent review
gates, or the explicit do-not-merge instruction.

R1 revision: the previous approval binds `eba60d2215a7fa2543b3673e20cd90fa7dbde555`, not these revised bytes.
Cross-contract reconciliation against approved Ready PR #657 at `bb43e474577297f0792740676994aa153e5df1dd`
found that both specifications assigned incompatible meanings to schema 2. R1 preserves Ready's version and
separates the profile-bearing combinations in §4.4. The narrow uncontained-admission and receipt-dependency
clarifications preserve the decisions already recorded on KEL-78. Under the continuing owner delegation, R1's
technical content was independently reviewed through the permission-model, public-API and compiled-contract
lenses with no surviving blocker; its exact committed bytes and review receipts are recorded on #1120/KEL-78.
This is a new design approval, not inherited approval of changed bytes, runtime implementation or native
proof. Current main `f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f` still implements only schema 1. The PR remains
draft and do-not-merge. [S21][S22]

### Governing references
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
are unverified must not override later platform-specific evidence; the specific macOS T2c strict qualification
remains open. [S1][S13]



## 3. Acceptance criteria
Original section scope: Acceptance criteria.

Each criterion has a falsifier in section 7.


### AC-1: Original criterion 1
- **Given:** Supply that field without the root manifest /profile.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**One source:** only the decoded root property `/profile` in `keld.permissions.jsonc` selects a requested profile. Its accepted values are exactly `"strict"` and `"legacy"`.

- **Negative control:**
  - **Single fault:** Accept a profile from an alternate configuration field.
  - **At-risk precondition:** Supply that field without the root manifest /profile.
  - **Expected RED:** The no-request parser assertion fails.

### AC-2: Original criterion 2
- **Given:** Parse a manifest whose decoded profile is null.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Presence matters:** an absent property means no declared request and follows the existing unverified/default path. Null, wrong types, other strings, duplicate decoded keys, or invalid JSONC are errors.

- **Negative control:**
  - **Single fault:** Treat an explicit null profile as absent.
  - **At-risk precondition:** Parse a manifest whose decoded profile is null.
  - **Expected RED:** The invalid-profile rejection assertion fails.

### AC-3: Original criterion 3
- **Given:** Set the option with no explicit root profile.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**No inference:** Electron sandbox options, fuses, package/app names, platform, environment, CLI flags, import failures, missing primitives, and failed strict proofs never activate legacy.

- **Negative control:**
  - **Single fault:** Infer legacy from an Electron sandbox option.
  - **At-risk precondition:** Set the option with no explicit root profile.
  - **Expected RED:** The no-inference admission assertion fails.

### AC-4: Original criterion 4
- **Given:** Replace the manifest after the retained digest-verified snapshot.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Verified consumption:** the host derives its request from the same retained, digest-verified manifest used by broker authorization. It never reopens a path or trusts a child-supplied profile.

- **Negative control:**
  - **Single fault:** Reopen the policy path after verification.
  - **At-risk precondition:** Replace the manifest after the retained digest-verified snapshot.
  - **Expected RED:** The snapshot-binding test observes a changed request and fails.

### AC-5: Original criterion 5
- **Given:** Present that combination to a supporting host before resource creation.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Version admission:** a profile-bearing manifest requires schema 3 for a renderer-declared boot or schema 4
for a facade boot. Schemas 1 and 2 retain their respective renderer/facade meanings without an explicit
profile. A supporting host refuses declaration/version or closed-shape mismatches before resources; a reader
that lacks profile support refuses 3 and 4 rather than ignoring their declarations (§4.4).

- **Negative control:**
  - **Single fault:** Accept a profile-bearing schema-2 artifact.
  - **At-risk precondition:** Present that combination to a supporting host before resource creation.
  - **Expected RED:** The closed version/profile matrix rejection fails.

### AC-6: Original criterion 6
- **Given:** Exercise the qualified macOS launch with permitted and denied calls.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Real legacy launch:** on the qualified macOS primary-role path, explicit legacy reaches the Guard-owned requested-state admission dispatcher through `admit_uncontained(Legacy)` (§4.5); common launch validation, authenticated links, guard checks, and enrolled lifecycle cleanup still run. Qualified containment paths retain the full `admit()` API.

- **Negative control:**
  - **Single fault:** Return a Legacy receipt without launching the child.
  - **At-risk precondition:** Exercise the qualified macOS launch with permitted and denied calls.
  - **Expected RED:** The real bound-child and guard-call evidence fails.

### AC-7: Original criterion 7
- **Given:** Use an explicit Strict request with missing proof or known-uncontained primary.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Strict unchanged:** a qualified candidate uses the existing full strict admission checks. Missing, stale,
incomplete, mismatched, or wrong-layer proof, missing primitive, or unexpected grant refuses launch. The
known-uncontained macOS primary path instead calls `admit_uncontained(Strict)` and refuses with
`KELD-GUARD008` before execution, without fabricated proof inputs (§4.5). No unconfined replacement starts.

- **Negative control:**
  - **Single fault:** Downgrade a rejected Strict request to an unconfined launch.
  - **At-risk precondition:** Use an explicit Strict request with missing proof or known-uncontained primary.
  - **Expected RED:** The no-child strict-refusal assertion fails.

### AC-8: Original criterion 8
- **Given:** Change the manifest between successive generations of one host session.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Recovery frozen:** the selected request and policy snapshot remain fixed for a host session. Every successor generation uses them. Strict proof/admission is rechecked for each candidate generation.

- **Negative control:**
  - **Single fault:** Recapture profile policy during recovery.
  - **At-risk precondition:** Change the manifest between successive generations of one host session.
  - **Expected RED:** The retained snapshot assertion fails.

### AC-9: Original criterion 9
- **Given:** Remove the declaration without retiring old links and the host.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Fresh transitions:** removing legacy takes effect only after the old host session and links are retired and a fresh host captures new policy. Without successful strict admission, the new run is unverified. Setting strict requires the normal strict proof.

- **Negative control:**
  - **Single fault:** Apply legacy removal to a still-live host session.
  - **At-risk precondition:** Remove the declaration without retiring old links and the host.
  - **Expected RED:** The fresh-session transition assertion fails.

### AC-10: Original criterion 10
- **Given:** Trigger profile admission rejection.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Truthful evidence:** product legacy receipts map to `legacy_sandbox_off`; strict primary receipts map to `strict_bun`; unverified receipts map to `unverified`. Rejected launches cannot emit successful admission or workflow evidence.

- **Negative control:**
  - **Single fault:** Emit a successful admission receipt for a rejected launch.
  - **At-risk precondition:** Trigger profile admission rejection.
  - **Expected RED:** The rejected-launch evidence assertion fails.

### AC-11: Original criterion 11
- **Given:** Admit a legacy primary and run the applicable reporting surface.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Truthful reporting:** legacy output includes the exact forfeiture; doctor separates a declared request from admitted state; scoreboard derives its label from the existing accessor. Renderer status remains separate.

- **Negative control:**
  - **Single fault:** Omit the exact legacy forfeiture from live reporting.
  - **At-risk precondition:** Admit a legacy primary and run the applicable reporting surface.
  - **Expected RED:** The disclosure assertion fails.

### AC-12: Original criterion 12
- **Given:** Accept a generic write without accepting the weaker-security choice.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Migration acceptance:** automated migration writes legacy only after acceptance specific to the visible weaker-security choice. Analysis, cancellation, default answers, and generic write permission do not select legacy.

- **Negative control:**
  - **Single fault:** Write legacy on generic migration acceptance.
  - **At-risk precondition:** Accept a generic write without accepting the weaker-security choice.
  - **Expected RED:** The migration declaration assertion fails.

### AC-13: Original criterion 13
- **Given:** Request qualification for a non-primary role without its own proof.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Scope preserved:** the first implementation qualifies only the macOS primary Bun role. Other roles and platforms cannot inherit a legacy qualification from it.

- **Negative control:**
  - **Single fault:** Reuse macOS primary qualification for another role.
  - **At-risk precondition:** Request qualification for a non-primary role without its own proof.
  - **Expected RED:** The role-scope assertion fails.

### AC-14: Original criterion 14
- **Given:** Launch through that alternate entry with explicit Strict.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**No alternate bypass:** diagnostic and alternate execution entry points cannot ignore an explicit profile and launch under a falsely declared state.

- **Negative control:**
  - **Single fault:** Ignore an explicit profile in a diagnostic entry point.
  - **At-risk precondition:** Launch through that alternate entry with explicit Strict.
  - **Expected RED:** The alternate-entry admission assertion fails.

### AC-15: Original criterion 15
- **Given:** Score the unchanged unverified corpus records.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Behavioral evidence retained:** unverified records and the approved homogeneous/mixed-board scoring rules remain unchanged.

- **Negative control:**
  - **Single fault:** Relabel existing unverified evidence as legacy.
  - **At-risk precondition:** Score the unchanged unverified corpus records.
  - **Expected RED:** The existing evidence-label regression fails.

### AC-16: Original criterion 16
- **Given:** Issue a denied-channel call from an admitted legacy role.
- **When:** Exercise the original criterion below and its section 7 oracle.
- **Then:** Preserve the following observable contract.

**Existing authority preserved:** no profile branch expands manifest grants, bypasses channel checks, relaxes stale-generation rejection, changes WebView hardening, or enables a forbidden updater/native-addon path.

- **Negative control:**
  - **Single fault:** Bypass a denied channel in the legacy branch.
  - **At-risk precondition:** Issue a denied-channel call from an admitted legacy role.
  - **Expected RED:** The preserved manifest-denial assertion fails.

## 4. Design: Contract and execution model
Original section scope: Design.

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

The existing 64 KiB manifest limit, JSONC comment handling, recursive decoded-key uniqueness, UTF-8 rules, and
unrelated unknown-key behavior remain intact. The document must be a JSON object; positional array
deserialization is not another manifest syntax. An implementation must distinguish missing from explicit null;
a naive optional-enum decoder that treats null as missing violates this contract. [S5][S14]

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

The request is application-session policy. The first admitted product receipt covers only macOS's primary Bun
role. A future separately declared role must consume the same request through its owning admission path, or
refuse an unsupported combination before execution. No per-role override syntax or mixed-profile application
is approved here. A primary receipt cannot qualify another role.

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

The current schema-1-only reader already rejects `schema != 1` with `KELD-CORE-035`. A Ready-only reader
supporting exactly 1/2 must likewise reject 3/4. Thus a profile-bearing facade cannot silently run under an
old Ready implementation that ignores unknown manifest keys. Tests must bind both reader capability sets, with
an actual previous artifact where available; a hypothetical 1/2 reader model is a compatibility contract test,
not an executed historical artifact. [S15][S21]

**Partial rollout:** #588 may prepare and qualify the renderer/profile row 3 before GH #446 implements facade
startup. That producer emits only supported rows and must refuse a requested facade/profile combination; it
must never manufacture a renderer or emit 3 for it. Neither 2 nor 4 is executable until the complete GH #446
Ready/window-ownership consumer exists. The later facade/profile integration emits and consumes 4 atomically
with profile preflight, mandatory disclosure and GH #446's unchanged Ready/recovery/custody rules. Reserving 4
is not permission to add an accepting stub or to claim draw.io activation. Removing a profile from a fresh,
fully supported facade session selects 2, not 1; window ownership must not change as a side effect of the
profile edit.

If GH #446's schema-2 implementation is integrated first, its producer and verified host preflight must
already recognize-and-refuse an explicit root security profile. It need not activate profiles or support 3/4,
but it cannot silently accept a declaration that it does not enforce. This is an integration prerequisite for
newly introduced schema-2 support, not a claim that old, unmodified schema-1 artifacts can be repaired
retroactively.

**Rejected alternatives:** repurpose 2 (conflicts with its approved closed shape); permit explicit profiles in
2 (a Ready-only old reader could ignore Strict); infer mode from field presence or try a different parser
after an error (changes ownership on malformed input); add a second manifest profile value to the descriptor
(two authority sources); use one extensible new union for every combination (unneeded rewrite of both closed
contracts). Versions 3/4 reuse the existing shapes with a deliberate compatibility barrier. There is no
automatic downgrade from 3/4 to 1/2, no declaration stripping, and no retry in another mode.

Migration and generated artifacts must pin a supporting compiler and host pair. New code cannot retroactively make an old compiler plus old host understand a future key. Hand-editing the key into such an old installation is unsupported. Build/release producers inherit this requirement.

Every execution entry point must use common profile preflight. In particular, `run_unprivileged` currently
discards the permissions descriptor for a diagnostic session. It must refuse explicit-profile versions 3/4 and
any explicit declaration presented through 1/2; its renderer-only diagnostic path also cannot execute facade
versions 2/4. For a supported profile-free diagnostic input, retain the same verified manifest integrity
check. The high-level `HostOwnedHelloSession` diagnostic entry used directly by CLI dev helpers must also
refuse an explicit project profile before creating a listener or child: use the existing bounded manifest
loader for a direct project input, and the retained digest-verifying loader when a compiled descriptor
supplies a digest. Never invent verified provenance for a direct input without one. Missing-file behavior
stays with the existing entry's contract; malformed/unreadable policy is not absence.

Valid profile-free diagnostic execution remains separately labelled unverified. Shared preflight intentionally
moves malformed or digest-mismatched diagnostic policy from an ignored input to a refusal. The compiler also
parses captured manifest bytes to choose the schema, so malformed input may fail during compilation instead of
later host startup; tests must preserve refusal while documenting this earlier error phase. [S16]

This is a narrowly scoped compiled-contract version amendment requiring KEL-96/KEL-102 review. It is not a public kipc frame change.

### 4.5 Runtime admission and policy lifetime
1. Validate the selected artifact, package/stage identity, and boot descriptor through the existing owners.
2. Load the permissions through the existing retained-handle, digest-verifying loader. Parse the same bytes once.
3. Check boot version against declaration presence. Reject invalid or unsupported combinations before creating an application child, listener, or window.
4. Freeze the requested state in the existing host-session policy snapshot. Retain the permissions digest as its identity.
5. For explicit Legacy on the known-uncontained macOS primary path, call `admit_uncontained(Legacy)`, which shares the existing requested-state dispatcher; full candidate/proof paths keep `admit()`. Do not fabricate proof identities to call the full API, and do not treat either pure state result as proof of a successful launch.
6. For Strict on a qualified containment path, gather the actual candidate artifact/platform/profile/proof
inputs and call the unchanged full `admit()` checks. On the known-uncontained macOS primary path, call
`admit_uncontained(Strict)` and refuse with the existing missing-primitive error before execution. Either
failure ends the attempt; there is no unsandboxed retry or fabricated proof identity.
7. Emit the canonical legacy forfeiture on the real launch path before releasing application execution,
explicitly marked as a requested profile until admission succeeds. Use the existing role/supervisor path for
enrollment, link authentication, guard-before-handler dispatch, and generation revocation. Only after the
actual required launch/bind events may the host expose an admitted receipt.
8. On recovery, retain the same requested profile and permissions snapshot. Revalidate candidate-specific strict admission and proof freshness. Never reread user-editable configuration to downgrade a running strict session.
9. To change profile or grants, retire the full host session and links, then capture new policy in a fresh host. A renderer reload or Bun restart alone cannot switch policy.

Separate digests: the permissions content digest identifies configuration bytes; the strict proof's profile digest identifies its OS containment policy; the artifact digest identifies the executable under proof. One is not a substitute for another.

**Known-uncontained primary path:** the current macOS launcher has no qualified Strict containment or complete
candidate proof identity. It must not manufacture artifact/profile digests merely to call the full proof API.
A narrow Guard-owned `admit_uncontained(requested: ProfileState) -> Result<ProfileState, AdmissionError>` may
share the existing requested-state dispatcher and return only Unverified or Legacy. Strict must fail with the
existing missing-primitive error (`KELD-GUARD008`); if the primitive check ever stops rejecting, absent proof
must still fail with `KELD-GUARD011`, never admit Strict. This entry describes a positively known uncontained
path, not a fallback after `admit()` fails. The full qualified-path `admit()` and its ordered proof checks
remain unchanged. Recovery reuses the frozen request through the same admission owner. This adds no successful
Strict mode and supplies no runtime or product receipt by itself.

No declaration must not disable an existing Windows/Linux containment mechanism. The existing default runtime behavior remains intact. The new declared-profile reporting path maps omission to Unverified; it never invents a strict claim from platform choice. Existing separately qualified strict platform fixtures and their evidence are not relabelled by this amendment.

The initial explicit-legacy implementation is qualified on macOS only. Unsupported target/role combinations
fail before app execution with an actionable unsupported-profile diagnostic. A non-macOS compiler must not
discard an explicit project profile by staging its historical fixed empty-policy fallback; it must detect and
reject unsupported explicit-profile input. Existing Windows/Linux strict mechanisms stay with their owners and
are not removed to make legacy start.

Existing strict failures remain `KELD-GUARD008` (primitive unavailable), `009` (unexpected grant), `010`
(host-handle leak), `011` (proof missing), `012` (proof stale), `013` (identity mismatch), `014` (incomplete
catalog), and `015` (wrong evidence layer/oracle). Use the existing error types and fixes; do not invent
replacement runtime codes. Manifest syntax/type errors remain in the existing parser error category, and
boot/version inconsistency uses the boot-contract category. [S6]

[S13]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/architecture/03-security.md
[S14]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/verified_manifest.rs
[S15]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session/boot_contract.rs
[S16]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session.rs#L1846-L1888
[S1]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/kel78-strict-profile-sandbox.md
[S21]: https://github.com/gyldlab/keld/blob/bb43e474577297f0792740676994aa153e5df1dd/docs/specs/gh446-facade-boot-ready.md
[S22]: https://github.com/gyldlab/keld/compare/3503a8aa383427dcf50ccbf6653c4298e7fb8aad...f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f
[S5]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/lib.rs
[S6]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/admit.rs
