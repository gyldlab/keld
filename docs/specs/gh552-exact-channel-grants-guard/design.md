# Design

Part of: [index](index.md)
Owns: section 4

## 4. Design

### 4.1 First-principles atoms, reuse and failure isolation

### D-1: Policy admission

- **Owner:** GH552, `keld-guard`.
- **Boundary:** One Host-verified immutable JSONC snapshot.
- **Input:** Existing `PermissionsManifest` parsing and the proposed opt-in marker.
- **Output:** A closed window/channel table and unchanged app scopes.
- **Failure:** A malformed opted-in policy rejects the whole manifest.
- **Observable:** The existing size and parse errors remain distinct.
- **Negative control:** Ignore a decoded wildcard or explicit null. The malformed manifest must then load.

### D-2: Authority identity

- **Owner:** GH542, `keld-wv`.
- **Boundary:** Native WebView, frame, world and committed document.
- **Input:** Actual native view/controller/frame/content-world and navigation state.
- **Output:** One current checked generation and separate realm credentials.
- **Failure:** A stale or unqualified document cannot bind.
- **Observable:** A forged or retired sender produces zero Host and Bun effects.
- **Negative control:** Bind from URL or WebView id alone. The hostile-frame test must fail.

### D-3: Window binding

- **Owner:** GH552 Host consumer, using the GH542 native association.
- **Boundary:** One immutable window selection from the session snapshot.
- **Input:** Verified creation configuration and current WebView identity.
- **Output:** A borrowed Host-resolved grant selection.
- **Failure:** A page cannot select a window or global grant union.
- **Observable:** Window B's grant cannot authorize window A.
- **Negative control:** Read the window key from page JSON. The cross-window test must fail.

### D-4: Channel decision

- **Owner:** GH552, `keld-guard`.
- **Boundary:** The existing shared evaluator and dispatcher.
- **Input:** Host-bound WebView principal, exact topic and borrowed selection.
- **Output:** A channel-only permit or GUARD003.
- **Failure:** Prefix matching or native permit conversion is forbidden.
- **Observable:** Only an exact listed literal reaches the renderer callback.
- **Negative control:** Replace equality with prefix matching. An unlisted suffix must become incorrectly allowed.

### D-5: Final effect

- **Owner:** GH552, `keld-ipc` and a narrow `keld-core` adapter.
- **Boundary:** One nonblocking final app-link handoff.
- **Input:** Live original-document stamp, window selection and captured Bun role.
- **Output:** One bounded app-link write after a single Guard Allow.
- **Failure:** Revocation or role retirement before admission means zero writes.
- **Observable:** Independent writer and Bun-listener counters prove the effect.
- **Negative control:** Skip the final identity check. The stale-request test must fail.

### D-6: Carriage and settlement

- **Owner:** GH540 and GH465/543/544.
- **Boundary:** Existing canonical v2 decoder, WorkerLink and credit owner.
- **Input:** Bounded v2 envelope and opaque endpoint bytes.
- **Output:** An admitted or terminal event with ordered release.
- **Failure:** No second parser, ledger, reader or early credit release.
- **Observable:** Existing quota, role-retirement and native-release oracles remain authoritative.
- **Negative control:** Release credit before native completion. The retained-work test must fail.

### D-7: Explain only

- **Owner:** GH552, `keld-cli`.
- **Boundary:** Offline hypothetical query only.
- **Input:** Parsed snapshot, hypothetical window key and exact channel.
- **Output:** The same pure decision. Q-2 owns any extra offline pointer and its presence rules.
- **Failure:** No live principal, permit, Bun link or effect callback.
- **Observable:** The manifest and app link remain unchanged.
- **Negative control:** Return the offline Allow permit alongside the diagnostic result. The public result-type fixture must fail compilation.

### D-8: OS containment

- **Owner:** KEL-78.
- **Boundary:** Existing OS execution profile.
- **Input:** The already admitted execution profile.
- **Output:** An independently qualified confinement result.
- **Failure:** A channel grant cannot stand in for OS confinement.
- **Observable:** Native OS/profile controls remain separate from Guard tests.
- **Negative control:** Treat a channel Allow as sandbox proof. The OS control must reject that claim.

### D-9: Evidence provenance

- **Owner:** GH552 with GH541 and GH464 owners.
- **Boundary:** Exact source commits and owned acceptance criteria.
- **Input:** Pinned source, native/transport observations and one-fault mutations.
- **Output:** A scoped pass, fail or unknown result.
- **Failure:** A stub or synthetic page cannot prove a Bun effect.
- **Observable:** Each receipt binds the exact source, platform and independent oracle.
- **Negative control:** Replace the real handler with a canned result. The effect oracle must fail.

**INFERRED — reuse decision:** The existing unique-key JSONC loader must remain the parser.
Guard owns pure `evaluate_query` on the selected candidate. IPC owns the sole callback invocation.
The canonical kipc receiver and native identity mint also remain in their current owners.
Do not add a second policy parser, global window registry, JS permission cache, protocol,
socket, transport reader or helper-only authority.
No measured performance result justifies a rewrite.
The target is semantic correctness and the existing warmed zero-allocation Allow path.

### 4.2 Policy grammar and snapshot capture

The proposed admitted policy fragment is shown below.
It is illustrative and does not claim that draw.io needs every listed topic.

~~~json
{
  "$keldWindowChannelGrants": 1,
  "windows": {
    "editor": { "channels": ["el:rendererReq"] },
    "about": { "channels": [] }
  }
}
~~~

**FACT:** At the pinned source, `PermissionsManifest` contains only `app`.
It ignores unknown top-level `windows` metadata.
See `crates/keld-guard/src/lib.rs:621-629`.

**INFERRED — compatibility proposal:** Require exact top-level marker
`"$keldWindowChannelGrants": 1` before interpreting `windows` as channel authority.
The selected Windows Legacy artifact adopts P01 deliberately. B-1 requires the owner's exact-text decision before implementation.
The proposal has three cases:

- If the marker is absent or unsupported, ignore channel-list contents for channel admission.
  Preserve the independent loader's size, duplicate-key, window structure, and response-policy checks.
  The selected Windows candidate requires window objects even without channel opt-in.
  The v0 pin accepts scalar metadata because it has no window response-policy contract.
  That historical observation grants no adoption or compatibility guarantee for another artifact.
  Preserve existing `app` grants and unrelated metadata.
  Grant zero incoming channel authority, even for grant-shaped legacy objects.
  Never infer opt-in from a `channels` key, document, page or CLI argument.
- If the marker is exactly `1`, validate `windows` after unique-key JSONC decoding.
  Every named window must be an object.
  A present `channels` field must be an array of strings.
  Wrong types, explicit null, non-string members, wildcards, controls, malformed Unicode,
  duplicate decoded keys or oversized suffixes reject the entire opted-in manifest.
  Use the existing `KELD-GUARD005` error.
  Do not filter bad entries or apply `deny_unknown_fields` to the whole manifest.
- Missing `windows`, a missing window, missing `channels` and empty lists grant nothing.
  They never inherit `app` scopes or another window's grants.
  Unrelated top-level metadata remains tolerated.
  No automatic rewrite or migration of stored user data is allowed.

**FACT:** The scoped Windows choice still needs the GH541 owner's exact-content record for B-1.
The GH541 owner must approve a matching P01 change and migration fixture.
Until then, no GH552 parser implementation or channel-grant enablement may proceed.
Existing stored policy files need no rewrite.
A user who wants channel grants must explicitly adopt the approved marker and schema.

Cold snapshot validation, string allocation and indexing remain within the existing 64 KiB cap.
The Host captures one immutable verified snapshot at session creation.
A later file edit cannot reparse the snapshot or widen authority.
Compare decoded UTF-8 bytes exactly.
Escaped JSON letters decode to the same literal.
An escaped wildcard still rejects the whole opted-in policy.
Duplicate strings add no authority. The first exact match is deterministic.
Do not trim, case-fold, normalize, percent-decode or apply path, regex or prefix rules.
Malformed surrogate data fails during decoding and is never replaced.

GH540 limits the transmitted unprefixed channel name `c` to 256 UTF-8 bytes.
The adapter adds one internal `el:` prefix of three ASCII bytes.
A valid policy grant therefore has 1–256 suffix bytes and 4–259 total bytes.
Reject a suffix above 256 bytes during policy admission.
Do not increase the transport limit or strip a second prefix.
Count UTF-8 bytes, not scalar values or JSON escape syntax.

### 4.3 Proposed typed query and one authoritative dispatcher

**INFERRED:** Reuse the selected Guard's sealed `GuardQuery`, existing query types, and associated permits.
Guard owns only pure evaluation. IPC alone invokes the callback.
The proposed typed entry resides at `keld_ipc::guard_dispatch::typed::dispatch_privileged`.

~~~rust
pub fn dispatch_privileged<Q: GuardQuery, T>(
    manifest: &PermissionsManifest,
    principal: Principal,
    query: Q,
    handler: impl for<'p> FnOnce(&'p Q::Permit) -> T,
) -> Result<T, DenyReason>
~~~

Keep `keld_ipc::guard_dispatch::dispatch_privileged<T>` as the five-argument native compatibility function.
It constructs `NativeCapabilityQuery { operation, path }` and forwards the original handler.
It performs no evaluation, path validation or callback invocation.
Use the typed function for the renderer.
Only that body calls `evaluate_query` and invokes the handler after Allow.
Preserve native principal refusal before filesystem grammar and filesystem grammar before grant-shape errors.
Use the existing `GuardQuery::native_capability` to select the existing IPC filesystem validator.
Renderer queries never enter native filesystem validation.
The renderer query borrows immutable window grants from the same manifest.
Native and renderer callbacks borrow distinct permits and cannot return those borrows.
Use compile-positive checks for each query and unchanged native callers.
Use compile-fail checks for crossed permit types and escaped borrows.
The callback owner adds no heap allocation to warmed Allow.

A current valid channel miss returns the existing `ChannelForbidden` variant.
Preserve its GUARD003 code, kind and fix text from GH541.
Do not add a JSON pointer to the frozen wire `CallError`.
The optional pointer belongs only to trusted diagnostics or MCP output.
Native filesystem path checks remain on `NativeCapabilityQuery`.
Renderer topics are not filesystem paths.

### 4.4 Final handoff, generation and error ordering

A renderer value is not proof of identity.
The native `keld-wv` owner validates the content world, frame and WebView.
It also validates the committed application document and GH542 checked-u32 generation.
The same stamp supplies `Principal::Webview.generation`.
Capture the authenticated Bun role/link generation at admission, not at dequeue.

The narrow GH552 Core adapter is the only new consumer for generic renderer forwarding.
It must perform these steps:

1. Refuse malformed input at the existing GH465 canonical decoder.
   Require verified native identity and a current document.
   Do not trust or reparse page-supplied target, window or generation values.
2. Borrow the immutable Host-current window grant selection.
   If identity, eligibility, link availability or generation is stale, return its typed
   no-effect or outcome-unknown result before checking for a missing grant.
3. Bound, charge and copy opaque payload metadata through GH540's existing owners.
   At the final nonblocking handoff, order revocation against forwarding.
   Compare the captured document and authenticated Bun role with current authority.
   Then call `guard_dispatch::typed::dispatch_privileged` exactly once.
4. Only the Allow closure may begin the original app-link write.
   A grant denial invokes no handler and writes no Bun frame.
   If revocation or role replacement wins first, never retarget old work.
   If dispatch already won, do not invent rollback.
   Release accounting only when the operation is quiescent.

Never block the UI/navigation owner while waiting for Bun, a writer queue or socket I/O.
`event.reply` and `webContents.send` remain GH543/GH466 outbound delivery.
`event.reply` retains the original document and frame.
`webContents.send` captures its current target at send admission.
These routes do not inherit incoming `windows` permissions or broadcast authority.
GH542 must qualify application-document admission and native attempt correlation.
GH540/465 must qualify one v2 decoder, link, event route and credit retirement.
GH552 cannot claim AC-5, AC-8 or AC-10 before those dependencies pass.

### 4.5 Diagnostic-only MCP API proposal

**FACT:** The current MCP explain API requires `principal="app"`.
A different principal returns `KELD-MCP012`.
A present channel returns `KELD-MCP014`.
There is no window selector.
See `crates/keld-cli/src/mcp/permissions.rs:14-33,90-132`.

**INFERRED — proposed additive request:** Keep the existing app query unchanged.

~~~json
{
  "manifest_path": "/path/to/keld.permissions.jsonc",
  "operation": {
    "principal": "webview",
    "channel": "el:rendererReq",
    "hypothetical_window_key": "editor"
  }
}
~~~

This is an offline hypothetical diagnostic.
The caller asks what a Host-selected window would receive.
The selection never authorizes an actual WebView.
The query needs no running Host and mints no native principal.
It selects no live Bun link and returns neither live permit type.
It uses the same pure evaluator on a separately loaded offline snapshot.
  Convert the public diagnostic channel to its operand by removing exactly one leading `el:` prefix.
On denial, it returns the same GUARD003 code, message and fix.
Q-2 must specify whether each valid outcome returns a separate diagnostic pointer.
If present, construct `/windows/<escaped-key>/channels`.
Escape `~` as `~0`, then `/` as `~1`.
Select grants from the offline manifest. The hypothetical numeric WebView is local input to pure evaluation.
Drop the pure result after projecting Allow or the frozen denial into the diagnostic response.
The offline path cannot borrow a live Host snapshot, dispatch a handler, or export a permit.
A suggested patch is advisory and never changes the manifest.

When the marker is absent or unsupported, keep the frozen GUARD003 `CallError` unchanged.
Add migration guidance only as a separate non-wire diagnostic.
State that adding a channel alone grants nothing.
Tell users to review all legacy `windows` metadata before opt-in.
Do not append this guidance to `CallError.message`.
Do not infer grants or auto-migrate.
This API shape requires independent public-API approval.

Requests without a channel keep the v0 app route.
A WebView principal without a hypothetical key or channel is invalid.
An app principal with a hypothetical key is invalid.
A channel combined with native capability arguments is invalid.
Use typed MCP validation errors, never a made-up Allow.
During the Windows stage, keep the existing CLI/MCP API and its validation order.
An unknown principal gives MCP012 before the present-channel check gives MCP014 for app requests.
Both refusals precede manifest loading. Q-2 must decide intersecting future request errors before T-4.
The reviewer must approve the field names and failure precedence.
No diagnostic result can enter the authoritative dispatch closure.

### 4.6 Lifecycle, platform, migration and current-code limitations

**FACT:** The current source boundary is pinned in section 2.
The WebView decoder accepts v1 Bind/Invoke only.
The macOS bridge uses a wrapping u64 counter and one nonce.
Core forwards an Echo-only renderer request.
These facts do not qualify a generic renderer-to-Bun path.

The existing supervised WorkerLink, bounded ring and primary-role authentication remain mandatory.
GH540 owns compatibility credits and authenticated delivery.
macOS is the required real-native first-proof lane.
Pure tests may run on macOS, Linux or Windows.
They do not qualify WebView2 or WebKitGTK native behavior.
KEL-78 owns OS sandbox and strict confinement.

**INFERRED — migration rule:** Do not rewrite stored user data automatically.
For the selected Windows Legacy artifact, keep native `/app` scopes and the existing response-policy contract.
Without the approved marker, even grant-shaped legacy metadata grants nothing.
Adding a channel while the marker is absent or unsupported grants no authority.
Before deliberate adoption, review the selected final artifact's entire literal list and CSP.
Do not infer, import or silently convert legacy entries.
New opt-in documents require an intentional user/configuration change.
GH541 must approve the schema amendment and migration fixture first.
GH499 owns `keld migrate` grants-source logic.
Existing MCP callers keep v0 behavior.
No temporary independent evaluator may persist.
