# GH541 acceptance criteria

Part of: [index](index.md)
Owns: §3, criteria 1–22

## 3. Acceptance criteria

Each criterion names an observable, not an implementation assertion. Section 7 maps them to tests.

The numbered criteria below retain their original numbers. Each Then field contains the original criterion.
INFERRED: The other fields describe a witness for each criterion from its text and the [test plan](evidence.md). The test plan retains all required controls and their IDs.

### AC-1: Verified policy snapshot

- **Given:** The selected Windows Legacy artifact and its marker-stripped control.
- **When:** Load the paired verified snapshots, then query the same literal.
- **Then:**
  An explicitly opted-in valid policy with exact channel strings loads once from the verified host-selected
  snapshot. Missing `windows`, a missing window, missing `channels`, or `[]` grants nothing.
- **Negative control:**
  - **Single fault:** P01: ignore the marker.
  - **At-risk precondition:** The exact literal is listed in both snapshots.
  - **Expected RED:** The stripped snapshot makes the listed literal eligible.

### AC-2: Whole-policy rejection

- **Given:** An opted-in policy with a malformed channel list.
- **When:** Load the policy before application resources.
- **Then:**
  In an opted-in policy, a malformed windows/channel structure, non-string member, decoded wildcard, invalid
  channel spelling, or duplicate decoded object key rejects the whole policy with `KELD-GUARD005` before
  application resources. There is no filtering, partial policy, or fallback.
- **Negative control:**
  - **Single fault:** P01: disable decoded-string validation.
  - **At-risk precondition:** The opted-in list contains a decoded wildcard.
  - **Expected RED:** The malformed policy loads instead of yielding GUARD005.

### AC-3: Guard before forwarding

- **Given:** An authenticated current document with a usable matching Bun link.
- **When:** Request an unlisted channel and its exact permitted control.
- **Then:**
  Given an authentic current document bound to window A and a usable matching app link, an unlisted channel yields
  the existing `ChannelForbidden` contract and zero app-link writes and zero Bun handler entries. A listed exact
  literal in the same fixture produces one observed forwarding operation.
- **Negative control:**
  - **Single fault:** P02: omit the shared dispatch check.
  - **At-risk precondition:** The exact denied request reaches the forwarding boundary.
  - **Expected RED:** The app-link observer records a write for the denied request.

### AC-4: Per-window isolation

- **Given:** Two host-authenticated windows have different grant lists.
- **When:** Request a literal granted only to the other window.
- **Then:**
  The same literal in window B, `/app` scopes, another role, a renderer field, or a dynamically registered handler
  cannot grant window A authority.
- **Negative control:**
  - **Single fault:** P03: use a global grant union.
  - **At-risk precondition:** Window B lists the literal and window A omits it.
  - **Expected RED:** The observer records ingress from window A.

### AC-5: Exact literal matching

- **Given:** A window grants one exact decoded literal.
- **When:** Compare the listed spelling with a distinct valid spelling.
- **Then:**
  Prefix, suffix, case, whitespace, percent-encoding, and Unicode-normalization variants do not match unless
  independently listed as valid distinct literals. JSON escape variants of the same decoded string are the same
  literal.
- **Negative control:**
  - **Single fault:** P04: use prefix matching.
  - **At-risk precondition:** The requested channel extends the listed name.
  - **Expected RED:** The unlisted suffix variant is authorized.

### AC-6: Dispatcher ownership

- **Given:** A channel request reaches the authority-bearing dispatch path.
- **When:** Check the shared authority owner and a real denied effect.
- **Then:**
  Every authority-bearing channel path calls `dispatch_privileged`; its callback is the sole place that can
  forward the message. The typed dispatcher calls Guard-owned `evaluate_query`. The existing native call signature
  delegates to that dispatcher. Diagnostic explain is the named non-dispatch exception. Repository-wide
  sole-caller completion additionally requires the accepted KEL-102/T4 media-integration evidence; this channel
  amendment does not waive that gate.
- **Negative control:**
  - **Single fault:** P07: add a direct authority-bearing evaluator caller.
  - **At-risk precondition:** A channel admission can reach that caller.
  - **Expected RED:** The callsite review finds channel authority outside the shared dispatcher.

### AC-7: Allocation-free allowed dispatch

- **Given:** The actual permitted policy and dispatcher path is warm.
- **When:** Measure allocations on that path.
- **Then:**
  The warmed exact-allow policy/dispatcher path performs zero heap allocations, measured with a counting allocator
  around the actual path. Channel serialization and transport allocations are measured separately, never hidden
  inside the result.
- **Negative control:**
  - **Single fault:** P08: clone the channel string on an allowed request.
  - **At-risk precondition:** The counting allocator surrounds the warmed path.
  - **Expected RED:** The allocator records a heap allocation.

### AC-8: Explain parity

- **Given:** A diagnostic query and a real evaluation select equivalent inputs.
- **When:** Compare decisions and inspect the diagnostic result.
- **Then:**
  MCP (Model Context Protocol) permission explain uses the same pure decision logic and returns the same channel
  decision. Its hypothetical window selection cannot mint a live principal or dispatch permission.
- **Negative control:**
  - **Single fault:** P09: export a usable live permit from the diagnostic path.
  - **At-risk precondition:** The diagnostic query selects an otherwise permitted channel.
  - **Expected RED:** The diagnostic result carries usable admission authority.

### AC-9: One navigation source

- **Given:** A native admitted request has a trusted document stamp.
- **When:** Compare the stamp with the guard principal at admission.
- **Then:**
  One host-owned navigation source supplies both admitted request stamps and `Principal::Webview { id, generation
  }`. Their values must be equal. No renderer identity, pointer conversion, URL, token text, numeric manifest key
  guess, or separate counter supplies authority.
- **Negative control:**
  - **Single fault:** N01: rotate only the stamp.
  - **At-risk precondition:** The admitted request uses the new generation.
  - **Expected RED:** The captured generations differ.

### AC-10: Cross-document revocation

- **Given:** A queued request still holds the retiring document identity.
- **When:** Order revocation before its final handoff.
- **Then:**
  A native cross-document commit invalidates the old nonce and pending identity. A late old request cannot cross
  the final dispatch boundary if revocation won the ordering race. Old replies/events cannot settle or enter the
  new document. Final handoff is bounded and nonblocking; a stalled transport cannot block native revocation on
  the UI thread.
- **Negative control:**
  - **Single fault:** N02: omit the final currentness comparison.
  - **At-risk precondition:** Revocation wins before the queued request reaches Bun.
  - **Expected RED:** The observer records Bun ingress for that request.

### AC-11: Same-document navigation

- **Given:** A real macOS WKWebView has a current stamp and a pending result.
- **When:** Perform same-document navigation.
- **Then:**
  Hash, `pushState`, `replaceState`, and same-document history navigation leave navigation generation and valid
  pending work unchanged, observed on real macOS WKWebView.
- **Negative control:**
  - **Single fault:** N04: rotate on a same-document event.
  - **At-risk precondition:** The navigation preserves the underlying document.
  - **Expected RED:** The observed stamp changes.

### AC-12: Cancelled renderer navigation

- **Given:** A real macOS WKWebView has a valid sending document.
- **When:** Start and cancel a renderer provisional navigation.
- **Then:**
  A cancelled renderer-initiated provisional navigation preserves the old document's nonce, generation, and
  ability to send. A committed replacement, including a reload to the same URL, does not.
- **Negative control:**
  - **Single fault:** N05: rotate at renderer provisional start.
  - **At-risk precondition:** No replacement document commits.
  - **Expected RED:** The original generation changes.

### AC-13: Host-load closure

- **Given:** The host begins a full-document load with a previously bound document.
- **When:** Check admission during the uncommitted interval.
- **Then:**
  A trusted host full-document load/reload invalidates before invoking the engine. An old document cannot rebind
  during the uncommitted interval. A failed/cancelled load does not roll authority back. Native-attempt
  correlation rejects obsolete transitions; a duplicate event is an idempotent no-op. The live running-app
  host-load seam belongs to #459.
- **Negative control:**
  - **Single fault:** N06: remove the AwaitingCommit refusal.
  - **At-risk precondition:** The retired document attempts admission while the host load awaits commit.
  - **Expected RED:** The native observer records admission during AwaitingCommit.

### AC-14: Terminal exhaustion

- **Given:** The checked generation has reached the maximum u32 value.
- **When:** Invalidate the document once more.
- **Then:**
  Navigation arithmetic is checked `u32`. An attempted increment above `u32::MAX` permanently refuses that
  WebView's dispatch/delivery; no wrap, saturation, truncation, reset, or rebind escapes the terminal state.
- **Negative control:**
  - **Single fault:** N07: use wrapping arithmetic.
  - **At-risk precondition:** The next increment exceeds the representable maximum.
  - **Expected RED:** The generation wraps instead of reaching permanent refusal.

### AC-15: WebView destruction

- **Given:** The native subprocess owns a WebView with a live binding.
- **When:** Destroy the WebView and check its retired callbacks.
- **Then:**
  Destroying a WebView revokes queued admission, pending delivery, document-owned renderer subscriptions, and
  callbacks exactly once. Reusing a native address or numeric id cannot resurrect the old lifetime. Global Bun
  `ipcMain` registrations belonging to the still-running application are not document-owned subscriptions.
- **Negative control:**
  - **Single fault:** N08: omit the lifetime comparison.
  - **At-risk precondition:** A callback from the destroyed lifetime is presented to a reused identity.
  - **Expected RED:** A retired callback has authority in the successor lifetime.

### AC-16: Authenticated origin

- **Given:** A native WebView holds a nonempty channel grant list.
- **When:** Check admission for documents inside and outside the approved set.
- **Then:**
  Wrong WebView, wrong native frame/content world, bad nonce, unknown authority header, stale navigation, or
  mismatched app-link identity fails closed before forwarding.
- **Negative control:**
  - **Single fault:** N11: admit a document from its WebView id alone.
  - **At-risk precondition:** A document outside the approved set occupies the same WebView.
  - **Expected RED:** The native binding gate admits that document.

### AC-17: Bun session recovery

- **Given:** Work admitted on the old Bun generation is queued.
- **When:** Recover the link before dequeuing the old work.
- **Then:**
  Bun recovery revokes old app-link correlation/work exactly once, preserves the surviving document identity,
  creates fresh authenticated app-link authority, and never automatically replays an uncertain operation.
- **Negative control:**
  - **Single fault:** N09b: select the current link without comparing the captured generation.
  - **At-risk precondition:** A successor authenticated link is usable at dequeue.
  - **Expected RED:** The successor link receives the old work.

### AC-18: Opaque bounded envelope

- **Given:** The production relay receives application data in a bounded outer envelope.
- **When:** Check routing metadata and the reviewed byte bound.
- **Then:**
  The host treats application values as opaque payload bytes. Payload contents cannot select a guard principal,
  grant, native handler, window, correlation, or codec. The bounded outer envelope supplies the validated channel
  and routing metadata.
- **Negative control:**
  - **Single fault:** W01: remove the request byte bound.
  - **At-risk precondition:** An otherwise admissible envelope exceeds that bound.
  - **Expected RED:** The production relay admits an envelope outside the reviewed bound.

### AC-19: Original reply target

- **Given:** A reply has captured the original sender document.
- **When:** Deliver after its target has acquired another document.
- **Then:**
  `event.reply` retains the original sender document/frame identity. `webContents.send` resolves the current
  target document at its own send admission. Neither targets by numeric window id alone, broadcasts, or falls back
  to a newer document.
- **Negative control:**
  - **Single fault:** N03: remove the document-binding comparison.
  - **At-risk precondition:** A completion addresses the retired document.
  - **Expected RED:** The new document observes the old completion.

### AC-20: Main-side enumeration

- **Given:** Main literal registrations and the required trace are available.
- **When:** Inspect the first-proof migration artifact.
- **Then:**
  Main-side string-literal registrations are the only grant-enumeration source. Dynamic registrations are reported
  unenumerable and wildcard emission is refused. #499's first-proof artifact further intersects those
  registrations with the observed PANEL-P3 trace.
- **Negative control:**
  - **Single fault:** M01: include preload-only names when constructing grants.
  - **At-risk precondition:** The observed PANEL-P3 trace includes the preload-only name and main has no literal registration for it.
  - **Expected RED:** The emitted literals include the preload-only name.

### AC-21: Architecture example

- **Given:** The architecture 03 example enumerates the example grants.
- **When:** Inspect the example alongside the migration grant source.
- **Then:**
  The architecture 03 example contains only enumerated literal channel grants. The capability-set-subset rule is
  explicitly about schema-declared channels, not opaque Electron payloads.
- **Negative control:**
  - **Single fault:** Restore a wildcard in the architecture 03 channel list.
  - **At-risk precondition:** The example contains an incoming channel list.
  - **Expected RED:** The architecture example contains a grant other than an enumerated literal.

### AC-22: Conformance provenance

- **Given:** The Electron navigation rows lack the required recorded oracle.
- **When:** Validate the corpus classification.
- **Then:**
  Corresponding navigation conformance rows 10–12 remain `UNKNOWN` for Electron behavior until #551's recorded
  oracle comparison; a passing Keld-side test cannot promote them.
- **Negative control:**
  - **Single fault:** C01: promote a row without the oracle artifact.
  - **At-risk precondition:** Only a Keld-side result is available.
  - **Expected RED:** Corpus validation fails and refuses the unsupported Electron PASS.
