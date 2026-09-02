# Spec: host-owned persistent webview profile identity
Status: approved
Linear: KEL-135 · Owner: GYLDLAB · Updated: 2026-09-02
Approval: Linear comment `75d75f6e-76e9-4fd1-a130-9d57548d0372` · decision SHA-256 `b9f48f14a5d6fefe4cd0f94b1a97b14292cead5ba0202facbd81c6b6a1a44040`

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
- Microsoft, [`WinVerifyTrust`](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-winverifytrust):
  Windows T2 validates the packaged object under Authenticode policy before extracting
  its signer scope and signed app manifest.
- Apple, [`WKWebsiteDataStore`](https://developer.apple.com/documentation/webkit/wkwebsitedatastore):
  default is persistent, nonpersistent is memory-only, and identifier-addressed stores
  provide persistent profiles.
- Apple, [identified-store removal](https://developer.apple.com/documentation/webkit/wkwebsitedatastore/remove%28foridentifier%3Acompletionhandler%3A%29):
  release every using `WKWebView` before removal.
- Apple, [code-signing information keys](https://developer.apple.com/documentation/security/signing-information-dictionary-keys)
  and [`SecCodeCopySigningInformation`](https://developer.apple.com/documentation/security/seccodecopysigninginformation%28_%3A_%3A_%3A%29):
  macOS T3 first validates code, then consumes the Team Identifier and signed identifier;
  copying signing information without validity checking is insufficient.
- WebKit, [profiles with identified data stores](https://webkit.org/blog/14423/building-profiles-with-new-webkit-api/):
  identifier-addressed persistent stores are a macOS 14 addition.
- WebKitGTK, [`WebKitWebsiteDataManager`](https://webkitgtk.org/reference/webkit2gtk/unstable/class.WebsiteDataManager.html)
  and [`WebKitWebContext`](https://webkitgtk.org/reference/webkit2gtk/stable/class.WebContext.html):
  explicit managers own base data/cache directories; ephemeral managers do not persist.
- freedesktop.org, [XDG Base Directory Specification 0.8](https://specifications.freedesktop.org/basedir-spec/latest/):
  user data and cache have separate absolute roots and a relative environment value is
  invalid.
- Linux kernel, [`boot_id`](https://www.kernel.org/doc/html/v6.12/admin-guide/sysctl/kernel.html):
  the kernel-generated UUID is unvarying within one boot.
- Apple, [`sysctlbyname`](https://developer.apple.com/documentation/kernel/1387446-sysctlbyname)
  and WebKit's primary
  [`bootSessionUUIDString`](https://github.com/WebKit/WebKit/blob/main/Source/WTF/wtf/UUID.cpp)
  implementation: macOS exposes `kern.bootsessionuuid` as the boot-scoped UUID that
  WebKit itself uses for boot-specific cache validity.
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
   same relative identity under a new location and preserves the old roots; v1 neither
   infers nor migrates the undiscoverable old location.
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
10. Given two host processes for the same user, `ProfileIdentity`, and validated platform
    namespace roots, when both attempt persistent startup, then one atomic exclusive
    profile lease wins and the other fails
    as `KELD-WV-009` (or a later separately approved activation UX contacts the owner).
    It never creates a suffix, default, temp, or second store. On Windows a successor
    controller also remains rejected until the old WebView2 browser collection releases
    the UDF after host crash. Different identities acquire different leases and stores.
11. Given Bun generation rotation, graceful host restart, display-name/executable
    relocation, a compatible app update, or rollback, when the authenticated profile
    identity is unchanged, then the same store is reused. After host death with a
    non-idle durable engine state, Windows may recover only through its exclusive-UDF
    release oracle; macOS/Linux mark the profile quarantined and fail same-boot persistent
    restart. Death after durable `idle` is safe. A changed identity selects a different store and
    preserves the old store; v1 performs no cross-identity data migration. A
    changed validated Linux XDG root is not automatically discoverable: normal startup
    may create a new store at the new roots and must leave old roots untouched; an
    future migration is separately specified; v1 accepts no old-root input.
12. Given ordinary uninstall, when packaging removes app bytes, then profile data is
    preserved by default. An explicit user-requested purge is packaging-owned, requires
    the exact validated identity and exclusive lease, waits for the platform's named
    engine release or async-clear barrier, persists a resumable purge intent before
    destructive change,
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
    exclusive lease, or performs purge before its platform release/clear barrier, then the one
    corresponding identity, platform, concurrency, or lifecycle test fails.

## 4. Design

### First-principles ownership and reuse decision

This is an architecture change because it adds an authenticated application-identity
input to webview/profile ownership. It does not change process, window, renderer, origin,
wire, capability, or Bun-principal ownership.

| Atom | Owner and boundary | Input → output | Failure and direct observable | Independence |
|---|---|---|---|---|
| Release identity | platform package verifier in `keld-core` | validated platform publisher scope + canonical signed `app.id` → private identity input | unsigned/config/page/Bun value → no persistent identity | does not choose a platform path |
| Profile identity | host-agnostic `keld-wv::profile` policy called only after core verification | validated publisher-scope/app-id bytes → opaque 32-byte `ProfileIdentity` | noncanonical tuple or substitution → `KELD-WV-009` before engine | not filesystem containment; preserves `keld-core → keld-wv` dependency direction |
| Dev mode | host boot | current unsigned dev stage + OS randomness → per-launch ephemeral selection | any persistent fallback → startup failure | makes no release identity claim |
| Namespace | shared owner in `keld-wv::profile` | profile identity + platform root → exact key/path or Apple UUID | collision/marker mismatch/link escape → `KELD-WV-009` | backend does not mint identity |
| Store consumption | `keld-wv` platform backend | validated profile selection → WebView2 UDF / WK store / WebKit manager+context | default/actual-path mismatch → no first navigation | does not define origin policy |
| Containment | platform filesystem owner | retained root/leaf handles + current OS user → verified store boundary | foreign/broad access or link substitution → fail closed | same-user native threat remains outside claim |
| Concurrency | host-owned profile lease | user + profile identity + validated namespace roots + process lifetime → one exclusive owner | second owner or same-boot macOS/Linux predecessor left in `starting`/`running`/`stopping` → deterministic `KELD-WV-009` | not browser process-pool policy |
| Lifecycle | packaging/update owner plus host teardown | stable identity + lifecycle event → retain/isolate/preserve/purge | early/wrong-identity delete → typed failure and untouched store | Bun generation never owns data |
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

Names may be refined during public-API review, but dependency direction and ownership
must remain. `keld-core` depends on `keld-wv`; `keld-wv` never names a core type:

```rust
/// keld-core-private package-verifier output.
struct ValidatedAppIdentity {
    publisher_scope: [u8; 32],
    app_id: CanonicalAppId,
}

/// Opaque host-agnostic namespace owned by keld-wv::profile.
pub struct ProfileIdentity([u8; 32]);

impl ProfileIdentity {
    /// Called by trusted host code only after package signature verification.
    pub fn from_host_verified_parts(
        publisher_scope: [u8; 32],
        canonical_app_id: &str,
    ) -> Result<Self, ProfileError>;
}

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

`ValidatedAppIdentity` is core's private package-verifier output. Core calls the
host-agnostic lower-crate constructor only after verification and passes the resulting
`keld-wv` type back into `WebProfileSelection`. The Rust library API is part of the
trusted host TCB, not a sandbox: it cannot make a public cross-crate function callable
only by core. The security oracle is that no page, Bun process, renderer input, config
string before signature verification, IPC message, or generated client can reach this
Rust construction path. This shape avoids both a dependency cycle and raw path/UUID
inputs at backend constructors.

Each platform verifier produces the same 32-byte interface only after authenticating the
container and the app id it covers:

- Windows: `SHA-256("keld.publisher.windows/v1\0" || leaf_signer_spki_der)` after
  Authenticode/chain policy succeeds and the app id is read from the authenticated
  package relationship;
- macOS: `SHA-256("keld.publisher.macos/v1\0" || team_identifier_ascii)` after code
  validity succeeds and the app id is the signed code/bundle identifier;
- Linux: `SHA-256("keld.publisher.linux/v1\0" || ed25519_public_key)` after a detached
  Ed25519 signature verifies the literal package-manifest bytes containing `app.id`
  against the installer-pinned key.

`ProfileIdentity` is SHA-256 over the exact length-delimited byte sequence
`"keld.profile.identity/v1\0" || publisher_scope || u16be(app_id.len) || app_id`.
`CanonicalAppId` is 1–255 bytes of lowercase ASCII dot-separated segments; each segment
starts and ends with `[a-z0-9]` and otherwise contains only `[a-z0-9-]`. Validation
rejects noncanonical input rather than lowercasing or Unicode-normalizing it. The app id
and publisher scope are public identity material, not secrets; logs may name the app id
but must not expose filesystem paths, handles, cookies, or browser data.

The platform verifier owns the stability rule: its scope plus canonical app id is
identical across compatible updates/rollback and distinct across publisher scopes/apps.
A scope change deliberately changes identity and v1 preserves the old store without
copying it. T1 implements no package verifier and cannot claim a release identity;
T2/T3/T4 own their exact platform producer and real signed-container evidence. The
current dev compiler supplies no substitute.

Filesystem directory names use the full 64-lowerhex `ProfileIdentity`. The macOS store
UUID is deterministic UUIDv8 material made from the first 16 bytes of
`SHA-256("keld.wk-store/v1\0" || ProfileIdentity)`, with RFC variant and version bits
set. Because this intentionally compresses 256 identity bits into a UUID, fixed vectors
alone are insufficient. Creation first durably writes and fsyncs one `binding` intent
containing the full identity and UUID. Recovery resumes these idempotent phases: acquire
the registry lock; verify neither path is bound to another tuple; atomically create-new
without replacement and fsync `store-uuids/<uuid>/profile.owner.v1` first; then
create-new/fsync `identities/<full-identity>/profile.owner.v1`; enumerate identifiers;
construct the Apple store
only when no active store has previously been recorded; verify its identifier and
persistence; mark the binding `active`; then clear the intent. A crash after any phase
resumes it. An `active` binding whose Apple identifier is absent is corruption and fails
without silently creating an empty store. Both records contain the full identity and UUID
and must agree before lookup, reuse or purge; a mismatch is `KELD-WV-009`.

Before Apple removal, the lifecycle owner atomically commits a `purging` intent containing
the full identity and UUID. Normal lookup is forbidden while it exists. Recovery checks
identifier enumeration, treats an already absent Apple store as successful idempotent
removal, removes the reverse record and identity record in that order, then clears the
intent. A crash at any step resumes this sequence; it cannot recreate a store or accept a
one-sided binding.

### Platform policy

| Platform | Persistent owner | Ephemeral dev | Required read-back / limit |
|---|---|---|---|
| Windows | `FOLDERID_LocalAppData/Keld/profiles/v1/<identity>/webview2`; direct WebView2 environment receives the retained validated path | unique owner-private per-launch disk UDF removed only after browser exit | local volume; environment-reported UDF equals validated final path; exclusive-UDF option; no reparse escape; preserve/read back engine-required access; distinct ordinary user denied |
| macOS 14+ | identifier-addressed `WKWebsiteDataStore` using deterministic UUID plus Keld-owned Application Support identity/UUID records and lease; Apple owns physical store layout | one engine-owned `nonPersistent` store shared by every view in that host | both registry directions, configuration store identifier and persistence state before first view; non-idle host death quarantines persistence for that boot; no Apple physical-path claim |
| macOS <14 | persistent mode unsupported and typed `KELD-WV-009` | `nonPersistent` store | prove no default persistent fallback |
| Linux | explicit `WebKitWebsiteDataManager` with `$XDG_DATA_HOME/Keld/profiles/v1/<identity>/webkit` and `$XDG_CACHE_HOME/Keld/profiles/v1/<identity>/webkit`, then one engine-owned context | one engine-owned ephemeral manager/context shared by every view | live-WebView context→manager identity, exact data/cache getters and artifacts, `is-ephemeral`, current UID, owner-only modes and pre-existing-link containment; non-idle host death quarantines persistence for that boot |

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

The Windows identity parent, outside its `webview2` child, retains the ownership marker,
exclusive lease and durable `purging` intent. Purge commits/fsyncs intent first, closes
controllers, waits for browser-process release, clears/deletes only the validated UDF,
records completion, and clears intent last. Every interrupted phase resumes or fails
closed before environment creation; normal lookup cannot recreate a partial store.

Windows dev-ephemeral UDFs live at a unique owner-private
`FOLDERID_LocalAppData/Keld/ephemeral/v1/<launch-nonce>` leaf with a durable schema
marker and exclusive lease; they are never selected by a later app session. Graceful
exit waits for `BrowserProcessExited` before guarded deletion. After host crash, the next
host performs bounded marker-validated scavenging; a still-busy leaf remains quarantined
for a later pass and is never reused. Here `ephemeral` means nonpersistent session
selection, not a false guarantee that a crashed WebView2 process leaves zero disk bytes
immediately.

Linux follows the XDG rule that relative environment values are invalid. The host
resolves/defaults the absolute data and cache bases once before GTK/WebKit
initialization. The persistent control leaf
`$XDG_DATA_HOME/Keld/profile-control/v1/<identity>` is never deleted by browser-data
purge and owns the full-identity marker, durable intents and an exclusive close-on-exec
OFD lock. T4 must probe that the resolved data volume is local and supports the selected
lock primitive; unsupported locking fails persistent startup. The data leaf has a
`root_role=data` marker and the cache leaf a separate `root_role=cache` marker. Data is
created/validated first, cache second. An absent empty cache leaf may be recreated only
while the control marker and lease match; a nonempty unmarked cache leaf fails.

Linux purge retains the nondeletable control lease and intent. It destroys every live
view, then uses the WebKitWebsiteDataManager asynchronous clear operation and waits for
its completion rather than treating raw unlink as a helper-process exit oracle. It
verifies the live manager reports no remaining website data and that the browser state
is absent on a fresh context. The durable phases are `prepared`, `clearing`, `cleared`,
and `verified`; a crash in `clearing` reissues the idempotent manager clear, and intent is
removed only after fresh-context verification. Cache/data directories may remain empty;
no raw recursive deletion is required. Startup cannot recreate state while the control
intent is active.

Each Keld-owned component is `0700` and wrong-owner, group/world-writable, pre-existing
link, or final-path mismatch fails. WebKitGTK ultimately consumes path strings, so
retained handles do not prove protection from a concurrent hostile same-user swap; that
threat is explicitly outside AC14. The negative oracle covers links/substitution present
before manager construction and other-user mutation, not an unprovable same-user race.

Pinned wry 0.56.1 cannot construct a context with separate data/cache roots or retain one
shared explicit ephemeral context. T4 must first land/reuse an upstream wry API and
reviewed release that accepts an explicit persistent/ephemeral WebsiteDataManager,
separate roots, live-WebView context read-back, and async clear completion. A local fork
and a parallel Keld WebKit builder are forbidden by this contract. That dependency/API
slice is part of T4 and its review gate; T4 is blocked if the upstream facility remains
unavailable.

macOS and Linux persist engine-lifetime state in their nondeletable Keld control record.
Linux boot identity is the strict UUID read from `/proc/sys/kernel/random/boot_id`;
macOS boot identity is the strict UUID returned by
`sysctlbyname("kern.bootsessionuuid")`. An unavailable, malformed, wrong-sized,
environment-derived, wall-clock-derived, or caller-substituted value fails persistent
startup as `KELD-WV-009`; it never clears quarantine. Same-boot reads must be
byte-identical and each real reboot acceptance must observe a changed value.

Under the profile lease, startup writes/fsyncs `starting { host_pid, process_birth }`
plus that OS boot identity before constructing any engine object and advances it to `running` only after the selected
store/context is attached. Clean teardown writes `stopping`, completes the platform
release/clear barrier, writes `idle`, and only then releases the lease. A successor that
acquires the lease and finds `starting`, `running`, or `stopping` whose exact process
identity is no longer live atomically writes `quarantined` and returns `KELD-WV-009`
before store lookup. Same-boot elapsed time or a free process-local lock is insufficient.
After a real OS reboot, a changed boot identity proves the old engine processes cannot
survive; the next host revalidates registry/markers, returns `quarantined` to `idle`, and
reuses the same store. A leftover `idle` record is already admissible. Crash tests
terminate the host after each non-idle durable phase, require same-boot quarantine, and
require real reboot recovery; death after `idle` must remain restartable.

### Concurrency and lifecycle matrix

Lock order is global and never inverted: (1) packaging/update lifecycle lock for the
validated app generation, shared by ordinary startup and exclusive for
update/rollback/uninstall/purge; (2) platform registry lock when a reverse
namespace index is touched; (3) profile-identity control lease; (4) platform
engine/store operation and durable intent. Profile code never calls the updater
while holding its lease. An exclusive lifecycle operation blocks relaunch/publication,
and startup encountering an intent cannot bypass it. CI state-machine tests and each
real-platform lifecycle task race update/rollback/start against purge and fail any
reversed acquisition, deadlock, republish or fallback.

| Event | Identity/lease owner | Required result |
|---|---|---|
| second same-app host with equal validated namespace roots | profile manager | first owner retains exclusive lease; second gets `KELD-WV-009`; Windows also requires engine-exclusive UDF release; no fallback store |
| second different app | independent profile manager | distinct identity, lease and store |
| Bun generation restart | existing host | retain same lease/store; Bun never receives selection authority |
| graceful host restart | new validated host | reacquire same identity/store after clean engine teardown |
| death in `starting`/`running`/`stopping` | platform profile manager | Windows waits for exclusive-UDF release; macOS/Linux persist same-boot quarantine and recover only after changed real boot identity; death after `idle` remains admissible |
| display-name, cwd, executable or renderer change | none | no identity/store change |
| compatible update or rollback | signed package verifier | same tuple and store; profile schema must remain backward-compatible |
| authenticated identity change | profile manager | distinct store; preserve old store; no v1 cross-identity migration |
| validated Linux XDG root change | profile manager | use the new location and preserve the undiscoverable old roots; no v1 old-root inference or migration |
| ordinary uninstall | packaging | preserve profile by default |
| explicit purge | packaging + profile manager | durable intent, exact identity, package lock then profile lease, all platform release/clear barriers, and idempotent recovery; affect only owned state; macOS store removal precedes UUID-binding removal |
| test cleanup | test-owned root + profile manager | same purge protocol; no production user directory |

`KELD-WV-009` is the reserved profile-selection failure. Its detail distinguishes
missing authenticated identity, unsupported persistent store, namespace/containment
failure, actual-store mismatch, and profile-in-use; every message names the concrete
fix. Library code does not panic or retry deterministic failures.

## 5. Boundaries

T0 implements only this file and generated documentation if the source is allowlisted.

Future implementation ownership:

- `keld-core`: verify signed package identity, choose persistent versus dev-ephemeral
  mode, and call the lower `keld-wv` identity/profile API without exposing it to a child;
- `keld-pack`/the approved signing contract: produce authenticated package/container
  facts and signed app id; Linux also produces the detached Ed25519 manifest;
- `keld-core` platform verifier adapters in T2/T3/T4: validate Authenticode, macOS code
  signing, or Linux Ed25519 facts and mint `ValidatedAppIdentity`; these are identity
  gates as well as distribution gates;
- `keld-update`/packaging lifecycle: update/rollback continuity, package lock and purge
  authority;
- `keld-wv::profile`: host-agnostic identity derivation, namespace/registry state,
  profile selection and lease types consumed by platform constructors, plus read-only
  evidence needed by tests/doctor;
- platform backend modules: WebView2 actual-UDF/ACL/handle evidence, WK identified
  store, WebKitGTK manager/context;
- `keld-host`: carry the validated selection through app-session startup/teardown;
- tests/fixtures: real same-origin App A/B and same-app concurrency oracles.

Must not touch in T0: Rust/TypeScript behavior, `Cargo.toml`, lockfiles, config/boot
schema, KEL-79 policy, KEL-132 GPU/media installation, installers, update feeds, CI, or
agent instructions.

## 6. Tasks (each one scoped PR/artifact)

Every task artifact is `keld.execution-artifact/v1` and must contain exact
`issue_id=KEL-135`, the task/node ids below, this spec path plus approved blob SHA and
Linear approval-comment id, a landed `head_sha` that is an ancestor of fetched main,
and an acceptance array whose stable row ids carry class and `passed` status. A generic
passed artifact, wrong task, unlanded SHA, missing approval provenance, or `awaiting`
required row cannot satisfy an edge.

- [ ] **T0 — contract freeze** (`task_id=KEL-135/T0`,
  `node_id=webview-profile-identity-contract`): after explicit human approval changes
  `Status` to approved, land only this contract and its passed artifact; acceptance row
  `KEL-135/T0-contract` is `not-applicable` for OS evidence.
- [ ] **T1 — common identity/profile foundation**
  (`task_id=KEL-135/T1`, `node_id=webview-profile-identity-foundation`): requires the
  exact passed T0 artifact. Add the feasible `keld-core → keld-wv` verified-parts API,
  publisher-scope/app-id derivation vectors, common marker/registry/intent state
  machines, exclusive lease abstraction, `KELD-WV-009`, package/profile lock-order model,
  and explicit dev-ephemeral mode. It accepts only synthetic verifier output in tests;
  no release profile can start until a platform task lands its producer.
  Rows `identity-vectors`, `substitution`, `registry-recovery`, `lock-order`, and
  `dev-mode-state` are CI-only. It owns no engine/store product pass.
- [ ] **T2 — Windows WebView2 vertical slice** (`task_id=KEL-135/T2`,
  `node_id=webview-profile-windows`): requires exact passed T1. Consume the common
  manager after WinVerifyTrust/chain verification of publisher scope plus authenticated
  app id; in direct COM validate known-folder handles/reparse/local-volume/actual UDF,
  enable exclusive UDF, validate rather than replace the engine ACL, implement durable
  persistent and ephemeral cleanup intents, and pass real Windows rows `windows-app-ab`,
  `windows-package-identity`, `windows-dev-ephemeral`, `windows-second-user`,
  `windows-profile-containment-acl`, `windows-concurrency`, `windows-crash-release`, and
  `windows-purge-recovery`.
- [ ] **T3 — macOS identified-store vertical slice** (`task_id=KEL-135/T3`,
  `node_id=webview-profile-macos`): requires exact passed T1. Consume the common manager,
  validate code and extract Team Identifier plus signed identifier, require macOS 14+ for
  persistence, implement crash-recoverable bidirectional UUID
  binding and purge intents plus one engine-owned nonpersistent dev store, and pass real
  macOS rows `macos-app-ab`, `macos-second-user-state`, `macos-concurrency`,
  `macos-package-identity`, `macos-dev-ephemeral`, `macos-binding-recovery`,
  `macos-crash-quarantine-reboot-recovery`, `macos-purge-recovery`, and
  `macos-older-fail-closed`. Direct Apple physical-store path/ACL proof remains
  unverified.
- [ ] **T4 — Linux explicit-context vertical slice** (`task_id=KEL-135/T4`,
  `node_id=webview-profile-linux`): requires exact passed T1 and a reviewed upstream wry
  artifact/release that supplies explicit persistent+ephemeral managers, separate roots,
  live-context read-back and async clear completion. Use a nondeletable profile-control
  lease/intent leaf, prove every live WebView reaches the shared manager, verify the
  installer-pinned Ed25519 key and literal signed manifest/app id, then pass real
  Linux rows `linux-app-ab`, `linux-package-identity`, `linux-dev-ephemeral`,
  `linux-second-user`, `linux-context-manager-containment`, `linux-concurrency`,
  `linux-crash-quarantine-reboot-recovery`, `linux-purge-start-race`,
  `linux-clear-recovery`, and `linux-xdg-relocation`. The T4
  artifact must also record exact `wry_version`, crates.io checksum, upstream source
  commit, API symbols and dependency-review evidence. No local wry fork or parallel
  builder is allowed.
- [ ] **T5 — package/update/uninstall orchestration** (`task_id=KEL-135/T5`,
  `node_id=webview-profile-package-lifecycle`): requires all three exact passed T2, T3
  and T4 artifacts—an empty or partial set fails the edge. Implement only the common
  package lifecycle lock ordering, same-identity update/rollback reuse, preserve-on-
  ordinary-uninstall, and delegation to the already-landed per-platform purge primitives.
  Pass CI rows `package-lock-order`/`lifecycle-routing` and three distinct real rows
  `windows-update-rollback-purge-race`, `macos-update-rollback-purge-race`, and
  `linux-update-rollback-purge-race`.
No later task is frontier-ready merely because T0 lands. Each requires the named landed
predecessor, issue/claim authority, applicable platform availability, and a fresh
frontier artifact when its prompt requires one.

## 7. Test plan

| AC | Future owner | Class and independent oracle | Required negative control |
|---|---|---|---|
| 1–2 | T1 | CI-only synthetic publisher-scope/app-id vectors, strict parser and dependency-direction/API tests | accept noncanonical/display/config/Bun input or introduce `keld-wv → keld-core` |
| 1 | T2/T3/T4 | real platform signature/container verification and exact publisher-scope/app-id read-back | accept unsigned/tampered package facts or another publisher with the same app id |
| 3 | T1 | CI-only persistent-versus-dev mode state model; no engine pass | map missing identity to persistent/default state |
| 3 | T2/T3/T4 | real backend proof of unique nonreused Windows ephemeral UDF, one shared macOS nonpersistent store, and one shared Linux ephemeral manager | default persistent fallback, cross-launch reuse, or per-view ephemeral context |
| 4 | T1 | CI-only identity/UUID vectors plus crash-recoverable binding registry state model | alias two tuples or crash after each initial binding phase |
| 4 | T3 | real identifier enumeration/binding creation and active-binding missing-store failure | silently recreate an absent active Apple store |
| 5 | T1 | CI-only marker/registry state model; no platform containment pass | mismatched/nonempty unmarked logical leaf or UUID binding |
| 5 | T2/T3/T4 | separate real platform binding/root read-back; macOS reads Keld registry plus public store identifier, not Apple path | pre-existing link/marker/binding mismatch for that platform |
| 6–7 | T2 | real Windows environment UDF, local volume, final path, descriptor, exclusive crash-release, App A/B and second user | hostile loader override, remote volume, reparse ancestor, broad foreign-user grant, missing exclusive option |
| 8 | T3 | real macOS 14+ two-way UUID binding, identifier/persistence read-back, shared ephemeral store and same-origin App A/B | remove binding/identifier or run below 14 and require default fallback to fail |
| 9 | T4 | real live-WebView→context→manager identity, separate getters/artifacts, UID/mode and same-origin App A/B | default/parallel context, relative or post-selection root substitution, conflated roots, pre-existing symlink, one missing marker |
| 10 | T2/T3/T4 | real two-process exclusive lease for equal validated roots and different-app parallel success; a deliberate Linux XDG-root change is a separately selected location | delete lease, put it in a deleted leaf, race startup against purge, or add suffix/temp fallback |
| 11 | T1 + T5 platform rows | deterministic lifecycle state model plus real graceful restart/update/rollback; identity or Linux root change selects a new location and preserves old state | derive from display name, stage nonce or executable path; silently copy/delete/infer old state |
| 11 | T2/T3/T4 | real Windows crash-release or macOS/Linux same-boot quarantine after death in each non-idle phase plus changed-boot recovery; death after `idle` restarts | let a free process-local lease admit a successor with unproved lifetime, quarantine `idle`, or clear quarantine without reboot |
| 12 | T2/T3/T4 | each real platform's engine release/async-clear barrier plus durable exact-identity purge recovery | purge while live, after link substitution, with B identity, without intent, or crash after each platform phase |
| 11–12 | T5 | CI package/profile lock-order and routing plus real update/rollback/uninstall raced with each landed platform purge | reverse locks, relaunch/republish during intent, or treat a partial platform set as complete |
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
- public API: applies — future engine construction and host-TCB identity/selection shapes;
- permission model: applies — per-user filesystem ownership, engine-required access and
  purge authority;
- dependency addition: none in T0; every later addition is separately reviewed;
- wire protocol: none; no kipc bytes, HELLO, app-link, channel or error frame changes.

Packaging/signing review also applies to T2/T3/T4/T5 because authenticated platform
identity and lifecycle ordering cross that boundary. KEL-79 security/origin review
remains separate.

## 9. Perf impact

T0 has no runtime impact. Later persistent startup adds identity hashing, one lease, and
bounded filesystem/engine read-back on cold startup only. No steady-state IPC or frame
path changes. T2–T4 record cold-start change; a measured regression over architecture
01's 5% threshold needs a written waiver. Exclusive same-app ownership may reduce
cross-process engine sharing; this is a correctness decision, not a performance claim.

## 10. Open questions

None for T0. The approval recorded in the header explicitly accepts:

1. persistent release identity consumes the platform mappings above—Windows trusted
   signer SPKI, macOS validated Team Identifier, or Linux installer-pinned Ed25519 key—
   plus the authenticated canonical app id; publisher-scope change selects a new store
   and preserves the old one in v1;
2. unsigned current dev sessions are ephemeral rather than sharing persistent state;
3. same-app concurrent host processes with equal validated namespace roots are exclusive
   in v1, while an explicit Linux XDG data-root change selects a new preserved location;
4. persistent macOS profile support begins at macOS 14, with no default-store fallback;
   and
5. T5 cannot run until Windows, macOS and Linux vertical artifacts all pass; Linux T4
   additionally waits for the named upstream wry manager/context facility; and
6. after non-idle macOS/Linux host death, persistent state remains quarantined for the
   rest of that OS boot and is recovered only after a real reboot proves helper death.

That approval makes this contract `approved`; it does not make a later implementation
task frontier-ready or pass any platform row. T1–T5 still require their exact landed
artifacts, claims, review gates and OS evidence.
