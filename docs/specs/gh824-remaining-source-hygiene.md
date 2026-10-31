# Spec: Remaining source-hygiene ownership slices
Status: implementing
Linear: GH-824 (#517) | Owner: repository owner | Updated: 2026-10-25

## 1. Goal & non-goals

Make two independently owned responsibilities discoverable without moving runtime
lifecycle or security authority: bounded transcript storage in `keld-runtime`, and
local research-checkout operations currently embedded in `justfile`. This is not a
file-length policy. Existing implementation-state reconciliation and PR #1178's
formatting/navigation fixes remain separate evidence, not full-program completion.

Non-goals: changing output limits, supervisor behavior, OS handles, permissions,
wire formats, dependencies, public command names, reference-lock ownership,
private research contents, CI admission, or numerical source-size enforcement.
No blanket `tools/` move, stamped transport split, or other-owner branch edits.

## 2. Spec refs

`docs/architecture/06-runtime-and-tooling.md` section 1 owns the supervisor contract.
Closed KEL-134 and KEL-225 own bounded capture and its literal host-retention
ceiling. `crates/keld-runtime/AGENTS.md` keeps process-family/unsafe ownership
unchanged. `.agents/test-layout.md` owns test placement and discovery safeguards.
`docs/agents/workflow.md` and `.agents/coordination.md` own claims, managed
worktrees, reference-resource coordination, review and the GH-824 merge hold.
No architecture contract is amended by these source-only slices.

## 3. Acceptance criteria (binary, each becomes a test)

1. Existing external callers still compile using `keld_runtime::CapturedOutput`
   and all five existing `keld_runtime::CAPTURE_*` constants. No new public module
   or access to private capture mutation is exposed.
2. The captured-output algorithm and constants reconstruct from the original
   source after accounting only for the private-module move, parent visibility
   needed by existing callers, explicit test imports and qualified rustdoc links.
   No byte bound, UTF-8 boundary, line-boundary, counter, marker or locking logic changes.
3. Before/after compiled runtime discovery has the same total and ignored states.
   Exactly the ten pure capture tests listed below may receive the documented
   `captured_output::tests::` prefix; every other selector is unchanged. Every
   referenced selector and test-reader fingerprint is reconciled atomically.
4. The literal 327680-byte ceiling and existing capture/UTF-8/truncation cases
   pass before and after. Temporarily removing a retention guard makes a named
   existing regression fail; restoration returns green. Full affected native
   supervisor tests and warning-denied lint remain required, not replaced by
   the source-equivalence check.
5. `just research-sync` and `just research-push` retain their arguments, messages,
   exit statuses and reference-run locking. Local fixture checkouts prove dirty
   commit/push, clean push, wrong/missing origin, non-repository destination,
   parent-checkout refusal and failed Git operations without contacting a real
   remote or reading private research. Whitespace-bearing paths/messages survive.
6. The research origin-admission predicate exists once. Both operations consume
   exactly the currently accepted three URL spellings and `.git` normalization;
   no new allowed host/repository, fallback authority or deletion operation appears.
7. Complete diff review, final applicable local/hosted checks and exact-head
   independent review pass. No unrelated source/test inventory or policy weakens.

## 4. Design

### Facts and reuse

No boundary change. `CapturedOutput` currently owns only pure retained head/tail
storage, raw-byte counters, elision and UTF-8/line-boundary presentation. It is
separate from `CaptureState`, stream-marker observation, capture threads and
`Supervisor`. Moving this already cohesive implementation to a private module
reduces the source needed to understand retention without introducing a helper
facade, another copy or a new state machine.

`workspace.py reference-run` already owns the lock and shared primary reference
root. Keep that implementation and every public Just entrypoint. The private
sync/push recipes currently duplicate the accepted-origin normalization and
contain their complete shell implementations. One domain-specific shell owner
can retain those operations without turning Just into another program host.

No performance improvement is claimed. The baseline is source/selector parity
plus task navigation: the retention task should require its implementation and
its adjacent tests, not the full runtime supervisor file; the research task
should require its helper and stable recipes, not unrelated verification recipes.
A cosmetic split without that task-locality improvement is rejected.

### Capture module

Create private `src/captured_output.rs`; re-export the existing public struct and
five constants from `lib.rs`. Existing fields/public methods retain signatures.
Only the currently parent-called private `push_chunk` gets parent visibility;
this does not create a downstream public mutation API. Keep `CaptureState`, marker
search, reader threads, session counters, process helpers and every sanctioned
`unsafe` operation in their existing files. No compatibility adapter is needed:
the root re-export is the permanent unchanged public facade.

Move the ten pure retained-output tests beside that owner under
`src/captured_output/tests.rs`, using explicit named imports. Keep process-spawn,
chatty-child, drain, restart and lifecycle tests with their existing owners.
A before/after discovery map permits only these ten names:

- `capture_max_retained_bytes_is_the_documented_host_ceiling`
- `bounded_capture_keeps_a_fixed_tail_and_counts_every_byte`
- `capture_late_newline_cannot_defeat_the_retention_ceiling`
- `elision_never_fabricates_a_line`
- `newline_free_output_is_separated_rather_than_spliced`
- `carriage_return_only_output_is_separated_rather_than_spliced`
- `one_enormous_line_still_respects_the_ceiling`
- `totals_count_raw_bytes_not_lossy_decoded_length`
- `bounded_capture_pins_the_head_against_a_later_flood`
- `bounded_capture_cuts_on_a_utf8_character_boundary`

### Research helper

Create `tools/research_checkout.sh` with only `sync` and `push` subcommands.
Its inputs are operation, the root returned by the existing `reference-root`
owner, and for push the literal message. The Just wrappers retain positional
quoting and enter `reference-run` before invoking it. Do not duplicate root
resolution, locking, Python-platform selection, workspace records or cleanup.

Move the existing Git operations and diagnostics; share only origin normalization
and admission. Preserve the current distinction between hook-safe sync failures
that warn/return zero and push integrity failures that return nonzero. Do not
fetch/push real private resources during tests. Test only owned temporary local
repositories and command-failure controls. Cache execution/provenance stays in
its current owner; this spec does not authorize moving it merely for uniformity.

### Contracts and platforms

New types/channels: none. Capabilities/manifests: none. Wire changes: none.
Runtime seam: none; process, principal, crash, handle and callback ownership is
unchanged. Windows, Linux and macOS retain the same capture implementation.
Shell verification includes native Windows Git Bash and stock macOS Bash 3.2;
Linux behavior is qualified by its own actual run. No OS pass is inferred from
another OS's compilation. Generated fingerprint updates use the existing generator
on the integrated tree, never copied hash strings from another branch.

## 5. Boundaries

Implement only the selected slice after approval and a fresh winning claim:

- Capture: `crates/keld-runtime/src/lib.rs`, new private
  `captured_output.rs` and `captured_output/tests.rs`, and exact selector consumers
  proven by the before/after reference scan.
- Research: the two private research recipes in `justfile`, new
  `tools/research_checkout.sh`, its narrowly scoped local-fixture tests and the
  existing navigation map. Keep public wrappers/reference-run contracts.
- Reader/census digests required by the checks may change through canonical rebind.

Must not touch root `Cargo.toml`, workflows, root/nested instructions, transport
source, native guardian/job/profile implementations, other owners' worktrees,
private reference contents, or historical audit bytes. Refresh PR #1075 and all
claims before touching ordinary overlapping Just regions; do not take a shared
workflow writer's authority. Root file ownership conflict blocks only that slice.

## 6. Tasks (ordered vertical slices)

- [x] T1. Record exact current source, claims, consumers, compiled test names and
      unchanged public aliases before the capture move; implement and validate
      the private module and its adjacent pure tests as one reviewable unit.
- [x] T2. After reference-owner/Just overlap reconciliation, extract the existing
      research operations with one origin predicate and local-only failure proofs.
- [ ] T3. Reconcile source/reader hashes, navigation, exact-diff independent review
      and relevant real-platform results. Consolidate reviewed changes through
      `gh stack` without a PR/push per file; do not create a speculative mega-refactor.

The repository owner may approve either slice independently. Neither slice makes
GH-824 complete while other recorded review/security/approval criteria remain open.

## 7. Test plan

AC1-2: unchanged external import probe, source reconstruction and warning-denied
rustdoc/Clippy. AC3: actual compiled libtest/nextest lists including ignored flags,
with a bijective ten-selector rename map and a zero-selected old-prefix control.
AC4: existing pure unit tests, deliberate retention-guard mutation and affected
native package/workspace tests. AC5-6: real local Git fixtures with temporary
bare remotes and exact status/file observations, not network mocks. AC7: final
`just ci`, applicable hosted lanes and independent exact-head review.

No sleep synchronization, retries, increased deadlines, deleted tests or suppressed
exit status. A timeout is only an outer kill switch. Unavailable native evidence
stays explicit and blocks any criterion that requires it.

## 8. Review gates triggered

Public API: review root re-export/type documentation equivalence, although no
signature or supported import changes. Unsafe, permission, dependency and wire
protocol: none; any proposed change to them stops this source-only slice.

## 9. Perf impact

None claimed. Algorithms, allocations, retention budgets and execution order stay
unchanged. Do not justify a new assembler, generic task framework or dependency
with an unmeasured speed/memory claim.

## 10. Open questions

None for the approved T1/T2 implementation scope. On 2026-10-25 the repository
owner explicitly authorized implementation of both slices on Windows in the current
conversation; the authorized exact scope and receipt are posted to GH #824. This does
not approve a PR merge. The separately owned unsafe-path admission/instruction-format
decision and numeric source-size caps remain outside this spec, with their own
approval criteria. The standing GH-824 no-merge hold remains unchanged.
