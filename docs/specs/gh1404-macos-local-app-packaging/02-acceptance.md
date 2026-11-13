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

- **Given:** An admitted LocalDevelopment artifact has consistent signed bundle, boot, policy and graph identities.
- **When:** Exercise the owning local-profile decision with valid provenance and a publisher-backed store request.
- **Then:** The ordinary local launch selects a fresh ephemeral WebView profile.
The local-profile owner denies publisher-backed persistence. Local provenance grants neither Strict containment
nor updater authority. Existing Apple publisher and development paths retain their own rejection tests.
- **Negative control:** Use a reviewed owner-level probe, not a new public configuration field.
  - **Single fault:** Request publisher-backed persistence at the local-profile decision for valid local provenance.
  - **At-risk precondition:** Bundle admission and policy integrity pass before the probe reaches that owner.
The Guard Legacy/Strict policy field is not a WebView store selector.
  - **Expected RED:** Record the local-profile owner's denial, with no persistent store selected.
An earlier signature, manifest-digest or graph refusal does not satisfy this control.

In an isolated nonshipping fault test, disable only the local-profile restriction.
The negative check must detect selection of the forbidden persistent store and turn RED.
Test Guard Strict and updater authority separately at their owning decisions, with consistent identities.
These are proposed tests, not permission to weaken shipped admission or add a public bypass.

### AC-6: Guard enforcement

- **Given:** An installed Legacy app has one authenticated link and the exact Guard manifest.
The manifest permits a broker operation and denies a separate operation.
- **When:** Invoke both operations through that same link and manifest.
- **Then:** The permitted call produces its authorized effect, verified outside Guard's decision result.
The denied call returns its typed refusal and produces no prohibited effect.
Legacy's ambient OS authority remains explicitly disclosed, never called Strict.
- **Negative control:** Pair the denied operation with the permitted successful-effect control.
  - **Single fault:** Request the operation denied by the exact manifest.
  - **At-risk precondition:** Authentication succeeds and both calls reach Guard.
The permitted call demonstrates that an unconditional denial cannot satisfy the test.
  - **Expected RED:** A prohibited external effect or missing typed refusal fails the denied control.
A missing authorized effect fails the permitted control.

In separate isolated nonshipping fault tests, inject deny-all and allow-all decisions.
Deny-all must fail the permitted-effect control. Allow-all must fail the denied-effect control.
Use owned disposable resources and independently read back both effects.

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

### AC-12: Original editor workflow

- **Given:** The pinned original editor opens a document through its native workflow.
- **When:** Save state A, make unsaved edit B, close and select Cancel in the observed native dialog.
Then save B, close and reopen, make unsaved edit C, close and select Discard in a second observed native dialog.
- **Then:** Cancel preserves the open window and unsaved B while disk still contains A.
Saving B changes disk to B. Reopening reads B. Discard closes without saving C, and disk retains B.
Reopening again shows B without C, followed by ordinary Quit.
Record native window/document state and independent saved-file readback at each boundary.
- **Negative control:** Isolate the Cancel branch after the fresh unsaved edit B.
  - **Single fault:** Treat Cancel as permission to close.
  - **At-risk precondition:** Independent native observation confirms the unsaved-change dialog is visible
and Cancel is selected while B is dirty.
  - **Expected RED:** Closing the window or losing B fails the native observation.
Neither the earlier Save nor a label saying unsaved Close establishes this precondition.

### AC-13: Process lifecycle and cleanup

- **Given:** An installed app has a live Host, enrolled Guardian and active Bun generation with exact process identities.
- **When:** Exercise healthy quit/relaunch and separate Bun-only, Host-only and Guardian-only death cases.
- **Then:** Required cleanup and bounded subsequent launch succeed, with link revocation and owned-group retirement.
Independent PID/start identities and descendant census show no remaining owned children or stale authenticated link.
Installed bundle and user-document hashes remain unchanged.
- **Negative control:** Exercise Guardian-only loss independently of Bun-only and Host-only controls.
  - **Single fault:** Kill only the enrolled Guardian while Host is initially alive and Bun is active.
  - **At-risk precondition:** Record Host, Guardian and Bun PID/start identities and generation enrollment
before termination. Confirm the Host was not simultaneously terminated.
  - **Expected RED:** A surviving owned child, stale authenticated link, changed bundle/document hash
or failed required relaunch before the declared deadline fails the test.

Declare bounded observation and relaunch deadlines before the run, not after seeing its outcome.
Healthy quit/relaunch remains the positive control. Known simultaneous-death and detached-descendant limits
do not exempt Guardian-only loss. If this required case fails, keep AC-13 open with the existing runtime
owner named for the remaining work. Do not silently waive the requirement or add an unreviewed supervisor.

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

### AC-17: Present but unqualified executable module

- **Given:** A disposable validly signed local bundle has consistent boot, policy and runtime facts.
An executable marker is physically present but excluded from the independently qualified graph.
- **When:** After relocation, a qualified entry attempts static and dynamic imports, require and worker entry points.
- **Then:** Record two distinct verdicts: Pack producer refusal and installed-resolver refusal.
The installed resolver denies the present unqualified marker before execution, including after Bun restart.
An independently qualified version of that marker executes under otherwise identical bundle/runtime conditions.
- **Negative control:** Require a second fixture reaching installed admission in addition to the Pack refusal fixture.
  - **Single fault:** Exclude the present executable marker's identity from the independently qualified graph.
  - **At-risk precondition:** Valid signatures, boot, policy and runtime facts admit the disposable bundle.
The qualified entry reaches installed module admission for the marker after relocation.
  - **Expected RED:** Marker execution or missing installed-owner refusal fails installed-resolver acceptance.
A Pack refusal alone cannot satisfy this verdict.

Build the disposable installed fixture through reviewed test-fixture construction and normal local signing.
Do not disable installed admission or qualify the excluded marker to make the fixture reach its owner.
Record which owner refused each fixture. If earlier admission prevents reaching the installed resolver,
record that coverage blocker and keep this verdict unpassed until a reviewed fixture reaches the owner.
Repeat all four loading paths after restart and relocation, with the qualified positive control.

A failed required cleanup, trust, graph or installed workflow criterion blocks this
mission's completion. A narrower passing subset must not be relabeled full acceptance.
Second-machine acceptance remains separately unverified when unavailable.
