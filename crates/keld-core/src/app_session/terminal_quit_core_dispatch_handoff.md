# GH451 authenticated reader dispatch experiment — test-only

Exact base: local c421dc83acc44a1b2e0786543f5bc71ed99ffb46 (fast-forwarded from
the owned W1 branch); published #1581 db74c84673d912c720320470e04f84468e760589;
#1432 c779aa2ac (GH451 selected semantics, joint amendments pending).
No production API, wire, policy, signing, native strategy or merge authority
is selected or exercised by this patch.

## Actual source and test boundary

- `app_session.rs` adds an explicitly armed, one-use test hook only under
  macOS test configuration (excluding native renderer main). The actual
  `read_primary_frames` decodes the existing LifecycleRequest::Quit behind
  `with_primary_caller_writer`, then checks the fixture slot against the exact
  principal, attempt, reader Arc and correlation. Unarmed and nonmatching
  requests retain the legacy path. The fixture carrier never returns a
  synthetic Quit response or allocates conditional wire semantics.
- `terminal_quit_core_dispatch_candidate.rs` performs Prepare using the actual
  Core guard, selected/Ready/subscribed WindowSession, registry observation
  and FS state. The real reader enters the candidate commit handler, releases
  all owner locks before calling real FS drain, rechecks and reserves through
  the existing SessionShutdownState under the same owner cut. It reports a
  test-only disposition on a private channel; Retained resumes the reader.
  Committed stops the reader. The real terminal tail is not entered.
- `tests/conditional_quit_dispatch.rs` reuses the actual accepted Runtime
  role and Bun WorkerLink fixture, real Core reader and guarded FS worker.
  The test-only WindowSession and registry accessors live under their existing
  test modules. A fixture native desktop/build-failure receipt is injected;
  there is no physical native window or installed-app qualification.
  The first test observes actual admitted Create then build failure, a
  Retained cut despite empty census, and healthy Echo on that same link.
  It checks stale reader refusal and rejects rearming the unresolved carrier.
  The second holds actual FS work and its terminal write lease, submits a
  later Create while the reader drains, then observes a single Committed cut
  and no later native Create command.
- The fixture carrier remains unresolved on the wire by design; no success
  or conditional Retained frame is fabricated. The independent test observes
  the cut out of band and disposes only its own fixture through the existing
  router/role owner. This does not prove Status, conditional reply delivery,
  exact correlation reclamation, physical teardown or production tail.

## Smallest shipping integration after joint owner decisions

| Current owner | Proposed surgical change and invariant |
| --- | --- |
| Primary reader, app_session.rs read_primary_frames | Use approved existing lifecycle schema and negotiated capability; route conditional operations after the current authenticated decode. Leave the current forceful Quit arm unchanged. Keep one unresolved exact operation and ordered Status/reclamation with the actual IPC allocator, not this test carrier. |
| PrimaryRouterHandle / WindowSession | Keep reversible attempt state in one existing lifecycle owner. Under transition/current/windows bind the exact current link, selected Ready+subscribed role and authoritative registry facts. Prepare alone never reserves shutdown. Match Cancel and Commit to the exact Host-minted token. |
| WindowRegistry::create and create_with_original_preload; window_router.rs window_request_locked | Invalidate at the successful shared fresh/adopted admission cut, including preload; no rejected admission invalidation. The fixture samples the existing nonrecycled incarnation after an empty cut; production must integrate at the actual successful Create cut, with explicit adoption/retirement coverage. Opening and Closing remain live until existing authoritative receipt. |
| PrimaryRouterHandle::retire_current_generation_locked | Revoke exactly the retired reversible attempt; preserve known terminal winner and unknown disposition custody. Do not create another generation owner or reopen a terminal decision. |
| FsDispatchSession::drain, submit_fs_terminal_write | On the actual reader pause further dispatch while Running. Release transition/current/windows locks for real drain; verify all owed replies and leases and recheck current exact owner. A lost configured FS owner or failed drain is non-success/unknown, never an empty drain. |
| SessionShutdownState::reserve_guarded; lifecycle_quit | Final conditional cut atomically rechecks token, Create invalidation, registry and FS, then reserves once. Extract/share the existing post-reservation tail only after joint owner approval. Release windows before cleanup paths that re-enter that lock. Use the real pending reply/retirement owner; do not call the complete legacy lifecycle_quit after an earlier reserve. GH523 and KEL96 share the terminal winner and tail, not a new authority. |

## Validation and next exact commands

Executed in this isolated worktree: existing 25/25 projection tests with
warning-denied rustc, both established negative controls RED, actual dispatch
adapter typecheck via exact source projection (real caller guard, selection
and registry observations; unexercised owner type shapes substituted),
TypeScript fixture transpilation, formatting and diff checks.
No heavy Cargo build was started. The two new authenticated component cases
are not full-crate compiled or executed here. The prime owns the available
Cargo target and must compile/run these exact tests before claiming evidence:

`cargo test --locked -p keld-core --lib authenticated_reader_retains_after_transient_create_and_services_same_link`

`cargo test --locked -p keld-core --lib authenticated_reader_holds_terminal_cut_through_real_fs_reply_and_lease`

Use the existing test harness/process cleanup bounds; a hung drain is a
failure, not evidence to weaken the production wait. Also run the retained
legacy Quit and existing role authentication regressions in the same target.
Joint GH451/GH531/GH523/KEL96/FS/IPC amendments and exact runtime/native gates
remain prerequisites for shipping integration. No publication from this tree.
