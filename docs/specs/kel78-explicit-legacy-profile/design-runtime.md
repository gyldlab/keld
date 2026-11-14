# Legacy runtime and reporting
Part of: [index](index.md)
Owns: Admission, evidence, migration and source

## 4. Design: Runtime admission and reporting
### 4.5 Runtime admission and policy lifetime

1. Validate the selected artifact, package/stage identity, and boot descriptor through the existing owners.
2. Load the permissions through the existing retained-handle, digest-verifying loader. Parse the same bytes once.
3. Check boot version against declaration presence. Reject invalid or unsupported combinations before creating an application child, listener, or window.
4. Freeze the requested state in the existing host-session policy snapshot. Retain the permissions digest as its identity.
5. For explicit Legacy on the known-uncontained macOS primary path, call `admit_uncontained(Legacy)`, which shares the existing requested-state dispatcher; full candidate/proof paths keep `admit()`. Do not fabricate proof identities to call the full API, and do not treat either pure state result as proof of a successful launch.
6. 6. For Strict on a qualified containment path, gather the actual candidate artifact/platform/profile/proof inputs and call the unchanged full
   `admit()` checks. On the known-uncontained macOS primary path, call `admit_uncontained(Strict)` and refuse with the existing missing-primitive
   error before execution. Either failure ends the attempt; there is no unsandboxed retry or fabricated proof identity.
7. 7. Emit the canonical legacy forfeiture on the real launch path before releasing application execution, explicitly marked as a requested
   profile until admission succeeds. Use the existing role/supervisor path for enrollment, link authentication, guard-before-handler dispatch,
   and generation revocation. Only after the actual required launch/bind events may the host expose an admitted receipt.
8. On recovery, retain the same requested profile and permissions snapshot. Revalidate candidate-specific strict admission and proof freshness. Never reread user-editable configuration to downgrade a running strict session.
9. To change profile or grants, retire the full host session and links, then capture new policy in a fresh host. A renderer reload or Bun restart alone cannot switch policy.

Separate digests: the permissions content digest identifies configuration bytes; the strict proof's profile digest identifies its OS containment policy; the artifact digest identifies the executable under proof. One is not a substitute for another.

**Known-uncontained primary path:** the current macOS launcher has no qualified Strict containment or complete candidate proof identity. It must
  not manufacture artifact/profile digests merely to call the full proof API. A narrow Guard-owned
  `admit_uncontained(requested: ProfileState) -> Result<ProfileState, AdmissionError>` may share the existing requested-state dispatcher and return only Unverified or Legacy. Strict must fail
  with the existing missing-primitive error (`KELD-GUARD008`); if the primitive check ever stops rejecting, absent proof must still fail with
  `KELD-GUARD011`, never admit Strict. This entry describes a positively known uncontained path, not a fallback after `admit()` fails. The full
  qualified-path `admit()` and its ordered proof checks remain unchanged. Recovery reuses the frozen request through the same admission owner.
  This adds no successful Strict mode and supplies no runtime or product receipt by itself.

No declaration must not disable an existing Windows/Linux containment mechanism. The existing default runtime behavior remains intact. The new
  declared-profile reporting path maps omission to Unverified; it never invents a strict claim from platform choice. Existing separately
  qualified strict platform fixtures and their evidence are not relabelled by this amendment.

The initial explicit-legacy implementation is qualified on macOS only. Unsupported target/role combinations fail before app execution with an
  actionable unsupported-profile diagnostic. A non-macOS compiler must not discard an explicit project profile by staging its historical fixed
  empty-policy fallback; it must detect and reject unsupported explicit-profile input. Existing Windows/Linux strict mechanisms stay with their
  owners and are not removed to make legacy start.

Existing strict failures remain `KELD-GUARD008` (primitive unavailable), `009` (unexpected grant), `010` (host-handle leak), `011` (proof
  missing), `012` (proof stale), `013` (identity mismatch), `014` (incomplete catalog), and `015` (wrong evidence layer/oracle). Use the existing
  error types and fixes; do not invent replacement runtime codes. Manifest syntax/type errors remain in the existing parser error category, and
  boot/version inconsistency uses the boot-contract category. [S6]

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

Doctor's profile finding is labelled `security_profile`. A valid static Legacy declaration can report `ok: true` for configuration validity only,
  with detail that includes the forfeiture and “runtime admission not performed.” A strict declaration without matching admission/proof must not
  produce an overall “strict verified” finding. When proof is unavailable, the finding reports failure to verify with the existing actionable
  error structure. With no declaration, the static finding says that no profile was selected, a default run would be unverified, and runtime
  admission was not performed. It does not invent an observed runtime state or silently select legacy.

Do not change the JSON array into a wrapper object or make MCP use a different parser. Renderer sandbox facts are a separate finding, and unknown renderer status stays unknown.

For product evidence, the run receipt binds the selected declaration/policy snapshot, actual effective state, role generation, artifact/runtime
  identity, platform/engine, and behavioral evidence. A strict receipt additionally binds the existing strict ProofIdentity: artifact digest, OS
  ProfileDigest, OS, archive identity, and policy generation/freshness or its matching immutable archive reference. The permissions digest never
  substitutes for this containment proof. Use the existing corpus receipt owner; do not create a competing universal ledger or expose link
  tokens. The KEL-74 record schema stays `keld.compat.evidence/v1`.

**Unresolved collection dependency:** GH #587 owns the product receipt format and trusted collection boundary. Application stdout/stderr is
  forwarded into host diagnostics, so an application can print a receipt-shaped line there. Such a line is not authoritative host evidence. The
  #587 consumer must distinguish an actual host-origin admitted result from forged application output and bind it to the exact
  role/runtime/artifact. Until that contract and reader exist, do not introduce an unused private receipt structure, label diagnostics as
  receipts, emit synthetic successful corpus records, or declare P1 receipt/P2 acceptance complete. This is a named dependency, not a waiver of
  the receipt criteria. [S23]

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

Automated migration must show the exact profile diff and the forfeiture before it writes `"profile": "legacy"`. Accepting file writes in general
  is not acceptance of weaker containment. Cancellation, no answer, unattended defaults, and compatibility detection alone produce no new legacy
  key. The exact interactive/noninteractive UX remains #497-owned, but any machine option must name legacy acceptance specifically.

Preserve existing explicit strict. A failed strict run may offer an explanation of the explicit legacy option, but cannot rewrite policy, retry unsandboxed, or automatically accept the choice. An existing valid explicit legacy declaration may be retained without a redundant prompt; any new authority change must be shown.

Removing legacy does not enable strict. It returns a fresh session to unverified/default behavior. Changing it to strict requires strict admission and its proof. Recompilation creates the corresponding descriptor version. Do not delete application data or reinterpret stored WebView profiles during this change.

A profile edit changes the exact permissions digest. Future frozen-permissions checks must detect it, including removal and semantically equivalent byte changes under their existing byte-identity policy.

The migration analyzer's static findings are not compatibility scores. The planned draw.io proof uses the reviewed hand-authored fixture first; completing the general migration command is not a prerequisite to testing the runtime seam.

### 4.9 Draw.io-specific consumers

The existing first-proof source receipt [S20] pins desktop commit `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e` and webapp commit
  `24b76c2cbd55d88e354042e8d329a2e4708972bc`. It selects a macOS “configuration plus one edit” route: replace the close-with-unsaved synchronous
  message box with the approved awaited asynchronous call inside the existing async handler. This is a scoped plan with remaining falsifiers, not
  a current Keld compatibility result. [S17]

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


[S1]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/kel78-strict-profile-sandbox.md
[S3]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/gh532-first-proof-evidence-rules.md
[S6]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/admit.rs
[S9]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/verb.rs
[S10]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/doctor.rs
[S11]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/engineering/compat-scoreboard.md
[S13]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/architecture/03-security.md
[S17]: https://github.com/gyldlab/keld/issues/420#issuecomment-6034980014
[S18]: https://github.com/gyldlab/keld/issues/497#issuecomment-6026217139
[S20]: https://github.com/gyldlab/keld/issues/420#issuecomment-6022061800
[S23]: https://github.com/gyldlab/keld/issues/587#issuecomment-6371500644
