# Spec: no-flag host-as-app boot (Unique #1 lifecycle half)

- Status: approved
- Tracker: KEL-96
- Owner: GYLDLAB
- Updated: 2026-11-15
- Pin: 249e7a64d9b7732cf2f1ab629ff1b2e02810d4b5
Decision state: human-approved selections recorded; final-head review is PR merge evidence
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

## Parts

| Part | Owns | Load when |
|---|---|---|
| [boot.md](boot.md) | KEL96 descriptor and startup-session contract | Inspect approved acceptance and owning design |
| [lifecycle.md](lifecycle.md) | KEL96 quit, recovery, cross-OS lifecycle and verification | Implement, test or verify scoped delivery |

## Current blockers

- B-1

## 8. Review gates triggered

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
