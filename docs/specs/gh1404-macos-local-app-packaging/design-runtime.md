# macOS installed runtime and original graph
Part of: [index](index.md)
Owns: Boot, signing, runtime and resource graph

## 4. Design: Installed runtime
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
with causal tests. If any required no-abandoned-process criterion fails, keep AC-13
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
