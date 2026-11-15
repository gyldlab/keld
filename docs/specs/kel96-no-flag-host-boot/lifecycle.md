# KEL96 quit, recovery, cross-OS lifecycle and verification

Part of: [index](index.md)
Owns: sections 4–7, 9–10

## 4. Design: quit, recovery and OS boundaries
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
no-flag T1b has no `KELD_DEV_LEASE` and does not monitor terminal stdin. On Windows,
once KEL-254 T3 lands, a launch without a valid dev lease is admitted only as an
authenticated installed package or refused (KEL-254 AC2); a lease-less staged layout is
no longer a dev-stage boot there. macOS and Linux standalone boot is unchanged until
their own authenticated installed-root successor and a separate reviewed amendment
converge them on the same rule; this is a temporary applicability difference, not a
trust-model exception.

CLI exit/crash closes the only writer and yields EOF. `CliLeaseLost` is a
distinct shutdown cause: it quiesces new work and joins the common
revoke/link-close/terminate/reap/event-loop-exit tail, but sends no impossible
Quit reply because no lifecycle Call exists. A forged environment value can at
most make the caller's own host monitor stdin and shut down, or, on Windows, route
the launch to the `DevStage` validator, whose owner-private stage checks still decide
admission (KEL-254 AC2); it carries no app root, path, digest, principal, permission,
or application-resource ownership.
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

## 6. Tasks

### T-1: Completed initial human spec approval

- **Blocked by:** none

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
      `3d9e5c78db273f550c47f1a6b4655712de2723a0` stages a dedicated minimal
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

## 7. Test plan & evidence

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

## 9. Perf impact

No performance claim is made by this documentation PR. The boot descriptor is
bounded at 64 KiB and parsed once on the cold startup path. A later implementation
must attribute any cold-to-window or RSS claim using the architecture 01 §5
fixtures; moving work into Rust is not itself a measured improvement.

## 10. Open questions, blockers & unknowns

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
