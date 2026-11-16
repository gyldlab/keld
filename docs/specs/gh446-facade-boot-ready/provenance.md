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

**Exact-content gate satisfied for specification approval.** The authoritative
[delegated-approval record](https://github.com/gyldlab/keld/pull/657#issuecomment-6371083658)
approves technical head `a04e1ed889733a64cc33c1b64535e02b4afb0902` against base
`3503a8aa383427dcf50ccbf6653c4298e7fb8aad`. It binds this specification's blob
`2f53f2a3cf558c099c66ee886c935084f4e31440` and GH-531's blob
`7b0fdeaf98c847387ea47ffdef9158f64868685b`, their SHA-256 file hashes, and content
digest `3ebcbad966250005c6a8d4059405a8e519b02d93334fe9cbe817704ed434d4e5`.
Independent reviewers `/root/ready_architecture_review` (lifecycle/API) and
`/root/ready_security_review` (authority/custody/wire order) cleared that exact head.
The record includes independent refutations and all three corrected findings.
This following commit changes approval metadata only; it does not alter that
technical content. The original KEL-96 D-1 through KEL-96 D-11, KEL-139 and GH-531 approvals remain
historical evidence and do not cover these new semantics. No runtime acceptance is
implied, and no PR may be merged under the current user instruction.

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
