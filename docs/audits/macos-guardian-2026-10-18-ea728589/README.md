# macOS Guardian architecture coverage — pinned partial ledger

**Status: partial review evidence for #824; not a completed architecture audit, security certification, or merge authorization.** All implementation pull requests cited here remain draft and unmerged. This is a repository-tree comparison, not evidence that the draft stack is present on `main`.

## Exact revisions and scope

| Role | Commit |
| --- | --- |
| Current upstream `main` snapshot (fetched 2026-10-18) | `baf558f24b094c0dd5c0a77b5939f6e7013ba4aa` |
| Reviewed macOS stacked tip, draft [#948](https://github.com/gyldlab/keld/pull/948) | `ea728589d0143df134c0f21fe5cf335240018d6a` |
| Stack merge base with current `main` | `709d302dc8df9b3623fd29a0fca0a45d9a6087db` |

The two branches are not the same tree. The inventory deliberately compares
their Git blobs without implying a merge or completed merged-tree acceptance.
It continues the earlier [1,014-path source census](../source-census-2026-10-15/README.md);
it does **not** replace that manifest or recount unrelated source as reviewed.

Included: the production module `crates/keld-runtime/src/macos_guardian.rs`, every
tracked file under `crates/keld-runtime/src/macos_guardian/`, and the directly
owned `crates/keld-runtime/tests/macos_host_death_guardian.rs` integration
entrypoint with its subdirectory. The `ledger.csv` lists each tip path, its
exact Git blob, the corresponding `main` blob when present, source/test class,
and review disposition. `summary.json` pins the scope and the counts.

| Exact-tip inventory | Files |
| --- | ---: |
| Production (B) | 18 |
| Tests (C) | 59 |
| **Total in this bounded macOS scope** | **77** |
| Verified *for a prior narrow security PR*, not full architecture | 6 |
| Reopened: a later review found a defect in the pinned blob | 2 |
| **Pending complete file-level architecture review** | **69** |
| Added relative to pinned `main` | 75 |
| Modified relative to pinned `main` | 2 |

All 77 rows are pinned to Git tree objects; `added` means absent from the
specified `main` tree only. The 69 pending rows must **not** be inferred as
cohesive, safe, or no-change. Even the six scoped-verified rows still require
any broader #824 cohesion, cross-component and integration disposition not
demonstrated by their narrow security fixes.

## Existing reviewed security paths

**Correction (2026-10-22).** Two rows that this ledger first recorded as
`verified-scoped` are now `reopened`, because a later review found a defect in
the exact pinned blob. Neither blob is reviewed evidence:

- `guarded_primary.rs` blob `edf7b83a` (#947 at `b88b4ef6`): the
  [#947 review](https://github.com/gyldlab/keld/pull/947#pullrequestreview-5579005672)
  found a HIGH defect at exactly that commit (a retained witness allowed a raw
  `killpg` on a recycled PGID during retry). The fix landed in `1b5de775` and
  `03815dee`, with different blobs.
- `process_group.rs` blob `ab6e23fb` (#945 at `c20f0151`): the
  [#945 review](https://github.com/gyldlab/keld/pull/945) of 2026-10-21 found a
  medium duplicated group-absence predicate. The single absence owner landed in
  `20997b55`..`90c46fa0`, with different blobs.

The descriptions below record what each PR claimed at the pinned commits.


- [#945](https://github.com/gyldlab/keld/pull/945), commit `c20f015143233d47efa1e67341e7899d7b647d26`:
  `generation_host.rs`, `process_group.rs` and
  `tests/generation_host/clear_authority.rs`. KGC1 `CONTROL_CLEAR` with
  successor authority requires original group absence via two native `ESRCH`
  observations, not just successful `SIGKILL`; the full-budget unreaped-zombie
  regression rejected a delayed incorrect `CLEARED` acknowledgment.
- [#947](https://github.com/gyldlab/keld/pull/947), commit
  `b88b4ef6d38909107ff89faebfe993c113893f8a`:
  `guarded_primary.rs` and `tests/fatal_cleanup_custody.rs`.
  Fatal native cleanup failure retains the original birth-attested witness.
- [#948](https://github.com/gyldlab/keld/pull/948), commit
  `ea728589d0143df134c0f21fe5cf335240018d6a`:
  `registered_guardian.rs`, `tests/host_guardian_failed_custody.rs`
  and `tests.rs`. Legacy KGR1 terminal cleanup retains the original
  GroupIdentity on native refusal, while a consumed writer prevents terminal
  session reuse. Five native cases include genuine externally reaped `ECHILD`
  and a direct `exec` fixture.

On 2026-10-18 the eight exact blobs were compared with their then-final PR
heads and matched **byte-for-byte** at this ledger's tip. Those PR heads have
since moved (#945, #947 and #948 were changed after review), so this
comparison is historical and does not describe the current PR heads. Prior #948 evidence: full native
macOS Nextest **470/470 pass, five skipped**, warning-denied Clippy and hygiene
green; independent exact-head review **no blockers**; hosted workflow
[39144935715, attempt 2](https://github.com/gyldlab/keld/actions/runs/39144935715)
**22/22 jobs passed**. These results do not replace per-file architecture review.

## Source-driven boundaries and open decisions

1. **Do not conflate KGR1 and KGC1.** Generic terminal
   `HostGuardian::shutdown` has a signal/guardian-wait contract. A successful
   birth-attested `SIGKILL` need not prove all kernel group members reaped.
   KGC1 `CONTROL_CLEAR` has the stronger requirement because it authorizes a
   successor. Existing generic tests expect successful terminal shutdown while
   an independently test-owned group leader can still be unreaped. A new
   dual-`ESRCH` KGR1 requirement needs an explicit contract decision and an
   independent native falsifier; it is not an automatic #948 follow-up.
2. **Documentation drift.** `docs/architecture/06-runtime-and-tooling.md`,
   around lines 143–145 at the pinned tip, says `HostGuardian` consumes the
   group identity on every terminal path. That statement became false after
   #948: native cleanup failure retains the witness but cannot restart a
   terminal session. **Resolved in #948 `ec503285`** (2026-10-22): the
   paragraph now states that `HostGuardian` retains the birth on a failed
   cleanup and never signals again, because every terminal path drops the
   liveness writer and every public entry and `Drop` are gated on it;
   `llms-full.txt` was regenerated in the same commit.
3. **Independent Mac host acceptance gap.** #948 hosted attempt 1 failed in
   unchanged `keld-host::no_flag_macos::public_fs::real_pointer_public_fs_uses_stock_project_policy`
   at the child-exit/output deadline; attempt 2 passed with the same code and
   commit. This does **not** establish the intermittent host retirement cause
   or fix it. Neither extend test deadlines nor disable the test.
4. **Broader process custody remains unqualified.** Simultaneous host/Guardian
   death, detached process-group escapes, kernel-pinned group ownership,
   PID/PGID reuse races and signed installed GUI acceptance remain separate
   open contracts. Do not claim they are closed based on the scoped regression
   evidence.

### New targeted no-change decision (not full-file signoff)

Independent macOS read-only source review of the *one-generation supervised
lease* path found no source-proven loss of the last cleanup owner from
`SupervisedGuardianLease::revoke(self)` taking its local GroupIdentity before
a fallible native signal. The lease is one-shot, the Supervisor treats native
revocation failure as terminal `Failed`, and Bun cannot exec without the
original host's authenticated KGR1 registration acknowledgment. After that
acknowledgment, `HostGuardian` separately holds the original group birth.
The candidate would need a native falsifier showing both local and host
custody lost, a clean-success error, or unauthorized surviving live child.
None was established. No speculative production patch is authorized by this
review.

Source anchors at this tip: `supervised_lease.rs:137-173` (blob
`d27a8b3e65c96386cd703cf5acd6da02c3842b4f`),
`supervision_runner.rs:104-130` (blob
`4476e66f03ec1ee813bf631dcb051cfc84e776dc`),
`registration_ack.rs:27-79` (blob
`12ff97e80be5f95c20798976baba0be3b92ae995`) and
`tests/supervised_bootstrap.rs:133-222` (blob
`dab45f1853f87067bf1925ae7875cbc8f644f063`). This is an
**issue-specific no-change judgment, not full-file review completion**;
those rows correctly remain pending in the CSV.

An independent second read-only review of the *pre-owner generic*
`HostGuardian` constructor also found no proven executing-Bun orphan on
legitimate KGR1 constructor refusal. In `registered_guardian.rs:24-70`,
failed birth re-observation, liveness-FD validation or child inspection enter
`reject_proven_registration` (`:366-379`): it closes the only host liveness
writer, performs bounded exact-Guardian child wait and uses only
birth-attested group termination. `bootstrap.rs:193-214` sends the
`HOST_GROUP_RETAINED_ACK` **after** successful host owner construction.
`registration_ack.rs:21-85` and `spawn_admission.rs:73-134` refuse Bun
exec if the ACK/permit is missing or malformed. A failed cleanup signal
before owner construction still leaves a typed error without retryable host
custody; that residual limitation is not proof of unauthorized execution.
This decision is specific to the *pre-ACK constructor error*, not every
possible post-construction boot failure. All affected files remain
pending broader architecture review in the ledger.

Existing tests provide partial, not branch-complete, native proof:
`rejected_registration_deadline.rs:60-102` keeps an unrelated live process
group safe after refused KGR1 admission; its `:341-393` case covers
registered-child cleanup with a clean Guardian exit.
`retained_ack.rs:225-309` proves real Bun cannot exec before host retention
ACK; `generic_pre_registration_crash.rs:64-166` kills the actual Guardian
before registration and checks no Bun image/marker. A dedicated native
barrier-controlled test for the exact `registered_guardian.rs:51-52`
`Ok(Some(status))` constructor branch has not been demonstrated. This is
an explicit test-coverage gap, **not** a demonstrated security defect.
The distinct **post-construction ACK-write error** at
`bootstrap.rs:201-212` also returns a typed error and invokes the already
constructed owner's bounded shutdown; it does not return a successful
`HostGuardian`. The real pre-exec Bun permit still requires that ACK. An
independently controlled EPIPE/error cut before the ACK write is not
demonstrated by the existing retained-ACK host-death test; record that native
negative oracle as pending, without assuming an executing-child escape.

## Ownership and next review work

The narrow fixes belong to their exact draft PRs above. The separate macOS
auth-capacity draft [#941](https://github.com/gyldlab/keld/pull/941) is a
sibling of the CLEAR chain; do not modify it or other-agent worktrees.
The `main` tip is external to the #948 ancestry. No retarget, merge or
integration acceptance has been performed.

The next coherent, independently reviewed source groups are the remaining
production Guardian admission, supervision, registration, recovery, wire and
liveness modules; then their compiled native test owners and fixtures. Each
pending row needs explicit responsibility, callers, boundary/error analysis,
native falsifier where applicable, actual PR owner and a recorded fix,
no-change, or blocked disposition. Cross-component callers in
`keld-core/src/app_session.rs`, `keld-host` and platform webviews need their
own later ledger entries. A source-path census is not a dependency audit.

For reproducibility, use `git ls-tree -r -z <pinned-commit> -- <four scope roots>`
and verify each Git blob in `ledger.csv` against the pinned tip and the
corresponding pinned-main entry. Recount status and category columns without
substituting PR diff size for architecture coverage. Add an independent
reviewer and update this ledger only in a separate, correctly owned draft PR
as the next scoped decisions are actually proved. Do **not** merge.
