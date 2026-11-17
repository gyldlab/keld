# Acceptance contract

Part of: [index](index.md)
Owns: section 3

## 3. Acceptance criteria

### AC-1 ? Realms

- **Given:** A supported macOS WKWebView, the pinned unmodified draw.io preload, and the actual first inline page script are in scope.

- **When:** The Host constructs the view and installs the app/trusted worlds and scripts before navigation.

- **Trigger:** Construct one eligible macOS 11+ WKWebView before navigation

- **Then:** Three isolated worlds (page, app, trusted) and **two distinct world-scoped handlers** for app and trusted roles on **one per-view controller**. Main-frame only; below macOS 11 fails closed, never falls back to page world.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.first_inline_complete and f07.world_and_k142_isolation before its restored positive control passes.

- **Single fault:** Move the fixed wrapper to a microtask or AtDocumentEnd, or install the app script in the trusted world.

- **At-risk precondition:** The first inline page script runs before the app-world preload has registered the actual methods and required seed.

- **Expected RED:** The independent first-inline oracle must miss the required callable methods/seed, or the wrong-world observer must detect trusted authority leakage.

### AC-2 ? First-inline blocker

- **Given:** The unmodified pinned draw.io preload and adaptive-colors valid, invalid, nullish and explicit-value fixtures are present.

- **When:** The preload executes before the actual first inline page script and applies the conditional .configuration.defaultAdaptiveColors seed.

- **Trigger:** Execute **unmodified** draw.io preload and the actual first inline page script across the adaptive-colors fixtures in §4.2

- **Then:** Exactly four **predeclared** `window.electron` methods (`request`, `registerMsgListener`, `sendMessage`, `listenOnce`) are callable before first inline; page `window.process.type` and `.versions` are copied/frozen facts **from the actual preload/session**. Independently prove the original unmodified **app-world preload script executed, registered
  the four actual functions, and completed any applicable `.configuration` startup seed before first inline**; a visible static wrapper alone is insufficient. Subsequent effect-bearing calls reach only matching **current original app-world preload registrations** after native bind and Host grant; missing/mismatched registrations are refused, not replaced by
  canned page stubs. With valid `--initial-adaptive-colors=` mode (`auto`, `simple`, `none`) and absent/nullish existing field, first-inline `.configuration.defaultAdaptiveColors` equals that mode. Missing/invalid flag and explicit existing value leave storage unchanged; malformed JSON is caught and does not abort page loading. Observations are synchronous
  **before** first inline; delayed Promise, late-filled placeholder, forged DOM method identity or generic dynamic-contextBridge parity claim fails.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.adaptive_seed_valid_nullish, f07.adaptive_seed_no_invalid_flag, f07.adaptive_seed_explicit and f07.adaptive_seed_malformed before its restored positive control passes.

- **Single fault:** Skip the nullish seed, overwrite an explicit value, accept an invalid flag, or remove the malformed-storage catch.

- **At-risk precondition:** A valid flag may meet a missing/nullish .configuration.defaultAdaptiveColors setting, but an explicit non-nullish preference or malformed configuration must not be overwritten or abort navigation.

- **Expected RED:** The first-inline .configuration.defaultAdaptiveColors value must be absent/wrong, explicit stored bytes must change, or malformed storage must prevent page loading.

### AC-3 ? Exact `require`

- **Given:** The source image has been admitted and the preload uses the bounded resolver.

- **When:** The preload requests each of the nine allowlisted module specifiers and an unapproved specifier such as fs.

- **Trigger:** Preload calls nine admitted specifiers, then `require('fs')`

- **Then:** Exactly §4.2 alias/module identities resolve; `require('fs')` synchronously throws `Error` with exact message `module not found: fs` and has no Node filesystem effect.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.require_nine before its restored positive control passes.

- **Single fault:** Add fs or a relative/Node fallback to the resolver.

- **At-risk precondition:** A preload may resolve only the nine exact aliases; unsupported module names must fail synchronously.

- **Expected RED:** The independent resolver test must observe fs or a relative/Node module resolving instead of the exact module-not-found error.

### AC-4 ? Electron module

- **Given:** The admitted preload receives the restricted Electron object in the app world.

- **When:** The preload enumerates the object and calls members outside the approved seven-name surface.

- **Trigger:** Preload inspects `require('electron')`

- **Then:** Seven named renderer members: `contextBridge`, `crashReporter`, `ipcRenderer`, `nativeImage`, `sharedTexture`, `webFrame`, `webUtils`. Unimplemented operations throw registry-approved typed `KELD-COMPAT` unsupported errors, never silent no-ops. #467 owns functioning contextBridge and ipcRenderer APIs.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.require_nine before its restored positive control passes.

- **Single fault:** Add an unsupported Electron member or silently no-op an unsupported call.

- **At-risk precondition:** Only the seven named Electron members are exposed; unsupported calls produce the registered typed compatibility error.

- **Expected RED:** The independent object-shape or error oracle must detect an extra member or a missing typed refusal.

### AC-5 ? Error

- **Given:** The preload source is admitted, and a separate fixture can throw during top-level JavaScript execution or fail source admission.

- **When:** The wrapper executes the preload and observes the thrown JavaScript error; source-admission failure is exercised before execution.

- **Trigger:** Top-level admitted JS preload throws

- **Then:** `Unable to load preload script: <path>` plus error logged, and page finishes loading; effects before throw persist. A **file admission failure** rejects window creation before any preload executes.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.js_exception before its restored positive control passes.

- **Single fault:** Propagate the preload exception as a navigation failure or suppress the required console error.

- **At-risk precondition:** A JavaScript exception logs the exact compatibility prefix/error and page navigation completes; source admission failure refuses construction before JS executes.

- **Expected RED:** The page-load observer must fail to complete or the exact console marker must be absent; a source failure must never execute a sentinel.

### AC-6 ? Process

- **Given:** Canonical Host session facts and a fixed additionalArguments vector are available to the admitted preload.

- **When:** The preload reads process.type, sandbox flags, versions, argv and env.

- **Trigger:** Preload reads `process`

- **Then:** `type='renderer'`, `sandboxed=true`, `contextIsolated=true`, own-property-free `env={}`; no inherited `KELD_APP_LINK`. `argv` includes exact `additionalArguments` in order; `versions` deep-equals canonical #452/#570 session facts (no locally invented fields).

- **Negative control:** The single-fault mutation below must fail the independent test row f07.process_env_argv_versions before its restored positive control passes.

- **Single fault:** Copy the Bun role environment into process.env or hardcode versions instead of using canonical facts.

- **At-risk precondition:** The process subset is copied/frozen from canonical facts, argv is fixed, env is empty and KELD_APP_LINK is absent.

- **Expected RED:** The independent environment observer must see a leaked role variable or a value that does not match canonical session facts.

### AC-7 ? Root mapping

- **Given:** The app supplies the computed absolute preload path and the Host retains an admitted stage-root identity plus trusted package manifest.

- **When:** The Host maps the supplied absolute path to exactly one trusted relative manifest entry.

- **Trigger:** App supplies computed absolute preload path

- **Then:** Host matches exactly **one trusted package-manifest relative entry** under its retained stage-root identity, distinct from page `file://` and CWD. Component-boundary and exact-entry checks reject unmapped/outside/traversal/root-swap paths before an attacker-selected file read. Unmodified draw.io absolute path must map without a source rewrite.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.root_path_and_symlinks before its restored positive control passes.

- **Single fault:** Reopen by display path, allow an in-root symlink, or use lexical-prefix-only path matching.

- **At-risk precondition:** Only one exact component-boundary manifest entry is selected; traversal, aliases, symlinks and out-of-root paths are refused before source read.

- **Expected RED:** The filesystem oracle must observe an out-of-root read, an admitted symlink or more than one matching manifest entry.

### AC-8 ? Source image

- **Given:** A single manifest entry is matched and its expected digest is authenticated outside the same-UID-writable package tree.

- **When:** The Host opens one descriptor, checks file kind and caps, reads the complete bounded image and verifies the trusted digest before JS.

- **Trigger:** Matched manifest entry is admitted

- **Then:** Authenticate its expected SHA-256 **outside the same-UID-writable package tree**; bounded single-descriptor read (512 KiB/window, 8 MiB aggregate), verify every owned byte against that digest **before executing any preload**, then retain immutable Host-owned bytes. An absent independent trust anchor, changed/in-place-raced bytes, any symlink
  component, special file, +1-size or +1-capacity refuses. No self-derived digest or reopen-by-display-path.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.manifest_external_trust, f07.source_kind_caps and f07.inplace_concurrent_write before its restored positive control passes.

- **Single fault:** Trust an adjacent editable manifest or omit the complete-byte digest check during an in-place concurrent write.

- **At-risk precondition:** Only the complete immutable byte image matching the independent trust anchor may execute; file-kind, per-window and aggregate caps are enforced.

- **Expected RED:** The hostile writer or manifest mutation must not cause attacker-controlled/mixed bytes to execute; the independent digest oracle must refuse before JS side effects.

### AC-9 ? Window/document lifecycle

- **Given:** A window has one admitted immutable preload snapshot and can navigate, experience Bun recovery, or be destroyed.

- **When:** The Host reuses the snapshot for navigation while creating fresh JS state and releases retained bytes exactly once on view destruction.

- **Trigger:** Same window navigates; Bun crashes/restarts; window retires

- **Then:** Reuse same window's source snapshot but **rerun in new document with fresh JS callback state**. No forced reload/reset of surviving page on Bun crash, no source re-open on navigation; destroy releases handles/bytes/handler once.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.snapshot_lifecycle before its restored positive control passes.

- **Single fault:** Reopen source on navigation or reload the WebView after a Bun restart.

- **At-risk precondition:** The same snapshot hash remains stable across navigation, JS state resets, unsent page text remains intact after Bun recovery, and destruction releases capacity once.

- **Expected RED:** The snapshot hash must change, surviving page state must be lost, or accounting must release twice/never release.

### AC-10 ? Trusted isolation

- **Given:** The app world and KEL-142 trusted renderer world are separate on the same per-view controller, and a hostile app preload is present.

- **When:** The hostile preload attempts to read trusted credentials, handler identity or trusted-world state.

- **Trigger:** Real draw.io preload + hostile app-world script

- **Then:** Original KEL-142 native AC-2 passes unchanged. Hostile preload cannot read/wrap trusted handler/bind hook, capture nonce, endpoint or grants, re-export them or get forged native/Bun effect.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.world_and_k142_isolation before its restored positive control passes.

- **Single fault:** Install the app script in the trusted world or remove the native world check.

- **At-risk precondition:** The app world cannot observe trusted handler/nonce state and forged app-world messages have no native effect.

- **Expected RED:** The independent hostile-world observer must detect trusted data exposure or a forged native effect.

### AC-11 ? Identity

- **Given:** A current qualified document has one checked generation and distinct stored App/Trusted world credentials.

- **When:** A message arrives from a wrong world, controller, view, frame, credential or stale/destroyed document.

- **Trigger:** Wrong world, controller, view, subframe, app/trusted credential or old/destroyed document sends app message

- **Then:** Host checks native per-view identity and **one implemented #542 generation/eligibility owner** before dispatch. #540/#542 must approve **two distinct native-owned world credentials**; an initial world bind is authenticated by native controller/world/frame/document facts and mints one new credential, while a later effect-bearing send requires an
  existing stored binding **and** supplied matching credential (missing/missing denies). Never expose/reuse KEL-142's trusted nonce in app/page or mint a second generation. Revoke both at cross-document commit or earlier host load; preserve at same-document/cancelled renderer provisional navigation. Forged/duplicate/stale bindings have zero effects and
  bounded settlement.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.frame_document_two_window and f07.world_scoped_bind before its restored positive control passes.

- **Single fault:** Remove the generation/stored-binding check or accept a supplied token without matching the stored token.

- **At-risk precondition:** Only the current native view/frame/document and matching stored-plus-supplied world credential can reach Host dispatch.

- **Expected RED:** A stale, swapped, missing or wrong-view credential must produce a Host/Bun effect for the negative control to go RED; the correct implementation must produce zero effects.

### AC-12 ? Permission & parser

- **Given:** A canonical v2 named-send envelope reaches the single approved decoder and an exact per-window el: grant may or may not exist.

- **When:** The decoder and Host Guard evaluate the named channel before any app-link write.

- **Trigger:** App-world v2 message names an `el:<topic>` with no or wrong exact-literal grant

- **Then:** **One canonical decoder**, extended by #465 after #540's approved v2 contract, handles named one-way send with bounded credit; #543 owns host-to-renderer event delivery and #464 its IPC conformance cases. #484's app handler consumes the approved decoder result through the existing app link/guard. Current v1 **cannot** accept a named channel.
  Default deny before effect, with no wildcard/prefix match; a correct exact grant permits only bounded, credited effects. No page-supplied principal, permission or KIPC endpoint.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.grants_decoder before its restored positive control passes.

- **Single fault:** Treat a missing exact grant as Allow or reinterpret numeric v1 invoke as a v2 named send.

- **At-risk precondition:** Only the canonical v2 shape and exact literal grant can admit a bounded credited effect; missing/wrong grants deny before Bun.

- **Expected RED:** The independent Bun effect counter must remain zero for missing/wrong grants and numeric-v1-as-named-send mutations.

### AC-13 ? No DOM authority

- **Given:** The four predeclared page methods are bound to actual private app-preload registrations, with the Q-1 A1/A2 owner decision explicit.

- **When:** A page call enters the selected page-to-app route and reaches the same current-document, private-method and Host grant checks.

- **Trigger:** GH-467's four predeclared methods route to actual app-private registrations and the app-native handler transmits admitted requests/results

- **Then:** **Current AC-17 forbids any page-world native endpoint**; the test-only static F0 raw page handler is not an authorized transport. GH484/KEL-142 must approve either [Q-1 A1 or A2](https://github.com/gyldlab/keld/issues/484#issuecomment-6390973710) before effect-bearing production routing. Under a future expressly approved A2 untrusted page-data
  carrier, even a direct call naming a permitted method is **not** a registry, credential, principal, role or grant; all permitted calls get identical private-method/current-document/final Host checks whether or not they used the visible wrapper. Unexposed/unguarded requests have zero forbidden effects. Neither `window.postMessage` nor page-forgeable DOM
  events/descriptors can authenticate a method; the original F0 DOM handshake remains forbidden in production.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.native_dom_silence before its restored positive control passes.

- **Single fault:** Treat page-visible DOM metadata as app identity, or expose a page-native handler under A1 without the approved A2 amendment.

- **At-risk precondition:** No page data is privileged; under A1 no page-native endpoint exists, and under approved A2 the page carrier remains untrusted data only.

- **Expected RED:** The independent native/Bun observer must detect a forbidden effect from an unexposed, stale, wrong-world or ungranted page request.

### AC-14 ? Diagnostic

- **Given:** The KEL-90-owned handler-cost metric registry has a reviewed diagnostic-only entry and no performance threshold.

- **When:** The handler callback path records diagnostic samples with the declared monotonic clock and census.

- **Trigger:** Measure app-native handler cost

- **Then:** One metric ID in existing `gyldlab/keld-benches` registry, required oracle/census/clock/statistic, class `diagnostic`, zero threshold and publication-ineligible.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.metric_registry before its restored positive control passes.

- **Single fault:** Mark the metric publishable or add a performance threshold.

- **At-risk precondition:** The metric remains diagnostic-only and has no pass/fail threshold or public comparison.

- **Expected RED:** The registry review must reject a publishable class or threshold.

### AC-15 ? Conformance first

- **Given:** The GH567 expected-fail baseline and one-fault runner assertions exist before any implementation change.

- **When:** A subsequent approved spec and implementation PR attempt to flip only owned F07 cells after native evidence.

- **Trigger:** Implementation is authorized by a subsequent approved spec

- **Then:** #567 F07-T4 expected-fail cases with runner assertions exist before change; flip **only** #484-owned world/preload/require/process entries after independent native proof. #467 owns the bounded app-specific facade/IPC/round-trip cells; all unimplemented **doc-cited** general dynamic `contextBridge` rows remain expected-fail or an independently
  approved recorded divergence, never UNKNOWN by default or PASS from the static wrapper. UNKNOWN applies only to genuinely unqualified engine-specific observations.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.expected_fail_flip before its restored positive control passes.

- **Single fault:** Pass a conformance cell based only on source presence or a sentinel.

- **At-risk precondition:** Only cells with exact owned native proof and independent RED/GREEN evidence may change from expected-fail.

- **Expected RED:** The runner must reject any cell flipped without its required native receipt and one-fault mutation.

### AC-16 ? Native receipt

- **Given:** A real physical macOS WKWebView/Host/Bun acceptance run has completed on the exact source and pinned draw.io/Electron inputs.

- **When:** The owner records the native receipt with platform, source hashes, identity, event order, guard/admission counts and mutation evidence.

- **Trigger:** Claim Mac acceptance

- **Then:** Exact host/Electron/draw.io hashes, macOS/WebKit/Bun, view/frame redacted document identity, first-inline synchronous observation, native effect census, separately run single-fault RED controls, then GREEN on restored tree.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.expected_fail_flip plus the exact native receipt required by AC-16 before its restored positive control passes.

- **Single fault:** Replace the real receipt with a synthetic fixture or omit an exact source/platform/observer field.

- **At-risk precondition:** The receipt binds exact KELD/draw.io/Electron hashes, macOS/WebKit and Bun versions, redacted native identity, event ordering and independent RED/GREEN evidence.

- **Expected RED:** The evidence review must reject a missing hash/platform/observer or any receipt based only on a mock/test fixture.

### AC-17 ? No accidental capability

- **Given:** No app preload/grant exists or the engine/runtime is outside the explicitly supported first-proof profile.

- **When:** A page or app attempts to reach a native capability through ambient Node, a secondary wire channel or an unapproved page-world endpoint.

- **Trigger:** No preload/grant or unsupported engine

- **Then:** KEL-142 existing `window.keld` behavior unchanged; no ambient Node resolver, secondary wire channel, **page-world native endpoint under the current unamended contract**, or Windows/Linux acceptance. Q-1 A2's proposed data-only ingress is **not admitted by this draft** and requires an explicit independently reviewed native/KEL-142/architecture-05
  security amendment; never silently weaken this prohibition based on the static F0 probe.

- **Negative control:** The single-fault mutation below must fail the independent test row f07.native_dom_silence and f07.world_and_k142_isolation before its restored positive control passes.

- **Single fault:** Add an ambient Node resolver, secondary wire channel or page-world native endpoint without the approved contract amendment.

- **At-risk precondition:** KEL-142 existing window.keld behavior remains unchanged and no accidental native, Node or secondary transport capability is exposed.

- **Expected RED:** The independent capability oracle must show that the forbidden resolver/channel/endpoint cannot be reached and no native effect occurs.
