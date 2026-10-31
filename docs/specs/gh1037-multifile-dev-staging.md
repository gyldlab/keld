# Spec: macOS multi-file development source staging (GH-1037)
Status: draft
Linear: GH-1037 (#517 tracker; KEL-96 reference) · Owner: @0monish / GYLDLAB · Updated: 2026-10-31

This is the evidence-led proposal after GH1037's native verification. Exact-text
human approval is required before production staging changes. The verification
fixtures and current-behavior README correction do not implement this proposal.

## 1. Goal & non-goals

Let a macOS development entry import sibling and other project-relative source
files from the ordinary owner-private stage, preserving its project-relative
layout and the existing Host lifecycle. Reuse the sole CLI stage producer and
the runtime's module resolver. Select option (a), a validated read-only source
snapshot, for the Bun side. Renderer resource delivery remains KEL-79-owned.

Non-goals: a new bundler, package resolver, configuration file/key, CLI flag,
transport, manifest format, Host principal, permission grant, installer, watch
loop, hot reload or source re-staging on recovery. This is not an admitted
Drawio closure, installed-package proof, schema-four facade qualification,
release authentication or a complete multi-platform staging implementation.
No production code changes belong in this specification/verification PR.

## 2. Spec refs

- Architecture01/03: Host authority, default-deny, explicit security profiles and
  one owner per source/permission/lifecycle rule.
- Architecture06 §2 and approved `docs/specs/kel96-no-flag-host-boot.md`:
  sole `boot.rs` stage producer, executable-located descriptor, private nonce,
  no-flag Host and fixed entry-relative layout.
- Approved `docs/specs/kel140-macos-public-filesystem.md` §4.5: retained source
  capture, invoking-owner predicate and the one existing policy capture buffer.
  Its §4.6 explicitly distinguishes prepared bundles from general module copying.
- KEL-79 owns renderer origin/resource transport. Its current live state is
  Backlog, unassigned; no approved KEL-79 implementation is inferred here.
- KEL-75/KEL-78 own role generation, cleanup/recovery and OS containment.
  GH499/GH502 own Drawio source admission and qualified installed alias graphs;
  GH593 owns runtime auto-install refusal. None is satisfied by copying files.
- [GH1037 verification](https://github.com/gyldlab/keld/issues/1037#issuecomment-6543050436),
  source `3fd5edf8f286ecb99e8f688dca08a0065a7f95bd`, based on
  `0f5128462d1329ad1945e6639c1956d38242dd4f`. Shipping no-flag Host and the stock
  stage producer ran on macOS26.5.1/build25F80, Darwin25.5.0 arm64, Bun1.4.4.

### Verified current behavior

The normal produced stage omits an original `src/sibling.ts`. Its entry's static
import cannot complete startup: the actual Host exits1 with
`KELD-CORE-037`/`KELD-RUNTIME-003`
before its initial authenticated generation binds. The Guardian error path does
not expose raw Bun loader text, so that text remains unavailable. A separate
**test-only** stage with identical entry bytes and bootstrap setup, with only
that sibling injected0400, completes real HELLO/Ready/Echo/Quit and cleanup.
That counterweight establishes a causal diagnostic; it is unsupported production
packaging and cannot be counted as a supported app launch.

The independent generic renderer-mode fixture actually reports `about:blank`
at native window-load. Its stylesheet callback is pending with the fallback
color11-22-33, its relative image errors with naturalWidth0, and inline controls
load with width3/color19-73-109. The original CSS would yield7-31-91 and the SVG
width7. Pending is not relabelled eventual error. This is the **schema-one inline
HTML path**, not the unrun Drawio facade/schema-four `BrowserWindow.loadURL`
path. It proves no Drawio resource failure or product-corpus outcome.

The first fixture revision0f13b7f failed0/2/EXIT101; its unsupported raw-stderr
assumption and all-callbacks reporter were corrected in retained later commits.
The changed-source verification passed2/2. Both attempts remain in the evidence
archive; a later pass does not replace the original failed result.

## 3. Acceptance criteria (binary, each becomes a test)

1. **Ordinary sibling:** the normal producer stages a project whose configured
   `src/main.ts` statically imports `./sibling.ts`; the unmodified shipping
   no-flag Host reaches actual HELLO/Ready and a sibling-selected effect, then
   Echo/Quit/link EOF and process/stage cleanup. No post-stage injection is allowed.
2. **Layout:** preserve every selected file's path relative to the canonical
   project root. Nested imports and imports to a sibling directory resolve under
   the staged root using Bun itself; no flattening, rewritten import or extra resolver.
3. **Captured bytes:** each selected regular file is read from its validated owned
   handle once into the bytes written to a new staged inode. A source-path
   replacement after open cannot replace those captured bytes. A source error
   fails with existing KELD-CLI-047 detail naming the path and remediation.
4. **Containment/type:** an outside-root reference is not copied; a symlink in
   the selected tree, including one in `src` pointing outside, is refused before
   Host launch. FIFOs, devices, sockets, unreadable and foreign-owned files also
   refuse; none silently becomes a missing-file fallback.
5. **Single policy owner:** the existing permissions-source owner captures and
   stages its one fixed policy buffer. Tree traversal never reopens or overwrites
   that policy, merges grants, changes the profile or writes a second descriptor.
6. **Failure cleanup:** a failure after any selected-file copy removes only that
   incomplete nonce via the existing stage guard. A healthy following project can
   stage and run; caller-selected paths and prior launch roots are not deleted.
7. **Recovery:** an existing recovery generation uses the same completed stage.
   Replacing project source after capture does not re-stage or change the next
   generation. Existing fresh principal/token, stale-generation refusal and
   permission-snapshot behavior retain their separate KEL-75/KEL-78 tests.
8. **Package identity:** when a qualified installed graph is supplied by its
   existing owner, preserve its relative layout and actual resolver/importer
   identity. Re-run that owner's identity matrix from the produced stage before
   accepting its compatibility consumer. Missing/unqualified graphs and unsupported
   file/link kinds are blockers; copied package bytes alone never imply admission.
9. **Renderer honesty:** keep the inline renderer behavior unchanged in this Bun
   staging slice. A copied stylesheet/image is not evidence that WebView URL
   delivery works. Preserve the generic inline fixture's actual states and the
   KEL-79 handoff; no Drawio facade or other engine is inferred from it.
10. **Compatibility:** the existing prepared-entry/canonical-transport hello
    remains healthy with unchanged descriptor/policy bytes and no new required
    configuration. Windows/Linux retain their current staging behavior until
    their separately qualified slices; no unrun OS is reported supported.

## 4. Design

### Facts, ownership and alternatives

**No boundary change:** CLI still owns cold input capture and the private stage;
Host owns windows, app-link, policy, principal/generation and Bun lifetime. No
handle or authority transfers to the renderer. Source file handles belong to
CLI until capture finishes; staged byte files belong to the completed dev stage.
This remains a cooperating-developer snapshot, not an atomic filesystem
transaction or authenticity against a malicious same-user writer.

| Option | Decision and reason |
|---|---|
| (a) Validated read-only project source tree | Selected for the Bun side. The observed missing sibling is ordinary file delivery. Preserve paths and reuse the existing producer, invoking-owner rule and Bun resolver. |
| (b) Entry import graph plus the app's renderer dev server | Deferred. A new import-graph implementation would duplicate resolver semantics and miss dynamic/data inputs; no current approved producer supplies this general graph. Existing qualified package/source graphs remain authoritative when available. The app's renderer dev server is the architecture06 destination under KEL-79; it does not solve this Bun copy defect. |
| (c) Document the current selected-file limit | Required immediately in README as current behavior. It leaves the verified sibling failure in place, so it is the compatibility baseline rather than the proposed fix. |

No implementation is rewritten and no performance advantage is claimed. Keep the
current prepared-entry route available for projects outside this first slice.
Do not invent a graph-qualified flag or approval registry for packaging.

### Source paths and capture

The canonical project root already selected by the CLI is the source root.
The configured relative entry path remains unchanged. Copy the selected tree
beneath the random stage root with that same relative path, so an entry at
`src/main/electron.js` and its local siblings retain their relative relationship.
No source path, working-directory hint or environment variable replaces the
Host's executable-located descriptor.

The first macOS slice admits **ordinary directories and regular files only**:
enumerate without following symbolic links, prove each selected object remains
beneath the retained project root, reuse the invoking-owner predicate and capture
from the selected no-follow handle. Reject every symlink rather than flattening
one or changing module identity. Supporting an internal-link package graph is
an explicit later qualification under its owning graph contract, not an inferred
fallback. Outside-root imports receive no copied outside bytes.

Source selection has one owner in `boot.rs`:
- Exclude Git metadata `.git` and the top-level producer-owned `.keld` subtree;
  never recurse into prior/current launch stages.
- Keep `keld.config.ts` a producer input rather than a staged runtime selector.
- Let the existing fixed `keld.permissions.jsonc` capture owner supply that
  staged leaf; do not capture it a second time through the tree walk.
- Treat collisions with `keld.boot.json`, staged Host/launcher executable names
  or other producer-created leaves as typed refusal, not overwrite.
- Deduplicate the existing configured entry, renderer and canonical transport
  selections within that same capture/copy owner. Do not mirror three copy paths.
- Ordinary package manifests, lockfiles and installed files are data with their
  original bytes/layout, not new resolver configuration. This slice rejects
  unsupported link kinds and does not upgrade an installed graph to qualified.

Use the current new-file/0400 staged-byte rule and private nonce/cleanup owner.
Do not make source files writable, follow an escape, auto-install dependencies,
or silently skip a failed selected input. No arbitrary file/byte quota is chosen
here; existing allocation/I/O failures must remain typed and clean up. Any later
resource quota requires measured inputs and its own explicit contract.

The source buffers actually captured are the copy inputs. Retained-handle
selection defeats path substitution, but this does not claim a simultaneous
version across all source files or prevent an authorized same-user in-place
writer. GH499's authenticated complete-graph/launch-generation admission remains
separate and must not be fabricated from a successful copy.

### Renderer, lifecycle and runtime seam

Renderer HTML still goes through its current inline path. Serving relative
assets, choosing an origin/base URL, dev-server authentication and the Drawio
custom resource entry remain KEL-79 and existing GH471/GH473 owners. No permission,
CSP exception, URL forge, HTTP service or transport is added here.

Before/after execution owner and crash domain remain CLI → shipping Host →
supervised Bun. Capture happens once before launch; recovery reuses the completed
stage. No new public Rust signature, channel, IPC message, schema field,
capability or persisted-state migration is proposed. Compatibility fallback:
the existing prepared-entry/canonical-transport route. No temporary adapter.

Initial qualification is macOS only. Linux directory mounting would change the
current strict launcher's file-only `/code` admission and needs its existing
containment owner plus real Linux proof. Windows namespace/ACL and cleanup
ownership likewise require its real platform slice. Neither is silently widened.

## 5. Boundaries

This PR: focused `keld-host` verification fixture/module registration,
`docs/specs/gh1037-multifile-dev-staging.md`, and README's current staging rule.
Generated docs change only if the allowlisted generator actually selects an input.

After exact approval, implement the smallest macOS producer slice in
`crates/keld-cli/src/boot.rs` and focused owning tests. Consume existing ownership,
policy and qualified-graph interfaces. No changes to Host/runtime/guard policy,
renderer transport, package resolver, aliases, workspace Cargo/toolchain,
CI workflow/fingerprint or other owners' active branches without their separate
scope and independent gates. No PR1306/1308 source adoption.

## 6. Tasks

- [x] T0 verification: run the independent native sibling/counterweight and
  calibrated inline-renderer cases; retain failed and changed-source attempts.
- [ ] T1 publish this draft and the truthful README; obtain independent exact-head
  review and human exact-text approval. No production staging implementation.
- [ ] T2 after approval: one macOS source-capture/copy owner, normal sibling and
  nested-layout vertical slice, all containment/type/failure-cleanup controls,
  unchanged prepared-entry route and actual shipping no-flag Host result.
- [ ] T3 consume separately qualified package graph and recovery contracts only
  when their owners supply them; prove actual produced-stage identity and causal
  recovery. Renderer and other-OS qualification remain their owning slices.

## 7. Test plan

AC1/2: convert the current normal-stage sibling observation to actual supported
startup only in the implementation that makes it pass; keep the injected stage
as labelled causal diagnostic only. Add nested/sibling-directory paths and an
independent Bun-loader effect, exact source-byte/layout checks and healthy
Echo/Quit/cleanup. Do not rely on unavailable error-path stderr.

AC3–6: real temporary files/directories, retained-path replacement, outside
sentinel, outside/in-root symlinks, FIFO/non-file, foreign-owner seam and failed
copy after an earlier success. Assert typed error and exact nonce cleanup, then
a healthy following operation. One-fault controls remove containment/type/capture
checks; each must fail its named oracle. Reuse existing owner helpers, no mock-only
OS proof or copied policy implementation.

AC7/8: existing real generation/graph owner fixtures plus the actual produced
tree; project mutation after capture, stale generation, nested foreign facade
and lost source/transport must be rejected or retain the owner's exact existing
outcome. Do not substitute the future product operation for a mechanism gate.

AC9/10: preserve real parse/script/load calibration and inline positive controls;
record pending explicitly. No physical clicks or focus oracle is needed for this
fixture. Other platform suites/qualified graph tests run only on their owning
real systems. Full format, warning-denied workspace Clippy, applicable CI and
workspace/native verification remain required; this proposal asserts none unrun.

## 8. Review gates triggered

- Public API: development staging contract and exact prepared-entry compatibility.
- Permission model/security: source containment/capture ownership, read-only bytes,
  failure cleanup and no implied graph/profile/renderer authority.
- Unsafe: none proposed; reuse existing safe platform facilities.
- Dependency addition: none.
- Wire protocol: none.

The user-routed external reviewer owns qualifying review. Builder observations
and this draft do not self-approve the design or close those gates.

## 9. Perf impact

More selected bytes require more cold copy I/O and disk space. No measured
improvement, regression waiver, startup/RSS claim or new quota is asserted.
Before performance claims, separately census selected files/bytes, actual copies,
queue/work, clock and statistic, and retain reproducible artifacts under the
existing performance owner. The native verification duration is test execution,
not an application performance benchmark.

## 10. Open questions

1. Exact-text approval of this bounded macOS regular-file tree contract. Rejecting
   all symlinks is deliberate for the first slice; a qualified installed graph
   requiring links remains blocked until its owner approves/proves a supported
   representation. Do not invent a flattening or resolver workaround.
2. KEL-79's renderer resource/origin implementation and Linux/Windows source-tree
   containment are separate owner decisions and unrun platform qualifications,
   not conditions this documentation may silently declare complete.

No production change, Drawio attempt, installation, product score or release is
approved by publishing this draft.
