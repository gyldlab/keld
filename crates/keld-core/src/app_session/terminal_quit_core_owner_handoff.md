# GH451 Core owner candidate — draft only

Base: published #1581 db74c84673d912c720320470e04f84468e760589 plus the
unregistered transaction prototype 2c02443271a3b36d8cfbd4edaacfb20180f2617e.
Design input: W7 appendix in #1432 eb7a18b42ef7e8213a280ec9c76015bf78a0964f
(unapproved); W5 facade candidate b2426a9d7 and W7 RED 144cf1bb4 /
4635f001b are independent evidence. No new acceptance or protocol allocation.

This lane adds an unregistered, test-only fixture. It composes the original
prototype with real WindowRegistry and SessionShutdownState APIs under a
test owner cut. The adjacent optional registration patch is for the owner
to apply to an isolated test checkout. It is not applied here. It registers
tests only, with no shipping behavior. No other owner files changed.

## Exact owner integration after design approval

| Atom | Existing source and proposed seam | Observable and open proof |
| --- | --- | --- |
| Qualification | app_session.rs:9637 with_primary_caller_locked authenticates identity, attempt and reader-stop Arc. window_router.rs:233 original_menu_quit_selected and app_session.rs:6926 exact Ready publication supply selection and generation readiness. | Bind opaque token to the exact current role/link and selection, not caller-provided generation. Test wrong identity, stale Arc and a successor Ready. Fixture only models current owner; live authentication unrun. |
| Registry | window_registry.rs:516 create owns successful fresh/adopted admission; :600 preload enters same create. window_router.rs:1226 and :1861 are both existing callers under transition/current/windows. | Store one transaction alongside existing WindowSession under its lock. Refuse committed/unknown before mutation; invalidate at the successful common registry admission cut, before send/reply. Failed validation has no effect. Do not miss an internal preload admission if later code errors. Normal route tested; exact preload/adoption integration still open. |
| Destruction | window_registry.rs:1626 native_released accepts only Closing(Some(native)); :710 native_failed retires Opening or Closing(None); :1702 has_live_windows excludes only Destroyed. managed_windows.rs:908 waits bridge/resource drain, removes/drops View, then emits WindowReleased. | Opening/Open/ClosePending/Closing all fence preparation. Wrong/duplicate release cannot grant authority. Treat build failure as physically empty only when the native owner has proved no partial custody. Model and inspected release source pass; real OS witnesses and build-failure cleanup proof remain open. |
| Reversible drain | app_session.rs:7844 lifecycle_quit pauses current reader; :842 FsSession::drain waits in_flight==0; :7681 submit_fs_terminal_write requires running and transition/current. | Keep existing reader dispatch paused during commit and drain without any of those locks. No early reserve, reader-stop, terminal link close or FS quiesce. Confirm actual terminal Reply/Err and lease release, and distinguish absent configured FS from lost owner. Simulated drain only; real IPC/FS receipt and no-new-admission proof open. |
| Final cut | app_session.rs:2447 reserve_guarded sets reader-stop and makes is_running false before accepted cause; :2460 accept_reserved_guarded; :2502 begin_tail. | Recheck same authenticated current role/link, pending token, selected Ready, census, invalidation history and true FS completion under transition/current/windows. Reserve once. After reserve release windows before invoking existing native-admission cancellation/cleanup, which may lock windows again. The shared post-reserve tail must be extracted by owner; never invoke lifecycle_quit again. Existing forceful path remains unchanged. |
| Retirement and errors | app_session.rs:7296 retire_current_generation_locked calls :7348 retire_window_owner, under transition. window_router.rs:965 owns window retirement. | Revoke exact token in existing retirement owner. Late retirement cannot cancel successor. Failed lock, unknown drain, failed write, competing terminal owner, or lost acknowledgement must not become a retryable Retained reply. Actual typed wire/retirement and reconciliation contract unapproved. |

The exact test fixture derives physical empty from registry phases, relying
on truthful WindowReleased/WindowBuildFailed. It does not certify actual
native destruction or FS terminal I/O. The fixture's current and windows
mutexes exercise ordering, but do not replace the production owner or prove
concurrent IPC dispatch. Keep forceful legacy Quit, existing registry policy,
wire discriminants, native AppKit success and all permission checks unchanged.

## Lightweight evidence

From the worktree root:
`python3 -B crates/keld-core/src/app_session/terminal_quit_core_owner_probe.py`

macOS rustc 1.99.1. The standalone probe projects the exact production
registry and shutdown method bodies, original prototype and this fixture;
unrelated IPC/native values and receipt delivery are minimal stubs.
It compiles and executes only small temporary Rust test binaries.
Observed: **17 passed** (12 existing prototype + 5 new), 0 failed.
Negative controls executed separately and expected to fail:
- Omit successful Create invalidation: commit returned Committed instead of
  Retained after real registry Opening -> native_failed; exit 101.
- Treat Closing as empty: prepare returned Prepared instead of Retained
  before native release; exit 101.

The optional registration patch is checked with git apply --check. No full
cargo build, workspace clippy, full test, native app, actual FS or IPC run.
For the next isolated owner-run, apply that patch then run the macOS Core
test filter terminal_quit_candidate (after resources and owner eligibility
permit); extend the existing authenticated fixtures to verify R1/R9/R10,
and Wv actual release/build-failure proof to verify R4. Do not publish,
merge, enable production or mark GH451 complete from this evidence.
