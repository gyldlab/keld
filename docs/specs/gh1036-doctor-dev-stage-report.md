# Spec: Read-only development-stage diagnostics

- Status: draft
- Tracker: GH-1036
- Owner: @0monish
- Updated: 2026-11-22
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Add one `dev-stage` finding to `keld doctor` for a selected project. Report the entries
observed under `.keld/dev`, their metadata age and stage-name shape, without claiming
that any entry is stale, dead or safe to delete. Reuse the doctor output used by the
command line and MCP. This is a future implementation contract, not implemented behavior.

Non-goals: deleting stages, repairing permissions, garbage collection, liveness probes,
opening stage contents, automatic removal commands, PID/lease markers, new Windows ACL
policy, installed-app diagnosis, or changing the Host/Guardian/sentinel cleanup owners.

## 2. Spec refs

- [GH-1036](https://github.com/gyldlab/keld/issues/1036) owns the read-only decision.
- Architecture [06 §1–2](../architecture/06-runtime-and-tooling.md) owns development
  staging and cleanup. [07 §2, §4 and §7](../architecture/07-agent-experience.md)
  owns structured errors and the shared CLI/MCP result.
- Source baseline: [`0eb3ce203187`](https://github.com/gyldlab/keld/tree/0eb3ce203187f35c5276a4237189c906a5220d5a).
  `crates/keld-cli/src/doctor.rs` owns `Check`, `DoctorFinding`, `run_checks`,
  `run_findings` and the top-level JSON array. It has no dev-stage finding.
- At that baseline, `crates/keld-cli/src/boot.rs` owns nonce construction,
  `prepare_linux_dev_root_for_uid`, retained Unix source handles and
  `open_windows_launch_guard`. The Linux preparation helper can create directories
  and change permissions; calling it from doctor would violate this contract.
- `crates/keld-cli/AGENTS.md` forbids caller-selected deletion and duplicated ACL
  policy. `crates/keld-cli/tests/doctor_mcp.rs` owns CLI/MCP comparison.
  Error identifiers remain owned by `docs/engineering/keld-error-codes.md`.

## 3. Acceptance criteria

### AC-1: Distinguish absent, empty and unselected projects

- **Given:** No project is selected, or a selected project has an absent or empty `.keld/dev` directory.
- **When:** Doctor runs without allocating any stage path.
- **Then:** With no selection, it emits no `dev-stage` finding and creates no path. With a selection, it emits one successful finding that distinguishes absent from empty.
- **Negative control:**
  - **Single fault:** Make absent and empty both return the same empty-success label.
  - **At-risk precondition:** The fixture exercises both cases with no stage entry created.
  - **Expected RED:** The output cannot distinguish absent from observed empty.

### AC-2: Show real stage names without a stale verdict

- **Given:** A selected project contains two real directories with distinct 32-character lowercase hexadecimal basenames.
- **When:** Doctor inspects direct stage entries without opening their contents.
- **Then:** The successful finding lists both names, observed kind, stage-name shape and modification-time age. This observation alone does not make doctor exit nonzero.
- **Negative control:**
  - **Single fault:** Treat any valid-looking nonce directory as stale and return a failed finding.
  - **At-risk precondition:** Both nonce directories are valid and one belongs to a live development session.
  - **Expected RED:** An informational entry incorrectly fails doctor or claims inactivity.

### AC-3: Describe unexpected entries without following them

- **Given:** The direct child is a malformed name, regular file, symlink or Windows reparse entry.
- **When:** Doctor enumerates and classifies that child.
- **Then:** The finding reports its observed kind and unexpected status. Doctor never follows its target, opens its contents, deletes it, or certifies it as a stage.
- **Negative control:**
  - **Single fault:** Follow a symlink to classify its target as a valid stage directory.
  - **At-risk precondition:** An outside target has a valid-looking nonce name while the direct child is a link.
  - **Expected RED:** The result certifies the target or an outside entry instead of describing the link.

### AC-4: Reject substituted project parents and dev directories

- **Given:** The selected `.keld` or `dev` directory is replaced by a symlink or Windows reparse entry, including after admission.
- **When:** Doctor resolves and enumerates the selected project.
- **Then:** The observation fails safely, and no children of the outside destination appear. Later operations remain bound to the admitted directory identity.
- **Negative control:**
  - **Single fault:** Replace retained-parent access with a second, unprotected path traversal.
  - **At-risk precondition:** A controlled substitution points `dev` outside the selected project between admission and enumeration.
  - **Expected RED:** An outside sentinel name appears, or doctor reports a complete successful scan.

### AC-5: Report private-directory failures without repair

- **Given:** A Unix `.keld` or `dev` parent has the wrong owner or a non-private mode, or a nonce entry has unsafe metadata.
- **When:** Doctor inspects the parent and entry through the shared non-mutating staging predicates.
- **Then:** Wrong parent ownership or mode yields a failed finding with the staging diagnostic and fix. The entry stays visible with its actual metadata. Doctor never changes owner, mode or permissions.
- **Negative control:**
  - **Single fault:** Call the existing preparation helper that changes permissions before reporting.
  - **At-risk precondition:** The fixture starts with an unsafe parent and records mode and ownership before and after.
  - **Expected RED:** The diagnostic changes the filesystem or reports the repaired state as the original observation.

### AC-6: Preserve incomplete enumeration and access errors

- **Given:** Enumeration, permission or metadata access fails, or an entry disappears during enumeration.
- **When:** Doctor attempts a read-only observation of that directory.
- **Then:** It returns a failed or explicitly incomplete finding, never a successful complete zero count. A vanished entry is reported as changed, not dead.
- **Negative control:**
  - **Single fault:** Convert an enumeration error into an empty successful collection.
  - **At-risk precondition:** The fault occurs after the selected project and staging parent are admitted.
  - **Expected RED:** A complete successful empty finding hides the observable failure.

### AC-7: Qualify modification-time ages

- **Given:** A stage has a missing, unreadable or future modification time, or the clock changes.
- **When:** Doctor computes age from one captured scan-time display clock.
- **Then:** Age is marked unknown or future-dated as appropriate. No negative duration, invented zero, creation-time claim or process-liveness verdict is emitted.
- **Negative control:**
  - **Single fault:** Clamp an unreadable or future timestamp to an age of zero.
  - **At-risk precondition:** The timestamp fixture forces the invalid/future branch with a fixed clock.
  - **Expected RED:** The result fabricates a zero-age fact instead of preserving uncertainty.

### AC-8: Preserve identical CLI and MCP finding semantics

- **Given:** The same project and stage observations are supplied to CLI doctor and MCP.
- **When:** Both consumers run the shared `Check` and `DoctorFinding` producer.
- **Then:** `doctor --json` remains a top-level array, MCP returns equivalent findings, and failed observations retain the existing structured error and truthful fix.
- **Negative control:**
  - **Single fault:** Drop the new finding from one serializer only.
  - **At-risk precondition:** Both consumers use identical normal and failed observations.
  - **Expected RED:** Their finding data, result shape or failure meaning diverges.

### AC-9: Prove zero explicit filesystem mutation

- **Given:** The selected project contains a nonce child with separate observable contents, under an attributed operation audit.
- **When:** Doctor scans the stage parent and reports direct-entry metadata.
- **Then:** Doctor makes no explicit create, content write, chmod, chown, rename or remove operation. It never opens nonce contents or walks child directories. Read access-time changes and concurrent actor changes are not attributed to doctor.
- **Negative control:**
  - **Single fault:** Open each nonce child file or run a chmod preparation step during diagnosis.
  - **At-risk precondition:** The audit observes the doctor process and records before/after names, contents, ownership and modes.
  - **Expected RED:** The operation trace or controlled manifest reports content access or an explicit doctor mutation.

### AC-10: Preserve live sessions and release every scan handle

- **Given:** A real `keld dev` stage is active, or an earlier scan has failed during enumeration.
- **When:** Doctor runs, then a later healthy doctor scan runs in the same environment.
- **Then:** The report states activity is unknown and does not stop the active session. All scan handles are released, and the healthy subsequent scan succeeds.
- **Negative control:**
  - **Single fault:** Leave a directory handle retained on the failing scan path.
  - **At-risk precondition:** A controlled failure and a live stage are both available without terminating the session.
  - **Expected RED:** The later scan fails from leaked ownership or the live session is interrupted.

### AC-11: Require honest manual cleanup guidance

- **Given:** Onboarding documentation explains the new stage finding and possible leftover entries.
- **When:** An operator reads guidance after observing a named stage.
- **Then:** Guidance distinguishes observations from independently established inactivity. It requires manual operator confirmation before removal, and generated-document freshness passes.
- **Negative control:**
  - **Single fault:** Replace the guidance with a command that automatically deletes old-looking entries.
  - **At-risk precondition:** A reported age or nonstandard shape has no independent inactivity evidence.
  - **Expected RED:** Documentation authorizes deletion from age alone, or generated-document checks fail.

## 4. Design

### First-principles and reuse

No application ownership, permission, wire or process boundary changes. This adds a
cold diagnostic reader to the existing CLI, not a new cleanup owner.

| Atom | Owner and boundary | Failure mode | Independent observable |
|---|---|---|---|
| Enumeration | CLI reader; selected project to names and metadata | A path substitution redirects reads | Outside-root sentinel names never appear |
| Stage rules | Existing boot producer/validators | Doctor copies or mutates admission policy | Shared predicate tests and unchanged modes |
| Interpretation | Doctor finding producer; metadata to prose/JSON | Age becomes a stale/dead verdict | Live-stage case remains activity-unknown |
| Delivery | Existing CLI/MCP serializers | One front end silently omits or changes failures | Equivalent findings, exit and error shape |
| Lifetime | Reader owns scan handles only | A failed scan leaks a handle or blocks normal use | Handle-release and healthy-next-scan controls |

These atoms are independent: a correctly formatted result does not prove safe
enumeration; private permissions do not prove process inactivity; non-following reads
do not prove that every entry was observed in a changing directory.

Use `boot.rs` as the shared owner for nonce shape and non-mutating metadata
validation. Extract the smallest pure validator required by the existing producer and
doctor. Do not call a preparation helper that creates or chmods directories. Do not
call a Host stage-admission path that opens or hashes stage files. No second ACL parser,
stage registry, cleanup service or general filesystem walker is required.

### Read scope and observation

Resolve the existing selected project once. Retain admitted directory identity while
walking only its fixed `.keld` and `dev` children. Use the existing retained-handle,
no-follow pattern on Unix and the existing Windows namespace/reparse checks. Validate
metadata on the object actually retained. A path-only check followed by a fresh
unprotected traversal is insufficient.

Enumerate direct children only. Inspect each child without following links and without
opening contents. Preserve a lossless escaped display of unusual names, including
control characters; never produce a shell command containing an entry's raw name.
Use a consistent lexical ordering for a completed scan. The count is the number of
entries observed, not an atomic snapshot or an active-session count.

Capture the display clock once per scan. Age means elapsed wall-clock time since the
observed modification time; it is informational, not creation time. A clock shift,
unavailable timestamp or concurrent deletion remains qualified in the result.
There is no retry loop that hides directory churn or turns an incomplete scan into empty.

On Unix, share the staging owner's current-user and private-directory rules for
`.keld` and `dev`; report violations without repair. On Windows, reuse the existing
stage-parent protection/reparse admission only where it is already supported. Do not
invent a deeper ACL validator or equate Unix mode bits with a Windows security proof.
If the existing safe reader cannot observe a path, return a failed/unsupported finding.

### Output and errors

Keep the `Check`/`DoctorFinding` shape. Add a `dev-stage` label through
`run_checks`; `run_findings`, CLI text/JSON and MCP consume that same result.
The detail lists each direct entry's escaped name, kind, shape and age qualification.
It states: an entry can belong to a running development session; age does not identify
unused stages; removal requires independent operator confirmation.

Absent/empty and fully observed informational entries have `ok: true`. Unsafe parents
and observation failure have `ok: false` and the existing error-object shape. A
nonce entry with unsafe metadata is named explicitly; no silent normalization occurs.
Do not suppress unrelated Bun/project/renderer failures when this finding succeeds.

Reuse an existing registered code only if its meaning covers the condition exactly.
Otherwise allocate the next free CLI diagnostic identifier in the single registry in
the implementation PR; do not reserve a guessed number here. Its fix explains how to
inspect the named parent or retry observation and never promises automatic deletion.

New public Rust type: none required. New channels, capabilities, manifest fields,
persisted state, dependency or wire bytes: none. Runtime seam: none.
Migration unit: doctor producer, CLI/MCP consumers, shared boot metadata predicate,
onboarding guidance and existing tests. No temporary adapter.

## 5. Boundaries

Implement in `crates/keld-cli/src/doctor.rs`, the smallest shared non-mutating
predicate in `crates/keld-cli/src/boot.rs`, and their existing test families.
Update `crates/keld-cli/tests/doctor_mcp.rs`, onboarding guidance and the existing
error registry only where the added finding requires them.

Do not change Host/Guardian/sentinel deletion, lifecycle, dev leases, stage production
semantics, security permissions, generated sources by hand or shared CI policy.
A new Windows production unsafe operation is outside this draft and needs its own
nearest-owner authorization; it cannot be hidden inside a diagnostic convenience helper.

## 6. Tasks

### T-1: Add the non-mutating stage metadata seam

- **Blocked by:** none

Extract the shared read-only metadata and nonce-shape predicate. Add the `dev-stage` finding with real absent, empty, ordinary and unexpected-entry fixtures. Prove that existing stage production stays unchanged.

### T-2: Qualify non-following parent enumeration

- **Blocked by:** T-1

Qualify every supported OS with retained parent identity, reparse and symlink substitution, permission errors and zero-mutation controls. Keep unqualified OS cells explicitly open.

### T-3: Reuse CLI and MCP findings without cleanup authority

- **Blocked by:** T-2

Wire the shared finding through CLI and MCP, error registry and onboarding guidance. Qualify a real active development session and rerun doctor and boot regressions on the exact final implementation.

## 7. Test plan & evidence

| Criteria | Test and independent negative control |
|---|---|
| AC-1 through AC-3 | Real temporary paths with exact expected names/kinds; constant-empty result fails |
| AC-4 | Outside sentinel plus deterministic parent/child substitution; removing no-follow fails |
| AC-5 | Owner/mode fixture plus before/after metadata; calling preparation/chmod fails |
| AC-6 and AC-7 | Injected observation failure and fixed clock plus real missing entry; success/zero fallback fails |
| AC-8 | Existing doctor CLI/MCP harness; omit the finding in either consumer and parity fails |
| AC-9 | Attributed operation audit and controlled manifest, excluding access time; content access or explicit mutation fails |
| AC-10 | Real per-OS dev session and scan-release control; stopping a session or leaking handles fails |
| AC-11 | Guidance review and actual generated-doc freshness checks |

Synchronize race controls with explicit rendezvous, not sleeps. Test helpers own their
temporary paths and cleanup; the diagnostic under test does not. Separate pure rendering
tests from actual Unix/Windows filesystem evidence. Source review or hosted
documentation CI does not satisfy the real-OS cases.

## 8. Review gates triggered

Public API: additive doctor/MCP finding and observable errors. Filesystem/security
review: retained identity, non-following observation and no mutation. No permission-model,
dependency or wire change is proposed. Unsafe: none authorized by this draft.

## 9. Perf impact

Cold CLI work proportional to direct-entry names/metadata, with no recursive reads,
file-content hashing or process polling. Measure duration and peak retained report size
on an empty directory and a large representative stage directory before claiming cost.
No performance improvement or hot-path impact is claimed.

## 10. Open questions, blockers & unknowns

### B-1: Require approved read-only OS primitives

- **Status:** open
- **Owner:** GH-1036 implementation and platform filesystem owners
- **Impact:** The stage scanner cannot claim safe access or run implementation while the retained-handle and reparse rules are unqualified.
- **Check:** Prove AC-4 on every proposed OS with an approved, non-mutating retained-directory reader. Keep unsupported cells marked unsupported.

### Q-1: Approve the exact design before implementation

Independent technical approval is pending. The responsible owner must review the exact source and AC-1 through AC-11 before implementation starts. This draft requests no permission to delete entries or start the implementation.
