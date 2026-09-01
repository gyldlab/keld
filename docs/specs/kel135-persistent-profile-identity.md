# Spec: host-owned persistent webview profile identity
Status: draft
Linear: KEL-135 · Owner: GYLDLAB · Updated: 2026-09-01

## 1. Goal & non-goals

Keld must bind persistent browser state to one host-validated application identity and
the current operating-system user. Two independently packaged applications that render
the same origin must not share cookies, local storage, IndexedDB, cache storage,
service-worker registrations, permissions, or other engine profile files. Page content,
Bun code, the working directory, a display name, a URL, app-link endpoint/token text,
and a per-launch staging nonce cannot select a persistent store.

This T0 freezes identity, namespace, engine, concurrency, lifecycle, and evidence
ownership. It changes no live `WebEngine`, profile path, boot descriptor, package,
installer, or product behavior, and it claims no operating-system acceptance pass.

Non-goals:

- no KEL-79 origin, navigation, resource-transport, service-worker capability, or
  per-extension policy;
- no WebView2, WKWebView, WebKitGTK, `WebEngine`, boot, package, update, or uninstall
  implementation in this PR;
- no assertion that a display `name`, reverse-DNS-looking string, unsigned config,
  executable filename, stage path, or bundle path is authenticated application identity;
- no protection from administrators, root, a compromised host, or arbitrary same-user
  native processes outside Keld's later strict profile;
- no physical-directory claim for Apple-managed identified website data stores;
- no cross-OS inference: Windows, macOS, and Linux pass only their own later real-OS
  evidence rows.

## 2. Spec refs

- `docs/architecture/01-overview.md` §2: the host is the authority root and webviews are
  untrusted UI documents.
- `docs/architecture/04-electron-compat.md` §2: `keld.config.ts` plans separate
  `app.id` and display `app.name` fields. The live parser does not implement that sketch.
- `docs/architecture/05-webview-and-native.md` §1: one engine-neutral host surface with
  explicit backend differences.
- `docs/architecture/06-runtime-and-tooling.md` §2 and §4: packaging/update artifacts
  authenticate release inputs and own install/update/uninstall workflow.
- `docs/specs/kel96-no-flag-host-boot.md` §4.2: the live dev stage is owner-private but
  per-launch and not release-authenticated; the current KEL-103 scope authenticates no
  app-specific sidecar facts.
- KEL-63: the current Windows fix moved WebView2 data away from the executable but left
  one global `dev.keld` namespace.
- KEL-79: origin/resource authorization is independent of persistent-store isolation.

This contract does not deviate from the architecture. It makes the planned distinction
between stable `app.id` and display `name` decision-complete before an implementation
changes the public engine-construction boundary.

### Primary platform contracts

- Microsoft, [Manage user data folders](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/user-data-folder):
  a UDF contains cookies, DOM storage and cache; a custom path needs appropriate runtime
  access; one UDF is one WebView2 session; deletion waits for every browser process.
- Microsoft, [WebView2 process model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model):
  equal UDF/options share one process collection and mismatched options fail.
- Microsoft, [WebView2 security-tool guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/measures):
  the engine's required UDF access must not be removed by a hand-written restrictive ACL.
- Microsoft, [Known folder identifiers](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid):
  `FOLDERID_LocalAppData` is a per-user known folder.
- Apple, [`WKWebsiteDataStore`](https://developer.apple.com/documentation/webkit/wkwebsitedatastore):
  default is persistent, nonpersistent is memory-only, and identifier-addressed stores
  provide persistent profiles.
- Apple, [identified-store removal](https://developer.apple.com/documentation/webkit/wkwebsitedatastore/remove%28foridentifier%3Acompletionhandler%3A%29):
  release every using `WKWebView` before removal.
- WebKit, [profiles with identified data stores](https://webkit.org/blog/14423/building-profiles-with-new-webkit-api/):
  identifier-addressed persistent stores are a macOS 14 addition.
- WebKitGTK, [`WebKitWebsiteDataManager`](https://webkitgtk.org/reference/webkit2gtk/unstable/class.WebsiteDataManager.html)
  and [`WebKitWebContext`](https://webkitgtk.org/reference/webkit2gtk/stable/class.WebContext.html):
  explicit managers own base data/cache directories; ephemeral managers do not persist.
- freedesktop.org, [XDG Base Directory Specification 0.8](https://specifications.freedesktop.org/basedir-spec/latest/):
  user data and cache have separate absolute roots and a relative environment value is
  invalid.
- pinned wry 0.56.1 `WebContext`/Darwin extension source: Linux's convenience context
  uses one path for data and cache, while macOS identified stores require
  `with_data_store_identifier` and macOS 14 or later. These are evaluated upstream
  facilities, not Keld policy owners.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a release boot, when the host constructs persistent profile state, then it
   consumes the package-verifier-owned `ValidatedAppIdentity` containing validated
   publisher scope and canonical signed `app.id`, derives one opaque `ProfileIdentity`,
   and never accepts construction of that identity by config/page/Bun/webview code.
2. Given substitutions of display name, cwd, executable filename/location, stage nonce,
   renderer URL, app-link endpoint/token, arbitrary environment, or IPC input, when
   profile selection runs, then the selected persistent identity and relative namespace
   remain unchanged. Linux's absolute data/cache roots are the separately validated
   boot-time XDG inputs. Changing a valid absolute XDG root between launches selects the
   same relative identity under a new location and preserves the old roots; Keld cannot
   infer or migrate the old location unless an explicit user-approved operation supplies
   and validates both old roots.
3. Given the current unsigned dev stage or any release boot missing authenticated app
   identity, when engine startup requests a profile, then dev receives an explicitly
   ephemeral host-minted profile and release fails as `KELD-WV-009`; neither path uses a
   default/global persistent store or shared temporary directory.
4. Given a validated identity, when its namespace is encoded, then the only filesystem
   segment is 64 lowercase hexadecimal characters for the 32 identity bytes. Invalid,
   noncanonical, colliding, mismatched-owner, or traversal-bearing material is rejected
   before an engine object exists. On macOS, a protected Keld metadata registry binds the
   full identity to the shorter Apple UUID in both directions; a UUID already bound to a
   different full identity is a collision and fails before store lookup.
5. Given an existing Keld-owned filesystem or metadata leaf, when the host opens it,
   then a schema-v1 ownership marker must match the full identity, platform and root
   role. A missing marker may be created only in a newly created empty leaf; a mismatch,
   nonempty unmarked leaf, pre-existing link/reparse point, escaping final path, wrong
   owner, or unsafe permissions fails before navigation. Apple-managed website-store
   directories are excluded: macOS proves its Keld metadata/UUID binding plus the public
   store identifier and persistence properties, not an Apple physical path or marker.
6. Given persistent Windows startup, when WebView2 creates its environment, then the UDF
   is under the per-user `FOLDERID_LocalAppData` Keld namespace for that identity, never
   `%LOCALAPPDATA%/dev.keld`, executable-adjacent, cwd, registry/environment-selected,
   or shared temp. The host compares WebView2's actual environment-reported UDF to the
   retained validated path before controller/navigation. The final volume must be local,
   not a remote/network-backed location, and the environment requests exclusive UDF
   access so a second process cannot share its browser collection.
7. Given the Windows profile tree, when permissions are inspected, then no broad
   `Everyone`, ordinary `Users`, or `Authenticated Users` write grant exists and a
   distinct ordinary user cannot read or write it. The implementation preserves and
   reads back WebView2-required LowIL/AppContainer access instead of copying the
   KEL-101 pipe DACL or assuming one current-user ACE is sufficient.
8. Given persistent macOS startup on macOS 14 or later, when the backend creates its
   configuration, then it assigns the deterministic identity-derived UUID to an
   identifier-addressed persistent `WKWebsiteDataStore` before creating `WKWebView`.
   The default persistent store is forbidden. On older macOS, persistent startup fails
   as `KELD-WV-009`; explicit dev mode uses `nonPersistent` and proves it is not
   persistent.
9. Given persistent Linux startup, when the backend creates WebKitGTK, then one
   host-owned `WebKitWebsiteDataManager` uses separate validated XDG data and cache roots
   for the identity and one engine-owned `WebKitWebContext` is shared by its views.
   Relative XDG values are ignored per the XDG contract; default/implicit contexts and a
   single conflated data/cache path are forbidden.
10. Given two host processes for the same user and `ProfileIdentity`, when both attempt
    persistent startup, then one atomic exclusive profile lease wins and the other fails
    as `KELD-WV-009` (or a later separately approved activation UX contacts the owner).
    It never creates a suffix, default, temp, or second store. On Windows a successor
    controller also remains rejected until the old WebView2 browser collection releases
    the UDF after host crash. Different identities acquire different leases and stores.
11. Given Bun generation rotation, host restart, display-name/executable relocation, a
    compatible app update, or rollback, when the authenticated profile identity is
    unchanged, then the same store is reused. A changed identity selects a different
    store unless a separately signed, crash-safe migration names both identities. A
    changed validated Linux XDG root is not automatically discoverable: normal startup
    may create a new store at the new roots and must leave old roots untouched; an
    explicit migration requires validated old+new roots and exclusive ownership.
12. Given ordinary uninstall, when packaging removes app bytes, then profile data is
    preserved by default. An explicit user-requested purge is packaging-owned, requires
    the exact validated identity and exclusive lease, waits for every browser/webview
    process to release the store, persists a resumable purge intent before deletion,
    rejects link/reparse substitution, and deletes no other identity. Startup encountering
    purge intent resumes or fails closed before normal lookup; it never recreates the
    store. Test cleanup follows the same ownership rule under a test-only root.
13. Given independently packaged App A and App B under one OS user and the same exact
    origin, when A writes cookie, localStorage, IndexedDB/CacheStorage and service-worker
    nonce state, then B observes none of it and A observes it after restart. The row runs
    separately on every OS claimed and cannot pass through a mock, source-string search,
    or different origin.
14. Given a second ordinary OS user, when that user launches the same app identity, then
    their store is distinct and cannot observe the first user's same-origin browser
    state. Windows/Linux additionally prove the first user's Keld-owned roots are not
    readable or writable. Apple exposes no sanctioned physical store path, so macOS
    filesystem ACL/path access remains unverified rather than inferred. Administrators,
    root and hostile same-user native processes are outside this claim.
15. Given a temporary mutation that removes identity consumption, aliases A/B namespace
    keys, enables default fallback, skips actual-path/marker validation, removes the
    exclusive lease, or performs deletion before engine-process exit, then the one
    corresponding identity, platform, concurrency, or lifecycle test fails.

## 4. Design

### First-principles ownership and reuse decision

This is an architecture change because it adds an authenticated application-identity
input to webview/profile ownership. It does not change process, window, renderer, origin,
wire, capability, or Bun-principal ownership.

| Atom | Owner and boundary | Input → output | Failure and direct observable | Independence |
|---|---|---|---|---|
| Release identity | future signed package verifier, consumed by `keld-core` | publisher scope + canonical signed `app.id` → private validated identity input | unsigned/config/page/Bun value → no persistent identity | does not choose a platform path |
| Profile identity | `keld-core` | validated publisher/app tuple → opaque 32-byte `ProfileIdentity` | noncanonical tuple or substitution → `KELD-WV-009` before engine | not filesystem containment |
| Dev mode | host boot | current unsigned dev stage + OS randomness → per-launch ephemeral selection | any persistent fallback → startup failure | makes no release identity claim |
| Namespace | shared profile owner in `keld-core` | profile identity + platform root → exact key/path or Apple UUID | collision/marker mismatch/link escape → `KELD-WV-009` | backend does not mint identity |
| Store consumption | `keld-wv` platform backend | validated profile selection → WebView2 UDF / WK store / WebKit manager+context | default/actual-path mismatch → no first navigation | does not define origin policy |
| Containment | platform filesystem owner | retained root/leaf handles + current OS user → verified store boundary | foreign/broad access or link substitution → fail closed | same-user native threat remains outside claim |
| Concurrency | host-owned profile lease | user + profile identity + process lifetime → one exclusive owner | second owner → deterministic `KELD-WV-009` | not browser process-pool policy |
| Lifecycle | packaging/update owner plus host teardown | stable identity + lifecycle event → retain/migrate/preserve/purge | early/wrong-identity delete → typed failure and untouched store | Bun generation never owns data |
| Evidence | per-platform implementation task | real engine/OS effect → isolated/persistent result | mock/source string/other OS → no pass | KEL-79 remains separate |

The live state proves why the identity input cannot be inferred:

- `ValidatedBootSelection` currently contains only the per-launch root, display `name`,
  entry identity, renderer bytes and permissions snapshot. `name` is parsed from
  developer-controlled config and used as the window title.
- Windows `WebView2Engine::new()` independently uses one fixed `dev.keld` UDF and an
  environment-variable fallback. macOS and Linux call bare `WebViewBuilder::new()`;
  pinned wry therefore selects default persistent stores/contexts.
- KEL-96 explicitly says its stage is not a signed production container, and KEL-103's
  standalone host-signing draft authenticates none of the app-specific sidecar facts.

Existing options evaluated:

- **Display `name`, planned raw `app.id`, cwd, bundle/executable name, or stage root:**
  rejected. None is sufficient until a package verifier authenticates its relation to
  the app and publisher; stage root also rotates each launch.
- **One random per-install profile ID without publisher scope:** rejected. A second
  package can deliberately reuse it. The verifier must supply a stable publisher scope
  and canonical signed app id as one validated tuple.
- **Windows fixed `dev.keld` or loader default:** rejected because it shares apps and
  accepts ambient overrides. The existing direct COM environment remains the upstream
  primitive and gains a validated path plus actual-UDF read-back.
- **wry `WebContext::new(Some(path))` everywhere:** rejected as the single owner. On
  Linux it conflates data and cache roots; on macOS its generic data-directory field is
  unused and the Darwin-specific identified-store API is macOS 14+.
- **WKProcessPool or browser-process separation:** rejected as a storage oracle.
- **Shared same-app cross-process stores:** rejected for v1 because WebView2 documents
  sharing but macOS/Linux simultaneous-writer behavior is not a portable guarantee.
  One exclusive host lease is the smallest uniform correct policy.

The implementation reuses each native identified-store/context primitive and one shared
host profile-selection type. No backend may copy identity parsing, hashing, marker,
lease, or lifecycle policy. Compatibility fallback is explicitly ephemeral dev mode;
there is no persistent global fallback. No performance claim is made.

### Identity and selection shape

Names may be refined during public-API review, but ownership must remain:

```rust
/// Package-verifier output; fields and constructors remain private.
pub struct ValidatedAppIdentity {
    publisher_scope: [u8; 32],
    app_id: CanonicalAppId,
}

/// Opaque profile namespace minted by keld-core only.
pub struct ProfileIdentity([u8; 32]);

pub enum WebProfileSelection {
    Persistent(ProfileLease),
    EphemeralDev(EphemeralProfile),
}

pub struct ProfileLease {
    identity: ProfileIdentity,
    platform: ValidatedPlatformProfile,
    // retained exclusive OS lease and directory handles where applicable
}
```

`ValidatedAppIdentity` is the package verifier's sole output and already contains the
validated publisher scope; there is no second public `ValidatedPublisherScope`
constructor or competing identity boundary.

`ProfileIdentity` is SHA-256 over the exact length-delimited byte sequence
`"keld.profile.identity/v1\0" || publisher_scope || u16be(app_id.len) || app_id`.
`CanonicalAppId` is 1–255 bytes of lowercase ASCII dot-separated segments; each segment
starts and ends with `[a-z0-9]` and otherwise contains only `[a-z0-9-]`. Validation
rejects noncanonical input rather than lowercasing or Unicode-normalizing it. The app id
and publisher scope are public identity material, not secrets; logs may name the app id
but must not expose filesystem paths, handles, cookies, or browser data.

The package verifier owns the stability rule: the tuple is identical across compatible
updates, rollback and signing-key rotation, and distinct across publishers/apps. T1
cannot implement persistent release mode until an approved signed-container predecessor
provides that exact guarantee. The current dev compiler supplies no substitute.

Filesystem directory names use the full 64-lowerhex `ProfileIdentity`. The macOS store
UUID is deterministic UUIDv8 material made from the first 16 bytes of
`SHA-256("keld.wk-store/v1\0" || ProfileIdentity)`, with RFC variant and version bits
set. Because this intentionally compresses 256 identity bits into a UUID, fixed vectors
alone are insufficient: the Keld metadata registry atomically creates
`identities/<full-identity>/profile.owner.v1` and
`store-uuids/<uuid>/profile.owner.v1`, both containing the full identity and UUID. Both
records must agree before lookup, reuse or purge; an existing mismatch is
`KELD-WV-009`. Before Apple removal, T5 atomically commits a `purging` intent containing
the full identity and UUID. Normal lookup is forbidden while it exists. Recovery checks
identifier enumeration, treats an already absent Apple store as successful idempotent
removal, removes the reverse record and identity record in that order, then clears the
intent. A crash at any step resumes this sequence; it cannot recreate a store or accept a
one-sided binding.

### Platform policy

| Platform | Persistent owner | Ephemeral dev | Required read-back / limit |
|---|---|---|---|
| Windows | `FOLDERID_LocalAppData/Keld/profiles/v1/<identity>/webview2`; direct WebView2 environment receives the retained validated path | unique owner-private per-launch disk UDF removed only after browser exit | local volume; environment-reported UDF equals validated final path; exclusive-UDF option; no reparse escape; preserve/read back engine-required access; distinct ordinary user denied |
| macOS 14+ | identifier-addressed `WKWebsiteDataStore` using deterministic UUID plus Keld-owned Application Support identity/UUID records and lease; Apple owns physical store layout | one engine-owned `nonPersistent` store shared by every view in that host | both registry directions, configuration store identifier and persistence state before first view; no Apple physical-path claim |
| macOS <14 | persistent mode unsupported and typed `KELD-WV-009` | `nonPersistent` store | prove no default persistent fallback |
| Linux | explicit `WebKitWebsiteDataManager` with `$XDG_DATA_HOME/Keld/profiles/v1/<identity>/webkit` and `$XDG_CACHE_HOME/Keld/profiles/v1/<identity>/webkit`, then one engine-owned context | one engine-owned ephemeral manager/context shared by every view | live-WebView context→manager identity, exact data/cache getters and artifacts, `is-ephemeral`, current UID, owner-only modes and pre-existing-link containment |

macOS resolves the current user's Application Support directory through Foundation and
owns `Keld/profiles/v1/` metadata there. The full-identity record owns the exclusive
lease; the reverse UUID record prevents compressed-key aliasing. Both records are
owner-only Keld metadata, not WebKit storage. One `WkWebViewEngine` retains exactly one
selected persistent or nonpersistent store and supplies it to every view it creates.

Windows does not copy KEL-101's named-pipe DACL. The WebView2 runtime may need
LowIL/AppContainer access inside its UDF. T2 starts from the normal per-user known-folder
inheritance, rejects broad ordinary-user write access, observes the actual engine-created
descriptor, and freezes a validation/read-back predicate rather than rewriting the
Microsoft-managed ACL. Any ACL mutation requires a separately justified real-WebView2
result and must preserve engine-managed ACEs and labels. This is a permission-model
review gate, not an implementation guess in T0. T2 also rejects a remote/network final
volume and sets WebView2 exclusive UDF access. After host crash, `ERROR_INVALID_STATE`
or equivalent controller failure remains required until `BrowserProcessExited` proves
the old collection released the UDF; a process-local Keld lease alone cannot pass.

Linux follows the XDG rule that relative environment values are invalid. The host
resolves/defaults the absolute data and cache bases once before GTK/WebKit
initialization. The data leaf is authoritative and owns a `root_role=data` marker; the
cache leaf has a separate `root_role=cache` marker. The exclusive lease lives outside
both deletable trees at owner-only
`$XDG_RUNTIME_DIR/keld/profile-leases/v1/<identity>.lock`. A missing, relative,
wrong-owner or non-`0700` runtime directory fails persistent startup; no shared-temp
lease fallback exists. Data is created/validated first, cache second. An absent empty
cache leaf may be recreated only while the authoritative data marker and runtime lease
match; a nonempty unmarked cache leaf fails. Purge retains the runtime lease, validates
both roots, removes cache first (absence is allowed), then data, and releases the lease
only after the deleted paths can no longer be recreated by a concurrent Keld host.

Each Keld-owned component is `0700` and wrong-owner, group/world-writable, pre-existing
link, or final-path mismatch fails. WebKitGTK ultimately consumes path strings, so
retained handles do not prove protection from a concurrent hostile same-user swap; that
threat is explicitly outside AC14. The negative oracle covers links/substitution present
before manager construction and other-user mutation, not an unprovable same-user race.

Pinned wry 0.56.1 cannot construct a context with separate data/cache roots. T4 must
first land/reuse an upstream wry API and reviewed release that accepts an explicit
WebsiteDataManager (or separate roots). A local fork and a parallel Keld WebKit builder
are forbidden by this contract. That dependency/API slice is part of T4 and its review
gate; T4 is blocked if the upstream facility remains unavailable.

### Concurrency and lifecycle matrix

| Event | Identity/lease owner | Required result |
|---|---|---|
| second same-app host | profile manager | first owner retains exclusive lease; second gets `KELD-WV-009`; Windows also requires engine-exclusive UDF release; no fallback store |
| second different app | independent profile manager | distinct identity, lease and store |
| Bun generation restart | existing host | retain same lease/store; Bun never receives selection authority |
| host restart | new validated host | reacquire same identity/store after prior owner and engine processes exit |
| display-name, cwd, executable or renderer change | none | no identity/store change |
| compatible update or rollback | signed package verifier | same tuple and store; profile schema must remain backward-compatible |
| authenticated identity change | package migration owner | distinct store unless a signed two-identity migration completes atomically |
| validated Linux XDG root change | profile manager; explicit migration only when old+new roots are supplied | normal startup uses the new location and preserves the undiscoverable old roots; explicit migration is exclusive and crash-safe |
| ordinary uninstall | packaging | preserve profile by default |
| explicit purge | packaging + profile manager | exact identity, exclusive lease, all views/processes exited, validate every platform binding/root, delete only owned store; macOS store removal precedes UUID-binding removal |
| test cleanup | test-owned root + profile manager | same purge protocol; no production user directory |

`KELD-WV-009` is the reserved profile-selection failure. Its detail distinguishes
missing authenticated identity, unsupported persistent store, namespace/containment
failure, actual-store mismatch, and profile-in-use; every message names the concrete
fix. Library code does not panic or retry deterministic failures.

## 5. Boundaries

T0 implements only this file and generated documentation if the source is allowlisted.

Future implementation ownership:

- `keld-core`: private validated identity, derivation, mode selection and lease lifetime;
- signed package/boot verifier (`keld-pack` plus the approved KEL-103 successor): stable
  publisher/app tuple, update/rollback continuity and purge authority;
- `keld-wv`: consume a sealed selection in platform constructors and expose only
  read-only evidence needed by tests/doctor;
- platform backend modules: WebView2 actual-UDF/ACL/handle evidence, WK identified
  store, WebKitGTK manager/context;
- `keld-host`: carry the validated selection through app-session startup/teardown;
- tests/fixtures: real same-origin App A/B and same-app concurrency oracles.

Must not touch in T0: Rust/TypeScript behavior, `Cargo.toml`, lockfiles, config/boot
schema, KEL-79 policy, KEL-132 GPU/media installation, installers, update feeds, CI, or
agent instructions.

## 6. Tasks (each one scoped PR/artifact)

- [ ] **T0 — contract freeze** (`node_id=webview-profile-identity-contract`): land this
  approved contract and a passed `keld.execution-artifact/v1`; no product/OS pass.
- [ ] **T1 — identity, derivation, ephemeral mode and lease**
  (`node_id=webview-profile-identity-foundation`): after an approved signed package/app
  identity predecessor, add the private core/host selection boundary, fixed vectors,
  ownership marker, exclusive lease, `KELD-WV-009`, and explicit dev-ephemeral mode.
  Artifact owns CI-only identity/substitution/collision/concurrency-state tests; it owns
  no platform store pass.
- [ ] **T2 — Windows WebView2 UDF** (`node_id=webview-profile-windows`): consume T1 in
  the direct COM backend, resolve the known folder without trusting environment, validate
  handles/reparse containment, local-volume status and actual UDF, enable exclusive UDF
  access, validate (not replace) the engine-required ACL, and run real Windows App A/B,
  distinct-user, crash-release, concurrency, restart and deletion controls.
- [ ] **T3 — macOS identified store** (`node_id=webview-profile-macos`): consume T1 via
  the Darwin builder/configuration, require macOS 14+ for persistent mode, prove UUID
  registry/store selection and one shared nonpersistent dev store, and run real macOS
  App A/B, two-user state isolation, concurrency, restart, removal and older-OS
  fail-closed controls. Direct physical store ACL/path proof remains unverified.
- [ ] **T4 — Linux explicit context** (`node_id=webview-profile-linux`): consume T1 via
  a reviewed upstream wry API/release for one engine-owned explicit
  WebsiteDataManager/WebContext, separate XDG data/cache roots and two-role markers;
  hold the exclusive lease outside those roots under validated `XDG_RUNTIME_DIR`;
  enforce owner/mode/pre-existing-link containment; prove every live WebView reaches the
  exact shared manager; and run real Linux App A/B, distinct-user, purge/start race,
  concurrency, restart and two-root deletion controls. No local wry fork or parallel
  builder is allowed.
- [ ] **T5 — package/update/uninstall lifecycle**
  (`node_id=webview-profile-package-lifecycle`): bind the signed package tuple and
  update/rollback continuity, preserve on ordinary uninstall, implement explicit purge
  after platform teardown, including durable idempotent macOS purge intent, and run
  migration/identity-change/crash-recovery tests for every platform whose T2–T4 artifact
  passed.

No later task is frontier-ready merely because T0 lands. Each requires the named landed
predecessor, issue/claim authority, applicable platform availability, and a fresh
frontier artifact when its prompt requires one.

## 7. Test plan

| AC | Future owner | Class and independent oracle | Required negative control |
|---|---|---|---|
| 1–4 | T1 | CI-only fixed identity/UUID vectors, strict parser and sealed-constructor compile/API tests | accept display/config/Bun input or alias two tuples |
| 5 | T1 | CI-only marker/registry state model; no platform containment pass | mismatched/nonempty unmarked logical leaf or UUID binding |
| 5 | T2/T3/T4 | separate real platform binding/root read-back; macOS reads Keld registry plus public store identifier, not Apple path | pre-existing link/marker/binding mismatch for that platform |
| 6–7 | T2 | real Windows environment UDF, local volume, final path, descriptor, exclusive crash-release, App A/B and second user | hostile loader override, remote volume, reparse ancestor, broad foreign-user grant, missing exclusive option |
| 8 | T3 | real macOS 14+ two-way UUID binding, identifier/persistence read-back, shared ephemeral store and same-origin App A/B | remove binding/identifier or run below 14 and require default fallback to fail |
| 9 | T4 | real live-WebView→context→manager identity, separate getters/artifacts, UID/mode and same-origin App A/B | default/parallel context, relative or post-selection root substitution, conflated roots, pre-existing symlink, one missing marker |
| 10 | T2/T3/T4 | real two-process exclusive lease and different-app parallel success; Linux lease is outside deletable roots | delete lease, put it in a deleted leaf, race startup against purge, or add suffix/temp fallback |
| 11 | T1 + T5/platform rows | deterministic lifecycle state model plus real restart/update/rollback; Linux root change creates a new location while preserving old unless explicit migration supplies both | derive from display name, stage nonce or executable path; silently delete/infer an old XDG root |
| 12 | T5 | real engine-process exit + exact-identity preserve/purge observation; macOS next-start resumes each interrupted purge phase | purge while live, after link substitution, with B identity, without intent, or crash after each Apple/binding deletion step |
| 13 | T2/T3/T4 | real same-origin cookie/localStorage/IndexedDB/CacheStorage/service-worker effect | force A/B same key; different origins are not accepted as proof |
| 14 | T2/T4 | real second ordinary OS user, state isolation and filesystem access result | loosen owner boundary; never substitute CI/emulation |
| 14 | T3 | real second macOS user and same-origin state isolation; physical path/ACL is explicitly unverified | alias user/store UUID or accept shared state |
| 15 | every task | one mutation per named atom, restored before review | test surviving its mutation blocks the task |

Platform tests use observable conditions and bounded kill-switch deadlines, never sleeps.
macOS/Linux/Windows results are recorded separately. Mock policy tests may prove pure
state only; source-string tests and another OS cannot prove engine/store behavior.

## 8. Review gates triggered

T0 review gates:

- unsafe: none in the T0 diff; T2/T3 and any direct platform FFI require exact-final-diff
  unsafe review under the existing `keld-wv` owner;
- public API: applies — future engine construction and sealed identity/selection shapes;
- permission model: applies — per-user filesystem ownership, engine-required access and
  purge authority;
- dependency addition: none in T0; every later addition is separately reviewed;
- wire protocol: none; no kipc bytes, HELLO, app-link, channel or error frame changes.

Packaging/signing review also applies to T1/T5 because authenticated identity and purge
authority cross that boundary. KEL-79 security/origin review remains separate.

## 9. Perf impact

T0 has no runtime impact. Later persistent startup adds identity hashing, one lease, and
bounded filesystem/engine read-back on cold startup only. No steady-state IPC or frame
path changes. T2–T4 record cold-start change; a measured regression over architecture
01's 5% threshold needs a written waiver. Exclusive same-app ownership may reduce
cross-process engine sharing; this is a correctness decision, not a performance claim.

## 10. Open questions

None inside this draft's proposed contract. Human approval must explicitly accept:

1. persistent release mode is blocked until a package verifier supplies the stable
   validated publisher/app tuple;
2. unsigned current dev sessions are ephemeral rather than sharing persistent state;
3. same-app concurrent host processes are exclusive in v1; and
4. persistent macOS profile support begins at macOS 14, with no default-store fallback.

Until approval, status remains `draft`, no implementation task is authorized, and none
of the platform rows is passed.
