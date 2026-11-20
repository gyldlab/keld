# Spec: Legacy watcher and Zettlr filesystem qualification

- Status: draft
- Tracker: GH-1355
- Owner: Monish Khandelwal; runtime and Zettlr evidence owners retain their scope
- Updated: 2026-11-06
- Keld source: `0eb3ce203187f35c5276a4237189c906a5220d5a`

## 1. Goal & non-goals

Qualify the actual filesystem watcher used by Zettlr under a declared Legacy Bun run.
A passing watcher experiment must demonstrate real file events and clean closure.
A product claim additionally requires the original Zettlr consumers to react correctly.
A library import, emitted log or copied suppression model cannot qualify that product claim.

The initial question retains Bun 1.4.2. Any current shipping Bun revision is a separate arm.
Freeze the exact executable digest, revision, dependency lock and fixture source for every arm.
No result transfers across those identities or from bare Bun to a Host-supervised Legacy run.

Non-goals:

- Implementing a watcher replacement, strict-profile filesystem facade or new process model.
- Changing the global Bun pin, KEL-78 admission, permissions, installed packaging or KEL-74 scoring.
- Claiming Windows/Linux watcher behavior from a macOS run.
- Running an experiment, accessing a user machine or changing application code in this documentation PR.
- Treating `watchdog.activatePolling` as proof that the underlying watcher uses polling.

## 2. Spec refs

Governing Keld sources at the header pin:

- Architecture 06 section 1 owns Bun's versioned process contract.
- Architecture 04 sections 4 and 5 separate operation evidence from package-name support.
- KEL-74 owns `keld.compat.evidence/v1`; GH1354/GH1358 own Zettlr product outcomes.
- KEL-78 and GH586/GH588 own declared authority, authentication and runtime receipts.
- KEL-77's child-process differential spec owns a different operation family. Reuse its
  subprocess/evidence utilities only where their contracts apply.
- GH503's native-addon classification does not qualify this pure-JavaScript watcher.

### Source-backed starting point

The Zettlr pin is `04128af42b58f7fea3f39f7b1efd597f5021f48a`.
Its lock resolves chokidar 5.0.0 and readdirp 5.1.1. [Z1]
The complete `FSALWatchdog` implementation creates a real `FSWatcher`, forwards
`all` events, exposes watched paths, and awaits `close()` during shutdown. [Z2]

The constructor uses persistent watching, `ignoreInitial:true`,
`followSymlinks:false`, `ignorePermissionErrors:true`, and 5,000 ms interval fields.
When `watchdog.activatePolling` is true it adds `awaitWriteFinish`, with a
1,000 ms fallback stability threshold and 100 ms size-check interval.
It does not set `usePolling`. The default configuration sets the switch false. [Z2][Z3]

Chokidar's pinned constructor can override polling settings from
`CHOKIDAR_USEPOLLING` and `CHOKIDAR_INTERVAL`.
Its `awaitWriteFinish` and `usePolling` are separate options. [C1][C2]
Record those environment values and resolved options. A source comment naming FSEvents
is not an observed backend receipt.

Zettlr has distinct watcher owners. FSAL maintains root watchers and forwards changes
through its own event/cache path. DocumentManager creates another watcher, registers
open file paths, suppresses one matching self-save change, handles external changes,
and closes unlinked files in all leaves. [Z4][Z5]
The prior [scratch claim][H1] explicitly described a suppression model and an experiment
not yet run. It is not a completed real-DocumentManager result.

## 3. Acceptance criteria

### AC-1: Reject evidence from a different input identity

- **Given:** independently hashed runtime, lock, source, fixture and mode records.
- **When:** the driver starts each reference, bare-Bun or Host-supervised arm.
- **Then:** observed executable/version/configuration match that arm's manifest.
  A mismatch produces an explicit failed admission; no operation receives a pass.
- **Negative control:** substitute a different Bun executable while retaining the expected
  record. The independent executable/version comparison must fail before filesystem probes.

### AC-2: Prove a real watch is active before scoring events

- **Given:** the actual pinned watcher, a fresh owned directory and installed observers.
- **When:** add the path, observe initial readiness and perform a distinct canary write.
- **Then:** the actual watch census contains the path and the canary produces its expected
  real filesystem event. An ignored permission error or an unwatched path cannot pass.
- **Negative control:** omit the real `watchPath`/add call while keeping registration logs.
  The canary event is absent and the driver fails the same expectation used by the positive arm.

### AC-3: Deliver add, in-place change and unlink from real filesystem operations

- **Given:** AC-2 has passed for a unique Markdown fixture.
- **When:** a separate fixture actor creates it, writes distinct bytes in place, then removes it.
  Each action waits for its prior observation before the next action starts.
- **Then:** the observer records the corresponding path and event category.
  Independent file reads/digests and final absence establish the actual effects.
  In the FSAL integration arm, its descriptor/change or unlink consumer also observes them.
- **Negative control:** drop only the change forwarding callback after the canary.
  The independent change expectation fails although add/unlink and filesystem writes still work.

### AC-4: Self-save suppression does not hide the next external edit

- **Given:** the actual DocumentManager owns an open file and uses its original save path.
- **When:** save a unique local edit, observe the real underlying change, then make a later
  independent edit whose bytes differ and whose observed modtime exceeds the stored value.
- **Then:** the self-save consumes its intended suppression entry.
  The later external change reaches the original handler and the expected reload or prompt.
  Missing underlying events cannot count as successful suppression.
- **Negative control:** retain the suppression entry instead of consuming it.
  The later external edit then lacks its required handler/app outcome and the test fails.

### AC-5: Watched-path changes reach the correct application document

- **Given:** two same-basename files in different directories are open in distinct leaves.
- **When:** change one file externally, then unlink that file.
- **Then:** the changed file's content or prompt is attributed to its full path.
  Its unlink closes that file in every affected leaf and removes it from the document set.
  The other file retains its bytes and open state; the watch census follows the live set.
- **Negative control:** route the event to the other same-basename path.
  The expected file remains stale or open and the neighbouring document changes incorrectly.

### AC-6: Optional write-stability mode is qualified without inventing a backend fallback

- **Given:** separate manifests select the default branch and the explicit stability branch.
- **When:** construct the real wrapper with the selected settings and perform a chunked write.
- **Then:** record the resolved options and actual events separately.
  The stability arm uses the selected `awaitWriteFinish` parameters and eventually exposes
  the final bytes. Report `usePolling` and backend attribution independently.
- **Negative control:** replace only the stability branch with the default options.
  The option-record comparison fails even if a fast final write happens to produce the same event.
- The default arm is required first. An unrun optional arm stays unknown and is not advertised
  as a qualified recovery path.

### AC-7: Awaited closure releases the actual persistent watcher

- **Given:** the experiment child has a live watcher proven by AC-2 and no unrelated keepalive.
- **When:** invoke the original `shutdown()`, await completion and stop test-owned observers.
- **Then:** a parent observer records natural child exit before its fixed deadline.
  Forced termination, an unawaited close or a process still alive is failure.
- **Negative control:** omit the underlying `close()` while retaining the live persistent watch.
  The natural-exit expectation must fail; a forced cleanup is recorded only as cleanup.
- Product shutdown additionally inventories each actual watcher owner. Closing the
  DocumentManager watcher alone does not prove the FSAL root watcher set was retired.

### AC-8: Preserve mode, authority and incomplete outcomes in the evidence

- **Given:** a bare-runtime success, a failed event arm and a claimed Host-supervised run.
- **When:** the existing evidence producer validates their records.
- **Then:** each result retains exact runtime/source/configuration, operation, mode and observations.
  A bare-runtime record has no invented Legacy receipt. Missing product, backend or optional-mode
  evidence stays unknown. No watcher pass fills an installation or unrelated product cell.
- **Negative control:** relabel the bare-runtime record as declared Legacy without a trusted
  Host admission witness. The consuming evidence validation must reject that promotion.

## 4. Design

### D-1: Independent atoms and reuse

| Atom and owner | Boundary | Observable / falsifier |
|---|---|---|
| Input identity, driver | Chosen artifacts to executed arm | Version/digest mismatch; AC-1 |
| Watch admission, actual watcher | Added path to live watch | Canary and census; AC-2 |
| Event delivery, FSAL | External action to consumer event | Independent disk effect; AC-3 |
| Caller semantics, DocumentManager | Save/event to app state | Next external marker; AC-4/AC-5 |
| Configuration, wrapper | Captured settings to actual options | Distinct option records; AC-6 |
| Lifetime, watcher and parent | Awaited close to child exit | Independent process lifetime; AC-7 |
| Provenance, evidence owners | Observations to scoped claim | No mode/profile promotion; AC-8 |

No boundary change to shipped APIs or process authority is proposed.
Reuse the real wrapper and consumers, the existing subprocess lifecycle utilities,
the current KEL-74 schema and GH1354 product collector.
A copied event loop, replacement suppression implementation or second scoreboard is rejected:
it could pass while the actual Zettlr caller fails.

### D-2: Two qualification layers

The isolated layer runs the exact watcher and wrapper with real temporary files.
It may use minimal logger/config dependencies but cannot replace the watcher, event delivery,
filesystem calls or closure. Observe the underlying watcher before adding paths.
The `all` event omits `ready` and `error`; the driver must observe those separately. [C1]
The wrapper's synchronous `unwatchPath` must not be mistaken for an awaited completion promise. [Z2]

Use a known initial scan/readiness event and a real canary. No fixed sleep establishes readiness.
Missing or inaccessible watched resources yield a failed event expectation.
The driver must prove its missing-event detector fails on the same path as the positive check.

The product layer uses the actual staged Zettlr application under explicit Legacy.
It records the original DocumentManager callbacks and independent file/window consequences.
A listener spy can observe the real call but cannot inject a synthetic event or edit the store.
The prior one-token model may remain diagnostic evidence; it cannot discharge AC-4 or AC-5.

The issue's Bun 1.4.2 arm is retained as a named historical target.
A proposed shipping revision, including the current Keld pin if different, requires separate runs.
An Electron reference freezes its real embedded Node/runtime identity.
An additional standalone Node comparison is diagnostic unless its equivalence is established.

### D-3: Scheduling, event meaning and independent expectations

Use sequential action barriers, full paths and unique content markers.
Do not require a universal one-event-per-write count: native events can coalesce.
Instead bind each tested action to its independently expected semantic outcome and transcript.
Duplicate callbacks, wrong targets, lost required events and error records remain visible.

For external reload, freeze the application's `alwaysReloadFiles` setting and dirty state.
Its handler checks modtime and content before prompting/reloading. [Z5]
Read those preconditions independently before requiring a visible result.
Do not silently manufacture them by writing the document store or calling the handler directly.

Proposed experiment bounds: 20 seconds per awaited positive event or close observation,
120 seconds per child, 16 owned files and 4,096 retained event records per child.
These are test kill switches and retention limits, not latency promises.
Overflow or deadline expiry is failure with the original transcript retained.
The runner clears its own timers/listeners on completion; they cannot keep the child alive.

The stability branch uses the pinned wrapper's configured threshold, including its documented
source fallback for non-number or negative thresholds. Do not infer constant-content stability from file size alone.
A candidate forced-polling workaround needs its own source change, owner decision and repeat
qualification; changing the configuration label alone supplies none.

### D-4: Authority, lifecycle and evidence

Bare runtime tests have ambient user authority and do not mint Keld principals.
Host-supervised tests consume the existing declared Legacy admission and same-generation
trusted receipt. Legacy still forfeits strict OS containment.
No new grant, wire message, profile field, parser or launcher is introduced.

Capture environment options before watcher construction. Record any relevant override.
Do not infer native backend from a comment, platform name, library version or a passing event.
Keep backend unknown unless an independent process-scoped observation identifies it.

Use the existing event/evidence value/error vocabulary.
A missing fixture, unreadable source, runtime mismatch, event timeout, watcher error or cleanup
timeout is a distinct failed observation. Reserve any new stable diagnostic through its owner.
The product collector, not a printed application line, decides whether its receipt qualifies.

Runtime seam: none in production. Fixtures invoke the actual existing library/app seams.
Migration unit: watcher setup, FSAL root consumers, DocumentManager save/change/unlink,
watch-set maintenance, shutdown and evidence consumer. No temporary shipping adapter.

## 5. Boundaries

Future isolated fixtures belong with the existing runtime differential/compatibility test owners.
GH1354 retains actual Zettlr stage and product-observation ownership.
Select concrete test-module paths with those owners in T-1; do not add a second generic harness.

This change adds only this specification.
Do not edit KEL-77's child-process oracle, KEL-74 schemas, global Bun pins, production FSAL,
Keld admission, install packaging, strict filesystem policy, root instructions or shared CI.
Any application/runtime defect found later goes to its owning issue before a fix is selected.
An upstream workaround is an explicit divergence, not a hidden test-only substitution.

## 6. Tasks

### T-1: Freeze fixtures, expectations and observation interfaces

- **Blocked by:** B-1.
- Pin runtime/dependency/source identities and selected mode/settings.
- Commit independently authored expected paths/content/state and the same missing-event check
  used by positive and negative arms. Preserve the historical scratch claim as a distinct lead.
- Obtain scoped operator/test-fixture authority and freeze the finite limits before execution.
- Register AC-1 through AC-8 before claiming any observed pass.

### T-2: Qualify the real isolated watcher on macOS

- **Blocked by:** T-1 and B-2.
- Execute AC-1/AC-2/AC-3/AC-6/AC-7 against actual files and the real wrapper.
- Record natural exit, no-op/drop/close mutants and every failure.
- Produce a bounded mechanism result. Do not wait for the full Zettlr app to qualify first.
- This task can resolve B-3's mechanism input; it cannot qualify the product consumer.

### T-3: Qualify the actual Zettlr consumers under declared Legacy

- **Blocked by:** T-2 and B-3.
- Through the existing product lane, exercise real self-save, external edit, same-basename unlink,
  watch-set maintenance and shutdown. Verify AC-4/AC-5 and applicable AC-1 through AC-8.
- Preserve framework, application and watcher failures as separate observations.
- Publish source-bound product evidence to GH1354/GH1358 without changing the denominator.

### T-4: Record dispositions and qualify any selected shipping revision

- **Blocked by:** T-2; product claims additionally require T-3.
- Validate exact version/profile/mode evidence through the existing consumer.
- Rerun the same applicable cases for a different selected Bun revision; do not transfer passes.
- Record supported, divergent or unknown outcomes and each required follow-up.
- An optional stability/fallback mode is claimable only after its own run; no automatic activation.

## 7. Test plan & evidence

| Criterion | Planned proof | Independent observation |
|---|---|---|
| AC-1 | Runtime/source admission | Executable version/digest and frozen lock |
| AC-2 | Actual watcher admission | Census plus live canary |
| AC-3 | File-operation integration | Event path/category and disk bytes/absence |
| AC-4 | Real app self-save/external-edit sequence | Underlying event, suppression and later consumer effect |
| AC-5 | Real multi-document integration | Target content/closure and unchanged neighbour |
| AC-6 | Separate option arms | Resolved actual options and final written bytes |
| AC-7 | Isolated child plus product owner census | Awaited close, natural exit and final process observation |
| AC-8 | Existing evidence consumer | Refusal of wrong mode/profile and unrelated cell promotion |

Use barriers and bounded deadlines, not sleep-based readiness.
Restore each mutant independently, rerun its positive arm and retain both outputs.
The same transcript parser must score ordinary and intentionally missing events.

Evidence available for this draft is GitHub source/tracker inspection on 2026-11-06.
No native experiment, application acceptance, formatter, Clippy or workspace test ran here.
The old scratch claim is attributed history, not a result produced or verified by this PR.

### Current-documentation receipt

- Applicability: pinned watcher options, event/readiness API and closure semantics.
- Context7: not used; this task is explicitly limited to GitHub sources.
- Official primary: Zettlr `04128af4`, chokidar 5.0.0 source `c0c8d20e`; links below.
- Supported claim: real source call paths, lock versions and separate stability/polling controls.
- Fallback/blocker: actual backend, macOS behavior and product qualification remain unrun.

## 8. Review gates triggered

This documentation-only proposal changes no production boundary.
Future fixture work reuses existing interfaces and evidence formats.
Public API or evidence-format changes require their owners' separate approval.
Permission model review verifies truthful Legacy provenance and no strict claim.
Unsafe and dependency additions: none proposed. Wire protocol: none proposed.
A new dependency or native observation mechanism is a new reviewed scope, not granted here.

## 9. Perf impact

No performance improvement or watcher-latency guarantee is claimed.
Measure event elapsed times and retained fixture resources as observations.
Keep the existing native watcher path as the baseline.
Any polling workaround needs measured CPU/resource impact and its own accepted tradeoff.

## 10. Open questions, blockers & unknowns

### B-1: Exact specification and fixture ownership approval

- **Status:** open.
- **Owner:** Monish Khandelwal with GH1355/GH1354 runtime, application and test owners.
- **Impact:** blocks fixture implementation and any new experiment under this draft.
- **Check:** approve exact text, test locations, selected runtime arms, limits and native scope.
  The current docs-only claim grants none of those implementation or operator actions.

### B-2: Actual macOS experiment environment

- **Status:** open.
- **Owner:** separately claimed macOS operator.
- **Impact:** blocks T-2 and native mechanism claims, not this planning document.
- **Check:** retain exact runtime/dependency artifacts, writable owned fixtures and independent
  process/event observers. Do not replace an installed runtime or reuse unverified old receipts.

### B-3: Real product prerequisites and observability

- **Status:** open.
- **Owner:** GH1354/GH1358 with GH547 and the shared profile/loader/lifecycle owners.
- **Impact:** blocks T-3 and Zettlr product claims, not isolated watcher qualification.
- **Check:** provide a runnable explicit-Legacy stage and real open/save/invoke consumers plus
  original-handler/file/window observers. T-2 supplies mechanism evidence; T-3 supplies later app proof.

### U-1: Backend, root-watch cleanup and runtime-specific event behavior

UNKNOWN: actual native backend, event timing/coalescing, complete product watcher retirement,
and behavior on any unrun runtime/OS/configuration.
FSAL's shown shutdown persists its cache; that source statement alone proves neither that all
root watchers are closed elsewhere nor that they leak in a full app run. [Z4]
Resolve ownership and observe each live watcher in T-3; do not invent a passing close receipt.

### U-2: Source and installed-app transfer

The issue's older Zettlr pin and the selected app stage may differ outside the watchdog file.
Record the full caller delta before transfer. Installed qualification remains with packaging
and product owners; this development/watch contract cannot fill an installation cell.

[Z1]: https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/yarn.lock
[Z2]: https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/fsal/fsal-watchdog.ts
[Z3]: https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/config/get-config-template.ts
[Z4]: https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/fsal/index.ts
[Z5]: https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/documents/index.ts
[C1]: https://github.com/paulmillr/chokidar/blob/c0c8d20e49d337491891078d1081bf91bd178de6/README.md
[C2]: https://github.com/paulmillr/chokidar/blob/c0c8d20e49d337491891078d1081bf91bd178de6/src/index.ts
[H1]: https://github.com/gyldlab/keld/issues/1355#issuecomment-6639619523
