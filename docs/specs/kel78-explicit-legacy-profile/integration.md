# Integration and verification

Part of: [index](index.md)
Owns: integration and verification.

## 4. Design: Integration and evidence
### 4.6 Authority and security-forfeit matrix
“Preserved” below describes existing framework mechanisms where implemented. Required future controls are explicitly marked pending. Neither wording promises resistance to arbitrary same-user native attacks when OS containment is absent.

| Surface | Strict contract | Explicit legacy contract |
|---|---|---|
| Host-minted identities and authenticated links | Required | Still required; no child-selected identity |
| Guard-before-handler on privileged Keld calls | Required | Still required; identical grants and denial semantics |
| Literal renderer channel grants and navigation generations | Separate required controls under #541/#552/#542; integration pending | Same requirements, with no legacy exemption; not claimed implemented by this amendment |
| Direct filesystem access outside role storage | OS-denied by qualified profile | Keld confinement guarantee forfeited |
| Direct network calls | OS-denied even if broker has a net grant | Keld confinement guarantee forfeited |
| Secrets, keychain/credentials, devices | Qualified OS restrictions | Keld confinement guarantee forfeited; ordinary OS/privacy controls may still deny |
| Host/sibling process control or memory/token access | Qualified hostile-process denial | No Keld guarantee against ambient same-user native access |
| Native syscalls and code loading | Qualified profile plus independent addon policy | No syscall-containment claim; addon/host-plugin approval rules unchanged |
| Inherited privileged handles | Strict allowlist and hostile proof | Existing handle hygiene retained; no new handles granted and no full containment claim |
| Descendants | Equally confined; no escape under qualified contract | Existing enrolled cleanup retained; detached hostile descendants are not claimed contained |
| Crash/restart and generation revocation | Required independently of containment | Same supervisor/guardian mechanisms; qualification remains per tested path |
| Renderer sandbox/CSP/media/navigation policy | Separate security boundary | Unchanged and reported separately |
| Package identity and signature verification | Required for relevant path | Still required; not a claim that ambient native code cannot tamper with user-writable assets |
| Updater authority and state protection | Existing independently qualified rules | No new authority; existing legacy same-user direct-update refusal remains |
| Zero ambient OS authority | Claim permitted only after exact strict qualification | Explicitly forfeited |

Legacy does not grant administrator/root privileges, remove operating-system permissions, turn off macOS privacy controls, or ensure every direct operation succeeds. It removes Keld's strict containment claim for the selected role.

The material application risk is that compromised Bun/dependency/native code can exercise ambient user privileges without taking a Keld broker path. Consequently, a narrow manifest cannot be advertised as a complete restriction on such code in legacy mode.

Canonical required line:

```text
legacy — zero-authority claim forfeited
```

Supporting disclosure:

```text
Keld-mediated privileged calls remain authenticated and permission-checked.
The Bun role is not admitted under Keld's strict OS-containment profile.
```

Do not turn the forfeit into a blanket “security disabled” message. Do not call legacy sandboxed because the renderer is sandboxed. [S1][S13]

### 4.7 Reporting contract
Use the existing typed request/effective-state owners. Reports consume their result; they do not infer it from app identity or clone a state machine.

| Surface | Required behavior | Current availability |
|---|---|---|
| Runtime/dev launch | Source location, requested profile, actual admitted state, forfeiture for legacy | #588 must add real path |
| Doctor text | Validated declaration and whether runtime admission was observed; never claim strict from config alone | Live command, new finding needed |
| Doctor JSON and MCP | Keep existing top-level array of `DoctorFinding`; add security-profile finding, use existing typed error envelope | Existing shape must remain compatible |
| Build | Declare request; attach qualified effective state only when supported by exact artifact/platform evidence; print legacy forfeiture | Reserved today; acceptance remains pending until real build exists |
| Release metadata | Bind declaration/permissions digest, platform/artifact, effective state and matching receipt; renderer separate | KEL-78 T6 integration remains open |
| Compatibility scoreboard | Use `Scoreboard::authority_profile()` and `AuthorityProfile::as_str()`; print legacy forfeiture beside legacy result | Existing scorer, product qualification pending |

Doctor's profile finding is labelled `security_profile`. A valid static Legacy declaration can report `ok:
true` for configuration validity only, with detail that includes the forfeiture and “runtime admission not
performed.” A strict declaration without matching admission/proof must not produce an overall “strict
verified” finding. When proof is unavailable, the finding reports failure to verify with the existing
actionable error structure. With no declaration, the static finding says that no profile was selected, a
default run would be unverified, and runtime admission was not performed. It does not invent an observed
runtime state or silently select legacy.

Do not change the JSON array into a wrapper object or make MCP use a different parser. Renderer sandbox facts are a separate finding, and unknown renderer status stays unknown.

For product evidence, the run receipt binds the selected declaration/policy snapshot, actual effective state,
role generation, artifact/runtime identity, platform/engine, and behavioral evidence. A strict receipt
additionally binds the existing strict ProofIdentity: artifact digest, OS ProfileDigest, OS, archive identity,
and policy generation/freshness or its matching immutable archive reference. The permissions digest never
substitutes for this containment proof. Use the existing corpus receipt owner; do not create a competing
universal ledger or expose link tokens. The KEL-74 record schema stays `keld.compat.evidence/v1`.

**Unresolved collection dependency:** GH #587 owns the product receipt format and trusted collection boundary.
Application stdout/stderr is forwarded into host diagnostics, so an application can print a receipt-shaped
line there. Such a line is not authoritative host evidence. The #587 consumer must distinguish an actual
host-origin admitted result from forged application output and bind it to the exact role/runtime/artifact.
Until that contract and reader exist, do not introduce an unused private receipt structure, label diagnostics
as receipts, emit synthetic successful corpus records, or declare P1 receipt/P2 acceptance complete. This is a
named dependency, not a waiver of the receipt criteria. [S23]

Scoring rules remain:

- Preserve a genuine unverified run as unverified.
- Require explicit declared-legacy receipt provenance for product `legacy_sandbox_off`.
- Reject record labels that disagree with the receipt.
- Do not aggregate mixed authority profiles into one percentage or complete board.
- A homogeneous unverified behavioral board can score under existing rules; it still proves no containment.
- The empty committed-product list currently prevents product percentages. A profile declaration does not register a corpus.
- Preserve the separately governed conformance-harness legacy label; it is not proof of a declared product launch.

A shared formatter test is not proof that the reserved build command works. Keep its existing error until the real build implementation consumes this contract. Never return success from a profile-only build stub. [S3][S9][S10][S11]

### 4.8 Migration and user acceptance
The hand-authored declaration is the operator's explicit selection. No second runtime “accept legacy” flag or token is introduced.

Automated migration must show the exact profile diff and the forfeiture before it writes `"profile":
"legacy"`. Accepting file writes in general is not acceptance of weaker containment. Cancellation, no answer,
unattended defaults, and compatibility detection alone produce no new legacy key. The exact
interactive/noninteractive UX remains #497-owned, but any machine option must name legacy acceptance
specifically.

Preserve existing explicit strict. A failed strict run may offer an explanation of the explicit legacy option, but cannot rewrite policy, retry unsandboxed, or automatically accept the choice. An existing valid explicit legacy declaration may be retained without a redundant prompt; any new authority change must be shown.

Removing legacy does not enable strict. It returns a fresh session to unverified/default behavior. Changing it to strict requires strict admission and its proof. Recompilation creates the corresponding descriptor version. Do not delete application data or reinterpret stored WebView profiles during this change.

A profile edit changes the exact permissions digest. Future frozen-permissions checks must detect it, including removal and semantically equivalent byte changes under their existing byte-identity policy.

The migration analyzer's static findings are not compatibility scores. The planned draw.io proof uses the reviewed hand-authored fixture first; completing the general migration command is not a prerequisite to testing the runtime seam.

### 4.9 Draw.io-specific consumers
The existing first-proof source receipt [S20] pins desktop commit `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`
and webapp commit `24b76c2cbd55d88e354042e8d329a2e4708972bc`. It selects a macOS “configuration plus one edit”
route: replace the close-with-unsaved synchronous message box with the approved awaited asynchronous call
inside the existing async handler. This is a scoped plan with remaining falsifiers, not a current Keld
compatibility result. [S17]

#497/#499 consume:

- Exactly one legacy declaration in the permissions manifest.
- App entry `src/main/electron.js`, declared role environment `DRAWIO_DISABLE_UPDATE=true`, and separately owned single-instance/session facts.
- Literal IPC channel grants from the security/channel owners; no wildcard authority.
- One `electron` package identity resolving to `@keld/electron`, plus the inert updater adapter.
- Updated scripts that no longer launch Electron/electron-builder.

They do not invent `keld.build.ts`, `keld.compat.ts`, bunfig aliases, or `tsconfig paths.electron` for this first proof. The draw.io plan's rule not to seed filesystem grants under legacy is application migration policy; it does not change the guard's general filesystem grammar. [S18]

| Dependent owner | Required draw.io integration |
|---|---|
| [#452](https://github.com/gyldlab/keld/issues/452) | Session facts, legacy paths and declared role environment |
| [#453](https://github.com/gyldlab/keld/issues/453) | Single-instance declaration and host-owned lock |
| [#446](https://github.com/gyldlab/keld/issues/446) | Facade Ready and app-owned first window; compose its schema-2 mode with explicit profile as schema 4 under §4.4 |
| [#491](https://github.com/gyldlab/keld/issues/491) | Inert updater adapter |
| [#541](https://github.com/gyldlab/keld/issues/541), [#552](https://github.com/gyldlab/keld/issues/552), [#542](https://github.com/gyldlab/keld/issues/542) | Permission amendment, exact channel enforcement and host-owned navigation generations; implementation/integration pending |
| [#497](https://github.com/gyldlab/keld/issues/497), [#499](https://github.com/gyldlab/keld/issues/499) | Output/key-owner contract and hand-authored fixture |
| [#587](https://github.com/gyldlab/keld/issues/587), [#500](https://github.com/gyldlab/keld/issues/500), [#589](https://github.com/gyldlab/keld/issues/589) | Product receipts, corpus cells and real-Mac workflow evidence |

The independent legacy-primary fixture may proceed before the renderer bridge exists. Exposing draw.io's bridge must wait for its separately owned channel and generation controls; an unchanged test suite cannot prove missing functionality.

Remaining observed integration risks include sender-URL matching against the application's expected file origin, synchronous screen/theme/locale/window getters, and an experiment that did not itself validate real Electron 44 equivalence. Profile selection does not solve these.

Doctor currently assumes `src/main.ts` for project layout. The draw.io entry is different. Route a focused configured-entry fix through the migration/config owner; a green profile finding must not hide that failure. [S10][S17]



## 5. Boundaries
Original section scope: Boundaries.

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
Original section scope: Implementation PR sequence.

### T-1: Preserve the approved implementation sequence
- **Blocked by:** none

The ordered deliverables and their external prerequisites remain as follows.

| Order | Owner | Vertical deliverable | Exit observable |
|---|---|---|---|
| P0 | #586 / KEL-78, with KEL-96/KEL-102/KEL-74 review | Exact-content spec; single location; unambiguous version/mode/profile matrix; stale unverified wording reconciled | Approved spec and consistent governing references |
| P1 | #588 | Parser, boot version, verified host selection, macOS primary receipt, and mandatory runtime/dev plus doctor disclosure on all already-live applicable surfaces, landed together | Real legacy launch prints forfeiture before app execution; old-host, invalid-input, strict-refusal and no-inference controls pass |
| P2 | #588 with KEL-78 T6/KEL-74 consumers | Downstream receipt-to-scoreboard integration and its report tests | Correct receipt binding and forfeiture; no relabelling/mixed aggregation; existing unverified scoring preserved |
| P3 | #497/#499/#587/#500 owners | Hand-authored draw.io artifact, accepted declaration, receipt contract and initially honest corpus cells | Exact fixture reproducible; unimplemented/unknown cells remain visible |
| P4 | #589 | Actual macOS activation and workflow evidence; corpus registration only with qualifying evidence | Per-cell evidence under explicit legacy; unresolved install remains unknown |
| Later independent integration | Build/release and X02 migration owners | Real build/release disclosure and migration acceptance UX | Actual commands consume contract; no stub accepted as completion |

P1 is atomic across producer, consumer, and disclosure: do not ship parser acceptance while the runtime
silently ignores the request, or an active legacy mode before its required visible forfeiture exists. P1 and
P2 may be separate reviewed PRs, but the macOS runtime portion of #588 is not complete until both land and the
live required surfaces pass. Neither #588 nor KEL-78 T6 may claim its original full build/reporting acceptance
complete while build remains reserved. Any approved milestone split must be recorded explicitly on the owning
tracker; a runtime milestone is not silent closure of the deferred command requirement.

A coherent launch/admission/disclosure draft can be prepared and tested while #587 is pending, but it remains
draft/do-not-merge and cannot satisfy full P1 or P2. The renderer/profile proof does not qualify schema 4:
facade/profile activation additionally needs the accepted GH #446 consumer and the cross-combination tests in
§4.4. No parser-only, receipt-shaped log, or placeholder collector completes either boundary.

The profile-dependent macOS activation work need not wait for macOS strict T2c or the complete build/migrate product. It must wait for honest explicit legacy admission. Full first-proof completion cannot claim an install pass from a development stage; KEL-141's qualifying prebuilt artifact remains the install dependency. [S19]



## 7. Test plan & evidence
Original section scope: Parsing, admission, and negative-control tests.

Failure-first means the intended observable fails on the pre-feature implementation. A negative control is a deliberate, narrowly scoped defect that must make the corrected test fail again. A constant printed label, source inspection, or a mock claiming launch is not a native-runtime oracle.

### 7.1 Parser and compiled-input matrix

- **ID:** P01
- **Input/condition:** Absent profile
- **Required result:** None/Unverified, no legacy request
- **Deliberate negative control:** Default absence to Legacy

- **ID:** P02
- **Input/condition:** Valid legacy and strict strings
- **Required result:** Exact distinct requested states
- **Deliberate negative control:** Swap or collapse enum mapping

- **ID:** P03
- **Input/condition:** Null, bool, number, array, object
- **Required result:** Typed parse rejection
- **Deliberate negative control:** Treat null as None or coerce type

- **ID:** P04
- **Input/condition:** Empty/case/whitespace/unknown strings
- **Required result:** Rejection with accepted values and location
- **Deliberate negative control:** Trim/case-fold or accept unverified

- **ID:** P05
- **Input/condition:** Duplicate profile, same/conflicting values
- **Required result:** Rejection
- **Deliberate negative control:** Last/first value wins

- **ID:** P06
- **Input/condition:** Escaped-equivalent duplicate keys
- **Required result:** Rejection
- **Deliberate negative control:** Compare only raw key spelling

- **ID:** P07
- **Input/condition:** One escaped key/value decoding to accepted spelling
- **Required result:** Valid, exact-byte digest retained
- **Deliberate negative control:** Inconsistent second profile parser

- **ID:** P08
- **Input/condition:** Misplaced security profile
- **Required result:** Location error, no alternate authority source
- **Deliberate negative control:** Accept nested/app-config fallback

- **ID:** P09
- **Input/condition:** Malformed JSONC, unterminated comment, oversized bytes, invalid UTF-8
- **Required result:** Existing specific boundary failures
- **Deliberate negative control:** Bypass common parser/ceiling

- **ID:** P10
- **Input/condition:** Profile changed after staged digest
- **Required result:** Integrity failure before app resources
- **Deliberate negative control:** Reopen by path or hash different bytes

- **ID:** P11
- **Input/condition:** Profile-bearing 3/4 on a schema-1-only reader and a Ready-only 1/2 reader
- **Required result:** Unsupported-version refusal before application marker/PID; distinguish actual old
  artifact from a 1/2 contract model
- **Deliberate negative control:** Downgrade 3→1 or 4→2 automatically

- **ID:** P12
- **Input/condition:** All four §4.4 rows, both explicit values, wrong presence, forbidden renderer/auth,
  duplicate/non-object input, unknown version, and unsupported facade/profile rollout
- **Required result:** Each supported pair selects its exact version/shape; all invalid or unsupported
  combinations refuse before resources
- **Deliberate negative control:** Independently swap the producer's mode mapping, omit the consumer presence
  check, accept a numeric range, or fabricate a renderer

- **ID:** P13
- **Input/condition:** Profile-free schema1 and, after GH #446 qualification, profile-free schema2
- **Required result:** Existing mode and semantics preserved; fresh profile removal maps 3→1 and 4→2 without
  changing window ownership
- **Deliberate negative control:** Require a new declaration unconditionally or turn profile removal into a
  mode change

- **ID:** P14
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

**Test ownership and OS scope:** strict proof permutations and recovery request retention use the existing
pure admission/state-preparer tests with synthetic proof tuples; these make no OS-containment claim. Real
macOS tests prove that an invalid strict attempt does not execute application code or start an unconfined
replacement. A successful native Strict launch followed by proof invalidation/restart is a platform-owner
acceptance test after that platform has qualified Strict. In particular, #588 does not wait for macOS T2c to
run its pure A05 regression and real refusal controls. Real explicit-legacy activation, guard side effects,
immutable policy, and enrolled cleanup remain required for the macOS slice.

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

Use table-driven parser tests, real temporary-file/digest integration, isolated real macOS process tests, and existing shared corpus/scorer tests. Timing tests use existing event/deadline observers, not sleeps. Prove denied side effects and process absence independently of log text. Replay each mutation against its named test and retain the observed red result.

No product/native tests or mutation experiments were run while drafting this specification. The tables are the required implementation acceptance plan, not executed results.


## 9. Perf impact
Original section scope: Performance impact.

One optional field decode, one version/declaration check, and one immutable selected-state value belong on the cold startup path. The existing exact-byte permissions digest is reused. No per-message profile parsing, manifest read, additional runtime, or new reaper is permitted.

No speedup or numeric overhead is claimed. If the integration materially changes launch allocation or process setup, compare the existing attributed macOS startup path using the repository's established budget rules. Strict and legacy behavior must be semantically correct before comparing timings.


## 10. Open questions, blockers & unknowns
Original section scope: Open questions, approved decisions, and handoff.

### 10.1 Prior approval and R1 decisions
Under the repository owner's continuing 2026-10-23 delegation, this revision approves the following target after independent review of the corrected technical content. The new R1 approval, not the old eba60d22 approval, covers the schema correction and the added public admission entry:

1. Root `keld.permissions.jsonc.profile` is the sole authoring location.
2. Only strict/legacy are configurable; absence remains unverified/default and never implies legacy.
3. Schema 2 remains GH #446's no-profile facade contract. Explicit profiles use 3 for renderer boots and 4 for facade boots; profile-free renderer inputs remain 1. Unsupported combinations refuse (§4.4).
4. Strict admission and all independent guard/identity/lifecycle/renderer/native/updater owners remain intact.
5. Legacy explicitly forfeits OS containment and is qualified first for the macOS primary role.
6. KEL-74's current unverified evidence/scoring contract is preserved; stale #586/#588 exclusions are superseded.
7. The staged PR sequence and test matrix govern implementation; build/migrate integration is reported pending until those commands are real.

No further user choice is requested. KEL-78 remains the profile owner; GH #446 remains the facade Ready owner.
The independent review found and resolved the earlier literal-API obligations that contradicted
`admit_uncontained`; AC-6/AC-7 and §4.5 now state the same routing rule. The reviewed content digest and new
delegated approval are published on #1120/KEL-78, with the matrix correction handed to #446/#588. Each
implementation PR still requires independent review of its exact final public contract, permission boundary
and compiled-boot seam. Design approval neither changes the explicit no-merge instruction nor supplies any
implementation or OS evidence.

### 10.2 Verified blockers and first completion checks
| Blocker | Why it matters | Owner and next action | First observable completion |
|---|---|---|---|
| #586 integration pending | Approved design is not yet integrated into main | KEL-78/#586 publish the exact reviewed amendment and reconcile old briefs; preserve do-not-merge | Repository PR and consistent tracker decision receipts; main integration remains separately gated |
| Key currently ignored | Merely editing permissions does not select Legacy/Strict | #588 extend existing parser and actual host preflight atomically | Explicit declaration changes real admitted receipt |
| Old host ignores profile | New strict request could be ignored on unsupported runtime | #588 plus KEL-96/KEL-102 implement 3/4 pairing without repurposing Ready's 2 | Previous host refuses new artifact before app execution |
| Competing schema-2 meanings | Renderer/profile and facade/no-profile cannot share a closed version | #586 corrects its own assignment; #446 keeps Ready ownership; independent review covers §4.4 | Reviewed exact four-row matrix and consistent producer/consumer tests |
| Product receipt collector absent | Forwarded application output can forge a diagnostic receipt lookalike | #587 defines trusted host-origin collection and exact artifact binding, then #588 consumes it | Forged-app-output negative control fails while a real bound-host receipt succeeds |
| Generic admit is not a live proof | Returning Legacy alone proves no launch/auth/guard behavior | #588 wire existing host/session path | Real macOS bound child plus permitted/denied calls |
| macOS strict T2c unqualified | Explicit strict cannot be promised for draw.io today | Existing KEL-78 strict owner continues signed-helper hostile proof separately | Exact artifact/profile archive passes strict checks |
| Build/migrate reserved | Their future output cannot be claimed complete now | Existing command owners consume this contract; use hand-authored fixture first | Real commands execute and report honestly |
| Draw.io dependency graph still open | Profile alone does not provide IPC/window/preload/dialog behavior or pending renderer controls | #499/#589 plus #541/#552/#542 and API owners | Actual Mac workflow with denied-channel/stale-generation controls |
| Install/product denominator incomplete | Development launch cannot establish installed/product compatibility | #589/KEL-141 and corpus owners | Qualifying install evidence and committed denominator |
| Doctor entry assumption | A valid profile does not fix layout validation for draw.io | #497/config owner | Configured src/main/electron.js passes intended layout check |

The specification and delegated decisions are published through the #586 PR and KEL-78/#588 handoffs. Runtime implementation, native qualification, reserved-command integration, and merging remain open. Issue comments and PR records carry execution status; this document defines the approved target contract.

### 10.3 Evidence ledger
All source links below are pinned to the inspected commit. Main advanced from `442f3ebc` during research; the direct comparison to `3503a8aa383427dcf50ccbf6653c4298e7fb8aad` changed only Windows foreign-app admission tests and the KEL-254 spec. The profile/parser/reporting findings are unchanged.


Additional owning links: [GH #588](https://github.com/gyldlab/keld/issues/588), [GH
#499](https://github.com/gyldlab/keld/issues/499), [GH #587](https://github.com/gyldlab/keld/issues/587), [GH
#500](https://github.com/gyldlab/keld/issues/500), [approved vocabulary PR
#612](https://github.com/gyldlab/keld/pull/612), [direct-update legacy
refusal](https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/architecture/06-runtime-and-tooling.md#L1098-L1103).

Decision history: KEL-78 owner comment `99767a07-b061-497d-89c1-2381032bad95`, 2026-10-07, selects unverified evidence option (b). The tracker returned no comment-specific URL; the KEL-78 issue link and the approved merged spec provide the verifiable references.

Structural migration does not close or approve the original open decisions and execution blockers above.

[S10]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/doctor.rs
[S11]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/engineering/compat-scoreboard.md
[S13]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/architecture/03-security.md
[S17]: https://github.com/gyldlab/keld/issues/420#issuecomment-6034980014
[S18]: https://github.com/gyldlab/keld/issues/497#issuecomment-6026217139
[S19]: https://github.com/gyldlab/keld/issues/589
[S1]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/kel78-strict-profile-sandbox.md
[S20]: https://github.com/gyldlab/keld/issues/420#issuecomment-6022061800
[S23]: https://github.com/gyldlab/keld/issues/587#issuecomment-6371500644
[S3]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/gh532-first-proof-evidence-rules.md
[S9]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/verb.rs
