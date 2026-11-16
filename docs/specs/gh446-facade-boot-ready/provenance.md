# Delegated decisions and remaining evidence
Part of: [index](index.md)
Owns: Delegated decisions, approval provenance, historical decisions and unrun evidence.

## 10. Open questions, blockers & unknowns

**Owner choice resolved on 2026-10-23.** In the active user session, the repository
owner explicitly delegated decisions and requested publication in the appropriate
repository and issue trackers under the agent instructions. ChatGPT made the scoped
decision on that authority and posted the
[owning #446 claim](https://github.com/gyldlab/keld/issues/446#issuecomment-6370495240)
through the authenticated `0monish` account. The connector account is the publisher;
this record does not assert that the human personally inspected the revised bytes.

The decision adopts §4.10's exact recovery qualification: preserve schema-1 native
and document continuity; preserve every schema-2 survivor's native identity; preserve
its document until recovered application code explicitly navigates it. Retain the
existing authority, generation, filesystem-quiescence and native-custody guarantees.
This is a new KEL-139 amendment. It supersedes the previous proposal's claim that no
amendment was needed and resolves its parked owner-choice question.

**Historical exact-content decision; current split-head gate pending.** The
[delegated-approval record](https://github.com/gyldlab/keld/pull/657#issuecomment-6371083658)
approves technical head `a04e1ed889733a64cc33c1b64535e02b4afb0902` against base
`3503a8aa383427dcf50ccbf6653c4298e7fb8aad`. It binds the original single-file
GH-446 blob `2f53f2a3cf558c099c66ee886c935084f4e31440` and GH-531 blob
`7b0fdeaf98c847387ea47ffdef9158f64868685b`, their SHA-256 file hashes, and content
digest `3ebcbad966250005c6a8d4059405a8e519b02d93334fe9cbe817704ed434d4e5`.
That record is valid historical approval for that exact candidate only. It does not
approve this split directory or the amended GH-531 bytes at split head
`4ff6d72612f289eb7323bf7ec1e5c5b8c333b7eb`; the split parts and amended GH-531
document are different blobs. Therefore exact-content approval for the current split
head is **NOT SATISFIED**.

A separate independent review of the final split commit on PR #657 is required
before engineering relies on this specification. That review must bind the final
split commit and base, reviewer identity, review ID/source URL, review state,
submission time, and blob IDs for the index, each part, and GH-531. Record only
actual submitted review values; do not use the historical reviewers or refutations
below as current-head clearance. The T-1 implementation PR must also record its own
exact-diff review separately, with its own reviewer, source, state, time, commit,
base and changed-file blobs, as required by §4.10.

Historical independent reviewers `/root/ready_architecture_review` (lifecycle/API)
and `/root/ready_security_review` (authority/custody/wire order) cleared the
historical head named above. Their review and refutation records do not clear the
current split head. The original KEL-96 D-1 through KEL-96 D-11, KEL-139 and GH-531
approvals remain historical evidence and do not cover these new semantics. No
runtime acceptance is implied, and no PR may be merged under the current user
instruction.

**Remaining evidence, with owners.** #445/#446 own the compiling RED, lifecycle
ordering and native zero-window proof; #446 owns schema-1/security/recovery/custody
regressions; #449 owns accepted Subscribe/replay, constructor identity and adoption
controls; #450 retains ClosePending recovery. The macOS milestone latency distribution
and qualified target dependency evaluation need actual receipts. §6 states exactly
which evidence resolves the Ready blocker and which still belongs to BrowserWindow.
These are technical acceptance tasks, not a request for another owner decision.

Historical decisions recorded on 2026-10-08 (preserved as history):

- **Compat main-role loss.** GH-531 D-6 Option B holds for every boot kind.
  - The GH-531 registry owns the no-second-window rule, keyed on registry state, and
    the facade complies (§4.7; GH-531 D-6 scope, amended by #657).
  - The rule lands with #449 (GH-531 T-1), and `pending_close` with #450 (GH-531 T-2).
  - X06-D1(c) is superseded. An earlier KEL-143 attribution was wrong: KEL-143 is
    KEL-139 T-3, host continuity.
- **The issue's AC-1.** It is restated as AC-12, and no `app.ready.no-host-window` cell
  is added.
  - #656 records both cited cells as `pass`, so nothing flips.
  - gh566 admits only keld-compat libtest and `packages/**/*.test.ts` targets, and its
    D-8 rejected Rust oracle targets in other crates. That routing edge belongs to
    GH-531's AX-click real-Mac cells (GH-531 §4.i D-3), not to this spec.
