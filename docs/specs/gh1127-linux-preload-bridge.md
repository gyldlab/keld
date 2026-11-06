# Spec: Linux isolated app preload and guarded bridge

- Status: draft
- Tracker: GH-1127
- Owner: Monish Khandelwal (@0monish, approval pending)
- Updated: 2026-11-05
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Specify the minimum Linux app-preload boundary for the pinned draw.io workload.
Execute admitted preload bytes in a separate app world.
Preserve the trusted Host world and untrusted page boundary.
Enable named compatibility traffic only after source, document, role and guard admission pass independently.
This document proposes future work. It does not enable a renderer capability.

- Reuse GH-484's preload contract and GH-467's approved method-exposure scope.
- Reuse the single document, parser, codec, grant and credit owners.
- Exclude dynamic contextBridge parity, sendSync, arbitrary Node resolution and remote content privileges.
- Exclude GTK4 migration, packaging, Windows/macOS changes and product score changes.
- Do not copy macOS native-provenance claims into Linux.

## 2. Spec refs

FACT: Linux currently creates its application view through the guarded media builder.
Sources: `crates/keld-wv/src/webkitgtk/mod.rs:516` and `crates/keld-wv/src/media.rs:289`.
The listed source path installs no Linux app-owned isolated preload runtime.
FACT: The canonical renderer decoder accepts v1 Bind and numeric Invoke.
Source: `crates/keld-wv/src/wv_link.rs:40`.
This is not a named Electron send/event implementation.

FACT: The Linux lockfile selects Wry 0.56.1, webkit2gtk 2.0.2 and JavaScriptCore bindings 1.1.2.
Sources: `Cargo.lock:3733`, `Cargo.lock:3230` and `Cargo.lock:1427`.
Native WebKitGTK version and session type must be recorded during future execution.

FACT: Governing sources are `docs/architecture/03-security.md:8`,
`docs/architecture/04-electron-compat.md:63` and `docs/architecture/05-webview-and-native.md:167`.
The architecture's bridge sketch is a destination, not Linux qualification.

FACT: Cross-owner inputs are pinned here for review:

- Preload/source contract: #1155 `docs/specs/gh484-macos-app-preload.md:84@0ad15d8df9897620bcad9c923b9b3f6aa224a982`.
- Source-authentication requirements: #1155 `docs/specs/gh484-macos-app-preload.md:122@0ad15d8df9897620bcad9c923b9b3f6aa224a982`.
- Document/realm owner: #1192 `docs/specs/gh542-native-document-dual-world-binding.md:121@c889a0f054b89de9b838a1a3451238cf638f86e2`.
- Literal grant owner: #1123 `docs/specs/gh541-channel-grants-navigation.md:101@3b879e40ff33aaf1b27ef49153f5e97aeb50cc68`.
- Single carrier: #1122 `docs/specs/gh540-compat-ipc.md:326@16b00a2e388449c4525a30913937bc2f6e11c472`.
- Single codec: #1190 `docs/specs/gh544-endpoint-value-codec.md:7@0437c9104077e6a52c9128e4367e91c57937b22a`.

FACT: GH-542 explicitly leaves Linux provenance unqualified at that pinned revision.
Source: #1192 `docs/specs/gh542-native-document-dual-world-binding.md:153@c889a0f054b89de9b838a1a3451238cf638f86e2`.
These references do not transfer an approval or merge status to this draft.
GH-1126 owns Linux navigation/evaluation. GH-1125 owns Linux windows.
GH-473/GH-415 retain resource containment and CSP. GH-1090 retains reload-origin repair.

## 3. Acceptance criteria

All criteria below are targets. No native pass is claimed by this specification.

### AC-1: Authenticate complete preload bytes before execution

- **Given:** A retained admitted root and independently trusted expected digest identify the selected preload entry.
- **When:** Load the unmodified absolute preload path, including missing, outside-root, symlink and in-place mutation cases.
- **Then:** Execute only the complete verified immutable snapshot. Reject every invalid case before any preload effect.
- **Negative control:**
  - **Single fault:** Skip the final complete-byte digest comparison.
  - **At-risk precondition:** An in-root regular file changes after path admission. Its trusted expected digest remains unchanged.
  - **Expected RED:** The altered preload effect marker appears. A real native effect oracle detects it before the app starts.

### AC-2: Separate page, app and trusted worlds

- **Given:** The native owner installs distinct app/trusted worlds on one dedicated per-view content manager before app navigation.
- **When:** Page, app and trusted scripts inspect globals and attempt to call each other's private handlers.
- **Then:** Private bindings and credentials remain unavailable across worlds. Allowed shared DOM access does not confer native authority.
- **Negative control:**
  - **Single fault:** Install the app private handler in the page world.
  - **At-risk precondition:** Page JavaScript reaches the native registration under test. Other source and document checks remain valid.
  - **Expected RED:** The page observes the private endpoint. The world-exposure oracle fails before any later guard can mask the defect.

### AC-3: Prove the actual native document and frame

- **Given:** B-1 has an approved Linux mechanism for view, world, main frame and document provenance.
- **When:** A subframe, wrong world, retired document or recycled view presents a valid-looking private envelope.
- **Then:** Refuse before Host/Bun forwarding. A current admitted main-frame control succeeds through the same owner.
- **Negative control:**
  - **Single fault:** Accept a renderer-supplied document field without the native provenance check.
  - **At-risk precondition:** Replay a formerly valid message at the provenance owner. Keep parsing, role and grant inputs otherwise valid.
  - **Expected RED:** The native owner admits the obsolete message. Its admission oracle fails even if final dispatch later refuses it.

### AC-4: Keep source, role and exact channel grants independent

- **Given:** A current source-qualified app world has a live native binding and authenticated Bun role.
- **When:** Send one allowed literal channel and one denied literal channel through the canonical decoder.
- **Then:** The allowed call reaches the actual Bun handler once. The denied call reaches it zero times with the registered denial outcome.
- **Negative control:**
  - **Single fault:** Omit final dispatch_privileged evaluation for the denied literal.
  - **At-risk precondition:** Source, native binding, payload and authenticated role all pass. No earlier policy check blocks this literal.
  - **Expected RED:** The independent Bun effect counter increments on the denied channel.

### AC-5: Preserve the restricted preload wrapper

- **Given:** The shared preload wrapper consumes canonical frozen facts and its approved module map.
- **When:** Run the actual admitted preload and probe forbidden require specifiers, environment keys and missing facts.
- **Then:** Preserve the approved synchronous module refusal. Expose an empty environment and no Bun token, filesystem or process authority.
- **Negative control:**
  - **Single fault:** Fall back to ambient Node/Bun resolution for an unlisted module.
  - **At-risk precondition:** The requested module exists outside the restricted map. Valid preload source reaches the resolver.
  - **Expected RED:** A forbidden module loads. The resolver's exact exception/result oracle fails.

### AC-6: Publish only genuine admitted functions at the required time

- **Given:** B-2 has approved a Linux page-to-app route and its actual registered method identities.
- **When:** The first inline page script calls each admitted method, including a nested callback case required by the pinned preload.
- **Then:** Calls use actual current registrations and produce each independently expected method effect. Missing or retired registrations cannot fabricate success.
- **Negative control:**
  - **Single fault:** Drop one required method's actual registration after its page wrapper is published.
  - **At-risk precondition:** The visible wrapper's first-inline call reaches method dispatch. Source, document, role and grant gates remain valid.
  - **Expected RED:** The required genuine effect is absent. The positive application-effect oracle fails even when refusal is safe and correct.

### AC-7: Refuse early work without retaining or replaying it

- **Given:** The page-visible API exists, but native document or Bun-role admission is not complete.
- **When:** Call a required method before admission, then complete admission and call it again explicitly.
- **Then:** The early call follows the shared refusal contract with zero retained work. Only the fresh call can produce an effect.
- **Negative control:**
  - **Single fault:** Enqueue the early call for replay after admission.
  - **At-risk precondition:** Keep admission closed until the early call is observed at its owner, then open the gate.
  - **Expected RED:** The early operation appears in the post-admission effect transcript or retains a charged queue entry.

### AC-8: Revoke stale delivery and keep one aggregate credit owner

- **Given:** The canonical carrier has pending app traffic and retained native callback work in document A.
- **When:** Replace A, destroy its view, or recover the Bun role. Then deliver late callbacks in both predecessor/successor orders.
- **Then:** No old callback reaches the successor. Release each old charge once. Preserve a valid fresh operation and the role-wide bound.
- **Negative control:**
  - **Single fault:** Reset transport credits when the app world rebinds.
  - **At-risk precondition:** A prior callback still retains its charged payload when the second world or successor binds.
  - **Expected RED:** Aggregate actual retained work exceeds the canonical allowance. The shared owner ledger and retained-work census disagree.

### AC-9: Qualify the actual Linux application path

- **Given:** The exact admitted source, Host, generated SDK, Bun, policy and native library identities are recorded.
- **When:** The unmodified pinned preload performs the required draw.io request and callback flows through real Host and Bun owners.
- **Then:** Record observable app effects, denied-effect controls, cleanup and healthy recovery. Score only through existing corpus owners.
- **Negative control:**
  - **Single fault:** Delete the actual Linux app-handler registration.
  - **At-risk precondition:** The app's real preload executes and submits its first valid named request.
  - **Expected RED:** The bounded real response/effect oracle fails. Static wrapper availability cannot pass the cell.

## 4. Design

### D-1: First-principles ownership atoms

| Atom | Existing owner and boundary | First independent falsifier |
|---|---|---|
| Source trust | GH-484/package admission maps retained source to immutable snapshot | Alter admitted file bytes while the trusted digest stays fixed. AC-1. |
| Native identity | GH-542 plus Linux adapter maps actual document to stamp | Admit a retired/subframe callback with valid-looking data. AC-3. |
| Authentication | Existing role-link owner maps live Bun connection to role generation | Late G1 delivery reaches G2. AC-8. |
| Authorization | GH-541/GH-552 maps exact grant to dispatch decision | Denied literal increments actual Bun counter. AC-4. |
| OS containment | KEL-78 and resource owners retain their separate guarantees | Preload route claims a path or sandbox grant from a resource locator. B-3. |
| Lifecycle/resources | GH-540 owns aggregate credit and release | Rebind refills while old work remains retained. AC-8. |
| Exposure | GH-467 owns genuine page methods and callback identities | Removing actual registration eliminates the required genuine effect. AC-6. |
| Evidence | GH-567/GH-1129 own conformance and product admission | Shipping registration deletion still passes. AC-9. |

Source trust cannot authenticate a native frame. A world name cannot authorize a channel.
An OS sandbox does not prove message provenance. A response does not prove release.
The edges are source eligibility to execution, document/role admission to forwarding, and forwarding to bounded retirement.
Keep separate oracles at every edge.

### D-2: Keep Wry construction and add only qualified native hooks

FACT: [Wry's safe Unix extension](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/lib.rs#L2409)
returns the existing WebKitGTK handle. Retrieved 2026-11-05. Quote: "Returns Webkit2gtk Webview handle".
FACT: [Locked user-script bindings](https://github.com/tauri-apps/webkit2gtk-rs/blob/8de4e245df5c08562a28348e77bcf51cb01e67ea/src/auto/user_script.rs#L40)
provide `UserScript::for_world`. Retrieved 2026-11-05. Quote: "pub fn for_world(".
The arguments select world, frame scope and injection time.

FACT: [API 4.1 registration](https://webkitgtk.org/reference/webkit2gtk/stable/method.UserContentManager.register_script_message_handler_in_world.html)
returns false on duplicate registration. Retrieved 2026-11-05.
Quote: "TRUE if message handler was registered successfully, or FALSE otherwise."
Require successful registration before app execution. Connect the detailed handler before registering its name.
Give each registered world a distinct handler name. Retain every owned signal ID for teardown.

Use one dedicated content manager per view. Preserve the existing media witness and GPU preparation.
Coordinate the pre-load construction seam with GH-1126. Never expose its raw builder to callers.
Install top-frame document-start scripts only after approved source admission.
Default-page Wry IPC is not an authorized Electron carrier.
Its removal or isolation must occur before app content and preserve every existing owned registration.
Do not adopt a second event loop, WebView registry, policy parser or GTK4 stack.

### D-3: Native provenance is an unresolved Linux requirement

FACT: [WebKitGTK's signal](https://webkitgtk.org/reference/webkit2gtk/stable/signal.UserContentManager.script-message-received.html)
passes a content manager and JavaScript result. Retrieved 2026-11-05.
Quote: "The value received from the JavaScript world."
FACT: [WebKitGTK 2.52.6 source](https://github.com/WebKit/WebKit/blob/4fb33923db2f945803df49546f75867980365c08/Source/WebKit/UIProcess/API/glib/WebKitUserContentManager.cpp#L412)
does not forward its native FrameInfoData or ContentWorld parameters through that signal.
Retrieved 2026-11-05. Quote: "signals[SCRIPT_MESSAGE_RECEIVED]".

INFERRED: A manager pointer and message body alone cannot establish GH-542's complete provenance tuple.
Named-world registration proves a registered route, not the originating current main-frame document.
A top-frame injection setting does not authenticate every later native callback.
Do not use the current view URL as the sender URL.
Do not trust JSON world/frame/generation fields, DOM events, BroadcastChannel or callback timing as native identity.

B-1 requires a supported native mechanism and exact shared-owner approval.
Compare a narrow supported extension with an upstream public API extension.
Do not add a WebProcess extension, dependency or native IPC protocol through this draft.
If neither option proves the boundary, refuse Linux app-bridge admission and publish the missing evidence.
Do not weaken GH-542 to make a platform probe pass.

### D-4: Consume the shared source and wrapper contracts

An absolute preload path selects one admitted package entry. It is not filesystem authority.
Reuse the retained-root loader and independently authenticated expected digest from GH-484.
Check complete source bytes before execution. A digest next to mutable source cannot authenticate itself.
Reject symlinks and normalized aliases as that owner's first-proof contract requires.
Hold an immutable snapshot through the window lifetime. Capture additionalArguments and canonical process facts at construction.
Never reopen a checked source by its display path.

FACT: The pinned GH-484 wrapper has a closed module map and an empty preload environment.
Source: #1155 `docs/specs/gh484-macos-app-preload.md:84@0ad15d8df9897620bcad9c923b9b3f6aa224a982`.
Reuse those exact definitions. Do not add a Linux-specific module map or fake process facts.
That wrapper's process object is distinct from GH-570's main-role process object.
Missing canonical facts refuse initialization. Browser source, environment and credentials cannot fill the gap.

A source-admission failure prevents execution. A later script exception follows GH-484's selected app-visible behavior.
Do not claim transactional rollback of earlier DOM or storage effects.
The native callback copies bounded data only. No native handle, role token or executable capability enters the page.

### D-5: Keep first-inline publication separate from effect admission

FACT: The pinned GH-484 first-proof scope describes a fixed four-method page facade.
Its transport and genuine callable binding remain blocked at that revision.
Source: #1155 `docs/specs/gh484-macos-app-preload.md:110@0ad15d8df9897620bcad9c923b9b3f6aa224a982`.
This draft does not approve a Linux substitute.

GH-467 owns the page method table, nested callbacks and allowed caller semantics.
GH-484 owns execution of the admitted actual preload and conditional configuration seeding.
Native document-start injection is not proof of relative app/page publication order.
AC-6 must test both registration orders and the first actual inline script.
Freeze any route only after B-2 closes. Neither a static method shape nor a DOM handshake proves genuine callable identity.

Before source, document, role and grant admission, retain zero compatibility calls.
Follow the shared public return and private refusal contract. Do not invent a retry or readiness queue.
If required app startup calls arrive too early, leave product acceptance blocked.
A safe refusal is not proof that the unmodified app works.

### D-6: One transport, one identity owner, explicit teardown

The adapter submits to the canonical GH-540/GH-465 decoder and GH-543 outbound owner.
GH-544 encodes values. The Host remains opaque to endpoint values.
GH-541/GH-552 own literal channel grants and final dispatch_privileged evaluation.
GH-542 owns the single navigation generation. Linux does not mint another counter.

Each world slot consumes the shared document owner. No credential is shared across trusted and app realms.
Duplicate bind cannot reset credit or replace an admitted slot.
Record native callback lifetime separately from Bun role generation and window generation.
Bun recovery retires old role work without pretending the surviving document was replaced.

Revoke delivery before unregistering handlers. Disconnect owned signal IDs and remove only owned scripts.
Cancel pending callbacks without claiming immediate native release.
Retain old charges until callback completion or proven teardown. Never charge a successor for predecessor cleanup.
Preserve one role-wide transport ledger, including queued, canceled and retiring work.
The source snapshot, callback table and diagnostics need explicit bounds under B-5.
No silent drops, extra queues, retries or per-world credit pools are approved.

Migration includes the actual preload loader, generated wrapper, page facade, native registrations and final Host/Bun route.
Keep permanent Electron facades. No persisted application schema changes.
Any temporary platform adapter needs an owner and a removal condition before implementation approval.

## 5. Boundaries

- This PR changes only this specification.
- Future Linux code belongs under `crates/keld-wv/src/webkitgtk/` and focused native tests.
- The existing media witness, shared document owner and Core handoff require coordinated owner changes.
- GH-484/GH-467 own shared wrapper/exposure code. GH-540/GH-544 own parser/codec code.
- Do not edit Windows/macOS implementations, active claims, root policy, CI, generated outputs or product scores here.
- No new unsafe, dependency, manifest key, wire field or protocol is approved by this document.

## 6. Tasks

T-2 through T-4 use isolated acceptance builds with production delivery disabled.
Shared-owner prerequisites cannot substitute for this adapter's integrated acceptance evidence.

### T-1: Qualify the Linux native seam without privileged traffic

- **Blocked by:** none
- Obtain separate scoped test-only approval and a claim before native probes.
- Exercise named worlds, registration failure, genuine first-inline registration order and document replacement.
- Run AC-2 and the native portions of AC-3 and AC-6 using nonprivileged local effect witnesses.
- Keep actual app-source, Host/Bun and complete method-flow acceptance open for T-2 through T-4.
- Record supported mechanisms and limits. Resolve B-1 and Q-1 through exact owner review.

### T-2: Compose admitted source and restricted wrapper

- **Blocked by:** T-1, B-1, B-3, B-5, B-6
- Integrate the approved source loader, native app world and actual unmodified preload as one vertical slice.
- Run AC-1, AC-2 and AC-5, including in-place source mutation and forbidden-module controls.
- Keep named effect delivery disabled until T-3 passes.

### T-3: Integrate genuine methods with guarded delivery

- **Blocked by:** T-2, B-2, B-4, B-5, B-6
- Connect actual registered methods through the canonical carrier, grant owner and authenticated role.
- Run AC-3, AC-4, AC-6, AC-7 and AC-8 against real Host/Bun effects.
- Re-run trusted bridge isolation and Linux media denial after installing both worlds.

### T-4: Qualify the complete Linux application flow

- **Blocked by:** T-3, B-7
- Run AC-1 through AC-9 at the integrated implementation head.
- Publish all attempts and controls through GH-567/GH-1129's existing evidence procedure.
- Keep hosted X11, real Wayland, development source and installed distribution claims separate.
- This task produces B-8's final integrated acceptance evidence.

### T-5: Enable only the qualified application boundary

- **Blocked by:** T-4, B-8
- Reconcile final source, shared-owner approvals and shipping caller migration.
- Enable only the source, platform and application flows qualified by B-8.
- Leave every unresolved route denied and its compatibility claim unqualified.

## 7. Test plan & evidence

| Criterion | Owner | Required evidence |
|---|---|---|
| AC-1 | Source loader | Trusted expected digest, actual executed bytes and zero invalid-source effects |
| AC-2 | Native world installer | Page/app/trusted visibility matrix and wrong-world registration mutation |
| AC-3 | GH-542/Linux adapter | Real frame/document/lifetime provenance and independent admission refusal |
| AC-4 | GH-552/Core/Bun | One permitted effect and zero denied-literal handler entries |
| AC-5 | GH-484 wrapper | Exact resolver behavior, missing-fact refusal and empty environment |
| AC-6 | GH-467/GH-484 | Actual method registration and first-inline caller transcript in both orders |
| AC-7 | Shared early admission | Zero retained early work and one explicit fresh-call effect |
| AC-8 | GH-540/lifetime owner | Aggregate retained work, once-only release and healthy successor |
| AC-9 | GH-567/GH-1129 | Real app/preload/Host/Bun effects with shipping-registration deletion |

Use isolated subprocesses, temporary admitted roots and observable synchronization.
Never synchronize races by sleeping. A timeout only terminates a failed arm.
Retain every failed attempt and each single-fault result. Restore source and rerun its positive control.
Record native versions, session type, immutable inputs and cleanup witnesses.
Static JavaScript visibility, screenshots and generated receipts cannot replace native effect counters.

### E-1: Source and current-documentation inspection

- **Run:** Read the pinned repository, binding and WebKitGTK sources cited in sections 2 and 4 on 2026-11-05.
- **n:** One repository snapshot. Zero native executions.
- **Environment:** Cloud review workspace using GitHub and official API/source documents.
- **Limits:** This proves source shape and documented API availability only. It proves no native realm, frame, source or product acceptance.

### E-2: Required Linux native evidence

- **Run:** Not executed. T-1 and later tasks must record exact executable commands and immutable fixture artifacts.
- **n:** Zero runs in this documentation task. Record every baseline, failure and independent mutant when run.
- **Environment:** Future native Linux with pinned app, Host, SDK, Bun, GTK and WebKitGTK identities.
- **Limits:** macOS WKWebView and Windows WebView2 evidence cannot qualify this Linux boundary.

FACT: Context7 discovery used `/websites/webkitgtk_reference_webkitgtk_stable` on 2026-11-05.
The query covered named worlds, document-start scripts and native message provenance.
Discovery returned GTK4-family examples. This spec instead checks API 4.1 and the exact locked bindings.
Primary documentation confirms API shape. B-1 records the remaining native authority gap.

## 8. Review gates triggered

- Permission model: required for native source/document admission and final exact channel authorization.
- Public API: required for preload wrapper, method exposure, callbacks and failure behavior.
- Dependency: required for any explicit locked binding or selected native extension.
- Unsafe: none in this document. Future native unsafe needs exact-path approval and lifetime proof.
- Wire protocol: no change authorized. GH-540 retains private envelope, credit and retirement ownership.
- Independent reviewers must challenge the complete final spec and each one-fault control at the rule's actual owner.

## 9. Perf impact

No runtime or comparative performance result is claimed.
Future measurements separate source snapshot bytes, callback retention, transport copies and native UI progress.
Use the same admitted workload and canonical metric owner. Do not create a Linux metric registry.
Payload limits do not bound engine allocations that occur before the native callback receives a value.

## 10. Open questions, blockers & unknowns

### Q-1: Select a supported Linux provenance mechanism

Recommend retaining Wry/tao and qualifying the smallest supported native hook first.
If the public UI signal is insufficient, compare a supported WebProcess extension with an upstream API extension.
Each option needs explicit dependency, lifecycle and security review.
Do not substitute page claims or lower GH-542's acceptance boundary.

### B-1: Native main-frame and document provenance

- **Status:** open
- **Owner:** Monish Khandelwal coordinating GH-542 and Linux engine/security owners.
- **Impact:** Blocks T-2 and every production app-bridge admission. Named worlds alone cannot close this gate.
- **Check:** T-1 produces actual source-frame/document/lifetime evidence and an independent stale/subframe admission failure on Linux.

### B-2: Genuine first-inline method publication

- **Status:** open
- **Owner:** GH-467/GH-484/KEL-142 owners, coordinated by Monish Khandelwal.
- **Impact:** Blocks T-3, callable compatibility claims and complete preload acceptance.
- **Check:** Approve a Linux route using T-1 or independent owner probes with genuine local method effects in both script orders.

This prerequisite does not require integrated Host/Bun traffic. T-3 and T-4 must prove actual application effects under B-8.

### B-3: Source trust and resource eligibility

- **Status:** open
- **Owner:** GH-484/package-source and GH-473/GH-415 resource owners, coordinated by Monish Khandelwal.
- **Impact:** Blocks T-2 and preload execution. A retained path or self-hashed mutable tree cannot substitute for trusted bytes.
- **Check:** Pin the approved shared loader, trusted digest inputs and independent Linux loader mutation/containment evidence from its existing owner.

These producer prerequisites must not require this adapter. T-2 and T-4 prove actual preload execution and invalid-source effect refusal.

### B-4: Canonical carrier and grant implementation

- **Status:** open
- **Owner:** GH-540/GH-465/GH-543/GH-541/GH-542/GH-552 owners, coordinated by Monish Khandelwal.
- **Impact:** Blocks T-3 and all named compatibility sends/events. The current v1 decoder cannot supply this contract.
- **Check:** Pin owner-approved carrier, grant and identity implementations with their independent contract evidence. No Linux adapter proof is required here.

T-3 and T-4 produce permitted and forbidden integrated Host/Bun effect evidence. B-8 gates production delivery on those results.

### B-5: Construction, retention and diagnostics contract

- **Status:** open
- **Owner:** keld-wv construction owner with GH-484/GH-540, coordinated by Monish Khandelwal.
- **Impact:** Blocks T-2/T-3, source/callback bound claims and any new typed source-admission error allocation.
- **Check:** Approve pre-load installation, cleanup, each retention bound, registered error mapping and independent test plans.

T-1 supplies the construction probe. T-2 through T-4 execute integrated limit-plus-one and retained-callback controls before B-8 closes.

### B-6: Exact specification and shared-owner approval

- **Status:** open
- **Owner:** Monish Khandelwal and independent architecture/security reviewers.
- **Impact:** Blocks T-2 through T-5 and any new public, permission, dependency or unsafe surface.
- **Check:** Resolve Q-1 and record exact-content approval of prerequisite contracts and final-head review.

This approval gate excludes B-8's later integration evidence. Isolated qualification tasks must be able to produce that evidence.

### B-7: Complete native operator and corpus inputs

- **Status:** open
- **Owner:** Monish Khandelwal coordinating GH-567/GH-1129's Linux acceptance operator.
- **Impact:** Blocks T-4 and every Linux product or distribution claim.
- **Check:** Name the operator and freeze actual source, runtime, native library and scenario identities before running the admitted product corpus.

### B-8: Integrated Linux acceptance before production delivery

- **Status:** open
- **Owner:** Monish Khandelwal with Linux, preload, security and GH-567/GH-1129 acceptance owners.
- **Impact:** Blocks T-5, production delivery and compatibility claims. Isolated T-2 through T-4 qualification may produce its evidence.
- **Check:** T-4 passes every AC and single-fault control on the final implementation head, including real app effects, cleanup and independent review.

### U-1: First-inline order and callback quiescence

UNKNOWN: Linux script order and late-callback release have not been measured by this task.
AC-6 and AC-8 require native observations. An injected wrapper or an early rejection cannot prove either property.

