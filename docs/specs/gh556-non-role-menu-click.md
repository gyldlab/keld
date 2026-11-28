# Spec: macOS non-role application menu selection

- Status: draft
- Tracker: GH-556
- Owner: Monish Khandelwal (@0monish, approval pending)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Deliver a genuine native application-menu selection to its original Bun callback once during a healthy admitted run.
Preserve the menu item, action-time focus, modifier flags and state ordering. Use the existing role link and native menu owner.

This is a future macOS specification. It grants no runtime approval or machine claim.
Exclude popup menus, Tray, global shortcuts, arbitrary native selectors, Windows/Linux qualification and generic Electron parity.
Do not repeat GH480 role actions in Bun. Do not authorize the selected callback's later filesystem, shell or renderer operations.

## 2. Spec refs

FACT: At the header pin, `packages/@keld/electron/src/native-surface.test.ts:44` tests the absence of template construction.
`packages/@keld/electron/src/native-surface.test.ts:55` tests a missing MenuItem constructor. Neither test runs an application menu.
`crates/keld-compat/tests/support/corpus_manifest.rs:306` identifies the existing native conformance owner.
`docs/specs/gh508-kipc-channel-table.md:588` distinguishes guarded tree submission from future activation delivery.
`docs/specs/gh531-window-registry-close-state-machine.md:605` gives a window reference both identity and incarnation.

FACT: #1166 `docs/specs/gh480-guarded-macos-application-menu.md:22@83ded4df3c2355d69bf88d6f4202749febd40eb1` leaves non-role interaction to GH556.
#1166 `docs/specs/gh480-guarded-macos-application-menu.md:238@83ded4df3c2355d69bf88d6f4202749febd40eb1` owns atomic native replacement and retirement.
#1166 `docs/specs/gh480-guarded-macos-application-menu.md:444@83ded4df3c2355d69bf88d6f4202749febd40eb1` requires root and nested menus to preserve Host-controlled enabled state.
Its native probes do not qualify this future activation path.

FACT: [GH556](https://github.com/gyldlab/keld/issues/556) is the tracker. [GH1358](https://github.com/gyldlab/keld/issues/1358) maps GH480 then GH556 for Zettlr Save.
[GH1354](https://github.com/gyldlab/keld/issues/1354) owns that application proof. The next-milestone implementation hold remains.

## 3. Acceptance criteria

### AC-1: Admit a distinct authenticated activation path

- **Given:** An application role installed an authorized menu. A renderer can submit a similar-looking message.
- **When:** Each origin attempts to enter menu activation dispatch.
- **Then:** Admit only the approved Host-origin event on the authenticated role link. The guarded install call cannot carry a click backward.
- **Negative control:**
  - **Single fault:** Admit the guarded-install receive class as an activation source.
  - **At-risk precondition:** Present an otherwise well-formed selection at the direction/class validator. Hold later dispatch disabled.
  - **Expected RED:** The validator admits a forbidden class. Its owner-local disposition fails independently of later callback defenses.

### AC-2: Resolve the original item and action-time window

- **Given:** Window A has focus when a native non-role action begins. Window B also exists.
- **When:** Shift focus to B before Bun delivery. Separately retire A and create another window before delivery.
- **Then:** Resolve only A's captured incarnation when live. Never substitute B or a recycled window. Apply the B-2 stale/absent disposition.
- **Negative control:**
  - **Single fault:** Resolve focus at Bun delivery instead of consuming the captured window reference.
  - **At-risk precondition:** A and B remain live in the focus-change arm. The role and installed menu stay current.
  - **Expected RED:** The application callback receives B. The native action transcript names A.

### AC-3: Send the application callback signature and genuine event

- **Given:** One enabled normal item has a distinct callback. A real pointer and an approved key equivalent can select it.
- **When:** Select the item once by each route in separate attempts with known modifier states.
- **Then:** Call the captured options callback once per selection with the actual MenuItem, resolved window and event. Preserve the qualified event fields.
- **Negative control:**
  - **Single fault:** Pass the internal dispatch argument order to the application function.
  - **At-risk precondition:** A current window, actual menu item and observed event are all distinct. Other admission gates pass.
  - **Expected RED:** The callback's ordered identity and event assertions fail despite one invocation.

### AC-4: Commit checkbox and radio state before user code

- **Given:** The installed tree contains a checkbox, two radio items in one group and a checked item in another group.
- **When:** Click the checkbox twice. Select each radio, repeat its current selection and observe every callback entry.
- **Then:** Checkbox callbacks see consecutive opposite values. Each radio selection leaves exactly that item checked in its group. Preserve the other group.
- **Negative control:**
  - **Single fault:** Invoke the checkbox callback before applying its checked transition.
  - **At-risk precondition:** Start unchecked. The callback records its immediate read before making any mutation. The native action reaches Bun.
  - **Expected RED:** The first callback reads false even if a later native projection shows the correct mark.

### AC-5: Update the one Bun tree and native projection

- **Given:** A live installed tree contains a normal item, a checkbox and a nested submenu.
- **When:** Mutate checked, enabled, visible and label individually. Include a callback that changes its own checkbox again.
- **Then:** Synchronous reads show the actual Bun objects. The acknowledged native revision matches final values. Stale writes cannot overwrite newer revisions.
- **Negative control:**
  - **Single fault:** Apply an old checked-state projection after a newer acknowledged revision.
  - **At-risk precondition:** Both writes address the same live item. Set opposite values and release the held stale update last.
  - **Expected RED:** The UI owner's revision/effect observation accepts the old state or the native mark diverges from the Bun item.

### AC-6: Preserve effective eligibility for every input route

- **Given:** A nested item is disabled. Another hidden item has a shortcut and an explicit hidden-accelerator policy.
- **When:** Force native menu validation and exercise pointer and key routes. Probe the target boundary independently of the displayed state.
- **Then:** Disabled items cause no action. Hidden pointer targets are absent. Key eligibility uses the qualified macOS hidden-item rule in D-4.
- **Negative control:**
  - **Single fault:** Omit the effective-enabled check at the native action boundary.
  - **At-risk precondition:** A controlled native target invocation reaches the current disabled item. Role, grant, menu and command checks remain valid.
  - **Expected RED:** The action owner admits the disabled selection. Later Bun refusal cannot hide this failure.

### AC-7: Keep recognized roles on their existing path

- **Given:** A supported close-role item also carries an application click function. A separate normal item remains callable.
- **When:** Activate the role by pointer and its built-in shortcut. Exercise the existing veto case independently.
- **Then:** Only GH480/GH450 handle close. Observe one close request, zero custom callbacks and the established veto outcome. Normal selection still works.
- **Negative control:**
  - **Single fault:** Also emit a custom activation for the role item after its native role action.
  - **At-risk precondition:** An actual callback is installed and the role resolves. Observe at the native emitter before Bun filtering.
  - **Expected RED:** The role-action transcript contains a forbidden custom event, even if Bun rejects it.

### AC-8: Do not retarget a retired menu selection

- **Given:** A pending selection names A. A replacement B contains the same public item ID, label and installation ordinal.
- **When:** Deliver across replacement in both orders. Repeat across role retirement and recovery. Include duplicate delivery within a live generation.
- **Then:** Never invoke B for A. Reject retired or duplicate input at its owner. A new deliberate B action still works once.
- **Negative control:**
  - **Single fault:** Use only the installation ordinal at Bun dispatch after a menu replacement barrier.
  - **At-risk precondition:** Feed the previously admitted A envelope after B's barrier in the same live role. Its ordinal selects a valid B callback.
  - **Expected RED:** The Bun resolver accepts B for A. Observe resolution before invoking user code or applying later effect guards.

### AC-9: Keep public menu identity and default installation observable

- **Given:** Separate startup attempts have no explicit setter, an early custom setter, or an early explicit null setter.
- **When:** The qualified pre-ready default hook runs. Read getApplicationMenu and inspect the actual guarded submission.
- **Then:** Only the unset case builds the reviewed default tree and uses the normal install path. Return that object. Custom and null suppress defaults.
- **Negative control:**
  - **Single fault:** Build the default only inside the Host instead of invoking the ordinary facade path.
  - **At-risk precondition:** No explicit setter has run. Boot, profile and menu grant allow the default attempt.
  - **Expected RED:** The getter lacks the expected facade object or the normal install witness is absent despite a native menu.

### AC-10: Register custom accelerators without broadening authority

- **Given:** A bounded tree contains a supported custom shortcut, an invalid accelerator and a normal pointer-only control.
- **When:** Install it and exercise each selected input. Compare against the pinned reference and disambiguate collisions under B-2.
- **Then:** One Host parser registers only qualified shortcuts. An invalid shortcut yields the bounded diagnostic and no key binding. Preserve pointer selection.
- **Negative control:**
  - **Single fault:** Replace a rejected accelerator with a valid default chord.
  - **At-risk precondition:** That chord is otherwise unused. The item is enabled, authorized, visible and has a live callback.
  - **Expected RED:** The independent binding census or real key action identifies the forbidden fallback.

### AC-11: Bound deferred activations and recover without replay

- **Given:** The authenticated Worker is live while Bun's main thread is parked or a callback deliberately stalls consumption.
- **When:** Submit zero, exact-capacity and capacity-plus-one activations. Inject transport loss before and after delivery/consumption.
- **Then:** Bound retained bytes and records. Apply the approved overflow outcome. Never retry a possibly executed callback or block the UI waiting for Bun.
- **Negative control:**
  - **Single fault:** Append an activation to deferred storage before its admission reservation succeeds.
  - **At-risk precondition:** The queue is at its approved byte or record limit. The valid selection otherwise fits and reaches that owner.
  - **Expected RED:** The independent retained-object census exceeds the bound even if a later link failure clears the queue.

### AC-12: Exercise the actual application consumer

- **Given:** An approved Zettlr build uses its pinned macOS Save item. Two documents contain distinct dirty markers in separate live windows.
- **When:** Press Cmd+S with A focused. Follow the app's original callback and renderer save route.
- **Then:** Observe one shortcut for A and the expected change in A's selected file. Verify B's file remains unchanged.
- **Negative control:**
  - **Single fault:** Remove the genuine Save callback registration while leaving its visible item and accelerator intact.
  - **At-risk precondition:** The native selection reaches the handler boundary. The app has a writable target, a unique dirty marker and no alternate save trigger.
  - **Expected RED:** A's required file effect is absent. A visible checkmark, matched key or synthetic renderer response cannot satisfy the app oracle.

## 4. Design

### D-1: Ground the actual caller and reference behavior

FACT: These immutable upstream sources were inspected on 2026-11-06. They establish code demand and documented semantics only.

| Source | Inspected behavior |
|---|---|
| [Zettlr macOS menu](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/menu/menu.darwin.ts#L214-L219) | `Cmd+S` uses the supplied focused window to send the Save shortcut. |
| [Zettlr editor](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/win-main/MainEditor.vue#L113-L119) | The `save-file` listener calls the document provider. GH1354 owns the full product outcome. |
| [draw.io checkbox](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L1954-L1962) | The original callback reads the current item's checked value before updating a preference. |
| [draw.io installation](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js#L2063-L2073) | Disabling updates removes the update items before installation. Do not manufacture a checkbox for that run. |
| [MenuItem options](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/menu-item.md#L18-L24) | The options callback is `click(menuItem, window, event)`. A declared role takes precedence. |
| [MenuItem wrapper](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/menu-item.ts#L103-L114) | The internal invocation receives different arguments. It changes state before calling the options callback. |
| [Reference dispatch](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/menu.ts#L89-L100) | The dispatcher looks up the command, reads focus and invokes its wrapper. |
| [Radio setter](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/api/menu.ts#L314-L334) | The installed group's setter clears peers and selects the target. Repeating its click does not clear it. |
| [Keyboard event](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/keyboard-event.md#L3-L7) | The structure includes optional modifier booleans and `triggeredByAccelerator`. |
| [Default builder](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/default-menu.ts#L7-L24) | A prior setter suppresses defaults. The default uses normal template construction and installation. |
| [Bootstrap](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/lib/browser/init.ts#L175-L181) | Default dispatch is registered at `will-finish-launching`, before Ready. |
| [Menu getter](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/menu.md#L44-L60) | Null suppresses the automatic default. The installed getter does not promise structural live insert/remove. |
| [Accelerator parser](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/ui/accelerator_util.cc#L20-L69) | Non-ASCII input and a missing valid key produce diagnostics and refuse the binding. |

UNKNOWN: The [focus getter](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/base-window.md#L397-L399) documents `null`.
The callback docs describe an undefined window. Measure the actual no-window and open-but-unfocused callback values under B-2.
Do not convert a BaseWindow into a fabricated BrowserWindow. Scope the first product case to an actual registered BrowserWindow.

### D-2: Reuse the authenticated role, menu and window owners

Proposed private fields below describe obligations, not allocated channel IDs, public getters, wire tags or raw handles.

| Atom | Input to output | Owner and first failure witness |
|---|---|---|
| Installation | Frozen Bun tree to guarded native projection | GH480 validates exact `app.system` literal `menu`; denied or failed installation preserves the previous native tree. |
| Identity | Installed role/menu generation and native command to captured node | GH480 owns nonreused native commands. Match the accepted installation snapshot. AC-8. |
| Focus | Current native selection to action-time WindowRef or qualified absence | GH531 owns focus and incarnation. Reuse the facade's existing object. AC-2. |
| Delivery | Native input to one admitted Host event on the existing link | GH508 and GH527 own class, ordering, lifetime and bounded buffering. AC-1, AC-11. |
| State | Callback identity plus Bun item state to application call and projected revision | GH556 extends the existing GH480 objects. AC-3 through AC-5. |
| Evidence | Actual native selection to app callback and separate downstream effect | GH477/GH566 conformance and GH1354 product owners keep their separate claims. AC-12. |

Allocate no second event connection, policy parser, accelerator catalog, focus registry, main-thread pump or native menu owner.
The facade retains the actual item and callback object, not a serialized function. Distinguish public item ID, public commandId and private native command identity.
Never use a label, array index, application-provided numeric ID, renderer IPC or public MenuItem.click invocation as native authority.

Proposal: associate each native command with a validated node ordinal in one frozen installation snapshot.
The receiver resolves that ordinal only inside the authenticated role and acknowledged menu generation.
The Host maps current native command to ordinal. Bun maps that accepted snapshot to its retained original object.
Duplicate labels and repeated public IDs cannot redirect the native command. Never reinterpret an ordinal against the current public getter alone.

Use one owner-approved directional activation contract. Carry bounded installation identity, node ordinal, action sequence, optional window identity and captured event fields.
Host and Bun observe ordered install/retire barriers on the existing authenticated link. Admit no action for a partially materialized or unbound projection.
Reserve binding/publication order before enabling a new native target. Keep the old tree serviceable until the replacement contract reaches its commit boundary.
B-1 must resolve ordered receipt versus explicit binding acknowledgment using the actual transport owner. Do not assume FIFO across unrelated senders.

Capture the focused native window and modifier flags at action admission on the UI thread. A later focus change cannot retarget admitted work.
At Bun dequeue, validate role, install generation, command binding, sequence and qualified window lifetime before resolving the options callback.
A previously admitted event before its retirement barrier may address only its original item. After the barrier, refuse unstarted old work.
Do not claim a callback already executing can be recalled. Never retry it after an exception, missing acknowledgment or role death.
Repeated deliberate clicks have distinct sequences. Duplicate sequence delivery has at most one callback effect within the live generation.

### D-3: State and failure ordering

1. Validate input and reserve the bounded event before retaining additional work.
2. Resolve the original live item and captured focus. Apply the qualified absent/stale disposition.
3. For a checkbox, invert its current Bun value once. For a radio, select the item and clear its existing group peers.
4. Commit the Bun state before invoking the app. Reuse the same mutable MenuItem in the callback.
5. Publish the latest valid state to the native projection through the existing ordered update owner. Never let a delayed reply overwrite later application writes.

Propose one bounded pending projection snapshot per admitted installation plus one in flight. Include bytes, item records, revisions and source references in the cap.
Coalesce only superseded visual projections. Never coalesce separate selection events, callback effects or ordered retirement barriers.
A callback may update the item, replace the menu, throw or request a blocking Host call. Release internal locks before user code.
Do not block the UI thread on Bun, or run arbitrary callbacks inside WorkerLink's parked synchronous reply path.
A callback exception follows the facade's approved error contract. Preserve consumed sequence and completed state transitions. No automatic retry or fabricated success.
On saturation or ambiguous projection commit, apply the approved bounded failure path and disable unqualified delivery until reconciliation.
Do not silently drop a user selection and report successful delivery. Keep pre-admission refusal distinct from a possibly executed effect.

Stable public read path: construct items, append/insert into a not-yet-installed tree, return ordered `Menu.items`, and resolve nested IDs through the same tree.
Preserve object identity for `item.menu`, original item and installed setter argument. Validate cycles, duplicate-object reuse and bounds before native materialization.
Qualify missing ID, duplicate ID, invalid positions, parent ownership and supported radio grouping against the reference before advertising those behaviors.
Installing a menu does not authorize subsequent structural edits to a live native tree. Rebuild and explicitly reinstall through GH480 until a separate contract is approved.
Keep ordinary item-property updates separate from structural mutation. Qualify checked, enabled, visible and label individually using AC-5 and AC-6.

Bun records the last locally valid setter argument synchronously, including null, and tracks whether a setter ever ran.
The getter reports the local application Menu or null. It is not a native installation receipt.
Keep the last admitted native generation separate from the last public setter argument when a void install fails or its effect is unknown.
An unaccepted replacement cannot steal callbacks from the last admitted tree. An ambiguous outcome cannot automatically replay installation or a selection.
Explicit null before the default hook suppresses defaults. Do not infer that null after an existing Mac installation removes all native UI.
B-2 qualifies post-install null behavior against the actual reference. A default without `menu` authority must cause zero unauthorized native effects.
Neither startup nor a facility called “default” bypasses Guard. Do not block Ready waiting for a callback on a parked role.
Expand default role templates through the existing role owner. Freeze the expected source roles, order and native dispositions in B-1/B-2.
If the approved projection cannot represent the required default, leave default compatibility blocked. An empty or truncated native menu cannot pass AC-9.

### D-4: Accelerator and property scope

FACT: [The macOS option](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/menu-item.md#L40-L47) defaults `acceleratorWorksWhenHidden` to true.
Do not reject all hidden accelerators as if they were hidden pointer selections. Disabled state still prohibits action.
Reuse GH480's role-default lookup. Non-role pointer and accelerator activation must converge on the same Host eligibility, generation and emission path.
One Host parser normalizes supported modifiers and keys. An invalid chord cannot acquire a fallback binding or invalidate unrelated normal pointer selection.
Qualify aliases, case, layout, duplicate chords, repeated key actions, pointer modifiers and user overrides on the pinned Mac reference.
Record booleans as fields of a plain event object. Preserve observed presence versus absence. Never serialize an NSEvent, sender object or raw handle.
Do not use the app's argument, guessed keyboard layout or a synthetic key-match Boolean as evidence of a real native invocation.

| Tracker surface | Proposed first disposition and proof |
|---|---|
| `click`, `checked`, `enabled`, `visible`, `label`, `accelerator` | Require actual construction, reads, callback entry and native effect for each supported branch. |
| `Menu`, `MenuItem`, `items`, `append`, `insert`, `getMenuItemById`, `id`, `commandId`, `menu` | Test pre-install object identity and ordering. Keep private native bindings separate from public lookup. |
| `getApplicationMenu`, default template, explicit null | Qualify all three startup arms; no hidden Host-only default or implicit permission. |
| `icon`, `sublabel`, `toolTip`, `accessibilityLabel` | Preserve only explicitly qualified values and native updates. File-based icons need separate resource admission. Otherwise leave the member unqualified. |
| `userAccelerator` | Read-only macOS user mapping. No inferred value from the requested shortcut. Defer support until a real override is observed. |
| `registerAccelerator` | The cited option is Linux/Windows-specific. Do not use it as a new macOS security gate or claim either platform here. |
| `KeyboardEvent` | Qualify actual `ctrlKey`, `metaKey`, `shiftKey`, `altKey`, and `triggeredByAccelerator` separately for admitted input routes. |

### D-5: Bounded transport and migration

Inherit the bounds eventually approved by GH480. Its current tree/depth limits are proposals, not approved capacities.
B-1 must freeze event bytes, pending event records, snapshot bytes and callback-map lifetime.
Use a role-wide ceiling across current, pending and retiring snapshots and events. Refuse new retention before it exceeds the shared budget.
Freeze bounded sequence width, overflow action, per-generation limits and reconciliation deadline. No sequence wrap or generation reset may revive old commands.
Test zero, exact and limit-plus-one for each budget. Count old snapshots during replacement until actual teardown, not just getter replacement.
Use barriers for both focus orders, both install orders, role loss before/after emission, callbacks during replacement and reentrant setter effects.
Do not create unbounded per-click promises, diagnostics or retries. Keep counters and diagnostics bounded without retaining menu text or app payloads.

Migrate the facade tree, Host native projection, generated channel contract, authenticated dispatcher, conformance cells and existing app consumers together.
Keep the GH480 typed unsupported diagnostic until an isolated implementation build qualifies delivery. Atomically switch one current admitted path at activation.
Preserve the old diagnosed behavior for unsupported/disabled feature configurations. No temporary alternate channel or capability key is permitted.

## 5. Boundaries

- This PR adds only this document. No runtime API, channel ID, app profile, installed menu or dependency changes here.
- Future edits to `packages/@keld/electron/`, `crates/keld-native/`, Core dispatch, window registry and generated contracts require their designated writers.
- GH480 retains materialization and native roles. GH508/GH527 retain channel/transport policy. GH531 retains window identity. GH446 retains Ready ordering.
- Preserve GH450/GH563 Close/Quit and GH557 popup/Tray. No app-specific implementation belongs in shared menu code.

## 6. Tasks

### T-1: Freeze decisions and reference fixtures

- **Blocked by:** none
- Prepare exact owner amendments, conformance fixtures, supported member table, bounded event rules and reference experiment plan.
- Obtain separate scoped operator approval before native reference experiments. This document authorizes none.
- Resolve B-1 and B-2 from approved contracts and independent reference observations. Do not require the later integrated Keld outcomes.

### T-2: Register conformance and implement isolated facade behavior

- **Blocked by:** T-1, B-1, B-2
- Register a pinned GH477/GH566 click-signature case before implementation. Preserve existing role cells and denominator rules.
- Exercise the pure or controlled-transport portions of AC-2 through AC-5 and AC-8 through AC-10. Keep native and app portions open.
- Cover constructors, pre-install structure, getter identity, default/custom/null order and callback exception/reentrancy. No enabled native event path yet.

### T-3: Integrate authenticated native selections under test

- **Blocked by:** T-2, B-3
- Use a scoped isolated build with the approved GH480 projection and event contract. Implement both native and Bun guards before activation.
- Execute AC-1 through AC-11 with real native effects where required. Mutate one owner predicate at a time.
- Verify rollback, role loss, overflow, parked-main and replacement behavior. Preserve native role execution exactly once.

### T-4: Qualify the caller and enable only accepted behavior

- **Blocked by:** T-3, B-4
- Run AC-12 through the actual application callback and repeat applicable AC-1 through AC-11 against the exact integrated candidate.
- Publish observed divergences and remaining unknown members. This task produces the evidence for B-5.
- Do not enable production delivery or promote conformance cells until B-5 closes. Keep full Zettlr product qualification with GH1354.

## 7. Test plan & evidence

| Criteria | Required independent observations |
|---|---|
| AC-1, AC-7 | Receive-class decision and native emission census. A role action does not imply a custom callback. |
| AC-2, AC-3 | Original item/window identity, input source, modifier presence, callback order and invocation count. |
| AC-4, AC-5 | Callback-entry state, group peers, newest acknowledged native revision and actual checkmark. |
| AC-6, AC-10 | Native target eligibility, bounded registration inventory, and real pointer/key action counts. |
| AC-8, AC-11 | Exact installed generation, receiver barrier, retained objects and fresh successor action. |
| AC-9 | Pre-ready hook, facade getter, ordinary guarded submission and independently observed native tree. |
| AC-12 | Actual Zettlr callback, intended renderer shortcut, saved marker and unchanged neighbouring file. |

Use actual native input for the positive end-to-end route. A direct target invocation tests only the owner predicate it reaches.
Freeze finite test timeouts and nonoverlapping operator claims. A timeout is a failed/incomplete run, not a synchronization technique.
Retain first failed attempts and distinct diagnostic reruns. Restore each mutated candidate and confirm its positive control.

### E-1: Repository and upstream source review

- **Run:** Inspect section 2 and D-1 through D-4 at their immutable GitHub revisions on 2026-11-06.
- **n:** One Keld main pin, one GH480 candidate, one pinned Electron release, and two independently pinned application sources. Zero native runs.
- **Environment:** Isolated cloud documentation workspace. No Mac, Windows or Linux product session used.
- **Limits:** Source establishes owner shape and application demand. It proves no input delivery, native timing, actual focus, default ordering or product compatibility.

### E-2: Documentation discovery

- **Run:** Resolve Context7 `/electron/electron/v44.4.5`, query menu selection/state/default semantics, then inspect the official pinned code and docs in D-1.
- **n:** One discovery query. Zero reference executable or candidate runs.
- **Environment:** Electron v44.4.5 docs and corresponding implementation at commit 694f45852a0f1726cd23bfd379854de489cccb65.
- **Limits:** Keep conflicting absence semantics and native ordering open under B-2. No documentation assertion substitutes for a platform test.

## 8. Review gates triggered

- Public API: callback identity, focus, event fields, mutable state, default menu and sync getter semantics.
- Permission model: verified `app.system` `menu` at the existing owner. Menu actions confer no additional resource authority.
- Wire protocol: Host-origin receive class, bounded fields, install barriers and retirement require exact reviewer approval.
- Unsafe: none in this document. Future AppKit operations require exact-operation and lifetime review.
- Dependency addition: none proposed. Any new binding feature needs independent approval.

## 9. Perf impact

No measured change. Future work measures UI responsiveness, deferred event counts, copy/retention bounds and native projection churn separately.
Synchronous getters must read Bun state without per-read Host traffic. Report measured overhead before changing any existing budget.

## 10. Open questions, blockers & unknowns

### Q-1: Choose installation binding and failure semantics

Prefer the existing ordered role link with an explicit serviceable-generation barrier and bounded in-flight state.
Allocate exact internal fields, confirmation order and error disposition only with the GH508/GH527/GH480 owners.
Preserve truthful getApplicationMenu results and old native projection on definitive refusal. Define recovery for effect-unknown outcomes.

### B-1: Scheduling and exact shared contract approval

- **Status:** open
- **Owner:** Monish Khandelwal with GH480, GH508, GH527, GH446 and security owners.
- **Impact:** Blocks T-2 through T-4, channel allocation and production work. The next-milestone dependency is not implementation authorization.
- **Check:** Approve the exact spec, Q-1 decision, public error/absence model, finite limits, bootstrap and non-role event direction. No future Keld app proof is required here.

### B-2: Pinned macOS reference semantics

- **Status:** open
- **Owner:** GH556 operator with compatibility/conformance owners.
- **Impact:** Blocks T-2/T-3 and public compatibility claims where source and docs differ or native behavior is unknown.
- **Check:** Supply independently scoped Electron observations for no-focus, retired focus, default/custom/null ordering, radio grouping, hidden/invalid/colliding accelerators and modifier fields.

A retired captured window is a Keld transport race. If Electron has no comparable delay, document the owner-approved Keld refusal rather than inventing parity.

### B-3: Usable upstream native and transport inputs

- **Status:** open
- **Owner:** GH480 native writer with GH531 window and GH508/GH527 transport owners.
- **Impact:** Blocks T-3 and native installation/selection claims.
- **Check:** Supply an approved testable menu projection, exact grant owner, focus/incarnation interface and bounded role transport. Do not require GH556's integrated click proof as an input.

### B-4: Application and native observer availability

- **Status:** open
- **Owner:** GH1354 application owner with a separately claimed macOS operator.
- **Impact:** Blocks T-4 and any Zettlr Save compatibility assertion.
- **Check:** Supply pinned app, accepted caller/renderer/save-path inputs, isolated test files and real native input/effect observers. Final Zettlr product scoring is not a prerequisite.

### B-5: Integrated acceptance before activation

- **Status:** open
- **Owner:** Monish Khandelwal and independent native, transport and compatibility reviewers.
- **Impact:** Blocks production activation and supported-cell promotion, not isolated T-3/T-4 experiments.
- **Check:** T-4 publishes exact-head native/control evidence, bounded teardown, a genuine consumer effect and independent review. Retain all unqualified member rows.

### U-1: Loss and app-visible behavior

UNKNOWN: The real selected runtime can maintain the required event ordering, bounded retention and callback identity across every admitted failure cut.
T-3/T-4 must measure those cuts. A source-level promise, a direct callback invocation or an AppKit key-match return cannot resolve them.
