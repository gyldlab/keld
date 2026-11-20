# KEL-78 delivery and tasks
Part of: [index](index.md)
Owns: sections 5–6

## 5. Boundaries

The historical R1 design below defines the proposed implementation boundary for this split; the split still requires exact-content owner review. Publishing this specification does not implement it; #588 still requires an integrated specification base, a winning implementation claim, and the independent gates below.

| Area | Expected files/owners |
|---|---|
| Profile parse/accessor and parser tests | `crates/keld-guard/src/lib.rs`, existing JSONC/unique-key tests |
| Strict-state reuse | `crates/keld-guard/src/admit.rs`; preserve strict checks |
| Verified policy lifetime | `crates/keld-guard/src/verified_manifest.rs`, `crates/keld-core/src/app_session.rs`; no second loader |
| Boot version and all-entry preflight | `crates/keld-core/src/app_session/boot_contract.rs`; owning host/boot tests |
| macOS descriptor producer | `crates/keld-cli/src/boot.rs`; reuse exact-byte capture |
| Launch integration | Existing macOS app-session and supervisor/guardian owners; no second reaper |
| Doctor and shared disclosure | `crates/keld-cli/src/doctor.rs`; existing JSON/MCP/error contracts |
| Receipt/scoreboard integration | Existing KEL-74/GH-532 corpus receipt/report owners |
| Governing docs | KEL-78 amendment, relevant Architecture 03/06 clauses, key-owner/reporting references |

Do not change public kipc frames, per-channel grant grammar, renderer generation ownership, updater authority, native-addon policy, persistent WebView profiles, role declaration grammar, or OS sandbox mechanisms. The descriptor version is the sole intentional boot-contract change.

No new production unsafe operation or dependency is required by this design. If implementation reveals one is required, it needs its own explicit review rather than being smuggled into profile parsing.

## 6. Tasks

Historical R1 completed P0 at the recorded exact revision. The split revision has not completed exact-content owner review.

### T-1: land parser, boot version and verified Host selection
- **Blocked by:** none
- Implement the parser, compiled boot version and verified Host selection.
- Implement the macOS primary receipt and required runtime/developer disclosure.
- Keep producer, consumer and disclosure atomic.

### T-2: integrate downstream receipts and scoreboard
- **Blocked by:** T-1
- Bind receipts to the exact artifact and admitted state.
- Preserve unverified scoring and reject mixed-authority aggregation.

### T-3: prepare the hand-authored draw.io fixture
- **Blocked by:** T-1, T-2
- Keep the exact fixture reproducible.
- Keep unimplemented and unknown corpus cells visible.
- Do not claim full P1/P2 while the trusted collector or runtime remains pending.

### T-4: qualify macOS activation and original workflow
- **Blocked by:** T-3
- Run the real macOS activation and workflow.
- Register corpus evidence only after the required acceptance proof.

### T-5: integrate reserved build, release and migration commands
- **Blocked by:** T-3
- Make real commands consume the approved contract.
- Prove build/release disclosure and migration acceptance UX.
- Do not accept stubs or formatter-only success.

P1 remains atomic across producer, consumer and disclosure. P1 and P2 may use separate reviewed PRs. The macOS runtime portion of #588 is not complete until both land and live required surfaces pass.
A coherent launch/admission/disclosure draft may be prepared while #587 is pending. It remains draft/do-not-merge and cannot satisfy full P1 or P2.
The profile-dependent macOS activation need not wait for macOS strict T2c or the complete build/migrate product. It must wait for honest explicit Legacy admission. A development stage cannot satisfy installed acceptance.

### Original owner PR sequence and exit observables

- **P0 — #586 / KEL-78, with KEL-96/KEL-102/KEL-74 review**
  - **Vertical deliverable:** Exact-content spec; single location; unambiguous version/mode/profile matrix; stale unverified wording reconciled
  - **Exit observable:** Approved spec and consistent governing references
- **P1 — #588**
  - **Vertical deliverable:** Parser, boot version, verified host selection, macOS primary receipt, and mandatory runtime/dev plus doctor disclosure on all already-live applicable surfaces, landed together
  - **Exit observable:** Real legacy launch prints forfeiture before app execution; old-host, invalid-input, strict-refusal and no-inference controls pass
- **P2 — #588 with KEL-78 T6/KEL-74 consumers**
  - **Vertical deliverable:** Downstream receipt-to-scoreboard integration and its report tests
  - **Exit observable:** Correct receipt binding and forfeiture; no relabelling/mixed aggregation; existing unverified scoring preserved
- **P3 — #497/#499/#587/#500 owners**
  - **Vertical deliverable:** Hand-authored draw.io artifact, accepted declaration, receipt contract and initially honest corpus cells
  - **Exit observable:** Exact fixture reproducible; unimplemented/unknown cells remain visible
- **P4 — #589**
  - **Vertical deliverable:** Actual macOS activation and workflow evidence; corpus registration only with qualifying evidence
  - **Exit observable:** Per-cell evidence under explicit legacy; unresolved install remains unknown
- **Later independent integration — Build/release and X02 migration owners**
  - **Vertical deliverable:** Real build/release disclosure and migration acceptance UX
  - **Exit observable:** Actual commands consume contract; no stub accepted as completion

P1 is atomic across producer, consumer, and disclosure: do not ship parser acceptance while the runtime
silently ignores the request, or an active legacy mode before its required visible forfeiture exists. P1 and
P2 may be separate reviewed PRs, but the macOS runtime portion of #588 is not complete until both land and
the live required surfaces pass. Neither #588 nor KEL-78 T6 may claim its original full build/reporting
acceptance complete while build remains reserved. Any approved milestone split must be recorded explicitly
on the owning tracker; a runtime milestone is not silent closure of the deferred command requirement.
A coherent launch/admission/disclosure draft can be prepared and tested while #587 is pending, but it
remains draft/do-not-merge and cannot satisfy full P1 or P2. The renderer/profile proof does not qualify
schema 4: facade/profile activation additionally needs the accepted GH #446 consumer and the
cross-combination tests in §4.4. No parser-only, receipt-shaped log, or placeholder collector completes
either boundary.
The profile-dependent macOS activation work need not wait for macOS strict T2c or the complete build/migrate product. It must wait for honest explicit legacy admission. Full first-proof completion cannot claim an install pass from a development stage; KEL-141's qualifying prebuilt artifact remains the install dependency. [S19]
