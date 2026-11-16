# Contract and acceptance

Part of: [index](index.md)
Owns: sections 2 and 3

## 2. Spec refs

### GH541 — grant and navigation amendment

**FACT:** [PR #1123](https://github.com/gyldlab/keld/pull/1123) remains open and draft at head `20e1dff14578a4287674e64a5b18e5a381ed4b44`.
Its current spec blob is `cccb85a1fed9650292b905b793cb585345074d2c`.
The [historical owner-delegated approval](https://github.com/gyldlab/keld/issues/541#issuecomment-6373759995) covers blob `05a4348a3af4ec0856dcab601e2f719d59295143`.
That approval is bound to head `7e0c1b89c4767f2e6625cd2bfc4d36d86b30fc26`.
Its decision digest is `2da2c23a53848d618f08612a1106d53283b7488601be7fd3eaa5ed95038df5af`.
The current PR blob differs from the approved blob.
The current PR does not contain the proposed GH552 opt-in marker.
The current strict `windows` rule conflicts with the compatibility proposal in this spec.
The GH541 owner must approve an exact-content amendment before implementation consumes the opt-in.

**FACT:** The historical approval is not GH552 or GH542 implementation approval.
It covers the cited GH541 contract and its P01–P10 rules only.
The current GH541 draft requires refreshed exact-content approval before consumption.

### GH540 — bounded compatibility IPC

**FACT:** [PR #1122](https://github.com/gyldlab/keld/pull/1122) remains open and draft at head `ffd6d16811d684305abd3c17fdbf5c868965ede1`.
Its current spec blob is `a166e18fa8410e21d618ddd0d65c6a2be8d616f2`.
The [historical owner approval](https://github.com/gyldlab/keld/issues/540#issuecomment-6371793024) covers blob `fe9b0aa41bfc34e478cd9cd150bf40fc08ab412b`.
That approval is bound to commit `3d72e492916259d89b1bf4723329e602ea6e1691`.
The current PR blob differs from the historically approved blob.
Refresh approval before relying on changed content.

**FACT:** The approved GH540 contract owns one family-neutral `compat-control` entry.
It owns v2 `send/consumed`, opaque endpoint bytes and DATA/CONTROL reservations.
It also owns authenticated WorkerLink and terminal release.
It does not grant channel authority.

### KEL-102 and architecture 03

**FACT:** KEL-102 owns the Host-selected immutable policy snapshot.
It owns trusted principals, native capability scopes and shared guard-before-handler dispatch.
An existing `/app` grant never becomes WebView authority.
GH541 owns its architecture and KEL-102 amendments.
See [KEL-102](../kel102-host-guard-enforcement.md) and [architecture 03](../../architecture/03-security.md).

### GH542 and KEL-142

**FACT:** GH542 owns the checked `u32` navigation generation.
It also owns native application-document binding and host-load attempt correlation.
Its owner retains lifecycle and recovery.
See the [GH542 owner issue](https://github.com/gyldlab/keld/issues/542) and the [KEL-142 renderer bridge](../kel142-macos-renderer-bridge-api/index.md).
GH542 must approve its implementation contract separately.
Its proposed app/trusted credentials remain unqualified.
GH552 does not invent those credentials.

### Other transport and application owners

**FACT:** [GH465](https://github.com/gyldlab/keld/issues/465) and [GH540](https://github.com/gyldlab/keld/issues/540) own canonical v2 native ingress and WorkerLink carriage.
[GH543](https://github.com/gyldlab/keld/issues/543) and [GH466](https://github.com/gyldlab/keld/issues/466) own reply and event delivery.
[GH544](https://github.com/gyldlab/keld/issues/544) owns endpoint codec rules.
[GH449](https://github.com/gyldlab/keld/issues/449) owns subscription ownership.
GH552 receives a validated channel and opaque bytes after their admission.
GH552 does not implement those protocols.

### Current source baseline

**FACT:** This specification is pinned to main commit `ec29b7ce2b155a8c98f96036cbb9c54488b5c302`.
See the [GH552 source-evidence handoff](https://github.com/gyldlab/keld/issues/552#issuecomment-6391174207) and [architecture 02](../../architecture/02-ipc.md).
At that pin, `PermissionsManifest` contains only the `app` field.
Its source comment says unknown top-level `windows` keys are ignored.
See `crates/keld-guard/src/lib.rs:621-629`.
The same evaluator rejects non-AppProcess principals with GUARD006.
See `crates/keld-guard/src/lib.rs:761-792`.
The shared `dispatch_privileged` callback accepts only `&ScopePermit`.
See `crates/keld-ipc/src/guard_dispatch.rs:48-70`.
The WebView decoder accepts v1 Bind and Invoke envelopes.
See `crates/keld-wv/src/wv_link.rs:139-197`.
The macOS bridge stores a wrapping `u64` navigation generation and one nonce.
See `crates/keld-wv/src/wkwebview/macos_bridge.rs:323,357-389`.
The Core renderer route forwards Echo replies.
See `crates/keld-core/src/app_session.rs:4517-4523`.
The MCP explain API rejects non-app principals and present channel fields.
See `crates/keld-cli/src/mcp/permissions.rs:14-33,90-132`.

**INFERRED:** These source facts do not implement GH552 channel grants.
They do not implement a checked-u32 navigation mint or v2 Send.
No source behavior is approved by this specification.

### Conformance and evidence owners

**FACT:** GH464 owns Electron compatibility evidence.
GH532 owns evidence rules.
GH566 owns manifest evidence.
Their failing-first conformance cells remain pending.
Missing native evidence cannot be scored PASS.
GH552 pure tests cannot promote unqualified Electron rows.
See [GH464](https://github.com/gyldlab/keld/issues/464), [GH532](../gh532-first-proof-evidence-rules.md) and [GH566](../gh566-corpus-manifest-owner.md).

If GH541 merges or changes before GH552 approval, refresh these pins.
Compare the merged content with the exact approved artifact.
Do not rewrite historical approval identity.
Resolve any cross-spec discrepancy through its owner before implementation.

## 3. Acceptance criteria

### AC-1: Strict immutable policy admission

- **Given:** One Host-loaded JSONC policy within the existing 64 KiB input cap.
- **When:** The Host parses the policy with the marker absent or exactly integer `1`.
- **Then:** An absent or unsupported marker preserves v0 `windows` metadata and grants zero channel authority.
  Exact integer `1` enables strict window and channel validation.
  Missing `windows`, window, `channels` or an empty list grants no authority.
  An input above 64 KiB returns `ManifestError::TooLarge` / `KELD-GUARD017` before parsing.
  Malformed opted-in content within the limit rejects the whole manifest with `KELD-GUARD005`.
  The Host captures the verified snapshot once per session.
  A same-session file edit cannot widen authority.
- **Negative control:**
  - **Single fault:** Treat a grant-shaped unmarked `windows` object as active authority.
  - **At-risk precondition:** V0 accepts unknown top-level `windows` metadata.
  - **Expected RED:** The unmarked legacy fixture gains a channel grant.

### AC-2: Exact decoded literal grammar

- **Given:** An opted-in policy with decoded channel strings.
- **When:** The Host validates and later compares a requested channel.
- **Then:** A grant starts with literal ASCII `el:` and has a nonempty suffix.
  The suffix has at most 256 UTF-8 bytes.
  The internal grant has at most 259 bytes including the three-byte prefix.
  Reject wildcard, NUL, C0/C1 controls, DEL and malformed Unicode.
  Do not trim, case-fold, normalize Unicode, percent-decode or use prefix matching.
  Duplicate grants add no authority.
  Empty transmitted channel names remain ungrantable.
  Reject an opted-in oversized suffix with `KELD-GUARD005`.
- **Negative control:**
  - **Single fault:** Replace exact equality with prefix matching.
  - **At-risk precondition:** A valid grant shares a prefix with an ungranted channel.
  - **Expected RED:** `el:rendererReqX` becomes allowed by `el:rendererReq`.

### AC-3: Host-bound window selection

- **Given:** Two Host-bound windows with separate immutable grant selections.
- **When:** A WebView request includes page-controlled window metadata.
- **Then:** The Host uses only its verified WebView-to-window association.
  A grant for window B cannot satisfy a request from window A.
  A page cannot select a window, URL, WebView id, role, principal, nonce or header.
  Reusing a numeric WebView id cannot revive an old selection.
- **Negative control:**
  - **Single fault:** Select the window key from page-controlled request data.
  - **At-risk precondition:** Window B has a grant that window A does not have.
  - **Expected RED:** A request from A is admitted using B's grant.

### AC-4: Typed channel and native capability separation

- **Given:** One native capability query and one renderer channel query.
- **When:** Each query enters the shared policy evaluator and dispatcher.
- **Then:** Each query has its own inferred callback type.
  Native callbacks accept only `&ScopePermit`.
  Renderer callbacks accept only `&RendererChannelPermit`.
  Neither permit converts to the other.
  A grant `el:fs.read` authorizes only the Bun message named `fs.read`.
  It cannot authorize a native filesystem or media operation.
  Existing AppProcess `/app/fs/*` behavior remains unchanged.
- **Negative control:**
  - **Single fault:** Permit a renderer channel permit in the native callback.
  - **At-risk precondition:** A renderer grant names `el:fs.read`.
  - **Expected RED:** The compile-fail type check or native effect oracle fails.

### AC-5: Deny an ungranted current request

- **Given:** An authentic current original document, correct Host window binding, valid bounded v2 request and usable Bun role.
- **When:** The window has no exact `el:rendererReq` grant.
- **Then:** Return the existing `ChannelForbidden` reason and `KELD-GUARD003`.
  Preserve kind `channel_forbidden` and the frozen fix text:
  "Add `{channel}` to this principal's channels list in keld.permissions.jsonc."
  Do not change the existing `CallError { code, message }` mapping.
  A missing opt-in marker adds only separate non-wire migration guidance.
  The denied callback never runs.
  Independent observers record zero app-link frames and zero Bun listener entries.
  A later current and granted request forwards once.
  Identity or unavailable-link refusal takes precedence over a grant miss.
- **Negative control:**
  - **Single fault:** Invoke the forwarding closure before the Guard decision.
  - **At-risk precondition:** The authentic current document lacks the requested grant.
  - **Expected RED:** The independent writer or Bun listener observes a forbidden effect.

### AC-6: Borrowed warmed Allow

- **Given:** A parsed immutable snapshot and a warmed policy index.
- **When:** The exact allowed channel query reaches the dispatcher.
- **Then:** The policy and dispatcher decision performs zero heap allocations.
  Measure transport serialization separately.
  Keep permits callback-scoped.
  Do not return or cache a permit beyond the callback.
  Measure cold parsing and denial formatting separately.
- **Negative control:**
  - **Single fault:** Clone the channel or window key on every Allow.
  - **At-risk precondition:** The warmed Allow path is measured with a counting allocator.
  - **Expected RED:** The allocator records an additional allocation.

### AC-7: Diagnostic-only explain parity

- **Given:** An offline manifest and a hypothetical window selector.
- **When:** The CLI/MCP explain request evaluates a renderer channel.
- **Then:** Use the same pure evaluator as live dispatch.
  Return the same allow or deny result and channel spelling.
  Return a trusted pointer at `/windows/<key>/channels`.
  Escape `~` as `~0` before escaping `/` as `~1`.
  Never mint a live principal, select a Bun link or return either permit.
  Never invoke an effect or write the manifest.
  Preserve existing app-only explain callers.
- **Negative control:**
  - **Single fault:** Construct a live renderer permit from the hypothetical selector.
  - **At-risk precondition:** The explain query names a valid window and channel.
  - **Expected RED:** The no-effect or permit-construction oracle fails.

### AC-8: Final identity and revocation ordering

- **Given:** A qualified native document stamp, Host window key and authenticated Bun role captured at admission.
- **When:** The request reaches the final nonblocking pre-Bun handoff.
- **Then:** Order revocation against admission at one owner.
  Recheck the full captured identity before the single dispatcher call.
  If revocation or role retirement wins, write zero Bun frames.
  Do not retarget old work to a successor role or document.
  Do not automatically replay the request.
  If dispatch wins first, preserve an honest outcome-unknown result.
  Do not promise rollback or exactly-once external effects.
  Do not hold a native UI lock across blocking I/O.
- **Negative control:**
  - **Single fault:** Remove the final captured-document or role-generation check.
  - **At-risk precondition:** Old work remains queued across navigation or role replacement.
  - **Expected RED:** The old request reaches the successor document or role.

### AC-9: Preserve outbound and native authority

- **Given:** A renderer channel grant for incoming messages.
- **When:** The app uses `mainResp`, `event.reply` or `webContents.send`.
- **Then:** Preserve existing outbound document and frame targeting.
  Do not inherit incoming grants for outbound messages.
  Do not broadcast through a window grant.
  Do not authorize native filesystem or media work.
  Preserve GH540's canonical v2 channel, credits, body caps and terminal release.
- **Negative control:**
  - **Single fault:** Reuse the incoming channel grant for native filesystem admission.
  - **At-risk precondition:** A window has an incoming grant named `el:fs.read`.
  - **Expected RED:** The native capability oracle records an unauthorized effect.

### AC-10: Composed positive and negative product proof

- **Given:** Implemented GH541, GH542 and GH540 prerequisites on an authorized macOS system.
- **When:** A verified application document sends denied and allowed requests.
- **Then:** The denied request produces zero app-link writes and zero Bun entries.
  The allowed request produces one actual Bun handler entry.
  A revocation winner produces zero writes.
  A retired role cannot deliver work to its successor.
  A fresh granted request remains healthy.
  Record exact source, OS, fixture and failing-first evidence.
  Pure policy tests do not establish native or product acceptance.
- **Negative control:**
  - **Single fault:** Replace the real Bun handler with a canned result.
  - **At-risk precondition:** The test otherwise uses a real native document and app-link.
  - **Expected RED:** The independent handler-entry oracle detects the missing effect.
