# Legacy policy compilation
Part of: [index](index.md)
Owns: Manifest and boot version details

## 4. Design: Policy compilation
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

Reserve the security-profile property at the known alternate policy locations `/app/profile`, `/windows/<window>/profile`, and
  `/roles/<role>/profile` so an apparent per-role override is not silently accepted. Do not recursively prohibit unrelated
  authentication-provider or persistent-WebView profile concepts. Window and role identifiers named “profile” are not themselves overrides.

The boot producer rejects a literal root security-profile declaration in its supported app-config syntax with guidance to move it to the
  permissions manifest. It must reuse the existing root-property scanning owner rather than execute TypeScript or add line-prefix security
  parsing. Dynamic/unsupported config syntax cannot become another profile source.

The existing 64 KiB manifest limit, JSONC comment handling, recursive decoded-key uniqueness, UTF-8 rules, and unrelated unknown-key behavior
  remain intact. The document must be a JSON object; positional array deserialization is not another manifest syntax. An implementation must
  distinguish missing from explicit null; a naive optional-enum decoder that treats null as missing violates this contract. [S5][S14]

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

The request is application-session policy. The first admitted product receipt covers only macOS's primary Bun role. A future separately declared
  role must consume the same request through its owning admission path, or refuse an unsupported combination before execution. No per-role
  override syntax or mixed-profile application is approved here. A primary receipt cannot qualify another role.

### 4.4 Boot-version compatibility and downgrade prevention

This is a required part of #588. Older hosts ignore unknown manifest properties, including `profile`. The manifest key alone is therefore insufficient to make those hosts respect a strict request. [S5]

Use the existing `keld.boot.json` version discriminator. Window ownership and the explicit security request are independent inputs. Ready's
  approved schema 2 is already a closed, windowless facade shape; it must not acquire the renderer-bearing meaning from the earlier revision of
  this amendment. Preserve it and assign the two profile-bearing combinations distinct versions. [S21]

| Window ownership selected by the existing config owner | Verified root profile | Exact compiled version | Exact closed shape |
|---|---|---|---|
| Renderer-declared | Absent | 1 | Existing `schema`, `name`, `entry`, `renderer`, `permissions`, optional `auth` |
| Facade (`initialWindow: "none"`, GH #446) | Absent | 2 | GH #446's `schema`, `name`, `entry`, `permissions`; no `renderer` or `auth` |
| Renderer-declared | Valid strict or legacy | 3 | Schema 1's fields and validation, including optional `auth`, with `schema: 3` |
| Facade (`initialWindow: "none"`, GH #446) | Valid strict or legacy | 4 | Schema 2's fields and validation, with `schema: 4`; no `renderer` or `auth` |

No descriptor carries a second `profile` value. The existing verified permissions bytes remain the sole selection source. The compiler selects
  one exact row from the captured configuration and permissions; it does not execute TypeScript, infer facade mode from a missing renderer, or
  infer a security request from the version alone. A malformed declaration fails before version selection.

Supporting readers retain the exact parsed version through verified-policy preflight and enforce all of these predicates before creating an application child, listener, or window:

- Versions 1/2 require absence of a root profile; an explicit declaration is a version mismatch, not an ignored extension.
- Versions 3/4 require a valid explicit declaration; absence is inconsistent compiled input.
- Versions 1/3 require a renderer and keep its existing validation; versions 2/4 forbid the renderer field even when null. The optional auth carrier remains available only in 1/3. Unknown or duplicate decoded fields, non-object documents, invalid UTF-8, and oversized input remain errors.
- Dispatch uses an exact supported-version set, not a numeric range or `schema >= 3`. A future version cannot enter a current profile or Ready branch by ordering alone.
- A recognized shape is not proof that its platform, window mode or role is implemented. Refuse unsupported combinations before resources. Reuse the boot-schema error for unsupported versions/mismatches and the owning unsupported-platform error for a valid but unavailable mode.

The current schema-1-only reader already rejects `schema != 1` with `KELD-CORE-035`. A Ready-only reader supporting exactly 1/2 must likewise
  reject 3/4. Thus a profile-bearing facade cannot silently run under an old Ready implementation that ignores unknown manifest keys. Tests must
  bind both reader capability sets, with an actual previous artifact where available; a hypothetical 1/2 reader model is a compatibility contract
  test, not an executed historical artifact. [S15][S21]

**Partial rollout:** #588 may prepare and qualify the renderer/profile row 3 before GH #446 implements facade startup. That producer emits only
  supported rows and must refuse a requested facade/profile combination; it must never manufacture a renderer or emit 3 for it. Neither 2 nor 4
  is executable until the complete GH #446 Ready/window-ownership consumer exists. The later facade/profile integration emits and consumes 4
  atomically with profile preflight, mandatory disclosure and GH #446's unchanged Ready/recovery/custody rules. Reserving 4 is not permission to
  add an accepting stub or to claim draw.io activation. Removing a profile from a fresh, fully supported facade session selects 2, not 1; window
  ownership must not change as a side effect of the profile edit.

If GH #446's schema-2 implementation is integrated first, its producer and verified host preflight must already recognize-and-refuse an explicit
  root security profile. It need not activate profiles or support 3/4, but it cannot silently accept a declaration that it does not enforce. This
  is an integration prerequisite for newly introduced schema-2 support, not a claim that old, unmodified schema-1 artifacts can be repaired
  retroactively.

**Rejected alternatives:** repurpose 2 (conflicts with its approved closed shape); permit explicit profiles in 2 (a Ready-only old reader could
  ignore Strict); infer mode from field presence or try a different parser after an error (changes ownership on malformed input); add a second
  manifest profile value to the descriptor (two authority sources); use one extensible new union for every combination (unneeded rewrite of both
  closed contracts). Versions 3/4 reuse the existing shapes with a deliberate compatibility barrier. There is no automatic downgrade from 3/4 to
  1/2, no declaration stripping, and no retry in another mode.

Migration and generated artifacts must pin a supporting compiler and host pair. New code cannot retroactively make an old compiler plus old host understand a future key. Hand-editing the key into such an old installation is unsupported. Build/release producers inherit this requirement.

Every execution entry point must use common profile preflight. In particular, `run_unprivileged` currently discards the permissions descriptor
  for a diagnostic session. It must refuse explicit-profile versions 3/4 and any explicit declaration presented through 1/2; its renderer-only
  diagnostic path also cannot execute facade versions 2/4. For a supported profile-free diagnostic input, retain the same verified manifest
  integrity check. The high-level `HostOwnedHelloSession` diagnostic entry used directly by CLI dev helpers must also refuse an explicit project
  profile before creating a listener or child: use the existing bounded manifest loader for a direct project input, and the retained
  digest-verifying loader when a compiled descriptor supplies a digest. Never invent verified provenance for a direct input without one.
  Missing-file behavior stays with the existing entry's contract; malformed/unreadable policy is not absence.

Valid profile-free diagnostic execution remains separately labelled unverified. Shared preflight intentionally moves malformed or
  digest-mismatched diagnostic policy from an ignored input to a refusal. The compiler also parses captured manifest bytes to choose the schema,
  so malformed input may fail during compilation instead of later host startup; tests must preserve refusal while documenting this earlier error
  phase. [S16]

This is a narrowly scoped compiled-contract version amendment requiring KEL-96/KEL-102 review. It is not a public kipc frame change.


[S5]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/lib.rs
[S14]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/verified_manifest.rs
[S15]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session/boot_contract.rs
[S16]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session.rs#L1846-L1888
[S21]: https://github.com/gyldlab/keld/blob/bb43e474577297f0792740676994aa153e5df1dd/docs/specs/gh446-facade-boot-ready.md
