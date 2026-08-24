# Spec: host-enforced guard sessions

Status: draft
Linear: KEL-102 · Owner: GYLDLAB · Updated: 2026-08-23

## 1. Goal & non-goals

Make `keld-guard` an actual host-enforced policy boundary for every live
privileged Keld call: the host loads one trusted `keld.permissions.jsonc`
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

## 3. Acceptance criteria (binary, each becomes a test)

1. Given an app launch with a host-authenticated boot artifact, when
   `keld-host` starts a privileged app session, then it resolves exactly
   `<app-root>/keld.permissions.jsonc` from the artifact's canonical app root,
   verifies the manifest bytes through that artifact's trust chain, and parses
   the file once before it creates a Bun child, app-link listener, or window.
   The Bun child, a webview, a kipc frame, and a working-directory change
   cannot select the root, path, or manifest bytes.
2. Given a missing, unreadable, malformed, traversing, outside-root, or
   integrity-mismatched manifest, when a privileged app session is requested,
   then the host returns a typed error with the corrective action, creates no
   privileged listener, starts no Bun child, and opens no application window.
   `KELD-GUARD004` / `KELD-GUARD005` retain their existing actionable text;
   an integrity failure receives a registered host configuration error that
   tells the developer to rebuild or re-sign the boot artifact. The host MUST
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
   allow webview media through `/app` grants.
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
| Manifest path and trust | The exact filename is `keld.permissions.jsonc` beneath the canonical app root. A production boot artifact authenticates both its relative location and the manifest content digest; the same chain that approves the boot artifact approves those bytes. `keld dev` supplies a canonical project root to the host as developer-controlled launch input, but the host still selects the exact relative filename and rejects a missing, escaping, or non-regular file. A child payload never selects it. |
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
   owns the boot-artifact format; KEL-102 requires that its authenticated
   result include a canonical `app_root` and a manifest descriptor for the
   fixed relative filename. A production descriptor binds the content digest
   to the boot artifact. If KEL-96 cannot authenticate that result, the host
   cannot enable privileged brokers for a release session.
2. Resolve `app_root / "keld.permissions.jsonc"` under the canonical root.
   Reject an absolute/escaping descriptor, symlink escape, non-regular file,
   unreadable file, or a production digest mismatch before parsing. The
   implementation must make this resolution a single host-owned routine;
   callers must not reimplement it.
3. Call one guard-owned single-read verified-load operation. It reads the
   resolved file once, applies the production content-digest verifier to those
   bytes when required, then parses those same bytes and retains the existing
   actionable `ManifestError` behavior. It must not verify a host read and
   then call `load_manifest` on the pathname again. Keep the resulting
   `PermissionsManifest` in an immutable `GuardSnapshot` held by the
   host/core session, together with an opaque policy generation/digest for
   diagnostics. Neither the snapshot nor its manifest path is sent on kipc.
4. Only after these steps succeed may the host create the authenticated
   app-link, bind a webview, or spawn Bun. A blank but valid `{}` manifest is
   a deliberate all-denied policy; a missing file is a startup error, not a
   blank policy.

For v0 development, `keld create` / the project fixture must provide the
explicit `{}` file. `keld dev` passes a canonical project root to the host,
not a manifest pathname, so a developer can alter their own declared
authority but an app child cannot. The destination signed release path applies
the boot-artifact content-digest check to the same fixed file.

### Trusted principal and dispatch context

The host owns a non-wire `TrustedDispatchContext` concept. Its exact Rust
visibility should remain crate-private unless a later implementation proves a
public API is necessary.

```rust
// Sketch only: names are not a committed public API.
struct GuardSnapshot {
    manifest: keld_guard::PermissionsManifest,
    policy_generation: PolicyGeneration,
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

### Guard-before-handler boundary

For every registered privileged channel, the production path is:

```text
authenticated host link or webview callback
  -> host resolves TrustedDispatchContext
  -> decode and validate request
  -> channel registration supplies capability + resource extractor
  -> dispatch_privileged(snapshot, derived principal, capability, resource, handler)
  -> native OS handler only on Allow
```

`keld-core` owns the first four steps and can route only registered native
channels. `keld-native` owns the handler and uses the existing shared
`dispatch_privileged` helper immediately around the operation. The KEL-71
filesystem session must be refactored only as needed so its production host
path and its real-session tests share that one handler path; it must not grow
a second policy implementation.

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
  model; workspace dependency manifests; CI routing; manifest generator;
  Electron facade options; or any direct-Bun permission shim.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T0: Human approves this KEL-102 draft and reconciles KEL-96's boot
      artifact with the fixed manifest descriptor/trust requirement. Do not
      implement from a draft.
- [ ] T1: KEL-96 no-flag host boot provides a verified canonical app root and
      fails before application resources on invalid boot input. The app
      template/fixture contains an explicit empty `keld.permissions.jsonc`.
- [ ] T2: Add the minimal guard-owned single-read verified-load extension,
      then host-owned manifest resolution, immutable `GuardSnapshot`, and
      typed fail-closed startup errors. Prove no window, child, or privileged
      listener exists on each manifest failure class and that a digest check
      applies to the exact parsed bytes.
- [ ] T3: Wire the v0 authenticated app link through `keld-core` to the live
      `keld-native::fs` broker with a host-derived `AppProcess` context. Prove
      allowed read/write and denied write/no-file side effect over a real kipc
      session; delete or refactor any parallel FS dispatch path rather than
      retaining both.
- [ ] T4: Pass the host snapshot and registry-derived webview principal to all
      three live media backends. Preserve default denial until a separately
      approved window-grant slice exists; prove a loaded `/app` grant cannot
      authorize a webview.
- [ ] T5: After KEL-75/KEL-97 define a role-aware guard principal and grant
      subset, bind that principal at accepted-link dispatch and prove stale
      role generations cannot invoke the FS handler. This is a dependent
      integration PR, not an independent role-policy design.
- [ ] T6: If controlled policy reload is needed, write and approve a separate
      specification first. Its first vertical slice must prove
      revocation-before-reprovision; no polling or per-call file reread.

## 7. Test plan

| AC | Test and independent oracle |
|---|---|
| 1–2 | Host binary integration using a temp app root: valid manifest starts the fixture; missing/malformed/outside/digest-mismatched manifests produce the typed error, no child PID, no app-link endpoint, and no window-ready marker. A file written outside the root is never accepted as the manifest. |
| 3 | Real authenticated app-link fixture: correct token binds only its host-selected v0 caller; foreign/stale token is the existing `KELD-IPC-007` rejection and no FS handler marker/reply occurs. |
| 4 | KEL-75 dependent integration: bind a generation, revoke it, then send a formerly valid privileged request. Independent oracle is typed stale/revoked rejection plus absent filesystem marker. |
| 5 | Backend state tests plus real platform smoke: media callback receives a host registry principal; missing state returns `KELD-GUARD007`; `/app` camera grant remains denied for a webview. Navigation-rotation assertions wait for the actual registry event once that feature exists. |
| 6–7 | Real socket/kipc FS session on temp paths: out-of-scope write returns `KELD-GUARD002` and target does not exist; allowed write/read returns exact bytes. The production adapter recorder is zero on denied read and nonzero on allowed read. |
| 8 | Start with a denying snapshot, modify the manifest, and verify the live session's decision is unchanged. After orderly host-session teardown and a fresh launch, the new snapshot is used with new app-link credentials. |

Anti-flake requirements: use a temporary root and port `0`; await host/child
markers and process termination rather than sleeping; run crash/reload cases
in a child process; clean every endpoint and temp directory. Platform media
claims require real macOS, Windows, and Linux backend smoke; unrun platforms
remain unverified.

Negative controls are mandatory for the privileged vertical slice:

1. Temporarily bypass or invert the call to `dispatch_privileged`; the denied
   FS integration test must create/change its target or enter the adapter and
   therefore fail.
2. Temporarily map a rejected/missing caller context to `AppProcess`; the
   foreign-link or missing-webview-principal test must fail.
3. Temporarily substitute `PermissionsManifest::default()` after a manifest
   load error; the startup-failure/no-child test must fail.

This documentation-only PR has no new behavior tests. The implementation PRs
must run `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo nextest run --workspace --profile ci`, their mapped integration tests,
and `just llms-check` after authoritative-document changes.

## 8. Review gates triggered

- unsafe: none expected for the v0 guard wiring; reevaluate if a platform
  backend changes its FFI callback/lifetime handling.
- public API: **yes**. The minimal `keld-guard` verified-load extension is a
  public crate API unless its existing public loader can be extended without a
  new public signature. Keep host dispatch context crate-private; human review
  is required for the loader shape and for any role-principal type.
- permission model: **yes**. This spec defines the fail-closed policy-loading,
  caller-identity, enforcement, and lifecycle contract; human sign-off is
  required before implementation.
- dependency addition: none expected.
- wire protocol: none. kipc frames and `KELD_APP_LINK` are unchanged; the
  existing manifest schema is read, not extended. A later role/window-grant
  schema change is its own manifest-schema review gate.

## 9. Perf impact

None claimed for this documentation pass. The one manifest read/digest check
is cold host-start work. A later implementation must measure only if it alters
the existing no-allocation guard allow path, IPC dispatch allocations, or the
architecture 01 §5 cold-start/RSS budgets.

## 10. Open questions

None. This remains `draft` because the human must approve the permission-model
contract and KEL-96 must provide the authenticated boot-artifact field that
binds the fixed manifest file; neither is permission to begin implementation.
