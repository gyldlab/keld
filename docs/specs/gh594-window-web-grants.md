# Spec: Window web grants through the shared Guard owner

- Status: draft
- Tracker: GH-594
- Owner: Monish Khandelwal (@0monish, proposed approval owner)
- Updated: 2026-11-05
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Define future camera and microphone grants for one Host-bound window.
Keep application grants, renderer channels, and web permissions separate.
This draft specifies behavior after B-1 and B-2 close.
It authorizes no production change.

FACT: Current Guard reads application grants and rejects Webview evaluation.
See `crates/keld-guard/src/lib.rs:622` and `crates/keld-guard/src/lib.rs:784`.
The media adapter currently supplies generation zero and resource `*`.
See `crates/keld-wv/src/media.rs:59` and `crates/keld-wv/src/media.rs:63`.
These facts establish current behavior, not permission support.

Non-goals:

- No clipboard capability, general device permissions, or origin extraction.
- No new channel, protocol, policy parser, window registry, or permission cache.
- No automatic migration, hot reload, persisted browser allow, or implicit Legacy selection.
- No native permission support claim from a pure evaluator test.
- No changes to active Windows, macOS packaging, or GH552 branches.

## 2. Spec refs

FACT: Architecture 03 owns principals, the manifest, and enforcement.
See `docs/architecture/03-security.md:8`, `docs/architecture/03-security.md:75`, and `docs/architecture/03-security.md:227`.
KEL-102 D9 excludes schema extensions from its earlier approval.
See `docs/specs/kel102-host-guard-enforcement.md:92`.

FACT: GH552 proposes one windows reader, borrowed Host selection, and separate native/channel queries.
It also leaves its compatibility opt-in undecided.
See #1191 `docs/specs/gh552-exact-channel-grants-guard.md:78@a73f301cfe7027ad3e1bd700800e9abe60c66bc7`.
See #1191 `docs/specs/gh552-exact-channel-grants-guard.md:100@a73f301cfe7027ad3e1bd700800e9abe60c66bc7`.
This draft consumes that owner after B-1. It does not approve the proposed marker.

FACT: Capability names belong to Guard, not a second channel registry.
See `docs/specs/gh508-kipc-channel-table.md:40`.
The historical GH594 reference to an X05 capability allocator is superseded by that contract.
GH508 is not an unresolved whole-issue dependency for this draft.

FACT: The architecture example includes `web.csp` metadata.
See `docs/architecture/03-security.md:98`.
Treating every web key as a capability would reject that documented metadata.
D-2 separates metadata from capabilities within the same reader.

## 3. Acceptance criteria

These are future tests. Production permission activation requires B-1, B-2, and B-4 to close.
Run AC-1 through AC-9 for camera and microphone independently.
Where a criterion says camera, repeat it with microphone and the same independent oracle.
Use camera-only, microphone-only, both, and neither grant fixtures.
For an engine request containing both kinds, require both grants before any combined allow.

### AC-1: Reject unsupported capability names at policy admission

- **Given:** An approved opted-in window policy and a control fixture with valid camera grants.
- **When:** The fixture adds `windows.main.web.hid`.
- **Then:** The sole loader returns KELD-GUARD005 before Host effects start. The diagnostic names the unsupported key and removal remedy.
- **Negative control:**
  - **Single fault:** Remove the closed capability-name check.
  - **At-risk precondition:** All other fields, schema selection, and JSON syntax are valid.
  - **Expected RED:** The malformed-policy fixture loads instead of returning the expected error.

### AC-2: Preserve one unambiguous JSONC reader

- **Given:** An opted-in policy repeats camera using literal and escaped-equivalent decoded keys.
- **When:** The retained policy bytes enter the existing loader.
- **Then:** The existing unique-key decoder rejects the policy. Both channels and web data use that decoded object.
- **Negative control:**
  - **Single fault:** Replace the shared unique-key decode with a last-key-wins decode.
  - **At-risk precondition:** Both duplicate values have valid types and supported capability names.
  - **Expected RED:** The duplicate-key fixture reaches policy admission.

### AC-3: Keep application grants outside Webview authority

- **Given:** A current Host-bound view, an application camera grant, and no window camera grant.
- **When:** Its media request reaches the shared evaluator.
- **Then:** The result is Deny and the native allow callback count stays zero.
- **Negative control:**
  - **Single fault:** Select the application grant table for the web query.
  - **At-risk precondition:** The requested sentinel exactly matches the application's granted scope.
  - **Expected RED:** The decision becomes Allow or the native allow callback runs.

### AC-4: Bind a grant to exactly one live window

- **Given:** Windows A and B have live identities. Only A grants camera for the sentinel.
- **When:** Each window requests camera.
- **Then:** A receives Allow and B receives Deny. A page-supplied window name cannot change either result.
- **Negative control:**
  - **Single fault:** Use a union of all window grants.
  - **At-risk precondition:** Both identities are current and request the same supported capability.
  - **Expected RED:** B's independent native allow counter increments.

### AC-5: Keep the resource sentinel distinct from an origin

- **Given:** A live window has only an origin-shaped camera scope.
- **When:** The engine adapter supplies the unattributed resource sentinel.
- **Then:** The evaluator denies. A separate exact `["*"]` fixture allows after all other gates pass.
- **Negative control:**
  - **Single fault:** Treat the sentinel as matching any declared origin.
  - **At-risk precondition:** Identity, capability, and window selection are valid.
  - **Expected RED:** The origin-only fixture returns Allow for an unattributed request.

### AC-6: Retire authority before the native permission effect

- **Given:** A valid camera request pauses before its final Host admission.
- **When:** Navigation retires its document, then the paused request resumes.
- **Then:** The native allow count stays zero. A current successor with its own valid grant can proceed.
- **Negative control:**
  - **Single fault:** Remove the final current-document check.
  - **At-risk precondition:** The old request passed parsing and has a matching grant before the pause.
  - **Expected RED:** The retired request permits a native effect.
  
### AC-7: Separate CSP metadata from permission names

- **Given:** An opted-in window contains a known channel, a camera grant, and an approved CSP selector.
- **When:** The single reader parses the window.
- **Then:** It retains the three typed values independently. Invalid CSP selectors fail without changing camera or channel authority.
- **Negative control:**
  - **Single fault:** Replace selector validation with fallback to the default selector.
  - **At-risk precondition:** All grants are valid and only the selector is invalid.
  - **Expected RED:** The invalid-selector fixture loads instead of failing.

### AC-8: Preserve legacy admission and deny new authority

- **Given:** A historically accepted policy contains uninterpreted windows metadata without the approved activation discriminator.
- **When:** The revised loader reads it.
- **Then:** Existing application behavior remains unchanged. The metadata provides zero web or channel authority.
- **Negative control:**
  - **Single fault:** Infer activation from the presence of a web object.
  - **At-risk precondition:** The legacy metadata resembles a valid camera declaration.
  - **Expected RED:** A formerly inert value grants camera authority.

### AC-9: Keep snapshots and native permission decisions session-scoped

- **Given:** A running session uses snapshot S1 with camera denied.
- **When:** The policy file changes to allow, or an earlier profile contains a saved allow.
- **Then:** S1 still denies. A successor session requires its own verified snapshot and native acceptance.
- **Negative control:**
  - **Single fault:** Reuse a previous session's cached allow in the new session.
  - **At-risk precondition:** The new snapshot explicitly denies and the device permits capture.
  - **Expected RED:** A native capture witness succeeds under the denying snapshot.

### AC-10: Keep clipboard claims honest

- **Given:** The two clipboard permission rows in D-5 and the current pinned Zettlr demand.
- **When:** A reviewer evaluates implementation readiness.
- **Then:** Each row remains unqualified until its pinned oracle and native effect receipt exist.
- **Negative control:**
  - **Single fault:** Mark one row supported from the Electron documentation receipt alone.
  - **At-risk precondition:** No native clipboard receipt exists for that row.
  - **Expected RED:** The evidence audit rejects the unsupported status.

### AC-11: Keep permits and effects separate

- **Given:** An admitted web query and a renderer channel grant named like a native capability.
- **When:** Code attempts to pass either permit to a filesystem callback.
- **Then:** Compile-fail cases reject both conversions. Web Deny invokes no native callback.
- **Negative control:**
  - **Single fault:** Add a conversion from the web permit to the filesystem permit.
  - **At-risk precondition:** The compile-fail fixture otherwise uses valid types and a real callback signature.
  - **Expected RED:** The prohibited conversion compiles.

### AC-12: Preserve the requested media kind

- **Given:** Camera-only and microphone-only policies with valid current identities and available devices.
- **When:** Each native permission kind requests its corresponding operation.
- **Then:** Each policy permits only its named kind. Combined requests require both grants.
- **Negative control:**
  - **Single fault:** Map the native microphone request to Camera.
  - **At-risk precondition:** The camera-only policy allows camera and has no microphone grant.
  - **Expected RED:** The microphone effect succeeds despite the expected denial.

## 4. Design

### D-1: Decision atoms and independence

All target rows below are proposals. Existing owners retain authority.

| Atom and owner | Input and output boundary | Failure and independent observable |
| --- | --- | --- |
| Schema, Guard with GH552 | Verified bytes to one immutable window table | Ambiguous or unknown keys load. AC-1, AC-2. |
| Authentication, Host and GH542 | Native view/document to current identity | Page data selects an identity. AC-4, AC-6. |
| Authorization, Guard | Current window plus capability to typed permit | App or another window grants leak. AC-3, AC-4. |
| Resource, Guard | Engine attribution state to exact scope comparison | An unknown origin becomes an allowed origin. AC-5. |
| Lifecycle, Host and KEL-135 | Snapshot/document retirement to revoked admission | Old document or persisted allow survives. AC-6, AC-9. |
| OS containment, KEL-78 | Existing role profile to OS restrictions | Guard success is misreported as sandbox proof. B-4. |
| Provenance, GH594 | Source and native receipts to scoped status | Documentation becomes compatibility proof. AC-10. |

Schema and resource matching do not prove identity.
Identity does not prove a native callback is installed.
The final effect depends on identity, authorization, and lifecycle together.
Each test supplies valid prerequisites except its one mutated rule.

### D-2: Reuse the shared closed window schema

FACT: The existing loader enforces size and unique decoded keys.
See `crates/keld-guard/src/lib.rs:652` and `crates/keld-guard/src/lib.rs:721`.
Reuse its 64 KiB ceiling and immutable KEL-102 snapshot.
Do not copy comment stripping, decode logic, or scope policy into a facade.

Proposed capability keys are `camera` and `microphone` under each window's `web` object.
Their scopes are string arrays.
Missing grants and empty arrays deny.
Wrong types, unsupported names, and malformed scopes reject the activated policy.

CSP is a separate typed metadata field inside that same object.
Its selector vocabulary and effective policy remain GH473/GH552-owned.
Do not copy CSP text into this permission spec.
An invalid selector never falls back.

The proposed GH552 opt-in currently covers channels and remains unapproved.
B-1 requires the owner to approve an activation boundary that also covers web fields.
Until then, old windows metadata remains inert.
A new schema flag, different parser, or unconditional reinterpretation is not authorized here.

### D-3: One evaluator and a narrow web effect

Proposed Rust shape, conditional on B-1 and B-2:

~~~rust
struct WebCapabilityQuery<'a, H> {
    principal: Principal,
    current_window: BorrowedHostWindowGrants<'a>,
    capability: WebCapability,
    on_allow: H,
}
enum WebCapability { Camera, Microphone }
// H: for<'permit> FnOnce(&'permit WebCapabilityPermit) -> T
// Use the existing single dispatch_privileged authority boundary.
~~~

These are design types, not existing exports.
GH552 owns the shared dispatch interface.
Guard owns the closed capability vocabulary.
The implementation defines an opaque, callback-scoped web permit through that approved interface.

The media adapter selects capability from the native permission kind.
Host state supplies current window and document identity.
No page field supplies principal, window key, resource attribution, or permit.
The final callback runs only after a currentness check adjacent to effect admission.
No lock spans a native callback or IPC wait.

The requested resource remains `*` until an independently approved origin adapter exists.
Here `*` means unattributed request, not a pattern for arbitrary origins.
Only its exact scope matches.
An origin grant never implies an unattributed grant.

### D-4: Lifecycle and migration unit

FACT: Current media mapping has a single owner and defaults unknown kinds to denial.
See `crates/keld-wv/src/media.rs:96` and `crates/keld-wv/src/media.rs:155`.
Extend that owner. Do not create one permission table per backend.

Migrate Guard parsing, typed dispatch, native media callers, generated declarations, and diagnostic tests together.
Retain existing application and renderer-channel behavior.
No temporary fallback may map a failed web query onto application grants.

Resolve the configured window from the verified creation configuration once.
Borrow that selection only while its Host registry entry remains current.
Reuse GH542 navigation retirement. Do not add another generation counter.

Platform callback installation and persisted-permission behavior need separate native receipts.
One engine's receipt cannot qualify another engine.
No new queued work, retry, or asynchronous permission wait is introduced by the pure policy decision.
If an adapter needs retained callbacks, its owner must specify their count, lifetime, and cancellation first.

### D-5: Clipboard disposition and external evidence

FACT: Electron 44.4.5 documents both clipboard permission names.
Its descriptions include "Request access to read from the clipboard" and "Request access to write to the clipboard."
Source: https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/session.md#sessetpermissionrequesthandlerhandler
Retrieved 2026-11-05. This is a documentation receipt, not a native oracle.

| Permission | Proposed disposition | Missing proof |
| --- | --- | --- |
| `clipboard-read` | No new GH594 grant. No supported or engine-decided claim. | B-3 requires denied/allowed native effect observations. |
| `clipboard-sanitized-write` | No new GH594 grant. No supported or engine-decided claim. | B-3 requires gesture, frame, and effect observations. |

FACT: GH1354 names Zettlr v4.9.0 as demand and distinguishes its lock Electron from the 44.4.5 oracle.
Source: https://github.com/gyldlab/keld/issues/1354
That planning statement is not a clipboard execution receipt.
B-3 freezes exact app, oracle, engine, platform, and corpus inputs before testing.

Rejected alternatives:

- Application grants for views erase the requesting-window boundary.
- A facade vocabulary creates a second policy owner.
- A local web parser splits channels and web validation.
- An engine-decided compatibility mark without native evidence hides unknown permission behavior.
- A broad origin wildcard invents attribution that the current callback does not provide.

## 5. Boundaries

Future implementation owners:

- Guard: existing manifest reader, vocabulary, evaluator, and typed permit.
- GH552: single windows reader, shared dispatch, and diagnostic interface.
- GH542: native identity and retirement.
- Media owner: existing `crates/keld-wv/src/media.rs` and approved backend callsites.
- KEL-135: persisted profile admission and permission retirement.
- GH505/KEL-79: wider security-surface mapping.

This documentation change edits only this file.
Architecture 03 and KEL-102 amendments accompany the future approval change under B-1.
The draft records those blockers instead of silently changing the accepted architecture.
Do not edit other owners' active specs or shared generators under this claim.

## 6. Tasks

### T-1: Freeze the schema and dispatch amendment

- **Blocked by:** none
- This task prepares approvals and upstream evidence. It changes no production behavior.
- Obtain exact-content approval from the Guard, GH552, and native identity owners.
- Update their canonical architecture/API statements in the same approval change.
- Close B-1 and B-2 using approved contracts and upstream identity evidence.
- Freeze candidate fixtures for AC-1 through AC-8, AC-11, and AC-12.
- Keep all native grants disabled until the required safety slices qualify.

### T-2: Implement the pure policy vertical slice

- **Blocked by:** T-1, B-1, B-2
- Extend the shared reader, evaluator, and generated declarations together.
- Run the pure evaluator, decoder, type, and kind-mapping portions of AC-1 through AC-5, AC-7, AC-8, AC-11, and AC-12.
- Use independent mutants. Leave every native effect and device portion unqualified for T-3.
- Preserve all application grant and channel tests.
- Prove the allowed decision path allocates no memory after policy indexing.

### T-3: Qualify one native consumer before activation

- **Blocked by:** T-2
- Integrate the first approved media callback and final identity check.
- Use an isolated acceptance build while production activation remains gated by B-4.
- Run AC-3 through AC-6, AC-9, and AC-12 against physical permission effects.
- This task produces the evidence that closes B-4 for that engine.
- Keep other engines denied until their own equivalent native acceptance passes.

### T-4: Publish the clipboard evidence disposition

- **Blocked by:** none
- Pin the app and oracle before independent clipboard experiments.
- Record the two distinct clipboard rows without creating clipboard grants.
- Run AC-10 against the pinned oracle and each claimed engine.
- This task produces B-3's evidence. B-3 blocks support claims until that evidence passes.
- Route any new capability proposal through Guard and the wider security owner.

## 7. Test plan & evidence

Use exact decision values, native effect counters, and compile results as independent oracles.
A supported-device positive control must pass before a denial result counts.
A missing camera, denied OS privacy setting, or absent hook cannot prove Keld denial.

| Tests | Criteria | Required evidence |
| --- | --- | --- |
| Manifest admission and old-policy fixtures | AC-1, AC-2, AC-7, AC-8 | Exact typed errors, one loader, unchanged legacy result |
| Live window, kind, and scope fixtures | AC-3, AC-4, AC-5, AC-12 | Exact allow/deny plus per-window effect counts |
| Retirement and persisted-profile cuts | AC-6, AC-9 | Both event orders, physical effect, healthy successor |
| Clipboard qualification audit | AC-10 | Separate oracle and engine receipts |
| Permit type fixtures | AC-11 | Real compile-positive and compile-fail cases |

### E-1: Current source inspection

- **Run:** Read the pinned Guard parser, media adapter, architecture, and GH552 proposal through GitHub.
- **n:** One main revision and one separately pinned GH552 proposal. Zero native runs.
- **Environment:** Cloud documentation workspace on 2026-11-05.
- **Limits:** Source identifies ownership and present gaps. It proves no camera, microphone, or clipboard behavior.

### E-2: Clipboard documentation receipt

- **Run:** Context7 discovery for Electron 44.4.5, followed by direct official source confirmation in D-5.
- **n:** Two documented permission entries. Zero Electron or Keld execution runs.
- **Environment:** Official Electron commit 694f45852a0f1726cd23bfd379854de489cccb65.
- **Limits:** Permission names are verified. Callback coverage, gestures, effects, and compatibility remain unqualified.

## 8. Review gates triggered

Permission model and public API require independent exact-content approval.
Wire protocol: none proposed.
Dependency addition: none proposed.
Unsafe: none in the pure policy slice. Any new native operation needs its own approved gate.
Documentation approval does not authorize such native operations.

## 9. Perf impact

No measured performance improvement is claimed.
Retain the existing warmed allocation-free Allow requirement.
Measure policy indexing separately from native permission latency.
The existing 64 KiB ceiling bounds source bytes only.
Measure parsed strings, maps, and indexes separately. Do not report that input ceiling as a retained-memory bound.
Architecture 01 section 5 remains the budget owner.

## 10. Open questions, blockers & unknowns

### B-1: Shared schema and API approval

- **Status:** open
- **Owner:** Monish Khandelwal with the GH552 and KEL-102 owners
- **Impact:** Blocks schema implementation, public query types, and all positive native web grants.
- **Check:** Approve one closed windows reader, activation/migration rule, CSP separation, and callback-scoped web permit.

### B-2: Current native document binding

- **Status:** open
- **Owner:** GH542 native identity owner
- **Impact:** Blocks effect admission based on numeric IDs or generation-zero media principals.
- **Check:** Supply the approved GH542 interface and independent current-document/retirement evidence from its existing owner.

This prerequisite does not require GH594 grants to exist.
GH594's later AC-4 and AC-6 integration results belong to B-4.

### B-3: Pinned clipboard oracle and engine observations

- **Status:** open
- **Owner:** Monish Khandelwal with GH505 and GH1354 corpus owners
- **Impact:** Blocks clipboard disposition approval and any supported or engine-decided clipboard claim.
- **Check:** Pin inputs and publish read/write effect receipts for both permission rows, including denied and gesture cases.

### B-4: Native profile and callback acceptance

- **Status:** open
- **Owner:** Media backend owner with KEL-135 profile owner
- **Impact:** Blocks production media activation and revocation claims on each unqualified engine. T-3 may produce the required evidence.
- **Check:** Prove callback installation, denied-request effects, persisted-allow isolation, retirement, and healthy successor behavior.

### U-1: Generated schema integration location

UNKNOWN: The final shared windows generator surface depends on GH552's approved schema decision.
T-1 must name the existing generator and all consumers before its implementation claim.
This unknown grants no permission to introduce a parallel schema.

