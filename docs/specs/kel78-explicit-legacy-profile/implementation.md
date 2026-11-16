# Legacy integration tasks and verification
Part of: [index](index.md)
Owns: Boundaries, implementation sequence, regressions

## 5. Boundaries
The approved design below is the implementation boundary. Publishing this specification does not implement it; #588 still requires an integrated specification base, a winning implementation claim, and the independent gates below.

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
### T-1: P0 legacy-profile delivery
- **Blocked by:** none
- **Original owner:** #586 / KEL-78, with KEL-96/KEL-102/KEL-74 review
- **Work:** Exact-content spec; single location; unambiguous version/mode/profile matrix; stale unverified wording reconciled
- **Exit observable:** Approved spec and consistent governing references

### T-2: P1 legacy-profile delivery
- **Blocked by:** T-1
- **Original owner:** #588
- **Work:** Parser, boot version, verified host selection, macOS primary receipt, and mandatory runtime/dev plus doctor disclosure on all already-live applicable surfaces, landed together
- **Exit observable:** Real legacy launch prints forfeiture before app execution; old-host, invalid-input, strict-refusal and no-inference controls pass

### T-3: P2 legacy-profile delivery
- **Blocked by:** T-2
- **Original owner:** #588 with KEL-78 T6/KEL-74 consumers
- **Work:** Downstream receipt-to-scoreboard integration and its report tests
- **Exit observable:** Correct receipt binding and forfeiture; no relabelling/mixed aggregation; existing unverified scoring preserved

### T-4: P3 legacy-profile delivery
- **Blocked by:** T-3
- **Original owner:** #497/#499/#587/#500 owners
- **Work:** Hand-authored draw.io artifact, accepted declaration, receipt contract and initially honest corpus cells
- **Exit observable:** Exact fixture reproducible; unimplemented/unknown cells remain visible

### T-5: P4 legacy-profile delivery
- **Blocked by:** T-4
- **Original owner:** #589
- **Work:** Actual macOS activation and workflow evidence; corpus registration only with qualifying evidence
- **Exit observable:** Per-cell evidence under explicit legacy; unresolved install remains unknown

### T-6: Later independent integration legacy-profile delivery
- **Blocked by:** T-5
- **Original owner:** Build/release and X02 migration owners
- **Work:** Real build/release disclosure and migration acceptance UX
- **Exit observable:** Actual commands consume contract; no stub accepted as completion


P1 is atomic across producer, consumer, and disclosure: do not ship parser acceptance while the runtime silently ignores the request, or an
  active legacy mode before its required visible forfeiture exists. P1 and P2 may be separate reviewed PRs, but the macOS runtime portion of #588
  is not complete until both land and the live required surfaces pass. Neither #588 nor KEL-78 T6 may claim its original full build/reporting
  acceptance complete while build remains reserved. Any approved milestone split must be recorded explicitly on the owning tracker; a runtime
  milestone is not silent closure of the deferred command requirement.

A coherent launch/admission/disclosure draft can be prepared and tested while #587 is pending, but it remains draft/do-not-merge and cannot
  satisfy full P1 or P2. The renderer/profile proof does not qualify schema 4: facade/profile activation additionally needs the accepted GH #446
  consumer and the cross-combination tests in §4.4. No parser-only, receipt-shaped log, or placeholder collector completes either boundary.

The profile-dependent macOS activation work need not wait for macOS strict T2c or the complete build/migrate product. It must wait for honest explicit legacy admission. Full first-proof completion cannot claim an install pass from a development stage; KEL-141's qualifying prebuilt artifact remains the install dependency. [S19]

## 7. Test plan & evidence
Failure-first means the intended observable fails on the pre-feature implementation. A negative control is a deliberate, narrowly scoped defect that must make the corrected test fail again. A constant printed label, source inspection, or a mock claiming launch is not a native-runtime oracle.

### 7.1 Parser and compiled-input matrix

- **P01**
  - **Input/condition:** Absent profile
  - **Required result:** None/Unverified, no legacy request
  - **Deliberate negative control:** Default absence to Legacy
- **P02**
  - **Input/condition:** Valid legacy and strict strings
  - **Required result:** Exact distinct requested states
  - **Deliberate negative control:** Swap or collapse enum mapping
- **P03**
  - **Input/condition:** Null, bool, number, array, object
  - **Required result:** Typed parse rejection
  - **Deliberate negative control:** Treat null as None or coerce type
- **P04**
  - **Input/condition:** Empty/case/whitespace/unknown strings
  - **Required result:** Rejection with accepted values and location
  - **Deliberate negative control:** Trim/case-fold or accept unverified
- **P05**
  - **Input/condition:** Duplicate profile, same/conflicting values
  - **Required result:** Rejection
  - **Deliberate negative control:** Last/first value wins
- **P06**
  - **Input/condition:** Escaped-equivalent duplicate keys
  - **Required result:** Rejection
  - **Deliberate negative control:** Compare only raw key spelling
- **P07**
  - **Input/condition:** One escaped key/value decoding to accepted spelling
  - **Required result:** Valid, exact-byte digest retained
  - **Deliberate negative control:** Inconsistent second profile parser
- **P08**
  - **Input/condition:** Misplaced security profile
  - **Required result:** Location error, no alternate authority source
  - **Deliberate negative control:** Accept nested/app-config fallback
- **P09**
  - **Input/condition:** Malformed JSONC, unterminated comment, oversized bytes, invalid UTF-8
  - **Required result:** Existing specific boundary failures
  - **Deliberate negative control:** Bypass common parser/ceiling
- **P10**
  - **Input/condition:** Profile changed after staged digest
  - **Required result:** Integrity failure before app resources
  - **Deliberate negative control:** Reopen by path or hash different bytes
- **P11**
  - **Input/condition:** Profile-bearing 3/4 on a schema-1-only reader and a Ready-only 1/2 reader
  - **Required result:** Unsupported-version refusal before application marker/PID; distinguish actual old artifact from a 1/2
    contract model
  - **Deliberate negative control:** Downgrade 3→1 or 4→2 automatically
- **P12**
  - **Input/condition:** All four §4.4 rows, both explicit values, wrong presence, forbidden renderer/auth, duplicate/non-object
    input, unknown version, and unsupported facade/profile rollout
  - **Required result:** Each supported pair selects its exact version/shape; all invalid or unsupported combinations refuse
    before resources
  - **Deliberate negative control:** Independently swap the producer's mode mapping, omit the consumer presence check, accept a
    numeric range, or fabricate a renderer
- **P13**
  - **Input/condition:** Profile-free schema1 and, after GH #446 qualification, profile-free schema2
  - **Required result:** Existing mode and semantics preserved; fresh profile removal maps 3→1 and 4→2 without changing window
    ownership
  - **Deliberate negative control:** Require a new declaration unconditionally or turn profile removal into a mode change
- **P14**
  - **Input/condition:** Existing missing-file dev fallback vs unreadable file
  - **Required result:** Only ENOENT uses old all-deny path
  - **Deliberate negative control:** Treat every read failure as absence


Parser fixtures include unchanged unrelated manifest grants and unknown fields to prove the profile addition does not silently tighten or loosen unrelated grammar.

### 7.2 Runtime and security matrix

| ID | Scenario | Independent observable | Deliberate negative control |
|---|---|---|---|
| A01 | Explicit legacy on real macOS primary path | Host observed Legacy request; real bound child; admitted receipt and guarded call | Print legacy but omit actual admission wiring |
| A02 | No key with Electron sandbox false/appSandbox off | Unverified receipt, never declared legacy | Read Electron option as selection |
| A03 | App name, fuse, environment, CLI flag, failed import | No change to request/effective state | Add one hidden fallback branch |
| A04 | Strict missing/stale/mismatched/incomplete/wrong-layer proof; missing primitive | Existing exact typed refusal; no app execution and no unconfined replacement | Replace error with Legacy/Unverified start |
| A05 | Strict restart after proof becomes invalid | Request remains Strict; successor application marker absent | Reset request on recovery |
| A06 | Legacy denied broker operation plus allowed control | Denied handler side effect absent; permitted operation succeeds | Skip guard when Legacy |
| A07 | Forged/foreign/stale link and generation | Existing rejection; healthy next operation succeeds | Trust payload profile/principal or stale link |
| A08 | Source manifest changes while host is alive | Current generation/recovery use original digest and request | Live reread or mutable profile cache |
| A09 | Remove legacy, retire host, start fresh | No cached legacy receipt; correct newly compiled schema | Reuse prior session state |
| A10 | Change legacy to strict without proof | Strict refusal; no fallback | Treat prior legacy acceptance as permission |
| A11 | Diagnostic/alternate entry with explicit profile | Refusal before app execution | Drop permissions and call unprivileged spawn |
| A12 | Existing guardian/enrolled cleanup and generation revocation | Retained mechanism tests pass, no duplicate reaper | Bypass shared owner on legacy branch |
| A13 | Unsupported target/role combination, including a compiler with fixed fallback permissions | Actionable refusal before execution; explicit profile never discarded | Claim all platforms from macOS pass or replace declared policy with empty bytes |
| A14 | Direct safe temp-file/socket probe on a legacy fixture | Record actual ambient outcome separately from broker denial | Count a broker/shim deny as OS containment |
| A15 | Renderer channels/generations and native-addon/updater policies | Existing regressions preserved; #541/#552/#542 integration controls required before exposing the draw.io bridge | Treat a pending control as passed, or skip it for legacy |

A14 runs only against disposable test-owned files, local endpoints, and child processes. It establishes disclosure accuracy, not that every ambient operation must succeed. Do not read real secrets or infer containment from an environmental denial.

Strict variants retain the full current negative matrix: artifact, profile digest, OS, archive identity, freshness, catalog completeness, and OS-oracle layer. A JS shim, guard denial, cleanup success, or resource-limit hit cannot satisfy strict OS evidence.

**Test ownership and OS scope:** strict proof permutations and recovery request retention use the existing pure admission/state-preparer tests
  with synthetic proof tuples; these make no OS-containment claim. Real macOS tests prove that an invalid strict attempt does not execute
  application code or start an unconfined replacement. A successful native Strict launch followed by proof invalidation/restart is a
  platform-owner acceptance test after that platform has qualified Strict. In particular, #588 does not wait for macOS T2c to run its pure A05
  regression and real refusal controls. Real explicit-legacy activation, guard side effects, immutable policy, and enrolled cleanup remain
  required for the macOS slice.

### 7.3 Reporting, evidence, and migration matrix

| ID | Scenario | Required result | Deliberate negative control |
|---|---|---|---|
| R01 | Legacy runtime/doctor/reporter | Canonical forfeit present | Remove formatter call |
| R02 | Strict configuration without admission | Requested strict; admission not performed/not verified; no effective runtime state asserted | Use raw config as effective state or invent an Unverified run |
| R03 | Doctor JSON/MCP | Same finding array and actionable errors | New wrapper or divergent parser |
| R04 | Unverified product receipt | Record preserved as unverified | Drop run or label legacy/strict |
| R05 | Legacy product record without explicit admitted receipt, including an application-printed receipt lookalike | Rejected by the GH #587 trusted collector/reader | Trust record's self-label or a line from forwarded app output |
| R06 | Homogeneous unverified board | Existing behavior and label retained | Blanket score exclusion or promotion |
| R07 | Mixed authority board | No percentage, not complete, no single profile | Select first record's label |
| R08 | Reserved build today | Existing typed failure | Return success from formatter stub |
| R09 | Future real build/release | Correct declaration and exact evidence-qualified state | Attach another artifact's proof |
| M01 | Analyze/default/cancelled migration | No new legacy declaration | Infer acceptance from app detection |
| M02 | Generic write permission | Does not alone authorize legacy selection | Treat --write as security acceptance |
| M03 | Accepted legacy diff | Exactly one root declaration; no new grants | Add duplicate source or wildcard |
| M04 | Existing explicit strict | Preserved or change separately accepted | Auto-rewrite after strict failure |
| M05 | Configured draw.io entry | Honest doctor result for configured path | Hardcode src/main.ts |
| M06 | Dev-stage launch used for install cell | No install pass | Relabel dev activation as installation |
| M07 | Product corpus not registered/denominator incomplete | No product percentage | Register or complete by declaration alone |

Use table-driven parser tests, real temporary-file/digest integration, isolated real macOS process tests, and existing shared corpus/scorer
  tests. Timing tests use existing event/deadline observers, not sleeps. Prove denied side effects and process absence independently of log text.
  Replay each mutation against its named test and retain the observed red result.

No product/native tests or mutation experiments were run while drafting this specification. The tables are the required implementation acceptance plan, not executed results.


[S19]: https://github.com/gyldlab/keld/issues/589
