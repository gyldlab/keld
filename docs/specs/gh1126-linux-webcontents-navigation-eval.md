# Spec: Linux navigation and JavaScript result delivery

- Status: draft
- Tracker: GH-1126
- Owner: Monish Khandelwal (@0monish, approval pending)
- Updated: 2026-11-05
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Define the Linux adapter for the existing webContents contracts.
An admitted navigation reports its actual terminal result once.
An admitted JavaScript evaluation preserves the shared result contract.
Retired callbacks cannot change a successor document.
This draft prepares future implementation. It changes no executable behavior.

- Reuse the shared facade, document identity, guard, codec and transport owners.
- Limit application scope to the pinned draw.io demand admitted by GH-1129.
- Exclude popup creation, general remote browsing, new permission keys and GTK4 migration.
- Exclude Windows/macOS edits, packaging, performance claims and product score changes.
- Keep every production task blocked by its named approval and safety dependencies.

## 2. Spec refs

FACT: The live engine uses GTK3 through Wry, not the destination GTK4 design.
Sources: `crates/keld-wv/Cargo.toml:96` and `docs/architecture/05-webview-and-native.md:3`.
The lockfile selects Wry 0.56.1 and webkit2gtk 2.0.2.
Sources: `Cargo.lock:3733` and `Cargo.lock:3230`.

FACT: Current `eval` is fire-and-forget.
Sources: `crates/keld-wv/src/engine.rs:376` and `crates/keld-wv/src/webkitgtk/mod.rs:587`.
The initial application path reports the first Wry Finished event as NavigationReady.
Source: `crates/keld-wv/src/webkitgtk/mod.rs:538`.
These source facts do not establish native acceptance.

FACT: Governing architecture covers UI ownership, default-deny and shared contracts.
Read `docs/architecture/01-overview.md:139`, `docs/architecture/02-ipc.md:447`,
`docs/architecture/03-security.md:8` and `docs/architecture/05-webview-and-native.md:35`.
This draft narrows the destination without amending those owners.

FACT: Shared documents remain separate approval inputs:

- Navigation owner: #1192 `docs/specs/gh542-native-document-dual-world-binding.md:69@c889a0f054b89de9b838a1a3451238cf638f86e2`.
- Exact grants: #1123 `docs/specs/gh541-channel-grants-navigation.md:190@3b879e40ff33aaf1b27ef49153f5e97aeb50cc68`.
- Codec owner: #1190 `docs/specs/gh544-endpoint-value-codec.md:7@0437c9104077e6a52c9128e4367e91c57937b22a`.
- Transport owner: #1122 `docs/specs/gh540-compat-ipc.md:666@16b00a2e388449c4525a30913937bc2f6e11c472`.

FACT: [GH-459](https://github.com/gyldlab/keld/issues/459) owns facade load events.
[GH-462](https://github.com/gyldlab/keld/issues/462) owns evaluation envelopes and Promise mapping.
[GH-460](https://github.com/gyldlab/keld/issues/460) and [GH-461](https://github.com/gyldlab/keld/issues/461) retain policy ownership.
[GH-1125](https://github.com/gyldlab/keld/issues/1125) retains native window lifecycle.
These planning records do not grant Linux production approval.

FACT: [GH-1090](https://github.com/gyldlab/keld/issues/1090) remains open at retrieval.
Its recorded string-load reload defect has a separate owner.
This adapter cannot claim reload support by replacing a reload with fresh HTML.

## 3. Acceptance criteria

All criteria below specify targets. E-2 records that native runs remain unperformed.

### AC-1: Resolve only a successful current load

- **Given:** One admitted load has a qualified native attempt and registered facade listener.
- **When:** The engine commits and successfully finishes that attempt.
- **Then:** Emit did-navigate at commit, then did-finish-load once. Resolve loadURL after its listener returns.
- **Negative control:**
  - **Single fault:** Ignore the recorded load-failed state when classifying that attempt's Finished event.
  - **At-risk precondition:** A real failing load emits load-failed then Finished. Reach the native classifier before terminal deduplication with valid identity and resources.
  - **Expected RED:** The owner classifies the failed attempt as successful. Its classification oracle fails even if settled-caller filtering preserves the earlier rejection.

### AC-2: Preserve the successor when obsolete callbacks arrive

- **Given:** Native attempt A has retained work. Host load B has already revoked A under the shared identity contract.
- **When:** A fails or completes before B commits, then repeat with A arriving after B commits.
- **Then:** Release only A's charge once. Preserve B's state, resource lease and result delivery.
- **Negative control:**
  - **Single fault:** Apply an obsolete failure to the current attempt without comparing native attempt identity.
  - **At-risk precondition:** Both attempts reach the native callback owner. B remains independently valid and can complete.
  - **Expected RED:** B rejects, loses its lease or fails its successful follow-up. The native owner transcript identifies the wrong transition.

### AC-3: Guard denial survives an application allow

- **Given:** The shared policy owner admits the test surface. The app hook allows a target that the guard denies.
- **When:** A page requests navigation, including a redirect target and a frame case admitted by that owner.
- **Then:** Invoke the required app hook once. Ignore the native policy decision before commit. Record zero forbidden resource effects.
- **Negative control:**
  - **Single fault:** Replace the guard-and-app decision with the app result at final native policy disposition.
  - **At-risk precondition:** Earlier parsing accepts the request. The native decision owner receives it, with no later denial masking this check.
  - **Expected RED:** The native disposition permits the denied target. The disposition oracle fails even if another resource layer blocks loading.

### AC-4: Preserve evaluation values and rejection semantics

- **Given:** A qualified document and the shared GH-462 envelope consume the approved GH-544 codec.
- **When:** Evaluate 1+1, strings, null, undefined, arrays, a fulfilled Promise and a rejected Promise in the page world.
- **Then:** Deliver the exact selected value or rejection once. Preserve null versus undefined and approved Error fields.
- **Negative control:**
  - **Single fault:** Replace the codec result path with JSON-only conversion.
  - **At-risk precondition:** The valid undefined case reaches the native result callback and the endpoint decoder.
  - **Expected RED:** The caller receives a missing, null or changed result. An independently authored literal expected-value table fails.

### AC-5: Reject stale results without retiring same-document work

- **Given:** A real evaluation is pending in document A, with a caller awaiting its result.
- **When:** Replace A before completion. Separately change only its hash while the same evaluation is pending.
- **Then:** Reject the replacement arm with the shared stale-document error. Preserve and settle the hash arm normally.
- **Negative control:**
  - **Single fault:** Make the final native callback-currentness predicate admit a retired document.
  - **At-risk precondition:** Hold A's real callback through replacement. Reach that predicate before settled-caller filtering with valid decoded results.
  - **Expected RED:** The owner-local disposition admits A. Its assertion fails even if the already-settled caller prevents outward delivery.

### AC-6: Bound retained work and preserve cleanup evidence

- **Given:** The approved B-3 limits cover script bytes, results, policy waits, pending calls and native callback retention.
- **When:** Exercise zero, exact limit and limit-plus-one, then policy timeout, cancel, destroy and WebProcess termination with work pending.
- **Then:** Reject excess before reservation. Settle each caller once. Retain accounting until callback quiescence or native teardown proves release.
- **Negative control:**
  - **Single fault:** Release a native reservation at public rejection while its callback still retains the payload.
  - **At-risk precondition:** Hold that callback after rejection. Admit replacement work against the same aggregate owner.
  - **Expected RED:** Actual retained bytes exceed the approved aggregate bound despite a lower ledger total.

### AC-7: Install policy before the first application load

- **Given:** The existing media witness owns construction. Navigation callbacks and denial hooks are required before app content starts.
- **When:** Construction fails between callback installations, or a first load tries a denied navigation immediately.
- **Then:** No partially configured view starts app content. Existing media denial remains installed. Release registered callbacks and handles once.
- **Negative control:**
  - **Single fault:** Start app content before completing the final navigation-hook installation.
  - **At-risk precondition:** The document's first action reaches the native policy request before the missing hook is installed.
  - **Expected RED:** The installation-order oracle observes a policy request without its required owner. Resource denial cannot hide this failure.

### AC-8: Prove the installed Linux caller path

- **Given:** The exact Host, Bun, generated SDK, source manifest and native library versions are recorded.
- **When:** The actual migrated caller loads, evaluates, cancels and destroys through the shipping adapter on Linux.
- **Then:** Record each application observable and terminal outcome. Separate hosted X11 evidence from a real Wayland session.
- **Negative control:**
  - **Single fault:** Delete the shipping evaluation-completion registration.
  - **At-risk precondition:** The migrated app sends a valid evaluation and reaches the native submission call.
  - **Expected RED:** Its bounded caller-result oracle fails. A model callback or imported module cannot satisfy the missing result.

## 4. Design

### D-1: Ownership and independent decision atoms

The proposed adapter preserves one owner for each boundary.

| Atom | Owner and input/output | Failure and independent observable |
|---|---|---|
| Identity | GH-542 native document input to current stamp | Obsolete attempt changes B. AC-2 observes B's lease and transition. |
| Authentication | Native view lifetime to admitted callback | Recycled view or unproven frame settles work. AC-5 observes callback-owner disposition and caller outcomes. |
| Authorization | GH-460/KEL-79 guard and app input to native disposition | App allow widens guard. AC-3 observes the decision owner. |
| Containment | GH-473/GH-415 resource identity to bounded bytes | URL spelling becomes filesystem authority. B-2 blocks that substitution. |
| Lifecycle | Linux UI owner and GH-462 caller to settlement/release | Early accounting release hides retained work. AC-6 measures both. |
| Evidence | GH-1129/Linux operator to qualified transcript | A fixture replaces the installed route. AC-8 deletes the real registration. |

Identity does not prove authorization. A completed callback does not prove resource release.
The explicit edges are resource eligibility to identity, identity to dispatch, and settlement to eventual release.
Failure at one edge cannot be repaired by another atom's passing test.

### D-2: Reuse the current construction and safe native handle

FACT: The existing media witness applies initial content and builds the GTK view.
Source: `crates/keld-wv/src/media.rs:289`.
Its builder must not escape to replace the guard callback.

FACT: Pinned [Wry source](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/lib.rs#L2409)
exposes `WebViewExtUnix::webview`. Retrieved 2026-11-05. Quote: "Returns Webkit2gtk Webview handle".
Use that safe handle for missing native callbacks, subject to B-1 and B-4.
Retain Wry/tao construction and GPU preparation. Reject a blanket GTK4 rewrite.

Extend the owning construction witness to install all required hooks before app content.
An inert construction phase may precede installation. It carries no renderer authority.
Registration failure closes the construction attempt. Never continue with fewer guards.
The media owner must review any shared helper change.

FACT: [Wry's Linux eval source](https://github.com/tauri-apps/wry/blob/14be44842747a62c4110bd982f61f6c1acd705c3/src/webkitgtk/mod.rs#L782)
stores pre-commit script text without its callback. Its later result path converts through JSON.
Retrieved 2026-11-05. Quote: "pending_scripts.push(js.into());".
INFERRED: That path cannot alone satisfy AC-4 or AC-8.
The replacement addresses callback and value semantics, not a claimed performance improvement.

### D-3: Distinguish native events from public outcomes

FACT: [WebKitGTK's load contract](https://webkitgtk.org/reference/webkit2gtk/stable/signal.WebView.load-changed.html)
says the load "finishes successfully or due to an error". Retrieved 2026-11-05, API 4.1.
The retained native source confirms this for WebKitGTK 2.52.6:
[WebKitWebView.cpp](https://github.com/WebKit/WebKit/blob/4fb33923db2f945803df49546f75867980365c08/Source/WebKit/UIProcess/API/glib/WebKitWebView.cpp#L1800).

| Native observation | Target adapter action |
|---|---|
| Provisional start | Associate the native attempt only after B-1 proof. Do not report commit. |
| Redirect | Apply the shared policy to the actual target. Do not grant source eligibility from its string. |
| Qualified cross-document commit | Consume GH-542's transition. Emit the shared committed fact once. |
| load-failed then Finished | Keep the first terminal failure. Finished must not fabricate success. |
| Successful Finished | Resolve only the matching live attempt after the shared facade event. |
| Same-document change | Preserve the stamp and pending evaluation under GH-542. |
| Destroy or WebProcess loss | Revoke delivery and settle pending callers. Prove callback release separately. |

Classify each attributed Finished callback at its native owner before duplicate-terminal filtering.
Record that classification in native acceptance traces. A failed attempt remains classified as failure after caller settlement.

UNKNOWN: Linux attempt and frame attribution are not yet qualified. B-1 blocks production identity claims.
URL equality, callback order and a page-provided generation cannot close B-1.
Preserve both adverse A/B completion orders from AC-2.

FACT: [Native policy documentation](https://webkitgtk.org/reference/webkit2gtk/stable/signal.WebView.decide-policy.html)
states: "The default signal handler will simply call webkit_policy_decision_use()."
Retrieved 2026-11-05, API 4.1. Return without a decision is not fail-closed.
The adapter explicitly denies on missing identity, missing policy or failed app-hook settlement.
It retains a decision only under the approved bound. Timeout denies without renewing the deadline.
An app hook remains deny-only. Host load bypasses that hook only where GH-460 requires it.
This first slice denies native popup creation. Future allow semantics remain with GH-461.

### D-4: Settle page-world evaluation through the shared envelope

FACT: The locked [safe binding](https://github.com/tauri-apps/webkit2gtk-rs/blob/8de4e245df5c08562a28348e77bcf51cb01e67ea/src/auto/web_view.rs#L590)
provides a result callback behind feature `v2_40`. Retrieved 2026-11-05.
Its `call_async_javascript_function` callback receives a JavaScript value or a native GLib error.
Quote: "Async operations only allowed if the thread is owning the MainContext".

FACT: [WebKitGTK 2.52.6 source](https://github.com/WebKit/WebKit/blob/4fb33923db2f945803df49546f75867980365c08/Source/WebKit/UIProcess/API/glib/WebKitWebView.cpp#L4520)
documents a Promise result reaching the callback. Retrieved 2026-11-05.
Quote: "The result of the operation can be a Promise".
Native execution and cancellation behavior still need E-2.

Use the default page world for application code. Never use the trusted or app-preload world.
GH-462 owns the execution envelope, source evaluation semantics and public Promise behavior.
GH-544 owns value encoding, supported types and result limits. Host forwards opaque value bytes.
The page's result is untrusted data. It cannot authorize a Host call or select a document.
Bind each submission to the admitted caller, view lifetime, document and role generation.
Wait for successful load as the shared facade requires. Charge waiting calls to the same pending owner.

No raw-string interpolation may change the application's JavaScript semantics.
GH-462 must approve the async function's envelope and source-escaping contract under B-3.
A native error and a rejected application Promise remain different inputs to that mapping.
No error code, wire tag, generated DTO or Rust trait signature is allocated here.
Add a shared trait method only with design review and a live backend implementation.

### D-5: Retirement, bounds and migration

Keep all native handles, signal registrations and cancellations on the Linux UI thread.
Register one owner for each signal ID and callback-held reservation.
Public rejection revokes delivery. It does not prove that JavaScript stopped or a native callback released memory.
Each native completion first records its currentness disposition at the owning callback boundary.
Observe that disposition before later caller-settlement filtering. This keeps AC-5's single fault independently reachable.
After cancellation, retain the charge until callback completion or proven teardown.
On bound exhaustion, refuse new admission rather than release a still-live reservation.

Use GH-540's aggregate transport ledger. Do not add per-view transport credit.
B-3 must freeze distinct pending-eval, script, diagnostic, policy-wait and callback-retention bounds.
The codec's value limit cannot bound engine allocation before conversion.
Do not claim a hard memory ceiling from payload accounting alone.

Migration includes the actual facade caller, Core queue, native registrations and generated contract consumers.
The permanent Electron facade remains. Existing fire-and-forget callers keep their published behavior.
No persisted state changes. Remove a temporary adapter only after every declared caller migrates.
Do not change GH-1090's load origin or resource semantics inside this adapter.

## 5. Boundaries

- This PR changes only this specification.
- Future Linux implementation belongs in `crates/keld-wv/src/webkitgtk/` and its focused native tests.
- Shared `engine.rs`, `media.rs`, Core and generated contracts require their existing owners' coordinated changes.
- GH-459/GH-462 retain facade behavior. GH-540/GH-541/GH-542/GH-544 retain their contracts.
- Do not edit Windows/macOS implementations, active worktrees, CI policy, permission schema or product scores.
- No new production unsafe is approved. Any unavoidable raw native call needs the crate's exact-path gate.

## 6. Tasks

T-2 through T-4 use isolated acceptance builds with production enablement disabled.
Upstream prerequisite evidence does not close this adapter's integrated acceptance criteria.

### T-1: Qualify native attribution and callback behavior

- **Blocked by:** none
- Obtain separate scoped test-only approval and a claim before building native probes.
- Record actual load, redirect, failure, callback and cancellation behavior on the locked binding graph.
- Exercise the native portions of AC-1, AC-2 and AC-7 without privileged Host/Bun routes.
- Leave their integrated facade, application and Host effects unqualified until T-2 and T-4.
- Freeze B-1 and B-4 decisions from evidence. A failed feasibility arm stays blocked.

### T-2: Integrate navigation through the existing owners

- **Blocked by:** T-1, B-1, B-2, B-3, B-4, B-5
- Add the Linux native adapter and actual facade caller in one coordinated vertical slice.
- Run AC-1, AC-2, AC-3 and AC-7 with first-failing conformance entries.
- Run AC-6's policy-wait bound and timeout portions. Its evaluation portions remain open for T-3.
- Keep evaluation and popup creation unavailable until their own gates pass.

### T-3: Integrate settled evaluation and bounded retirement

- **Blocked by:** T-2, B-3, B-5
- Integrate the shared envelope, codec and Linux result callback with its actual caller.
- Run AC-4, AC-5 and AC-6, including retained-native-work negative controls.
- Preserve the existing fire-and-forget API and run its regression tests.

### T-4: Qualify the actual Linux app path

- **Blocked by:** T-3, B-6
- Run AC-1 through AC-8 on the final integrated source.
- Publish Linux-specific evidence through GH-1129 and the existing corpus owners.
- Keep failed or unrun cells open. Do not infer Wayland support from hosted X11.
- This task produces B-7's integrated acceptance evidence.

### T-5: Enable only the qualified Linux slice

- **Blocked by:** T-4, B-7
- Reconcile exact implementation approvals and migrate the declared shipping callers.
- Enable only the native sessions and application inputs qualified by B-7.
- Preserve default refusal for every unqualified platform or operation.

## 7. Test plan & evidence

| Criterion | Test owner | Independent observation |
|---|---|---|
| AC-1 | Native navigation plus facade conformance | Native classification before terminal filtering, plus ordered facade/caller outcomes |
| AC-2 | GH-542/Linux lifetime tests | A-only release and healthy B completion in both orders |
| AC-3 | KEL-79/Linux policy tests | Native use/ignore disposition and forbidden resource census |
| AC-4 | GH-462/GH-544 differential tests | Literal values and registered typed rejections |
| AC-5 | Native callback retirement | Owner-local disposition, old caller rejection and same-document successful control |
| AC-6 | UI callback/resource owner | Actual retained payload census plus reservation ledger |
| AC-7 | Existing construction witness | Registration order and partial-failure handle release |
| AC-8 | GH-1129 product lane | Installed caller result and shipping-registration deletion |

Use isolated subprocesses for native crash and teardown arms.
Use temporary resource roots and observable rendezvous, not sleeps.
Record all attempts, runtime/library versions, backend, session type and source digests.
Record startup, callback and native quiescence separately. Test teardown must leave a healthy successor.

### E-1: Source and documentation inspection

- **Run:** Read the pinned files and upstream URLs in section 2 and D-2 through D-4 on 2026-11-05.
- **n:** One source snapshot. Zero native executions.
- **Environment:** GitHub source reads and official WebKitGTK documentation in the cloud review workspace.
- **Limits:** Source availability proves neither callback timing, platform feasibility, source trust nor production integration.

### E-2: Required native qualification record

- **Run:** Not executed. T-1 must publish exact executable commands and immutable fixture artifacts before claiming a result.
- **n:** Zero runs in this documentation task. Record every baseline and single-fault arm when executed.
- **Environment:** Future native Linux with exact Host/Bun/GTK/WebKitGTK/binding versions and X11 or Wayland identification.
- **Limits:** Neither macOS/Windows results nor state-model tests qualify Linux behavior. Product admission also requires T-4.

FACT: Context7 discovery used `/websites/webkitgtk_reference_webkitgtk_stable` on 2026-11-05.
Queries covered named worlds, native load failure, policy defaults and Promise completion.
Its results included the GTK4 API family. The cited API 4.1 documents and immutable sources control this draft.
Documentation inspection is not a native pass receipt.

## 8. Review gates triggered

- Public API: required for shared evaluation completion, errors and facade event order.
- Permission model: required for final native policy composition and source eligibility.
- Dependency: required for explicit locked direct bindings and feature `v2_40` if adopted.
- Unsafe: none in this document. Future additions require an exact operation and lifetime review.
- Wire protocol: no change approved here. Any shared envelope change belongs to GH-540 and its generated owner.
- Independent review must cover the complete final spec, each reachable negative control and every cross-owner contract.

## 9. Perf impact

No runtime change or measured improvement accompanies this draft.
Future work measures callback latency, UI progress, queued bytes and retained native work separately.
Use the same actual caller and exact source baseline. Preserve architecture 01 section 5 budgets.
A result callback or a Rust adapter alone provides no performance evidence.

## 10. Open questions, blockers & unknowns

### Q-1: Approve the narrow safe-binding adapter

Recommend Wry/tao construction plus existing safe WebKitGTK bindings for missing hooks.
Reject a GTK4 rewrite without a named unmet requirement.
Approval must include the supported native version floor and exact direct-dependency feature changes.

### B-1: Native document and attempt provenance

- **Status:** open
- **Owner:** Monish Khandelwal coordinating GH-542 and the Linux engine owner.
- **Impact:** Blocks production callback attribution, privileged binding, stale-result delivery and T-2.
- **Check:** T-1 proves real frame/document/attempt correlation in both AC-2 orders. URL equality and timing cannot satisfy the check.

### B-2: Resource and policy admission

- **Status:** open
- **Owner:** Existing GH-473/GH-415 resource owners and KEL-79 policy owner, coordinated by Monish Khandelwal.
- **Impact:** Blocks T-2 until approved Linux resource and policy inputs exist. GH-1090 remains separately owned.
- **Check:** Pin the resource/policy owners' approved contracts and independent producer evidence. They must not require this adapter's integration.

T-2 and T-4 supply this adapter's denied-target, native-disposition and resource-effect evidence under B-7.

### B-3: Shared evaluation shape and all retention bounds

- **Status:** open
- **Owner:** GH-460/KEL-79 and GH-462/GH-544/GH-540 owners, coordinated by Monish Khandelwal.
- **Impact:** Blocks T-2/T-3, numeric-cap selection, error allocation and claims of implementable policy waits or settled evaluation.
- **Check:** Approve the exact envelope, error mapping, script/result and policy-wait limits, pending/retired bounds, cancellation outcome and independent test plans.

Approval selects the contract. T-2 proves policy-wait bounds and timeout. T-3 proves evaluation and retained-callback controls.
T-4 repeats all applicable AC-6 portions on the final integrated head before B-7 closes.

### B-4: Linux construction and binding selection

- **Status:** open
- **Owner:** keld-wv media/engine owners and GH-1125, coordinated by Monish Khandelwal.
- **Impact:** Blocks production first-load integration and T-2. A raw builder cannot bypass the existing media witness.
- **Check:** Approve a real pre-load installation seam and locked binding features after AC-7 native proof.

### B-5: Exact specification and interface approval

- **Status:** open
- **Owner:** Monish Khandelwal and the named independent architecture/security reviewers.
- **Impact:** Blocks T-2 through T-5. This draft cannot authorize changes to shared owners.
- **Check:** Record exact-content approval of Q-1 and the prerequisite contracts. Recheck the final source and review head.

This approval gate does not require downstream B-7 evidence before the isolated integration tasks can produce it.

### B-6: Native operator and complete application inputs

- **Status:** open
- **Owner:** Monish Khandelwal coordinating GH-1129's Linux acceptance operator.
- **Impact:** Blocks T-4 and every Linux product-compatibility claim.
- **Check:** Name the operator and freeze actual app, SDK, Host, policy, runtime and native session identities before execution.

### B-7: Integrated Linux acceptance before production enablement

- **Status:** open
- **Owner:** Monish Khandelwal with Linux, facade, security and GH-1129 acceptance owners.
- **Impact:** Blocks T-5, production enablement and every compatibility claim. It does not block isolated T-2 through T-4 qualification.
- **Check:** T-4 proves every AC and single-fault control on the final implementation head, including healthy recovery and exact independent review.

### U-1: Cancellation and WebProcess quiescence

UNKNOWN: A public cancellation has not been shown to stop Linux native work or release callback-held bytes.
AC-6 must measure the native lifetime. An early rejected Promise cannot close this unknown.

