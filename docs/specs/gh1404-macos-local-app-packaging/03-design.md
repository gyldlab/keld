# Packaging design and bundle

Part of: [index](index.md)
Owns: Ownership, configuration and bundle admission

## 4. Design: ownership and bundle

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

Capture and independently qualify the complete executable dependency closure, including
package trees and worker modules, through the same graph owner. A package name or
lockfile entry alone is not a qualification receipt. Assembly must not rely on runtime
downloads or the installation location's ancestor `node_modules` to complete that closure.

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
