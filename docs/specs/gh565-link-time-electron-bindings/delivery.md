# GH-565 delivery and evidence

Part of: [index](index.md)
Owns: boundaries, tasks, test evidence, performance and open blockers

## 5. Boundaries

- Implement in:
  - `packages/@keld/electron/src/{index.ts,unsupported.ts,unsupported-bindings.ts}`;
  - `packages/@keld/electron/src/{module-bindings.test.ts,native-surface.test.ts,dialog.test.ts}`;
  - `packages/@keld/electron/fixtures/bindings_no_link.ts`;
  - `crates/keld-compat/fixtures/main-bindings-corpus/{corpus.json,denominator.json}`;
  - `crates/keld-compat/fixtures/native-corpus/{corpus.json,denominator.json}` (re-record only);
  - `crates/keld-compat/fixtures/doc-snapshots/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/`
    `{shell,clipboard,clipboard-item,native-image,session}.md`;
  - `crates/keld-compat/tests/support/corpus_manifest.rs` (one `REGISTRY` entry);
  - `docs/engineering/keld-error-codes.md`, regenerated `llms-full.txt`.
- Must not touch: `app.ts`, `browser-window.ts`, `@keld/api`, `@keld/kipc`, any Rust
  crate source, `Cargo.toml`, the kipc channel table, `keld-guard`, the architecture docs,
  the GH-563 transition-gate cases, `tsconfig.json` or `package.json` of the package.

## 6. Tasks

### T-1: Owner approval of the corrected GH-565 contract
- **Blocked by:** none

The owner approved the exact corrected contract on 2026-11-15.
This is a design-only decision and does not certify code.

### T-2: Admit red main-binding corpus cases first
- **Blocked by:** none

Record the pinned upstream pages, exact snapshots and manifest digests.
Create the electron-main-bindings-v1 corpus, nine binding cases and registry entry.
Keep the existing negative member corpus RED until its implementing tickets land.
Historical commits captured the red-first evidence before the new bindings.

### T-3: Export the nine typed-refusal bindings
- **Blocked by:** T-2

Implement the error type, refusing member tables, default parity and zero Host traffic.
Register KELD-COMPAT-013 and re-record native absence oracles as typed refusals.
Retain the independent mutation results and the exact linked-import proof.

### T-4: Reconcile ipcMain ownership with PR #1365
- **Blocked by:** T-3

Remove ipcMain from this slice, its corpus row and duplicate snapshot.
Keep eleven owned imports here; PR #1365 supplies the twelfth on composition.
Rebaseline dependent corpus cases when integration changes their observations.

### T-5: Apply approved platform and ClipboardItem corrections
- **Blocked by:** T-1, T-4

Run failure-first regressions for OS-specific absent, present and own-undefined members.
Reject invalid ClipboardItem records and snapshot enumerable payloads once.
Require native macOS, Linux and Windows tests, typed refusal preservation and no Host traffic.
Resolve the independent review findings only after exact-head evidence.

## 7. Test plan & evidence

| AC | Test |
|---|---|
| AC-1 | `module-bindings.test.ts`, nine cell cases `<module>.binding.named-import: …`, each spawning one Bun child with a one-name import (isolated temp file, `file://` specifier) |
| AC-2 | same file, `the eleven names of draw.io's import statement (electron.js:5-6) that this facade exports link in one module graph; ipcMain is PR #1365's` and the `MenuItem` SyntaxError control |
| AC-3 | same file, `the default export carries every named binding` |
| AC-4 | same file spawning `fixtures/bindings_no_link.ts` with `KELD_APP_LINK` set; asserts `connect=0` and `touched` equal to the actual platform's available-member count plus two constructors; ClipboardItem uses a valid nonempty record |
| AC-5, AC-6, AC-7 | same file, one case per table row from the test's own copy of §4.3 (independent of the implementation's table), plus a census case per binding |
| AC-8 | same file, `ClipboardItem` invalid-shape, valid-record and getter-side-effect/error cases; run before and after the constructor fix |
| AC-9 | existing `native-surface.test.ts` `MenuItem` cases, unchanged |
| AC-10 | `cargo test -p keld-cli --test error_registry` |
| AC-11 | `cargo test -p keld-compat` (registry: census, digest, citations, Bun admission); re-recorded cases in `native-surface.test.ts` and `dialog.test.ts` |
| AC-12 | manual mutation run, recorded in the PR with the failing case names |

Anti-flake: every child has a kill-switch timeout below Bun's per-test limit, writes only
to an isolated temp directory, and reports on stdout; no sleeps; no ports; no OS UI.

### E-1: Historical package-level validation
- **Run:** `cd packages/@keld/electron && bun test`
- **n:** 240 passing cases at the historical reconciliation commit.
- **Environment:** macOS, Bun 1.4.4, commit f09051340f0fc5f71a69df97c25f589e270118d4.
- **Limits:** Before corrected platform census, owner approval and final-head verification.

### E-2: Pre-correction exact-head CI
- **Run:** https://github.com/gyldlab/keld/actions/runs/43702188707
- **n:** 26 jobs, 24 pass and 2 fail.
- **Environment:** GitHub Actions, 2026-11-15, head 98b0b84c19ea3765f75efd0db614d53019a83fc2.
- **Limits:** Spec-structure baseline failure blocks CI; the implementation remains under review.

## 9. Perf impact

none. Module evaluation adds seven frozen objects and two classes; no frame, no allocation
on any kipc or event-loop path.

## 10. Open questions, blockers & unknowns

The [owner decision](https://github.com/gyldlab/keld/pull/1366#issuecomment-6893253680) resolves the design questions:
retain three native CI lanes and upstream platform-accurate member presence.
Use the corrected ClipboardItem admission from Electron v44.4.5.
Keep GH-476 as interim tracker for unspecified members.
Integrate PR #1366 before #1365; reconcile dialog overlaps with #1306/#1308.
No human design question remains open.

### B-1: Corrected implementation and native checks
- **Status:** open
- **Owner:** @0monish
- **Impact:** Implementation must not merge before exact-head tests prove the approved contract.
- **Check:** All native Bun matrix jobs, spec-structure, corpus, typecheck and required CI pass on the published correction.

### B-2: Independent review and integration hold
- **Status:** open
- **Owner:** @0monish
- **Impact:** Existing requested changes and cross-PR conformance can still block merge.
- **Check:** Reviewers verify corrected source, close grounded findings and reconcile #1365 and dialog-related corpus changes.
