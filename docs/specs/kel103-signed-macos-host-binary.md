# Spec: signed macOS prebuilt host binary (T1)

Status: draft
Linear: KEL-103 · Owner: GYLDLAB · Updated: 2026-08-21

Written against `origin/main` `39cac108774e455127e0c57163feae54450e7f5b`.
Implementation MUST NOT start until a human changes this spec to `Status: approved`.

## 1. Goal & non-goals

Prove Unique #1 with one falsifiable release slice: macOS CI builds the existing
arm64 `keld-host` target from a recorded commit, signs the raw Mach-O with the approved
Keld **Developer ID Application** identity, verifies the exact team and identity, and
publishes the verified binary as a commit-addressed CI artifact. A consumer downloads
that artifact and runs `keld-host --hello` without invoking Cargo, rustc, or rustup.
T1 proves only a signed CI artifact and a no-compiler consumer path; it does **not**
make today's `keld hello` or a future `@keld/cli` resolve the artifact.

Non-goals:

- DMG, `.app`, PKG, AppImage, NSIS, MSI, deb, rpm, or any installer/bundle authoring.
- Windows `signtool`, Windows signing, Linux signers, or Linux packagers.
- A `keld-update` feed, update manifest, delta, relaunch helper, rollback, or Electron
  `autoUpdater` facade (KEL-53 owns updater work).
- A `keld build --target` farm, universal binary, macOS x86_64 artifact, or any
  cross-platform release matrix.
- App Store submission, Gatekeeper distribution acceptance, notarization, stapling,
  or a notary ticket.
- KEL-78 App Sandbox or Hardened Runtime entitlement selection, separately signed Bun
  helpers, native-addon workers, or a strict-profile claim.
- Implementing `crates/keld-pack`, extending `Format::App`, adding signing code to a
  Rust crate, or adding signer dependencies to workspace `Cargo.toml`.
- Implementing the `@keld/cli` downloader/optional-dependency path or changing
  `keld hello` to launch the artifact.
- Changing `keld-host` without `--hello`; it remains the current pre-alpha banner.
- Inventing a fifth Keld unique or making an unfalsifiable “best in the world” claim.

## 2. Spec refs

- Root `AGENTS.md`: four uniques, first-principles/reuse, spec gate, YAGNI, review
  gates, CI shared-file review, and no implementation from a draft.
- `docs/architecture/01-overview.md` §§1–2, especially principle 5: the host is
  prebuilt per platform and app developers do not compile it. §3 identifies
  `keld-host` as the shipping executable and `keld-pack` as packaging destination.
- `docs/architecture/06-runtime-and-tooling.md` §3: `.app`/DMG, notarization,
  `rcodesign`, other platforms, and cross-target assembly are **destination prose**.
  They are cited, not treated as live or pulled into T1.
- `docs/architecture/03-security.md` §4.4 and §5: signed-host/update supply-chain
  destination. T1 does not implement update security or its keys/feed.
- `docs/engineering/decisions.md` §1: prebuilt host is Unique #1; there are four
  uniques only.
- `crates/keld-host/src/main.rs` and `crates/keld-host/Cargo.toml`: live executable;
  `--hello` opens the window slice and no flag prints the banner.
- `crates/keld-pack/src/lib.rs`: `Format` enum only; no authoring or signer exists.
- `docs/specs/kel75-principalized-bun-child-roles.md`: role generations, bootstrap,
  grants, and virtual ports remain KEL-75 work.
- `docs/specs/kel78-strict-profile-sandbox.md`: T2 App Sandbox entitlements and
  separately signed helpers remain KEL-78 work and are not implied by a signed host.
- Apple [Certificates overview](https://developer.apple.com/help/account/certificates/certificates-overview/):
  Developer ID Application signs Mac applications distributed outside the Mac App
  Store; the identity contains the team name and Team ID.
- Apple [Signing your apps for Gatekeeper](https://developer.apple.com/developer-id/):
  Developer ID signing and notarization are separate stages.
- Apple [TN2206](https://developer.apple.com/library/archive/technotes/tn2206/_index.html):
  `codesign --verify --strict` is the named native verification primitive; Gatekeeper
  assessment applies to the packaged product and is not claimed by this raw-binary T1.

This spec narrows the implementation order without changing architecture destination
prose. No architecture file is amended by this draft.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given Keld commit `${GITHUB_SHA}` on a real macOS runner, when the release lane runs
   `cargo build --release --locked -p keld-host --target aarch64-apple-darwin`, signs
   `target/aarch64-apple-darwin/release/keld-host` with
   `Developer ID Application: ${KELD_APPLE_TEAM_NAME} (${KELD_APPLE_TEAM_ID})`, and
   executes the named verification command below, then every command exits zero, the
   exact `Authority` and `TeamIdentifier` lines match, and only that verified binary is
   eligible for upload as `keld-host-macos-arm64-${GITHUB_SHA}`:

   ```bash
   artifact=target/aarch64-apple-darwin/release/keld-host
   /usr/bin/codesign --verify --strict --verbose=2 "$artifact"
   /usr/bin/codesign --display --verbose=4 "$artifact" 2>codesign.txt
   /usr/bin/grep -Fx \
     "Authority=Developer ID Application: ${KELD_APPLE_TEAM_NAME} (${KELD_APPLE_TEAM_ID})" \
     codesign.txt
   /usr/bin/grep -Fx "TeamIdentifier=${KELD_APPLE_TEAM_ID}" codesign.txt
   ```

2. Given the uploaded artifact from the same recorded commit, when a consumer downloads
   it on macOS and runs
   `PATH=/usr/bin:/bin:/usr/sbin:/sbin ./keld-host --hello --title KEL-103-T1`, then a
   real window with title `KEL-103-T1` opens and closes normally, and the consumer path
   invokes no `cargo`, `rustc`, or `rustup`. This proves a consumer can use the CI
   artifact without compiling the host. It does not claim that today's `keld hello` or
   `@keld/cli` downloads it.

3. Given the configured Developer ID Application certificate is absent, has no matching
   private key, or does not match the configured team, when the signing preflight runs,
   then signing and upload do not run, the job exits nonzero, and stderr starts with
   `KELD-PACK-001: Developer ID Application identity '<expected identity>' with its private key is unavailable.`
   The same message states the fix: import the approved Keld Developer ID Application
   certificate/private key into the ephemeral CI keychain, set the matching team name
   and Team ID, and retry. This code is proposed here; it is not landed by this draft.

4. Given an unsigned, invalid, ad-hoc-signed, or wrong-team `keld-host`, when the named
   verifier runs, then it exits nonzero, upload does not run, and stderr starts with
   `KELD-PACK-002: macOS host signature is missing, invalid, ad-hoc, or not from '<expected identity>'.`
   The message states the fix: sign the unchanged release binary with the approved
   Developer ID Application identity and rerun verification. Unsigned “success” is
   forbidden. This code is proposed here; it is not landed by this draft.

5. Given a successful T1 lane, when its commands and artifact manifest are inspected,
   then there is no `.app`, DMG, PKG, installer format, notarization/stapling command,
   `keld-pack` invocation, updater artifact, or other-OS signer. The uploaded executable
   is byte-for-byte the file that passed acceptance criterion 1.

## 4. Design

### First-principles and reuse decision

- **Ownership and trust:** the macOS release job owns the build/sign/verify/upload
  sequence. The Keld Apple Developer Program team owns the Developer ID certificate and
  private key. The raw private key exists only in an ephemeral CI keychain and is never
  an artifact or log value. `keld-host` remains the runtime authority root; its process,
  handle, principal, and permission ownership do not change.
- **Lifecycle:** build first; import the approved identity into an ephemeral keychain;
  preflight that the certificate and private key are present; sign the final release
  binary; verify validity plus exact identity/team; upload only after all checks pass;
  destroy the keychain when the job ends. No step may mutate the binary after signing.
- **I/O and provenance:** input is the checked-out `${GITHUB_SHA}` plus the approved CI
  signing secret. Output is one arm64 Mach-O named with `${GITHUB_SHA}` and CI metadata
  that records the source SHA. No package or installer is assembled.
- **Failure:** identity/preflight failure is proposed `KELD-PACK-001`. Signature or
  identity verification failure is proposed `KELD-PACK-002`. Either blocks upload. A
  generic shell success or an ad-hoc signature cannot be converted into a green lane.
- **Existing option selected:** retain the current Cargo binary target and use Apple's
  platform signer on a macOS runner. This introduces no Rust dependency and directly
  exercises the platform signature that macOS will read.
- **Rejected rewrite:** implementing `keld-pack::Format::App` is unnecessary because
  the T1 artifact is a raw executable. `Format` currently has no authoring path, and
  app/DMG work would expand T1 into architecture 06 §3's installer destination.
- **Rejected signer:** `rcodesign` remains a destination candidate for later
  cross-platform packaging. Native `codesign` already meets this macOS-only T1, so a
  signer dependency has no named unmet requirement.
- **Rejected identity:** ad-hoc signing is a negative control only. It does not prove
  the approved Developer ID team/identity and therefore cannot satisfy Unique #1's
  prebuilt signed-host artifact contract.
- **Compatibility fallback:** none. A missing Developer ID identity fails closed; it
  does not upload an unsigned or ad-hoc artifact.
- **Performance:** no speed claim and no runtime rewrite. T1 records unsigned and signed
  byte sizes only to make signature overhead observable.

### Proposed implementation surface after approval

- A dedicated macOS release workflow owns the secret-bearing lane; it is not folded
  into unrelated PR CI.
- A small verifier command may live under `tools/` and must emit the two proposed typed
  errors. The signer remains Apple's `/usr/bin/codesign`; the repository does not
  implement a cryptographic signer.
- The identity contract is exactly
  `Developer ID Application: ${KELD_APPLE_TEAM_NAME} (${KELD_APPLE_TEAM_ID})`. The two
  values are not secrets, but the certificate export, password, and private key are.
- Artifact retention/publication beyond the CI artifact is T2+; T1 makes no GitHub
  Release, npm, CDN, or update-channel promise.

New/changed Rust or TypeScript types: none.

Capabilities or manifest changes: none.

Wire/protocol changes: none.

Platform notes: macOS arm64 only. Windows and Linux remain unverified for signed-host
distribution. No result from this lane may be generalized to another OS or architecture.

## 5. Boundaries

- This draft implements only `docs/specs/kel103-signed-macos-host-binary.md` and the
  separate private research note.
- A future approved T1 may implement in a dedicated `.github/workflows/*macos-host*`
  workflow, a narrowly scoped `tools/` verifier, and
  `docs/engineering/keld-error-codes.md` for `KELD-PACK-001`/`002`.
- A future T1 must not touch workspace `Cargo.toml`, `crates/keld-pack/**`,
  `keld-ipc`, `keld-guard`, the `keld-runtime` `RoleRegistry`, `crates/keld-update/**`,
  updater architecture, `docs/architecture/06-runtime-and-tooling.md`, Dependabot
  (`.github/dependabot.yml`), or product crate behavior.
- A future T1 must not add App Sandbox/Hardened Runtime entitlements, a Bun/helper
  signature, `@keld/cli` resolution, an installer, or another OS signer under KEL-103.
- No boundary change to runtime ownership, principal minting, permissions, kipc, or
  updater authority.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] **T1 — implement this approved spec:** one macOS arm64 release lane builds the
  existing host target, performs Developer ID Application preflight/sign/strict
  team verification, publishes the commit-addressed CI artifact, and proves a real
  downloaded `keld-host --hello` runs without a compiler. No installer or notarization.
- [ ] **T2+ — separate approved issue/spec:** decide whether the distribution product is
  `.app`, ZIP, DMG, or PKG, then add notarization/stapling and Gatekeeper testing for
  that chosen outer container. Architecture 06 §3 is destination evidence, not approval.
- [ ] **T2+ — separate approved issue/spec:** add macOS x86_64/universal or Windows/Linux
  signed-host lanes only when their consumer and signer contracts are named.
- [ ] **T2+ — separate approved issue/spec:** connect `@keld/cli` or another distribution
  resolver to immutable host artifacts. Until then T1 remains a CI-artifact proof.

This branch stops after the draft specification. No task above is implemented while
`Status: draft`.

## 7. Test plan

| Acceptance | Test | Independent oracle |
|---|---|---|
| 1 | `macos_host_signature_matches_approved_team` on the uploaded candidate | Apple `codesign --verify --strict` exit status plus exact `Authority` and `TeamIdentifier` output |
| 2 | `macos_prebuilt_host_hello_needs_no_rust` on a real Mac after artifact download | Real titled window and normal close while PATH cannot resolve Cargo/rustc/rustup; build log has no compiler invocation in the consumer job |
| 3 | `macos_host_missing_identity_is_pack_001` | Remove the certificate or matching private key from the ephemeral keychain; exact code/message; signing and upload steps remain unexecuted |
| 4 | `macos_host_unsigned_or_wrong_team_is_pack_002` | Verify the untouched unsigned Cargo output, then an ad-hoc-signed copy; both fail with `KELD-PACK-002` and cannot upload |
| 5 | `macos_t1_outputs_no_installer` | Workflow command log and artifact manifest contain only the raw host/provenance surface; forbidden extensions/commands are absent |

Negative controls are mandatory for criteria 3 and 4. A test that passes after the
signature is deleted or replaced with `codesign --sign -` is invalid.

The implementation must run on a real macOS runner. Linux inspection, a mocked
`codesign`, or a parser-only fixture cannot prove a macOS signature. Do not run
notarization in this T1. The window smoke waits for an observable window/process-ready
condition and uses a timeout only as a kill switch; it must not sleep for correctness.

## 8. Review gates triggered

- unsafe: none — no unsafe or runtime code is proposed.
- public API: **yes** — the artifact name, signing identity contract, and no-compiler
  consumer behavior become an externally consumable distribution surface; human
  approval is required.
- permission model: none — no manifest, entitlement, principal, or grant changes.
- dependency addition: none — Cargo manifests remain unchanged; Apple system tools are
  reused.
- wire protocol: none — kipc is untouched.

Additionally, the future workflow is a human-reviewed shared CI file under root
`AGENTS.md`. That is a repository-process gate, not a sixth Keld unique or wire/security
review gate. This docs-only draft does not add the workflow.

## 9. Perf impact

No runtime path changes and no performance claim. T1 records unsigned and signed host
byte sizes so code-signature overhead is visible. Installer-size budgets do not apply
because T1 creates no installer; any later container must measure against architecture
01 §5 in its own approved spec.

## 10. Open questions

1. **Developer ID Application vs ad-hoc:** approve or reject this spec's proposed
   Developer ID Application identity. Ad-hoc signing is rejected by the current draft
   because it has no approved team-identity proof.
2. **Exact team/identity:** what exact Apple legal team name and
   `KELD_APPLE_TEAM_ID` replace the named variables? The spec cannot become approved
   until these values are recorded.
3. **CI secret ownership:** which human/team owns certificate issuance, encrypted
   `.p12` storage, password access, ephemeral keychain setup/deletion, rotation, expiry,
   and revocation response? Approval must name the owner without placing secret values
   in this repository.
