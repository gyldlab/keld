# Spec: host-enforced guard sessions

Status: draft
Linear: KEL-102 · Owner: GYLDLAB · Updated: 2026-08-28
Decision state: delegated candidate selections recorded; authenticated human approval pending
Decision digest: `sha256:868aaef5991a03ce4e394943d96f6ead62e725444ad66280a906a60830f862f3`

## 1. Goal & non-goals

Make `keld-guard` an actual host-enforced policy boundary for every live
privileged Keld call: the host loads one host-selected and digest-verified `keld.permissions.jsonc`
snapshot before an app session starts, derives the caller principal from its
own authenticated link or webview state, evaluates the guard before the
native handler, and returns a typed denial without a handler side effect.
The first vertical slice is host-brokered `fs.read` / `fs.write`; webview media
receives the same host snapshot but remains default-deny until window grants
are implemented.

This is a permission-model change. It does not claim that Bun itself lacks
ambient operating-system authority: KEL-78 owns strict-profile admission and
the OS proof for that separate layer.

Non-goals:

- Implementing this specification in this documentation PR.
- Changing `docs/architecture/03-security.md`, the kipc frame format, or the
  current `KELD_APP_LINK=<endpoint>#<64 hex chars>` contract.
- Adding `roles`, window grants, channel grants, manifest generation,
  dev-permissive mode, a denial recorder, or manifest linting. Those are
  destination work and must not be faked by applying `/app` grants to another
  principal.
- Adding a second guard engine, per-handler policy parser, or an in-process
  permission bypass.
- Claiming OS sandboxing, direct-Bun filesystem denial, or support for native
  modules beyond the live `fs` broker.
- Live permission reload. v0 intentionally uses a snapshot per host session.

## 2. Spec refs

- `docs/architecture/01-overview.md` §§3–4: host/core ownership and process
  model.
- `docs/architecture/02-ipc.md` §§1–2: host-mediated links, host-minted link
  metadata, authenticated `HELLO`, and the rule that identity is not a frame
  field.
- `docs/architecture/03-security.md` §§1–4: default-deny, manifest, guard
  before privileged handlers, and distinct broker versus OS-sandbox layers.
- `docs/architecture/05-webview-and-native.md` §3: native modules are
  host-owned, guarded kipc brokers.
- `docs/architecture/06-runtime-and-tooling.md` §§1–2: supervised Bun roles
  and the `keld dev` / `keld-host` boundary.
- `docs/specs/kel75-principalized-bun-child-roles.md`: destination role
  generations and role grants; KEL-102 does not duplicate that registry.
- `docs/specs/kel78-strict-profile-sandbox.md`: strict-profile admission is a
  separate, fail-closed OS-containment gate.
- `docs/specs/kel96-no-flag-host-boot.md`: the prerequisite no-flag host boot
  artifact and process-lifecycle owner.

This spec makes the destination enforcement stated in architecture 03
executable; it does not change the architecture contract. Architecture LIVE /
TARGET labels move only in the implementation PR that proves the relevant
acceptance criteria.

### 2.1 Approval-candidate decision record

The user delegated selection and preparation of the nine decisions to the
specification writer. That delegation authorizes this review candidate; it is
not the distinct-human approval required to change `Status: draft`, complete
T0, or emit a passed contract artifact. The canonical decision payload is the
single minified UTF-8 JSON line below, without its trailing newline. Its
SHA-256 is recorded in the header.

```json
{"schema":"keld.kel102-decisions/v1","permission_model":"approved:immutable-host-session-snapshot;fail-before-listener-child-window;no-load-error-fallback","public_api":"approved:verified-loader+run_guarded+media-policy;guard-owns-read-hash-parse;private-guard-context","kel96_dependency":"KEL-102/T2-consumes-KEL-96/T1a-acceptance-from-atomic-T1a+T1b-head","identity":"approved:accepted-v0-app-link-to-AppProcess;reject-caller-selected-identity","dispatch":"approved:evaluate+dispatch_privileged-only","task_order":"KEL-102/T2->KEL-102/T3->KEL-102/T4->KEL-102/T5","kel97_predecessor_task_id":"none","containment":"KEL-78-complementary-not-blocker-or-substitute;no-strict-release-claim","reload":"excluded-until-revocation-before-reprovision-spec","review_gates":"permission+public-api-required;unsafe+dependency+kipc-wire-absent-unless-separately-approved"}
```

| ID | Delegated candidate selection |
|---|---|
| `KEL-102-D1` | Approve one immutable host-owned policy snapshot per application session. Invalid boot or policy input fails before listener, child, or window creation. A load failure never becomes an empty/default policy; valid `{}` remains a deliberate all-deny policy. |
| `KEL-102-D2` | Approve the exact public APIs in §4: `keld_guard::verified_manifest::load_verified_manifest`, `keld_core::app_session::run_guarded`, and the `keld_wv::MediaPolicy`/`WebEngine::create` injection seam. `GuardSnapshot`, `TrustedDispatchContext`, raw policy bytes, and manifest fields remain private. |
| `KEL-102-D3` | `KEL-102/T2` consumes the `KEL-96/T1a` acceptance row from one landed atomic T1a+T1b `host-boot-and-session` artifact. It does not accept the stale standalone `host-boot-descriptor` terminal and does not depend on KEL-96 T2-T5. |
| `KEL-102-D4` | One host-accepted authenticated v0 app link maps to `Principal::AppProcess`. Frame data, token text, PID, environment, working directory, role name, and Electron options cannot select identity. Role generations remain KEL-75/KEL-97. |
| `KEL-102-D5` | The `keld-native` broker is the sole production caller of `keld_ipc::guard_dispatch::dispatch_privileged`, which is the sole caller of `keld_guard::evaluate` before its OS-operation closure. `keld-core` resolves trusted context, decodes, validates, and routes; it never evaluates or adds a second authorization check. |
| `KEL-102-D6` | Freeze the order `KEL-102/T2` verified load/snapshot → `T3` reachable filesystem vertical → `T4` webview media injection → `T5` role-generation binding after KEL-97. Set `kel97_predecessor_task_id=none`: KEL-97 owns role-link identity independently; KEL-102/T5 consumes KEL-97, never the reverse. |
| `KEL-102-D7` | KEL-78 OS containment is complementary to broker authorization, neither a blocker nor a substitute. KEL-102 alone cannot support a strict-profile or release-containment claim. |
| `KEL-102-D8` | Controlled reload is outside this specification until a separately approved revocation-before-reprovision contract exists. |
| `KEL-102-D9` | Permission-model and the exact public APIs above require authenticated human approval. This spec authorizes no unsafe, dependency, kipc-wire, or manifest-schema change. A T2 `sha2` dependency request and any later unsafe/wire/schema delta require their own explicit review gate. |

## 3. Acceptance criteria (binary, each becomes a test)

1. Given an app launch with a host-minted `ValidatedBootSelection`, when
   `keld-host` starts a privileged app session, then it resolves exactly
   `<app-root>/keld.permissions.jsonc` from the artifact's canonical app root,
   verifies the manifest bytes against that selection's fixed digest, and parses
   the file once before it creates a Bun child, app-link listener, or window.
   KEL-96's owner-controlled dev stage proves byte consistency but is not
   release authentication; a release claim additionally requires the
   KEL-103/successor container-to-root trust chain.
   The Bun child, a webview, a kipc frame, and a working-directory change
   cannot select the root, path, or manifest bytes.
2. Given a missing, unreadable, malformed, traversing, outside-root, or
   integrity-mismatched manifest, when a privileged app session is requested,
   then the host returns a typed error with the corrective action, creates no
   privileged listener, starts no Bun child, leaves the host window registry
   empty, and creates no platform-native application window handle. Absence of
   a window-ready marker alone is not evidence that no window was created.
   `KELD-GUARD004` / `KELD-GUARD005` retain their existing actionable text;
   an integrity failure is `KELD-GUARD016` and tells the developer to rebuild
   or re-sign the boot artifact. The host MUST
   NOT substitute `PermissionsManifest::default()`.
3. Given a valid v0 authenticated app link, when its `HELLO` is accepted by
   the host, then the host binds that accepted link to the sole
   `Principal::AppProcess` dispatch identity. Decoded frame/payload values,
   the endpoint, token, PID, environment, and child-provided role name cannot
   select or replace that identity. A foreign or stale link is rejected before
   a privileged request is decoded or dispatched.
4. Given a destination KEL-75 role link, when its authenticated generation is
   bound by the host registry, then the host obtains its guard principal from
   that binding and its declaration, never from a frame. A revoked generation
   has no dispatch context and cannot reach a handler. The role-grant data
   model is KEL-75 work; this criterion is the KEL-102 integration boundary,
   not permission inheritance between roles.
5. Given a webview permission request, when the host routes it to a media or
   future privileged handler, then it derives `Principal::Webview { id,
   generation }` from the host webview/navigation registry. Missing registry
   state fails closed as `KELD-GUARD007`; an app-link principal never stands in
   for a webview. Until window grants ship, a loaded manifest still does not
   allow webview media through `/app` grants. The callback's recorded policy
   digest must equal the session snapshot digest, so the test fails if any
   backend retains `PermissionsManifest::default()` instead of the injected
   verified policy.
6. Given an authenticated `fs.write` request outside the loaded
   `app.fs.write` scope, when the host dispatches it, then it returns the
   original typed `KELD-GUARD002` denial and the target file is not created or
   changed. Given an in-scope request, the same real host broker writes the
   requested bytes and returns its normal reply. A test-only recorder at the
   production filesystem-adapter entry shows that a denied `fs.read` attempt
   never enters the adapter.
7. Given a privileged `Call` from an accepted caller, when the host has
   decoded and validated it, then the dispatch order is: resolve trusted
   caller context; derive operation/resource; call
   `keld_ipc::guard_dispatch::dispatch_privileged`; only on `Allow` invoke the
   native handler. Echo and the existing host-lifecycle control frames remain
   explicitly ungated because they do not have OS authority. Any new native
   capability lacking this registered path is unavailable, not directly
   callable.
8. Given a running host session, when the manifest file changes on disk, then
   its current policy remains the immutable startup snapshot. `keld dev` or a
   production update must end the app session, revoke its app links and
   principals, and start a fresh host session before the new manifest can take
   effect; it MUST NOT reread the file per request or upgrade a live role.

## 4. Design

### First-principles and reuse decision

**Current facts at `origin/main` `1609dbd`:**

- `keld_guard::load_manifest` already supplies JSONC parsing and typed
  missing/read/parse failures. Nothing outside guard tests calls it. It does
  not yet let a caller verify the exact bytes it parses, so a production
  content-digest check needs a single-read extension in `keld-guard`, not a
  host-side read followed by a second parse/read.
- `keld_guard::evaluate` already default-denies ungranted/out-of-scope app
  operations and rejects non-app principals before grant lookup. It is a pure
  evaluator, not a process-global policy owner.
- `keld_ipc::guard_dispatch::dispatch_privileged` already evaluates first and
  invokes its closure only for `Decision::Allow`; its unit and real-session
  tests prove the existing side-effect boundary.
- `keld_native::fs` already calls that helper immediately around
  `std::fs::read` / `std::fs::write`, but no shipping crate depends on
  `keld-native`. Its session is therefore not reachable from the app host.
- `keld-core` declares a `keld-guard` dependency but does not reference it.
  `HostOwnedHelloSession` currently mints one authenticated echo link and
  starts Bun; its `EchoServer` dispatches an intentionally unprivileged echo
  session.
- `keld-host` has no no-flag boot path yet. It only opens the diagnostic
  `--hello` window or prints its pre-alpha banner.
- Current webview backends mint a host-assigned media `WebviewId`, but pass
  `PermissionsManifest::default()` to the permission callbacks. Media is
  safely deny-only today; it is not policy loaded for an app session.
- KEL-75 has host-owned role generations and authenticated Unix bootstrap
  links, but `keld_runtime::RolePrincipal` is not wired to
  `keld_guard::Principal` or a privileged dispatcher. KEL-78 admission is
  likewise independent of call authorization.

**Ownership, trust, lifecycle, I/O, and failure facts:**

| Fact | Decision |
|---|---|
| Policy-file authority | The `keld-host` process owns selection and the immutable policy snapshot. A single `keld-guard` verified-load operation owns reading, byte verification, and parsing. `keld-core` receives the snapshot as trusted session state; Bun and webviews receive neither a path nor a mutable manifest. |
| Manifest path and trust | The exact filename is `keld.permissions.jsonc` beneath the canonical app root. KEL-96's dev stage is owner-controlled and its digest proves byte consistency, not release authenticity. A future production boot artifact must authenticate its relative location, content digest, and container-to-root relationship through KEL-103 or an approved successor. In both cases the host selects the exact relative filename and rejects a missing, escaping, or non-regular file; a child payload never selects it. |
| Caller identity | The host creates the dispatch context after link authentication or from its webview/navigation registry. v0 maps its only accepted app link to `AppProcess`; destination role bindings come from KEL-75. No wire field, endpoint string, token, PID, environment variable, or Electron option is identity. |
| Permission decision | `keld_guard` remains the one evaluator and `keld_ipc::guard_dispatch::dispatch_privileged` remains the one guard-before-handler helper. `keld-core` owns routing and supplies trusted context; `keld-native` owns the OS operation and calls the shared helper immediately before it. |
| Side effect | Request decoding and resource extraction are not OS side effects. Normalization required by a future scope matcher happens before evaluation but must not open/create the target. The native OS operation is entirely inside the helper's `Allow` closure. |
| Lifecycle | The snapshot is constructed before app resources exist and lives for one host session. Changes require explicit full-session teardown and fresh links/principals; there is no reload API in v0. |
| Failure | Bad policy prevents a privileged app session from starting. Bad link identity prevents dispatch. Guard denial returns the existing typed reason and leaves the handler unentered. OS failures after an allow retain the native typed error. |

**Reuse:** do not write another manifest parser, matcher, decision cache, or
per-native-module policy check. Reuse and extend the existing `load_manifest`
owner with a single-read verified-load variant, then reuse `evaluate`,
`dispatch_privileged`, the authenticated app-link bootstrap, and the
host-owned webview id. The named unmet requirement for that small extension is
that a release host must verify and parse the same file bytes; host-side
verification followed by `load_manifest` would create a time-of-check to
time-of-use gap. The broader unmet requirement is ownership and wiring: no
shipping host session currently gives those shared primitives trusted policy
and principal inputs or routes to a live native broker.

**Rejected alternatives:**

- Loading a manifest in Bun or accepting its path in a CALL makes the
  semi-trusted caller the policy authority.
- Reading the manifest for every request makes policy changes race a live
  principal and adds filesystem I/O to a hot path.
- Falling back to an empty manifest after a load error obscures a broken
  release and permits a later careless handler to confuse configuration
  failure with a deliberate deny.
- Adding a core-local `if allowed` helper or letting each handler parse policy
  duplicates the single enforcement rule and will drift.
- Treating KEL-78 strict admission as a substitute for a host broker check
  confuses OS containment with per-capability authorization.

**Compatibility fallback:** diagnostic `keld-host --hello`, `keld hello`, and
standalone IPC diagnostics may remain unprivileged tools without a manifest.
They MUST NOT expose a native broker, mint an application principal, or become
an alternative app-session owner. There is no permissive fallback for an app
session with privileged APIs.

**Performance:** no performance claim or new steady-state allocation is
introduced. The manifest is read once on the cold host-start path. The
existing guard's allow path remains the hot-path evaluator; a later PR must
benchmark if its context representation changes allocations or lock behavior.

### Session policy source and loading

`keld-host` begins a privileged application session in this order:

1. Validate its boot input before starting a listener, window, or Bun. KEL-96
   owns the boot-artifact format and returns an opaque `ValidatedBootSelection`
   containing a canonical `app_root` plus the fixed manifest descriptor/digest.
   The owner-controlled dev stage may enable guarded brokers without claiming
   release authenticity. A release session cannot enable them until KEL-103 or
   an approved successor authenticates the exact sidecar, location, and root
   relationship; there is no caller-selectable trust-mode downgrade.
2. Resolve `app_root / "keld.permissions.jsonc"` under the canonical root.
   Reject an absolute/escaping descriptor, symlink escape, non-regular file,
   or unreadable file. Open it once with KEL-96's no-follow contract, validate
   identity/containment on that handle, and transfer the handle to the loader;
   do not validate a path and reopen it. The implementation must make this
   resolution a single host-owned routine; callers must not reimplement it.
3. Call the exact guard-owned single-read verified loader below. It reads the
   resolved file once into one private owned byte buffer, computes SHA-256 over
   that buffer, compares it with the decoded expected digest, validates UTF-8,
   and parses a borrow of those same bytes. It must not verify a host read and
   then call `load_manifest` on the pathname again. The buffer is dropped
   before return and is never exposed to the caller. Keep the resulting opaque
   `VerifiedManifest`—manifest and digest as one inseparable value—in an
   immutable `GuardSnapshot` held by the host/core session.
   Neither the snapshot, raw bytes, nor manifest path is sent on kipc.
4. Only after these steps succeed may the host create the authenticated
   app-link, bind a webview, or spawn Bun. A blank but valid `{}` manifest is
   a deliberate all-denied policy; a missing file is a startup error, not a
   blank policy.

For v0 development, `keld create` / the project fixture must provide the
explicit `{}` file. `keld dev` passes a canonical project root to the host,
not a manifest pathname, so a developer can alter their own declared
authority but an app child cannot. The destination signed release path applies
the boot-artifact content-digest check to the same fixed file.

### Verified-loader and host-session public API

T2 adds one public module file,
`crates/keld-guard/src/verified_manifest.rs`, exported at the exact path below:

```rust
// keld_guard::verified_manifest
#[derive(Clone)]
pub struct VerifiedManifest { /* private manifest + verified SHA-256 */ }

impl VerifiedManifest {
    pub fn manifest(&self) -> &keld_guard::PermissionsManifest;
    pub fn verified_sha256(&self) -> [u8; 32];
}

pub fn load_verified_manifest(
    file: std::fs::File,
    display_path: std::path::PathBuf,
    expected_sha256: [u8; 32],
) -> Result<VerifiedManifest, keld_guard::ManifestError>;
```

The module and function are `pub` because `keld-core` is the single production
caller. `keld-core` opens the fixed permissions file once with the KEL-96
no-follow/regular-file/containment contract and transfers that validated handle
by value. `display_path` is diagnostics-only and is never opened. The loader
owns the handle read, byte buffer, SHA-256 calculation, comparison, UTF-8
validation, and parse. The caller owns only the validated handle, display path,
and expected digest from `ValidatedBootSelection`; it cannot provide a verifier
callback, substitute already-read bytes, or recover the loader's raw bytes.
`VerifiedManifest` has no public constructor or mutable/raw-byte accessor, so
its manifest and digest cannot be paired independently. Existing
`load_manifest(&Path)` remains available for unprivileged
diagnostics but MUST NOT be used by a privileged app-session startup path.

`ManifestError` remains the one public typed loader error and gains only these
variants beyond the existing `NotFound`, `Read`, and `Parse` cases:

```rust
InvalidUtf8 { path: PathBuf, detail: String } // KELD-GUARD005
IntegrityMismatch {
    path: PathBuf,
    expected: [u8; 32],
    actual: [u8; 32],
} // KELD-GUARD016

impl ManifestError {
    pub fn code(&self) -> &'static str;
}
```

`KELD-GUARD016` tells the operator to rebuild the staged descriptor/fixture or
re-sign the release artifact; it never falls back to an empty manifest.
Adding the workspace-pinned `sha2` dependency to `keld-guard` is the selected
reuse path, but this specification does not approve that Cargo change: the T2
implementation PR must list and obtain the dependency review gate explicitly.
Hand-rolled SHA-256, transitive-dependency reuse, platform crypto wrappers, and
a host-side pre-read are rejected.

KEL-96's approved `keld_core::app_session` API remains intact. T2 adds exactly
one sibling entry point and no public snapshot/context type:

```rust
// keld_core::app_session
pub fn run_guarded(
    boot: keld_core::app_session::ValidatedBootSelection,
) -> Result<(), keld_core::app_session::HostAppError>;
```

`run_guarded` consumes the opaque boot selection, derives the fixed policy path
and digest from its private fields, opens and validates the fixed file once,
transfers that handle to `load_verified_manifest`, stores the returned opaque
`VerifiedManifest` inside the private `GuardSnapshot`, and completes all policy
preflight before any app resource. `HostAppError` preserves the nested
`ManifestError::code()` and fix.
The KEL-96 `run_unprivileged` function remains incapable of registering a
privileged channel and is not a fallback after `run_guarded` fails.

### Trusted principal and dispatch context

The host owns a non-wire `TrustedDispatchContext` concept. Its exact Rust
visibility should remain crate-private unless a later implementation proves a
public API is necessary.

```rust
// Crate-private representation; not a public API.
struct GuardSnapshot {
    verified: keld_guard::verified_manifest::VerifiedManifest,
}

enum TrustedCaller {
    // v0: created only after host-authenticated HELLO on the one app link.
    V0AppLink,
    // Destination: resolved from KEL-75 host RoleRegistry binding.
    RoleGeneration(/* host-owned declared role + generation */),
    // Created and rotated only by the host webview/navigation registry.
    Webview { id: u32, generation: u32 },
}
```

The core router converts `TrustedCaller` to the guard principal immediately
before dispatch:

- v0 `V0AppLink` becomes `Principal::AppProcess` only after the host accepts
  its token-authenticated link. It is not inferred from possession of a link
  string alone.
- Destination `RoleGeneration` is obtained from KEL-75's accepted-link
  binding. KEL-97 / KEL-75 own any necessary role-aware `keld-guard` principal
  API and role-grant subset. KEL-102 must not silently map every role to the
  app ceiling.
- `Webview` comes from the host's window/webview registry. Current backends
  use generation `0`; a later navigation registry must increment it before a
  post-navigation request can dispatch. Missing state is not converted to
  `AppProcess`.

The context is selected before decoding privileged request data. A decoded
request may supply its resource, such as `FsRequest::Read { path }`, but it
cannot supply the principal, role, window, capability, or policy generation.

### Webview policy-injection public API

T4 must remove all three production backend constructions of
`PermissionsManifest::default()`. It extends the existing `keld-wv` API with
one opaque policy value and one exact `WebEngine` signature change:

```rust
#[derive(Clone)]
pub struct MediaPolicy { /* private VerifiedManifest */ }

impl MediaPolicy {
    pub fn from_verified_manifest(
        manifest: keld_guard::verified_manifest::VerifiedManifest,
    ) -> Self;

    pub fn verified_sha256(&self) -> [u8; 32];
}

pub trait WebEngine {
    fn create(
        &mut self,
        spec: &WebviewSpec,
        media_policy: Option<MediaPolicy>,
    ) -> Result<WebviewId, WvError>;
    // Existing remaining methods are unchanged.
}
```

`MediaPolicy` lives at `keld_wv::MediaPolicy`; its fields and manifest accessor
remain private to `keld-wv`. `keld-core` is the named production constructor:
it clones the opaque `VerifiedManifest` from its private `GuardSnapshot`, so
the manifest and digest cannot be substituted independently. `None` is allowed
only for explicitly unprivileged diagnostic windows and means deny all.
`run_guarded` MUST pass `Some`; it cannot treat `None` as recovery from a
load/integrity error.

All media evaluation is a private `MediaPolicy` method. Its production method
reads the manifest and digest from the same opaque pair and invokes a test-only
recorder immediately before `evaluate`, recording `(WebviewId, generation,
verified_sha256)`. Each backend callback must call that method; it may not call
`evaluate` on a separately supplied/default manifest. T4 tests compare the
recorder digest with the host snapshot and assert an entry exists. A negative
control that preserves the correct digest but evaluates
`PermissionsManifest::default()` bypasses the method and therefore fails for a
missing recorder entry. Dropping `MediaPolicy` or mapping missing state to
`AppProcess` also fails. This is the public-API contract approved by this spec;
no public `GuardSnapshot` or `TrustedDispatchContext` is needed.

### Guard-before-handler boundary

For every registered privileged channel, the production path is:

```text
authenticated host link or webview callback
  -> host resolves TrustedDispatchContext
  -> decode and validate request
  -> core routes only to a registered keld-native handler, passing
     snapshot.verified + derived principal + validated request
  -> native handler derives capability/resource
  -> dispatch_privileged(snapshot.verified.manifest(), principal,
                         capability, resource, OS-operation closure)
  -> dispatch_privileged calls evaluate exactly once
  -> OS-operation closure runs only on Allow
```

`keld-core` owns trusted-context resolution, request decoding/validation, and
registered routing. It does not call `evaluate` or `dispatch_privileged`.
`keld-native` owns the handler and is the sole production caller of the shared
`dispatch_privileged` helper immediately around the OS operation. The private
core context is decomposed only into the guard-owned `VerifiedManifest`
reference, host-derived `Principal`, and validated request at that crate
boundary; no core type becomes public. The KEL-71 filesystem session must be
refactored only as needed so its production host path and real-session tests
share that one handler path; it must not grow a second policy implementation.

The existing echo demo and host lifecycle `Ready` / `Quit` controls are
explicitly unprivileged session control and stay outside this dispatcher. A
future operation is unavailable until it has a channel registration,
capability/resource extraction, a trusted caller path, and the same
guard-before-handler test. No native handler may be directly registered on an
app-link.

### v0 and destination boundary

| Surface | v0 implementation target | Destination, not implied by v0 |
|---|---|---|
| Policy file | Fixed file under canonical project/app root; host-owned startup snapshot; blank valid manifest denies all | Signed packaged descriptor and generated/frozen-permission workflow |
| App principal | One accepted host app-link maps to `Principal::AppProcess` | Per-role declaration and generation with a grant subset from KEL-75/KEL-97 |
| Webview principal | Host-minted id; current media generation is `0`; loaded snapshot is injected but `/app` grants still deny media | Per-window grants, navigation rotation, origin context, channel grants and CSP |
| Native API | Reachable host-brokered `fs.read` / `fs.write` only | Other native modules only after individual guarded vertical slices |
| Manifest errors | Refuse privileged app boot; diagnostic tools remain unprivileged | Same fail-closed behavior under package signature and update validation |
| Reload | None; full session restart is required | A separately approved revocation-before-reprovision protocol, never per-call reread |
| OS authority | Broker authorization only; Bun may still use direct `node:fs` | KEL-78 strict-profile admission and hostile real-OS proof |

### Lifecycle and reload

The guard snapshot's lifetime equals the host app session. A policy change is
not an event delivered to a running role. To adopt one, the host must first
stop admitting new privileged calls, revoke app-link/role dispatch contexts,
stop affected Bun roles, close privileged endpoints, and then construct a new
host session from a newly verified boot input. KEL-96 owns the broader
window/listener shutdown order; KEL-75 owns generation revocation. This rule
prevents an old connection from retaining or gaining authority across a
manifest change.

## 5. Boundaries

- Implement in later PRs: `crates/keld-guard` (the minimal single-read
  verified-load extension), `crates/keld-host` (boot ownership and startup
  errors), `crates/keld-core` (host session / trusted dispatch routing),
  `crates/keld-native` (reachable FS broker using the shared dispatcher),
  `crates/keld-wv` (inject host snapshot into media callbacks), tests,
  templates that need an explicit blank manifest, and the error registry when
  a new host error is emitted.
- Must not touch in KEL-102 implementation unless separately approved:
  `docs/architecture/03-security.md`; the kipc frame header/HELLO layout;
  `keld-runtime` sandbox admission internals; the `RoleRegistry` policy/data
  model; workspace dependency pins; CI routing; manifest generator;
  Electron facade options; or any direct-Bun permission shim.
- T2 may add the already workspace-pinned `sha2` entry to
  `crates/keld-guard/Cargo.toml` only when that implementation PR receives the
  explicit dependency review gate. This candidate spec records the design but
  does not grant that approval or edit a Cargo manifest.
- T3 may add the existing workspace member `keld-native` to
  `crates/keld-core/Cargo.toml` only when that implementation PR receives its
  own dependency review gate. Architecture 01 already names native as core's
  destination dependency. Routing from the thin host, copying the FS handler,
  or introducing a callback abstraction solely to hide the dependency are
  rejected alternatives.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T0: A human distinct from the writer approves the exact candidate PR
      head, spec blob, decision digest, permission model, and public APIs; the
      writer then records that provenance, changes `Status: draft` to
      `Status: approved`, checks T0, and obtains human review of the exact
      final status/T0 head. Do not implement from the delegated candidate.
- [ ] T0g: L0 replaces Prompt Tracker's stale standalone
      `host-boot-descriptor` node with the approved atomic
      `host-boot-and-session` artifact predicate, updates `guard/34` to require
      that artifact plus this contract artifact, and consumes
      `kel97_predecessor_task_id=none` by removing `runtime/04`'s stale semantic
      KEL-102-artifact requirement while leaving its static `AFTER` set empty.
      Then reissue the frontier. This spec PR does not edit Prompt Tracker or
      dormant `runtime/04`.
- [ ] T1: KEL-96 lands one atomic T1a+T1b `host-boot-and-session` head with
      both acceptance rows passed. The T1a row provides the verified canonical
      app root, fixed `keld.permissions.jsonc`, and decoded digest consumed by
      `KEL-102/T2`; T1b proves the first durable no-flag host consumer.
- [ ] `KEL-102/T2`: Add the exact guard-owned loader and `run_guarded` API,
      host-owned manifest resolution, private immutable `GuardSnapshot`, and
      typed fail-closed startup errors. Prove no listener handle, child
      process, host-registry window, or platform-native window handle exists
      for every manifest failure class and that the digest covers the exact
      parsed bytes. This is `first_task_id=KEL-102/T2`.
- [ ] `KEL-102/T3`: Wire the v0 authenticated app link through `keld-core` to
      the live `keld-native::fs` broker with a host-derived `AppProcess`
      context. Core routes and passes the verified snapshot/principal/request;
      the native broker alone calls `dispatch_privileged`, which alone calls
      `evaluate`. Prove allowed read/write and denied write/no-file side effect
      over a real kipc session; delete or refactor any parallel FS dispatch
      path rather than retaining both.
- [ ] `KEL-102/T4`: Pass `MediaPolicy` and the registry-derived webview
      principal to all three live media backends. Preserve default denial
      until a separately approved window-grant slice exists; prove a loaded
      `/app` grant cannot authorize a webview and the callback records the
      session's verified digest rather than a default policy.
- [ ] `KEL-102/T5`: After KEL-75/KEL-97 define and ship role-link identity,
      bind that role-aware guard principal at accepted-link dispatch and prove
      stale role generations cannot invoke the FS handler. This is a dependent
      integration PR, not an independent role-policy design.
- [ ] T6: If controlled policy reload is needed, write and approve a separate
      specification first. Its first vertical slice must prove
      revocation-before-reprovision; no polling or per-call file reread.

Task-specific predecessor artifacts:

| Task | Required passed artifacts |
|---|---|
| `KEL-102/T2` | `keld.execution-artifact/v1` with `node_id=kel102-contract-freeze`, `issue_id=KEL-102`, `first_task_id=KEL-102/T2`, the approved spec blob/digest/provenance, and `status=passed`; plus `node_id=host-boot-and-session`, `issue_id=KEL-96`, one landed `head_sha`, passed acceptance rows `KEL-96/T1a` and `KEL-96/T1b`, and `status=passed`. T2 semantically consumes T1a. The old `node_id=host-boot-descriptor` is invalid. |
| `KEL-102/T3` | `node_id=host-guard-enforcement`, `issue_id=KEL-102`, `task_id=KEL-102/T2`, landed `head_sha`, `status=passed`. |
| `KEL-102/T4` | Same schema with exact `task_id=KEL-102/T3`; a generic earlier KEL-102 pass is insufficient. |
| `KEL-102/T5` | Same schema with exact `task_id=KEL-102/T4`, plus `node_id=principal-shipping-link`, `issue_id=KEL-97`, landed `head_sha`, `status=passed`, and passed acceptance rows `KEL-97/role-guard-principal` and `KEL-97/stale-generation-dispatch`. A generic KEL-97 RoleRegistry/link pass is insufficient. KEL-97 or its approved predecessor must first adopt and prove those exact acceptances because current KEL-75 T1-T3 do not ship role grants. |

Successor decision: `kel97_predecessor_task_id=none`. KEL-97 may start after
its KEL-75 and board-ownership gates without waiting for KEL-102. It must not
claim broker authorization. KEL-102/T5 later joins the passed T4 and KEL-97
artifacts. L0 must preserve `runtime/04`'s lack of a KEL-102 predecessor when it
consumes this field; it must not manufacture a T2/T3/T4 edge.

## 7. Test plan

| AC | Test and independent oracle |
|---|---|
| 1–2 | Host binary integration using a temp app root: valid manifest starts the fixture; missing/malformed/outside/digest-mismatched manifests produce the typed error, no child PID, no app-link endpoint/listener handle, an empty host window registry, and no platform-native application window handle. A file written outside the root is never accepted as the manifest; marker absence alone is insufficient. |
| 3 | Real authenticated app-link fixture: correct token binds only its host-selected v0 caller; foreign/stale token is the existing `KELD-IPC-007` rejection and no FS handler marker/reply occurs. |
| 4 | KEL-75 dependent integration: bind a generation, revoke it, then send a formerly valid privileged request. Independent oracle is typed stale/revoked rejection plus absent filesystem marker. |
| 5 | Backend state tests plus real platform smoke: media callback receives a host registry principal; missing state returns `KELD-GUARD007`; `/app` camera grant remains denied for a webview; the production callback recorder's digest equals `GuardSnapshot` on all three backends. Retaining `PermissionsManifest::default()` or dropping `MediaPolicy` fails. Navigation-rotation assertions wait for the actual registry event once that feature exists. |
| 6–7 | Real socket/kipc FS session on temp paths: core routes a verified snapshot/principal/request to the native broker; the broker's sole `dispatch_privileged` call returns `KELD-GUARD002` for out-of-scope write and the target does not exist; allowed write/read returns exact bytes. The production adapter recorder is zero on denied read and nonzero on allowed read. A contract assertion rejects any core-local `evaluate`/`dispatch_privileged` call. |
| 8 | Start with a denying snapshot, retain its already-authenticated stream, modify the manifest, and verify the live session's decision is unchanged. Perform orderly full-session teardown and assert a privileged call on the retained old stream is rejected/closed with no handler entry. Only then launch a fresh session with new credentials and prove it uses the new snapshot. |

Anti-flake requirements: use a temporary root and port `0`; await host/child
markers and process termination rather than sleeping; run crash/reload cases
in a child process; clean every endpoint and temp directory. Platform media
claims require real macOS, Windows, and Linux backend smoke; unrun platforms
remain unverified.

Negative controls are mandatory for the privileged vertical slice:

1. Temporarily bypass or invert the call to `dispatch_privileged`; the denied
   FS integration test must create/change its target or enter the adapter and
   therefore fail.
   A separate mutation moves/adds `evaluate` or `dispatch_privileged` in core;
   the single-owner contract assertion must fail.
2. Temporarily map a rejected/missing caller context to `AppProcess`; the
   foreign-link or missing-webview-principal test must fail.
3. Temporarily substitute `PermissionsManifest::default()` after a manifest
   load error; the startup-failure/no-child test must fail.
4. Temporarily remove the `MediaPolicy` argument at any backend and restore the
   current `PermissionsManifest::default()` construction; the callback digest
   and recorder-entry assertions must fail even though `/app` media remains
   denied. A second mutation keeps the injected digest but evaluates the
   default manifest outside `MediaPolicy`; the missing recorder entry must fail.
5. Temporarily leave an already accepted old link dispatchable after session
   teardown; the retained-stream revocation assertion must fail.

The contract-freeze PR also runs a documentary contract check against this
file. Each temporary mutation below must make that check exit non-zero, after
which the untouched candidate must pass again:

1. remove the exact passed KEL-96 `host-boot-and-session` artifact identity;
2. remove `KEL-102-D1`'s explicit permission-model approval;
3. remove `KEL-102-D2`'s exact public-API approval;
4. replace `KEL-102-D5`'s native-broker sole-owner rule with a core-local or
   duplicate check;
5. replace T4's exact `task_id=KEL-102/T3` predecessor with a generic
   `host-guard-enforcement` pass; and
6. replace T3's exact `task_id=KEL-102/T2` predecessor with another KEL-102
   task or a generic pass;
7. replace T5's exact `task_id=KEL-102/T4` predecessor or either exact KEL-97
   acceptance row with a generic KEL-97 pass; and
8. replace `kel97_predecessor_task_id=none` with any static KEL-102 task edge.

This documentation-only PR has no new behavior tests. The implementation PRs
must run `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo nextest run --workspace --profile ci`, their mapped integration tests,
and `just llms-check` after authoritative-document changes.

## 8. Review gates triggered

- unsafe: **none authorized**. T2-T5 must not add or change `unsafe`; a platform
  backend FFI/lifetime delta stops for its own explicit unsafe review.
- public API: **yes**. Human approval must cover the exact
  `keld_guard::verified_manifest::load_verified_manifest`,
  `ManifestError` variants/`code()`,
  `keld_core::app_session::run_guarded`, and
  `keld_wv::MediaPolicy`/`WebEngine::create` contracts in §4.
  `GuardSnapshot` and `TrustedDispatchContext` remain crate-private. A
  role-principal API is not approved here and remains KEL-75/KEL-97-owned.
- permission model: **yes**. This spec defines the fail-closed policy-loading,
  caller-identity, enforcement, and lifecycle contract; human sign-off is
  required before implementation.
- dependency addition: **none approved by this spec**. T2's selected reuse of
  workspace-pinned `sha2` in `keld-guard` must receive a separate explicit
  dependency gate in its implementation PR. T3's `keld-core` → `keld-native`
  sibling-crate edge requires its own explicit dependency gate; neither is
  waived by an existing workspace declaration.
- wire protocol: **none authorized**. kipc frames and `KELD_APP_LINK` are unchanged; the
  existing manifest schema is read, not extended. A later role/window-grant
  schema change is its own manifest-schema review gate.

## 9. Perf impact

None claimed for this documentation pass. The one manifest read/digest check
is cold host-start work. A later implementation must measure only if it alters
the existing no-allocation guard allow path, IPC dispatch allocations, or the
architecture 01 §5 cold-start/RSS budgets.

## 10. Remaining gates, not open architecture questions

The delegated candidate resolves all nine architecture decisions, but remains
`draft`. A human distinct from the writer must approve the exact candidate
head/spec blob and decision digest, explicitly including permission model and
the selected public APIs. After the status/T0 finalization commit, that human
must review the exact final head again. The resulting approval artifact records
stable `approver_identity`, `approval_source_id`, `decision_digest`, and
`approved_spec_blob_sha`.

Implementation remains blocked after approval until L0 completes T0g and the
atomic KEL-96 `host-boot-and-session` artifact exists. T2 additionally requests
its dependency review; T4's real macOS/Windows/Linux backend observations and
T5's KEL-97 join remain task-owned evidence. None of those gates authorizes
product code in this PR or changes KEL-102 to Done.
