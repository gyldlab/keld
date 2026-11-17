# Delivery and evidence

Part of: [index](index.md)
Owns: sections 5–7, 9 and 10

## 5. Boundaries

- **This spec PR changes only**
  `docs/specs/gh467-contextbridge-ipcrenderer/` and generated
  documentation *if* the generator already includes this source.
- Future GH-467 implementation may add scoped renderer runtime files
  under `packages/@keld/electron/src/renderer/` and focused
  `packages/@keld/electron` integration tests, after the approved
  spec and one producer/consuming interface review.
- GH-484 alone owns `crates/keld-wv/src/wkwebview/macos_bridge.rs`,
  native injection, content-world naming, sandbox require map,
  package-root preload bytes and KEL-142 isolation re-run.
- GH-540/465/543/544/466 own their protocol/codec/main-listener
  changes; GH-542/552 native navigation/guard. Do not write
  `crates/keld-ipc`, `keld-guard`, shared schema, KEL-142,
  architecture 02/03, current window registry, or Electron upstream
  files from this task.
- If matching C01/C13 semantics require architecture 05/KEL-142
  amendment, GH-484 owns it; GH-467 provides a consuming
  acceptance assertion, not a second amendment.

## 6. Tasks

### T-1: Current draft and independent owner approval

- **Blocked by:** none
- Publish this GH-467 specification with existing contracts and exact approval questions.
- Obtain independent security, permission and public-API review.
- Stop until the exact reviewed specification is approved. Do not change product source.

### T-2: Pinned compatibility corpus prerequisite

- **Blocked by:** T-1, B-5
- GH-464/GH-567 land the pinned, mapped expected-fail contextBridge, Event, API and preload rows.
- Include a baseline test that passes by observing missing APIs.
- Add one fault mutation per decision atom. Do not create a fake missing-API PASS.

### T-3: Local facade behavior

- **Blocked by:** T-1, T-2, B-1, B-2
- Implement the approved predeclared four-slot page facade.
- Use finite copied argument and proxy domains plus read-only process facts.
- Keep dynamic graph acceptance deferred.
- Test actual app-world method registration, argument mutability and callback registries.
- Fail tests on broken freeze, nested callbacks, missing bindings or premature callback retirement.
- Depend on GH-484's accepted document-start publication seam.

### T-4: Listener and transport composition

- **Blocked by:** T-3, B-2, B-3, B-6
- Implement the approved renderer facade against the sole GH-540 transport.
- Consume the accepted GH-544 codec and GH-465/GH-543/GH-466 events.
- Include exact inbound-channel and renderer-action allowlists.
- Test once/remove, pre-bind local registration, terminal denial and callback cleanup.
- Do not change channel grants or add another transport.

### T-5: Real macOS application proof

- **Blocked by:** T-4, B-1, B-2, B-3, B-4
- Wait for accepted GH-484, GH-473, GH-542/GH-552 and owner codec/transport artifacts.
- Run the genuine pinned-page `EditorUi.init` `isFullscreen` request.
- Run a separately labelled test-controlled second request and watchFile fixtures on WKWebView.
- Run anti-stub and KEL-142 hostile-world controls.
- Do not count a controlled request as a second application-origin call.
- Do not mark PASS without the actual original-page callback and physical release.

### T-6: Bounded compatibility handoff

- **Blocked by:** T-5, B-5, B-6
- Keep behavior outside first-proof UNKNOWN or an explicit approved divergence.
- Assign later behavior to its existing owner. Do not add a speculative shim.

No GH-484 ↔ GH-467 cycle: GH-484 may deliver its world/loader
substrate with its own first-inline native fixture **without requiring
the final draw.io round-trip**; GH-467 consumes it and owns the final
actual application witness. Do not close GH-484 by falsely claiming
GH-467's post-integration criterion.

## 7. Test plan & evidence

All tests are mapped to named ACs. A control qualifies only when
changing **one named faulty behavior** makes the test fail on the
mutant, and a clean implementation then passes. No sleep-based
readiness, unchecked green CI or mock Host counts as native proof.

#### BRG001_first_inline

**ACs:** C01, C13.

**Fault and expected failure:** Delay the predeclared four-method page wrapper until after the first page inline script. Removing
only the wrapper leaves all four methods absent. The unmodified preload's conditional startup seed must also be visible before
inline. The early API must have zero pre-bind Bun effects. The F0 raw page handler is not an approved route. GH-484 must first
approve Q1 A1 or A2. Under A2, page-world calls remain untrusted and use the same private registry and final Host grants as
ordinary wrappers.

#### BRG001b_real_registration

**ACs:** C01, C05, C21.

**Fault and expected failure:** Keep the static page wrapper intact but suppress actual app-world `exposeInMainWorld`
registration, or bind one name to a different app function. A visible wrapper alone cannot pass C01/C05. A missing or mismatched
binding must produce zero wrong-function, Host or Bun effects. Restore correct registration and one granted call succeeds. A
canned `mainResp` does not qualify. GH-484 must also delay the actual preload's adaptive-colors side effect while keeping the
wrapper visible; the first-inline seed must go RED.
#### BRG002_exposed_freeze

**ACs:** C02

**Fault and expected failure:**
Remove recursive freeze / top-level read-only property → page mutation observable.

#### BRG003_mutable_arg

**ACs:** C03

**Fault and expected failure:**
Freeze receiving argument object → draw.io `reqId` assignment / listener deletion fails.

#### BRG004_nested_watch

**ACs:** C04

**Fault and expected failure:**
Proxy only top-level functions in the Option A argument path → nested `watchFile` callback absent; sequence test RED at second
differing payload. **Does not qualify deferred generic C06.**

#### BRG004b_graph_capacity

**ACs:** C02–C05,C07,C19 (not deferred C06)

**Fault and expected failure:**
For each 16-depth/256-entry/65,536-byte/64-proxy/64-callback/524,288-retained ceiling actually exercised by Option A
data/argument/callback crossings, prove 0/max pass and max+1 typed refusal before another allocation; bypass any one reservation →
bounded-count observer RED. This does not qualify arbitrary general dynamic graph capacity.

#### BRG004c_multifire_lifetime

**ACs:** C04,C12,C17

**Fault and expected failure:**
Two distinct `watchFile` deliveries reuse the same live slot, with per-event native/ring release; unlink or retire document then
replay stale callback → zero delivery. Free slot after first event or keep it after retirement → RED.

#### BRG005_void_return

**ACs:** C05

**Fault and expected failure:**
Add fabricated cross-world Promise or omit exposed method → exact shape/return RED.

#### BRG006_error_copy

**ACs:** C07

**Fault and expected failure:**
Return raw Error/native path or silently null → name/message/security-cell RED.

#### BRG007_send_snapshot

**ACs:** C08,C18

**Fault and expected failure:**
Mutate page object after send before Host handoff → transmitted bytes mismatch; Function/Symbol send must throw before Bun.

#### BRG008_emitter_once

**ACs:** C09

**Fault and expected failure:**
Remove once listener *after* callback and recursively emit → duplicate event RED.

#### BRG009_emitter_remove

**ACs:** C09,C16

**Fault and expected failure:**
Ignore original listener identity or buffer no-listener events → unmatched replay RED.

#### BRG010_event_shape

**ACs:** C10

**Fault and expected failure:**
Send raw WebContents/Host object as event.sender → facade equality/authority RED.

#### BRG011_final_guard

**ACs:** C11,C21

**Fault and expected failure:**
Replace byte-equal literal match with prefix/wildcard → unauthorized Bun write observed.

#### BRG012_stale_document

**ACs:** C12,C17

**Fault and expected failure:**
Drop addressed-document stamp and target newest page → successor sentinel RED.

#### BRG013_precommit_gate

**ACs:** C13

**Fault and expected failure:**
Treat early app-world publication or `file:` string as committed eligibility → forbidden handler count RED.

#### BRG013b_world_bind

**ACs:** C13,C21

**Fault and expected failure:**
Native app/trusted credentials are distinct under one current checked generation. Refuse missing/stale/swapped/duplicate
credentials and old-document delayed bind with independent Host/Bun zero-effect counters; remove present-binding check → RED.

#### BRG013c_early_void

**ACs:** C08,C13,C14

**Fault and expected failure:**
Invoke exposed void method before app bind or accepted Bun link, observe undefined + one internal refusal, zero retained work and
no queued successor replay, then allowed call after bind; buffer old work or invent reply → RED.

#### BRG014_real_page_is_fullscreen

**ACs:** C14

**Fault and expected failure:**
Replace actual pinned `EditorUi.init` `isFullscreen` with a harness-injected first call, bypass original main listener, hardcode
Boolean or duplicate assigned `reqId` → original-page/handler/response witness RED.

#### BRG015_release_barrier

**ACs:** C14

**Fault and expected failure:**
Initiate the **controlled** second preload request at callback while first native reservation is still held → independent
release/credit RED; after release a distinct request ID and healthy callback succeed. Controlled traffic is not a second natural
page call.

#### BRG016_file_url

**ACs:** C15

**Fault and expected failure:**
Rewrite committed URL to custom scheme or substitute guessed senderFrame.url → native URL equality RED.

#### BRG017_recovery

**ACs:** C12,C17

**Fault and expected failure:**
Replay old queued event after Bun restart or retain retired callback → stale-tag sentinel RED.

#### BRG018_disallowed_wire

**ACs:** C18

**Fault and expected failure:**
Serialize nested page callback or Symbol into IPC data → codec/local error and zero-write checks RED.

#### BRG019_manifest_admission

**ACs:** C19,C20

**Fault and expected failure:**
Change an expected-fail row to PASS/skip or drop implementing-ticket/source → GH-566 validation RED.

#### BRG020_native_world_hostile

**ACs:** C01,C21

**Fault and expected failure:**
Register app handler in trusted world or omit controller/frame check → native hostile sender causes forbidden effect RED.

#### BRG021_symbol_oracle

**ACs:** C22

**Fault and expected failure:**
Make observed-oracle comparison a fixed guess instead of pinned executable result → doc-vs-native discrepancy remains UNKNOWN and
test cannot qualify PASS.

#### BRG022_two_window

**ACs:** C12,C17,C21

**Fault and expected failure:**
Cross-bind old window/document callback to second window → other-window payload marker RED.

#### BRG023_page_listener_allowlist

**ACs:** C09, C10, C21.

**Fault and expected failure:** Permit page registration for `mainResp`, `fileChanged` or an unlisted channel. The hostile-page observer must see the private event or callback. Clean registration accepts only the pinned finite allowlist.

#### BRG024_renderer_action_allowlist

**ACs:** C08, C11, C14.

**Fault and expected failure:** Permit an unlisted `msg.action` under the `el:rendererReq` channel grant. The independent Host/Bun effect observer must record a forbidden action. The first-proof `isFullscreen` control must remain healthy.

#### BRG025_prebind_local_listener_registration

**ACs:** C09, C13, C21.

**Fault and expected failure:** Refuse local listener registration before bind, or forward that registration to Host. The pinned `EditorUi.init` listener must be available for a later allowed event after bind. The pre-bind Host/Bun frame count must remain zero.

#### BRG026_request_callback_terminal_cleanup

**ACs:** C08, C11, C13, C23.

**Fault and expected failure:** Suppress request preflight or the terminal Host denial event, or let a callback exception bypass cleanup. Repeated rejected requests must not retain `reqInfo`, leave `requestSync` pending or deliver to a successor document.

Do not aggregate all 22 into a single giant mixed-owner
test module. GH-464 owns its corpus cells, GH-567 preload/process
corpus, GH-484 native realm smoke and GH-467 graph/listener/application
fixtures. Keep the provenance of each author, executable binary,
OS/WebKit version, exact repo head, test id, mutant id, raw failure,
clean pass and CI links. Record an unrun cell as unrun.

## 9. Perf impact

No performance comparison or claim in this specification. The
existing KEL-90/keld-benches authority owns metric registration;
GH-484 registers its one PANEL-D21 app-world diagnostic metric.
GH-540 owns per-direction queued DATA/CONTROL bytes, terminal
receipts, native/ring physical release and all-producer caps.
GH-467 additionally proposes the **finite document-scoped
JS callback/proxy graph** in §4.3: depth 16, 256 members,
65,536 UTF-8 data bytes per crossing, 64 proxy endpoints,
64 callback registrations and 524,288 accounted retained bytes
per document. These limits need independent implementation
review and actual retained-representation measurement; they are
not WebKit heap measurements or approved runtime constants.

Capture a source-backed retained-reference census at: API exposure,
an ordinary call, pending watchFile callback, dropped listener,
same-document navigation, cross-document retirement, Bun restart
and WebView destroy. Test max+1 refusal and a healthy next-document
call. Do not relax Host20/Guardian/native timeouts, add sleeps to
hide ordering, or infer speed from a small 4 KiB fixture.

## 10. Open questions, blockers & unknowns

1. **Q1 — Option A product first-proof scope selected; security BLOCKED:**
   For the pinned draw.io only, use the document-start **predeclared
   four-function** page facade, with source-backed actual app-preload
   registry and copied frozen `process` facts (GH467 owner packet
   [selection](https://github.com/gyldlab/keld/issues/467#issuecomment-6390880361)).
   This deliberately does **not** implement full general dynamic
   `contextBridge`/C06 and is not a native Electron-parity PASS; GH-464
   must preserve the unproved generic cells. GH-484 must prove real
   document-start injection, exact authentic unmodified preload source,
   app-world handler and lifetime. **Separately, GH484/KEL-142 must
   resolve [Q1 A1 vs A2](https://github.com/gyldlab/keld/issues/484#issuecomment-6390973710):
   retain its existing no-page-world-native-endpoint rule and prove
   another WebKit transport, or explicitly amend that rule for a
   data-only untrusted page carrier with independent native/security
   review. Neither is currently approved.** GH-467 must prove that all four
   exposed methods invoke the current original app-registered methods
   and never derive authority from forgeable page messages. Independent
   WKWebView/Host/Bun positive, hostile-page, no-wrapper, no-bind,
   stale-document, absent-grant and one-fault allowlist controls remain
   mandatory. Existing fixed-wrapper 3/3 and 2/2 omission, F0 16/16
   and 6/6 nested-callback probes are **test-only** and do not approve
   shipping. **Alternative Option B:** full dynamic graph publication
   stays P0 blocked pending separately approved secure platform primitive
   and complete C06/score qualification. GH441's separate-world
   placement is CLOSED and unchanged. Named security/native/compat
   reviewers must still approve this narrowed first-proof contract
   before any production bridge code or conformance PASS.
2. **Q2 — Symbol oracle, UNKNOWN:** pinned Electron v44.4.5 documentation
   and native source disagree on Symbols. The executable pinned oracle
   or an explicitly approved compatibility divergence is required
   before a Symbol PASS. GH-544's IPC Symbol rejection is unchanged.
3. **Q3 — finite graph and local errors, proposed policy (§4.3):**
   depth 16, 256 keys/elements, 65,536 UTF-8 data bytes, 64 proxy
   endpoints, 64 callback slots and 524,288 charged retained bytes per
   document, with descriptor-only/no-accessor traversal and copied
   Error name/message. GH-467 and the error-registry owner must approve
   a distinct `KELD-COMPAT` over-limit code. Independent 0/max/max+1,
   alias/cycle, Proxy-trap, error-copy and measured-retention tests
   must fail the individual mutations before implementation qualifies.
4. **Q4 — pre-bind work, selected fail-closed:** zero queued early work;
   void caller sees `undefined`, the Host logs a bounded private typed
   refusal, and no later document/Bun role can replay it. Required
   pre-bind draw.io startup traffic would leave the first-proof C14
   **BLOCKED** until real startup order changes safely or #540/#542
   separately approve a revised policy. GH-540/#542 must approve two
   distinct native App/Trusted credentials under one checked-u32
   navigation owner. The [33-receipt physical native fixture](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584)
   supports that proposal, but does not prove KELD binding or grants.
5. **Q5 — multi-fire callback lifetime, proposed policy (§4.3):**
   retain `watchFile` slot across N events; charge/release independent
   GH-540 event/native reservations for every delivery; free the slot
   exactly once on unsubscribe, authenticated role-bound retirement
   or document retirement. Confirm exception copying and synchronous
   once-removal with original-function owner, repeated event controls,
   stale-role failure and counted slot teardown on the real Mac.
6. **Q6 — one bundled source and authority, BLOCKED:**
   GH-484 must identify the admitted immutable preload image, one
   renderer facade bundle built from `packages/@keld/electron` sources
   and a proven at-document-start import/export path that does not
   expose page-world Node or add a second transport. The admitted
   package source must satisfy GH-484's separately BLOCKED trusted
   manifest/digest and absolute path gates. Source presence alone is
   not proof of runtime execution.

### B-1: Secure cross-world publication decision

- **Status:** open
- **Owner:** GH-484 and KEL-142 native owners
- **Impact:** No production callable facade or page-to-app native endpoint may ship.
- **Check:** Approve A1 or A2 at exact content and pass the real first-inline hostile-page native proof.

### B-2: Document-correlated dual-world Bind

- **Status:** open
- **Owner:** GH-540 and GH-542 owners
- **Impact:** No app-world credential, bound-document signal or app v2 named send is enabled.
- **Check:** Approve the document-correlated Bind handshake and qualify two distinct realm credentials against the current committed document.

### B-3: Final Host action and channel authorization

- **Status:** open
- **Owner:** GH-552 permission-model owner
- **Impact:** No renderer request reaches Bun, even when the page facade is visible.
- **Check:** Approve exact channel and action allowlists, then pass zero-write denial and one genuine granted handler entry.

### B-4: Immutable preload source and document-start custody

- **Status:** open
- **Owner:** GH-484 source/image owner
- **Impact:** A visible wrapper cannot prove the authentic pinned preload or its startup seed.
- **Check:** Prove the admitted immutable preload bytes, real app-world registration and conditional seed before first inline.

### B-5: Pinned conformance corpus and evidence provenance

- **Status:** open
- **Owner:** GH-464 and GH-567 owners
- **Impact:** No general Electron compatibility PASS or expected-fail cell may be promoted.
- **Check:** Land pinned source cells, implementation-ticket mappings and independent one-fault corpus validation.

### B-6: Typed local bridge error and callback-retirement contract

- **Status:** open
- **Owner:** GH-467 and Host error-registry owner
- **Impact:** No implementation may invent a local error code or leave rejected request callbacks retained.
- **Check:** Approve the typed refusal and terminal Host error event; pass codec rejection, final denial, callback-throw and retired-document cleanup tests.
**Disposition:** changes above are reviewable draft contracts. Full
security-qualified publication, #540/#542 world binding, registered
errors, source-image authenticity, real Host/Bun effects and independent
security/API review remain **BLOCKED**. Keep this PR OPEN/DRAFT and
do not enable compatibility or promote conformance cells until all
named gates pass.
