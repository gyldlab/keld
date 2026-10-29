# Guardian observer successor review — 29 October 2026

## Scope and immutable source

This dated source supplement advances only two test-observer bodies from the
[28 October review](source-review-2026-10-28.md). It does not rewrite that report,
its CSV, the original CSV/JSON receipts or their historical hashes. The new
[source ledger](source-delta-2026-10-29.csv) binds the same four Guardian roots to
source-only commit `066bd64f53e347a55dd98e3e76c1ecc01569ec86`, parent
`89b21d89aa94de9aad704bdbc2045d48b15fa7b2`. The source commit and later census
integration are separate so the evidence does not contain a circular self-pin.
Neither an unattached source commit nor a source review is a publication/merge receipt.

All 86 paths remain: 18 production and 68 test paths. Relative to the 28 October
scoped source, 84 blobs are unchanged and exactly two test blobs differ; no path
was added/deleted and no production blob changed. Those 84 paths retain their
existing source-review provenance and qualifications, not a repeated review or
renewed acceptance. The two complete successor bodies were statically read over
the ranges below. No whole-repository or current-main audit was repeated.

| Test observer | 28 October blob | Successor blob | Static range |
|---|---|---|---|
| `tests/bootstrap_shutdown_race.rs` | `e47a67b15559b810845f9f733ff21b9c51c42f54` | `bb5218996289f567a76cc8ca2cfffb29e20f25b1` | 1–431 |
| `tests/test_support.rs` | `9b08ddbe2eb0e8024b27f8baad88b4dabe1936d1` | `75de5bb19e7e9b38dfc5a1384ee2197bc0eee95e` | 1–338 |

Paths in this table are relative to `crates/keld-runtime/src/macos_guardian/`.
The ledger's `current953_blob` remains the historical observed #953
`8dd43926297479851c555596aafa012ee992c7e5`; it does **not** claim that #953 advanced.
`baseline949`, `historical953`, original-review fields and their source pins stay
unchanged. Only `current955_blob` advances to the separately pinned successor.
Relative to historical #953 `46b91848`, the aggregate stays 50 unchanged,
29 modified and 7 added; relative to #949, 48 unchanged, 29 modified and 9 added.
These counts are provenance, not evidence of semantic or native acceptance.

## Held milestone observer

`receive_held_milestone` (164–185) retains the original deadline created before
acceptance and passes that same value to `read_held_milestone` (189–221). The
extracted actual socket reader checks strict `now() < deadline` after receiving
LF and before breaking. It preserves non-LF bounds, partial/error handling,
marker/PID parsing and the real `Instant::now` call at the existing caller.
The surrounding authenticated shutdown/Bun/Child custody paths are unchanged;
this test-only observation change does not change product framing or authority.

The new source-defined case (223–266) queues a literal UnixStream receipt and
selects an injected clock just before, exactly at and just after the deadline.
It checks the parsed PID on success or exact timeout panic on rejection, then
checks the receiver has no unread byte. The final check prevents an earlier
partial-record timeout from masquerading as final-LF refusal. These are actual
Rust test definitions using the observer, **not executed test receipts**.

## Spawned receipt observer

`await_spawned_gate` (153–176) retains its 10-second accept budget and independently
created 5-second record budget. `read_spawned_receipt` (180–224) retains the bounded
one-byte reader and parsing, with the same strict post-LF deadline test before
success. The seam introduces no new product clock or mutable global state.
The new source-defined case (226–282) similarly tests before/at/after completion
with literal socket bytes, inert PID fields, exact parsed result/timeout and an
empty remaining receive queue. Existing selectors/callers and cleanup ownership
are not replaced by these two observer tests.

The unchanged `join_before` (34–46) can still detach a running worker on timeout;
`NativeChildOwner::drop` (83–94) still ignores kill failure and synchronously waits.
Those prior custody qualifications remain. Neither extraction nor the additional
clock boundary tests establish cancellation/join, descendant custody or bounded
cleanup under ineffective kill.

## Canonical enforcement and evidence limits

The existing canonical verifier selects this new dated ledger and immutable
successor source. #953 and #955 are no longer required to be identical: each
historical column is checked against its own unchanged pin, while the successor
path set, Git index and CRLF-normalized checkout must match the new #955 pin.
Missing/stale ledger, source/path drift and invented full/native acceptance must
still fail. Existing live Guardian `input_docs` admission remains unchanged;
only affected canonical reader fingerprints are rebound by the existing generator.
There is no new workflow, framework, permission or test-gate bypass.

Both changed rows retain `unresolved-final-newline-deadline-oracle-gap;
no-security-closure` and `native_execution=NOT RUN`. The source-level guard is
prepared, but compiled discovery, actual Rust failure-first/restored execution,
rustfmt, Clippy and real macOS/native oracle acceptance were **NOT RUN** for this
supplement. In particular, removing the new LF assertion must be shown to make
the actual Rust boundary case fail; a Python source-shape check is not that proof.
Python/Git verifier regressions establish census and qualified-label integrity
only. Exact command results and final-head hosted outcomes belong to separate
publication receipts, not to the static source pin or this prose.

The prior final-newline review request remains open for executable evidence;
#834/#824 custody, installed-product, dual-death, detached-descendant and complete
whole-repository acceptance remain independent owner gates. No thread resolution,
security closure, merge readiness or inherited native proof is asserted.
