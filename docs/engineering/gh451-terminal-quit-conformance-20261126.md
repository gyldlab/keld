# GH-451 terminal Quit conformance checkpoint

Record of pending conformance cells, not an implementation approval.

- Contract: [GH-451 and GH-523 draft #1432](https://github.com/gyldlab/keld/pull/1432), review correction `6a8607e52f9c0e8a8d4b757ffb047d97f1cf12f3`; historical review pin `c779aa2ac08270911b86cdbe3ebcb4e5f6dbcf2a`.
- Lifecycle amendment: [#1603](https://github.com/gyldlab/keld/pull/1603), review correction `c99566e34a50f14f8eed5e998336cacafa8e4dfa`; historical draft `960cc7e778e9c493976100b1797d68f2032d89e5`.
- Published product under review: [#1581](https://github.com/gyldlab/keld/pull/1581), `db74c84673d912c720320470e04f84468e760589`.
- Component evidence: [#1602](https://github.com/gyldlab/keld/pull/1602), initial exact `c421dc83acc44a1b2e0786543f5bc71ed99ffb46`, strict TS correction `5fd8a8300ff903c574f193255b05aa6e75187f62`.
- Reported results: [GH-451 evidence](https://github.com/gyldlab/keld/issues/451#issuecomment-7182735331).

Historical failures apply only to their recorded source and system. All full-feature cells below remain pending on the current product. A fixture pass does not make an installed-app cell pass.

## Component witness inventory

- R: `crates/keld-core/src/app_session/terminal_quit_candidate.rs` (draft model).
- O: `crates/keld-core/src/app_session/terminal_quit_core_owner_integration_candidate.rs` (registry component).
- F: `crates/keld-core/src/app_session/terminal_quit_core_real_fs_candidate.rs` (actual guarded worker/socket reply and lease with draft transaction).
- W: `crates/keld-core/src/app_session/window_router/tests/close_policy.rs` (actual router, injected native receipts).
- A: `packages/@keld/api/fixtures/gh451-retained-link-draft/candidate.test.ts`.
- B: `packages/@keld/electron/review-fixtures/gh451-terminal-quit-draft.test.ts`.
- D: `packages/@keld/electron/review-fixtures/gh451-d2-recovery-draft.test.ts`.
- E: `packages/@keld/electron/review-fixtures/gh451-receipt-ack-draft.test.ts` (receipt acknowledgement and terminal settlement regressions).
- C: `packages/@keld/electron/review-fixtures/gh451-terminal-quit-composed.test.ts`.
- S: `packages/@keld/electron/review-fixtures/gh451-actual-facade-source.test.ts` (actual facade/API/Source modules with scripted link boundary; no authenticated Host or native window).
- H: `crates/keld-core/src/app_session/tests/conditional_quit_dispatch.rs` (actual authenticated reader, explicitly armed legacy test carrier and out-of-band result, not a production conditional wire or terminal tail).

## Pending cells

| GH-451 criterion | Scoped witness | Current full-feature state and missing proof |
| --- | --- | --- |
| AC-1 Before veto | B, D | Unrun. Original selected app before-veto/throw; no close or terminal effect. |
| AC-2 Physical receipts | B, C, O, W, S | Pending-fail historical clean Destroyed-to-terminal gap. Current signed original unrun. Verify reverse sweep, actual release then terminal acceptance. |
| AC-3 Sweep through veto | B, D, S (single-window veto only) | Unrun. Two dirty windows, both veto positions; do not stop remaining requests or retain suppression. |
| AC-4 Async Save and retry | B | Unrun. Real Save result, disk content, actual release and fresh before-quit. |
| AC-5 Will veto and retry | R, B, D, C | Unrun. Real conditional Prepare/Cancel on same authenticated role, fresh explicit retry. |
| AC-6 Native intent | Earlier generic native failure evidence | Pending-fail historical generic AppleEvent cancel/retained family. Current original unrun; Dock, AppleEvent and menu routes retain veto. |
| AC-7 Reentry and handoff | D, C, S | Unrun full feature. Held other ACK, actual callback await, same public Promise, private receipt custody and exact late error. |
| AC-8 Opening/replacement | R, O, W, B, D, H | Pending-fail historical hidden Opening gap. H observes an actual transient Create with injected build failure. Current production unrun; hold Prepare/Status response across actual Create/preload/adoption and verify invalidated Commit. |
| AC-9 Terminal/Create cut | R, O, B, H | Unrun full feature. H observes test-only drain and later Create queued at WorkerLink, not a proved later socket write. Actual same-turn Create/Commit serialization and no orphan native window remain. |
| AC-10 One terminal winner | R, F | Unrun. Shared terminal reservation model passes. Real eligible Exit/Quit orders, invalid input, immutable receipt/tail and actual parent status remain. |
| AC-11 Retained link/status | A, D, C, E, H | Unrun full feature. H observes same-link Echo after a retained test-carrier result. Actual negotiated old/new peers, allocator reservation, Host record, ordered Status and receipt-consumption/no-late proof remain. |
| AC-12 FS drain | F, R, H | Unrun full feature. Actual guarded Reply and Err plus held lease pass at component seam. H pauses the actual reader inside the armed test carrier. Production conditional reader pause and no additional admissions remain. |
| AC-13 Native reply and exit | Earlier generic native failure evidence | Pending-fail historical generic native cancellation/retained family. Current original unrun; actual pre-reply cleanup, one correlated reply and independent natural exit. |

Retain the historical RED outcomes from [the W7 review](https://github.com/gyldlab/keld/blob/c779aa2ac08270911b86cdbe3ebcb4e5f6dbcf2a/docs/engineering/gh451-terminal-commit-review-w7.md) and the GH-1404 issue. Do not replace a historical source failure with a later model pass.

The scoped Core fixture deliberately failed without the Exit drain guard. Restoring the guard passed. Red log SHA-256:
`a2c5dff65909fc677768b4b7145a96b59a101b5584e5d7922ae181eab47078c0`.
Green log SHA-256:
`d4e3f90679985c5ed5efc2a4f007fdff707f9371758be60d210f4741213631f6`.
This is component evidence, not a new Exit API or production wire.

## Reproduction and native admission

Local component commands at the stated evidence pins:

```sh
cargo test --locked -p keld-core --lib terminal_quit_candidate
cargo test --locked -p keld-core --lib gh451_release_and_opening_failure_gate_last_window_fact
cargo test --locked -p keld-core --lib authenticated_reader_ -- --test-threads=1
cargo clippy --locked -p keld-core --lib --tests -- -D warnings
bun test packages/@keld/api/fixtures/gh451-retained-link-draft/candidate.test.ts packages/@keld/electron/review-fixtures/gh451-terminal-quit-draft.test.ts packages/@keld/electron/review-fixtures/gh451-d2-recovery-draft.test.ts packages/@keld/electron/review-fixtures/gh451-terminal-quit-composed.test.ts packages/@keld/electron/src/gh451-original-quit-lifecycle.adversarial.test.ts
bun test packages/@keld/electron/review-fixtures/gh451-actual-facade-source.test.ts
bun test packages/@keld/electron/review-fixtures/gh451-receipt-ack-draft.test.ts
```

The wider 86 registry and 45 router tests passed before the final helper refinement. The final initial candidate passed 26 terminal cases, the release/Opening regression and strict Core Clippy. The 104 Bun tests passed after the strict fixture type corrections. The additional S fixture passed independently with actual source imports and a scripted link. At `c5e7d06a05f0137ef60b45932fe82fb760c5bf28`, 105 Bun tests and two authenticated reader tests passed locally. The subsequent local review correction passed 110 focused Bun tests (1,300 assertions) before formatting; the final revision requires its own checks. These counts are not full-product acceptance.

The composed fixture covers local allocator exhaustion separately after a valid Prepare for Cancel and Commit. An ordinary request occupies the sole remaining slot. No new conditional effect or Status identity is allocated, and the preparation remains in custody after the ordinary slot is released. The facade reports an unresolved attempt and permits no replay or fresh attempt. On macOS, both cases passed in the focused 117-case Bun run (1,380 assertions). A temporary copy that returned retryable Retained instead of unresolved failed both cases at the exact disposition assertion. Both package typechecks passed. The API/IPC and lifecycle owners still need to decide how to recover a surviving preparation after a proved local pre-send refusal. This fixture does not select an automatic retry or a production recovery rule.

Use [GH-443 oracle](https://github.com/gyldlab/keld/issues/443#issuecomment-6381736395) and [GH-451 observed sequence](https://github.com/gyldlab/keld/issues/451#issuecomment-6381751800) for the real Electron comparison. The original desktop pin is `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e` and webapp pin is `24b76c2cbd55d88e354042e8d329a2e4708972bc`. That Electron oracle is not Keld acceptance.

The installed original must prove the actual saved-A/unsaved-B Cancel, Save-B and reopen, unsaved-C Discard and reopen-B, ordinary and native Quit cases. Record independent physical window, file-content, role generation, terminal and process-exit observations. Use explicit barriers. Do not kill a process to manufacture clean exit.

The GH-1404 generic runner `crates/keld-host/tests/fixtures/macos_local_package_acceptance.py` is not an original-editor runner. It requires independently qualified CLI, Host, Bun, their hashes and versions, fresh evidence/install roots and an eligible GUI lease. Do not claim an eligible original runner or executable from placeholder/old preflight artifacts.

Latest cited [GH-1404 status](https://github.com/gyldlab/keld/issues/1404#issuecomment-7181239946): B2 approvals 0/17, B3 signed joins 0/4. Actual installed original acceptance remains unavailable. Re-read live pins, owner decisions, artifacts and resource/GUI eligibility before a new native attempt.
