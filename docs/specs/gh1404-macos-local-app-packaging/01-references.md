# Packaging references

Part of: [index](index.md)
Owns: Source baseline and primary references

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
[`keld-patched-apps`
0c2541f](https://github.com/gyldlab/keld-patched-apps/tree/0c2541f445668cb3af1cdf58f2839282654b8582/apps/drawio/snapshots/2026-11-03).
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
