# Windows #824 source-review ledger — integration witness, 18 October 2026

**Status: partial, not release acceptance and not approval to merge.** This is a
SHA-pinned Windows-specific source-review queue, not the A–F full-repository
ledger required by #824. It carries forward only claims independently reviewed
against the exact source Git blob; a passing CI job or existing PR never creates
an automatic semantic-review disposition.

## Immutable inputs and resulting counts

- Source: [DRAFT #937](https://github.com/gyldlab/keld/pull/937), exact commit
  `b9b3c10ac9054e4c4baba1b28c6026c65b38944c`, itself an immutable
  Windows stack integration witness.
- Original [DRAFT #891](https://github.com/gyldlab/keld/pull/891) queue:
  **307** Windows-related candidates from `ffdba0e907b366d2552379d9f0fbf60727dbbf3a`.
  Its historic CSV is kept intact, SHA-256
  `e72afe22b9abbf4a8745a1add0d3a92eb1464a05c56a24994b813d383285b978`.
- Integration-tip queue: **322** candidates: 111 Rust production/fixture,
  154 tests, 33 support and 24 CI/tooling. The path set has **15 additions,
  zero removals** since the original queue, with **15 existing candidate
  Git blobs updated**. New candidate paths belong to IPC lifecycle transcript,
  named-pipe policy, Job lifecycle/native test, LSA observation, updater
  locator/stage/writer native tests, signed-installed acceptance helpers and
  Rust/Just build-cache tooling. A changed blob is *not* automatically reviewed.
- Exact-blob supported semantic dispositions after the additional **19 October**
  independent read-only source review: **seven justified no-change** paths and
  **two conditional risks**. **313 candidates await independent semantic
  review; 315 remain not closed** (313 pending plus two conditional risks).
  Only three exact historical blobs were newly reviewed; no claim that every
  Windows file, a shipping privilege path or the whole program is complete.

## Exact source-review evidence and two bounded conditional risks

The original **six** attestations from the earlier
[#824 per-file review receipt](https://github.com/gyldlab/keld/issues/824#issuecomment-6248828406)
remain byte-identical; a separate, independently authored
[19 October pinned-blob review](https://github.com/gyldlab/keld/issues/824#issuecomment-6259406772)
adds exactly **three** reviews at the same #937 source SHA. All nine rows
carry exact Git blob OIDs, public reviewer citations, responsible owners and
independent failure-first falsifiers in
`windows-2026-10-18-reviewed-blobs.csv`. The remaining 313 paths stay
pending. Those three new entries are `windows_machine/initiating_token.rs`
(conditional release-integration risk), `windows_job/process_peer.rs`
(source-reviewed no-change) and `windows_extraction/lpac_probe.rs`
(test-fixture-only no-change). Other newer-source reviews did not have the
same historical blob ID and are **not** implicitly promoted.

`crates/keld-runtime/src/windows_lpac.rs` at blob
`63e2df931b576bddf7daf4464f897b7e0077f023`, lines 257–336:
`WindowsLpacProfile::grant_path` reads and publishes an access-control list
(ACL) by pathname, without retaining the exact object between those calls.
This is **conditional**, not a proven shipping exploit. The current in-tree
callers are Windows native tests and an extraction test probe. The next
independent falsifier uses two test-owned directories, swaps one pathname
between the ACL read and write, and observes the unrelated object's DACL.
A production correction belongs to a separate scoped owner only if native
reproduction and a real shipping caller warrant it.

`crates/keld-guard/src/windows_machine/initiating_token.rs`, exact blob
`4d121eff33e67eaed21fde9f65f7f29bba3b313b`, lines 51–63 and
89–249, has no proven unsafe rights-expansion in its isolated token owner:
it rejects elevated/non-Medium token facts, opens from an exact host-process
HANDLE, duplicates narrowly scoped token handles, and aborts if reverting
thread impersonation fails. However its public initiating-token admission
and duplication APIs are only called in that module's own `#[cfg(test)]`
selectors at this pinned source. The Machine-UAC S11 signed helper bootstrap
and positive elevated/ordinary-user installed acceptance remain **unwired
and unproven**. This is a conditional **integration risk, not an observed
shipping exploit or a justification to bypass the existing default-deny gate**.

`crates/keld-runtime/src/windows_job/process_peer.rs` (blob
`7531c6d93f911406ea5d4e4a057ae03d91f2b373`) retains the exact
OS process HANDLE for connected pipe PID/session/image/creation-time
observation and finite waits; callers must independently authenticate the
executable and lifecycle challenge. `crates/keld-update/src/windows_extraction/lpac_probe.rs`
(blob `2061a82e6581ca70a721c518649811908f7d61b1`) is test-only and
probes real LPAC stage mutation denial against positive RolePrivate writes.
Neither source-only review establishes signed-installed production acceptance.
The new reviewer **inspected** these native selector contracts but did not
claim that those tests were rerun in this docs-only review.

## Existing integration evidence, not new claims

The pinned integration head in [#937](https://github.com/gyldlab/keld/pull/937)
previously passed **927/927 Windows native runnable tests** with 51 configured
skips, and [hosted CI run 39078846874](https://github.com/gyldlab/keld/actions/runs/39078846874)
passed **22/22 jobs**. The independent `gatekeeper` PR-template failure was
subsequently corrected through PR-body metadata; a later gatekeeper check
succeeded. This ledger update does **not** rerun 927 tests and makes no
cross-platform process-custody or installed-binary acceptance guarantee.

## Reproduce and falsify ledger claims

Run at the pinned draft child checkout with both the prior and new source Git
objects available:

- `python -B tools/test_windows_architecture_closeout.py` — verifies rejection
  of stale/duplicate/unknown reviewer claims, forged statuses, missing public
  evidence and unowned/missing falsifiers.
- `python -B tools/windows_architecture_closeout.py --check` — rejects snapshot
  count, output-byte, historical-reference, evidence-hash and source-blob drift.
- `python -B tools/ci_inputs.py --check`, plus repository hygiene and hosted CI
  under the existing `just`/workflow contracts.

`tools/windows_architecture_closeout.py` extends, **without modifying**, the
existing reviewed queue generator `tools/windows_architecture_ledger.py`. The
new checker runs only when explicitly invoked; it introduces no new CI reader
of runtime source into an existing router consumer. `tools/ci-inputs.json`
has only its policy-reader source digest rebound for the newly added Python
files; no reader or consumer input scope was weakened.

## Still blocking whole-program closeout

Full A–F authored-path and module-responsibility coverage in #824; all
independently justified Windows dispositions; owner-approved PR integration
and descendant restacks; hosted WebView2 original intermittent navigation
root-cause verification (#814/#869); signed-installed Windows,
Machine-UAC/AppContainer and true host/Job cleanup acceptance; macOS Guardian
detached descendants and Linux containment; cross-platform GUI acceptance.
No architecture PR or integration witness may be merged, squashed, retargeted,
or auto-merged without explicit owner approval.
