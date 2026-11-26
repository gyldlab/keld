# Design

Part of: [index](index.md)
Owns: section 4

## 4. Design

### D-1: Keep bounded capture in one private module

**FACT:** CapturedOutput owns retained head and tail storage, raw-byte counters, elision and UTF-8/line-boundary presentation. Its public ceiling is 327680 bytes. The current split is 64 KiB head, 192 KiB tail and 64 KiB compaction slack.

- **Owner:** keld-runtime.
- **Boundary:** CapturedOutput, its five public CAPTURE constants and ten pure tests.
- **Input:** Raw output chunks from the existing capture workers.
- **Output:** A bounded transcript and byte-accurate accounting.
- **Failure:** Any changed limit, cut rule, counter, marker or lock order changes runtime behavior.
- **Observable:** The root re-export and all five public constants retain their names and values. Only the ten pure test selectors gain the captured_output::tests prefix.
- **Negative control:** Remove the compaction guard or change the literal ceiling. The existing bounded-retention test must fail.

**INFERRED:** Keep CaptureState, stream-marker observation, reader threads, session counters, process helpers and sanctioned unsafe code in their current owners. The module move must not create another state machine or facade.

The only parent-visible mutation method remains push_chunk with pub(super) visibility. This does not expose a downstream mutation API.

### D-2: Keep research root and lock in the workspace owner

**FACT:** The public Just wrappers call workspace.py reference-run. The private recipes call workspace.py reference-root before they invoke the helper.

- **Owner:** tools/workspace.py retains reference-root and reference-run. The helper owns only the two Git operations.
- **Boundary:** Sync and push inside the nested docs/research checkout.
- **Input:** The root resolved by the existing workspace owner. Push also receives the literal commit message.
- **Output:** Existing sync or push diagnostics, exit status and branch behavior.
- **Failure:** A missing or foreign checkout must not modify the Keld parent or overwrite an existing non-repository leaf.
- **Observable:** Local fixtures prove nested-root isolation, dirty push, clean push, wrong or missing origin, non-repository destination and failed Git operations.
- **Negative control:** Move root resolution or locking into the helper. The ownership test must detect a second reference owner.

**INFERRED:** Keep the two public Just entrypoints and the reference-run lock unchanged. Do not duplicate workspace records, Python selection, cleanup or root resolution.

### D-3: Keep one exact research-origin predicate

**FACT:** The helper admits the three current GitHub URL spellings. It normalizes an optional .git suffix. It checks both fetch and push URLs. It pins push to an explicit branch ref.

- **Owner:** tools/research_checkout.sh.
- **Boundary:** Origin admission, sync and push only.
- **Input:** The existing checkout, resolved root, current branch and optional push message.
- **Output:** An ff-only sync or one ordinary unforced push to origin and the same branch.
- **Failure:** A foreign URL, credential-bearing URL, pushurl mismatch, detached HEAD, non-branch symbolic HEAD or late non-fast-forward update must fail safely.
- **Observable:** The local bare-repository fixture records one explicit branch push and no Keld-parent changes.
- **Negative control:** Allow push defaults or rewrite a full branch ref. The branch/tag collision test must detect a wrong destination.

Preserve the current distinction between hook-safe sync failures and push integrity failures. Sync may warn and return zero when access is unavailable. Push integrity failures return nonzero.

### D-4: Keep the CI input router stateless

**FACT:** tools/ci_inputs.py uses schema v2 reader patterns and consumer input edges. It stores no reader-content digest. Unknown inputs fail closed.

- **Owner:** tools/ci_inputs.py and tools/ci-inputs.json.
- **Boundary:** Reader-pattern matching and consumer expansion.
- **Input:** Changed paths from the current base/head comparison.
- **Output:** The conservative lane and package selection.
- **Failure:** An unknown or incomplete input must not publish a skipped-green plan.
- **Observable:** just ci-route reports the plan. just ci-router-test tests the route. just ci runs the routed inventory.
- **Negative control:** Remove a cross-tree input edge or turn an unknown path into a skipped lane. The router contract must fail.

A reader-set change expands every consumer that uses that reader. Add a new cross-tree read to consumers[].inputs in the same change. Do not add a content digest or rebind step.

### D-5: Preserve platform and performance boundaries

**FACT:** The capture implementation is shared across macOS, Windows and Linux. Shell behavior differs by platform. Windows uses Git Bash. macOS uses stock Bash 3.2. Linux requires its own run.

- **Owner:** The platform test owner for each operating system.
- **Boundary:** Existing capture and research helper behavior.
- **Input:** The same source revision and the platform's actual shell and Git versions.
- **Output:** A separate receipt for each operating system.
- **Failure:** A pass on one OS cannot qualify another OS.
- **Observable:** Each receipt records the OS, source SHA, command and result.
- **Negative control:** Omit a required OS run and mark the matrix complete. The evidence review must reject that claim.

**FACT:** No speed or memory improvement is claimed. The capture algorithm and constants remain unchanged.

**INFERRED:** A module move is justified only by task-locality and source ownership. A cosmetic split without that improvement is rejected.
