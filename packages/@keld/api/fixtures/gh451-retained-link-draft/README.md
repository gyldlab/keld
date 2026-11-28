# GH-451 W2 retained-link contract fixture (review only)

Original base: PR #1581 `db74c84673d912c720320470e04f84468e760589`.
Q3 revision base: local consolidated prototype `24187ab15e6294493bf8790e35092d214b109ee8`.
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
An unexpected synchronous send failure retains the pre-reserved original
identity. Actual same-link Status support and the bounded Host disposition
owner are approval/integration blockers. The Q3 allocator model prevents
alias before send; it does not implement or prove the real WorkerLink allocator.
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

Historical initial W2 receipt: 23/23 tests, 211 assertions; 7/7 single-fault controls
detected (exit 1 at the named test; overall harness exit 0). The first
mutation run targeted a null case that returns earlier than the mutated
generic-error branch; corrected the test selector to the actual affected
generic-error case, then reran all controls and restored baseline.
Tracked production path diff empty. No new dependencies, wire tags, policy,
production enablement, external publication, installed execution, full Cargo
build, full CI or TypeScript typecheck (compiler unavailable on PATH).
The original four files were preserved byte-identically in owned scratch
`.keld-work/reports/gh451-w2-pre-24187ab15-20261126` before the fast-forward
to the exact consolidated base. The current Q3 delta is isolated here.

Historical SHA-256:
- candidate.ts: ab76cffe6d7f1404ac4e6f77c79cd82016b0d0718c58954d8311491a413e4cce
- candidate.test.ts: e6c1ed146b1126967fbbb07c1bc9dbbef718b291009c7882ed4c4bce5eb992bf
- negative_controls.py: f258832d44e0ec567b1facab10e74dcfc9eb983b127da4e43d6473a99a67dec3

## Q3 retained identity and recovery handoff

`reservation.ts` models one link-local allocator shared by ordinary calls,
one active attempt, one unresolved effect and one Status reservation. It
reserves before invoking the scripted send; no timeout or capacity-driven
eviction is allowed. A numeric identifier may wrap only after both exact
receipt consumption and independent transport evidence that no late reply
can alias that identity. A stale lease object cannot release a recycled
numeric slot. A lost Status consumes its local result but retains its
separate reservation until independently proven quiescent; repeated Status
while it remains reserved sends nothing.

W3/prime composition contract:

- Construct the trusted port with one `DraftCorrelationLedger`, captured
  selected/negotiated facts, current/live observations and
  `send(lease, request): Promise<unknown>`. Use the already-reserved
  `lease.correlation` and do not allocate again in send. The transport
  independently calls `ledger.proveNoLate(lease)` only on valid quiescence
  evidence. A rejected Promise by itself supplies none.
- `conditionalAvailable` is preflight for a **new** attempt; it is not a
  condition for `reconcile()` on an existing indeterminate attempt.
- `prepare()/cancel()/commit()/reconcile()` return typed outcomes.
  Unknown always contains the original identity, even on synchronous
  send failure. Direct Prepare cannot return `invalidated`; reject it as
  Unknown with the exact original identity. Only matching Status can report
  a revoked Prepare as Invalidated; it never grants Prepared authority.
  `competing-terminal` is distinct from Retained and Accepted.
- After applying a resolved disposition, W3 reads the exact
  `pendingReceipt` object (original, disposition, direct/status source)
  and calls `consumeReceipt(receipt)`. A false result leaves the
  reservation held until independent no-late proof, or definitive old-link
  retirement for a known terminal outcome. Never clone a receipt
  or interpret delayed release as nonexecution. Reading `pendingReceipt`
  synchronizes a release that the transport subsequently proved.
- Known Accepted fences the captured current link once, before any
  receipt consumption; a fence exception remains available as
  `fenceError` and never changes `terminalDisposition` to unknown.
  Historical Accepted on a retired link remains evidence and does not
  fence a successor. Cleanup is independent of receipt consumption.
  Exact definitive old-link retirement ends terminal effect lease custody
  without any receipt acknowledgement. A delivered result or local fence
  is not retirement evidence. After release, `historicalTerminalReceipt`
  keeps one immutable original identity, source, and terminal result for
  exact historical delivery. It is not a consumable receipt, cannot reopen
  the link, and does not let a late result fence the successor.
- A pre-send Cancel or Commit capacity refusal after Prepared keeps that
  attempt blocked even if ordinary capacity later returns. No original
  identity exists for the unsent operation, so no Status or effect is sent.
  Recovery of a surviving preparation remains outside the fixture; an
  initial Prepare capacity refusal still leaves no attempt and can be retried.
- The original link/generation owns the ledger. This fixture does not
  implement the Host's ordered disposition record, actual transport
  quiescence proof, a wire receipt-consumption acknowledgement, automatic
  retirement, or an approved nonprogress deadline. Those require named
  owners and real tests. The fixture's local lease identity is not a
  Host-minted wire attempt/token.

Historical Q3 focused validation: 32/32 Bun tests, 708 assertions; 15/15 named
single-fault negative controls detected, including pre-send reservation,
active-attempt exclusivity, exact receipt consumption, late-alias release,
capacity, Status identity and fence failure. No dependencies installed.
The prior composed facade fixture still uses the older port interface;
prime owns its adapter update. W2 does not change facade/Core/production
paths. No full workspace CI, typecheck or authenticated/native proof
is claimed.
