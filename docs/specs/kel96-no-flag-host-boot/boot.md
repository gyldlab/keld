# KEL96 descriptor and startup-session contract

Part of: [index](index.md)
Owns: sections 2–4

## 2. Spec refs

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

GH452 T2a (owner-delegated 2026-11-01, comment 6549729612) consumes the already validated
name/root and live development lease as data, without extending any descriptor schema.
The private delivery contract is `gh452-app-session-facts/contract.md` T2a (design §4). Unavailable metadata
remains absent; this amendment does not select package.json, locale or OS-path values.

GH452 T2b (delegated PR1362 comment 6550291308; independently reviewed design) adds
one optional `version` metadata field to the existing closed descriptor shapes. The
current literal root config supplies a nonempty, NUL-free string of at most 4096
UTF-8 bytes through the same config capture. Absent descriptors remain byte-identical;
present null, duplicate or invalid values fail closed. Core retains the value without
reopening config/package files. Older Hosts reject this added field; the compiler must
ship its matching Host. This does not allocate schema 3/4 or change profile/graph
ownership. GH452 T2b (design §4) owns the non-authority native path snapshot and delivery bounds.

GH452 T2c (`gh452-app-session-facts/contract.md` T2c (design §4)) versions only the private launch-facts
JSON to schema 3 for an optional Host preferred-language value. It leaves this boot
descriptor and KEL78's profile schema numbers unchanged. Older private facts stay
readable; the existing Host/Guardian retain one captured value for all generations.

### KEL-96/T1a — compiled boot descriptor contract

#### AC-1: Compiled boot descriptor is a strict closed schema

- **Given:** the shipping Host receives a compiled `keld.boot.json` descriptor.
- **When:** the Host validates this schema-v1 document.
- **Then:** it enforces a 64 KiB UTF-8 bound and requires `schema`, `name`,
  `entry`, `renderer`, and `permissions`. The shipping Host also admits only
  the separately approved optional `version` (GH452 T2b) and `auth` (KEL89)
  fields, under their owning validation contracts. `permissions` contains only
  `file` and `content_sha256`. Reject duplicates, other unknown fields, unknown
  schemas, invalid UTF-8 and oversized input.
- **Negative control:** admit one extra or duplicate descriptor key.
  - **Single fault:** skip closed-field verification.
  - **At-risk precondition:** the boot descriptor is processed before any resource creation.
  - **Expected RED:** the malformed-descriptor refusal test accepts the extra field.

2. `permissions.file` is exactly `keld.permissions.jsonc`.
   `permissions.content_sha256` is exactly `sha256:` followed by 64 lowercase
   hexadecimal characters and denotes the SHA-256 digest of the manifest's
   exact raw bytes. T1a/T1b validate and decode this field but do not compare it
   at host runtime; the artifact producer's consistency test compares it during
   staging, and KEL-102/T2 owns the first runtime compare on its one policy
   read. It does not authenticate an unsigned sidecar.
3. For the non-release fixture only, the `keld-cli` boot compiler creates an owner-private
   per-launch directory with the `.keld/dev/<launch-nonce>/` suffix, stages the
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
6. The historical fixture producer and the current Windows/Linux development
   producer stage the exact UTF-8 bytes `{}\n` as
   `keld.permissions.jsonc` and set `content_sha256` to
   `sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356`.
   The artifact-consistency test
   independently recomputes that value. The T1a host validates the fixed file's
   presence/readability/regular-file containment but neither hashes nor parses
   its policy bytes. KEL-102/T2 owns the single runtime read/hash/parse.
   Approved macOS successor: [KEL-140](../kel140-macos-public-filesystem.md) §4.5
   selects only the fixed project-root `keld.permissions.jsonc` through retained
   no-follow root/leaf handles and invoking-owner checks. A genuinely absent
   project file preserves these exact historical bytes/digest. A present file is
   bounded-captured once; the same owned buffer is staged and hashed. Unsafe,
   unreadable or over-budget present input never becomes fallback. The CLI does
   not parse or authorize; KEL-102 retains sole runtime policy validation before
   app resources. Historical T1a artifacts and other-platform behavior are unchanged.
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

## 4. Design: boot and session ownership

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

**D4 applicability clarification (outside the frozen decision block):** the earlier
exact-content-approved KEL-254 contract is the Windows direct-install successor for
binding the current host and exact sidecar bytes/location/root relationship. The
multi-mode expansion is approved by exact-content approval recorded in Linear comment
`e0b276f9-5ecc-42a9-ac8f-8a5e05f44245`; that approval leaves its implementation and
native qualification gates open. Neither contract changes T1a's owner-private dev
layout or adds a caller-selected release mode. The KEL-96
consumer (KEL-254 T3 Part B) implements Windows installed boot over KEL-53's
executable-located active selection and the existing KEL-135 verified signer/app
identity; its native boot evidence covers `PerUserDirect` only, and each machine-mode
cell stays unqualified until KEL-53's native install-mode proof and the consumer's own
native boot evidence exist for it. Per-user direct and
machine-wide direct installs have separate access/authority acceptance cells;
machine-seamless activation additionally remains behind KEL-270's lifecycle and
authority proof gate. Managed/package-manager installs remain owned by their deployment
mechanism. KEL-254 amendment A3 makes the executable path a locator only and keeps two
Windows boot states: dev lease, or authenticated installed provenance, otherwise refuse.
This note is applicability only: it does not change or reapprove any frozen
KEL-96 decision row.

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
non-empty string. `entry` and `renderer` are relative lexical paths meeting acceptance criterion 3;
the host resolves and containment-checks them under the canonical app root
before use. The host opens the sidecar and renderer with no-follow semantics,
validates regular-file identity/containment on the opened handles, and consumes
their bytes from those same handles. The fixed permissions file is likewise
opened no-follow for T1a metadata validation, but KEL-102 later owns the one
handle-based read/hash/parse. `entry` is opened no-follow, identity/containment
checked, and closed immediately before Bun spawn while no untrusted child yet
exists. The owner-controlled dev user remains the fixture trust boundary; this
does not claim release-grade resistance to same-user mutation.

The non-release fixture has one exact executable-adjacent per-launch layout:

```text
<platform-stage-namespace>/.keld/dev/<128-bit-random-launch-nonce>/
  keld-host[.exe]            # new-inode copy/COW clone of current dev host
  keld.boot.json
  keld.permissions.jsonc
  <entry and renderer paths named by the sidecar>
```

Linux and Windows keep `<platform-stage-namespace> = <project>`. The bounded
macOS isolation continuation uses
`<canonical-system-temporary>/keld-dev-<invoking-uid>/<project-id>`, where
`project-id` is lowercase SHA-256 of the canonical project path's native bytes.
This is a directory locator, not authentication. The temporary directory must
already be invoking-owner private (`0700`) and outside the project. The CLI
creates/opens each private namespace directory relative to retained no-follow
parent handles and refuses wrong ownership, mode, type or links without repair.
It also refuses `node_modules`, `package.json`, `tsconfig.json` and `jsconfig.json`
in stage ancestors, and rechecks retained private-parent identity, owner/mode and
the ancestor-input census before completing capture. This removes the live
project's ancestor-resolution path. Before Host handoff, the existing CLI staging
guard retains those parents and opens the nonce relative to the retained `dev`
handle. Every failure unwind requires the full chain and nonce to retain their
named owner/mode and device/inode identities before deletion; an unknown or changed
identity preserves the incomplete stage and replacement path. That identity check
does not repeat the resolver census, so a late-input refusal can still clean its
own nonce. The existing random nonce and executable-derived Host/Guardian cleanup
continue to own the running stage. This corrects the old macOS project-local
location under GH1037/KEL-75; source-tree copying, graph admission and
startup-environment qualification remain separate pending work.
The Host's required qualification before every Bun generation is not supplied by
this CLI capture check; retained CLI handles do not freeze later path lookups.
Independent review and real Host acceptance of this continuation remain required.

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

The approved macOS [KEL-140 successor clause](../kel140-macos-public-filesystem.md#45-exact-macos-policy-source-amendment--reuse)
uses an explicit fixed project policy when present, with retained-object ownership
and one bounded buffer for staging and digest. Absent project policy retains the
historical fixed bytes above. Present unsafe input fails; missing staged policy
still fails startup. The existing host-selected stage and single read/hash/parse
are unchanged. This successor changes only the macOS development producer; it
does not relabel historical T1a/T1b acceptance or authenticate a release.

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
