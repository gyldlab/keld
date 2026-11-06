# Spec: Host-sourced renderer process facts for Zettlr
Status: draft
Linear: GH-571 (#517) · Owner: @0monish (exact contract approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Run the selected unmodified Zettlr bundled preload with truthful platform, version, architecture, uptime and system-version
values supplied through the existing Host/session-facts and preload owners. Real preload and page consumers must observe the
same approved facts.

No renderer environment allow-list, new bundler, sendSync, dropped-file paths, arbitrary Node globals, dynamic contextBridge
approval, or whole-Zettlr feature claim. Original preload execution and page publication are separate acceptance layers.

## 2. Spec refs

- [Owning issue GH-571](https://github.com/gyldlab/keld/issues/571); current public owner decisions remain authoritative.
- [Host app-facts owner](https://github.com/gyldlab/keld/blob/c2dd2ff53767adf20dc0e91d276310095fe31acf/docs/specs/gh452-app-session-facts.md).
- [Main-role process mapping owner](https://github.com/gyldlab/keld/blob/9b6b4b495d1e8d060cc8a189ade9cc87c23edd0e/docs/specs/gh570-main-role-process-facts.md).
- [Existing macOS preload contract](https://github.com/gyldlab/keld/blob/0ad15d8df9897620bcad9c923b9b3f6aa224a982/docs/specs/gh484-macos-app-preload.md).
- [Preload conformance owner](https://github.com/gyldlab/keld/pull/1165).
- [Native bridge owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel142-macos-renderer-bridge-api.md).
- [Actual Zettlr preload](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/common/modules/preload/index.ts).
- [Actual Forge preload entries](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/forge.config.js).
- [Pinned Electron process API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/process.md).

Verified source baseline: Zettlr's pinned preload copies platform/version/versions/arch, exports uptime as a function that
calls process.uptime(), and calls process.getSystemVersion() immediately while building the exported object. Its page
getSystemVersion property is therefore a string, not the original method. Forge points multiple window entries at the shared
preload source. That configuration proves source selection, not a verified emitted bundle. GH570 currently scopes main-role
identity; GH452's partially approved facts do not automatically approve these additional fields.

The proposed contract below is future work. This publication changes no runtime boundary and grants no implementation or
predecessor approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given two independently supplied Host fact fixtures, when the actual admitted bundled preload runs, then
platform/version/arch equal the corresponding approved Host payload values and change when that payload changes.

Independent negative control: Hardcode any renderer-side value; the fixture with a different Host value must fail the exact
equality oracle.

### AC2

Given an admitted window's document-start initialization, when the original preload evaluates its process exposure, then
getSystemVersion() returns a qualified non-empty string synchronously and the page's exported value matches that string.

Independent negative control: Remove the method, return a Promise, or replace the preload with a manually constructed process
object; each must fail the original-call observation.

### AC3

Given the actual exported uptime function, when called at two Host-observed elapsed instants, then it returns finite
nonnegative seconds under the approved epoch/clock mapping without a synchronous cross-process wait.

Independent negative control: Return a frozen initial uptime or reset its epoch after an unrelated Bun restart; the
independently bracketed elapsed-time check must fail.

### AC4

Given process.env in the preload and its copied page object, when inspected, then both have zero own keys and no KELD_* value
is reachable.

Independent negative control: Copy the Bun environment or add one of the five deferred Zettlr env names without its trigger
and approval.

### AC5

Given the selected emitted bundle and its source map/build receipt, when import diagnostics run, then erased type-only
imports produce no runtime unsupported-import diagnostic; unresolved runtime imports remain typed refusals.

Independent negative control: Scan the TypeScript source instead of the emitted artifact and diagnose an erased type import.

### AC6

Given a missing, malformed or unsupported required Host fact, when initializing the preload, then initialization fails with
the shared typed diagnostic before falsely publishing a complete process surface.

Independent negative control: Fill missing version/systemVersion with host executable metadata, userAgent parsing, an empty
string or a guessed constant.

### AC7

Given navigation, destruction or another window, when a previously exposed method is used, then current-document routing and
teardown follow the existing bridge owner; stale functions cannot gain new Host authority.

Independent negative control: Reuse an old window/document's authenticated fact channel or attach a new Host operation to the
public page wrapper.

### AC8

Given a supported engine lane, when conformance is published, then the actual bundled preload, original callable
registration, page publication and these five values each have separate recorded results.

Independent negative control: Mark all process rows PASS from a synthetic app-world object or a static page wrapper while
original preload execution is suppressed.

## 4. Design

### 4.1 Fact provenance and ownership

GH452 supplies the Host-minted fact transport; GH570 owns Bun/main mapping; GH484 and each engine's preload owner install the
app-world process object. Add missing field definitions through those owners, not through a GH571-only payload or mirrored
constants. Keep platform, architecture, runtime version and OS version distinct. In particular process.version is not the app
version or ELECTRON_VERSION_SHIM; its selected runtime meaning must be frozen with GH570.

### 4.2 Capture and synchronous shape

Capture stable platform/architecture/version/system-version facts before admitting this window's preload. Validate and freeze
an own-data snapshot before executing the original bundle. getSystemVersion reads that snapshot synchronously. Nothing awaits
a Host request on the first-inline path. The public page object is built by the real preload through its approved publication
owner; this spec does not broaden the draw.io-only fixed method shape to Zettlr.

### 4.3 Uptime is a clock contract

Proposed mapping: the existing Host/window owner records the renderer/document uptime epoch and a monotonic base before
preload, then exposes elapsed seconds through its qualified engine clock. The method must advance and never use wall-clock
jumps. A new document has the owner-approved reset semantics; replacing the Bun main does not silently change a surviving
renderer's epoch. Cross-process monotonic clocks must not be subtracted without a qualified mapping. Exact epoch, suspend
behavior and Electron divergence remain T1 approval gates.

### 4.4 Wrapper and bundle reuse

Reuse the approved function-wrapper arguments, module map, isolated app world and document eligibility rule. Run Zettlr's own
emitted preload bytes without rewriting its call sites. Resolve the bundler's actual emitted path, hash, source and lock
before execution; a shared source entry is not proof every window's bundle is identical. Reject missing runtime modules
through the existing diagnostics owner. No Keld build/migrate implementation is introduced by this slice.

### 4.5 Empty environment and trust

The existing empty environment contract remains. PANDOC_VERSION, QUARTO_VERSION, GIT_VERSION, ZETTLR_IS_TRAY_SUPPORTED and
ZETTLR_TRAY_ERROR remain deferred until a committed reader cell triggers a separately approved allow-list. Page copies cannot
mutate the original facts. A process fact, raw page message or function name is not a principal, token or permission.

### 4.6 Lifetime, failure and platform qualification

Initialization is all-or-fail for this five-field tranche; failure never publishes plausible partial success. Existing native
document identities own revocation and callback lifetime. macOS is the first real lane; WebKitGTK and Windows require their
own approved isolated-preload/publication routes and native receipts. An old GH484/GH570 draft or partially approved GH452
slice is not implicitly approved by this document.

## 5. Boundaries

Future changes belong to the current Host fact schema/mapping, existing engine preload installer and GH567 conformance rows
after joint owner approval. No new global process shim, public wire decoder, environment grant, page native endpoint or
unrelated Zettlr compatibility surface.

This publication changes only `docs/specs/gh571-renderer-process-facts.md` and related grouped-PR/issue handoffs. Shared
architecture, policy, generated contracts, implementations and other owners' documents are not edited.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Freeze field-by-field Host provenance, process.version meaning, uptime epoch/clock and exact predecessor revisions; amend only the owning facts/preload contracts and add pending conformance rows.

### T2

- [ ] Implement stable fact capture and typed missing/malformed refusal through that existing transport; exercise two different Host snapshots with the real unmodified bundle.

### T3

- [ ] Implement and qualify the synchronous system-version method and advancing uptime function, then verify real original page publication through the existing approved bridge route.

### T4

- [ ] Run emitted-bundle import diagnostics and per-engine native/conformance evidence, keeping env empty and unsupported lanes explicit.

## 7. Test plan

AC1/6 are exact independent Host-payload comparisons. AC2/8 observe both actual original preload execution and the page's
exported string/method identity; suppressing preload must fail even if a static wrapper is visible. AC3 brackets calls with
an external Host monotonic observer and approved tolerance, not fixed sleeps or a copied clock formula. AC4 checks both
realms. AC5 uses the emitted artifact. AC7 uses native navigation/destruction barriers. Use an explicit call and completion
beacon rather than treating navigation completion as proof of preload success.

Each acceptance criterion has its own observable and negative control above. Documentation verification covers all ten
sections, source-versus-proposal wording, file limits and exact GitHub readback. GitHub CI belongs to the exact published
head. Native/runtime/conformance/OS outcomes remain unrun until the implementing owner executes them; green documentation
checks do not prove those outcomes.

The user restricted this session to GitHub. Local `just ci`, hooks and machine session-closeout were not run. Official
immutable upstream GitHub sources are used where relevant; Context7 was not used under that source restriction. No
external-machine or native receipt is fabricated.

## 8. Review gates triggered

Public API, fact-schema/wire ownership if extended, and native preload/security review for any new publication route. No new
dependency is proposed. Existing unsafe allowances are not expanded by this draft.

Draft publication is not exact-content approval. Pending predecessor approvals and native qualification remain separate
gates.

## 9. Perf impact

Stable facts are captured once per admitted document and remain bounded by the existing launch-fact budget. Uptime has no
per-call IPC or background sampler. No startup-speed claim or measured timing result is made.

## 10. Open questions (block implementation)

1. GH452/GH570 owners must approve missing platform/version/arch fields, exact version meaning and their transport without
expanding previous partial approvals.

2. Approve and qualify the renderer uptime epoch, clock mapping and suspension semantics on each claimed engine; do not
invent an Electron-compatible clock.

3. GH484/GH467 and platform owners must approve actual Zettlr preload registration/page publication beyond the current
draw.io subset.

4. Obtain the selected emitted bundle and immutable build receipt, then run its original callers on a real engine;
configuration alone is insufficient.

Independent review must cover the final published revision. Every remaining owner/native gate stays explicit; this document
is not a completed feature.

