# Spec: Explicit legacy compatibility profile: design

Part of: [index](index.md)
Owns: Profile design, versioning, runtime admission and evidence

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

Reserve the security-profile property at the known alternate policy locations `/app/profile`, `/windows/<window>/profile`, and `/roles/<role>/profile` so an apparent per-role override is not silently accepted. Do not recursively prohibit unrelated authentication-provider or persistent-WebView profile concepts. Window and role identifiers named
“profile” are not themselves overrides.

The boot producer rejects a literal root security-profile declaration in its supported app-config syntax with guidance to move it to the permissions manifest. It must reuse the existing root-property scanning owner rather than execute TypeScript or add line-prefix security parsing. Dynamic/unsupported config syntax cannot become another profile source.

The existing 64 KiB manifest limit, JSONC comment handling, recursive decoded-key uniqueness, UTF-8 rules, and unrelated unknown-key behavior remain intact. The document must be a JSON object; positional array deserialization is not another manifest syntax. An implementation must distinguish missing from explicit null; a naive optional-enum
decoder that treats null as missing violates this contract. [S5][S14]

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

The request is application-session policy. The first admitted product receipt covers only macOS's primary Bun role. A future separately declared role must consume the same request through its owning admission path, or refuse an unsupported combination before execution. No per-role override syntax or mixed-profile application is approved here.
A primary receipt cannot qualify another role.

### 4.4 Boot-version compatibility and downgrade prevention

This is a required part of #588. Older hosts ignore unknown manifest properties, including `profile`. The manifest key alone is therefore insufficient to make those hosts respect a strict request. [S5]

Use the existing `keld.boot.json` version discriminator. Window ownership and the explicit security request are independent inputs. Ready's approved schema 2 is already a closed, windowless facade shape; it must not acquire the renderer-bearing meaning from the earlier revision of this amendment. Preserve it and assign the two profile-bearing
combinations distinct versions. [S21]

| Window ownership selected by the existing config owner | Verified root profile | Exact compiled version | Exact closed shape |
|---|---|---|---|
| Renderer-declared | Absent | 1 | Existing `schema`, `name`, `entry`, `renderer`, `permissions`, optional `auth` |
| Facade (`initialWindow: "none"`, GH #446) | Absent | 2 | GH #446's `schema`, `name`, `entry`, `permissions`; no `renderer` or `auth` |
| Renderer-declared | Valid strict or legacy | 3 | Schema 1's fields and validation, including optional `auth`, with `schema: 3` |
| Facade (`initialWindow: "none"`, GH #446) | Valid strict or legacy | 4 | Schema 2's fields and validation, with `schema: 4`; no `renderer` or `auth` |

No descriptor carries a second `profile` value. The existing verified permissions bytes remain the sole selection source. The compiler selects one exact row from the captured configuration and permissions; it does not execute TypeScript, infer facade mode from a missing renderer, or infer a security request from the version alone. A malformed
declaration fails before version selection.

Supporting readers retain the exact parsed version through verified-policy preflight and enforce all of these predicates before creating an application child, listener, or window:

- Versions 1/2 require absence of a root profile; an explicit declaration is a version mismatch, not an ignored extension.
- Versions 3/4 require a valid explicit declaration; absence is inconsistent compiled input.
- Versions 1/3 require a renderer and keep its existing validation; versions 2/4 forbid the renderer field even when null. The optional auth carrier remains available only in 1/3. Unknown or duplicate decoded fields, non-object documents, invalid UTF-8, and oversized input remain errors.
- Dispatch uses an exact supported-version set, not a numeric range or `schema >= 3`. A future version cannot enter a current profile or Ready branch by ordering alone.
- A recognized shape is not proof that its platform, window mode or role is implemented. Refuse unsupported combinations before resources. Reuse the boot-schema error for unsupported versions/mismatches and the owning unsupported-platform error for a valid but unavailable mode.

The current schema-1-only reader already rejects `schema != 1` with `KELD-CORE-035`. A Ready-only reader supporting exactly 1/2 must likewise reject 3/4. Thus a profile-bearing facade cannot silently run under an old Ready implementation that ignores unknown manifest keys. Tests must bind both reader capability sets, with an actual previous
artifact where available; a hypothetical 1/2 reader model is a compatibility contract test, not an executed historical artifact. [S15][S21]

**Partial rollout:** #588 may prepare and qualify the renderer/profile row 3 before GH #446 implements facade startup. That producer emits only supported rows and must refuse a requested facade/profile combination; it must never manufacture a renderer or emit 3 for it. Neither 2 nor 4 is executable until the complete GH #446
Ready/window-ownership consumer exists. The later facade/profile integration emits and consumes 4 atomically with profile preflight, mandatory disclosure and GH #446's unchanged Ready/recovery/custody rules. Reserving 4 is not permission to add an accepting stub or to claim draw.io activation. Removing a profile from a fresh, fully supported
facade session selects 2, not 1; window ownership must not change as a side effect of the profile edit.

If GH #446's schema-2 implementation is integrated first, its producer and verified host preflight must already recognize-and-refuse an explicit root security profile. It need not activate profiles or support 3/4, but it cannot silently accept a declaration that it does not enforce. This is an integration prerequisite for newly introduced
schema-2 support, not a claim that old, unmodified schema-1 artifacts can be repaired retroactively.

**Rejected alternatives:** repurpose 2 (conflicts with its approved closed shape); permit explicit profiles in 2 (a Ready-only old reader could ignore Strict); infer mode from field presence or try a different parser after an error (changes ownership on malformed input); add a second manifest profile value to the descriptor (two authority
sources); use one extensible new union for every combination (unneeded rewrite of both closed contracts). Versions 3/4 reuse the existing shapes with a deliberate compatibility barrier. There is no automatic downgrade from 3/4 to 1/2, no declaration stripping, and no retry in another mode.

Migration and generated artifacts must pin a supporting compiler and host pair. New code cannot retroactively make an old compiler plus old host understand a future key. Hand-editing the key into such an old installation is unsupported. Build/release producers inherit this requirement.

Every execution entry point must use common profile preflight. In particular, `run_unprivileged` currently discards the permissions descriptor for a diagnostic session. It must refuse explicit-profile versions 3/4 and any explicit declaration presented through 1/2; its renderer-only diagnostic path also cannot execute facade versions 2/4. For
a supported profile-free diagnostic input, retain the same verified manifest integrity check. The high-level `HostOwnedHelloSession` diagnostic entry used directly by CLI dev helpers must also refuse an explicit project profile before creating a listener or child: use the existing bounded manifest loader for a direct project input, and the
retained digest-verifying loader when a compiled descriptor supplies a digest. Never invent verified provenance for a direct input without one. Missing-file behavior stays with the existing entry's contract; malformed/unreadable policy is not absence.

Valid profile-free diagnostic execution remains separately labelled unverified. Shared preflight intentionally moves malformed or digest-mismatched diagnostic policy from an ignored input to a refusal. The compiler also parses captured manifest bytes to choose the schema, so malformed input may fail during compilation instead of later host
startup; tests must preserve refusal while documenting this earlier error phase. [S16]

This is a narrowly scoped compiled-contract version amendment requiring KEL-96/KEL-102 review. It is not a public kipc frame change.

### 4.5 Runtime admission and policy lifetime

1. Validate the selected artifact, package/stage identity, and boot descriptor through the existing owners.
2. Load the permissions through the existing retained-handle, digest-verifying loader. Parse the same bytes once.
3. Check boot version against declaration presence. Reject invalid or unsupported combinations before creating an application child, listener, or window.
4. Freeze the requested state in the existing host-session policy snapshot. Retain the permissions digest as its identity.
5. For explicit Legacy on the known-uncontained macOS primary path, call `admit_uncontained(Legacy)`, which shares the existing requested-state dispatcher; full candidate/proof paths keep `admit()`. Do not fabricate proof identities to call the full API, and do not treat either pure state result as proof of a successful launch.
6. For Strict on a qualified containment path, gather the actual candidate artifact/platform/profile/proof inputs and call the unchanged full `admit()` checks. On the known-uncontained macOS primary path, call `admit_uncontained(Strict)` and refuse with the existing missing-primitive error before execution. Either failure ends the attempt;
   there is no unsandboxed retry or fabricated proof identity.
7. Emit the canonical legacy forfeiture on the real launch path before releasing application execution, explicitly marked as a requested profile until admission succeeds. Use the existing role/supervisor path for enrollment, link authentication, guard-before-handler dispatch, and generation revocation. Only after the actual required
   launch/bind events may the host expose an admitted receipt.
8. On recovery, retain the same requested profile and permissions snapshot. Revalidate candidate-specific strict admission and proof freshness. Never reread user-editable configuration to downgrade a running strict session.
9. To change profile or grants, retire the full host session and links, then capture new policy in a fresh host. A renderer reload or Bun restart alone cannot switch policy.

Separate digests: the permissions content digest identifies configuration bytes; the strict proof's profile digest identifies its OS containment policy; the artifact digest identifies the executable under proof. One is not a substitute for another.

**Known-uncontained primary path:** the current macOS launcher has no qualified Strict containment or complete candidate proof identity. It must not manufacture artifact/profile digests merely to call the full proof API. A narrow Guard-owned `admit_uncontained(requested: ProfileState) -> Result<ProfileState, AdmissionError>` may share the
existing requested-state dispatcher and return only Unverified or Legacy. Strict must fail with the existing missing-primitive error (`KELD-GUARD008`); if the primitive check ever stops rejecting, absent proof must still fail with `KELD-GUARD011`, never admit Strict. This entry describes a positively known uncontained path, not a fallback
after `admit()` fails. The full qualified-path `admit()` and its ordered proof checks remain unchanged. Recovery reuses the frozen request through the same admission owner. This adds no successful Strict mode and supplies no runtime or product receipt by itself.

No declaration must not disable an existing Windows/Linux containment mechanism. The existing default runtime behavior remains intact. The new declared-profile reporting path maps omission to Unverified; it never invents a strict claim from platform choice. Existing separately qualified strict platform fixtures and their evidence are not relabelled by this amendment.

The initial explicit-legacy implementation is qualified on macOS only. Unsupported target/role combinations fail before app execution with an actionable unsupported-profile diagnostic. A non-macOS compiler must not discard an explicit project profile by staging its historical fixed empty-policy fallback; it must detect and reject unsupported
explicit-profile input. Existing Windows/Linux strict mechanisms stay with their owners and are not removed to make legacy start.

Existing strict failures remain `KELD-GUARD008` (primitive unavailable), `009` (unexpected grant), `010` (host-handle leak), `011` (proof missing), `012` (proof stale), `013` (identity mismatch), `014` (incomplete catalog), and `015` (wrong evidence layer/oracle). Use the existing error types and fixes; do not invent replacement runtime codes.
Manifest syntax/type errors remain in the existing parser error category, and boot/version inconsistency uses the boot-contract category. [S6]

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

Doctor's profile finding is labelled `security_profile`. A valid static Legacy declaration can report `ok: true` for configuration validity only, with detail that includes the forfeiture and “runtime admission not performed.” A strict declaration without matching admission/proof must not produce an overall “strict verified” finding. When proof
is unavailable, the finding reports failure to verify with the existing actionable error structure. With no declaration, the static finding says that no profile was selected, a default run would be unverified, and runtime admission was not performed. It does not invent an observed runtime state or silently select legacy.

Do not change the JSON array into a wrapper object or make MCP use a different parser. Renderer sandbox facts are a separate finding, and unknown renderer status stays unknown.

For product evidence, the run receipt binds the selected declaration/policy snapshot, actual effective state, role generation, artifact/runtime identity, platform/engine, and behavioral evidence. A strict receipt additionally binds the existing strict ProofIdentity: artifact digest, OS ProfileDigest, OS, archive identity, and policy
generation/freshness or its matching immutable archive reference. The permissions digest never substitutes for this containment proof. Use the existing corpus receipt owner; do not create a competing universal ledger or expose link tokens. The KEL-74 record schema stays `keld.compat.evidence/v1`.

**Unresolved collection dependency:** GH #587 owns the product receipt format and trusted collection boundary. Application stdout/stderr is forwarded into host diagnostics, so an application can print a receipt-shaped line there. Such a line is not authoritative host evidence. The #587 consumer must distinguish an actual host-origin admitted
result from forged application output and bind it to the exact role/runtime/artifact. Until that contract and reader exist, do not introduce an unused private receipt structure, label diagnostics as receipts, emit synthetic successful corpus records, or declare P1 receipt/P2 acceptance complete. This is a named dependency, not a waiver of the receipt criteria. [S23]

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

Automated migration must show the exact profile diff and the forfeiture before it writes `"profile": "legacy"`. Accepting file writes in general is not acceptance of weaker containment. Cancellation, no answer, unattended defaults, and compatibility detection alone produce no new legacy key. The exact interactive/noninteractive UX remains
#497-owned, but any machine option must name legacy acceptance specifically.

Preserve existing explicit strict. A failed strict run may offer an explanation of the explicit legacy option, but cannot rewrite policy, retry unsandboxed, or automatically accept the choice. An existing valid explicit legacy declaration may be retained without a redundant prompt; any new authority change must be shown.

Removing legacy does not enable strict. It returns a fresh session to unverified/default behavior. Changing it to strict requires strict admission and its proof. Recompilation creates the corresponding descriptor version. Do not delete application data or reinterpret stored WebView profiles during this change.

A profile edit changes the exact permissions digest. Future frozen-permissions checks must detect it, including removal and semantically equivalent byte changes under their existing byte-identity policy.

The migration analyzer's static findings are not compatibility scores. The planned draw.io proof uses the reviewed hand-authored fixture first; completing the general migration command is not a prerequisite to testing the runtime seam.

### 4.9 Draw.io-specific consumers

The existing first-proof source receipt [S20] pins desktop commit `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e` and webapp commit `24b76c2cbd55d88e354042e8d329a2e4708972bc`. It selects a macOS “configuration plus one edit” route: replace the close-with-unsaved synchronous message box with the approved awaited asynchronous call inside the existing
async handler. This is a scoped plan with remaining falsifiers, not a current Keld compatibility result. [S17]

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
