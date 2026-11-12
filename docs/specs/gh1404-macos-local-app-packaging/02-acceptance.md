# Packaging acceptance

Part of: [index](index.md)
Owns: Installed application acceptance obligations

## 3. Acceptance criteria

All implementation criteria below are unrun at draft creation.

### AC-1: Two independent applications

- **Given:** A qualified minimal app and original draw.io share the framework builder.
- **When:** Build both configured apps.
- **Then:** One framework path builds two configured macOS apps, including a minimal fixture and original
draw.io, without app-specific framework branches.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Replace the generic build path with an app-name-specific branch.
  - **At-risk precondition:** Both configurations are admissible and reach the build path.
  - **Expected RED:** The second app cannot complete the same framework path.

### AC-2: Input refusal

- **Given:** An otherwise qualified build request is ready for assembly.
- **When:** Submit the request with the mutated input.
- **Then:** Missing/invalid metadata, Host, Bun, entry, permission file or resource, unknown config, wrong
target and escaping paths fail before final output publication.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Delete its required entry file.
  - **At-risk precondition:** All other inputs and configuration remain valid.
  - **Expected RED:** Typed refusal occurs before final output publication.

### AC-3: Installed launch

- **Given:** A valid app has been copied to an independent installation.
- **When:** Launch the copied app through Launch Services.
- **Then:** The copied .app launches through Launch Services with no terminal, checkout, Cargo workspace,
external Bun, dev lease, test flag or private test credential.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Remove the developer checkout.
  - **At-risk precondition:** The copied bundle remains valid and Launch Services starts it.
  - **Expected RED:** Any checkout access or failed installed launch fails this criterion.

### AC-4: Bundle admission

- **Given:** An otherwise admitted bundle contains exact boot, policy and runtime members.
- **When:** Request installed boot.
- **Then:** Installed selection validates the enclosing bundle and exact boot/policy/runtime relationship
before listeners, Bun or application windows. Invalid ID/boot/manifest and unsigned or mutated contents
refuse.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Change only CFBundleIdentifier and validly re-sign the outer app with its original signing
identifier.
  - **At-risk precondition:** All seals, the signing identifier and remaining inputs are valid. The altered
CFBundleIdentifier reaches the identifier consistency validator.
  - **Expected RED:** Identifier mismatch refuses before listeners, Bun or application windows.

### AC-5: Local trust boundary

- **Given:** An ad-hoc local bundle has otherwise valid signed metadata.
- **When:** Request a publisher-backed persistent profile.
- **Then:** Local provenance cannot select a publisher-backed persistent profile, Strict containment or
updater authority. The existing Apple publisher path and dev path retain their own rejection tests.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Change only the requested profile class and re-sign the local bundle.
  - **At-risk precondition:** The local seal remains valid so signature rejection cannot hide profile
enforcement.
  - **Expected RED:** The local provenance owner refuses publisher-backed persistence.

### AC-6: Guard enforcement

- **Given:** An installed Legacy app has an authenticated link and exact Guard policy.
- **When:** Issue a capability request denied by that policy.
- **Then:** The exact existing Guard manifest is enforced. A denied authenticated capability/resource request
has no prohibited side effect. Legacy's ambient OS authority is explicitly disclosed, never called Strict.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Request one denied capability.
  - **At-risk precondition:** Authentication succeeds and the request reaches Guard.
  - **Expected RED:** Typed denial occurs with no prohibited external side effect.

### AC-7: Installed launch facts

- **Given:** An installed generation can otherwise run its first application import.
- **When:** Start the generation.
- **Then:** Actual installed launch facts precede application imports: correct packaged state, app/resource
roots and Bun executable/argv semantics; forged or missing facts fail.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Omit required installed launch facts.
  - **At-risk precondition:** Valid runtime and bootstrap inputs reach fact admission.
  - **Expected RED:** Application imports cannot establish packaged state or proceed with missing facts.

### AC-8: Bundled runtime identity

- **Given:** A valid bundle has a sealed Bun digest and valid boot metadata.
- **When:** Start an installed Bun generation.
- **Then:** Bundled Bun is selected by validated absolute identity. Missing/replaced Bun and hostile
PATH/preload inputs cannot launch a substitute.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Replace Bun with different signed bytes satisfying the same designated requirement.
  - **At-risk precondition:** The replacement signature and executable mode are valid.
  - **Expected RED:** Sealed digest validation refuses the replacement before application effects.

### AC-9: Signing and executable modes

- **Given:** A copied final app has passed signature and executable-mode checks.
- **When:** Validate its executable modes and launch eligibility.
- **Then:** Each nested executable verifies before the containing bundle is signed; the final outer
application has a verified ad-hoc signature. Pinned Bun retains its valid input signature and bytes.
Independent executable-content/resource mutations fail signature verification; executable modes have separate
package/admission checks.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Set the copied Host mode to `0644`, removing all execute bits without changing its bytes.
  - **At-risk precondition:** Its signature remains valid, isolating the mode check.
  - **Expected RED:** Pack or Launch Services refuses the non-executable Host.

### AC-10: DMG lifecycle

- **Given:** A disposable DMG copy has an otherwise valid build receipt.
- **When:** Verify the image before installation.
- **Then:** The DMG verifies, mounts read-only, contains the app and Applications shortcut, supports copying,
and detaches using the observed device identity.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Corrupt one image content block.
  - **At-risk precondition:** Image verification executes before mounting or application launch.
  - **Expected RED:** Integrity verification fails and unrelated mounts/files remain untouched.

### AC-11: Qualified module resolution

- **Given:** An installed app has a complete independently qualified sealed graph.
- **When:** Rename or relocate the app, then start it from an unrelated cwd.
- **Then:** The app starts and its executable application dependencies resolve only from that graph.
No source/resource/runtime access escapes to the checkout or developer cache.
Missing or unqualified imports in the separate refusal case below refuse before evaluation,
without install, registry or ambient-cache fallback.
- **Negative control:** Use a separate missing-dependency fixture, not the complete-graph success fixture.
  - **Single fault:** Omit one imported dependency from the captured graph.
  - **At-risk precondition:** Valid boot, runtime and signature inputs reach executable module resolution.
A matching marker package exists only in an ancestor node_modules directory.
  - **Expected RED:** The missing import refuses before external marker evaluation, without fallback acquisition.
The complete-graph success fixture still launches after relocation from an unrelated cwd.

- **Additional negative control:** Use a separate present-but-unqualified module fixture.
  - **Single fault:** Add one executable marker module to a signed bundle without
    admitting its identity to the independently qualified resource graph.
  - **At-risk precondition:** The disposable bundle has valid signing, boot, policy
    and runtime facts. The qualified entry attempts to import the marker module.
  - **Expected RED:** The existing graph owner or installed resolver refuses the
    unqualified module before marker execution. A valid code seal is insufficient.
  - **Valid control:** The independently qualified and sealed version of that module
    imports successfully under the same runtime and bundle settings.

### AC-12: Original editor workflow

- **Given:** The pinned original editor opens a document and has unsaved changes.
- **When:** Close its native window and select Cancel.
- **Then:** Original draw.io passes Launch → Open → Edit → Save → unsaved Close → Cancel → Save → Reopen →
Discard → Quit, with native observations and saved-file readback.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Treat Cancel as permission to close.
  - **At-risk precondition:** The unsaved-change dialog is reached and Cancel is selected.
  - **Expected RED:** Window/document state loss fails independent native observation.

### AC-13: Process lifecycle and cleanup

- **Given:** An installed app owns a Bun generation with known process identity.
- **When:** Trigger ordinary cleanup and relaunch.
- **Then:** Normal quit/relaunch, Bun failure/recovery where applicable, Host failure and owned-child cleanup
pass using exact process identities. No installed files or user documents are removed by cleanup.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Kill the owned Bun process.
  - **At-risk precondition:** The recorded generation is live before termination and cleanup observes its
death.
  - **Expected RED:** Leaked owned children or changed bundle/document hashes fail the lifecycle check.

### AC-14: Qualification identity

- **Given:** A qualification receipt identifies admitted Host/provider/app/runtime inputs.
- **When:** Evaluate the receipt for altered inputs.
- **Then:** Pre-sign input qualification and independent post-sign final-artifact qualification identify exact
Host/provider/app/runtime bytes; no historical receipt silently transfers.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Change one provider qualification pin.
  - **At-risk precondition:** The receipt is structurally valid and reaches qualification comparison.
  - **Expected RED:** The old receipt cannot qualify the changed input.

### AC-15: Rebuild evidence

- **Given:** Two local builds use recorded input and tool identities.
- **When:** Compare their reproducibility claims.
- **Then:** A rebuild comparison states exactly which content is reproducible; final app/DMG digests and
tool/source/runtime identities are retained. Byte-identical DMGs are claimed only if demonstrated.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Change only the second final artifact bytes.
  - **At-risk precondition:** Both output receipts are available for independent comparison.
  - **Expected RED:** A byte-identical claim is rejected when final digests differ.

### AC-16: Implementation gates

- **Given:** An exact implementation head is submitted for required checks and review.
- **When:** Evaluate closeout eligibility.
- **Then:** Applicable format, warning-denied Clippy, Rust/TypeScript/native/CI checks and independent review
pass on exact implementation heads. Future-platform design/issue mapping is present, with no implementation.
- **Negative control:** Isolate the following fault before evaluating the required observable.
  - **Single fault:** Leave one applicable required gate failing.
  - **At-risk precondition:** The closeout evaluates the exact head, not an earlier receipt.
  - **Expected RED:** The failing gate blocks a complete acceptance claim.

A failed required cleanup, trust, graph or installed workflow criterion blocks this
mission's completion. A narrower passing subset must not be relabeled full acceptance.
Second-machine acceptance remains separately unverified when unavailable.
