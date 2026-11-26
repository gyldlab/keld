# GH-451 W2 retained-link contract fixture (review only)

Base: PR #1581 `db74c84673d912c720320470e04f84468e760589`.
Reference only: W7 appendix at `6c13ce03346835982c8a70eda4390881e4b38b0c`,
canonical draft AC-11/12 and §4.2 in the local W3 report, Core prototype
`2c02443271a3b36d8cfbd4edaacfb20180f2617e` (not merged into this tree).
#1432 `eb7a18b42ef7e8213a280ec9c76015bf78a0964f` remains draft.
These files are not included in the public API package or wired into runtime.

## Observed existing owners

- Rust `crates/keld-ipc/src/lifecycle.rs`: legacy request Quit=0, Subscribe=1,
  Init=2, existing response and event tags; golden tests pin the bytes.
- `packages/@keld/api/src/link.ts` LifecycleLink.quit marks closed before
  invoking quitAndCloseLink; `packages/@keld/kipc/src/transport.ts:3928`
  closes in finally even after a typed refusal.
- WorkerLink.call (transport.ts:3215) allocates the real correlation in its
  own allocator and returns only Promise<Uint8Array>. On deadline it drains,
  removes the pending entry, posts abandon and rejects with the original
  error. The allocator at 3388 wraps to 1 and excludes current blocking and
  pending correlations, not an explicit unresolved conditional attempt.
- Therefore the real conditional owner needs a private transport receipt
  for the original correlation and Promise, allocated/reserved before write
  with no unresolved-correlation reuse. Existing call() is insufficient for
  ordered Status. Preserve its ordinary and legacy behavior.
- Status must be ordered in the existing Host reader, query the exact old
  operation/correlation/token on this authenticated generation, and use its
  own fresh transport correlation. A missing record is not proof of
  nonexecution before that original operation cannot execute. The Host
  retains the bounded disposition; the client cannot manufacture it.
- Use the existing current LifecycleLink/WorkerLink and trusted launch/Ready
  qualification. No package-version or unknown-tag capability probe, no
  additional link, no forceful Quit fallback, no public export changes.

## Atoms and test oracles

| Atom / owner | Input and output | Falsifier and independence |
| --- | --- | --- |
| Negotiation / trusted link admission | Immutable selected+negotiated facts, current/live role -> zero sends or eligible | Flip eligibility; old peer must observe zero writes regardless of token/result. |
| Disposition / existing private API | Exact decoded correlated reply -> prepared, retained, cancelled, accepted, or unknown | Close on Retained; same port probe and subsequent Prepare fail; unrelated to Host registry authority. |
| Operation identity / WorkerLink | Exact original allocator correlation, operation, opaque peer token -> one outstanding attempt | Reset unresolved latch; duplicate effect observed. Original and Status identities must differ. |
| Reconciliation / existing Host reader plus API | Ordered exact recorded result -> same disposition, otherwise unknown | Ignore original identity or alias correlation; test must fail even when result shape is valid. |
| Generation / current link owner | Current original at issue and delivery -> no retired role authorizes WillQuit | Retire before response; Prepared must not be delivered. This does not validate actual Host generation admission. |
| Acceptance / existing terminal link owner | Exact Accepted -> fence once; acceptance is not OS cleanup | Assert no fence before reply, no fence for other dispositions. Generic errors are not proof of Retained. |

The fixture uses an abstract scripted peer with no socket, Host registry,
codec, runtime launch or real window. It isolates client-side contract
behavior, not authority, actual call ordering, FS drain or physical proof.
Its illustrative strings and error vocabulary allocate no protocol values.
The fixture conservatively leaves unapproved alternative Cancel dispositions
and delivery after role retirement unknown. A known Accepted for the
still-current original may precede EOF, but never means clean OS exit.
An unexpected synchronous start failure lacking an original correlation
blocks new effects and cannot be reconciled by guessing. Actual same-link
Status support and the bounded disposition owner are approval/integration
blockers. The fixture detects a reused correlation at receipt processing;
the real allocator must prohibit the alias before sending anything.
The production owner must also preserve captured intrinsic/error behavior,
define accepted-fence failure custody and verify actual link and reader
ordering; no cleanup success is inferred by this model.

## Reproduction

Run from this managed worktree, with the existing Bun 1.4.4 binary:

```sh
/Users/centillionaire/.bun/bin/bun test packages/@keld/api/fixtures/gh451-retained-link-draft/candidate.test.ts
python3 -B packages/@keld/api/fixtures/gh451-retained-link-draft/negative_controls.py /Users/centillionaire/.bun/bin/bun
git diff --exit-code HEAD -- crates/keld-ipc packages/@keld/kipc packages/@keld/api/src packages/@keld/electron/src crates/keld-core
```

Observed final: 23/23 tests, 211 assertions; 7/7 single-fault controls
detected (exit 1 at the named test; overall harness exit 0). The first
mutation run targeted a null case that returns earlier than the mutated
generic-error branch; corrected the test selector to the actual affected
generic-error case, then reran all controls and restored baseline.
Tracked production path diff empty. No new dependencies, wire tags, policy,
production enablement, external publication, installed execution, full Cargo
build, full CI or TypeScript typecheck (compiler unavailable on PATH).
The files remain local, uncommitted review artifacts in this managed tree.

SHA-256:
- candidate.ts: ab76cffe6d7f1404ac4e6f77c79cd82016b0d0718c58954d8311491a413e4cce
- candidate.test.ts: e6c1ed146b1126967fbbb07c1bc9dbbef718b291009c7882ed4c4bce5eb992bf
- negative_controls.py: f258832d44e0ec567b1facab10e74dcfc9eb983b127da4e43d6473a99a67dec3
