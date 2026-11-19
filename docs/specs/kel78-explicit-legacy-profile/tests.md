# KEL-78 test plan
Part of: [index](index.md)
Owns: section 7

## 7. Test plan & evidence

Failure-first means the intended observable fails on the pre-feature implementation. A negative control is a deliberate, narrowly scoped defect that must make the corrected test fail again. A constant printed label, source inspection, or a mock claiming launch is not a native-runtime oracle.

### 7.1 Parser and compiled-input matrix


- **P01:** Absent profile
  - **Observable:** None/Unverified, no legacy request
  - **Negative control:** Default absence to Legacy

- **P02:** Valid legacy and strict strings
  - **Observable:** Exact distinct requested states
  - **Negative control:** Swap or collapse enum mapping

- **P03:** Null, bool, number, array, object
  - **Observable:** Typed parse rejection
  - **Negative control:** Treat null as None or coerce type

- **P04:** Empty/case/whitespace/unknown strings
  - **Observable:** Rejection with accepted values and location
  - **Negative control:** Trim/case-fold or accept unverified

- **P05:** Duplicate profile, same/conflicting values
  - **Observable:** Rejection
  - **Negative control:** Last/first value wins

- **P06:** Escaped-equivalent duplicate keys
  - **Observable:** Rejection
  - **Negative control:** Compare only raw key spelling

- **P07:** One escaped key/value decoding to accepted spelling
  - **Observable:** Valid, exact-byte digest retained
  - **Negative control:** Inconsistent second profile parser

- **P08:** Misplaced security profile
  - **Observable:** Location error, no alternate authority source
  - **Negative control:** Accept nested/app-config fallback

- **P09:** Malformed JSONC, unterminated comment, oversized bytes, invalid UTF-8
  - **Observable:** Existing specific boundary failures
  - **Negative control:** Bypass common parser/ceiling

- **P10:** Profile changed after staged digest
  - **Observable:** Integrity failure before app resources
  - **Negative control:** Reopen by path or hash different bytes

- **P11:** Profile-bearing 3/4 on a schema-1-only reader and a Ready-only 1/2 reader
  - **Observable:** Unsupported-version refusal before application marker/PID; distinguish actual old artifact from a 1/2 contract model
  - **Negative control:** Downgrade 3→1 or 4→2 automatically

- **P12:** All four §4.4 rows, both explicit values, wrong presence, forbidden renderer/auth, duplicate/non-object input, unknown version, and unsupported facade/profile rollout
  - **Observable:** Each supported pair selects its exact version/shape; all invalid or unsupported combinations refuse before resources
  - **Negative control:** Independently swap the producer's mode mapping, omit the consumer presence check, accept a numeric range, or fabricate a renderer

- **P13:** Profile-free schema1 and, after GH #446 qualification, profile-free schema2
  - **Observable:** Existing mode and semantics preserved; fresh profile removal maps 3→1 and 4→2 without changing window ownership
  - **Negative control:** Require a new declaration unconditionally or turn profile removal into a mode change

- **P14:** Existing missing-file dev fallback vs unreadable file
  - **Observable:** Only ENOENT uses old all-deny path
  - **Negative control:** Treat every read failure as absence

Parser fixtures include unchanged unrelated manifest grants and unknown fields to prove the profile addition does not silently tighten or loosen unrelated grammar.

### 7.2 Runtime and security matrix


- **A01:** Explicit legacy on real macOS primary path
  - **Observable:** Host observed Legacy request; real bound child; admitted receipt and guarded call
  - **Negative control:** Print legacy but omit actual admission wiring

- **A02:** No key with Electron sandbox false/appSandbox off
  - **Observable:** Unverified receipt, never declared legacy
  - **Negative control:** Read Electron option as selection

- **A03:** App name, fuse, environment, CLI flag, failed import
  - **Observable:** No change to request/effective state
  - **Negative control:** Add one hidden fallback branch

- **A04:** Strict missing/stale/mismatched/incomplete/wrong-layer proof; missing primitive
  - **Observable:** Existing exact typed refusal; no app execution and no unconfined replacement
  - **Negative control:** Replace error with Legacy/Unverified start

- **A05:** Strict restart after proof becomes invalid
  - **Observable:** Request remains Strict; successor application marker absent
  - **Negative control:** Reset request on recovery

- **A06:** Legacy denied broker operation plus allowed control
  - **Observable:** Denied handler side effect absent; permitted operation succeeds
  - **Negative control:** Skip guard when Legacy

- **A07:** Forged/foreign/stale link and generation
  - **Observable:** Existing rejection; healthy next operation succeeds
  - **Negative control:** Trust payload profile/principal or stale link

- **A08:** Source manifest changes while host is alive
  - **Observable:** Current generation/recovery use original digest and request
  - **Negative control:** Live reread or mutable profile cache

- **A09:** Remove legacy, retire host, start fresh
  - **Observable:** No cached legacy receipt; correct newly compiled schema
  - **Negative control:** Reuse prior session state

- **A10:** Change legacy to strict without proof
  - **Observable:** Strict refusal; no fallback
  - **Negative control:** Treat prior legacy acceptance as permission

- **A11:** Diagnostic/alternate entry with explicit profile
  - **Observable:** Refusal before app execution
  - **Negative control:** Drop permissions and call unprivileged spawn

- **A12:** Existing guardian/enrolled cleanup and generation revocation
  - **Observable:** Retained mechanism tests pass, no duplicate reaper
  - **Negative control:** Bypass shared owner on legacy branch

- **A13:** Unsupported target/role combination, including a compiler with fixed fallback permissions
  - **Observable:** Actionable refusal before execution; explicit profile never discarded
  - **Negative control:** Claim all platforms from macOS pass or replace declared policy with empty bytes

- **A14:** Direct safe temp-file/socket probe on a legacy fixture
  - **Observable:** Record actual ambient outcome separately from broker denial
  - **Negative control:** Count a broker/shim deny as OS containment

- **A15:** Renderer channels/generations and native-addon/updater policies
  - **Observable:** Existing regressions preserved; #541/#552/#542 integration controls required before exposing the draw.io bridge
  - **Negative control:** Treat a pending control as passed, or skip it for legacy

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


- **R01:** Legacy runtime/doctor/reporter
  - **Observable:** Canonical forfeit present
  - **Negative control:** Remove formatter call

- **R02:** Strict configuration without admission
  - **Observable:** Requested strict; admission not performed/not verified; no effective runtime state asserted
  - **Negative control:** Use raw config as effective state or invent an Unverified run

- **R03:** Doctor JSON/MCP
  - **Observable:** Same finding array and actionable errors
  - **Negative control:** New wrapper or divergent parser

- **R04:** Unverified product receipt
  - **Observable:** Record preserved as unverified
  - **Negative control:** Drop run or label legacy/strict

- **R05:** Legacy product record without explicit admitted receipt, including an application-printed receipt lookalike
  - **Observable:** Rejected by the GH #587 trusted collector/reader
  - **Negative control:** Trust record's self-label or a line from forwarded app output

- **R06:** Homogeneous unverified board
  - **Observable:** Existing behavior and label retained
  - **Negative control:** Blanket score exclusion or promotion

- **R07:** Mixed authority board
  - **Observable:** No percentage, not complete, no single profile
  - **Negative control:** Select first record's label

- **R08:** Reserved build today
  - **Observable:** Existing typed failure
  - **Negative control:** Return success from formatter stub

- **R09:** Future real build/release
  - **Observable:** Correct declaration and exact evidence-qualified state
  - **Negative control:** Attach another artifact's proof

- **M01:** Analyze/default/cancelled migration
  - **Observable:** No new legacy declaration
  - **Negative control:** Infer acceptance from app detection

- **M02:** Generic write permission
  - **Observable:** Does not alone authorize legacy selection
  - **Negative control:** Treat --write as security acceptance

- **M03:** Accepted legacy diff
  - **Observable:** Exactly one root declaration; no new grants
  - **Negative control:** Add duplicate source or wildcard

- **M04:** Existing explicit strict
  - **Observable:** Preserved or change separately accepted
  - **Negative control:** Auto-rewrite after strict failure

- **M05:** Configured draw.io entry
  - **Observable:** Honest doctor result for configured path
  - **Negative control:** Hardcode src/main.ts

- **M06:** Dev-stage launch used for install cell
  - **Observable:** No install pass
  - **Negative control:** Relabel dev activation as installation

- **M07:** Product corpus not registered/denominator incomplete
  - **Observable:** No product percentage
  - **Negative control:** Register or complete by declaration alone

Use table-driven parser tests, real temporary-file/digest integration, isolated real macOS process tests, and existing shared corpus/scorer tests. Timing tests use existing event/deadline observers, not sleeps. Prove denied side effects and process absence independently of log text. Replay each mutation against its named test and retain the observed red result.

No product/native tests or mutation experiments were run while drafting this specification. The tables are the required implementation acceptance plan, not executed results.
