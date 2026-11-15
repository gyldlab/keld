# Packaging requirements and admission

Part of: [index](index.md)
Owns: Sections 2–3 and design sections 4.1–4.3

## 2. Spec refs

Tracker of record: [GH-1404](https://github.com/gyldlab/keld/issues/1404).
Future design only: [Windows GH-1405](https://github.com/gyldlab/keld/issues/1405)
and [Linux GH-1406](https://github.com/gyldlab/keld/issues/1406).
The user authorized implementation of the exact reviewed contract at
`3c490596ceb483254551d37041d25e7e110dfa41` on 2026-11-05. The assistant recorded
that authorization through the connected non-author account in
[review 5787262030](https://github.com/gyldlab/keld/pull/1407#pullrequestreview-5787262030),
after verifying the specification and independent review findings. This is the
[workflow step 2](../../agents/workflow.md) approval record, not native implementation
or installed-artifact acceptance. The approved behavioral contract below is unchanged.
The user's delivery objective remains open until installed macOS acceptance passes.

Implementation note: the first local installed cell requires an explicitly declared
Legacy permission profile; an absent profile is refused rather than inferred as
consent to uncontained execution. Existing development schemas and their historical
profile behavior are unchanged. The local Guardian reuses the authenticated private
role with a closed `local-bundle-v1` suffix carrying the exact Bun, boot and policy
digests; those expected values are data, not an admission capability. Installed
origin is independently derived from current code before each generation. These
mechanical choices do not qualify the original application graph or final artifact.


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

All implementation criteria below were unrun at draft creation.
The historical A1–A16 labels remain aliases for the corresponding AC headings.

### AC-1: Historical A1 obligation
- **Given:** Two differently configured apps with independently qualified inputs.
- **When:** The same shipping framework packages both apps.
- **Then:** One framework path builds two configured macOS apps, including a minimal fixture and original
  draw.io, without app-specific framework branches.
- **Negative control:**
  - **Single fault:** Inject an app-specific dispatch condition rejecting the differently configured second fixture.
  - **At-risk precondition:** Both app inputs remain valid; their names, paths and entries differ.
  - **Expected RED:** The valid second app fails to build, exposing the forbidden app-specific framework branch.

### AC-2: Historical A2 obligation
- **Given:** A valid config and qualified required build inputs.
- **When:** Pack validates inputs before output publication.
- **Then:** Missing/invalid metadata, Host, Bun, entry, permission file or resource, unknown config, wrong
  target and escaping paths fail before final output publication.
- **Negative control:**
  - **Single fault:** Remove the required Host input.
  - **At-risk precondition:** All remaining required inputs are valid.
  - **Expected RED:** Typed missing-input refusal and no published output.

### AC-3: Historical A3 obligation
- **Given:** An independently copied signed app.
- **When:** Launch Services starts the app without development dependencies.
- **Then:** The copied .app launches through Launch Services with no terminal, checkout, Cargo workspace,
  external Bun, dev lease, test flag or private test credential.
- **Negative control:**
  - **Single fault:** Remove the sealed installed boot file.
  - **At-risk precondition:** Launch uses the copied app rather than an existing development instance.
  - **Expected RED:** Installed launch refuses instead of using a development fallback.

### AC-4: Historical A4 obligation
- **Given:** A signed local bundle at installed admission.
- **When:** Host validates the boot, policy and runtime relationship.
- **Then:** Installed selection validates the enclosing bundle and exact boot/policy/runtime relationship
  before listeners, Bun or application windows. Invalid ID/boot/manifest and unsigned or mutated contents
  refuse.
- **Negative control:**
  - **Single fault:** Change the boot application ID.
  - **At-risk precondition:** The remaining bundle metadata reaches the identity comparison.
  - **Expected RED:** Identity mismatch refuses before listeners, Bun or windows.

### AC-5: Historical A5 obligation
- **Given:** A validly re-signed local-provenance package.
- **When:** Installed selection evaluates profile authority.
- **Then:** Local provenance cannot select a publisher-backed persistent profile, Strict containment or
  updater authority. The existing Apple publisher path and dev path retain their own rejection tests.
- **Negative control:**
  - **Single fault:** Request a publisher-backed persistent profile.
  - **At-risk precondition:** The local bundle otherwise meets admission checks.
  - **Expected RED:** Publisher-only privilege selection refuses despite valid local signing.

### AC-6: Historical A6 obligation
- **Given:** An authenticated app with the exact admitted Guard manifest.
- **When:** The app requests a protected effect.
- **Then:** The exact existing Guard manifest is enforced. A denied authenticated capability/resource request
  has no prohibited side effect. Legacy's ambient OS authority is explicitly disclosed, never called Strict.
- **Negative control:**
  - **Single fault:** Request a capability denied by that manifest.
  - **At-risk precondition:** The request is authenticated and reaches Guard.
  - **Expected RED:** Typed denial and no prohibited external side effect.

### AC-7: Historical A7 obligation
- **Given:** An admitted installed app about to import its main.
- **When:** The bootstrap consumes launch facts.
- **Then:** Actual installed launch facts precede application imports: correct packaged state, app/resource
  roots and Bun executable/argv semantics; forged or missing facts fail.
- **Negative control:**
  - **Single fault:** Forge packaged state through a public environment value.
  - **At-risk precondition:** No authenticated launch-facts record establishes that forged state.
  - **Expected RED:** The public value cannot establish packaged state or bypass fact admission.

### AC-8: Historical A8 obligation
- **Given:** A signed installed app and a hostile PATH Bun that writes a marker.
- **When:** The runtime selects the bundled Bun.
- **Then:** Bundled Bun is selected by validated absolute identity. Missing/replaced Bun and hostile
  PATH/preload inputs cannot launch a substitute.
- **Negative control:**
  - **Single fault:** Remove the installed Bun.
  - **At-risk precondition:** The substitute remains reachable through PATH.
  - **Expected RED:** Launch fails and the substitute marker stays absent.

### AC-9: Historical A9 obligation
- **Given:** A copied final signed app with independently recorded input identities.
- **When:** Independent validation checks executable and resource integrity.
- **Then:** Each nested executable verifies before the containing bundle is signed; the final outer
  application has a verified ad-hoc signature. Pinned Bun retains its valid input signature and bytes.
  Independent executable-content/resource mutations fail signature verification; executable modes have
  separate package/admission checks.
- **Negative control:**
  - **Single fault:** Mutate one copied app-resource byte after signing.
  - **At-risk precondition:** The resource is included in the final signature seal.
  - **Expected RED:** Signature verification refuses the mutated copy.

### AC-10: Historical A10 obligation
- **Given:** A disposable copy of the exact generated DMG.
- **When:** Independent tooling verifies and mounts the image.
- **Then:** The DMG verifies, mounts read-only, contains the app and Applications shortcut, supports copying,
  and detaches using the observed device identity.
- **Negative control:**
  - **Single fault:** Corrupt the image.
  - **At-risk precondition:** The image verifier receives the corrupted copy.
  - **Expected RED:** Integrity verification fails without altering unrelated mounts or files.

### AC-11: Historical A11 obligation
- **Given:** A signed app copied to a renamed location with unrelated cwd.
- **When:** The app resolves its installed resources.
- **Then:** Installed app works after renaming/relocation and from unrelated cwd. No source/resource/runtime
  access escapes to the checkout or developer cache.
- **Negative control:**
  - **Single fault:** Remove the required installed app resource.
  - **At-risk precondition:** The historical checkout still contains the same resource.
  - **Expected RED:** Launch refuses instead of rescuing the resource from the checkout.

### AC-12: Historical A12 obligation
- **Given:** The pinned original editor in an installed native window.
- **When:** An observer performs the complete original workflow below.
- **Then:** Original draw.io passes Launch → Open → Edit → Save → unsaved Close → Cancel → Save → Reopen →
  Discard → Quit, with native observations and saved-file readback.
- **Negative control:**
  - **Single fault:** Suppress the real file write during Save.
  - **At-risk precondition:** The observer reaches Save with modified content.
  - **Expected RED:** Independent file readback fails the required saved-content assertion.

### AC-13: Historical A13 obligation
- **Given:** An installed session with exact Host and child process identities.
- **When:** A bounded observer checks failure cleanup and relaunch.
- **Then:** Normal quit/relaunch, Bun failure/recovery where applicable, Host failure and owned-child cleanup
  pass using exact process identities. No installed files or user documents are removed by cleanup.
- **Negative control:**
  - **Single fault:** Suppress the recovery owner action that starts the required replacement generation.
  - **At-risk precondition:** The identified Bun exits unexpectedly in a session whose contract requires recovery.
  - **Expected RED:** The required new child generation is absent at the bounded deadline, failing the lifecycle oracle.

### AC-14: Historical A14 obligation
- **Given:** Independent input qualification and final-artifact receipts.
- **When:** Qualification compares exact source and artifact identities.
- **Then:** Pre-sign input qualification and independent post-sign final-artifact qualification identify exact
  Host/provider/app/runtime bytes; no historical receipt silently transfers.
- **Negative control:**
  - **Single fault:** Change one provider qualification pin.
  - **At-risk precondition:** The remaining receipt inputs match.
  - **Expected RED:** The affected qualification boundary rejects the mismatched identity.

### AC-15: Historical A15 obligation
- **Given:** Two build outputs and recorded tool, source and runtime identities.
- **When:** An observer compares the declared reproducible content.
- **Then:** A rebuild comparison states exactly which content is reproducible; final app/DMG digests and
  tool/source/runtime identities are retained. Byte-identical DMGs are claimed only if demonstrated.
- **Negative control:**
  - **Single fault:** Change one final artifact byte.
  - **At-risk precondition:** The comparison uses the exact recorded final artifact digest.
  - **Expected RED:** Digest equality fails; byte-identical output cannot be claimed.

### AC-16: Historical A16 obligation
- **Given:** Exact implementation heads and their required check matrix.
- **When:** Independent reviewers validate quality and future-platform scope.
- **Then:** Applicable format, warning-denied Clippy, Rust/TypeScript/native/CI checks and independent review
  pass on exact implementation heads. Future-platform design/issue mapping is present, with no implementation.
- **Negative control:**
  - **Single fault:** Supply a passing CI receipt for a different head.
  - **At-risk precondition:** The current head requires its own quality evidence.
  - **Expected RED:** Receipt identity mismatch leaves the exact-head gate unpassed.

A failed required cleanup, trust, graph or installed workflow criterion blocks this
mission's completion. A narrower passing subset must not be relabeled full acceptance.
Second-machine acceptance remains separately unverified when unavailable.

## 4. Design

### 4.1 First-principles and reuse decision

The installed application needs one native event-loop owner, one authenticated
application link, one Bun generation owner, one permission evaluator, and a durable
resource root. Packaging does not need a new runtime process.

| Atom / owner | Input → output and failure boundary | First falsifier / independence |
| --- | --- | --- |
| Authoring / Core extraction + CLI | literal config and independent input pins → one typed build request | ambiguous/missing fields refuse; config never grants authority |
| Assembly / Pack | admitted inputs → isolated bundle candidate | absent/escaping member publishes nothing; assembly is not admission |
| Local provenance / Core + Apple APIs | current executable and enclosing sealed bundle → opaque local bundle facts | copied naked Host or altered seal refuses; no publisher identity follows |
| Profile identity / Core + WV | verified provenance → ephemeral local store or existing publisher store | ad-hoc persistence/Strict request refuses; no fabricated Team ID |
| Graph qualification / existing graph owner | qualified input Host and providers → exact admitted application graph | changed provider/input Host refuses; a code seal cannot qualify graph semantics |
| Authorization / Guard | existing exact policy and authenticated caller → bounded operation | denied request has no outside effect; launch success confers no grant |
| Custody / Runtime + thin Host | authenticated Guardian and exact bundled Bun → owned generations and cleanup | hostile PATH/invalid bootstrap/Host death; installation is never disposable stage |
| Evidence / native acceptance | finalized artifact → independent receipts | altered artifact or foreign-OS receipt cannot inherit a pass |

Alternatives rejected:

- Ship the native harness or synthesize a dev lease: retains test/developer authority
  and the wrong lifecycle/cleanup contract.
- Add a shell/native wrapper around the unmodified Host: does not repair bundle
  admission, runtime selection or packaged facts, and adds an unnecessary owner.
- Relax Team Identifier checks or derive publisher identity from a config string:
  crosses an authentication boundary.
- Add a trust store, local certificate authority or launchd/XPC service immediately:
  not necessary for the stated cooperating-developer local artifact; any stronger
  same-user or process-custody requirement must be separately justified and reviewed.
- Add generic platform traits or reuse Windows's archive/PE carrier: no second
  implemented consumer justifies the abstraction or that representation.
- Use cross-host signing for the first native build: Apple tools are present and
  authoritative; `rcodesign` remains future work, not a new dependency here.

No performance improvement is claimed. Existing development and publisher-backed
paths remain compatibility paths with their existing fail-closed contracts.

### 4.2 Explicit local artifact trust class

Introduce `LocalDevelopment` bundle provenance distinct from both an authenticated
development lease and an Apple-publisher identity. It establishes consistency of the
locally built package, its canonical application identifier and its selected inputs.
It does not authenticate an external publisher.

The artifact is suitable for a cooperating local developer who chooses the package.
Ad-hoc signing cannot prevent an attacker with the same user's ability to replace and
re-sign the entire app. No checksum inside that app solves this. The implementation
must neither claim that protection nor silently acquire release identity after re-signing.

Use an existing host-random nonpersistent WebView profile for this provenance. Browser
storage/prefs do not persist across launches in this first local mode. User documents
saved through the approved file workflow do persist. Do not use the default/global
store, fabricate an identity scope, or mark the package as an authenticated dev lease.
A content-stable persistent local profile is a separate future identity design.

Explicit Legacy is retained for draw.io; requesting Strict on an unqualified local
macOS child refuses. Legacy still has ambient Bun/OS authority outside brokered calls.
Host broker permissions, IPC authentication and generation revocation remain enforced.
Publisher-backed persistence, update signing keys and privileged updater authority
are not inferred from an intact seal or matching application ID.

### 4.3 Configuration and first build surface

Use `keld build --target macos-arm64` on native arm64 macOS. This first implemented
cell emits both .app and DMG with explicit local ad-hoc signing selected in
`keld.build.ts`. Unsupported targets/signing modes fail before creating output.
Do not add unfinished Windows/Linux commands or implementation branches.

Keep each authoring value in one file:

| File | Accepted local fields and owner |
| --- | --- |
| `keld.config.ts` | Existing root `name`, `entry`, `renderer`/ `initialWindow`, `auth`; integrate existing root `version` owner and add root `id` once. No nested `app` aliases. |
| `keld.permissions.jsonc` | Existing exact policy, including explicit profile; no new packaging grants. |
| `keld.build.ts` | Closed literal fields `target`, `signing`, `bundleVersion`, `minimumSystemVersion`, optional `icon`, `resources`, `host`, `bun`, `outDir`. |

The build subset has `target: "macos-arm64"`, `signing: "adhoc-local"`,
`resources: string[]`, and `host`/`bun` objects containing `path`, `sha256`,
`version`. Paths are build inputs only. No installed code reads `keld.build.ts`
or chooses binaries through those original paths. Prebuilt download/install
acquisition remains KEL-141's separate responsibility.

For this first command, the project root is the canonical invocation directory,
which must contain all three authoring files. There is no ancestor search or
configuration-file import. All relative build paths use that same project root;
installed paths are derived afresh from the verified bundle after relocation.

| Build field | Literal type; presence/default | Resolution and validation |
| --- | --- | --- |
| `target` | String; required, no default | Exactly `macos-arm64`, matching the CLI target and qualified binaries. |
| `signing` | String; required, no default | Exactly `adhoc-local`. |
| `bundleVersion` | String; required, no default | Apple's `CFBundleVersion` representation; no silent normalization. |
| `minimumSystemVersion` | String; required, no default | Apple's `LSMinimumSystemVersion` representation and the qualified binary floors. |
| `icon` | String; optional, absence means no custom icon | Project-relative regular `.icns` file; copied to the fixed icon destination. |
| `resources` | String array; required, may be empty | Normalized project-relative file/directory roots; no absolute or escaping roots. |
| `host`, `bun` | Objects; required, no default | Each has exactly `path`, `sha256`, `version`; no extra fields. |
| `host.path`, `bun.path` | Strings; required | Absolute or project-relative regular executable files; exact independent input qualification is required. |
| `host.sha256`, `bun.sha256` | Strings; required | Exactly 64 lowercase hexadecimal characters, matched against captured input bytes and independent qualification. |
| `host.version`, `bun.version` | Strings; required | Nonempty, at most 128 UTF-8 bytes, no control bytes; must exactly match the independently qualified input record. They do not select or download code. |
| `outDir` | String; required, no default | Absolute or project-relative new directory. It must not overlap a captured input/resource tree; no overwrite or reuse. |

A project-relative source `assets/icons/a.svg` is always installed as
`Contents/Resources/app/assets/icons/a.svg`; selecting `assets` does not flatten
its children. Reject duplicate or ancestor/descendant overlaps among explicit
resource roots. An implicit entry/renderer already covered by a declared root is
included once. The existing qualified producer owns provider and generated leaves;
user resources cannot overwrite its boot, exact policy, graph or canonical preload
destinations. Reject different sources mapping to one destination, including
case-equivalent names on the output filesystem, before copying. The existing graph
capture rules still govern path normalization, symlinks and special files.

`name`, `id` and application `version` are not duplicated in the build config.
Root `id` uses the existing pure canonical-ID predicate in `keld-wv`'s identity
owner; expose that predicate if required, rather than fabricating a publisher scope
to call `ProfileIdentity`. Also apply Apple's bundle-identifier constraints.
Application `version` must fit Apple's short-version representation; reject an
unrepresentable version rather than silently dropping a suffix. `bundleVersion`
is an explicit Apple build version, not a second application version.
`minimumSystemVersion` must not understate the requirements of either pinned executable.

Read each authoring file once with explicit bounds. Extend the existing strict
exported-root scanner/tokenizer in `hello/auth_config` for literal strings,
objects and string arrays. Do not introduce a TypeScript evaluator or copy the weaker
line-prefix scanner from `hello.rs`. Duplicate/unknown/ambiguous declarations,
expressions, path traversal, NUL/control bytes in metadata, noncanonical identifiers
and invalid versions fail with a typed diagnostic before assembly.

Preserve existing config semantics when moving consumers to the shared extraction
result. Changes to a supported caller are covered by its existing regression tests.
The destination `app: { id, name, version }` sketch in architecture 04 is not a
parallel accepted source in this slice.

The entry and renderer plus declared resource roots define the application input
tree. Expand directories through the existing graph capture rules: preserve qualified
relative module/worker layout, reject escapes and unsupported special files, and
record the exact inventory. Do not recursively copy the whole checkout by default.
Exclude development/test harnesses, .git, credentials and private evidence. Required
licenses/attributions remain resources. Native code discovered among resources must
be explicitly classified and placed in a standard code location or rejected; it
must never be silently treated as inert data.
