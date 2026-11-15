# Spec: local macOS application packaging: design

Part of: [index](../index.md)
Owns: Native packaging, signed graph, boot and platform design

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

### 4.4 Bundle and boot contract

| Content | Fixed bundle-relative location |
| --- | --- |
| Info.plist | `Contents/Info.plist` |
| Shipping Host / CFBundleExecutable | `Contents/MacOS/keld-host` |
| Pinned Bun | `Contents/Helpers/bun` |
| App entry, renderer, packages, resources | `Contents/Resources/app/` preserving relative layout |
| Generated boot and exact permission leaf | Within the same validated app resource root, using existing fixed filenames |
| Optional icon | `Contents/Resources/app.icns` |
| Actual supported native libraries | Standard `Contents/Frameworks` layout, only if independently qualified |

Emit `CFBundlePackageType=APPL`, correct executable, configured identifier/name,
short version/build version, minimum system version and optional icon. Escape XML
values and validate with `plutil`. Do not advertise document types, URL schemes,
background modes or entitlements whose consumers are not implemented.

The generated, sealed Info.plist also contains one closed `KELDLocalBundle`
dictionary. Its exact fields are `schema` (integer `1`), `target` (string
`macos-arm64`) and `bunSha256` (64 lowercase hexadecimal characters). Pack derives
the Bun digest from the verified, independently pinned input bytes; it is not a
free-form installed configuration or an external receipt. Missing, duplicate,
unknown or wrongly typed dictionary fields refuse. This small platform-specific
record supplies the local-origin discriminator and exact runtime pin; it is not a
second boot, permission or application-metadata language. The fixed Bun path stays
code-owned, not manifest-controlled. No final Host self-hash is stored in the bundle.

The shipping Host is the native launcher and event-loop owner. Extend the existing
opaque `ValidatedBootSelection` with a macOS-private origin, conceptually:

```rust
enum MacBootOrigin {
    DevStage(ValidatedDevStage),
    LocalDevelopment(ValidatedLocalBundle),
}
```

These are proposed internal shapes, not existing types or a new public constructor.
Only the Core verification factory can mint `ValidatedLocalBundle`. It retains the
canonical bundle/app/resource relationships, boot/policy objects, runtime identity
and local provenance. A boolean such as `skip_cleanup` or a user-provided bundle
path is not an admission capability.

Derive the origin from the running code object and its actual enclosing application.
An enclosing local-app candidate must pass this entire path or refuse; missing or
invalid local metadata must not fall through to development-stage selection. Require
the outer/main code's ad-hoc signature flag for this local schema. A nested Bun
signature or Team Identifier cannot establish the outer app's provenance. Existing
development-stage and Apple publisher selection remain separate; this schema does
not activate a new Developer ID application path.

Use Apple's dynamic validation for the running executable and full static validation
of the enclosing bundle, including nested code, resources, strict structure and every
present architecture. Do not use resource/executable-skip flags. Validate the expected
fixed Host/Bun relationship, `CFBundleExecutable`, canonical equality of the outer
signing identifier and `CFBundleIdentifier`, and supported target/version. Compare
the opened Bun bytes with sealed `bunSha256`, even when its signature validates.
Apple's nested-code identity is not a substitute for that exact digest. Complete
these checks before a listener, child or app window exists. A naked copied Host refuses.

Then feed the existing closed boot parser, exact opened permission bytes and existing
GuardSnapshot path. Do not define a second boot or permission language. A minimal
native app uses supported boot schema; draw.io consumes its separately committed
qualified graph/facade schema. Unknown schemas stay errors.

Apple's static validation is not a lock against future mutation. Retain opened roots
and boot/policy objects, read bounded bytes from those same objects, compare the
qualified graph/runtime identities at their existing handoff points, and recheck each
new Bun generation. Reject symlink/path substitution and changed identity at those
boundaries. Same-user mutation after validation is not cryptographically prevented
by this local artifact; no race-free or hostile same-user guarantee may be claimed
from an API call alone. Dynamic resource access remains governed by the applicable
existing graph and permission contracts.

The app bundle is immutable during operation. Runtime state uses existing approved
state locations; no cache, lock, socket, temporary stage or user document is written
inside the signed bundle.

Executability is a separate package predicate, not a signature property. Pack
requires regular executable Host/Bun inputs without set-user-ID or set-group-ID
bits, and emits both code files with POSIX mode `0755`. Validate the final files'
exact mode and regular-file kind before publication. Core also checks those facts
for installed admission, and Runtime rechecks Bun for each generation; unusable
execution (including an ACL denial) remains a typed launch failure. No signature
or digest success can waive that check. A copied Host with all execute bits removed
cannot run to reject itself: Launch Services refusal is its installed oracle.

### 4.5 Runtime and installed application facts

Keep the existing one-use Guardian registration, host-retention acknowledgement,
Bun process group, generation owner, link revocation, recovery and shutdown ordering.
No new supervisor or launcher process is added.

After authenticating its bootstrap, the Guardian independently derives the same
validated local bundle, checks the parent-selected entry/runtime identities, and
executes the absolute bundled Bun. Existing environment sanitation and qualified
canonical preload remain mandatory. No PATH, BUN_OPTIONS, ambient bunfig, developer
Bun or source-tree fallback is allowed.

The first installed cell also selects an explicit empty TypeScript configuration
with `--tsconfig-override=/dev/null`. Application `tsconfig.json` path aliases must
not redirect the fixed bootstrap or verified entry. Such aliases must be resolved
before packaging; ordinary relative imports and package exports remain supported.
Development launch behavior is unchanged. Real Bun controls cover wildcard and
exact preload/entry aliases, including removal of the override as a negative control.

The transport remains a separately packaged canonical `kipc-transport.ts` or
`.js` with the existing release build/stamp requirements. Do not inline it into
entry code or use `bun build --compile` to bypass that contract.

Cleanup is a capability selected by verified origin: DevStage may delete its exact
owned nonce under existing rules; LocalDevelopment retires owned processes and
runtime state but has no authority to delete the installed bundle. Failures cannot
turn that capability into an arbitrary path janitor.

Extend the existing `launch_facts` owner and closed codec with packaged provenance.
Do not call `DevelopmentLaunchFacts::from_validated_development` for a lease-less app.

| Observable | Local installed value |
| --- | --- |
| `app.isPackaged` | `true` from verified bundle provenance; packaging state is not publisher trust |
| `app.getAppPath()` | Verified `Contents/Resources/app` |
| `process.resourcesPath` | Verified `Contents/Resources` |
| `process.execPath` | Actual bundled Bun executable, preserving Bun/Node semantics |
| `process.argv` | Actual `[bundled_bun, packaged_entry]` in the first zero-user-argument cell |
| `process.defaultApp` | `true`, coupled to that argv shape; never inferred as `!isPackaged` |

The existing GH-570 argument-slicing contract yields an empty user-argument list.
The Host currently rejects general user arguments; forwarding, associations and
open-document delivery remain separate work. Native menu Open satisfies the present
draw.io workflow. Main-role facts must arrive through the existing authenticated
bootstrap before app imports; config/environment/renderer messages cannot mint them.

**Bounded implementation note (2026-11-06):** reuse the closed development codec
from PR #1362 at `c2dd2ff53767adf20dc0e91d276310095fe31acf` and the standalone
process-property owner from PR #1363 at
`9b6b4b495d1e8d060cc8a189ade9cc87c23edd0e`. This approved section supplies the
installed behavior; it does not approve or integrate the rest of PR #1363's draft
development-spawn proposal. Private schemas 1–3 retain their development meaning.
The independent immutable original-content snapshot already uses schema 4 for
development single-instance data and schema 5 for original-resource policy data.
The installed variant therefore uses schema 6 with the exact fields
`schema`, `kind: "local-development"`, `name`, `root`, `version` and
`resourcesRoot`. This bounded consumer refuses schemas 4/5, unknown fields,
mixed kinds and missing installed fields; it does not reinterpret those records.

The authenticated Guardian derives and retains this record from its independently
validated local bundle. The existing private 7-argument development and 11-argument
`local-bundle-v1` Guardian shapes remain unchanged. The existing parse-time
`__KELD_HOST_LAUNCH_FACTS__` constant carries data to Bun. The historical macOS
development path explicitly defines its absence. No environment authority or
application IPC channel is added.

One Core source list supplies three fixed generated files below
`keld-launch-facts/`: the intact KIPC launch-fact codec, the intact Electron
process-facts owner and their tiny preload composition, with their relative package
layout preserved. Installed admission compares all three against the Host's compiled
owner bytes, retains their file identities and repeats validation before each Bun
generation. The existing transport generator embeds the same standalone codec into
the stamped canonical transport, so development/template sidecar layout stays
standalone. The installed preload invokes the process-property owner before app
imports; a later facade import is idempotent. This slice does not provide the original
application graph or installed draw.io workflow acceptance.

Known existing limitations include simultaneous Host+Guardian death and detached
descendants outside the owned process group. Qualify actual owned-family cleanup
with causal tests. If any required no-abandoned-process criterion fails, keep A13
open and route its minimum resolution to the existing runtime owner; do not silently
waive it or add launchd/XPC architecture without review.

### 4.6 Original application graph and signing order

The approved original-content path is not an ordinary project option. Reuse its
existing provider/package/lock/graph/CSP validators under an explicitly reviewed
local-bundle producer caller. Qualification facts must come from the independent
integration owner, not hashes computed from the same unqualified project input.

Required predecessor: immutable committed provider/boot/launch-fact integration,
exact input pins, relevant review/gates and an independently established Host/provider
qualification record. The dirty integration tree and developer ZIP are leads only.

The D1 input Host SHA check is producer-only; it is not serialized into boot/graph.
Graph entries exclude Host, boot, policy and the graph leaf under their existing
separate ownership. Preserve this arrangement and avoid a self-hash cycle:

1. Verify independently qualified pre-sign input Host/provider/app/runtime identities.
2. Capture the resource graph and compile existing boot/policy bytes.
3. Assemble into a newly created, task-owned staging directory; retain exact input
   identity and detect changes during capture.
4. Preserve the qualified Bun input byte-for-byte, including its existing signature,
   flags and entitlements, and verify it in its final nested location. Any additional
   independently qualified framework code needing a signature is signed explicitly
   inside out with its reviewed properties, then verified. Reject unsigned or invalid
   Bun inputs rather than inventing a new Bun signing recipe in this slice.
5. Sign the containing application last, using native `/usr/bin/codesign`.
6. Verify the entire bundle recursively and inspect its resulting signature facts.
7. Record post-sign Host/Bun/app identities in an external artifact receipt.
8. Independently qualify those exact final signed bytes with native app/security tests.
9. Construct the DMG only from the verified final app, verify it, then publish outputs.

Do not separately sign the main executable just to sign it again as the app; the
containing app operation signs its main code. Do not use `codesign --deep` for
signing. Recursive `--deep --strict` verification is allowed.

Local outer signing uses `--sign -` and `--timestamp=none`; no timestamp service or
paid identity is required. Nested code must already have a valid signature before
the containing bundle is signed. The first pinned Bun satisfies that requirement
without re-signing. Record its existing flags and entitlements; do not copy its
exceptions into the Host. The outer app is locally ad-hoc signed even though this
nested runtime retains its vendor signature. Do not claim Hardened Runtime for the
whole app from Bun's flag. Changing Bun signing properties is a separately qualified
input change, not a build fallback; public hardened/notarized distribution remains
KEL-103/future scope.

Example command forms, not executed acceptance:

```bash
/usr/bin/codesign --verify --strict --verbose=2 "$keld_app/Contents/Helpers/bun"
/usr/bin/codesign --sign - --force --timestamp=none "$keld_app"
/usr/bin/codesign --verify --deep --strict --verbose=2 "$keld_app"
```

The real implementation also compares the pinned Bun digest before sealing the
generated local record. It derives identifiers from configuration and passes
arguments as separate process arguments. Tool failure invalidates the candidate; never
publish success from a partial output. No global shell interpolation of metadata.

### 4.7 DMG, output identity and reproducibility

Use a new task-owned DMG staging directory with the final app and an `Applications`
symlink to `/Applications`. That external symlink is outside the signed app.
Use native `hdiutil create -srcfolder ... -format UDZO`, verify with
`hdiutil verify -nocache`, attach read-only with plist output and verify, copy with
`ditto`, and detach the exact observed device. Never guess a disk number, use
`-ov` to overwrite user artifacts, or delete a caller-chosen existing directory.

Output publication succeeds only after the .app signature and DMG integrity pass.
The final output directory must be new. Failure removes only staging created and
owned by this invocation, retaining requested diagnostics. Interruptions must not
leave a partial final artifact looking successful.

An external receipt records repository and application commits, disclosed app edits,
exact provider/lock/Host/Bun hashes, permissions/profile, target, tool/OS versions,
signing flags/entitlements, per-file modes/link targets/digests, post-sign code
identities, final DMG SHA-256, command outcomes and native evidence references.
It contains no credentials, environment dump or private test data. This receipt is
provenance/evidence, not runtime authorization.

Compare two builds from the same frozen inputs. Require identical semantic content
inventory and normalized metadata where specified. Record signed-byte and DMG-byte
equality independently; filesystem/image metadata can differ, and no Apple
byte-reproducibility guarantee is assumed. Do not hide differences behind an
unqualified “reproducible” claim.

### 4.8 Future Windows/Linux boundaries — design only

Both future issues are parked, unassigned and explicitly
**STATUS: DESIGN ONLY / IMPLEMENTATION DEFERRED.** They authorize no code, adapter,
dependency, pipeline or launcher. A platform trait is unnecessary before a second
implemented consumer demonstrates shared behavior.

Reuse owners and data semantics, not filesystem layouts. Name, canonical application
ID, version/target, entry/resource identity, exact permission bytes, profile and
artifact identity are reusable data. Core/Host own boot, Guard owns authority,
Runtime owns process custody, Pack owns assembly and CLI owns orchestration.
`ExpectedAppIdentityPayload` is a bounded existing codec for ID/channel/target/update
key, not an authenticator; a local no-update app does not need that unrelated key.
Its Windows PE carrier and Apple bundle sealing remain platform-specific. Current
Windows update archives do not establish Unix mode/link representation (KEL-137).

**Windows / GH-1405.** Consume the existing authenticated image → expected identity
→ installation provenance → selected boot tree route (KEL-135/KEL-254/KEL-53).
An EXE signature alone does not authenticate its neighbors. Preserve the existing
post-sign container checks and Runtime Job/generation owners. KEL-122 owns no-console
launch. Decide per-user/machine and updater ownership before choosing an installer;
MSI/NSIS/MSIX are not selected here. Define WebView2 architecture, presence, offline
provisioning and servicing. Microsoft distinguishes shared automatically serviced
Evergreen from app-delivered Fixed Version; Edge browser is not its production
substitute. Actual application readiness comes from #1182/#1183, not this Mac result.

**Linux / GH-1406.** Specify the authenticated installed-root successor before ordinary
installed launch. Qualify the real dependency graph: current pinned Keld uses
GTK3/WebKitGTK 4.1/libsoup3 through wry 0.56.1; GTK4/WebKitGTK 6.0 is a separate runtime
migration. Freeze distribution/libc/architecture/engine/display-session and sandbox
feature cells. Preserve Runtime's strict helper/Bubblewrap/seccomp/Landlock refusal
rules; no silent Legacy fallback. Choose native/portable/container delivery from
that matrix; deb/rpm/AppImage/Flatpak remain open. Derive desktop metadata and
correctly quoted Exec from config; declare D-Bus activation only if implemented.
XDG locations are state integration, not application identity. The Linux trust
authority and installed-integrity mechanism remain an explicit future decision.

**Future acceptance and dependencies.** Each needs its own approved spec and real OS
run: clean installed launch; runtime/dependency absence; executable/ID/policy/resource
substitution; actual process cleanup/relaunch; original app workflows and artifact
identity. Windows adds WebView2 absence, provenance/DACL/reparse and no-console
checks. Linux adds exact engine ABI, missing helper/kernel features, mode/link
preservation, desktop integration and every claimed distribution/session cell.
Reject mocked or foreign-OS receipts. #1405 maps #1182/#1183 and KEL-19/122/135/254/53/75/78.
#1406 maps #1124 and KEL-96/135/75/78/79/82. #1404 supplies reviewed boundaries/evidence
only; macOS source-specific code is not an assumed portable foundation. KEL-137/KEL-53
become dependencies only if canonical updates are selected later.
