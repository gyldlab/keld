# Spec: Read-only development-stage diagnostics
Status: draft
Linear: GH-1036 (#517) · Owner: @0monish · Updated: 2026-11-06

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

## 3. Acceptance criteria (binary, each becomes a test)

1. Given no selected project, doctor emits no `dev-stage` finding and creates no path.
   Given a selected project with absent or empty `.keld/dev`, it emits one successful
   finding that distinguishes absent from observed empty.
2. Given two real directories whose basenames are 32 lowercase hexadecimal characters,
   doctor lists both names, entry kind, stage shape and modification-time age.
   This informational finding is successful; it alone cannot make doctor exit nonzero.
3. Given an unexpected basename, regular file, symbolic link or Windows reparse entry,
   doctor reports that entry's actual kind and unexpected status without following it,
   opening its target, deleting it or treating it as a verified stage.
4. Given a symbolic-link/reparse substitution at `.keld` or `dev`, doctor refuses
   that observation. An outside directory's children must never appear in the result.
   Replacing a selected parent after admission cannot redirect later reads outside it.
5. Given a Unix `.keld` or `dev` directory with the wrong owner or non-private mode,
   doctor emits a failed finding with the shared staging rule's diagnostic and fix.
   It changes neither mode nor ownership. A non-private nonce entry is reported as such;
   the diagnosis never grants authority to repair it.
6. Given a permission-denied, enumeration or metadata failure, doctor reports failed or
   explicitly incomplete observation, never a complete successful zero count.
   An entry disappearing during enumeration is reported as changed, not as proof of death.
7. Given unreadable, missing or future modification time, the age is explicitly unknown
   or future-dated. It is never a negative duration, fabricated zero or liveness verdict.
8. The same observations produce equivalent `Check` and `DoctorFinding` data.
   `doctor --json` remains a top-level array; MCP uses the same finding producer.
   Failed findings carry the existing structured error shape and a truthful remedy.
9. An attributed filesystem-operation audit or controlled before/after fixture proves
   that doctor made no explicit create, content-write, chmod, chown, rename or removal
   operation. Membership, contents, owner, mode and modification time stay unchanged
   in that fixture. Kernel access-time updates from reads are not writes by doctor;
   concurrent actors' changes are not attributed to it. A separately observable file
   inside a nonce is not opened; child directories are not traversed.
10. A real running `keld dev` stage may appear in the report. The report says activity
    is unknown and does not interrupt that session. A healthy subsequent doctor scan
    succeeds after a failed or concurrently changed scan; every scan handle is released.
11. Onboarding guidance points to this finding and retains manual operator confirmation
    before any removal. It distinguishes an observed entry from a leftover proven
    inactive by some independent means. Generated-document freshness checks pass.

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

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 Add the shared read-only metadata/shape seam and dev-stage finding with real
  absent, empty, ordinary and unexpected-entry fixtures; prove producer behavior stays unchanged.
- [ ] T2 Complete safe-parent and non-following enumeration on each supported OS,
  with substitution, access-error and zero-mutation controls; keep unqualified OS cells open.
- [ ] T3 Wire the one finding through CLI/MCP, typed errors and guidance; qualify a live
  development session and rerun existing doctor/boot regressions on the final source.

## 7. Test plan

| Criteria | Test and independent negative control |
|---|---|
| 1–3 | Real temporary paths with exact expected names/kinds; constant-empty result fails |
| 4 | Outside sentinel plus deterministic parent/child substitution; removing no-follow fails |
| 5 | Owner/mode fixture plus before/after metadata; calling preparation/chmod fails |
| 6–7 | Injected observation failure and fixed clock plus real missing entry; success/zero fallback fails |
| 8 | Existing doctor CLI/MCP harness; omit the finding in either consumer and parity fails |
| 9 | Attributed operation audit and controlled manifest, excluding access time; content access or explicit mutation fails |
| 10 | Real per-OS dev session and scan-release control; stopping a session or leaking handles fails |
| 11 | Guidance review and actual generated-doc freshness checks |

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

## 10. Open questions

Exact-content approval remains pending. The implementation owner must confirm that the
existing per-platform safe-read primitives satisfy criterion 4 without new unsafe scope.
If a platform lacks that primitive, keep its diagnosis explicitly unsupported and obtain
a separately reviewed owning-primitive change before claiming support.
No permission is requested here to delete entries or to start implementation.
