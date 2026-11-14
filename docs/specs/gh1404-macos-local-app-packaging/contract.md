# macOS packaging contract
Part of: [index](index.md)
Owns: Spec refs and sixteen acceptance criteria

## 2. Spec refs
- [Architecture 01](../../architecture/01-overview.md): Host/process ownership.
- [Architecture 03](../../architecture/03-security.md): permission and profile boundaries.
- [Architecture 04 §2](../../architecture/04-electron-compat.md): configuration ownership;
  its nested `app` example is a destination sketch, not an implemented parser.
- [Architecture 06 §§1–3](../../architecture/06-runtime-and-tooling.md): boot,
  Guardian, transport layout, runtime input and packaging ownership.
- [KEL-96](../kel96-no-flag-host-boot.md), especially §4.2: an installed macOS
  container needs a separately approved successor; dev-stage validation is not it.
- [KEL-135](../kel135-persistent-profile-identity.md): publisher-authenticated
  persistent profiles and nonpersistent development profiles.
- [KEL-78](../kel78-strict-profile-sandbox.md): exact OS containment and lifecycle scope.
- [KEL-254](../kel254-windows-installed-root-boot.md): existing Windows installed
  identity boundary, reused by its future owner, not copied into macOS.
- [GH-499](https://github.com/gyldlab/keld/issues/499), [GH-502](https://github.com/gyldlab/keld/issues/502),
  [GH-553](https://github.com/gyldlab/keld/issues/553): original graph, provider and
  qualified-content admission. [GH-452](https://github.com/gyldlab/keld/issues/452)
  and [GH-570](https://github.com/gyldlab/keld/issues/570): app/process launch facts.
- [GH-589](https://github.com/gyldlab/keld/issues/589) consumes the installed
  draw.io evidence; it is not a cyclic prerequisite for building the minimal app.
- KEL-19 owns the packaging/CLI direction; KEL-141 owns related prebuilt acquisition;
  KEL-103 owns a separate Developer ID/public-release slice; KEL-74 owns evidence
  eligibility. Their existing scopes and approval requirements are not broadened
  implicitly by this proposal.

Source baseline inspected: `df99602ef10abd2cd6f3b5c633ba6d1cffee97bc`.
On that main, `keld build` is reserved; `keld-pack` implements Windows update and
identity-container primitives, not macOS bundle assembly. macOS no-flag selection
still selects a private development stage. The no-dev-lease profile path requires
Apple-issued signing identity. The Guardian locates `bun` through the environment
and validates dev-stage cleanup ownership.

The original draw.io integration includes additional graph, launch-fact and transport
work in an independently owned dirty integration tree. Its commit alone does not
identify those working bytes. Required changes must first have a committed, reviewed
handoff; this task does not incorporate uncommitted work or assume main has those APIs.

The source snapshot record is pinned at
[`keld-patched-apps` 0c2541f](https://github.com/gyldlab/keld-patched-apps/tree/0c2541f445668cb3af1cdf58f2839282654b8582/apps/drawio/snapshots/2026-11-03).
It names original draw.io 31.7.0, Bun 1.4.4 and an explicitly Legacy developer
snapshot, not an installable app. Its ZIP SHA-256 is
`a420b00b2450b89ef31bdff6e1ef40f93e4f6c817110fc40e09a4f39540057c7`.
The preserved Host independently hashed to
`d695ec963eb20a115e7c1ecd37bedef6dd68034bb10a423442058f539dba2280`;
the older workflow record names a different Host. Freeze a new exact input and
post-sign evidence set rather than inheriting that record.

Native read-only probes on 2026-11-05 observed the preserved Host's ordinary
`codesign --verify --strict` exit 0 and the explicit Apple developer requirement
exit 3. Its signature reports ad-hoc and no Team Identifier. The snapshot CLI
`keld build` returned `KELD-CLI-045`, exit 2. These probes establish the current
boundary, not installed launch, security acceptance or a generated artifact.

The preserved Bun separately passes strict signature verification. It is already
Developer ID signed, with Hardened Runtime and five existing runtime exceptions.
The Host declares minimum macOS 11.0; Bun declares 13.0. These Mach-O declarations
are compatibility floors, not acceptance on those versions. Preserve independently
pinned, verified Bun bytes and their signature in this first slice; do not force
re-signing and accidentally discard its runtime properties. Bun's publisher identity
is not the application's publisher identity.

### Primary documentation receipt

Retrieved 2026-11-05. Context7 discovery used Apple/Bun documentation and Microsoft
WebView2/WebKitGTK libraries; primary documents and local Apple manuals decide claims.

| Decision | Primary source |
| --- | --- |
| Standard bundle/code/resource locations | [Apple bundle layout](https://developer.apple.com/documentation/bundleresources/placing-content-in-a-bundle) |
| Inside-out signing; no deep signing | [Apple signing](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac) |
| Nested signature identity is not an exact runtime-version pin | [Apple TN2206](https://developer.apple.com/library/archive/technotes/tn2206/) and [TN3126](https://developer.apple.com/documentation/technotes/tn3126-inside-code-signing-hashes) |
| Ad-hoc has no publisher identity | [Apple ad-hoc semantics](https://developer.apple.com/documentation/security/seccodesignatureflags/adhoc) |
| Dynamic identity versus full resource validation | [Dynamic validation](https://developer.apple.com/documentation/security/seccodecheckvalidity(_:_:_:)) and [static validation](https://developer.apple.com/documentation/security/secstaticcodecheckvaliditywitherrors(_:_:_:_:)) |
| Static verification does not prevent later mutation | Same Apple static-validation contract; retained-input checks below are additional obligations |
| DMG tooling | [Apple packaging](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution), installed `hdiutil(1)` |
| Bun entitlement hypotheses, not approved blanket exceptions | [Bun guide](https://bun.com/guides/runtime/codesign-macos-executable), [Apple JIT requirements](https://developer.apple.com/documentation/apple-silicon/porting-just-in-time-compilers-to-apple-silicon) |
| Future WebView2 provisioning | [Microsoft distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution) |
| Future Linux desktop/state integration | [Desktop Entry 1.5](https://specifications.freedesktop.org/desktop-entry/latest-single/), [XDG Base Directory 0.8](https://specifications.freedesktop.org/basedir/latest/) |

These references do not prove that the installed implementation works. Exact API
availability/flags, executable requirements and minimum entitlements must also be
checked against the SDK and pinned binaries used for implementation.

## 3. Acceptance criteria
All implementation criteria below are unrun at draft creation.


### AC-1: Installed package criterion 1
- **Given:** Two valid app configurations exist.
- **When:** The framework emits both application bundles.
- **Then:** One framework path builds two configured macOS apps, including a minimal fixture and original draw.io, without app-specific framework branches.
- **Negative control:**
  - **Single fault:** Inject an app-specific framework-only switch.
  - **At-risk precondition:** The generic app is selected.
  - **Expected RED:** The second app build must fail.

### AC-2: Installed package criterion 2
- **Given:** A package has required identities and resources.
- **When:** Pack validates inputs.
- **Then:** Missing/invalid metadata, Host, Bun, entry, permission file or resource, unknown config, wrong target and escaping paths fail before final output publication.
- **Negative control:**
  - **Single fault:** Remove one required Bun or Host executable.
  - **At-risk precondition:** The candidate has an otherwise valid config.
  - **Expected RED:** Pack must refuse publication.

### AC-3: Installed package criterion 3
- **Given:** An ad-hoc local app has been installed independently.
- **When:** Launch Services opens it.
- **Then:** The copied .app launches through Launch Services with no terminal, checkout, Cargo workspace, external Bun, dev lease, test flag or private test credential.
- **Negative control:**
  - **Single fault:** Select external Bun through PATH.
  - **At-risk precondition:** No checkout or dev lease is available.
  - **Expected RED:** Launch must fail or remain bundle-owned.

### AC-4: Installed package criterion 4
- **Given:** An installed bundle has sealed boot and policy.
- **When:** Host performs installed admission.
- **Then:** Installed selection validates the enclosing bundle and exact boot/policy/runtime relationship before listeners, Bun or application windows. Invalid ID/boot/manifest and unsigned or mutated contents refuse.
- **Negative control:**
  - **Single fault:** Mutate the boot descriptor.
  - **At-risk precondition:** No app resource has been allocated.
  - **Expected RED:** Host must refuse before side effects.

### AC-5: Installed package criterion 5
- **Given:** The bundle has local ad-hoc provenance.
- **When:** Runtime selects authority.
- **Then:** Local provenance cannot select a publisher-backed persistent profile, Strict containment or updater authority. The existing Apple publisher path and dev path retain their own rejection tests.
- **Negative control:**
  - **Single fault:** Claim publisher-backed Strict or updater authority.
  - **At-risk precondition:** Only local provenance has been verified.
  - **Expected RED:** Runtime must refuse promotion.

### AC-6: Installed package criterion 6
- **Given:** A Legacy app requests a denied capability.
- **When:** Guard handles the authenticated request.
- **Then:** The exact existing Guard manifest is enforced. A denied authenticated capability/resource request has no prohibited side effect. Legacy's ambient OS authority is explicitly disclosed, never called Strict.
- **Negative control:**
  - **Single fault:** Remove the capability grant.
  - **At-risk precondition:** The app attempts the denied operation.
  - **Expected RED:** No prohibited side effect may occur.

### AC-7: Installed package criterion 7
- **Given:** Installed startup facts were captured.
- **When:** Bun consumes pre-entry facts.
- **Then:** Actual installed launch facts precede application imports: correct packaged state, app/resource roots and Bun executable/argv semantics; forged or missing facts fail.
- **Negative control:**
  - **Single fault:** Forge a public environment value.
  - **At-risk precondition:** Boot and process identities are sealed.
  - **Expected RED:** The pre-entry state must reject the forgery.

### AC-8: Installed package criterion 8
- **Given:** An installed Bun is sealed.
- **When:** Host launches the runtime.
- **Then:** Bundled Bun is selected by validated absolute identity. Missing/replaced Bun and hostile PATH/preload inputs cannot launch a substitute.
- **Negative control:**
  - **Single fault:** Replace Bun with ambient PATH executable.
  - **At-risk precondition:** The installed app starts.
  - **Expected RED:** A marker from the ambient Bun must not execute.

### AC-9: Installed package criterion 9
- **Given:** Nested images are signed in order.
- **When:** The outer bundle is verified.
- **Then:** Each nested executable verifies before the containing bundle is signed; the final outer application has a verified ad-hoc signature. Pinned Bun retains its valid input signature and bytes. Independent executable-content/resource mutations fail signature verification; executable modes have separate package/admission checks.
- **Negative control:**
  - **Single fault:** Alter one nested executable after signing.
  - **At-risk precondition:** The final app seal exists.
  - **Expected RED:** Signature verification must fail.

### AC-10: Installed package criterion 10
- **Given:** A local DMG contains the .app.
- **When:** The installer mounts the artifact.
- **Then:** The DMG verifies, mounts read-only, contains the app and Applications shortcut, supports copying, and detaches using the observed device identity.
- **Negative control:**
  - **Single fault:** Corrupt its payload bytes.
  - **At-risk precondition:** The image is a retained test copy.
  - **Expected RED:** DMG verification must fail.

### AC-11: Installed package criterion 11
- **Given:** A signed app is relocated.
- **When:** Host resolves its installed root.
- **Then:** Installed app works after renaming/relocation and from unrelated cwd. No source/resource/runtime access escapes to the checkout or developer cache.
- **Negative control:**
  - **Single fault:** Run with a checkout-only resource.
  - **At-risk precondition:** The new cwd is unrelated.
  - **Expected RED:** No checkout resource access may succeed.

### AC-12: Installed package criterion 12
- **Given:** Pinned original editor is installed.
- **When:** Native Open/Edit/Save and Close run.
- **Then:** Original draw.io passes Launch → Open → Edit → Save → unsaved Close → Cancel → Save → Reopen → Discard → Quit, with native observations and saved-file readback.
- **Negative control:**
  - **Single fault:** Use inert HELLO editor instead.
  - **At-risk precondition:** Real draw.io source was requested.
  - **Expected RED:** Workflow and file readback must fail.

### AC-13: Installed package criterion 13
- **Given:** Host and Bun own process descendants.
- **When:** Shutdown and relaunch run.
- **Then:** Normal quit/relaunch, Bun failure/recovery where applicable, Host failure and owned-child cleanup pass using exact process identities. No installed files or user documents are removed by cleanup.
- **Negative control:**
  - **Single fault:** Kill the Host during an active role.
  - **At-risk precondition:** Owned process identities are recorded.
  - **Expected RED:** Cleanup/relaunch oracle must detect leftovers.

### AC-14: Installed package criterion 14
- **Given:** An exact source receipt and app exist.
- **When:** Post-sign verification compares identities.
- **Then:** Pre-sign input qualification and independent post-sign final-artifact qualification identify exact Host/provider/app/runtime bytes; no historical receipt silently transfers.
- **Negative control:**
  - **Single fault:** Change a provider receipt pin.
  - **At-risk precondition:** The artifact bytes remain unchanged.
  - **Expected RED:** Qualification must refuse.

### AC-15: Installed package criterion 15
- **Given:** Two controlled build outputs exist.
- **When:** The content comparison is made.
- **Then:** A rebuild comparison states exactly which content is reproducible; final app/DMG digests and tool/source/runtime identities are retained. Byte-identical DMGs are claimed only if demonstrated.
- **Negative control:**
  - **Single fault:** Swap one runtime input between builds.
  - **At-risk precondition:** Both recorded inputs are identified.
  - **Expected RED:** The report must not claim byte identity.

### AC-16: Installed package criterion 16
- **Given:** A source implementation reaches quality gates.
- **When:** CI validates it against exact HEAD.
- **Then:** Applicable format, warning-denied Clippy, Rust/TypeScript/native/CI checks and independent review pass on exact implementation heads. Future-platform design/issue mapping is present, with no implementation.
- **Negative control:**
  - **Single fault:** Skip a required platform test.
  - **At-risk precondition:** That platform is selected by the router.
  - **Expected RED:** Required CI must refuse completion.


A failed required cleanup, trust, graph or installed workflow criterion blocks this
mission's completion. A narrower passing subset must not be relabeled full acceptance.
Second-machine acceptance remains separately unverified when unavailable.
