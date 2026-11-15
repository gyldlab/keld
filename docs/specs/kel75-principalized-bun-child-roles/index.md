# Spec: principalized Bun child roles and virtual-port routing

- Status: approved
- Tracker: KEL-75
- Owner: GYLDLAB
- Updated: 2026-11-15
- Pin: 249e7a64d9b7732cf2f1ab629ff1b2e02810d4b5

## 1. Goal & non-goals

Keld extends KEL-70's live generic single-child supervisor into a host-owned family of
independently supervised Bun roles while retaining one authority root, default-deny
dispatch, and an app-agnostic runtime core. Each role instance receives a new
host-minted principal generation and private authenticated link. The host, rather than a
role, creates roles, virtual ports, windows and privileged resources. This specification
makes the later Electron `utilityProcess` and `MessageChannelMain` facades consumers of
that generic model.

Non-goals:

- Replacing KEL-70's generic `keld_runtime::Supervisor`, its restart policy, output
  capture, or crash-loop breaker. T1 extends it through a role-bootstrap boundary.
- Implementing `@keld/api`, `@keld/electron`, shared memory, or an OS sandbox in T1.
- Giving a webview a Bun socket, pipe, file descriptor, mapping handle, reconnect
  capability, or direct OS channel.
- Making a PID, listener path, token, environment variable or a wire field an identity.
- Adding VS Code role names, package-name branches, or arbitrary process spawning to
  `keld-runtime`.
- Claiming that any strict sandbox profile is implemented; KEL-78 owns its real-OS
  proof.

## Parts

| Part | Owns | Load when |
|---|---|---|
| [roles.md](roles.md) | KEL75 role identity, grants and lifecycle | Inspect approved acceptance and owning design |
| [routing.md](routing.md) | KEL75 virtual port routing, boundaries and delivery | Implement, test or verify scoped delivery |

## Current blockers

- B-1

## 8. Review gates triggered

- unsafe: none in this specification; later platform sandbox and shared-memory changes
  require review.
- public API: yes — the original role configuration, generated policy and virtual-port
  contract, T4a's lifecycle/renderer observable, and T8's cross-platform bootstrap
  stream plus bound-generation handoff require human review.
- permission model: yes for the original role-specific grant subset/elevation contract.
  The T4a/T8 amendments add no grant representation or evaluator decision and therefore
  do not approve or implement privileged Windows dispatch or another permission model.
- dependency addition: none in this specification, T4a or T8.
- wire protocol: no new frame in this specification, T4a or T8; later virtual-port channel,
  readiness signal or handshake metadata changes require their own versioned wire
  review.
