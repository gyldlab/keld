# Spec: KEL-64 macOS startup attribution
Status: approved
Linear: KEL-64 · Owner: GYLDLAB · Updated: 2026-08-17

## 1. Goal & non-goals

Identify whether Keld's macOS cold-start tail comes from Keld's own window and
webview construction, or from process launch / system WebKit scheduling outside
that construction. The observable outcome is a reproducible, benchmark-only
per-arm stage trace correlated with KEL-64's existing independent double-rAF
beacon. It will decide whether an optimisation task is justified; it will not
declare a performance win by itself.

Non-goals:

- No change to Keld's normal `--hello` behavior, command-line surface,
  permissions, IPC, or release artifact.
- No synthetic input, sleep-based synchronisation, fallback score, or relaxed
  foreground/focus requirement.
- No optimisation before the trace attributes a concrete Keld-owned stage.
- No cross-engine or cross-session overall-leader claim.
- No non-macOS implementation in this task.

## 2. Spec refs

- `docs/architecture/01-overview.md` §4 and §5: UI-thread ownership and the
  cold-start-to-first-paint / RSS budgets.
- `docs/architecture/05-webview-and-native.md` §1: the live `keld-wv` backend
  owns webview/window construction on the UI thread.
- `docs/architecture/06-runtime-and-tooling.md` §5: observed performance must
  describe what is actually live rather than the destination runtime.
- KEL-64's strict external monotonic-clock, nonce, focused-beacon, foreground,
  restoration, and coalition-RSS contract remains the score oracle.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given the committed KEL-64 Keld recipe with trace enabled, when a macOS
   `keld-host --hello` arm reaches a valid external beacon, then the harness
   records exactly one trace for that arm with its launch nonce and all four
   ordered stages: `wv_run_entered`, `event_loop_created`, `window_built`, and
   `webview_built`.
2. Given a trace with a wrong nonce, duplicate stage, omitted stage, or
   non-monotonic duration, when the harness evaluates the arm, then it rejects
   the arm with a typed startup-trace measurement failure; the external beacon
   alone must not publish the trace result.
3. Given a missing, unreadable, or pre-existing trace path, when the traced
   adapter starts, then the arm fails closed with an actionable measurement
   error; it must never consume a previous arm's trace.
4. Given the same clean Keld source and fixture commit, when alternating
   trace-disabled and trace-enabled Keld arms are run under the existing strict
   oracle, then each reported performance score comes only from trace-disabled
   arms. Trace-enabled arms are attribution evidence only.
5. Given a valid startup trace but a hidden, unfocused, malformed, stale, or
   timed-out beacon, when the harness evaluates the arm, then the arm remains
   invalid. A stage trace is never a paint, focus, or foreground oracle.
6. Given the completed trace set, when Keld's internal construction intervals
   do not explain the end-to-end p90 tail, then no product optimisation lands
   from this task; the result is recorded as an external/runtime scheduling
   limitation with its evidence.

## 4. Design

- The public `macos/keld/hello` recipe expands its committed benchmark-only
  adapter patch. It changes only the fresh detached benchmark source checkout;
  Keld's repository and its shipping artifact do not read a trace setting.
- The patch adds a private `keld-wv` trace recorder used only by the patched
  macOS hello path. It captures `Instant`-relative nanoseconds at the four
  Keld-owned points above. It has a fixed four-slot state, accepts each stage
  once, and never allocates on the stage-marking path.
- The trace starts immediately on entering `keld_wv::hello::run`; it marks
  after `WkWebViewEngine::new`, after `WindowBuilder::build`, and after
  `WebViewBuilder::build`. These boundaries measure Keld's UI-thread work
  without asserting that they are equivalent to first paint.
- The record is emitted once to the harness-provided unique path only after
  `webview_built`. The record contains the existing run nonce, ordered stage
  names, and relative nanoseconds—no app content, user data, process IDs, or
  window handles. The trace write is not included in any scored arm.
- The Swift harness generates and reserves a non-existent per-arm trace path,
  passes it only to the traced Keld artifact, and reads it only after the
  existing externally timed beacon has been accepted. Its parser validates the
  token and all shape invariants before the trace is recorded.
- A trace-enabled arm is paired with a trace-disabled arm from the same clean
  source/recipe and run in an alternating order. The disabled arm remains the
  only score-bearing artifact; the enabled arm establishes attribution.
- Capabilities required; manifest changes: none.
- Wire/protocol changes: none. The trace file is an internal benchmark evidence
  format, not kipc or a product contract.
- Platform notes: this implementation is macOS-only. Windows and Linux retain
  their current KEL-64 behavior and receive no trace setting or claim.

## 5. Boundaries

- Implement in: `gyldlab/keld-benches` `macos/harness/` and
  `macos/keld/hello/`; the recipe's fresh source patch may touch only the
  macOS Keld hello implementation required to emit the four records.
- Must not touch: Keld's checked-in product source, `keld-guard`, `keld-ipc`,
  public `WebEngine` API, workspace dependencies, benchmark validity policy,
  or competitor fixtures. The harness may add an optional privacy-safe trace
  field and must refuse publishing trace-enabled diagnostic arms.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1 Add the four-stage benchmark-only Keld patch and a strict harness
  parser with falsifiable malformed/missing/stale/duplicate controls.
- [x] T2 Build immutable traced and untraced Keld artifacts from the same
  source, run alternating strict arms, and record raw evidence plus stage
  distributions.
- [x] T3 Make and record the attribution decision. If Keld-owned work explains
  the p90 tail, create a separate approved optimisation spec; otherwise record
  the external limitation and close this attribution slice.

## 7. Result (2026-08-17)

A clean `keld-host` adapter from Keld `59e0987` and public recipe
[`258756c`](https://github.com/gyldlab/keld-benches/commit/258756c051fb951590d69d16fbad96f85c605d8b)
ran 11 trace-disabled and 11 trace-enabled arms in alternating order. All 22
samples met the existing focused double-rAF, foreground, restoration, cleanup,
and coalition-RSS contract. Finder was the stable pre-arm foreground anchor.

The trace-enabled diagnostic arm reached `webview_built` at 149.031 ms median
(168.192 ms p90), while the independent valid beacon reached 352.211 ms median
(392.408 ms p90). Its post-webview residual was 197.656 ms median and 215.243
ms p90. The 739.960 ms maximum completed Keld-owned webview construction at
138.650 ms, leaving 601.311 ms after it. Therefore Keld's construction work
does not explain the retained macOS tail. That residual includes WebKit process
startup/navigation and the canonical-page scheduling path; this experiment does
not isolate those external components further. No product optimisation is
justified by this attribution slice.

Raw local evidence: `/private/tmp/keld-startup-attribution-paired-11-finder-20260817.json`.
The trace-disabled lane is intentionally not a publishable score because the
paired diagnostic run contains trace-enabled arms.

## 8. Test plan

- Harness pure tests: reject wrong nonce, missing stage, duplicate stage,
  out-of-order stage, non-monotonic duration, wrong source identity, and a
  stale existing path. Each negative control mutates a real accepted record.
- Recipe test: shell syntax plus a clean detached-source build; confirm that
  the untraced build has no trace output and the traced build yields exactly
  one record after the fourth mark.
- macOS real-OS test: use the existing KEL-64 harness, port 0, unique nonce,
  temporary output path, and condition-based deadlines. Run the trace-disabled
  score arms and trace-enabled attribution arms alternately; no sleeps are
  used for ordering.
- Falsifiability: temporarily deleting `webview_built` must make the strict
  parser reject the trace; replacing its timestamp with a prior stage's value
  must reject it as non-monotonic. The external beacon must still be required
  in both controls.

## 9. Review gates triggered

Unsafe: none. Public API: none. Permission model: none. Dependency addition:
none. Wire protocol: none.

## 10. Perf impact

The score-bearing untraced arms continue to measure architecture 01 §5's cold
start → first-paint and coalition RSS values. Traced arms are diagnostic only;
their overhead is never folded into a score. The final report includes medians,
p90s, raw samples, and the exact public recipe/source commits.

## 11. Open questions

None. Human approval was given for this attribution-first scope on 2026-08-17.
