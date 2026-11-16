# macOS DMG and portability
Part of: [index](index.md)
Owns: DMG, provenance and deferred platform design

## 4. Design: Distribution
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
