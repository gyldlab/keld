# Delivery and evidence

Part of: [index](index.md)
Owns: sections 5, 6, 7, 9 and 10

## 5. Boundaries

This is a specification-only PR.
It writes only the GH552 specification directory.
The existing documentation generator may update `llms.txt` and `llms-full.txt` only if it selects this directory.
No Rust, TypeScript, manifest, architecture or CI source is changed.
No production behavior is approved by this DRAFT.

### Future GH552 implementation ownership

Implementation may begin only after this spec is approved.
Each slice needs a separate PR and a fresh winning claim.

#### Pure policy parser and evaluation

- Candidate paths: `crates/keld-guard/src/lib.rs` and its existing manifest tests.
- Add focused guard tests in the same crate.
- Dependency: GH541 integration and GH552 permission/public-API approval.
- Preserve AppProcess, legacy metadata and cold parser behavior.

#### One shared guarded query

- Candidate path: `crates/keld-ipc/src/guard_dispatch.rs` and its existing tests.
- Add only the narrow typed query API.
- Native filesystem callsite changes require the current owner's review.
- Do not add another authorized dispatcher.

#### Diagnostic-only explain

- Candidate paths: `crates/keld-cli/src/mcp/permissions.rs` and `crates/keld-cli/tests/doctor_mcp.rs`.
- Change only necessary tool-schema snapshots.
- The API is additive and read-only.
- It must never mint live privilege or accept a renderer-selected live window.

#### Final Host forwarding adapter

- Candidate path: a small GH552-owned extraction or a narrow change in
  `crates/keld-core/src/app_session.rs`.
- Add focused composed native-effect tests.
- Wait for GH542 current-document qualification and GH540/465 v2 send acceptance.
- Coordinate with the existing Core/file owner before writing.
- Prove AC-5, AC-8 and AC-10.

### Excluded ownership

Do not change or borrow approval from GH542 `keld-wv` navigation or WebKit handlers.
Do not change GH540/465/543/544 wire, codec or credit tables.
Do not change KEL-142, GH449, GH1171, GH1181 or native FS/media owner files.
Do not change root Cargo files, CI workflows or sibling worktrees.
Keep historical KEL-102, GH541 and GH540 approval artifacts immutable.
Route any required change outside these paths to its current owner.

## 6. Tasks

Tasks are ordered vertical slices.
No production task is authorized while the status remains draft.

### T-1: GH552 specification and independent approval

- **Blocked by:** none
- Keep the complete contract in the current directory layout.
- Refresh all source pins and links.
- Run the current specification, documentation and whitespace checks.
- Request independent permission-model and public-API review from Amisha.
- Stop until exact-content approval changes this spec to approved.
- A green documentation check does not authorize a product patch.

### T-2: Pure policy slice

- **Blocked by:** B-1, B-2
- Start with failing-first P01, P03, P04 and P06 tests.
- Extend only the existing `PermissionsManifest` parser and exact window lookup.
- Preserve old `/app` behavior and unknown metadata.
- Flip each regression only with the actual implementation.
- Do not add a WebKit dependency.

### T-3: One authoritative dispatch API

- **Blocked by:** B-1, B-2
- Implement the approved sealed query types inside the existing `dispatch_privileged`.
- Adapt native callers only with their owner's review.
- Prove independent handler inference and compile-fail type isolation.
- Prove failure-first Allow/Deny callbacks and callback-scoped permits.
- Measure zero allocations on warmed Allow.
- A pure closure is not a real Bun write.

### T-4: Diagnostic-only MCP API

- **Blocked by:** B-2
- Follow the approved public input and output decision.
- Extend only the read-only `keld_permissions_explain` request and response.
- Preserve legacy error behavior.
- Test hypothetical selection, escaped pointer and parity with the pure evaluator.
- Prove that no live permit or effect path is reachable.

### T-5: Qualified adapter and real enforcement

- **Blocked by:** B-1, B-3, B-4
- Wait for GH542 native document/currentness acceptance.
- Wait for GH540/465 bounded v2 and WorkerLink acceptance.
- Integrate only the narrow Core adapter.
- Prove GH541 P02/N02/N09b and GH552 AC-5/AC-8/AC-10.
- Use real authorized macOS and independent app-link/Bun counters.
- Run each single-fault mutation.

### T-6: Integration and review handoff

- **Blocked by:** B-2, B-3, B-4, B-5
- Obtain exact-file security/permission and public-API review.
- Obtain shared-dispatch/KEL-102 T-4 media review.
- Obtain GH542 native and GH540 transport evidence.
- Obtain GH464/GH532 conformance evidence.
- Preserve remaining Electron UNKNOWN/fail cells and known draw.io gaps.
- Merge requires separate owner authority and every repository gate.

Pure guard tests may be designed in parallel with GH542's specification.
They cannot claim the GH552 feature is complete.
T-5 remains blocked until native and transport prerequisites pass.
Do not implement T-2 through T-6 in this DRAFT branch.

## 7. Test plan & evidence

Every future executed case records the exact source SHA, implementation PR, command,
platform, raw effect or denial, and one deliberate fault that the oracle rejects.
Failing-first means the unimplemented source fails for the expected reason.
A skipped test or a red CI lane is not failing-first evidence.
Use event/barrier synchronization.
Timeouts are kill switches, not proof of no effect.

### G552-P01-manifest — AC-1

- **Positive and denial observable:** Unmarked legacy `windows` grants zero authority.
  Editing a channel list without the marker grants nothing.
  The diagnostic tells users to review all legacy metadata before opt-in.
  A 64 KiB plus one byte input returns GUARD017.
  Within-cap opted-in schema failures return GUARD005.
- **Single fault / Expected RED:** Grant authority from an unmarked legacy object.
  The legacy fixture must become incorrectly allowed.

### G552-P04-literal — AC-2

- **Positive and denial observable:** Exact `el:rendererReq` matches.
  `el:rendererReqX`, case variants and percent/Unicode variants deny unless listed.
  A 256-byte transmitted suffix is valid.
  The internal `el:` policy literal is 259 bytes at that boundary.
  A 257-byte suffix rejects with GUARD005.
  Test multibyte UTF-8 at both boundaries.
  Empty transmitted channel remains ungrantable.
- **Single fault / Expected RED:** Use prefix matching, case-folding or Unicode normalization.
  An unauthorized variant must become allowed.

### G552-P03-window — AC-3

- **Positive and denial observable:** Only window A's Host-bound grant admits A.
  Window B's grant cannot satisfy A.
  Page-supplied window data is ignored.
- **Single fault / Expected RED:** Use a global grant union or select the key from page JSON.
  The cross-window callback must run.

### G552-P06-native-isolation — AC-4

- **Positive and denial observable:** Both query types compile and dispatch independently.
  No unused handler type is required.
  Renderer `el:fs.read` never enters the native filesystem broker.
  Native AppProcess `fs.read` keeps its existing scope tests.
  Compile-fail cases reject wrong callback types, cross-kind conversion and escaping borrows.
- **Single fault / Expected RED:** Reintroduce a shared two-handler generic bound.
  The independent query compile-positive fixture must fail.
  Separately route a channel query to the native filesystem selector.
  The native-effect regression must fail.

### G552-P02-effect — AC-5

- **Positive and denial observable:** With a live current document and role, a missing grant returns GUARD003.
  Observe zero writer frames and zero Bun listener entries.
  A neighboring allowed control forwards once.
- **Single fault / Expected RED:** Forward outside the Allow closure.
  The independent Bun effect counter must detect the forbidden effect.

### G552-P08-allocator — AC-6

- **Positive and denial observable:** After cold parse and indexing, warmed Allow performs zero heap allocations.
  Measure denial allocation separately.
- **Single fault / Expected RED:** Clone the channel or window key on every Allow.
  The counting allocator must observe an extra allocation.

### G552-P09-explain — AC-7

- **Positive and denial observable:** A hypothetical window returns the same pure result and escaped pointer.
  It writes neither the manifest nor the app link.
  Existing app queries remain unchanged.
- **Single fault / Expected RED:** Use an alternate matcher or leak a live permit.
  The parity or no-effect oracle must fail.

### G552-N02-revoke — AC-8

- **Positive and denial observable:** Pause work at the final pre-Bun boundary.
  If native revocation wins, observe zero frames and zero handler entries.
  A fresh valid control forwards.
  The native UI remains able to progress during blocked transport.
- **Single fault / Expected RED:** Omit the final stamp check or hold the revocation lock across I/O.
  The stale-effect or UI-progress oracle must fail.

### G552-N09b-role-recovery — AC-8

- **Positive and denial observable:** Old work queued across Bun restart cannot reach the successor.
  No replay or credit theft occurs.
  A fresh same-document request works.
- **Single fault / Expected RED:** Select the current link at dequeue without comparing the captured role.
  The successor must observe the old request.

### G552-N11-doc-admission — AC-8 and AC-10

- **Positive and denial observable:** The native qualified document, window and checked-u32 stamp match the Guard principal.
  Foreign documents and unapproved `file:` claims produce no effects.
- **Single fault / Expected RED:** Mint identity from URL or WebView id, or wrap the generation.
  The stale sender must reach the Bun effect oracle.

### G552-C14-two-requests — AC-9 and AC-10

- **Positive and denial observable:** Use genuine draw.io `rendererReq/mainResp` from the real main handler.
  Exercise two distinct request IDs.
  Observe the page callback and independent native/transport release before second admission.
  The denied control has zero effects.
- **Single fault / Expected RED:** Return a canned false value, fake the callback or release credit early.
  The receipt/barrier oracle must fail.

Do not combine all rows into one mock.
GH552 owns pure Guard, dispatcher and diagnostic tests.
GH542 owns real native navigation.
GH540/465/543/466 own transport and application effects.
GH464/567 own pinned conformance evidence.
Run native and failing-first tests only when those owners authorize their source.
A synthetic callback is not AC-5 or AC-10 evidence.

## 9. Perf impact

No speed or memory improvement is claimed.
The target is zero heap allocations for warmed exact Allow in the policy/dispatcher decision.
Use a counting allocator as the oracle.
Cold JSONC parsing and indexing may allocate within the existing 64 KiB cap.
Denial formatting may allocate.
Measure transport serialization separately.

GH540 owns the 4 KiB first-proof payload.
Its transmitted channel name is limited to 256 UTF-8 bytes.
The internal `el:` policy literal may therefore contain 259 bytes.
GH540 also owns one DATA record per direction, reserved CONTROL capacity,
per-hop retained-byte/scratch bounds and terminal native release.
Do not count transport allocations as Guard allocations.
Do not add a second queue or ledger.

After implementation, measure cold parse, warm Allow, denial, final Host handoff
and real native latency as separate samples.
Record actual values before making any performance claim.
No diagram or benchmark threshold is required in this documentation-only PR.

## 10. Open questions, blockers & unknowns

### Q-1: Public Rust query shape and callsite compatibility

**Owner:** Amisha, GH552 and the existing native-dispatch owner.
Approve two independently inferable query types:
`NativeCapabilityQuery<'a, H>` and `RendererChannelQuery<'a, H>`.
Use one sealed internal dispatch adapter and one public `dispatch_privileged` entry.
Keep the Host-selected grant borrow and disjoint callback permit types.
Alternatively, select a comparably narrow type with compile-positive and compile-fail proof.

The previous `GuardQuery<'a, NativeHandler, ChannelHandler>` enum is not acceptable.
Its inactive generic handler has no inferred type.
It also must satisfy the opposite `FnOnce` bound.
Approve a lint-clean sealed implementation.
Do not export another callable dispatch method.
Decide whether native AppProcess callsites may be adapted in the future GH552 PR.
If not, name the separately claimed owner change.
No renderer-query/native-capability fallback is allowed.

### Q-2: MCP public diagnostic input

**Owner:** Amisha and GH552.
Approve the additive `hypothetical_window_key` field.
Approve the conditional `principal="webview"` and `channel` shape.
Approve the pointer and invalid-combination errors.
Or provide the exact alternative before code.
Confirm that hypothetical selection never enters a live effect path.
It returns neither live permit type.
It does not change frozen GUARD003 wire text.

### Q-3: Core adapter boundary and final order

**Owner:** GH552, GH542 and GH540.
Approve the narrow Core forwarder scope.
Name the source of the Host-current window grant handle.
Name the native document stamp/liveness accessor.
Define the captured app-link generation and nonblocking final order.
GH542 must separately approve its checked-u32 native implementation and N11 proof.
GH540/465 must separately qualify the v2 link and WorkerLink.
Pure GH552 tests do not prove composed Bun effects.

### Q-4: MCP and manifest release compatibility

**Owner:** GH541.
Approve or replace the exact `"$keldWindowChannelGrants": 1` opt-in.
Approve a matching GH541 P01 amendment before implementation.
V0 ignores all `windows` shapes, including scalar `{"windows":"legacy"}`.
GH541's strict wrong-shape rule conflicts with preserving those files.
Until approval, legacy files keep their old parse behavior and grant zero channel authority.
Acceptance must cover legacy scalar/object metadata, absent/wrong marker,
valid and invalid opted-in grants, unrelated metadata and unchanged native `app` rules.
No auto-migration, new error code or mutable hot reload is allowed.
Coordinate generator, tool-schema and shared native-caller changes with their owners.

### Q-5: Upstream merge and independent source gates

**Owner:** GH541, GH540, GH464, GH542 and GH465 owners.
GH541 and GH540 remain open draft PRs.
Their owners retain merge authority.
Completion also depends on GH464/GH542/GH465 prerequisites.
Recheck exact blobs and claims before each future PR.
GH552 approval does not override upstream dependencies.

### B-1: GH541 compatibility amendment

- **Status:** open
- **Owner:** GH541 owner
- **Impact:** No parser implementation or channel-grant enablement may proceed.
- **Check:** Record exact-content approval for the opt-in marker, strict schema and migration fixture.

### B-2: GH552 permission-model and public-API review

- **Status:** open
- **Owner:** Amisha, independent reviewer
- **Impact:** This spec cannot change from draft to approved.
- **Check:** Record exact-head permission and public-API decisions for Q-1 and Q-2.

### B-3: GH542 native document identity

- **Status:** open
- **Owner:** GH542 owner
- **Impact:** No real current-document or final-revocation acceptance is available.
- **Check:** Pass the real native qualified-document and checked-u32 N11 acceptance.

### B-4: GH540 and v2 transport lifecycle

- **Status:** open
- **Owner:** GH540 and GH465/543/466 owners
- **Impact:** No canonical v2 send, role retirement or end-to-end Bun delivery may be claimed.
- **Check:** Pass the exact approved decoder, WorkerLink, subscription and terminal-release acceptance.

### B-5: Conformance and evidence boundary

- **Status:** open
- **Owner:** GH464 and GH532 owners
- **Impact:** No Electron compatibility or draw.io product acceptance may be claimed.
- **Check:** Reconcile the pinned conformance cells and independent evidence receipts.

**Next action:** The independent permission-model and public-API reviewer must decide Q-1 and Q-2.
The GH541 owner must decide Q-4.
Only then may the owner update this document to approved.
No GH552 production code is authorized by this DRAFT.
