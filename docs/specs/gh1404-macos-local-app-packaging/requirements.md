# Spec: local macOS application packaging: requirements

Part of: [index](index.md)
Owns: Source contracts, acceptance criteria and primary documentation

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

| ID | Required observable |
| --- | --- |
| A1 | One framework path builds two configured macOS apps, including a minimal fixture and original draw.io, without app-specific framework branches. |
| A2 | Missing/invalid metadata, Host, Bun, entry, permission file or resource, unknown config, wrong target and escaping paths fail before final output publication. |
| A3 | The copied .app launches through Launch Services with no terminal, checkout, Cargo workspace, external Bun, dev lease, test flag or private test credential. |
| A4 | Installed selection validates the enclosing bundle and exact boot/policy/runtime relationship before listeners, Bun or application windows. Invalid ID/boot/manifest and unsigned or mutated contents refuse. |
| A5 | Local provenance cannot select a publisher-backed persistent profile, Strict containment or updater authority. The existing Apple publisher path and dev path retain their own rejection tests. |
| A6 | The exact existing Guard manifest is enforced. A denied authenticated capability/resource request has no prohibited side effect. Legacy's ambient OS authority is explicitly disclosed, never called Strict. |
| A7 | Actual installed launch facts precede application imports: correct packaged state, app/resource roots and Bun executable/argv semantics; forged or missing facts fail. |
| A8 | Bundled Bun is selected by validated absolute identity. Missing/replaced Bun and hostile PATH/preload inputs cannot launch a substitute. |
| A9 | Each nested executable verifies before the containing bundle is signed; the final outer application has a verified ad-hoc signature. Pinned Bun retains its valid input signature and bytes. Independent executable-content/resource mutations fail signature verification; executable modes have separate package/admission checks. |
| A10 | The DMG verifies, mounts read-only, contains the app and Applications shortcut, supports copying, and detaches using the observed device identity. |
| A11 | Installed app works after renaming/relocation and from unrelated cwd. No source/resource/runtime access escapes to the checkout or developer cache. |
| A12 | Original draw.io passes Launch → Open → Edit → Save → unsaved Close → Cancel → Save → Reopen → Discard → Quit, with native observations and saved-file readback. |
| A13 | Normal quit/relaunch, Bun failure/recovery where applicable, Host failure and owned-child cleanup pass using exact process identities. No installed files or user documents are removed by cleanup. |
| A14 | Pre-sign input qualification and independent post-sign final-artifact qualification identify exact Host/provider/app/runtime bytes; no historical receipt silently transfers. |
| A15 | A rebuild comparison states exactly which content is reproducible; final app/DMG digests and tool/source/runtime identities are retained. Byte-identical DMGs are claimed only if demonstrated. |
| A16 | Applicable format, warning-denied Clippy, Rust/TypeScript/native/CI checks and independent review pass on exact implementation heads. Future-platform design/issue mapping is present, with no implementation. |

A failed required cleanup, trust, graph or installed workflow criterion blocks this
mission's completion. A narrower passing subset must not be relabeled full acceptance.
Second-machine acceptance remains separately unverified when unavailable.

### AC-1: Local signed packaging retains exact source identity

- **Given:** The project declares the Legacy profile and independently pinned Host, Bun, source and permissions inputs.
- **When:** Pack assembles, signs and verifies the macOS app and DMG.
- **Then:** The installed bundle and build receipt bind the admitted inputs without asserting unqualified Strict containment.
- **Negative control:** Change one bundled input after signature verification while keeping the original pins.
- **Single fault:** Replace one signed app resource after the expected receipt is captured.
- **At-risk precondition:** The signed artifact is selected for installed first launch.
- **Expected RED:** The installed signature and byte verification refuse the modified artifact before launch.
