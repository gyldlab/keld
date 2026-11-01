# Spec: macOS multi-file development source staging (GH-1037)
Status: draft
Linear: GH-1037 (#517 tracker; KEL-96 reference) · Owner: @0monish / GYLDLAB · Updated: 2026-10-31

This is the evidence-led proposal after GH1037's native verification. General
source-tree copying remains draft and requires its source/startup owner contract.
The bounded macOS stage-isolation prerequisite below is the current implementation
continuation under the user's delegated technical decisions. Its independent
review and native acceptance remain separate gates; it does not approve or
implement general source-tree copying.

The later **private graph-bound first-proof slice** in §11 is separately
approved. It does not enable general tree copying in ordinary `keld dev` or
change the historical verification results recorded below.

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
The original verification delta contains no production change. The continuation
changes only macOS stage placement and its existing consumers; source selection,
graph admission, renderer delivery and other-platform staging remain unchanged.

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
The operative cause is that base, not file absence: the reflected `href`/`src`
stay the unresolved raw values, and an uncommitted diagnostic run that also
copied both files into the stage produced a byte-identical snapshot.

The first fixture revision0f13b7f failed0/2/EXIT101; its unsupported raw-stderr
assumption and all-callbacks reporter were corrected in retained later commits.
The changed-source verification passed2/2. Both attempts remain in the evidence
archive; a later pass does not replace the original failed result.

External [review6543505060](https://github.com/gyldlab/keld/issues/1037#issuecomment-6543505060)
found that the later159 renderer predicate did not enforce its relative-resource
claims. Two actual old159 HTML-only one-fault controls confirmed this: removing
only the stylesheet href assignment, or separately only the image src assignment,
still passed the unchanged Rust predicate and inline controls. This does not
falsify the independently recorded native baseline above; it does falsify that
predicate's ability to guard the probe.

The R1 correction records live DOM attribute presence/value, reflected href/src,
node insertion/type/owner, document readiness and actual resource effects.
Dedicated predicates require both relative target assignments and the exact
measured inline-document baseline, separately from inline positives. DOM
assignment/insertion is not claimed to be a network request, and CSS pending
remains pending. At source `4d929512ff972788c41d59a7ef98c42a691d85d4`,
[R1 verification](https://github.com/gyldlab/keld/issues/1037#issuecomment-6543683166)
passed eight pure predicate cases and the healthy native renderer. Each actual
href/src omission failed its dedicated target oracle while inline controls and
orderly lifecycle cleanup passed. The restored healthy-following case passed.
These controlled failures are retained separately from the two old159 false
passes and the earlier3fd2/2 result. External exact-head re-review remains required.

A later adversarial review reproduced four findings natively. Deleting the
stylesheet listeners left a byte-identical pending snapshot that still passed;
a `<base>` change was blamed on the target oracle; README blamed staging rather
than the base for unloaded renderer assets; and the sibling case asserted less
than this section states. The R2 correction routes every element through one
fixture observer that alone mints pending, so a bypassed target reports
UNOBSERVED, and a data-URL stylesheet control through that observer must load.
Base-resolved reflections are document-baseline fields, read only for an
assigned attribute. The sibling case now asserts exit1 plus both codes. On the
same macOS26.5.1/25F80 arm64 Mac with Bun1.4.4, ten pure and both native cases
pass. Deleting the observer listeners, bypassing it for the stylesheet, or
omitting href or src fails; a `<base>` change fails only the document baseline.

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
   incomplete nonce via the existing stage guard when the retained nonce and
   parent identities still match. Missing, substituted or uninspectable identities
   refuse destructive cleanup and retain the owned incomplete stage for safe
   disposition. A healthy following project can stage and run; caller-selected
   paths and prior launch roots are not deleted.
7. **Recovery:** an existing recovery generation uses the same completed stage.
   Replacing project source after capture does not re-stage or change the next
   generation. Existing fresh principal/token, stale-generation refusal and
   permission-snapshot behavior retain their separate KEL-75/KEL-78 tests.
8. **Package/startup identity:** when a qualified installed graph is supplied by its
   existing owner, preserve its relative layout and actual resolver/importer
   identity. Re-run that owner's identity matrix from the produced stage before
   accepting its compatibility consumer. Missing/unqualified graphs and unsupported
   file/link kinds are blockers; copied package bytes alone never imply admission.
   Package/configuration/environment/preload inputs that affect startup must be
   qualified by their existing owners before evaluation; file presence is not proof.
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

**Source-copy ownership is retained:** CLI still owns cold input capture and the private stage;
Host owns windows, app-link, policy, principal/generation and Bun lifetime. No
handle or authority transfers to the renderer. Source file handles belong to
CLI until capture finishes; staged byte files belong to the completed dev stage.
This remains a cooperating-developer snapshot, not an atomic filesystem
transaction or authenticity against a malicious same-user writer.

### Isolated macOS stage prerequisite

The live project must not be an ancestor of the produced stage. A fresh Bun 1.4.4
control repeated the owning KEL-75 observation: a staged import reached live
project `node_modules`, `tsconfig.json` paths and `package.json` imports even with
`--no-install --no-env-file`. The same three imports failed from an isolated
placement before either target or entry executed, while captured relative source
still ran. This is resolver evidence, not a shipping Host or draw.io acceptance.

The CLI therefore owns this bounded filesystem-boundary correction before tree
copying. Its canonical system temporary directory must be outside the canonical
project, owned by the invoking user and already mode `0700`. No fallback directory
or permission repair is selected. The project namespace is
`<temporary>/keld-dev-<uid>/<project-id>/.keld/dev/`, where `project-id` is the
lowercase SHA-256 of the canonical project path's native bytes. It is only a
non-authoritative locator; it does not authenticate the application or grant a
capability. The existing random 128-bit nonce remains the per-launch leaf.

Create each private namespace component with mode `0700`, open it relative to
the retained parent with directory/no-follow flags, verify its existing owner and
mode, and retain those parents until staging completes. Refuse a symlink,
non-directory, foreign owner or unexpected mode without modifying it. Refuse
`node_modules`, `package.json`, `tsconfig.json` or `jsconfig.json` in any stage
ancestor. Immediately before completing capture, recheck the retained private
parents' owner/mode and device/inode identity against their pathnames, and repeat
the ancestor-input census. A missing, inaccessible or unsuitable temporary base
is a typed staging failure, not a reason to return to the project-local layout.

The existing `StageGuard` owns the retained namespace chain and opens the allocated
nonce relative to the retained `dev` directory, with directory/no-follow flags.
Every failure unwind checks the retained parents and nonce against their named
owner/mode and device/inode identities before removing that incomplete nonce.
Unknown, missing, substituted or uninspectable identity skips deletion, preserving
the moved original stage and any replacement pathname. This identity-only cleanup
check is separate from the resolver-input census: a late input with stable
directory identities still removes only the owned incomplete nonce. Immediately
before publishing a completed stage, require the retained nonce identity and
recheck it together with the retained parent chain, after the final ancestor-input
census. Any refusal leaves the same `StageGuard` responsible for its identity-bound
failure unwind. A missing identity is a failure, not permission to publish by path.

The shipping Host and Guardian still derive their exact `.keld/dev/<nonce>` cleanup
root from the running executable and the existing lease; no new deletion authority
or caller-selected root is introduced. Their production selection code is
unchanged. The original entry/renderer/transport and one policy capture remain the
only staged inputs. Linux and Windows retain their existing location and qualification.

The native acceptance observers use the actual producer-returned parent and the
kernel-observed Host image, while preserving exact source/policy byte checks and a
live-stage positive before cleanup assertions. Test-only fallback cleanup happens
after those assertions. The CLI regression must reject each live-project-only
import and run a healthy captured entry; namespace tests cover unsuitable
temporary placement, resolver-bearing ancestry, symlinks, mode preservation and
capture-time path/input substitution. A late-input refusal must remove only the
incomplete nonce while preserving an earlier completed stage and the new input;
a healthy following capture must succeed after restoring the test fixture.
The full staging path must also reject a parent replacement made during source
capture while preserving a same-nonce replacement sentinel, the moved original
incomplete stage and an earlier completed stage's Host/descriptor bytes. Its
healthy-following control restores only test-owned paths after those oracles.
The same full-producer regression must also substitute only the nonce leaf while
leaving every parent unchanged. It must refuse publication and preserve the moved
original stage, the replacement sentinel, and the earlier completed stage before
any test-only restoration. This distinguishes final leaf admission from the
already-qualified error-unwind cleanup predicate.
This amendment changes KEL-96 stage-location text and architecture06 together.

This slice does not freeze startup environment, stop runtime package acquisition,
admit installed aliases, or supply GH499's missing source graph. Those remain
KEL-75/GH499/GH502/GH593-owned implementation gates. The Host's qualification
immediately before every Bun generation is not implemented by this CLI check.
Same-user namespace mutation after capture remains outside the
cooperating-developer snapshot guarantee; held CLI descriptors do not freeze
Bun's later path-based lookup. Identity reads also do not lock namespace entries
against a later same-user replacement during pathname cleanup.

| Option | Decision and reason |
|---|---|
| (a) Validated read-only project source tree | Selected for the Bun side. The observed missing sibling is ordinary file delivery. Preserve paths and reuse the existing producer, invoking-owner rule and Bun resolver. |
| (b) Entry import graph plus the app's renderer dev server | Deferred. A new import-graph implementation would duplicate resolver semantics and miss dynamic/data inputs; no current approved producer supplies this general graph. Existing qualified package/source graphs remain authoritative when available. The app's renderer dev server is the architecture06 destination under KEL-79; it does not solve this Bun copy defect. |
| (c) Document the current selected-file limit | Required immediately in README as current behavior. It leaves the verified sibling failure in place, so it is the compatibility baseline rather than the proposed fix. |

The isolation prerequisite does not rewrite source selection or claim a
performance advantage. Keep the current prepared-entry route available for
projects outside the proposed source-copy slice.
Do not invent a graph-qualified flag or approval registry for packaging.

### Source paths and capture

The canonical project root already selected by the CLI is the source root.
The selected tree must be qualified by the existing source/installed-graph and
role-startup owners for any startup-affecting inputs. The exact existing
qualification interface for that general tree remains an explicit draft
unknown below; this text does not invent one or claim it exists. Ordinary
relative-file verification did not exercise such configuration or package graphs.
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
- Preserve admitted package/lock bytes and layout, but do not call package
  manifests, TypeScript path settings, Bun preloads or environment files inert:
  they can affect resolution and execution. Their effective source/importer and
  startup environment require the existing GH499/GH502/GH593/KEL-75 owners'
  qualification. Do not add a competing parser, filename bypass or auto-grant.
  This draft selects no new configuration, preload or environment-loading policy.
  Unsupported link kinds remain refused; copying cannot upgrade qualification.

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

Original verification: focused `keld-host` fixture/module registration, this
specification, and README's current staging rule. The isolation continuation owns
`crates/keld-cli/src/boot.rs` stage placement and `boot/macos_stage.rs`, their
owning tests, the existing macOS stage observers, and matching KEL-96/architecture06
location text. It does not edit the boot-descriptor metadata block or shared CI keys.
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
- [ ] T0a isolation prerequisite: fail first on actual producer ancestry, move the
  macOS nonce outside the project using the existing private-stage/cleanup owners,
  and qualify namespace refusal, preserved stock launch and cleanup. Candidate
  implementation is not completed acceptance.
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

1. **Source/startup admission interface — implementation blocker, not a fabricated
   completed closure.** GH499/source admission, GH502 installed graph and
   GH593/KEL-75 role-startup owners must identify the existing qualification
   consumed by the sole stage producer, including effective package/path config,
   preload and environment inputs before evaluation. The exact owner action is
   to bind that input selection and refusal contract to this source-tree scope;
   its first check substitutes a startup preload/path redirect and must refuse
   before its sentinel executes. No such artifact is proven by this fixture.
   Until that contract is supplied, keep this draft unapproved for general-tree
   production implementation; do not silently copy or silently discard active
   configuration, and do not author a second resolver or config checker.
2. Exact-text approval of this bounded macOS regular-file tree contract. Rejecting
   all symlinks is deliberate for the first slice; a qualified installed graph
   requiring links remains blocked until its owner approves/proves a supported
   representation. Do not invent a flattening or resolver workaround.
3. KEL-79's renderer resource/origin implementation and Linux/Windows source-tree
   containment are separate owner decisions and unrun platform qualifications,
   not conditions this documentation may silently declare complete.

No production change, Drawio attempt, installation, product score or release is
approved by publishing this draft.

Current-documentation receipt (2026-10-31): Context7 discovery
`/oven-sh/bun` queried runtime configuration/preload/environment and path
remapping. Official Bun1.4.4 documentation confirms
[runtime preloads/configuration](https://bun.sh/docs/runtime/bunfig#preload),
[automatic environment-file loading](https://bun.sh/docs/runtime/environment-variables),
and [package/path resolution](https://bun.sh/docs/runtime/module-resolution).
Supported claim: these are active startup/resolution inputs, not inert copied
data. No new loader mechanism is selected; the owning qualification is unknown.

## 11. Private graph-bound macOS first-proof slice (2026-11-01)

The scoped [GH499 graph decision](https://github.com/gyldlab/keld/issues/499#issuecomment-6556551302)
and [integration follow-up](https://github.com/gyldlab/keld/issues/499#issuecomment-6557697903)
authorize an explicit Rust-only `stage_dev_boot_with_approved_graph` caller.
The [entry-path correction](https://github.com/gyldlab/keld/issues/499#issuecomment-6557906883)
preserves the actual relative Guardian argument. These are owner-delegated
contract decisions, not original-application, release or other-OS qualification.

The existing CLI stage transaction remains the sole producer. Before allocating
its private nonce, it compares the approved root package and real npm lock bytes
and all three installed runtime providers against independent packed-artifact
expectations: `electron` (package name `@keld/electron`), `@keld/api`, and
`@keld/kipc`. The expected provider fingerprint is SHA-256 over each sorted
package-relative UTF-8 path, NUL, decimal size, NUL, raw file digest, NUL. The
expectations are private caller inputs; the application-authored graph cannot
provide its own trust root.

Copy the admitted regular-file source from one retained no-follow root. Check
parent and leaf device/inode/type/timestamps before and after capture, compare
the streamed destination bytes, and repeat the complete physical census before
publication. Reject symlinks, special files and empty directories not implied by
captured file paths. Do not copy `.git`, `.keld`, or `keld.config.ts`; permissions
remain separately captured by their existing owner. This is a cooperating-user
development integrity boundary, not resistance to arbitrary same-user mutation
after the final check or signed-release authenticity.

The fixed `keld.stage-graph.json` has exactly three fields: `schema` with value
`keld.stage.graph/v1`, portable relative `entry`, and sorted `files`. Each file
has exactly `path`, `size`, and lowercase prefixed SHA-256. Bounds are 8 MiB for
the graph, 32,768 regular files, 65,536 census entries, 64 path components,
4,096 UTF-8 path bytes, and 4 MiB per package/lock/config JSON. File copying and
hashing use a 64-KiB buffer. The shared Guard-owned strict JSON parser refuses
duplicate package/lock keys. Reject nested alternate runtime providers,
unapproved package imports and runtime/path configuration, missing/unlisted
files, and mismatched size or digest. The three-field graph has no extra
permission, profile, source-provenance or provider-approval fields.

Schema five extends only the existing explicitly profiled facade. Its graph
binding names the fixed leaf and hashes its exact raw bytes. Profile authority
still comes only from the verified permissions snapshot. Host uses its retained
stage root for the descriptor, entry, policy and graph. Guardian V2 receives
the graph digest after authenticated registration and rechecks the same graph
before every prepared Bun generation. Private argv[3] remains relative and
must equal the graph entry; absolute aliases and traversal are refused.

For these unpublished local first-proof packages, the owned installer may use
exact `file:.keld/approved-artifacts/<package>.tgz` selectors. Root dependency,
actual npm lock selection, installed package identity and independently packed
provider bytes must agree. Generate the lock with npm; never hand-edit it to
fake a registry alias. `.keld` holds build inputs only and is excluded from the
runtime stage. A `file:` prefix or integrity-looking string alone grants no trust.

Every graph generation preloads the actual admitted
`node_modules/@keld/kipc/transport.js` before application dependencies. The
installed SDK imports that same module; no copied second transport is created.
Historical schema-one-to-four sidecar selection and environment/no-install
rules remain unchanged. Native resource consumers obtain a retained code-root
descriptor and immutable asset facts from the same graph off the UI thread;
those accessors alone do not implement resource delivery or preload execution.

The bounded fixture `graph_stage.rs` packs the actual three SDK providers,
installs them offline, then drives shipping Host/Guardian G1 and G2. Its early
application dependency changes `DataView`; the test failed with one hook before
the installed transport preload was wired and passes with zero afterward.
A separate native case changes one captured asset after G1 and proves Guardian
refuses the successor before its application entry connects. These are authored
framework fixtures, not draw.io. Full original launch/open/edit/save/reopen and
unsaved-close acceptance remains separately required.
