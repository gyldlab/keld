# Implementation boundaries and task
Part of: [index](index.md)
Owns: Allowed implementation paths and the ordered implementation task.

## 5. Boundaries

**Existing implementation owners to reuse; change only for a proven gap:**

- `crates/keld-core/src/app_session/boot_contract.rs`: the current parser owner.
- `crates/keld-core/src/app_session.rs`: `run_app`, `run_guarded`, the coordinator and
  retained main-thread custody; use the extracted `app_session/tests/` owners for
  existing tests, including `boot_schema_and_preflight.rs`.
- `crates/keld-core/src/hello.rs`: the config resolver.
- `crates/keld-host/src/main.rs`: the terminal function.
- `crates/keld-cli/src/{boot,doctor,dev}.rs`.
- `crates/keld-wv/src/engine.rs`, `crates/keld-wv/src/lib.rs` (the deadline rename) and `crates/keld-wv/src/wkwebview/mod.rs`.
- `crates/keld-host/tests/no_flag/{macos,linux,windows}/` and the macOS native
  observation support/Swift fixture for the dedicated absence oracle. Preserve the
  existing positive observer's contract.
- `crates/keld-compat/fixtures/app-corpus/` (the one `unknown` cell) and `packages/@keld/electron/src/app-surface.test.ts` (its mapped today-test).
- `docs/engineering/keld-error-codes.md` (034, 035, 044).
- The spec and architecture files in §4.10.

**Must not touch:**

- `keld-ipc` wire and channel tables.
- `packages/@keld/api` and `packages/@keld/kipc` behaviour.
- The webview2 and webkitgtk backends.
- The frozen `electron-lifecycle-v0` corpus and its receipts.
- The existing `electron-app-v1` cells (#656).
- GH-531 registry code (F02-T2).
- Workspace `Cargo.toml` (no new dependency).

## 6. Tasks

### T-1: Schema-2 facade boot vertical slice

- **Blocked by:** none
- **External gate:** #445 must land before T-1 starts.
- **Review prerequisite:** Before T-1 starts, independently review the exact current
  split head and bind approval to its specification blobs as required by §10. The
  historical technical-head approval does not satisfy this prerequisite.

- **Scope:** One vertical slice covers AC-1–AC-15, the host/link portion of AC-17 and AC-20. AC-16, AC-18 and AC-19 are F02-T2's tests of this spec's
  cross-owner rules. At current Pin `ec29b7ce`, the parser, compiler, macOS
  `HostInitialized` producer, coordinator deadline, terminal callback and Linux /
  Windows product refusal already exist. T-1 is a reconciliation and qualification
  task, not a greenfield implementation. It MUST reuse those owners. It MUST NOT add
  a parallel parser, compiler, event loop, terminal path or Ready authority.
  T-1 contains:
  - reconcile the existing schema-2 parser, resolver and compiler against AC-1–AC-3;
    fix only a proven gap;
  - reconcile the existing macOS `HostInitialized` loop, coordinator, bounded `Init`
    wait, install-failure path and `KELD-CORE-044` against AC-4–AC-7 and AC-14–AC-15;
    add no second event loop or Ready authority;
  - verify the existing binary-owned terminal function and retained main-thread custody
    against AC-15 and AC-20;
  - verify Linux and Windows product refusal with `KELD-CORE-034` against AC-11;
    keep the Windows debug-only test admission out of product builds;
  - the `app.ready.recovered-generation` cell;
  - any missing tests and negative controls, including current-Pin failing-first proof
    for each remaining gap;
  - the §4.10 text, the error-code entries and the recomputed KEL-96 digest.

  These parser, producer and consumer paths already exist at Pin
  `ec29b7ce2b155a8c98f96036cbb9c54488b5c302`. Their source presence is not acceptance.
  T-1 must qualify the existing vertical slice and fix only proven gaps. A parser or
  staging test alone does not complete T-1.

**Minimal sequence and #449 gates.** First bind this decision to independently reviewed
specification bytes. Preserve and finish the existing #445/#656 conformance lane;
#659 is already merged, and accepted structural AC-4 binding is separate from #663's
harness-record production. At the current Pin, the parser, compiler,
`HostInitialized` producer, coordinator and terminal path already exist. Reconcile
their exact source and tests first. For any remaining gap, add a compiling RED against
this Pin before a focused production edit. Then run the real-Mac and unchanged
regression checks. #449 consumes the accepted T-1 slice and owns AC-16/18/19 and
first-window creation.

| Gate | Exact requirement | What it authorizes |
|---|---|---|
| #449 engineering start | Approved exact #657/GH-531 amendments, an uncontested #449 claim, and named dependency commits for a draft stack | Work on the registry/facade slice with missing dependencies stated; it does not close an issue dependency |
| #446 Ready blocker resolved | T-1 is accepted and integrated: schema 2 is real, initial Ready follows authenticated HELLO plus genuine Init with zero application windows, failures stay closed, schema-1/recovery/custody regressions pass, and receipts bind the accepted head | Remove the #446 dependency on #449; a spec decision or a stub alone is insufficient |
| Remaining #449 dependency reconciliation | Check #448, #531 and #597 against their accepted commits; #528 must have its required macOS transport acceptance, with merged #666 considered and tracker evidence reconciled | Determine that the macOS constructor transport is available; an unrelated Windows lane is not silently made a new macOS requirement |
| #449 completion | Usable public readiness after accepted subscription/replay; first constructor creates exactly one requested native window; host/public identity controls, eligible-survivor adoption and legitimate multi-window controls pass at the final head | Close #449 only after its own acceptance/review/integration; host Ready alone is not BrowserWindow completion |

These are gates, not a claim that any unrun acceptance is complete. No PR is merged
under the current user instruction.
