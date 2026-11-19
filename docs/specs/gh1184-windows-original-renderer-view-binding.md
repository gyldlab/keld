# Spec: Windows original renderer view binding

- Status: draft
- Tracker: GH-1184
- Owner: @0monish, GH531 Window Registry contract authority
- Updated: 2026-11-19
- Pin: 16a9545ccc417b72b2f0508520c360f9fd5e2ced

## 1. Goal & non-goals

Bind each selected Windows original WebView to its logical Window. The unchanged XML app receives the genuine `WebContents` sender and can reply to the exact source.

Reuse the existing document, permission, writer, ledger and ownership boundaries. Keep other Window dialects and the public Electron facade intact. Recovery of a registered route on a surviving document remains GH1404 work.

## 2. Spec refs

- [GH531](https://github.com/gyldlab/keld/issues/531) owns the Window pair and replay. Its approved [recovery rule](https://github.com/gyldlab/keld/blob/9a1559f3c0a5b7174144b6351d6a86985628dbb7/docs/specs/gh531-window-registry-close-state-machine.md#L291-L295) requires the full Created census first.
- [GH540 exact approval](https://github.com/gyldlab/keld/issues/540#issuecomment-6371793024) owns DATA and CONTROL. Sections 4.8 and 4.9 separate applied state and deferred dispatch.
- [GH1184 approved Windows XML scope](https://github.com/gyldlab/keld/issues/1184#issuecomment-6801094337), [GH1185](https://github.com/gyldlab/keld/issues/1185) and [GH1188](https://github.com/gyldlab/keld/issues/1188) own selected preload, native navigation and the Windows window adapter. Architecture 02 section 2 owns framing. Architecture 03 owns default-deny.
- The pinned main defines the logical pair at `crates/keld-ipc/src/window.rs:27`. The published Windows candidate allocates the physical view independently at #1531 `crates/keld-wv/src/webview2/view_build.rs:277@1a8325d4262ea5cab5799aa2ecdb1987b2512e71`.
- The published candidate has Window event ordinals 0 through 10 at #1531 `crates/keld-ipc/src/window.rs:95@1a8325d4262ea5cab5799aa2ecdb1987b2512e71`. The final shared base must reconcile the qualified GH1185 finish at 11 before this proposal allocates 12.

## 3. Acceptance criteria

### AC-1: Independent identities select the genuine sender

- **Given:** Two actual selected views load the same URL at equal generations. Each physical identifier differs from its logical identifier.
- **When:** Each preload sends a granted topic. The main role replies to each original source.
- **Then:** Each handler receives its genuine `WebContents` and authentic URL. Each view receives only its own reply.
- **Negative control:**
  - **Single fault:** Select the physical view by the logical identifier.
  - **At-risk precondition:** The fixture observes distinct physical and logical identifiers independently.
  - **Expected RED:** The exact sender identity or addressed reply fails.

### AC-2: Creation and recovery preserve their ordering

- **Given:** A native Create succeeds. Two previously bound original views survive a separate role replacement. At least one has a permitted recovered document observation.
- **When:** Publish Create success and the authenticated successor's Subscribe replay. Repeat Subscribe.
- **Then:** Creation sends Created, binding, then its reply. Recovery sends all Created facts in creation order. Each permitted document fact follows its own view binding. All replay facts precede the reply. Repeated Subscribe adds no duplicate facts. Adopt preserves the pair.
- **Negative control:**
  - **Single fault:** Send the first binding before the last surviving Created fact.
  - **At-risk precondition:** Two eligible survivors exist in distinct creation order.
  - **Expected RED:** The captured stream has a binding inside the Created census.

### AC-3: A held delivery cannot follow document replacement

- **Given:** A and B use one physical view. The same role stays active. Hold one admitted A delivery. B later has a fresh commit, registration and the exact grant.
- **When:** Commit B, release the held A delivery, then send a fresh B message. Test old event reply and old current-main delivery separately. Also exercise late A retirement after B commit.
- **Then:** B observes only the fresh delivery. Late A retirement preserves B. The tests keep role authority live throughout.
- **Negative control:**
  - **Single fault:** Remove final document and lease revalidation for the held delivery.
  - **At-risk precondition:** B's exact writer, role and grant can accept the same topic.
  - **Expected RED:** The final native handoff carries A's lease while B is the committed document. Observe the handoff before the preload credential check. Keep the raw B counter as a separate live oracle.

### AC-4: Binding grants no registration or topic authority

- **Given:** Created and the binding are visible. Hold qualified Committed or Registered in separate runs. The current topic is granted.
- **When:** Attempt delivery before releasing each held receipt. Then complete qualification and try again.
- **Then:** Both early attempts cause zero native effects. A qualified attempt reaches the intended view.
- **Negative control:**
  - **Single fault:** Admit final delivery from the physical pair alone.
  - **At-risk precondition:** The writer and role are live and the pair already exists.
  - **Expected RED:** The final native admission witness appears before the required receipt.

### AC-5: The codec preserves previously defined records

- **Given:** Literal controls cover zeroes, truncation, overflow, trailing bytes and a free ordinal after final-base census.
- **When:** The supported Rust and TypeScript receive entry points decode those controls. Test the earlier installed decoder separately.
- **Then:** The new codec accepts `0c 07 ac 02 05` as logical pair (7,300) and physical view 5. The earlier decoder refuses it. Previously defined records keep their bytes. The new decoder refuses a still unallocated ordinal.
- **Negative control:**
  - **Single fault:** Omit the nonzero check for one identity field. Repeat independently for all three fields.
  - **At-risk precondition:** Each payload has only that zero identity and is otherwise valid.
  - **Expected RED:** The decoder accepts a zero identity at that entry point.

### AC-6: The binding has a hard received limit

- **Given:** An overlong tag encodes ordinal 12 before the complete maximum tuple.
- **When:** Receive `8c 00 ff ff ff ff 0f ff ff ff ff ff ff ff ff ff 01 ff ff ff ff 0f` through each supported raw entry point.
- **Then:** The 22-byte record refuses before the mirror sees it. The equivalent 21-byte canonical record succeeds.
- **Negative control:**
  - **Single fault:** Remove the raw size check for detected ordinal 12.
  - **At-risk precondition:** The payload fits the general WINDOW limit and its fields are valid.
  - **Expected RED:** The decoder accepts the oversized binding.

### AC-7: WorkerLink applies observations before DATA

- **Given:** The stream holds Created, binding, qualified commit, matching DATA and a blocking-call reply. Bun main is inside the real blocking call.
- **When:** WorkerLink applies through the reply watermark while the fixture holds user dispatch.
- **Then:** The exact pair and document exist at the DATA state applier and immediately after call return. User callback count remains zero at both cuts. Deferred dispatch receives the genuine sender.
- **Negative control:**
  - **Single fault:** Move commit application into the deferred Window listener.
  - **At-risk precondition:** The real ordered stream arrives before the blocking call returns.
  - **Expected RED:** One of the state observations lacks the committed document. A later successful handler does not satisfy this check.

### AC-8: An old publisher cannot acquire successor custody

- **Given:** The actual source capture yields an old caller for an Open view. The successor can own that view. Final admission can be probed in isolation if the live publication turn excludes that overlap.
- **When:** Present the stale capture to final admission. Separately retire the original role with its genuine writer stalled and let that operation settle before successor replay.
- **Then:** Stale admission refuses. The live old write follows its own retirement. Only the successor's own replay appears on its link. The old ledger receives no new credit.
- **Negative control:**
  - **Single fault:** Reconstruct the target from the latest caller at final admission.
  - **At-risk precondition:** The probe sees an exact stale capture and a different active caller. The live run separately records old turn release.
  - **Expected RED:** The probe returns a successor target for the old capture.

### AC-9: A neighboring topic remains ungranted

- **Given:** An active view has the literal `el:mainResp` grant. A raw page observer counts that topic and the adjacent `mainRespx` literal.
- **When:** Submit well-formed outbound DATA for each literal to the same registered view.
- **Then:** The exact topic reaches the view. The neighbor returns the existing Guard denial with zero native effects. A granted follow-up succeeds.
- **Negative control:**
  - **Single fault:** Omit the exact topic check at Host-to-renderer DATA admission.
  - **At-risk precondition:** The neighboring topic passes shape, role, view, lease and capacity checks.
  - **Expected RED:** The raw page observes the neighboring topic.

### AC-10: A successor starts unregistered on a surviving view

- **Given:** An authenticated replacement takes over a view with a live original lease.
- **When:** Replay and adopt the view. Attempt both delivery directions before and after Adopt. Then qualify a fresh document.
- **Then:** Replay and Adopt preserve the pair. The successor gains no route until fresh qualified commit and registration. The fresh document can deliver.
- **Negative control:**
  - **Single fault:** Initialize successor registration from the surviving lease at replay.
  - **At-risk precondition:** The old lease, new role and exact topic grant are valid.
  - **Expected RED:** The successor is registered before fresh qualification.

### AC-11: Deferred DATA keeps its original sender

- **Given:** The real link applies DATA for A. Hold its dispatch as Bun applies later state.
- **When:** Apply A retirement and B commit. In a separate case call synchronous Destroy with another selected window still open. Then release A's dispatch.
- **Then:** Navigation keeps the same `WebContents` object. Destroy keeps the original facade available to the deferred event. Both return null `senderFrame` and keep valid old replies inert. B receives no old effect and can receive fresh granted DATA. Destroyed preserves public tombstone order.
- **Negative control:**
  - **Single fault:** Resolve the deferred sender from the live facade map in the Destroy case.
  - **At-risk precondition:** Synchronous Destroy removed the original facade before A's deferred dispatch. A different view remains live.
  - **Expected RED:** A's genuine sender disappears or changes.

### AC-12: The exact provider reaches the actual Bun generation

- **Given:** A qualified Host, CLI and packed SDK launches the real initial and successor roles. A valid earlier provider is also available.
- **When:** Test initial and successor provider mismatch. In separate cases change the provider after Host admission and before consumption.
- **Then:** Only the qualified generation can receive the binding or selected authority. An earlier source refusal counts if it identifies the mismatch and no selected bytes arrive. Other dialects preserve their existing behavior.
- **Negative control:**
  - **Single fault:** Remove one generation's effective exact-provider custody check.
  - **At-risk precondition:** The positive tuple works. The owner probe reaches the targeted check with the wrong provider while all its other admission inputs are valid.
  - **Expected RED:** That owner accepts incompatible bytes. An end-to-end zero-frame check remains independent. Exercise actual consumption after the last check with all other prerequisites observed.

### AC-13: An opening window stays outside successor replay

- **Given:** The native WindowBuilt result is held before its publication turn. Its entry remains Opening.
- **When:** Retire the old role, replay on the successor, then release the held result.
- **Then:** The replay excludes the opening window and its binding. The original candidate follows checked native teardown.
- **Negative control:**
  - **Single fault:** Include the retired build's teardown-only entry in the replay census.
  - **At-risk precondition:** Role retirement has fenced the original build. No Created transition was admitted when the successor subscribes.
  - **Expected RED:** The successor sees the uncommitted pair.

### AC-14: The frame getter keeps the original document generation

- **Given:** A and B share one live `WebContents`. The link has applied an A DATA record and holds user dispatch.
- **When:** Apply A retirement and B commit. Dispatch A. Then dispatch a separately granted B record.
- **Then:** Both events identify the same facade. A's getter returns null. B's getter returns B's authenticated frame. Application handlers still observe each exact record once.
- **Negative control:**
  - **Single fault:** Resolve `senderFrame` from the current document on the physical view.
  - **At-risk precondition:** B is current and its frame is readable before A dispatches.
  - **Expected RED:** The getter returns B's frame for retained A DATA.

### AC-15: Qualified receipts name the same lease

- **Given:** On one real view, retain a commit receipt for A and a later registration receipt for B. The role and topic grant stay live.
- **When:** Present both receipts to the readiness join in each arrival order. Then present B's matching commit and registration.
- **Then:** The mixed pair has zero admissions. B's matching receipts can admit delivery.
- **Negative control:**
  - **Single fault:** Remove the exact lease equality check at the join.
  - **At-risk precondition:** Both qualified receipts reach the join while later native fences remain independent.
  - **Expected RED:** The owner reports readiness from two distinct leases.

### AC-16: Partial publication preserves an eligible survivor

- **Given:** The Registry accepted BuildOk and the original link received Created. The same native view remains Open.
- **When:** Cause recoverable role loss before binding in one run and after written binding in another. Hold the Create reply before admission. Release the old publication turn before the authenticated successor subscribes.
- **Then:** The original link receives zero correlated success frames. The same WindowRef and physical view survive. The successor receives one Created and one binding for that view in its ordered replay. It receives no old reply.
- **Negative control:**
  - **Single fault:** Treat the partially published Open entry as Opening at original role retirement.
  - **At-risk precondition:** The Host grants recovery. The view has no independent close or release, and the successor's Subscribe succeeds.
  - **Expected RED:** The same WindowRef and view fail to appear together in successor replay.

### AC-17: Session end releases a partially published view

- **Given:** The Registry accepted BuildOk and the original link received Created. Use both partial-publication cuts from AC-16.
- **When:** The Host accepts session end instead of recoverable role loss while the original Create reply remains unadmitted.
- **Then:** The original link receives zero correlated success frames. The Registry changes the same entry to Closing and issues one teardown command for its view. Observe the checked native release separately. The entry never enters replay.
- **Negative control:**
  - **Single fault:** Omit the Registry's per-entry teardown command for this Open window.
  - **At-risk precondition:** Observe the exact Core command boundary before terminal native cleanup can mask the omission.
  - **Expected RED:** Core issues no DestroyWindow command for that view. Later release cannot satisfy this command check.

### AC-18: An unproved reply remains with its original writer

- **Given:** The original writer has positive Created and binding receipts. It has admitted the Create reply for the same exact correlation.
- **When:** The actual writer reports an unknown reply outcome. Recover only through an authenticated successor that the existing Runtime admits.
- **Then:** Preserve the original outcome and its custody. Record any old-link bytes independently. If recovery is admitted and the view remains eligible, replay one fresh Created and binding for its same pair. Never complete or retry the old reply on the successor.
- **Negative control:**
  - **Single fault:** Reissue the original correlation as a successful reply when the successor subscribes.
  - **At-risk precondition:** The original writer retained the unknown outcome. The same view can replay. The successor uses a distinct correlation during the observation.
  - **Expected RED:** The successor link carries a reply for the retired original call.

### AC-19: Unproved binding cannot authorize a Create reply

- **Given:** The original writer has a positive Created receipt. The selected binding has no positive write receipt.
- **When:** Exercise the reply decision in the existing publication owner. Use an owner probe if actual write failure first retires the caller.
- **Then:** The owner never offers correlated Create success for that binding. Its writer-failure and native-custody dispositions remain independent.
- **Negative control:**
  - **Single fault:** Omit the positive binding receipt check at the reply decision.
  - **At-risk precondition:** The source and correlation are otherwise eligible. A positive binding receipt exercises the successful admission path separately.
  - **Expected RED:** The origin offers correlated success without a proved binding.

## 4. Design

### D-1: Physical association on WINDOW

The Registry owns the logical pair. The native View owns its physical identifier. Publish one selected observation to the current authenticated Runtime link.

Append `WindowEventValue::OriginalRendererViewBound { window: WindowRef, webview: u32 }` on WINDOW. Approval covers ordinal 12 while it remains free on the final shared base. A collision requires new vectors and refreshed approval. Preserve previously defined events, requests, replies, framing and HELLO.

| Element | Postcard order | Bound |
| --- | --- | --- |
| Variant | Unsigned varint 12 | Hex `0c` |
| `window.id` | Unsigned varint u32 | Nonzero |
| `window.incarnation` | Unsigned varint u64 | Nonzero |
| `webview` | Unsigned varint u32 | Nonzero |

Use `EVENT`, flags 0, WINDOW channel 5 and correlation 0. The literal for `(1,258,7)` is `0c 01 82 02 07`. The canonical maximum is `0c ff ff ff ff 0f ff ff ff ff ff ff ff ff ff 01 ff ff ff ff 0f`.

Check the 21-byte received limit for detected ordinal 12 before field decode. Count the original tag bytes. Share the existing discriminant reader. Each supported raw receive entry point must enforce the same bound. A decoded value alone cannot prove the received size. Preserve unrelated Window and COMPAT receive limits.

### D-2: Publication and replay

Use the actual WindowBuilt result and existing publication turn. Send Created and then binding. Require positive write receipts for both before admitting the correlated Create reply. Recheck the exact entry, physical view and role at final admission.

Send the whole Created census first in recovery under the existing Subscribe turn. Send each permitted document observation after its own qualified binding. Complete replay before the acknowledgment. Duplicate Subscribe remains idempotent. Only qualified Open or ClosePending entries can participate. Adopt preserves exact association.

The Registry can replay a surviving association independently of the retired caller. The current successor remains unregistered until fresh qualified commit and registration. Require separate GH1404 ownership for surviving-document route recovery.

Preserve the original writer outcome if publication fails or its receipt is unproved. Withhold later correlated success. If the reply write was attempted and its outcome is unknown, keep the exact writer's failure custody. An unknown receipt provides no proof of delivery. Never reissue the original success from replay.

Accepted BuildOk changes Opening to Open before publication. After Created or after binding, a publication failure does not itself change that native phase. If the Host grants role recovery, keep the checked Open view and its WindowRef for the authenticated successor's ordered replay.

A pending close follows the existing [abandoned-close rule](https://github.com/gyldlab/keld/blob/9a1559f3c0a5b7174144b6351d6a86985628dbb7/docs/specs/gh531-window-registry-close-state-machine.md#L809-L810) before replay. An independently Closing or Destroyed entry stays outside replay.

If the Host accepts session end, move the same Open view to Closing and wait for its checked native release through normal teardown. Session end supplies no replay. If role loss or session end wins while the entry is still Opening, use the late-build cleanup from AC-13. An unproved native handoff cannot qualify for replay.

These cuts inherit GH531's [BuildOk and Opening rules](https://github.com/gyldlab/keld/blob/9a1559f3c0a5b7174144b6351d6a86985628dbb7/docs/specs/gh531-window-registry-close-state-machine.md#L769-L779).

The [Open retirement rules](https://github.com/gyldlab/keld/blob/9a1559f3c0a5b7174144b6351d6a86985628dbb7/docs/specs/gh531-window-registry-close-state-machine.md#L791-L794) own recovery and session end. The existing primary writer retains the old link and its charges.

### D-3: Mirror, applied state and deferred dispatch

Apply the binding in the canonical Window mirror. Require the exact live Created pair. Reject an unknown, destroyed, foreign, changed or multiply bound view through the existing invalid-fact terminal path. An identical repeat may be idempotent. Retain one scalar association per row.

Use the existing qualified navigation facts for document state. Route selected `DocumentRecovered` and `NavigationFinishedUnitPinchV1` through their existing owners. Recovery grants no registration. Qualified finishes complete each load once.

The canonical WorkerLink applies Created, binding and commit before matching DATA, including during a blocking main call. Resolve the stable original sender association at DATA application. Preserve enough identity to return the exact existing facade after retirement clears the live lookup.

A facade constructed after blocking Create still materializes for its exact WindowRef before accepted dispatch. Keep this within the existing bounded record lifetime.

Capture the original view and generation for `event.reply`. At ordered invalidation the live `senderFrame` getter returns null. A valid retained old reply is inert. The exact route for approved DeliverToCurrentMain is resolved at admission and rechecked before native effect.

Binding proves association only. Require genuine commit, registration, live role and exact Guard grant. Commit and registration may arrive in either order. Both must name the same opaque lease. COMPAT DATA and CONTROL bytes remain identical. ReleaseConsumed follows actual disposal, independently of native completion.

### D-4: Exact provider and compatibility

Use the existing admitted graph, distribution tuple and per-generation Runtime source recheck. Select only the compatible packed Host, CLI and SDK. Prove custody from admission to the Bun main decoder and Worker transport imports on initial and successor launch.

A physical graph receipt alone does not prove either later import. Unproved custody takes the existing refusal path before selected output.

Earlier decoders refuse the new tag. Keep other dialects unchanged. Reuse the sealed GH540 delivery layout. A repeated Window pair in every DATA frame or a second document acknowledgement would duplicate existing ownership. A URL, focus or numerical identity inference cannot distinguish equal-generation views.

## 5. Boundaries

Use the existing Rust WINDOW receive owner, Core Registry and primary writer. Update the existing TypeScript generator, Bun Window mirror and sole KIPC state applier. Use the internal Electron facade owner for the stable sender identity. Preserve the public methods, grants, dependencies, native APIs and COMPAT layout.

## 6. Tasks

### T-1: Freeze the exact codec

- **Blocked by:** B-1, B-3
- Add literal accepted and rejected vectors to the existing receive tests. Move the new suite's old ordinal-12 unknown test to a free tag. Preserve old rejection in the earlier-provider fixture. Reuse the existing generator.

### T-2: Join native publication and mirror application

- **Blocked by:** T-1
- Connect the real Built association, complete Created census, existing replay turn and checked admission. Apply the fact in the canonical WorkerLink before deferred dispatch. Keep selected traffic gated on provider qualification.

### T-3: Qualify both provider edges

- **Blocked by:** T-1
- Build and verify the matched distribution. Prove the effective main decoder and Worker transport at startup and recovery. Close B-2 with independent source and consumption witnesses.

### T-4: Qualify selected delivery and retirement

- **Blocked by:** T-2, T-3, B-2
- Join approved GH540 DATA and CONTROL using the existing Guard, writer and ledger. Run AC-1 through AC-19 on one exact Windows Host, CLI and SDK tuple.

## 7. Test plan & evidence

### E-1: Source and wire evidence

- **Run:** Independently inspect sealed specs and pinned sources. Calculate the literal vectors and enumerate replay and deferred-dispatch schedules.
- **n:** Two independent review lenses and a separate refutation of their proposed findings.
- **Environment:** Windows x64, local source inspection and data-only controls on 2026-11-19.
- **Limits:** These checks do not execute the proposed product behavior or prove Windows acceptance.

### E-2: The earlier qualified provider refuses the binding

- **Run:** Independently recompute the three pinned provider tree digests. Decode an existing Window fact and the new literal with the earlier installed SDK module.
- **n:** Three matching provider digests, one accepted existing fact and one rejected binding.
- **Environment:** Windows x64 and Bun 1.4.4 on 2026-11-19.
- **Limits:** This runs the earlier decoder in isolation. It proves neither physical link ingress nor the new provider boundary.

This receipt uses an earlier local BOOT8 distribution. The [published #1531 candidate](https://github.com/gyldlab/keld/pull/1531) supplies the code structure reference.

| Earlier provider | Tree SHA-256 |
| --- | --- |
| Electron | `41fc320426d8e17de206948ec77092ce3c40b5ebf8b756095498c13649928f50` |
| API | `a0f3d878d278a19e5f45fcf7ded4e59f6be90d15673251b0c6c5d0de35381ac2` |
| KIPC | `b7f34a14bdf4aa5de79aa1b2fb897dac878324e0d745d94025306985b184f009` |

The installed decoder SHA-256 is `668a9c6e284ffbda1b983dc6ff8fc2cd642e7cbde425e2eba684c544fd5e9c5c`. It accepted `06 01 82 02` and rejected `0c 01 82 02 07` with `KELD-IPC-003`.

Keep byte, writer-frame, state-applier, listener and native-effect witnesses independent. Test the correct order and the specified single-fault schedule separately. Identify a source-owner probe separately from a live link test.

## 8. Review gates triggered

Public API and wire protocol. The implementation requires independent architecture and security evidence on its exact diff. This proposal preserves the existing permission model, dependencies and unsafe surface.

## 9. Perf impact

The new layout has one bounded 21-byte record per selected view at construction or replay. DATA has no added bytes. Use the existing writer and bounded accepted record lifetime. Measure actual retention under a held receipt before a performance claim.

## 10. Open questions, blockers & unknowns

### B-1: Exact contract approval and write scope

- **Status:** open
- **Owner:** @0monish and the designated WINDOW contract writer.
- **Impact:** Wire, SDK and Host implementation wait for exact approved bytes and a winning claim.
- **Check:** Approve the exact draft hash against the final base. Record the shared owner on GH1184. Preserve the sealed GH531 contract and closed tracker.

### B-2: Prove provider custody through actual consumption

- **Status:** open
- **Owner:** Selected Runtime and Windows distribution owners.
- **Impact:** Keep selected emission disabled until both physical generations prove matching provider consumption.
- **Check:** Observe both loaded provider edges and actual traffic at each authenticated role. A valid old provider or a post-admission substitution cannot receive the binding or selected authority.

### B-3: Reconcile existing selected variants

- **Status:** open
- **Owner:** WINDOW contract and GH1185 navigation owners.
- **Impact:** Final codec, publication and replay depend on the reconciled selected contract.
- **Check:** Census defined ordinals on the final shared base. Preserve qualified behavior and bytes for tags 0 through 11. Verify ordinal 12 remains free before approval.
