# Installed graph and artifact design

Part of: [index](index.md)
Owns: Design sections 4.5–4.8

## 4. Design: installed artifacts

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

