# Spec: repository-contained agent workspace
Status: approved
Linear: KEL-245 · Owner: repository maintainer · Updated: 2026-09-25

Approved by the repository owner on 2026-09-21 through the instruction to audit and
merge the design if ready. This specification defines the implementation target;
commands and hook enforcement become available only as the tasks below land.

On 2026-09-25 the owner authorized the Linux pilot agent to decide and repair its
prerequisites. The Unix socket failure amends physical scratch placement below:
session ownership stays explicit while scratch moves to a shallow private directory.
No product boundary changes; historical scratch and evidence stay readable.

## 1. Goal & non-goals

Give each primary Keld checkout one ignored `.keld-work/` directory for agent-created
worktrees, session evidence, scratch, auxiliary checkouts and optional tooling caches.
Starting, inspecting and finishing work should require a few predictable `just`
commands. A developer can locate an artifact by issue and session, see why it remains,
and clean eligible resources without reconstructing an agent conversation.

The user explicitly selects placement inside the primary repository, replacing the
current sibling-worktree policy. This applies on Windows, macOS and Linux.

Non-goals: product runtime changes; an agent orchestrator; a background janitor;
automatic deletion based on age; a database; global Git/shell settings; duplicating
Linear ownership; relocating OS-managed application profiles, credentials or global
package caches; promising that Git ignore rules sandbox arbitrary agent writes.
Existing useful research and benchmark source remains owned by its existing repository.

## 2. Spec refs

No boundary change to Keld's product: no handle/crash ownership or principal minting
changes. Development-process resource ownership changes are defined here.

- `docs/agents/workflow.md`: session continuity, claims, isolation, closeout.
- `.agents/coordination.md`: resource disposition and removal safety.
- `.agents/instructions.md`: one owner, routing, budgets and evaluations.
- `.agents/research.md`: research and benchmark repository ownership.
- `.agents/testing.md`: temporary fixtures, failure retention, native OS evidence.
- `docs/onboarding/05-development-guide.md`: user-facing setup and hook guarantees.

Implementation changes the owning instructions together; it must not leave both
`../keld-...` and the new layout as competing defaults.

### Current audit and evidence

The preceding local audit found 62 Keld-related directories including Prompt Tracker,
about 35.30 GiB, and 59 loose files in the parent workspace. Those are historical
measurements, not current disk-space or deletion eligibility guarantees. Subsequent
builds already change them. The audit separated generated output from dirty trees,
unique branches, useful reference checkouts and unpublished evidence.

Current source confirms the causes and integration constraints:

| Surface | Observation | Required consequence |
|---|---|---|
| Workflow isolation | Explicitly chooses sibling `../keld-...` trees | Replace the owning rule and its examples |
| Closeout | Validator hashes referenced files; receipts contain absolute paths; unlisted ignored files are outside its census | Reuse the validator; add containment checks for new sessions and preserve historical receipts |
| Storage | Closeout namespace has also accumulated build/probe trees | Keep evidence, scratch and rebuildable output distinct |
| Git hooks | Tracked checkout/merge scripts print reminders; local audit found no configured `core.hooksPath` | Do not attribute current clutter to automatic clone creation; keep hooks notification-only |
| Session hooks | Start/stop adapters; Cursor writes metadata; repeated repair is bounded | Extend existing adapters rather than installing another hook layer |
| Scanners | `agent_context.rs` walks directories; TSV transport test walks root; ignore is not automatically honored | Exclude the exact managed root without exempting similarly named source paths |
| Temporary files | Shell helpers and Python tests use system temp; renderer has a literal `/tmp` config path | Pass scoped scratch to tooling and remove hard-coded tool-output locations |
| Build output | Just recipes and consumers use `target/...` | Preserve per-checkout build layout; do not impose a shared Cargo target directory |

Windows probe with Git 2.52.0.windows.1: creating a linked worktree beneath a
gitignored directory kept primary status clean and resolved the correct common Git
directory. Git move preserved its files and registration. Removal refused an
untracked file. After committing that file, Git allowed removing the clean worktree
despite its unique commit (the named branch retained the commit). Therefore cleanliness
alone is insufficient for automatic task retirement. macOS/Linux native execution of
these probes remains required; WSL is not macOS evidence.

Primary documentation checked 2026-09-21:

- [Git 2.52 worktree manual](https://git-scm.com/docs/git-worktree/2.52.0): shared
  metadata, stable NUL-delimited inventory, move/remove restrictions and locks.
- [Cargo environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html):
  target output can be redirected, but package cache is a separate resource.
- [Python tempfile](https://docs.python.org/3/library/tempfile.html): explicit `dir`
  and inherited temp variables select scratch; cleanup remains the caller's concern.

Context7 discovery used `/git/htmldocs` for worktree inventory, common directory and
removal semantics; the official manual and local probe support the chosen Git behavior.
No external benchmark or runtime performance conclusion is inferred from this audit.

## 3. Acceptance criteria (binary, each becomes a test)

1. Commands invoked from the primary checkout or any managed linked tree resolve the
   same `.keld-work/` root. Bare, missing-primary, ambiguous or unsupported separate
   Git-directory arrangements fail with a repair message before writes.
2. Start resolves `--base` to a commit, defaulting to the locally available
   `origin/main`, never the invoking branch's HEAD. A missing/unresolvable ref refuses
   writes with guidance to fetch `origin main` or supply a valid base. Record the
   resolved SHA before tree creation. Existing paths, branches or active ownership
   collisions refuse overwrite; same-owner reuse preserves the existing task's base.
3. Root Git status excludes managed resources. Source scanners exclude the managed
   root but continue checking tracked source and detect an attempted force-add of
   managed content. A linked tree never creates another workspace inside itself.
4. A managed command receives session-owned scratch/temp paths below the primary workspace, runs in the
   assigned checkout, preserves argument boundaries and returns its actual exit code.
   Failure retains its bounded log and marks unfinished work; it cannot silently become
   success. Exceeding the log cap records truncation and total bytes without changing
   the child's exit status or growing the retained log past its declared limit.
5. Clean without an apply option changes nothing and reports exact paths, ownership,
   reclaimable logical bytes, retention reasons and unknowns. No home/disk-wide walk.
6. Cleanup refuses primary roots, traversal, foreign repositories, active sessions,
   Git locks, links/reparse points, mount-boundary escape, dirty/untracked source,
   unclassified ignored files, unknown PR state and evidence references into a target.
7. A clean task with unique unpreserved work or an open PR remains retained. Verified
   landed content and released ownership permit Git worktree removal without force.
   Squash merges use content evidence, not ahead/behind counts alone.
8. Evidence, baselines and closeout receipts remain available and hash-valid after
   scratch/cache cleanup and worktree retirement. Old receipts are not rewritten merely
   to make migration look complete.
9. Start hooks provide a bounded path pointer; stop hooks reuse one validator. Hooks
   neither download/build nor delete. Inactive factual turns do not create a session.
   Existing bounded repair and trust behavior is preserved.
10. Explicit migration plans distinguish registered trees, standalone clones, evidence,
    caches and unrelated data. Apply rejects changed manifests/owners and refuses
    unsupported moves. Dirty/active/open work remains in place with an actionable reason.
11. Missing cleanup metadata, an interrupted operation or a stale session never becomes
    permission to delete. Recovery reports the exact partial operation without retrying
    deterministic failures or taking over another writer.
12. Windows/macOS/Linux real filesystem tests cover nesting, quoted/Unicode paths and
    platform links. Client qualification separately confirms instruction-root loading
    in nested trees; no duplicated or truncated root/nested AGENTS chain is accepted.
13. A native session started in the primary checkout before issue selection receives
    a stable session path without creating files. Binding one issue, adding another
    through steering and resuming preserve exactly one original baseline location.
14. Research sync/push and competitor sync invoked from a linked tree use the verified
    primary reference checkout or explicitly refuse; they cannot create a task-local
    reference clone. Existing origin and dirty-tree protections still apply.

## 4. Design

### First-principles and reuse

Git owns worktree registration, refs and locks. Linear remains the cross-device task
claim authority. Existing session baselines/receipts own findings and evidence. A small
Python stdlib tool owns local path allocation and cleanup admission; `just` exposes it.
No new Rust crate, Python dependency, daemon or second closeout system is required.

Atomic components:

| Owner / boundary | Input → output | Failure / independent proof |
|---|---|---|
| Workspace resolver / Git identity | checkout + Git inventory → primary root | Linked tree picks itself: compare common identity from both roots |
| Git / worktree lifecycle | explicit branch/base → registered task tree | Dirty or unique work lost: real Git negative fixtures |
| Closeout validator / artifact lifetime | session receipts → preserved evidence | Referenced scratch removed: hashes and dangling-reference negative control |
| Local tool / mutation scope | validated ownership + targets → exact operation | Link/foreign/changed target accepted: refusal with sentinel outside root |
| Native adapter / client lifecycle | trusted event → bounded context/check | Prompt recursively scans or deletes: side-effect trace and native replay |
| Existing scanners / own source | root inventory → tracked/owned contracts | Ignored nested corpus double counted: hostile fixture plus tracked control |

Edges are explicit: resolver precedes any mutation; evidence references constrain
cleanup; scanners must exclude the root before managed worktrees are created.
Identity is not authorization: matching a Git common directory does not grant a task
claim. Authentication uses existing local accounts and GitHub/Linear credentials;
the tool stores no tokens. This is cooperative governance, not OS containment.
Same-user hostile processes can bypass the tool and mutate paths; no race-proof
security boundary is claimed. Cleanup requires quiescent owned targets and refuses
observable interference. Hard containment would require a separately approved sandbox.

### Layout and naming

```text
keld/
  .keld-work/                         ignored in every checkout
    worktrees/
      kel-245-workspace/              one Git worktree per issue/concern
      kel-245-workspace.json          small local ownership record
    sessions/
      codex-session-id/               stable before an issue is selected
        baseline.json
        current.json                 only if needed by the native adapter
        turns/turn-id.json
        evidence/                    retained logs, reports and selected artifacts
        scratch-owners/              retained per-run token and filesystem identity
        scratch/                     legacy disposable files, still supported
    tmp/<short-exclusive-token>/     private per-run scratch and fixtures
    cache/                           optional, explicitly rebuildable tool downloads
    repos/                           auxiliary standalone repositories when needed
      keld-benches/
      prompt-tracker/
```

Directories are lazy: do not create empty category trees to appear complete.
Task slug: `kel-` plus positive issue number and a lowercase kebab slug, maximum
64 ASCII characters. Client session/turn IDs reuse the existing validator grammar;
each is checked as a single component. Files use descriptive kebab names. Repeated
experiments use separate exclusive scratch directories mapped to their run IDs; only necessary result evidence is
promoted. Do not accumulate `final-final`, `retry2` or full copied checkouts as evidence.

All paths derive from the verified primary worktree, even from deeply nested linked
trees. Use `git worktree list --porcelain -z`, resolve the first primary record, then
cross-check its common Git identity. Do not infer the primary by removing `/.git`
from an arbitrary path or accept a free-form environment override for deletion roots.
Normal primary checkouts are supported first; unsupported layouts receive an error.

Keep normal checkout-owned `target/` and package `node_modules/` locations: linked
trees put them under `.keld-work/worktrees/...`, and primary builds remain in primary
`target/`. Existing `docs/research/` and `competitors/` are already repo-contained
canonical references and remain there in this version. No duplicate reference clones
are created in task worktrees. Optional benchmark/Prompt Tracker clones use `repos/`;
publication still goes to their owning remote repository. Do not replace canonical
paths with symlinks/junctions to avoid updating consumers.

Session paths depend only on primary Git identity and the client's existing namespaced
session ID, never on the selected issue or current working directory. Before activation,
a prompt hook may emit that path without writing it. Work-start binds the native
session ID to its issue/task in the local task record; manual clients provide an explicit
session ID. Additional issues add associations without moving the session or replacing
its original objective inventory. Stop resolves the same session when its cwd is still
the primary checkout. Work-status derives issue-to-session views from these records.

Research/competitor helper recipes must use the primary resolver too. Resolve reference
storage in the primary but use the invoking reviewed checkout's tool/lockfile inputs;
record their revision so a feature branch's reference pin is not silently replaced by
main's pin. An update conflicting with another active reference consumer is refused.
Research-push retains its separate-repository and allowed-origin checks at the resolved
primary destination. These operations remain explicit, never run by checkout hooks.

### Minimal commands

These are proposed interfaces; they do not exist until T1/T2 land.

| Command | Contract |
|---|---|
| `just work-status` | Read-only task/session/legacy summary; sizes only with `--sizes` |
| `just work-start kel-245 workspace` | Create/reuse the issue tree; default base is local `origin/main`; `--base` overrides it |
| `just work-run kel-245-workspace -- just ci` | Run argv in the registered tree with scoped scratch and captured evidence |
| `just work-finish kel-245-workspace` | Release this session after valid closeout; print eligible cleanup plan |
| `just work-clean kel-245-workspace` | Preview disposable resources for exactly this task |
| `just work-clean kel-245-workspace --apply` | Revalidate and remove only preview-eligible scratch/cache/retired tree |
| `just work-import manifest.json` | Validate/preview explicit legacy paths; `--apply` performs verified moves |
| `just work-check` / `just work-test` | Validate layout/integration and run negative-control tests |

`work-run` passes arguments as an array without a shell-built command string. It
sets child-only `TMPDIR`, `TEMP` and `TMP` to its session-owned scratch, using the right native
path representation for the invoking OS. No global environment or `HOME` rewrite.
Do not redirect Cargo target output across active checkouts: existing recipes depend
on local target paths and concurrent reuse can serialize or confuse artifact identity.
Repo-owned helpers with literal external temp paths are changed to use explicit
scratch. Application-profile/native-containment acceptance keeps its real platform
locations when that location is the tested contract. Each owning test allocates an
exclusive unique resource and records its exact path and creation identity before use;
its fixture cleanup revalidates that identity and removes only that created resource.
Pre-existing files, credentials and profiles are preserved. Unknown/replaced ownership
is a retained failure. `work-clean` never deletes these external exceptions; it reports
them for the owning fixture/operator. A pre-existing outside sentinel must survive.

Command output streams to the operator; retained diagnostic tails default to at most
4 MiB per stdout/stderr stream (8 MiB combined per run), with total/omitted byte counts
and an explicit truncation marker. A task needing larger logs selects a finite integer
`--log-limit-mib` from 1 through 64 per stream; invalid/unlimited/over-64 requests refuse
before launch. Thus all runs have a maximum retained payload of 128 MiB combined.
This is an operational per-run bound, not a claimed total workspace quota: accumulated
evidence still needs explicit retention review. Output beyond the selected cap remains
diagnostic tails and cannot be called complete evidence. Do not dump
environment variables, credentials or raw argument values into metadata. Retained logs
are local/private; publication requires deliberate selection and review. Work-status
with sizes reports retained evidence separately from reclaimable scratch/cache so
preservation does not conceal accumulating storage.

The local task record stores schema version, task/issue ID, branch, starting commit,
session IDs, relative category paths and active/released disposition. Git status,
PR status and sizes are derived on demand, not cached as truth. Records are created
exclusively and updated atomically; a per-task exclusive operation lock prevents two
cooperating commands mutating one task. No global registry database or recursive hash
manifest of build outputs. Lost lock ownership is reported and inspected, not timed out
into automatic takeover. Managed child PID data is diagnostic, not proof that no
external editor/process is using the tree.

### Cleanup and retention

Default retention is by purpose, not file age:

- Active worktree, uncommitted work, unclassified data and unpublished evidence: keep.
- Released session scratch: eligible after referenced evidence has been promoted.
- Rebuildable caches: optional exact-category cleanup; report rebuild cost, not an
  unmeasured speed benefit. No cleanup on every prompt or checkout.
- Closed task tree: eligible only after release, fresh remote PR/base checks, source
  preservation and ignored-file classification. Offline/unknown stays retained.
- Published evidence: retained locally by default. Evidence deletion needs a separate
  explicit archive/purge action after hashes, retrievability and all references are
  verified; v1 `work-clean` never deletes evidence.

Resolve and validate every target against its category before mutation. Refuse roots,
`..`, absolute user-supplied deletion targets, symlink/junction/reparse ancestors,
mount transitions and nested foreign repositories. Recheck immediately before each
operation; local locking is not a defense against a malicious same-user process.
Use Git removal for registered worktrees without `--force`. Unknown ignored content
blocks whole-tree removal even if Git calls the worktree clean. For validated source
symlinks, do not follow them; if the cleanup implementation cannot establish safe
native handling, retain that tree for explicit operator disposition.

Never delete the source branch as part of cache cleanup. Worktree retirement preserves
its ref when a historical receipt names it. Before optional branch retirement, verify
the PR merge and preserved source identity; ahead/behind alone does not prove absence
of unique work after squash. The existing closeout validator requires a retained
`source_ref` for removed checkouts: preserve that invariant or explicitly version its
replacement before removing the ref. Deletion errors remain failures with exact paths.

Work-finish validates the pre-cleanup disposition and releases only its own session.
Cleanup records each actual removal/failure, then writes a new current-turn receipt
and runs the existing validator after the final state change. Preserve previous
receipts as historical evidence; never replay an earlier pass as proof of cleanup.
A crash between removal and the new receipt remains an explicit recovery/handoff.

### Hook audit and integration

Keep one existing hook adapter and validator. Codex/Claude start emits a short
workspace/session pointer. Cursor retains its small atomic `current.json`. No full
drive scan, full cache census, builds, installs, remote calls or automatic deletion
on these events. Ordinary unactivated questions stay cheap and create no baseline.
Stop checks only the activated session and its declared resources/evidence; retain
the existing single repair cycle and truthful handoff. Large evidence verification
is explicit closeout work, never repeatedly triggered by every tool invocation.

Use a shared resolver from `tools/workspace.py` in closeout/adapter integration;
update the hash-bound bootstrap to verify every imported project module before use.
Regenerate registrations through the existing generator, preserving other user hooks.
Do not install globally or assert live activation from a JSON file alone. Existing
native trust must be renewed by the client's supported mechanism after changed hashes.
Git checkout/merge hooks remain notification-only and never execute incoming scripts.

Nested worktrees can affect how clients search ancestor AGENTS files. Native traces
must show one intended root chain without parent duplication, budget overflow or
truncation for each claimed client. An unsupported client gets an explicit manual
workflow limitation; it is not silently exempted from containment or budget rules.

### Native socket path allocation

`work-run` and `reference-run` reuse one allocator: exclusive owner-private
`mkdtemp` below primary `.keld-work/tmp/`, with an eight-character token. Retained
`sessions/<session>/scratch-owners/<token>.json` records session, run and the created
directory's device/inode identity. Long session/run names never enter child temp paths.
Cleanup resolves only that session's records, validates the fixed immediate-child
namespace and exact identity, then applies the existing release, reference, mount,
link and per-entry identity checks. It also supports legacy session scratch. Never
recursively clean the shared `tmp` parent; unrecorded/ambiguous resources stay retained.
The existing same-user cooperative metadata model remains; this is not an OS sandbox.

Rejected alternatives: relative temp paths change meaning when children change cwd;
symlink or inherited-descriptor aliases add platform/lifetime coupling; unmanaged
system-temp fallback violates repository ownership. This physical-layout repair
preserves arbitrary child argv/cwd/exit behavior and requires no test-specific override.
A very long primary checkout can still exceed native Unix socket limits; use a shorter
real primary checkout path for socket suites. No fixed global pathname allowance can
prove every child fixture, and unrelated non-socket commands remain available.
Prove actual binds/exchanges through `work-run`, session isolation, replaced/forged
ownership refusal, referenced evidence retention and legacy cleanup before landing.
Linux results do not stand in for native macOS/Windows qualification.

### Windows runtime and managed path admission

Windows workspace mutation requires a final CPython 3 release with the
`os.mkdir(0o700)` private-directory fix: 3.9.20, 3.10.15, 3.11.10, 3.12.4 or a later
patch in those branches, or 3.13 onward. The shared admission refuses earlier versions,
prereleases, other implementations and unqualified major versions before writes. It
reuses patched `tempfile.mkdtemp`, with no custom ACL implementation or automatic
runtime/configuration changes. Unix support is unchanged. See the Python
[CVE-2024-4030 advisory](https://mail.python.org/archives/list/security-announce@python.org/thread/PRGS5OR3N3PNPT4BMV2VAGN5GMUI5636/)
and the versioned `os.mkdir` documentation for its security backports.

Before allocating a task, lock or scratch directory, the shared workspace owner
checks the derived task/session metadata paths against a conservative Windows support
cell: at most 247 UTF-16 code units per absolute directory path and 259 per file path.
This reserves directory-creation and terminating-NUL space, including generated
scratch-owner names, cleanup/run evidence, stream logs and atomic record replacement
names. The cell applies even when a host enables longer ordinary paths; it makes no
claim about arbitrary child paths or Git-tracked source filenames. The existing
128-character session grammar remains unchanged. Unsupported combinations fail before
operation-side writes with the exact path and shorter real primary checkout guidance;
preserve the session identity and historical evidence. No extended-path rewrite,
system-temp fallback or machine/global configuration change is performed.

### Migration and compatibility

Start with a new-session cutover, then migrate legacy data separately. Existing
`.git/keld-closeout/` stays readable at its original location; existing hashes and
absolute paths are immutable historical evidence. New sessions use the new root;
resuming an old activated session continues its original namespace. If both
namespaces contain an activation for the same identity, refuse ambiguity. A legacy
namespace is accepted only for an existing baseline, never a silent new-session fallback.

`work-import` consumes an explicit list of absolute sources, intended categories and
owners. The command never scans siblings by prefix and assumes they belong to Keld.
Recheck clean/active/claim/PR state and path identity before apply. Registered trees
move through Git; standalone clones with linked worktrees, submodules, dirty changes,
active locks or unknown external references stay pending. Dirty trees can be relocated
later after their owner closes the session and approves the exact move. Evidence with
absolute references is archived without breaking the original receipt paths; moving
it requires a separately versioned reference migration, so v1 keeps it in place.
Each move has source/destination manifests and a post-move content/registration check;
on failure preserve both locations and report, without overwriting or guessing repair.

Compatibility fallback: legacy sessions/checkouts remain visible and retained, while
new task allocation is restricted to the canonical root. Existing helper commands and
source repository locations remain valid during migration. Retirement happens only
after every legacy owner has a verified disposition.

## 5. Boundaries

Implement in: `tools/workspace.py` and its colocated tests; existing
`session_closeout.py`, `session_closeout_hook.py` and tests; `justfile`; `.gitignore`;
workflow/coordination/testing owner sections; development guide; existing scanner
owners and their tests; generated client registration and docs as needed.

The workspace policy belongs in the existing coordination resource section. Workflow
links that section and changes its isolation example; it does not duplicate lifecycle
rules. Keep root `AGENTS.md` unchanged unless routing evidence proves it necessary.
Do not add a new universally loaded instruction file or raise an always budget.
Instruction deltas record before/after bytes and pinned tokens, route/consumer changes,
representative eval and rollback. Reallocate routed prose before proposing cap growth.

Must not touch: production crate implementation, Cargo dependencies, public kipc/permission schemas,
global client settings, unrelated project directories, credentials or `.env*` files.
The Linux strict acceptance fixture may explicitly allocate its RAII-owned hostile
launcher ancestor in native `/tmp`: a mutable directory behind managed private scratch
is correctly accepted and cannot prove public-ancestor rejection. This is the native
containment location exception above, not an unmanaged command-temp fallback. Preserve
the negative assertion, check the actual public/sticky directory mode, and change no
production trust logic.
Scanner changes stay narrowly scoped; reuse `repo_path_contract.rs` for shared Rust
path decisions rather than creating a second copy. Cross-language consumers receive
one narrow published path contract and shared sentinel cases, not a new generator.

## 6. Tasks (each approximately one PR; ordered)

- [ ] T1: approved layout, exact ignore/scanner boundary, resolver, task start/status/run,
  naming and ownership, docs/route updates, scoped tool scratch and real Git tests.
- [ ] T2: explicit finish/preview/apply cleanup, safe retention, evidence-reference
  protection and negative controls; integrate new-session storage in existing hooks
  with hash regeneration and native activation evidence.
- [ ] T3: explicit migration inventory and import command; migrate eligible legacy
  resources after owner checks, leave actionable pending rows for others, record actual
  reclaimed bytes and native OS qualification. Do not mark the whole migration complete
  while active/dirty/unknown resources remain.

Each slice ships usable behavior; do not expose unimplemented commands or placeholder
handlers. New command recipes land with their implementations and tests.

## 7. Test plan

| Criteria | Independent proof |
|---|---|
| 1–3 | Real nested Git fixture; same resolver from primary/linked tree; omitted base uses local origin/main; explicit base overrides; missing ref refuses; ignored fake AGENTS/TSV excluded; tracked decoy and force-added local content rejected |
| 4 | Child reports cwd/temp/argv and exits nonzero; spaces and Unicode retained; literal shell metacharacters remain argv data; default, 1/64 MiB and rejected 0/65/unlimited log limits; over-cap tails report exact omitted counts |
| 5–7 | Snapshot before preview; active/dirty/untracked/open/unknown/foreign/unique fixtures survive; merged clean fixture removed only through Git |
| 6, 11 | Outside sentinel survives symlink/junction/path traversal and platform-fixture cleanup; created fixture removed only with matching identity; operation interrupted at allocation/move/remove; second writer refused |
| 8, 10 | Hash-bound evidence remains after cleanup; legacy baseline roundtrip; changed import inventory refused; nested clone and submodule cases held |
| 9, 12 | Inactive/start/stop/stale/repeated-error adapter tests plus real supported client traces on each claimed OS |
| 13–14 | Primary-start session retains its baseline after issue selection/steering and stop; linked-tree sync/push resolves the one primary reference without cloning |

Run the instruction-specific gates, regenerated corpus checks and the full local
gate on the final implementation. One builder per checkout. Preserve successful
evidence across unchanged revisions; rerun affected checks after a real change.
Native macOS/Windows/Linux filesystem claims require their own execution receipts.

## 8. Review gates triggered

Product gates: unsafe none, public API none, permission model none, dependency none,
wire protocol none. Independent instruction and destructive-filesystem safety review
is required for implementation. Hook trust/loader changes require their native
activation evidence; passing simulated events alone is insufficient.

## 9. Perf impact

No product performance claim. Record separate measurements for inventory count,
filesystem work, hashing/copy bytes, child queues, monotonic elapsed time and retained
artifact size. `work-status` without sizes should visit metadata/task records, not
every file in build/reference trees. Size census is opt-in and reports logical bytes,
not guaranteed physical disk recovery (hard links/compression can differ).

Measure start/stop hook overhead on inactive and active sessions with small versus
large caches. Negative control: growing cache files must not increase files scanned
by a normal prompt hook. Do not add a background service or shared build cache based
on assumed speed. Retention and fewer redundant checkouts reduce management work;
numeric improvements require a before/after census on the same machine.

## 10. Open questions

None blocking the approved design. Native macOS/Linux and per-client nested instruction
traces remain implementation acceptance work, not evidence established by this Windows
audit. Merging this specification does not complete T1–T3 or close KEL-245.
