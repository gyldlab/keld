# Delivery and evidence

Part of: [index](index.md)
Owns: sections 5, 6, 7, 9 and 10

## 5. Boundaries

Implement only these slices:

- Capture: crates/keld-runtime/src/lib.rs, the private captured_output.rs module and captured_output/tests.rs. Update only exact selector consumers found by a before/after reference scan.
- Research: the two private research recipes in justfile, tools/research_checkout.sh, its local-fixture tests and the existing navigation map.
- Router: tools/ci-inputs.json only when a new cross-tree reader edge is required. The v2 contract stores no content digest and has no rebind step.

Must not touch:

- Root Cargo.toml, CI workflows, root or nested agent instructions.
- Transport source, native guardian, job or profile implementations.
- Other owners' worktrees or private reference contents.
- CI admission, numeric source-size caps or unsafe-path policy.
- Unrelated Just recipes or broad tools/ moves.

Refresh PR #1075 and all claims before editing overlapping Just regions. Do not take a shared workflow writer's authority. A root-file ownership conflict blocks only that slice.

## 6. Tasks

### T-1: Move the bounded capture owner and pure tests

- **Blocked by:** none
- **Status:** complete
- Record the source and compiled-test baseline before the move.
- Create the private captured_output.rs module.
- Re-export the existing public type and five constants from lib.rs.
- Move only the ten pure capture tests beside the module.
- Reconcile exact test selectors and reader fingerprints atomically.

### T-2: Extract local research Git operations

- **Blocked by:** none
- **Status:** complete
- Reconcile the Just and reference-owner overlap first.
- Move only the sync and push operations into research_checkout.sh.
- Keep one exact origin predicate and explicit branch push.
- Test with local bare repositories and failure controls.
- Keep the workspace reference-run lock and reference-root owner unchanged.

### T-3: Final source, platform and GH-824 reconciliation

- **Blocked by:** B-1
- **Status:** open
- Reconcile source hashes, navigation and compiled test selectors.
- Run the affected native package and workspace tests.
- Run the shell fixture on Windows Git Bash, macOS Bash 3.2 and Linux.
- Complete exact-head independent review and applicable hosted checks.
- Preserve the GH-824 merge hold until its owner releases it.
- Consolidate reviewed changes through gh stack. Do not create a PR per file.

## 7. Test plan & evidence

### E-1: Public API and selector parity

- **Run:** Compile external imports of keld_runtime::CapturedOutput and all five CAPTURE constants. Compare compiled test inventories before and after the move.
- **n:** One before/after inventory per required operating system.
- **Environment:** Exact source SHA on macOS, Windows and Linux.
- **Limits:** A selector map does not prove runtime capture behavior.

### E-2: Bounded capture and native runtime behavior

- **Run:** The ten captured_output tests, affected keld-runtime tests, affected native supervisor tests and warning-denied Clippy.
- **n:** One complete affected-package run per required operating system.
- **Environment:** Exact source SHA and actual native host.
- **Limits:** Pure tests do not replace process-spawn, drain, restart or lifecycle tests.

### E-3: Research helper and local Git safety

- **Run:** bash tools/test_research_checkout.sh and the applicable shell syntax checks.
- **n:** One run per required operating system.
- **Environment:** Windows Git Bash, macOS stock Bash 3.2 and Linux. Use only temporary local bare repositories.
- **Limits:** These fixtures do not access or qualify the real private research remote.

### E-4: Exact-head integration and review

- **Run:** just ci-route, just ci-router-test, just ci, the applicable hosted CI and independent exact-head review.
- **n:** One result for the final exact PR head.
- **Environment:** The final PR head and current main base.
- **Limits:** Green CI does not release the GH-824 owner merge hold.

## 9. Perf impact

None claimed. The algorithms, allocations, retention budgets and execution order remain unchanged. Do not add an assembler, generic task framework or dependency based on an unmeasured speed claim.

## 10. Open questions, blockers & unknowns

### B-1: GH-824 whole-codebase audit and owner merge hold

- **Status:** open
- **Owner:** GYLDLAB (repository owner)
- **Impact:** PR #1178 and the GH-824 architecture program remain under the no-merge hold.
- **Check:** The owner records the complete file-by-file audit and explicitly releases the merge hold in the GH-824 tracker.

### U-1: Final native platform receipt set

The final T-3 evidence must name each actual OS, shell, Git version, source SHA and result. A receipt from one operating system does not qualify another.

The separately owned unsafe-path admission and instruction-format decision remain outside this spec. Numeric source-size caps also remain outside this spec. This document authorizes no change to either area.

The T-1 and T-2 source slices do not complete GH-824. The standing owner merge hold remains unchanged.
