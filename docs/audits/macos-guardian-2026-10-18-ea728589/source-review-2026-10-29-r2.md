# Guardian observer source correction — 29 October 2026, revision 2

## Source pin and preserved history

This bounded correction succeeds the [29 October source review](source-review-2026-10-29.md)
and [ledger](source-delta-2026-10-29.csv), without modifying either or any earlier
report/CSV/JSON bytes. The [revision-2 ledger](source-delta-2026-10-29-r2.csv) binds
source-only commit `8679d875146772d638632bf0cfb50efba68d2379`, parent
`3d1b04f982f01b0472d9fe6c3b3cbcba363e7cbd`. A subsequent integration commit selects
that immutable source pin; no circular self-pin or intermediate passing-state claim
is used.

All four Guardian roots and 86 paths remain unchanged in membership. Exactly two
test blobs differ from the previous successor; 84 blobs, including all 18 production
paths, retain their prior qualified review provenance. No whole-audit rereview or
new native acceptance is implied. Historical #949/#953 pins, their blob columns,
original-review fields and baseline delta counts remain unchanged. The two changed
rows alone update their `current955_blob`, full-file read range and review reference.

| Test observer under `crates/keld-runtime/src/macos_guardian/` | Prior successor blob | Corrected blob | Full-file range |
|---|---|---|---|
| `tests/bootstrap_shutdown_race.rs` | `bb5218996289f567a76cc8ca2cfffb29e20f25b1` | `c5bbc0bf2fec0a9dd92035e62f610a27b14bea93` | 1–439 |
| `tests/test_support.rs` | `75de5bb19e7e9b38dfc5a1384ee2197bc0eee95e` | `f21bedff938e5149ff56d0441278b7ad54c67917` | 1–340 |

## Bounded correction review

[CI run 40852187744](https://github.com/gyldlab/keld/actions/runs/40852187744) on
`3d1b04f` reported formatting differences in the bootstrap test and
`clippy::unchecked_time_subtraction` in both new boundary cases. Mac test execution
was skipped after Clippy failure. That compilation/lint result is **not** the
requested late-LF oracle failure-first result.

Both cases now construct the preceding tick with
`deadline.checked_sub(Duration::from_nanos(1)).expect("test deadline has a preceding tick")`
(using the existing qualified Duration path in the bootstrap case). Bootstrap's
three-element test array and sender call chain also receive the reported formatting
correction. Static comparison reverses these four edits to the exact prior bodies;
the remainder of both previously read bodies is unchanged. Their new full-file
ranges above include those small substitutions, not a claim of new runtime evidence.

The actual observer LF guards, strict `<`, original accept/record budgets, injected
clock calls, literal socket bytes, result/panic checks and drained-receiver check
are unchanged. Product code, parser/size/error handling, process custody and existing
selector names are unchanged. The prior join/kill/wait and owner qualifications
remain, including ineffective-kill and detached-worker limitations.

## Canonical evidence and outstanding execution

The canonical verifier and its fixtures select this revision's CSV and source pin.
The same historical-pin, live-index/checkout, CRLF, qualified-label and path-membership
checks remain. Substituting the previous published ledger must fail against the new
pin. Only actual affected policy/audit fingerprints are rebound through the existing
CI-input generator; live Guardian admission, other reader sets and Rust routing are
unchanged. Historical validation receipts are not inherited as this revision's result.

Both rows still say `unresolved-final-newline-deadline-oracle-gap; no-security-closure`
and `native_execution=NOT RUN`. Local Rust/rustfmt/Clippy, compiled discovery and
native/mutant execution remain **NOT RUN** for this correction. The next exact-head
hosted run must establish formatting and warning-denied compilation, then actual LF
case execution. The missing-LF-guard mutant still needs executed rejection; a source
comparison or Python provenance pass does not supply that proof.

No final-newline review-thread resolution, security closure, installed-product,
#834/#824 custody, whole-repository acceptance or merge readiness is asserted.
