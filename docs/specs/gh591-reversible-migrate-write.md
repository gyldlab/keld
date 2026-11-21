# Spec: Reversible project writes for keld migrate

- Status: draft
- Tracker: GH-591
- Owner: 0monish (tracker), keld-cli (implementation)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Specify how a developer can review, apply and reverse the first draw.io migration. Every application edit has a named consumer, exact before/after bytes and a recovery path. A partially applied operation must be distinguishable from a completed one.

Scope covers the command-line writer. Analysis, installation, package resolution, runtime permission checks and the installed application proof keep their existing owners. Do not execute project files, install packages, emit a second resolver or generate a permission from a dialog.

## 2. Spec refs

- FACT: Main lists migrate as reserved at `crates/keld-cli/src/main.rs:110` and `docs/architecture/06-runtime-and-tooling.md:352`. The developer flow in `docs/architecture/04-electron-compat.md:9` is a target.
- FACT: #1172 `docs/specs/gh497-first-proof-migration-output.md:25@b7816c4c2ba2a53c4800904601b7e2b5a703a34c` specifies independent write consent, Legacy consent, key owners, reader gates, omission reasons and qualification limits. Use its M01–M18 dispositions. Reconcile changes before implementation.
- FACT: #1410 `docs/specs/gh590-migration-analyzer-scan-rules.md:1@65a406c0345c61351db7afc4fcf2132feb44d59d` concerns static input discovery. #498 owns the read-only command. The writer consumes findings and their evidence.
- FACT: `crates/keld-guard/src/lib.rs:652` owns permission parsing. `docs/architecture/03-security.md:1` assigns authority to the Host. Successful parsing cannot qualify a grant whose reader ignores the requested key.
- FACT: [Reference artifact](https://github.com/gyldlab/keld/issues/499), [activation cell](https://github.com/gyldlab/keld/issues/500) and [writer brief](https://github.com/gyldlab/keld/issues/591#issuecomment-6026224158) assign distinct owners and oracles.

## 3. Acceptance criteria

### AC-1: Preview never writes

- **Given:** A writable app contains a config with an execution marker and a plan containing an eligible edit.
- **When:** The developer runs default analysis or cancels the write preview.
- **Then:** Compare file census, contents and relevant metadata. Observe no file mutation, marker, installer or child process. Include recovery sidecars in the census.
- **Negative control:**
  - **Single fault:** Create a journal before explicit write consent.
  - **At-risk precondition:** The plan contains a real, eligible edit.
  - **Expected RED:** The independent project or recovery directory census gains an entry.

### AC-2: Bind consent to the exact diff

- **Given:** A reviewed plan requests Legacy and includes an otherwise valid edit.
- **When:** The user supplies write consent alone, or any reviewed input changes before apply.
- **Then:** Reject the plan before publishing a file. Name the missing consent or changed input and request a new review. Record explicit acceptance of reduced guarantees separately.
- **Negative control:**
  - **Single fault:** Treat write approval as Legacy consent.
  - **At-risk precondition:** The complete plan remains valid and the profile reader exists.
  - **Expected RED:** The independent fixture sees a profile declaration enter the project.

### AC-3: Emit only owned, literal outputs

- **Given:** Static findings include valid calls, `el:*`, a computed registration, a hostless glob and a `$HOME/**` scope.
- **When:** The writer assembles its output candidates.
- **Then:** Report each refused item with its location, reason and fix. Emit no inferred filesystem scope. Retain existing separately approved scopes. Require the live owner and consumer test for each proposed write.
- **Negative control:**
  - **Single fault:** Include `el:*` in the candidate manifest.
  - **At-risk precondition:** Inspect writer output before a downstream parser could reject it.
  - **Expected RED:** The direct byte inventory finds the forbidden literal as a generated grant.

### AC-4: Preserve intervening writes and contain each target

- **Given:** A valid root-owned target changes after review. A second fixture replaces its parent or target with an external alias.
- **When:** The writer attempts publication, including after the final ordinary path check.
- **Then:** Report a conflict or unsupported target. Preserve edited and external bytes. Refuse in-place operation where the backend cannot establish the required exclusion.
- **Negative control:**
  - **Single fault:** Reopen an unchecked target by name for the final write.
  - **At-risk precondition:** A barrier installs the replacement after validation and before that write.
  - **Expected RED:** The external sentinel or intervening edit changes. An early refusal alone cannot qualify the backend.

### AC-5: Recover partial work without overwriting later edits

- **Given:** Two eligible files exist. Stop the writer after the first replacement. Edit that file before recovery.
- **When:** Recovery reads the durable receipt, including a receipt missing its final progress update.
- **Then:** Compare actual identities and bytes with the recorded preimage and postimage. Preserve the new edit. Expose the original bytes separately and report the unresolved path. A clean run restores original bytes and applicable mode, including original absence.
- **Negative control:**
  - **Single fault:** Restore the preimage unconditionally.
  - **At-risk precondition:** The first target now contains a distinct third value.
  - **Expected RED:** The later edit disappears. A reported conflict alone cannot pass the byte oracle.

### AC-6: Compare the complete generated output independently

- **Given:** B-1 is closed and #499 supplies an independently hand-authored, digest-bound reference at the same app pin.
- **When:** Apply the generator to a fresh copy of that app.
- **Then:** Compare every owned relative path and byte to #499. Compare an independent whole-tree census to the declared edits. Record omitted keys with reasons and the exact inverse for each edit.
- **Negative control:**
  - **Single fault:** Change one output byte.
  - **At-risk precondition:** Keep #499's original reference immutable and separate from the generator.
  - **Expected RED:** Byte comparison identifies the affected path even if parsing succeeds.

### AC-7: Qualify the generated app in the upstream cell

- **Given:** B-1 and B-2 are closed. The real macOS operator has the same upstream commit, webapp gitlink and qualified environment as #500.
- **When:** The operator substitutes the generated output for #499's fixture in the actual activation cell.
- **Then:** Record the production cell's expected positive observable and downstream disposition. An expected-failure match or a fixture launch cannot qualify activation. Preserve the separate install and workflow cell results.
- **Negative control:**
  - **Single fault:** Remove a required channel grant from the generated copy.
  - **At-risk precondition:** The positive path reaches that guarded operation. Recompute packaging integrity normally so unrelated integrity checks do not mask the mutation.
  - **Expected RED:** The independently observed activation condition fails at the denied call. A runner returning zero for an expected failure is insufficient.

## 4. Design

### D-1: Materialize a finite write plan

The CLI owns cold project editing. Read input bytes once into a bounded plan. Bind its identity to the project root, analyzer evidence, reader versions, reviewed output dispositions and complete diff. Reject ambiguous keys, unowned paths and missing required readers. Keep omission reasons in the report.

Use at most four existing approved target names at the project root. The list below limits candidates. It does not enable a missing reader.

| Target | Proposed edit and required owner |
| --- | --- |
| `keld.config.ts` | Only the #497 table's live, tested keys. Its B10 ruling leaves proposed preload and web keys unwritten. Never evaluate dynamic window identity. |
| `keld.permissions.jsonc` | Use approved Guard vocabulary. Bind exact channel registration, direction, supported method, observed use and window identity as #497 M07 requires. Keep separate Legacy consent and profile admission. |
| `package.json` | Apply approved #502 and #491 declarations and remove or rewrite reviewed scripts. Preserve unrelated fields. Never represent a stale install as qualified. |
| Root-owned `tsconfig.json` | Remove an explicitly approved local `compilerOptions.paths.electron` mapping. Report inherited, wildcard or other unowned overrides and their owners. Do not repair them silently. |

Use the existing report and diagnostic shapes designated by #497 M14. Each row gives relative target, action, reason, source, owner, consumer test and inverse. List `keld.build.ts`, `keld.compat.ts`, Bun aliases and proposed resolver mappings with #497 M10 reasons. Leave package locks and installed dependencies to their owners.

Check the live Guard parser and the actual field consumer. Candidate permission serialization must use the owning vocabulary. No second parser or capability registry belongs in the CLI. Never remove or broaden separately approved filesystem scopes.

### D-2: Capture and preserve both file states

Before publication, retain exact bytes for every existing target and record absent targets explicitly. A digest alone cannot restore bytes. Record length, type, supported mode, root identity and operation order. Refuse unsupported metadata, multiply linked files, symlinks and special targets.

Create recovery records only after consent in a transaction-specific private directory outside the app. Never load arbitrary backup paths from the project. Limit each non-manifest image to 1 MiB and each permission manifest to the live 64 KiB ceiling. Reject oversize inputs before allocation. Four targets require at most eight bounded images.

Own at most one transaction per selected root. A second attempt returns a conflict immediately. Limit the receipt to the four enumerated root names. Bind and revalidate receipt version, root identity, image lengths and digests before use. Disclose recovery location separately from public or telemetry output.

### D-3: Publish with a recoverable sequence

Persist immutable preimages and planned postimages before the first target change. For each target, preserve the original value or absence until publication. Prepare same-filesystem replacement bytes, persist them, then publish only under the validated target condition. Persist progress after each replacement.

A process can die between file publication and journal update. On restart, classify each observed target under the same exclusion. Exact old means unapplied or restored. Exact new means applied. Any third value, type mismatch, missing material or contradictory identity requires manual resolution.

Read back the entire output set before declaring success. An I/O error or mixed tree is a failed or incomplete transaction. Report every affected path. Apply an inverse only to the exact owned postimage. Otherwise provide original bytes for manual comparison without changing that path.

A digest check followed by ordinary rename leaves a race. A cooperative lock excludes only cooperating writers. Require a proven exclusive editing boundary or a proven conditional publication primitive for the target. If unavailable, keep the preview and refuse in-place apply. B-3 governs backend enablement.

A fresh private working copy may exercise an explicitly scoped single-writer cell. Never advertise that result as a shared-working-tree guarantee. A power-loss guarantee requires filesystem-specific durability evidence beyond a killed-process test.

## 5. Boundaries

- Implement report consumption and transaction coordination in `crates/keld-cli`. Reuse existing Guard and config readers and existing findings types. Preserve default read-only CLI and analysis-only MCP.
- No changes to app runtime, Host, IPC wire, global dependencies, release policies, source application or platform implementation in this docs PR.
- No publicly assigned diagnostic number in this proposal. Follow the existing CLI registry when implementing typed conflict, refusal, incomplete operation and recovery errors.

## 6. Tasks

### T-1: Build the read-only plan and independent negative fixtures

- **Blocked by:** B-1
- Add AC-1 through AC-3 as red oracles for the new path. Return the full diff and refusal reasons without publishing anything.

### T-2: Qualify one backend behind the test harness

- **Blocked by:** T-1
- Add AC-4 and AC-5 with fault injection and a real filesystem. Submit these results to B-3 before exposing public writes.

### T-3: Enable the qualified writer

- **Blocked by:** T-2, B-3
- Expose only the qualified mode after the path, consent and recovery tests pass. Prove AC-6 with the independent #499 fixture.

### T-4: Hand off actual activation

- **Blocked by:** T-3, B-2
- Give #500 the generated byte manifest, app pin, webapp gitlink, package identity and recovery receipt digest. The assigned macOS operator runs AC-7.

## 7. Test plan & evidence

| Criterion | Independent observable |
| --- | --- |
| AC-1, AC-2 | Compare real project and recovery directory snapshots. Test cancel, missing consent, changed content, changed plan, no change and successful explicit consent. |
| AC-3 | Inspect eligible and ineligible generated rows separately. Include wrong handler kind, unobserved literal, unsupported method, computed window, duplicated key, existing scope and ignored parser field. |
| AC-4 | Use deterministic barriers around validation and replacement. Include changed bytes, appeared file, parent alias, replaced root, disallowed link, changed mode and two writers. |
| AC-5 | Kill a child at each durable boundary. Recover twice for both create and replace. Exercise changed, deleted, original and postimage cases. Reject corrupted/truncated images and mismatched roots. |
| AC-6 | Compare complete file set, bytes, edit list and reasons. Hash the reference fixture independently. A second run against already-migrated unchanged input lists zero content edits. |
| AC-7 | Run the separate macOS cell with real generated artifacts and the one-grant mutation. Keep failing and unavailable receipts distinct. |

### E-1: Baseline source inspection

- **Run:** Inspect the GitHub blobs pinned in section 2, the #591 brief and the separate #499 and #500 requirements.
- **n:** One KELD main revision, one #497 proposal revision and one #590 proposal revision.
- **Environment:** Repository-source inspection through GitHub. No native device or CLI run.
- **Limits:** This evidence establishes ownership and the current CLI boundary. Transaction behavior and installed activation need future tests.

## 8. Review gates triggered

Public API and permission model require independent review of consent, refusal, report semantics and owner boundaries. Unsafe, dependency addition and wire protocol are absent from this documentation change. New native primitives would trigger their own implementation review.

## 9. Perf impact

No measured change. The first-proof writer is a cold developer operation. Record actual input size, bytes staged and transaction duration when measuring implementation.

## 10. Open questions, blockers & unknowns

### B-1: Obtain approved producers, readers and reference output

- **Status:** open
- **Owner:** #497, #498 and #499 maintainers with the mapped field owners.
- **Impact:** The implementer cannot assume the final analyzer interface, missing reader, or exact fixture bytes.
- **Check:** Record approved contract versions and compare the full independently committed fixture under its exact app pin.

### B-2: Obtain a qualifying macOS activation cell

- **Status:** open
- **Owner:** The #500 product acceptance operator.
- **Impact:** Byte identity alone cannot establish that the installed generated application activates.
- **Check:** Verify a positive activation receipt under the actual #500 cell and qualification environment before testing the generated substitution.

### B-3: Prove a safe backend for each allowed apply mode

- **Status:** open
- **Owner:** The #591 implementation owner and independent filesystem reviewer.
- **Impact:** Unqualified roots remain preview-only. A successful macOS backend gives no Windows or Linux guarantee.
- **Check:** Demonstrate AC-4 and AC-5 with concurrent replacement, crash injection and independent sentinel files on the claimed filesystem.

### U-1: Crash durability outside the qualified filesystem

UNKNOWN: Separate process termination from sudden power loss. Qualify only the captured failure model and platform. Windows and Linux implementation receipts remain future work.
