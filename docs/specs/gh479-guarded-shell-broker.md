# Spec: Guarded native shell dispatch

- Status: draft
- Tracker: GH-479
- Owner: 0monish (tracker)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Enable the main-role shell facade on qualified macOS targets. A trusted Host dispatches only a permitted web URL or file operation. The returned value follows the specific Electron method. A denied operation has zero native side effects. Windows, Linux portals, shortcut editing and installer execution remain outside initial qualification.

## 2. Spec refs

- FACT: [Literal-origin decision](https://github.com/gyldlab/keld/issues/416#issuecomment-6022116349) selects exact origins and rejects scheme-wide or confirmation-based authority. [Owner brief](https://github.com/gyldlab/keld/issues/479#issuecomment-6026173215) assigns refusal and per-method behavior.
- FACT: [Security mapping](https://github.com/gyldlab/keld/issues/505) assigns openPath/reveal to fs.read and trashItem to fs.write. [Canonical resource finding](https://github.com/gyldlab/keld/issues/479#issuecomment-6280826585) separates origin formation from exact matching.
- FACT: `crates/keld-guard/src/lib.rs:784` owns authorization. The generic scope matcher at `crates/keld-guard/src/lib.rs:1276` also supports a terminal subtree syntax. A shell-origin grant needs an exact-origin resource policy.
- FACT: `crates/keld-ipc/src/guard_dispatch.rs:48` authorizes before lending a selected scope to a handler. `crates/keld-native/src/fs.rs:363` owns prepared filesystem anchors. Its byte-read/write methods do not establish safe native shell pathname handoff.
- FACT: [Electron v44.4.5 shell API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/shell.md#L24-L95), retrieved 2026-11-06, says openPath resolves with an error string on failure, “otherwise \"\".” openExternal and trashItem return promises without a success value.
- FACT: [Electron method bindings](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/common/api/electron_api_shell.cc#L49-L109) implement different success and failure channels.
- FACT: [macOS dispatch](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/common/platform_util_mac.mm#L90-L194) uses the configured handler and a completion callback for openExternal.

## 3. Acceptance criteria

### AC-1: Authorize the canonical origin and deliver the intended full URL

- **Given:** Grant only https://example.test. Register an isolated native handler for the test lane and restore previous associations through its operator.
- **When:** Call openExternal with HTTPS://EXAMPLE.TEST:443/a?q=one#part and activate=false. Repeat with the default activation setting.
- **Then:** The Guard receives exactly https://example.test. The native boundary receives the intended path, query and fragment, once. Map activate=false and default true. Fulfill with undefined only after native completion.
- **Negative control:**
  - **Single fault:** Compare the raw input directly to the origin literal.
  - **At-risk precondition:** The approved origin differs from the input in case, port and path. Verify a lowercase control independently.
  - **Expected RED:** The allowed control passes but the equivalent uppercase URL never reaches the handler.

### AC-2: Deny ambiguous or unauthorized URLs before any OS effect

- **Given:** Provide valid, malformed and over-broad manifest fixtures. Capture resolver, handler and launch calls separately.
- **When:** Try a sibling host, nondefault port, userinfo, opaque origin, file:, javascript:, scheme-only, literal wildcard and executable-launching targets.
- **Then:** A literal-origin miss raises the typed KELD-GUARD002 rejection with safe remediation. Invalid and forbidden schemes never reach native resolution or dispatch. A malformed grant cannot enable a request.
- **Negative control:**
  - **Single fault:** Treat a bare scheme as an origin wildcard.
  - **At-risk precondition:** Use a parseable manifest containing a bare scheme and an independently observed foreign URL. If manifests reject that input, challenge the literal matcher separately.
  - **Expected RED:** The authorization oracle admits an unlisted origin or the foreign handler sees a request.

### AC-3: Keep filesystem effects bound to the authorized object

- **Given:** Prepare separate read-only, write-only and ungranted test roots under independent principals. Include allowed inert files, executable or app targets and a symlink to an outside sentinel.
- **When:** Run openPath and reveal with read authority. Run trash with write authority. Swap a target or ancestor between validation and the native consumer.
- **Then:** Only the approved object can be opened, revealed or trashed. Wrong operations or external replacements never reach the effect. Refuse unsafe executable dispatch. A failed trash must leave the file intact.
- **Negative control:**
  - **Single fault:** Pass the original unchecked pathname to the native operation after the Guard selected a scope.
  - **At-risk precondition:** Install a deterministic swap after scope selection. Ensure the baseline qualified backend reaches its effect for an unchanged allowed object.
  - **Expected RED:** The external sentinel gets opened, revealed or trashed. An early blanket denial cannot satisfy the positive.

### AC-4: Preserve completion and method-specific return values

- **Given:** Use a real macOS success fixture plus controlled failing and delayed completion adapters. Keep pending requests distinguished by role generation and correlation.
- **When:** Run paired concurrent calls, reverse completions, inject an error, then retry on a successor generation. Also invoke reveal and beep.
- **Then:** Successful openExternal/trash fulfill undefined and reject on native failure. openPath resolves empty on success or a nonempty diagnostic on failure. showItemInFolder/beep return undefined. Duplicate or late callbacks cannot settle the successor.
- **Negative control:**
  - **Single fault:** Resolve the openExternal promise when a request enters the queue.
  - **At-risk precondition:** Hold a native failure completion until the pending state has been observed.
  - **Expected RED:** The caller fulfills before the error or settles twice; the independent handler receipt remains separate.

### AC-5: Route from the authenticated principal without minting grants

- **Given:** Give unrelated renderers and role generations the same URL string. One authorized main-role call is the control.
- **When:** Forge a principal or invoke after revocation. For beep send extra path or capability fields. Try a consent prompt as an authority substitute.
- **Then:** Guard decisions use the Host-minted caller and current policy. Reject unauthorized effects and malformed requests. Beep carries no resource or authority. No fallback invokes a shell, confirmation or generic command interpreter.
- **Negative control:**
  - **Single fault:** Trust a caller-provided privilege identity.
  - **At-risk precondition:** Choose a caller outside the real role's grant and a forged identity that would otherwise match.
  - **Expected RED:** The denial-before-effect marker changes or a native call appears.

## 4. Design

### D-1: Form the resource once and keep the matcher literal

The Host admits a bounded UTF-8 request from the authenticated role. Proposed caps: 8 KiB per web URL and 4 KiB per path, subject to stricter platform limits. Reject invalid input and checked-length overflow before queueing. No filesystem, DNS, login prompt or handler lookup may precede authorization.

Use one approved parser in the Guard-side resource-forming boundary. A parsed target supplies both the canonical origin for authorization and the full web URL for dispatch. Never reconstruct a URL from unvalidated path/query text or pass raw input to a second native parser with different authority semantics.

For the first slice accept hierarchical http or https only. Lowercase scheme and ASCII host and remove the default port. Preserve explicit nondefault ports as distinct origins. Reject credentials, ambiguous authority, controls, backslashes and unqualified host spellings.

Test IPv4, IPv6, encoded delimiters, IDNA and trailing dots against a shared pinned parser before admitting their exact supported forms.

Match only the canonical scheme, host and effective port against a declared literal origin. A path, wildcard, scheme name, origin prefix, wildcard suffix or user-entered confirmation cannot authorize. A path/query/fragment is part of the delivered target, not the resource key. Never authorize a different scheme based on text inside an allowed URL.

The mechanism authorizes one OS dispatch. External navigation after the browser accepts the URL belongs to that browser. Do not claim the broker constrains redirects, tests server reachability or proves a user viewed content. Custom protocols and file or script URLs remain denied even in a permissively spelled manifest.

### D-2: Retain a file target through the physical effect

Reuse final Guard dispatch, bound principal and selected read/write scope. The native adapter must exclude symlink/reparse escapes and rebind races until the OS consumes the target. A retained descriptor followed by an unchecked pathname reopen loses this guarantee. Do not widen the existing byte broker or invent a shell-private path matcher.

A safe openPath/reveal/trash backend may require a separate object-specific OS handoff. Without one, keep that operation unsupported. Neither fs.read nor a picker grants execution. Exclude an executable or application package from openPath, including aliases or alternate entry types. Reveal shows an item without launching it. Never implement trash using permanent unlink/rmtree.

### D-3: Dispatch according to the actual method

| Method | Native endpoint and public completion |
| --- | --- |
| openExternal | On macOS use the default URL handler and the validated activate option. A missing handler rejects. Success means the native API accepted the open, not that loading finished. |
| openPath | Default document handler after the path operation qualifies. Resolve an empty string or a redacted actionable error string. Keep policy failures distinguishable from native ones. |
| showItemInFolder | Reveal the eligible item. Preserve the void facade return. Record a bounded diagnostic for a denied or failed dispatch. No path expansion or fallback open. |
| trashItem | Request OS trash on the authorized item. Await completion. Reject if unavailable. Never substitute permanent deletion even when the volume lacks trash. |
| beep | Emit one platform beep from the authenticated facade, no path or permission grant. Return void. Test the adapter invocation separately from subjective audio volume. |

Bind pending calls to the active role generation and correlation. Enforce the existing approved outstanding-call budget and typed exhaustion. Count one terminal settlement per call. Reject a canceled request before native dispatch where possible. After dispatch, revocation cannot undo an external OS effect. Suppress stale return delivery and never retry a maybe-dispatched operation automatically.

Keep command execution, arbitrary working directories and platform-specific options out of the first slice. Validate option types and document unsupported options. Eligibility and OS calls remain in the Host/native adapter. The facade cannot self-authorize or resolve a promise from a transport enqueue alone.

## 5. Boundaries

Existing Guard, authenticated IPC and native broker owners retain their responsibilities. The Electron package owns external value/error mapping. #505 owns the policy matrix and the scoped wire owner must approve any new operation. No new public package, consent capability, shell parser or wire opcode is established by this draft.

## 6. Tasks

### T-1: Land conformance and fair refusal fixtures

- **Blocked by:** none
- Record pinned Electron argument, return and error behavior. Build independent origin, path, handler and completion oracles. Mark unsupported cells red.

### T-2: Implement the guarded URL vertical slice

- **Blocked by:** T-1, B-1
- Qualify one canonicalizer and exact grant owner. Enable macOS openExternal only after AC-1, AC-2, AC-4 and AC-5 pass through the real authenticated route.

### T-3: Enable separately qualified path and beep methods

- **Blocked by:** T-1, B-2
- Run the scoped physical-effect race and method return cells for each path operation. Qualify beep independently. Keep blocked methods explicitly unsupported.

## 7. Test plan & evidence

Use controlled native handlers with operator-owned isolated associations, not the user's ordinary browser. Capture the full delivered URL only in a private synthetic fixture. Record owner decisions and safe redacted errors independently.

For AC-1/2, exercise authority case, explicit port, path and query, deceptive userinfo, unicode/punycode, an embedded scheme, empty hosts, encoded separators and noncanonical grant syntax. Include a valid positive after every denial family.

For AC-3/4 use independent filesystem identity, app-package and symlink controls. Inject cancellation before dispatch, after native receipt and after success. Reject duplicate callbacks. Test simultaneous calls and successor generation without recycled-promise delivery. Real app/OS acceptance remains unrun.

### E-1: Pinned source audit

- **Run:** Read #416's decision, #479's latest brief and canonicalization finding, #505's operation mapping, pinned Guard/broker sources and Electron 44.4.5 docs and native macOS implementation.
- **n:** One KELD main revision, one Electron release and three repository issue owners.
- **Environment:** GitHub connector, 2026-11-06. Source inspection only.
- **Limits:** No installed GUI handlers, native shell calls, race fixtures or platform acceptance ran. Windows/Linux behavior needs its own approved and tested cells.

## 8. Review gates triggered

Permission model, wire protocol and public API. Native unsafe operations and a new parser dependency require separate owner review if selected. This proposal changes no code, wire value or manifest.

## 9. Perf impact

No measured change. Bound request sizes and pending completions. Count native dispatches per accepted call. Never replicate a native launch to mask a timeout.

## 10. Open questions, blockers & unknowns

### B-1: Approve exact origin and wire owners

- **Status:** open
- **Owner:** The #505/Guard and authenticated operation owners.
- **Impact:** An ordinary subtree matcher and a string-returning facade cannot enforce browser-origin semantics or authenticate a native dispatch.
- **Check:** Prove one parser forms the origin and native target; exact-only grants and forbidden schemes fail before any OS call.

### B-2: Qualify the target through native consumption

- **Status:** open
- **Owner:** The native filesystem and shell owners on each platform.
- **Impact:** A checked string or retained handle alone cannot prevent the native URL/path consumer resolving a replacement. Keep unqualified operations unsupported.
- **Check:** On the real backend race every file operation after validation and verify the exact outside sentinel remains untouched while the valid case succeeds.
