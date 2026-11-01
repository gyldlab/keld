# KELD #824 — final 19 historical Guardian source receipts

**Conservative, LIMITED source documentation.** This branch changes no Rust source,
Guardian behavior, process authority, native test or shipping release. A reviewed
historical path is not a certification of the current implementation or every OS
race.

This docs-only child builds directly on draft **#990** at immutable parent
`334bae72d27613ca744e5e3be86dac5b850d2e9f`. All source receipts reference
the historical **#949** revision
`899e287ea487f38ec1cd347f7c9db8f1927348ef` and **#952** revision
`77b1c5aef9202a6cc0d84195fc1f4c32e1eb1cba`. The 19 selected files
have identical Git blobs in both revisions and were previously marked
`pending-source-review` in the parent ledger. The earlier evidence files remain
unchanged.

## Exactly reviewed source paths

Every `review_append.csv` record names its source path and Git blobs, actual
caller/fixture responsibility, security invariant, native/recorded assertions,
source spans, a limited finding and remaining proof gap. The 19 paths are under
`crates/keld-runtime/src/macos_guardian/`:

- `guarded_lease/tests/native_activity.rs` — real child activity and external
  process-image observation; the observer subprocess has no independent deadline
- `tests/guardian_crash_pre_registration/retained_ack.rs` — original-child and
  group birth ownership; combined Host+Guardian death remains unproven
- `tests/guardian_crash_pre_registration/retained_ack/observers.rs` — real PID
  and image observations; late successful native marker can bypass the old clock
- `tests/liveness_and_shutdown.rs` — shutdown, death and rescue tests; test
  cleanup worker join may remain blocked after a failed rescue
- `tests/liveness_and_shutdown/bootstrap_admission.rs` — bootstrap admission
  and timed receipt; panic-path worker ownership needs a separate falsifier
- `tests/liveness_and_shutdown/cleanup_authority.rs` — attested owned native
  cleanup; Child wait may block when kill fails
- `tests/preparation_fixture.rs` — typed preparation fixture; an unbounded
  receiver gate can stall failed tests
- `tests/process_group_failsafe/cleanup_authority.rs` — wrong-birth denial;
  numeric birth mismatch is not forced real PID reuse
- `tests/process_group_failsafe/native_descendant.rs` — native descendant
  observation; successful LF can arrive after the original budget
- `tests/recovery_and_revocation.rs` — KGC1 recovery, control EOF and revocation
  order; its in-process schedules are narrower than all OS interleavings
- `tests/rejected_registration_deadline/cleanup_authority.rs` — altered-birth
  denial; helper Child wait is still unbounded on failed kill
- `tests/rejected_registration_deadline/witness.rs` — accepted control event
  and LF; historical successful accept/read could bypass original deadline
- `tests/runtime_race_validation/control_plane.rs` — test-only Unix control
  worker and shutdown; two joins lack a separate absolute completion bound
- `tests/runtime_race_validation/delivery_assertions.rs` — recorded delivery
  order checks; native authenticity depends on the relay actually reading Unix
  records
- `tests/runtime_race_validation/delivery_cut_oracles.rs` — four delivery cuts;
  zero/late successful native availability observation needs a postcheck
- `tests/runtime_race_validation/delivery_fixture.rs` — original G1/G2 fixture,
  native group and Q/KQA signals; unconditional collection joins may hang
- `tests/runtime_race_validation/private_record_relay.rs` — private relay and
  command receipts; no kernel ownership or bounded syscall promise
- `tests/runtime_race_validation/records_and_cut_types.rs` — relay record
  collection; a successful final read bypasses the original deadline check
- `tests/test_support.rs` — shared native gates and process-image helpers;
  accepted-event/read clocks restart and external /bin/ps is not independently
  bounded

These are source-level findings. Several identify **test-only** clock or cleanup
gaps that need separate real-native, failure-first regressions. They do not prove
that the production Guardian has the same defect.

## Ledger accounting and remaining limits

The derived `ledger_77.csv` preserves the inherited 77 historical paths,
all 27 parent columns and their values, and appends only the three
`batch19_*` fields. Only these 19 rows become
`source-reviewed-limited-batch19`. The historical
`pending-source-review` count falls **19 → 0**. This means the historical
status inventory has no unlabeled pending row; it does **not** mean all 77
files received complete semantic review. Prior LIMITED and historical-scope-only
classifications remain as they were.

Three source paths have different blobs since #949 and remain unreviewed as
version overlays: `authenticated_runner.rs`, `process_group.rs`, and
`tests/process_group_failsafe.rs`. One extra #952-only source,
`authenticated_runner/tests.rs`, is outside the original 77-file census.
Neither the changed source versions nor that new path is cleared by this PR.
No kernel-pinned original generation custody, simultaneous Host+Guardian death,
detached descendants, signed/installed app, full macOS integration, or complete
KELD #824 architecture result is certified. No native suite was rerun for
this docs-only branch, and independent review of the final commit is pending.

## Reproduce bounded structural checks

Run from this child worktree root with the pinned original Git objects available:

```sh
python3 -B docs/audits/macos-guardian-2026-10-19-54ac81e5-sevenfile/verify_seven.py
python3 -B docs/audits/macos-guardian-2026-10-19-86933528-final19/verify_final19.py
python3 -B docs/audits/macos-guardian-2026-10-19-86933528-final19/test_verify_final19.py
python3 -B docs/audits/verify.py
python3 -B docs/audits/test_verify.py
python3 -B tools/ci_inputs.py --check
just hygiene
git diff --check
```

The final19 verifier asserts immutable parent documents, the exact 77-row and
parent-column schema, 19 unique previous-pending reviews, matching #949 and #952
Git blob IDs, all 66 literal source citation intervals against those original
Git bytes, receipt/status joins, honest counts and unchanged completion flags.
Its **34 tests** include one positive and mutations covering all **66**
individually shortened intervals, each unresolved changed-source overlay,
historical statuses, source identities, counts and forbidden security/merge
claims. The checker does not validate the correctness of every prose claim or
every native test oracle.

The canonical `docs/audits/verify.py` now invokes the existing final19 verifier
and fails if it fails or its script or immutable Git evidence is missing.
`docs/audits/test_verify.py` runs the dedicated final19 negative controls and
canonical-process regressions for disposition/span corruption and missing
evidence. The existing hosted audit job and local audit recipe already invoke
these canonical entrypoints; no separate job is needed. This is structural
validation only, not native execution or semantic certification.

Earlier explicit local execution receipts remain historical evidence for their
original commits. Earlier hosted green statuses did **not** execute these nested
checks and are not retroactively upgraded by this integration. The immutable
#990/#949/#952 pins and original receipt hashes remain unchanged. The PR must
remain **OPEN / DRAFT / UNMERGED** pending independent exact-tip source review
and the remaining #824 work.
