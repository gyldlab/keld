# GH-1404 packaging acceptance
Part of: [index](index.md)
Owns: section 3

## 3. Acceptance criteria

### AC-1: One framework path builds two configured macOS apps, including a minimal fixture and
- **Given:** The framework and two independently configured macOS app sources are available.
- **When:** The framework builds the minimal fixture and original draw.io with one common path.
- **Then:** One framework path builds two configured macOS apps, including a minimal fixture and original draw.io, without app-specific framework branches.
- **Negative control:**
  - **Single fault:** Add an app-specific framework branch.
  - **At-risk precondition:** Build both the minimal fixture and original draw.io.
  - **Expected RED:** The framework-independence check detects different implementation paths.

### AC-2: Missing/invalid metadata, Host, Bun, entry, permission file or resource, unknown conf
- **Given:** The build receives metadata, Host, Bun, entry, permission and resource inputs.
- **When:** Pack validates all declared inputs and target paths.
- **Then:** Missing/invalid metadata, Host, Bun, entry, permission file or resource, unknown config, wrong target and escaping paths fail before final output publication.
- **Negative control:**
  - **Single fault:** Allow one escaping or missing required input.
  - **At-risk precondition:** Pack receives a configured app and starts validation.
  - **Expected RED:** The typed refusal occurs before final output publication.

### AC-3: The copied .app launches through Launch Services with no terminal, checkout, Cargo wo
- **Given:** A copied .app is ready for first installation.
- **When:** The user launches the copied app through Launch Services.
- **Then:** The copied .app launches through Launch Services with no terminal, checkout, Cargo workspace, external Bun, dev lease, test flag or private test credential.
- **Negative control:**
  - **Single fault:** Launch from a checkout or external runtime.
  - **At-risk precondition:** The copied .app is launched through Launch Services.
  - **Expected RED:** The process-origin check detects a terminal, checkout, Cargo workspace, external Bun or dev lease.

### AC-4: Installed selection validates the enclosing bundle and exact boot/policy/runtime rela
- **Given:** The installed app has a selected bundle, boot, policy and runtime record.
- **When:** Host admission validates the bundle and its exact boot/policy/runtime relationship.
- **Then:** Installed selection validates the enclosing bundle and exact boot/policy/runtime relationship before listeners, Bun or application windows. Invalid ID/boot/manifest and unsigned or mutated contents refuse.
- **Negative control:**
  - **Single fault:** Skip one boot, policy or bundle integrity check.
  - **At-risk precondition:** The installed app starts with a selected bundle.
  - **Expected RED:** The invalid ID, boot, manifest or mutated-content test refuses before listeners or app windows.

### AC-5: Local provenance cannot select a publisher-backed persistent profile, Strict containm
- **Given:** A local ad-hoc artifact is presented for profile or publisher privilege.
- **When:** The local artifact requests persistent profile, Strict or updater privileges.
- **Then:** Local provenance cannot select a publisher-backed persistent profile, Strict containment or updater authority. The existing Apple publisher path and dev path retain their own rejection tests.
- **Negative control:**
  - **Single fault:** Map local provenance to publisher-backed or Strict authority.
  - **At-risk precondition:** The local app requests a persistent profile or updater privilege.
  - **Expected RED:** The privilege-selection test refuses the request.

### AC-6: The exact existing Guard manifest is enforced. A denied authenticated capability/reso
- **Given:** The app has an exact existing Guard manifest and authenticated request.
- **When:** An authenticated capability or resource request is denied by the manifest.
- **Then:** The exact existing Guard manifest is enforced. A denied authenticated capability/resource request has no prohibited side effect. Legacy's ambient OS authority is explicitly disclosed, never called Strict.
- **Negative control:**
  - **Single fault:** Bypass Guard for one denied request.
  - **At-risk precondition:** The authenticated request lacks the required manifest grant.
  - **Expected RED:** The typed denial and absent side-effect oracle fail.

### AC-7: Actual installed launch facts precede application imports: correct packaged state, ap
- **Given:** The installed app is about to import its first application module.
- **When:** The application starts importing modules and reads its installed launch facts.
- **Then:** Actual installed launch facts precede application imports: correct packaged state, app/resource roots and Bun executable/argv semantics; forged or missing facts fail.
- **Negative control:**
  - **Single fault:** Use a forged or missing launch fact.
  - **At-risk precondition:** The first application import reads installed facts.
  - **Expected RED:** The pre-import provenance check refuses the facts.

### AC-8: Bundled Bun is selected by validated absolute identity. Missing/replaced Bun and host
- **Given:** The installed app must select its bundled Bun executable.
- **When:** The Host resolves and starts Bun using the installed identity.
- **Then:** Bundled Bun is selected by validated absolute identity. Missing/replaced Bun and hostile PATH/preload inputs cannot launch a substitute.
- **Negative control:**
  - **Single fault:** Select a PATH-provided Bun instead of the pinned absolute path.
  - **At-risk precondition:** The installed Bun is missing or replaced.
  - **Expected RED:** The hostile PATH marker is absent and launch refuses.

### AC-9: Each nested executable verifies before the containing bundle is signed; the final out
- **Given:** The app bundle contains nested executables and resources before signing.
- **When:** Pack signs the enclosing bundle after nested executables are verified.
- **Then:** Each nested executable verifies before the containing bundle is signed; the final outer application has a verified ad-hoc signature. Pinned Bun retains its valid input signature and bytes. Independent executable-content/resource mutations fail signature verification; executable modes have separate package/admission checks.
- **Negative control:**
  - **Single fault:** Mutate one nested executable after signing.
  - **At-risk precondition:** The signed bundle is independently verified.
  - **Expected RED:** Signature verification fails for the mutation.

### AC-10: The DMG verifies, mounts read-only, contains the app and Applications shortcut, suppo
- **Given:** A DMG image has been produced for distribution.
- **When:** The user mounts, copies and detaches the image.
- **Then:** The DMG verifies, mounts read-only, contains the app and Applications shortcut, supports copying, and detaches using the observed device identity.
- **Negative control:**
  - **Single fault:** Accept a corrupted DMG or wrong mount identity.
  - **At-risk precondition:** The image is mounted for verification.
  - **Expected RED:** Integrity or device-identity checks refuse it.

### AC-11: Installed app works after renaming/relocation and from unrelated cwd. No source/resou
- **Given:** The installed app is moved or renamed and launched from another cwd.
- **When:** The user launches the app after relocation from an unrelated working directory.
- **Then:** Installed app works after renaming/relocation and from unrelated cwd. No source/resource/runtime access escapes to the checkout or developer cache.
- **Negative control:**
  - **Single fault:** Resolve one runtime resource from the checkout.
  - **At-risk precondition:** The app launches after rename/relocation from unrelated cwd.
  - **Expected RED:** The resource-path oracle detects checkout or developer-cache access.

### AC-12: Original draw.io passes Launch → Open → Edit → Save → unsaved Close → Cancel → Save →
- **Given:** The pinned original draw.io application is installed.
- **When:** The user runs the original editor workflow.
- **Then:** Original draw.io passes Launch → Open → Edit → Save → unsaved Close → Cancel → Save → Reopen → Discard → Quit, with native observations and saved-file readback.
- **Negative control:**
  - **Single fault:** Substitute an inert page or mock editor.
  - **At-risk precondition:** The original workflow is presented as installed acceptance.
  - **Expected RED:** Native editor interaction or saved-file readback is missing.

### AC-13: Normal quit/relaunch, Bun failure/recovery where applicable, Host failure and owned-c
- **Given:** The Host, Bun and application have active owned processes.
- **When:** Bun or Host fails, recovers, quits or relaunches.
- **Then:** Normal quit/relaunch, Bun failure/recovery where applicable, Host failure and owned-child cleanup pass using exact process identities. No installed files or user documents are removed by cleanup.
- **Negative control:**
  - **Single fault:** Leave one owned child alive after failure or quit.
  - **At-risk precondition:** The exact Host birth reaches terminal cleanup.
  - **Expected RED:** The independent PID/start identity census detects the orphan.

### AC-14: Pre-sign input qualification and independent post-sign final-artifact qualification i
- **Given:** Pre-sign input qualification and post-sign artifact receipts are available.
- **When:** The release owner binds the inputs and final artifact.
- **Then:** Pre-sign input qualification and independent post-sign final-artifact qualification identify exact Host/provider/app/runtime bytes; no historical receipt silently transfers.
- **Negative control:**
  - **Single fault:** Change one input qualification pin.
  - **At-risk precondition:** The artifact is prepared for release qualification.
  - **Expected RED:** The independent receipt check refuses the changed input.

### AC-15: A rebuild comparison states exactly which content is reproducible; final app/DMG dige
- **Given:** Two builds and their tool/source/runtime identities are available.
- **When:** The build owner compares the resulting artifacts.
- **Then:** A rebuild comparison states exactly which content is reproducible; final app/DMG digests and tool/source/runtime identities are retained. Byte-identical DMGs are claimed only if demonstrated.
- **Negative control:**
  - **Single fault:** Claim byte-identical DMGs without a comparison.
  - **At-risk precondition:** The two-build comparison is reviewed.
  - **Expected RED:** The recorded content/digest comparison does not support the claim.

### AC-16: Applicable format, warning-denied Clippy, Rust/TypeScript/native/CI checks and indepe
- **Given:** The implementation candidate and its required review/CI gates are available.
- **When:** The final exact implementation head is checked.
- **Then:** Applicable format, warning-denied Clippy, Rust/TypeScript/native/CI checks and independent review pass on exact implementation heads. Future-platform design/issue mapping is present, with no implementation.
- **Negative control:**
  - **Single fault:** Skip one required CI or independent review gate.
  - **At-risk precondition:** The exact implementation head enters final review.
  - **Expected RED:** The required CI or review gate remains non-green.



A failed required cleanup, trust, graph or installed workflow criterion blocks this
mission's completion. A narrower passing subset must not be relabeled full acceptance.
Second-machine acceptance remains separately unverified when unavailable.
