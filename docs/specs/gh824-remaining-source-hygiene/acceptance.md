# Acceptance criteria

Part of: [index](index.md)
Owns: section 3

## 3. Acceptance criteria

### AC-1: Preserve the public capture API

- **Given:** Existing external callers import keld_runtime::CapturedOutput and the five keld_runtime::CAPTURE constants.
- **When:** The capture implementation moves into its private module.
- **Then:** All existing imports compile. The root re-exports the same public type and five constants. No new public module or private mutation API is exposed.
- **Negative control:**
  - **Single fault:** Remove one root re-export or make push_chunk public.
  - **At-risk precondition:** Existing downstream callers use the root API.
  - **Expected RED:** The external import probe fails or the API-surface check detects the new public mutation.

### AC-2: Preserve bounded-capture semantics

- **Given:** The original CapturedOutput algorithm and constants.
- **When:** The implementation moves to captured_output.rs.
- **Then:** Source reconstruction differs only by the private-module move, required parent visibility, explicit test imports and qualified rustdoc links. No byte bound, UTF-8 boundary, line boundary, counter, marker or locking logic changes.
- **Negative control:**
  - **Single fault:** Change the retention limit or one UTF-8/line-boundary rule.
  - **At-risk precondition:** Existing bounded-capture and source-reconstruction tests run.
  - **Expected RED:** The literal contract or source-equivalence check fails.

### AC-3: Preserve compiled test discovery

- **Given:** The compiled runtime test inventory before the move.
- **When:** The ten pure capture tests move beside the private module.
- **Then:** The total inventory and ignored states remain equal. Only these ten selectors gain the captured_output::tests:: prefix:
  - capture_max_retained_bytes_is_the_documented_host_ceiling
  - bounded_capture_keeps_a_fixed_tail_and_counts_every_byte
  - capture_late_newline_cannot_defeat_the_retention_ceiling
  - elision_never_fabricates_a_line
  - newline_free_output_is_separated_rather_than_spliced
  - carriage_return_only_output_is_separated_rather_than_spliced
  - one_enormous_line_still_respects_the_ceiling
  - totals_count_raw_bytes_not_lossy_decoded_length
  - bounded_capture_pins_the_head_against_a_later_flood
  - bounded_capture_cuts_on_a_utf8_character_boundary
  Every other selector stays unchanged. Reconcile every referenced selector and test-reader fingerprint atomically.
- **Negative control:**
  - **Single fault:** Leave an old selector or change any unrelated selector.
  - **At-risk precondition:** The before/after compiled inventory is captured.
  - **Expected RED:** The selector map is not bijective or the total/ignored-state comparison fails.

### AC-4: Prove the retention ceiling and native behavior

- **Given:** The documented 327680-byte ceiling and existing capture, UTF-8 and truncation cases.
- **When:** The moved module and affected runtime are tested.
- **Then:** The literal ceiling test passes before and after the move. The ten pure tests pass. A deliberate removal of a retention guard makes a named regression fail. Restore the guard and the regression passes. Full affected native supervisor tests and warning-denied lint remain required.
- **Negative control:**
  - **Single fault:** Remove the compaction guard or raise the public ceiling.
  - **At-risk precondition:** The flood, huge-line and late-newline cases execute.
  - **Expected RED:** A retained stream exceeds the ceiling or the literal ceiling assertion fails.

### AC-5: Preserve research command behavior

- **Given:** Existing just research-sync and research-push callers.
- **When:** Their private Git operations move into research_checkout.sh.
- **Then:** Arguments, messages, exit statuses and reference-run locking remain unchanged. Local fixtures prove dirty commit/push, clean push, wrong or missing origin, non-repository destination, parent-checkout refusal and failed Git operations. Whitespace-bearing paths and messages survive. Tests do not contact a real remote or read private research.
- **Negative control:**
  - **Single fault:** Route a fixture operation to the real remote or stage a parent-checkout file.
  - **At-risk precondition:** The local bare-repository fixture runs with its network guard enabled.
  - **Expected RED:** The fixture records an external attempt or detects a changed Keld parent.

### AC-6: Keep origin admission exact and singular

- **Given:** The three existing admitted origin spellings and optional .git suffix normalization.
- **When:** Sync and push validate their remotes.
- **Then:** Both operations use one origin predicate. No new host, repository, fallback authority or deletion operation is added. Fetch and push URLs must both be admitted. Push uses one explicit branch ref and ignores push defaults.
- **Negative control:**
  - **Single fault:** Add a near-match URL, accept a foreign pushurl or use an implicit push refspec.
  - **At-risk precondition:** Local fixtures exercise the admitted and denied URL matrix, pushurl mismatch and branch/tag collision.
  - **Expected RED:** The fixture observes an unauthorized commit/push or a wrong destination.

### AC-7: Require exact-head integration evidence

- **Given:** The two source slices and their complete PR diff.
- **When:** The owner prepares the final review and integration decision.
- **Then:** Complete diff review, final applicable local and hosted checks, and independent exact-head review pass. No unrelated source/test inventory or policy is weakened. GH-824's separate owner merge hold remains in force.
- **Negative control:**
  - **Single fault:** Mark the program complete from this two-slice PR alone or omit a required platform/review receipt.
  - **At-risk precondition:** The GH-824 audit and merge hold are still open.
  - **Expected RED:** The evidence ledger remains incomplete and the owner gate refuses completion.
