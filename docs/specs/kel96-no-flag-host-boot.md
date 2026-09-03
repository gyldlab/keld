# Spec: no-flag host-as-app boot (Unique #1 lifecycle half)

Status: approved
Decision state: human-approved selections recorded; final-head review is PR merge evidence
Linear: KEL-96 · Owner: GYLDLAB · Updated: 2026-08-29
Decision digest: `sha256:053ad0c45ccdb76ae81c554e803275313dcdceca04ef8f021904bf99d64c45da`

## 1. Goal & non-goals

Make the shipping `keld-host` **process** the application lifecycle root when
invoked with no diagnostic flag. The host consumes one strict compiled boot
descriptor, owns the UI event loop and windows, owns the authenticated app-link,
and supervises Bun. `keld dev` may stage and launch the host, forward logs, and
observe its exit, but it must not retain a second window registry, listener,
token owner, restart loop, or application principal.

This remains the **boot/lifecycle** half of Unique #1. The release-artifact half
is KEL-103 or an approved successor. This specification permits an explicitly
non-release dev/fixture boot boundary before that release trust chain exists; it
does not turn a mutable sidecar digest into authenticity.

Non-goals:

- Implementing product code in this specification PR.
- Inventing a fifth unique.
- Wiring `RoleRegistry`, role grants, or principal-before-privileged-dispatch
  (KEL-97 after KEL-75).
- Loading or evaluating `keld.permissions.jsonc` (KEL-102).
- Implementing keld-auth / KEL-89, strict-profile sandboxing / KEL-78, packagers,
  installers, update feeds, signing code, or cross-compile farms.
- Claiming the Windows named-pipe/current-user-DACL transport is live (KEL-101).
- Promoting `--hello`, `keld hello`, echo, or IPC client diagnostics into an
  application owner.

## 2. Evidence and decision authority

The factual basis is landed research `83-host-vs-cli-ownership.md` blob
`912ba954f2ec2d7ce2177b923555a4dc5508d809`. Its central finding remains true at
Keld `fdef6165ba70d26e353cb5fdd27c2addfe9c36b2`: `HostOwnedHelloSession` is a
`keld-core` library owner, but the shipping `keld dev` process is still the CLI;
no-flag `keld-host` still prints a pre-alpha banner. KEL-75's landed generation
primitives and KEL-105's landed failure surfacing do not change that OS-process
ownership fact. Research is evidence, not implementation authority.

The eleven selections below were delegated to the specification writer in a
direct session and canonicalized as `keld.kel96-decisions/v1`, producing the
digest in the header. A human distinct from the writer subsequently approved
the exact candidate through an authenticated GitHub review:

- `approver_identity`: `github:0monish#155816356`
- `approval_source_id`: `github-pull-request-review:5046236033`
- `approval_source_url`:
  `https://github.com/gyldlab/keld/pull/104#pullrequestreview-5046236033`
- `approved_candidate_head`: `427c986ce2b7a872445c6696e3d1b43026876440`
- `approved_candidate_spec_blob`: `85d39bb837e738a05bb510acf1d6d62a71819f84`
- `decision_digest`:
  `sha256:053ad0c45ccdb76ae81c554e803275313dcdceca04ef8f021904bf99d64c45da`
- `approved_at`: `2026-08-27T22:33:11Z`

That review explicitly approved all eleven frozen decisions, authorized this
status/T0 finalization, and excluded product implementation and the stale
standalone `host-cli/04` node. PR #104's final-head review must identify the
resulting commit and spec blob before merge; that current GitHub review is the
external finalization evidence and cannot be embedded self-referentially in
this blob. GitHub review IDs are stable references, not immutable receipts.
Immediately before merge, the merger must re-fetch both the candidate and
final-head reviews and require `APPROVED` state, the expected reviewer identity,
exact `commit_id`, `submitted_at`, and body bindings to the final commit, spec
blob, and decision digest. A missing, edited, or dismissed binding fails closed.

Governing sources:

- `docs/architecture/01-overview.md` §§1–4: destination host authority,
  process ownership, reuse, and real-OS evidence.
- `docs/architecture/02-ipc.md` §§1, 7: authenticated link metadata,
  one-session v0 behavior, revocation, and lifecycle failure semantics.
- `docs/architecture/03-security.md` §§1–4: host-minted authority,
  default-deny, and the separation between boot integrity and permission
  evaluation.
- `docs/architecture/06-runtime-and-tooling.md` §§1–2: supervised Bun
  generations, CLI/host ownership, and the KEL-105 SURFACE behavior.
- `docs/specs/kel102-host-guard-enforcement.md`: fixed manifest descriptor,
  same-read verification, and the `KEL-102/T2` consumer boundary.
- Landed KEL-72 lifecycle code and conformance evidence; KEL-75 generation
  contract; KEL-100 OS records; and KEL-101/KEL-103 decision records.

## 3. Acceptance criteria

### KEL-96/T1a — compiled boot descriptor contract

1. A valid `keld.boot.json` is a bounded, strict schema-v1 document. Its exact
   closed fields are `schema`, `name`, `entry`, `renderer`, and `permissions`;
   `permissions` contains only `file` and `content_sha256`. Duplicate or unknown
   fields, an unknown schema, non-UTF-8 input, or input over 64 KiB is rejected.
2. `permissions.file` is exactly `keld.permissions.jsonc`.
   `permissions.content_sha256` is exactly `sha256:` followed by 64 lowercase
   hexadecimal characters and denotes the SHA-256 digest of the manifest's
   exact raw bytes. T1a/T1b validate and decode this field but do not compare it
   at host runtime; the artifact producer's consistency test compares it during
   staging, and KEL-102/T2 owns the first runtime compare on its one policy
   read. It does not authenticate an unsigned sidecar.
3. For the non-release fixture only, the `keld-cli` boot compiler creates an owner-private
   per-launch directory at `<project>/.keld/dev/<launch-nonce>/`, stages the
   prebuilt `keld-host[.exe]` and the compiled app files there, and returns the
   validated staged layout without launching it. The T1b integration harness
   invokes that host with no Keld flag; T2 is the first shipping `keld dev`
   launcher. The host canonicalizes `current_exe()` and selects the
   literal sibling `keld.boot.json`; the canonical executable parent is the
   fixture app root. The host rejects a missing, unreadable, non-regular,
   symlink-escaping, malformed, or version-mismatched sidecar. `entry` and
   `renderer` must be non-empty project-relative paths with no root, platform
   prefix, empty component, `.` component, or `..` component.
4. Working directory, environment payload, Bun child, webview, decoded frame,
   and IPC request cannot replace the selected sidecar, canonical app root,
   entry, renderer, permissions filename, or digest. Before creating an app
   resource, the host resolves `entry` and `renderer` under that root and proves
   each target is readable, regular, and canonically contained. A missing,
   directory, or symlink-escaping target is a typed boot failure.
5. T1a produces an immutable, host-owned descriptor contract. It cannot be
   declared complete as a standalone public struct, silent no-op success,
   marker-only diagnostic, or "T1b pending" error. Its first landing is atomic
   with the T1b no-flag host consumer on one KEL-96 head, while T1a and T1b keep
   separate acceptance identities and artifacts.
6. The fixture producer must stage the exact UTF-8 bytes `{}\n` as
   `keld.permissions.jsonc` and set `content_sha256` to
   `sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356`.
   The artifact-consistency test
   independently recomputes that value. The T1a host validates the fixed file's
   presence/readability/regular-file containment but neither hashes nor parses
   its policy bytes. KEL-102/T2 owns the single runtime read/hash/parse.
7. T1a creates no window, listener, Bun child, guard snapshot, privileged
   broker, release signature, or KEL-102 policy parse. KEL-102 consumes only the
   T1a descriptor contract from the landed atomic head.

### KEL-96/T1b — no-flag host consumer

8. Given the valid compiled descriptor, launching `keld-host` with no diagnostic
   flag opens a real native window using `name` and `renderer`, starts the Bun
   `entry`, owns their event loop/session in the `keld-host` process, and does
   not print or return through the pre-alpha banner path.
9. Every T1a invalid-input class is rejected with a registered typed error that
   states the fix before the host creates a listener, child, or window. There is
   no default descriptor, source-config fallback, or diagnostic fallback.
10. One private host session owns one authenticated app-link handshake, one
    reader, serialized writes, and channel dispatch for both echo and KEL-72
    lifecycle traffic. A second listener/link or a second `HELLO` for lifecycle
    is forbidden. `Quit` reaches the UI thread through the live backend's
    event-loop wake primitive and enters one idempotent host `Quitting` state.
11. Startup follows §4.4's state machine. Lifecycle `Ready` is emitted only
    after authenticated `HELLO` and initial-window registration/navigation
    readiness; it is never implied by connection or handshake.
12. T1b proves its minimum primary-session shutdown contract: correlated Quit
    reply, admission/dispatch quiesce, endpoint/token revoke, link close, child
    termination/reap, and host event-loop exit. Window-bound roles, grants, and
    privileged routes are dependent hooks owned by KEL-75/KEL-97/KEL-102, not
    claims of this slice.

### Later KEL-96 lifecycle results

13. `keld dev` launches and observes the host; process-tree and OS evidence show
   that the window, listener, token/generation, and Bun supervisor belong to
   `keld-host`, not the CLI.
14. Bun completes authenticated `HELLO` plus a `CALL`/`REPLY`, and a second call
    remains possible while the same host-owned window/event-loop lifetime is
    live.
15. On a recoverable Bun crash, KEL-96/T3 integrates KEL-75's existing
    endpoint/token/generation mechanism. It revokes the old link authority
    before provisioning the successor, keeps the host window alive, and proves
    the replacement child completes `HELLO` plus one call. It must not duplicate
    generation policy or claim KEL-97 role/principal/grant wiring.
16. A primary child exit not caused by an accepted Quit/host shutdown is never
    success, including exit status zero. Before `Ready` it is typed startup
    failure. After `Ready` it follows the approved restart policy or, until T3
    lands, triggers typed non-zero ordered host shutdown. KEL-116 must land
    before T1b/T3 can reuse the shared supervisor ledger.
17. Last-window close and explicit/default quit follow the exact ordering in
    §4.5. The old endpoint is unusable before a forced child termination/reap,
    and no Bun child remains after orderly host exit.
18. Retained diagnostics stay unprivileged and cannot become an alternate
    application owner or authority path.
19. macOS, Windows, and Linux product evidence is recorded separately. Windows
    may use the documented loopback interim only for no-flag process,
    window, lifecycle, and echo proof. Linux requires a real Wayland or X11
    desktop and a fresh no-flag-host ownership run. KEL-96 stays open until the
    task/platform matrix in §7 is complete.
20. T2 freezes CLI-death behavior: loss of the private dev-host lease initiates
    ordered host/Bun shutdown without transferring application-resource
    ownership to the CLI. Abnormal host-death descendant reaping is not claimed
    by KEL-96; KEL-75/KEL-78 own the per-OS mechanism and are required evidence
    before the corresponding platform lifecycle row can close.

## 4. Normative design decisions

### 4.1 Decision table

<!-- KEL96_DECISIONS_V1_START -->
| ID | Frozen decision |
|---|---|
| `KEL-96-D1` | Approve refreshed research-83 as factual process-ownership evidence only. |
| `KEL-96-D2` | Keep distinct `KEL-96/T1a` descriptor and `KEL-96/T1b` no-flag consumer identities on one atomic first head. Existing `KEL-102/T1` means that landed head plus its explicit permissions fixture; `KEL-102/T2` is the first runtime hash/parse consumer. |
| `KEL-96-D3` | Use Option A: strict compiled `keld.boot.json` schema v1. No no-flag `keld.config.ts` parsing. |
| `KEL-96-D4` | T1a uses only the exact owner-private per-launch dev layout in §4.2 and makes no authenticity claim. No release verifier/mode exists here; release boot is blocked on KEL-103 or an approved successor that binds the host and exact sidecar bytes/location/root relationship. |
| `KEL-96-D5` | Fix `keld.permissions.jsonc` and `permissions.content_sha256 = sha256:<64 lowercase hex>` as immutable host-owned descriptor values. |
| `KEL-96-D6` | Reject the standalone T1a execution terminal as theater. T1a and its first durable T1b consumer land atomically while retaining separate acceptance identities. L0 must rebuild the stale standalone execution node before implementation. |
| `KEL-96-D7` | KEL-116 precedes the no-flag window/recovery path. KEL-96/T3 owns Unix shipping integration by reusing KEL-75 generation primitives; human-promoted `KEL-75/T8` owns the Windows primary generation coordinator before KEL-96/T4 integrates it. KEL-97 retains RoleRegistry/principal/grant ownership. |
| `KEL-96-D8` | One private multiplexed primary app session owns KEL-72 wire behavior and the new KEL-96 startup/quit/revoke/link-close/reap state machine. Window-bound roles and authority stores remain dependent owner hooks. |
| `KEL-96-D9` | `keld-host --hello`, `keld hello`, `ipc-echo`, and `ipc-client` remain unprivileged diagnostics. |
| `KEL-96-D10` | Windows KEL-96 lifecycle/echo proof may use interim loopback without closing KEL-101 or authorizing privileged Windows dispatch; every platform must still close its ownership, restart, shutdown, CLI-death, and applicable host-death rows in §7. |
| `KEL-96-D11` | `keld-core` owns boot validation and returns only the opaque `ValidatedBootSelection` minted from the current executable's staged layout; `run_unprivileged` consumes it and cannot register privileged channels. `keld-host` is the thin first caller. `keld.boot.json` and the exact §4.8 Rust surface trigger public-contract, permission-model, and manifest-schema gates. |
<!-- KEL96_DECISIONS_V1_END -->

Deleting a decision row, changing its meaning only in a header/comment, or
marking the spec approved while any row is unresolved invalidates approval.
Normative task and test text below must carry the same decisions.

`decision_digest` is SHA-256 over the exact UTF-8 bytes between the two
`KEL96_DECISIONS_V1` marker lines, excluding both markers and normalizing the
checked-in Git text to LF with one final LF. The digest header is outside that
block, so an independent reviewer can recompute it without a circular hash.

### 4.2 Boot format and ownership

The v1 sidecar is generated from reviewed project configuration by tooling;
the no-flag host never evaluates TypeScript source.

```json
{
  "schema": 1,
  "name": "Example",
  "entry": "src/main.ts",
  "renderer": "index.html",
  "permissions": {
    "file": "keld.permissions.jsonc",
    "content_sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  }
}
```

The parser rejects duplicate keys rather than accepting first- or last-key
wins behavior. It rejects unknown fields so schema evolution requires a version
decision instead of silently dropping authority-relevant data. `name` must be a
non-empty string. `entry` and `renderer` are relative lexical paths meeting AC3;
the host resolves and containment-checks them under the canonical app root
before use. The host opens the sidecar and renderer with no-follow semantics,
validates regular-file identity/containment on the opened handles, and consumes
their bytes from those same handles. The fixed permissions file is likewise
opened no-follow for T1a metadata validation, but KEL-102 later owns the one
handle-based read/hash/parse. `entry` is opened no-follow, identity/containment
checked, and closed immediately before Bun spawn while no untrusted child yet
exists. The owner-controlled dev user remains the fixture trust boundary; this
does not claim release-grade resistance to same-user mutation.

The non-release fixture has one exact per-launch layout:

```text
<project>/.keld/dev/<128-bit-random-launch-nonce>/
  keld-host[.exe]            # new-inode copy/COW clone of current dev host
  keld.boot.json
  keld.permissions.jsonc
  <entry and renderer paths named by the sidecar>
```

The reusable `keld-cli` boot compiler is the sole shipping-layout producer. T1a
implements it; the T1b integration harness invokes it directly, and T2 later
wires that same owner into shipping `keld dev` launch/log/lease orchestration.
For the first macOS slice the compiler creates a `0o700` stage directory, stages
the files, computes the permissions digest, and later T2 executes the staged
host with no Keld argument. T1a accepts a current developer-built host path,
hashes its bytes, copies or copy-on-write clones it into a new inode, recomputes
the staged digest for equality, and removes write access before launch. This is
copy-integrity evidence only; the source is not called verified or release
authenticated. A hard link that lets same-user app code mutate a source/cache
inode is forbidden. When a verified CLI cache later exists, it may supply the
same compiler input without changing this contract. The host calls
`current_exe()`, canonicalizes its parent, verifies the Unix mode is still
exactly `0o700` after staging,
and selects the literal sibling `keld.boot.json`. It does not consult cwd,
environment, argv data, a child, or a request. That canonical parent is the
fixture app root. Windows protected-current-user-only DACL creation/read-back
and Linux exact-`0o700` staging are T4 platform work. Both are now implemented:
Linux also stages the dedicated strict-role launcher and validates the complete
replacement-safe ancestor chain before any app resource.

T1a has no release entry path or caller-selectable `Fixture`/`Release` boolean.
This dev rule is not a universal release layout: a shared cached host can serve
multiple apps, and a macOS bundle separates executable and resource directories.
KEL-103 or its approved successor owns a later signed production
container/locator and must authenticate:

1. the expected host target and version;
2. the exact raw `keld.boot.json` bytes;
3. the sidecar's location in the signed container; and
4. the signed relationship from that container to the canonical app root.

The current KEL-103 draft proposes verification of a standalone codesigned
Mach-O; no Keld release signer, verifier, or signed app-container binding is
live. That proposed scope authenticates none of the app-specific sidecar facts.
Until an approved predecessor lands there is no release boot mode to downgrade
into the fixture verifier.

### 4.3 KEL-102 handoff

The fixture producer writes exact bytes `{}\n` to `keld.permissions.jsonc`, sets
the v1 descriptor digest to
`sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356`,
and proves that relation in an artifact generation test. T1a's host path checks
only that the fixed file is a readable, regular, contained file; it does not
read, hash, or parse policy bytes.

Existing `KEL-102/T1` retains its landed meaning: the atomic KEL-96/T1a+T1b head
exists **and** its generated fixture includes that explicit permissions file.
It is not relabeled as a T1a-only record. KEL-102/T2 then receives the immutable
canonical app root plus fixed filename and decoded 32-byte SHA-256 digest. Its
guard-owned loader must read `keld.permissions.jsonc` once, hash those exact
bytes, compare the digest, and parse those same bytes. A host-side check followed
by reopening the path is forbidden. Adding `sha2` to a shipping crate is a later
dependency review gate; the workspace pin does not waive that review.

No descriptor field grants permission. A valid `{}` permissions file remains a
deliberate all-denied policy; missing or invalid policy is a KEL-102 startup
failure, not a T1a default.

### 4.4 Startup, session, and restart ownership

```text
Legacy OSes: keld CLI owns window + HostOwnedHelloSession + Bun
macOS T1–T3: keld-host owns validated boot + window + generated app-link + Bun
Dev tooling: keld CLI stages/launches/logs keld-host, but owns none of those resources
```

KEL-96/T3 integrates, rather than copies, KEL-75's fresh generation lifecycle.
The old endpoint/token/generation is revoked before successor provisioning.
KEL-97 later binds accepted generations to RoleRegistry principals and guarded
dispatch. KEL-116 makes every unrequested self-termination observable,
including status zero. In the no-flag window path, an exit before `Ready` is
startup failure; after `Ready`, a nonzero crash follows the shared restart
policy while status zero or a tripped crash loop is fatal unless an accepted
Quit/host shutdown caused it.

The T3 integration keeps one persistent macOS guardian and one guardian-side
`Supervisor`. A crate-private KEL-75 owner remains the sole generation counter,
listener, HELLO admission and revoke implementation. The host receives only an
opaque authenticated `BoundPrimaryGeneration`; core owns no child, PID, token,
listener or restart policy. The authenticated guardian-registration stream is
retained for fixed 404-byte `KGC1` records: `Prepare/Prepared`,
`Spawned/Registered`, `Revoke/Revoked`, and `Clear/Cleared`. Records are bounded,
big-endian, attempt-correlated and reject unsupported versions, nonzero reserved
bytes/padding and out-of-order transitions. `Revoked(g)` and retired-group clear
must both complete before `Prepare(g+1)`. These are private guardian-control
bytes, not public kipc frames. The separate liveness pipe retains EOF plus the
already-approved accepted-Quit attribution byte and one acknowledged
live-host orderly-cleanup byte. Raw EOF without either acknowledgment is host
death and never waits for an impossible host-side revoke reply.
Recovery is armed only after the initial `Ready` write succeeds. A concurrent
successor request waits for that decision; a failed write or earlier crash
rejects successor preparation and remains startup failure.

KEL-75/T8 originally extended the landed generation coordinator to Windows over
D10's explicitly unprivileged loopback interim. Its real-Windows predecessor proof owns
fresh provision/bind/revoke/restart and the authenticated bound-stream handoff
without duplicating policy in `keld-core`. KEL-96/T4 consumes the landed T8
artifact and does not implement another Windows generation or transport loop.
T8 did not select the named pipe at landing; KEL-101/T3 later migrated the shared
listener and current consumers to that current-user-DACL boundary without
changing KEL-96's lifecycle ownership. KEL-101/T4 independently owns the real
foreign-user denial and LIVE transport evidence.

One private `keld-core` logical app-session router owns at most one accepted
generation stream. Each generation performs one HELLO and has one reader plus
one serialized writer; revocation detaches and attempt-qualifies that reader
before a bound successor becomes current, and the terminal router owner joins
all retained reader handles. The router dispatches the existing echo and
lifecycle channel ids across generations without recreating the window.
`LifecycleSession` and `EchoServer` are evidence/reuse inputs, not concurrent
stream owners. No lifecycle-side listener or lifecycle HELLO exists.

The UI main thread owns boot validation, initial window creation/registration,
renderer navigation, and event-loop state. The app-session I/O reader sends UI
commands through the live backend's event-loop wake primitive. A T1b platform
implementation may add that seam only with a live backend in the same PR; a
hollow `WebEngine` method is forbidden.

Startup is an explicit host state machine:

1. `ResolveBoot`: locate, read, and strictly parse the sidecar.
2. `ResolveTargets`: validate the staged permissions file and canonicalize/open
   `entry` and `renderer` as readable regular files contained by the app root;
   load renderer bytes needed by the initial navigation.
3. `LoadGuardSnapshot`: absent for the deliberately unprivileged
   `run_unprivileged` T1b slice, which has no privileged-channel registration
   surface. Once KEL-102/T2 lands, KEL-102 must add a separately exact,
   public-reviewed privileged constructor carrying its immutable verified
   snapshot into this gate. That path performs one read/hash/parse and must
   succeed before any application resource; it cannot call `run_unprivileged`,
   skip the gate, or install a parallel loader.
4. `ProvisionLink`: mint one endpoint/token/generation and begin bounded
   admission.
5. `SpawnPrimary`: start Bun with that one canonical `KELD_APP_LINK` plus log
   sinks; the child receives no boot-root/path/digest authority.
6. `Authenticate`: accept one HELLO and bind the stream to this host session.
7. `CreateInitialWindow`: create/register the window and finish its initial
   renderer navigation on the UI thread.
8. `Ready`: send the KEL-72 `Ready` event; only now enter `Running`.

Failure in steps 1–3 leaves no application resource. Failure in step 4
idempotently revokes the minted endpoint, token, and generation before closing
the endpoint; stale credentials must fail while a later legitimate provision
still succeeds. Failure in steps 5–8—including a failed `Ready` write before
successful emission—revokes the generation, closes the link and
endpoint, terminates/reaps the live child handle, closes any partial window, and
returns the phase-specific typed error. Only a failure after successful `Ready`
emission and transition to `Running` follows the runtime exit/restart/quit rules;
it is not reclassified as startup failure.

### 4.5 Last-window and quit order

Landed KEL-72 is the source of truth only for the `Ready`,
`LastWindowClosed`, and correlated `Quit` reply wire observations. The new
primary-session coordinator owns the surrounding state/order:

1. The host's primary-window count transition from one to zero sends
   `LastWindowClosed` over the still-live app link.
2. If the app registered a `window-all-closed` listener, it may keep the host
   session alive. Without a listener, `@keld/electron` sends `Quit`. An explicit
   `app.quit()` enters the same path.
3. On `Quit`, the KEL-96 coordinator atomically enters an idempotent `Quitting`
   state and
   rejects new admission/dispatch work, then sends the correlated `Quit` reply
   before closing the link.
4. After the reply, the host revokes the primary endpoint/token/generation,
   closes the link, terminates and reaps Bun by its live
   process handle; finishes remaining window/event-loop teardown; and exits.
5. Natural exit retains KEL-75's portability caveat: `try_wait` may already have
   reaped the child. Revocation still precedes any successor provisioning.

Window-generation revocation and draining window-bound roles remain KEL-75/T4.
Role grants and guarded privileged routes remain KEL-97/KEL-102. KEL-96 exposes
ordered coordinator hooks for those owners when they land; it neither duplicates
nor claims their policy in T1b.

The current `HostOwnedHelloSession::finish` order is not the destination oracle;
it shuts down/reaps the supervisor before stopping its echo server. T1b/T3 must
prove the ordering above instead of copying that sequence.

### 4.6 Diagnostic boundary

`keld-host --hello`, `keld hello`, `keld ipc-echo`, and `keld ipc-client` may
own diagnostic windows or test listeners. They must not read `keld.boot.json`,
load a policy snapshot, mint `Principal::AppProcess`, register privileged native
channels, supervise the application, or become an alternate no-flag owner.

### 4.7 Cross-OS transport and acceptance

KEL-96 proves process and lifecycle ownership, not transport hardening. Windows
may use the currently documented loopback TCP plus v2 possession token for its
unprivileged lifecycle/echo evidence. That result is labeled interim and cannot
close KEL-101 or authorize a privileged Windows channel. KEL-101's own approved
scope remains the authority for the named-pipe/current-user-DACL gate.

The atomic T1b head initially enabled no-flag startup only on macOS. The T4
Windows and Linux slices remove that guard only after protected staging/readback,
strict boot/policy validation, one T8-owned supervisor, the live Windows WebView2
or Linux WebKitGTK command/event loop, and real process/window/restart/teardown evidence pass.
Linux consumes the landed KEL-78/T4 strict mechanism through the shared primary
supervisor and its real Wayland product rows. No unsupported platform may fall through to
the pre-alpha banner, `--hello`, CLI-owned session, or a partially installed
app-link.

KEL-100's macOS and Windows records prove the current CLI-owned concurrent path,
not no-flag host ownership. Their ordered witness method is reusable. Each KEL-96
platform run must freshly prove the `keld-host` PID owns the real window/session,
Bun is live, a second authenticated reply occurs, and orderly close reaps the
child. Linux additionally requires an interactive real Wayland or X11 desktop;
Xvfb, CI, WSL, cross-compilation, and another OS are not product evidence.

T2 adds one private dev-host lease from the CLI launcher to the host without a
new inherited-handle protocol. The CLI spawns the host with stdin as an owned
pipe, retains only its writer, and sets `KELD_DEV_LEASE=stdin-v1` to classify
that existing stream as liveness-only. The host owns only the reader and marks
it non-inheritable before spawning Bun; Bun receives null stdin. Standalone
no-flag T1b has no `KELD_DEV_LEASE` and does not monitor terminal stdin.

CLI exit/crash closes the only writer and yields EOF. `CliLeaseLost` is a
distinct shutdown cause: it quiesces new work and joins the common
revoke/link-close/terminate/reap/event-loop-exit tail, but sends no impossible
Quit reply because no lifecycle Call exists. A forged environment value can at
most make the caller's own host monitor stdin and shut down; it carries no app
root, path, digest, principal, permission, or application-resource ownership.
Tests inspect both process handle tables: the host never owns a writer copy and
Bun owns neither end; otherwise EOF cannot satisfy the acceptance.
The Windows fixture uses raw `SystemExtendedHandleInformation` for CLI, host,
and Bun, cross-checks each count with `GetProcessHandleCount`, binds the host's
reported debug-only stdin handle value to the raw File entry, verifies read-only
access with no inherit attribute, and uses duplicated-handle object comparison
to prove Bun has no copy. A temporary inherit-bit mutation must fail that same
oracle; a process list or aggregate count alone is insufficient.

Windows dev-stage deletion uses the approved private
`keld.windows-dev-stage-cleanup/v1` sentinel. The CLI launches the installed,
non-staged `keld-host.exe` in that role after the staged host exists but before
releasing its no-share-delete namespace guards. The sentinel opens the exact
staged-host PID while the CLI still owns it, verifies from that live handle that
the process image is `<validated-stage>/keld-host.exe`, retains the process
handle, and reports no application readiness or authority. The CLI then releases
the namespace guards. The sentinel waits for the exact host object, rechecks
the protected current-user-only stage DACL, and deletes the nonce directory
after host exit. Nonce layout validation occurs once during sentinel prepare.
It is a CLI child and host sibling, so it
is never enrolled in or broken out of the host Job.

The private role receives only the stage deletion target and staged-host PID;
both are validation inputs, not authority to select an app. It cannot enter
boot validation, open an app-link, own a window, supervise Bun, or mint a
principal. Sentinel spawn failure drops the lease, waits/reaps the host,
releases the guards, deletes the stage, and fails `keld dev`; continuing without
a surviving cleanup owner is forbidden. Host self-delete, an inherited Job
handle, Job breakaway, a shell/PowerShell janitor, and reboot/next-launch-only
deletion are rejected. Direct-session approval and the atomic owner record are
captured in KEL-96 comment `bf1228bd-1128-47fc-ba98-865d3abbf076`.

Abnormal host-death descendant reaping remains the explicit KEL-75/KEL-78
platform dependency. Windows and Linux may close their KEL-96 lifecycle rows
only with the named reaper evidence from those specifications. macOS remains
awaiting a primary-sourced reaper mechanism; process-group or `launchd` claims
are not substitutes. KEL-96 records and consumes those artifacts but does not
invent a parallel reaper or claim strict-profile containment. Artifact presence
alone is insufficient: each platform row kills the real no-flag `keld-host` and
independently observes that its enrolled Bun descendant is gone and a subsequent
launch succeeds.

### 4.8 API boundary

`keld-core` owns sidecar selection/parsing, no-follow target opens, renderer-byte
loading, owned paths, decoded digest, and the single app-session coordinator.
T1a/T1b expose no raw parser, descriptor fields, or caller-supplied target paths.
They add this exact reviewed API in `keld_core::app_session`:

```rust
pub struct ValidatedBootSelection { /* private fields */ }

impl ValidatedBootSelection {
    pub fn from_current_exe_unprivileged() -> Result<Self, HostAppError>;
}

pub fn run_unprivileged(boot: ValidatedBootSelection) -> Result<(), HostAppError>;

pub struct HostAppError { /* private fields */ }

impl HostAppError {
    pub fn code(&self) -> &'static str;
}
```

`from_current_exe_unprivileged` performs the exact T1a validation state: it
derives the staged root from `current_exe`, reads/parses the sidecar from its
validated handle, rejects an empty `name`, validates/owns `entry`, loads renderer
bytes from its validated handle, and retains the fixed permissions descriptor.
All fields stay private. `run_unprivileged` consumes that opaque value,
owns the one multiplexed app session and UI event-loop lifetime, blocks until
ordered host exit, and returns the registered typed failure. `HostAppError`
implements `Display`/`Error`; its display text includes the fix. No boot bytes,
permissions path/digest, listener, stream, token, generation, window handle, or
child handle is caller-constructible or crosses this API. It cannot register a
privileged channel. The first and only production caller in T1b is the no-flag
`keld-host` main path. Internal parser/session/router types stay private.

## 5. Boundaries

- Implement after approval in: `crates/keld-host` (thin no-flag caller),
  `crates/keld-core` (boot validator, the exact §4.8 API, and private
  multiplexed session), `crates/keld-cli` (T1a boot compiler; T2
  launch/log/lease orchestration), and `crates/keld-wv` only for a live
  backend event-loop wake/navigation seam implemented and proved in the same
  platform slice. Generated fixture files, tests, and current-state
  architecture labels follow the behavior they prove.
- Must not touch without separate approval: any public boot API beyond §4.8;
  KEL-102 guard
  loading/evaluation; `RoleRegistry`/role grants; KEL-78 sandbox admission;
  `keld-runtime`/`keld-ipc` generation or transport changes not owned by a
  landed KEL-75/KEL-101 predecessor; `keld-pack`; release signing; updater;
  manifest generator; CI routing; kipc frame/HELLO bytes.

## 6. Tasks and dependency contract

- [x] T0: A human distinct from the writer approves the exact current PR head,
      final spec blob, and decision digest; then and only then change
      `Status: draft` to `Status: approved` and obtain current-head review again.
- [x] T0g: L0 replaces the stale standalone descriptor node with an atomic
      T1a/T1b execution node and reissues the frontier before implementation.
- [x] T1a: Implement and test the private schema-v1 descriptor/trust boundary.
      The producer stages the explicit permissions fixture/digest. The host
      validates its fixed file metadata but creates no application resource and
      exposes no public Rust API.
- [x] T1b: After KEL-116 lands, on the same first KEL-96 implementation head
      as T1a, make no-flag
      `keld-host` the durable consumer; add the single-link echo/lifecycle router,
      startup state machine, live-backend UI wake, minimum primary-session Quit
      shutdown, and first real macOS window/session proof. T1a and T1b have
      separate tests and artifacts even though their first landing is atomic.
- [x] T2: Wire shipping `keld dev` to the T1a boot compiler, launch the staged
      host, forward logs, and prove process/handle ownership, concurrent
      second-call behavior, and dev-host-lease shutdown when the CLI dies.
- [x] T3: After KEL-116 records all self-termination, integrate fresh KEL-75
      link generation and prove same-window macOS recovery; retain the qualified
      KEL-105 SURFACE behavior until this passes.
- [x] T4: Integrate the landed KEL-75/T8 Windows generation/recovery coordinator
      into the no-flag host, complete the remaining per-backend host lifecycle
      integration, run the real Windows and Linux product rows, and consume the
      applicable KEL-75/KEL-78 host-death reaper artifacts. This is
      implementation plus evidence, not an evidence-only task; it must not
      implement a second generation loop in `keld-core`.
      The Windows sub-row has real no-flag host/HWND/two-call/g1-to-g2/ordered
      Quit/CLI-death evidence on RAMANI. It now also consumes KEL-78/T3's
      non-breakaway Job through the real no-flag host: killing only that host
      reaps Bun plus a real descendant, the cleanup sentinel deletes the stage,
      and a fresh no-flag launch succeeds. Raw CLI/host/Bun handle census and
      post-CLI-death stage deletion also pass. Linux implementation head
      `cfb8a238d6ff8b61c27cf825dd31ee3b2d30f126` stages a dedicated minimal
      strict launcher and consumes KEL-78/T4 inside the existing primary
      supervisor. On native Ubuntu GNOME Wayland, all eight product tests pass:
      fresh no-flag host/Bun/descendant identities, two calls, ordered Quit,
      same-window recovery, CLI-death cleanup, exact stage deletion, invalid
      boot/lease pre-resource failure, and host-only SIGKILL of a four-process
      strict tree followed by fresh relaunch. Removing only
      `--die-with-parent` makes the host-death row fail on a surviving recorded
      process identity; no retry, external tree kill, or second supervisor is
      used. The current runtime-file manifest is explicitly Ubuntu/Debian
      x86_64; X11 and non-Debian evidence remain separate KEL-28 follow-up.
      The CLI atomically protects and retains no-share-delete handles for
      `.keld` and `dev` before nonce creation (plus the canonical project
      handle), then pins and validates
      the nonce before writing any staged child. It rechecks the locked stage
      DACL and host digest before launch. Windows direct-child terminal paths
      signal capture readers to drain currently buffered bytes and stop without
      waiting for descendant EOF; this preserves ledger ordering and prevents
      successor/host-exit delay without claiming descendant process ownership.
      Lease monitoring and a locked broken-pipe preflight precede listener/child creation,
      accepted shutdown closes successor admission before the reply tail, and
      revoked-attempt tombstones reject a bound stream observed after its
      revocation.
- [x] T5: Update architecture 01/02/06 LIVE/TARGET labels only to the behavior
      actually proved by landed T1a–T4 work.

KEL-102 dependency mapping:

- Existing `KEL-102/T1` is satisfied only by the landed atomic KEL-96/T1a+T1b
  head **and** its explicit generated `keld.permissions.jsonc` fixture/digest.
- `KEL-102/T2` is the first KEL-102 implementation consumer and may depend on
  the T1a artifact from the atomic T1a/T1b head; it does not depend on KEL-96
  T2–T5.
- `KEL-96/T1a` cannot claim KEL-102's guard snapshot, verified manifest parse,
  privileged listener, broker, or permission-model completion.

The landed Prompt Tracker `host-cli/04` node assumes a standalone T1a terminal
and is stale under D6. L0 must rebuild that node and its frontier before any
implementation claim. This specification PR does not edit Prompt Tracker.

Before the replacement node can be ready, L0 also requires a landed
`keld.execution-artifact/v1` with `node_id=app-self-termination`,
`issue_id=KEL-116`, `status=passed`, a passed
`acceptance.id=KEL-116/self-termination`, and `head_sha` proven to be an ancestor
of current Keld `origin/main`. KEL-116 must first be promoted and scoped by its
owner; this specification does not self-authorize its implementation.

Windows T4 also requires a landed artifact with
`node_id=windows-primary-generation`, `issue_id=KEL-75`, `status=passed`, a
passed `acceptance.id=KEL-75/T8`, and an ancestor-of-`origin/main` `head_sha`.
If T8 selects named-pipe transport, its approved dependency set must additionally
include the matching passed KEL-101 transport artifact. Removing the T8 hard
edge must make the replacement graph readiness test fail.

The replacement node emits one schema-valid `keld.execution-artifact/v1`:

- `node_id=host-boot-and-session`, `issue_id=KEL-96`, `status=passed`, one
  landed `head_sha`, and acceptance rows
  `{"id":"KEL-96/T1a","class":"ci-only","status":"passed"}` and
  `{"id":"KEL-96/T1b","class":"real-os","status":"passed"}` with their
  schema/fixture/negative-control and initial-platform product evidence.

L0 exposes that node to KEL-102/T2 only when both acceptance rows pass on the
one landed head; KEL-102 consumes the T1a evidence. It must replace the old
node, frontier edge, and graph tests. Removing the KEL-116 hard edge or allowing
the old standalone `host-boot-descriptor` artifact must make the graph test
fail.

## 7. Test and approval plan

| Contract | Required oracle |
|---|---|
| Decision completeness | Exactly one `KEL-96-D1` through `KEL-96-D11`; `Status: approved` is rejected without every row plus approver/source/head/blob metadata. |
| T1a/T1b distinction | Removing either task identity fails the dependency-contract check; no test may let T1a claim a window or let T1b bypass the descriptor. |
| Strict boot bytes | Valid bounded schema returns exact owned values; duplicate/unknown/version/UTF-8/size/path and malformed digest-encoding cases return typed errors with fixes. The artifact test compares the staged digest; KEL-102 owns runtime mismatch rejection. |
| Host-selected locator | cwd, environment, child, frame, and IPC substitution attempts do not change root/path/digest. Replacing a sidecar or renderer after locator calculation either leaves the already-open consumed bytes unchanged or produces a typed identity failure; KEL-102 applies the equivalent same-read test to policy bytes. |
| Trust boundary | Mutating unsigned sidecar bytes cannot be described as release-authenticated; no release mode/acceptance exists until the KEL-103/successor verifier succeeds on those exact bytes. |
| Pre-resource failure | Every invalid boot case records no listener endpoint or child PID and directly proves the host window registry plus the platform-native window-handle census are empty before the typed error returns; absence of a window-ready marker alone is insufficient. |
| Real consumer | Product integration invokes the no-flag binary; private-return-value inspection alone is insufficient. |
| Restart | Reverting revoke-before-successor or reusing the retired link fails the same-window recovery test. |
| Shutdown | Reordering link revocation after forced reap fails endpoint/reap observations; `Quit` reply still arrives before link close. |
| Diagnostics | Diagnostic paths cannot load boot/policy, mint app identity, or register privileged handlers. |
| OS evidence | T1a parser/state is CI-only; T1b/T2/T3/T4 window/process/restart/transport observations are real OS/device; T5 document checks are CI-only but consume those real artifacts. |

Task/platform completion matrix:

| Result | CI-only oracle | macOS real desktop | Windows real desktop | Linux real desktop |
|---|---|---|---|---|
| T1a descriptor/fixture | strict schema, artifact digest, substitution and mutation negatives | not applicable | not applicable | not applicable |
| T1b no-flag owner/session/shutdown | state model, same-stream echo+lifecycle, cleanup negatives | required on atomic first head | implemented/proved by T4 | implemented/proved by T4 |
| T2 CLI delegation/second call/CLI death | lease state model and process fixtures | required before T2 passes | required by T4 | implemented/proved by T4 |
| T3 fresh generation/window continuity | generation ordering and stale-link negatives | required before T3 passes | Windows equivalent implemented/proved by T4 | implemented/proved by T4 |
| Orderly shutdown/reap | state/child-process negatives | required by T1b/T3 | required by T4 | implemented/proved by T4 |
| Abnormal host death | consume KEL-75/KEL-78 artifact, never a mock | named reaper plus real kill-host/child-gone/relaunch run; currently awaiting mechanism | KEL-78 Job consumed; real host-only kill reaps Bun + descendant, deletes stage, and relaunches | KEL-78/T4 consumed; real host-only kill reaps Bun + descendant, deletes stage, and relaunches |

KEL-96 remains open while any required cell is awaiting; Linux has no remaining
cell in this matrix, while macOS abnormal host death still awaits its named mechanism. A platform's T4 row
cannot pass on ownership/echo evidence while its restart or teardown cell is
missing.

Documentary negative controls for this specification candidate:

1. Remove the T1a/T1b row or task mapping; the decision/dependency check must
   fail.
2. Delete the release trust owner or the fixed KEL-102 permissions descriptor;
   the approval check must fail.
3. Change only `Status: draft` to `Status: approved`; the approval check must
   fail until stable approver/source id, final commit, final spec blob, and
   current-head review are present.

Implementation tests use temp directories, port 0, real child processes, and
observable readiness/exit/cleanup. They must not sleep-sync, retry a deterministic
failure, inflate timeouts, substitute mocks for OS behavior, or weaken a failing
test. Critical boot/restart/shutdown tests require temporary negative-control
mutations and named failing tests.

## 8. Review gates

For this specification-only PR:

- unsafe: none.
- public API: **yes** — `keld.boot.json` is a new external producer-consumer
  contract even though its Rust implementation types remain private.
- permission model: **yes** — the fixed permissions filename/digest and trusted
  handoff to KEL-102 determine which policy bytes a future host may evaluate.
- dependency addition: none in this documentation PR.
- wire protocol / manifest schema: **yes** — schema v1 is a new versioned boot
  manifest contract. Existing kipc frame and HELLO bytes remain unchanged.

Future implementation gates:

- Public Rust API: **mandatory for T1b/T3** — review the exact §4.8
  `keld_core::app_session::{ValidatedBootSelection, run_unprivileged, HostAppError}`
  surface, any public `keld-wv` wake/navigation seam, T3's
  `keld_runtime::primary::BoundPrimaryGeneration`, and
  `keld_runtime::macos_guardian::{GuardedPrimary, GuardedPrimaryUpdate}`. T3
  exposes authenticated streams and terminal operations, never raw generation
  mutation, tokens, child handles or control records. T1a's parser/descriptor
  remain private; later KEL-102 API evolution requires another gate. The
  artifact API gate above also applies.
  T4 additionally exposes `PrimaryRecoveryGate`, the supervisor-owned
  link-failure restart request, platform-neutral app-window command/events, and
  the one shared Windows dev-stage ACL validator used by producer and host;
  those exact surfaces require the same human public-API review.
- Dependency addition: T4 adds target-only `windows-permissions` 0.2.4 plus
  direct already-locked `windows-sys` constants, handle isolation, and atomic
  directory creation bindings. `winapi` remains only the wrapper's transitive
  dependency. Their maintenance age, safe-wrapper boundary, features and
  alternatives require human review.
- Permission model: KEL-102 additionally owns manifest loading/evaluation and
  its separate human gate.
- Wire protocol: existing HELLO/lifecycle/echo bytes remain unchanged; any kipc
  change is another wire review beyond the boot-manifest gate above. T3's
  fixed bounded `KGC1` guardian record is a reviewed private wire extension on
  the already authenticated registration stream; it is not a kipc channel.
- Unsafe: **T4 Windows applies** to one `CreateDirectoryW` call that supplies
  the already-built self-relative security descriptor atomically at stage-root
  creation, one `SetHandleInformation` call that clears inheritance on the live
  borrowed stdin lease before Bun spawn, and read-only `PeekNamedPipe` calls
  for lease/capture state. The cleanup sentinel also uses `OpenProcess` plus
  `OwnedHandle::from_raw_handle` to acquire the exact host object,
  `QueryFullProcessImageNameW` to verify its staged image, and
  `WaitForSingleObject` to await that object before deletion. These blocks are
  Windows-only and carry pointer/handle-lifetime proofs; mandatory human review
  of unsafe and security-sensitive code applies.
  Named-pipe transport FFI remains KEL-101.

## 9. Performance impact

No performance claim is made by this documentation PR. The boot descriptor is
bounded at 64 KiB and parsed once on the cold startup path. A later implementation
must attribute any cold-to-window or RSS claim using the architecture 01 §5
fixtures; moving work into Rust is not itself a measured improvement.

## 10. Remaining gates, not open architecture questions

The eleven architecture decisions are frozen and the approval provenance is
recorded in §2. Four execution gates remain:

1. Rebuild the stale standalone T1a Prompt Tracker node/frontier so its first
   implementation landing includes the durable T1b consumer without merging
   their acceptance identities.
2. Promote and land KEL-116's all-self-termination ledger fact before T1b/T3
   reuse the supervisor for a no-flag window lifetime.
3. Obtain/consume the applicable KEL-75/KEL-78 per-OS abnormal-host-death reaper
   artifacts; macOS still lacks a named mechanism.
4. Human-promote and land KEL-75/T8's Windows primary generation coordinator;
   include KEL-101 only if T8 selects named-pipe transport.

The approval gate passed through authenticated GitHub review `5046236033`.
PR #104 must still record a review by that distinct human against the exact
final status/T0 head and identify its commit and spec blob before merge; that
review is merge evidence, not a fifth implementation gate.

Neither gate authorizes product implementation, marks KEL-96 Done, or implies
that T1a completes later KEL-96 T2–T5 work.
