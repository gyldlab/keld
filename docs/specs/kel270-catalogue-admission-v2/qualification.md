# Qualification, performance, and blockers

Part of: [index](index.md)
Owns: Definition and native evidence, performance limits, and open release or approval blockers.

## 7. Test plan & evidence: Identity

### E-1: Definition-only vectors

- **Run:** `python3 -B docs/engineering/evidence/kel270-catalogue-admission-v2/verify_identity_vectors.py --check`.
- **n:** 94 definition checks. The exact output digest is in the compact review receipt.
- **Environment:** Python 3.10 or newer with the test-only `cryptography` package.
  Hosted CI installs version 46.0.5 in an isolated virtual environment.
- **Limits:** This validates candidate bytes and signature domains. It does not execute product codecs, Authenticode, Windows storage, or migration.

The vector files contain synthetic public fixture keys and identities. They commission no real authority.
The executable checks byte bounds, closed fields, canonical OIDs, cross-version refusal, signatures, and single-field tampering.
Native AC-2 and AC-4 remain NOT_RUN. Require genuine changed-SPKI and joined browser/floor/LKG observations.

## 7. Test plan & evidence: Metadata

### E-20: Source comparison

- **Run:** Re-fetched S1 and S2 through the GitHub connector on 2026-11-15.
- **n:** One selected client revision and one normative revision.
- **Environment:** Read-only source inspection.
- **Limits:** No patched client, protocol test, or native execution occurred.

### E-21: Current-documentation receipt

- **Run:** Context7 resolved `/awslabs/tough` and queried persistence, reset, and equal-timestamp behavior.
- **n:** One narrow query, retrieved 2026-11-15.
- **Environment:** Discovery plus confirmation against S1 and S2.
- **Limits:** Context7 examples use develop. They did not decide the contract.

### E-22: Registry and build boundary

- **Run:** Historical native `cargo info tough@0.24.0 --registry crates-io`, session 16199.
- **n:** One package lookup, documented in S5's publication receipt.
- **Environment:** DESKTOP-3FMM79E, prior execution turn.
- **Limits:** This was not a build. This turn has no Rust compiler in its local Linux PATH.

Every AC above remains NOT_RUN. Definition review and golden-record checks do not change that status. A patch must make each negative control fail at its intended predicate. A later restored pass must use the same exact source.

## 7. Test plan & evidence: Windows admission

### E-40: Exact source inspection
- **Run:** GitHub file reads at the index source pin and current PR metadata on 2026-11-15.
- **n:** One current PR head and the listed owner sources.
- **Environment:** Remote GitHub source inspection. No Windows execution occurred.
- **Limits:** Source inspection proves the cited code shape. It does not prove native runtime behavior.

### E-41: Native acceptance
- **Run:** Not run. Execute AC-40 through AC-46 after implementation under the approved specification.
- **n:** Zero native runs in this definition task.
- **Environment:** Required real Windows supported modes and signed artifact set.
- **Limits:** No native admission, installed-product, crash, or power-loss qualification is claimed.

All rows below are NOT_RUN. Source inspection is not native test evidence. Each row requires a working positive, one targeted fault, a failing mutant, and a restored pass. Keep unrelated signature and policy layers valid so the intended predicate is reachable.

| Case | Single fault | Independent oracle |
|---|---|---|
| Offline preflight | Remove cache-only flag with an absent intermediate | Independent AIA endpoint and OS telemetry detect forbidden retrieval |
| Actual worker | Dispatch admitted intent to an impersonating worker | OS token facts and zero protected creations |
| Token errors | Return access denied instead of ERROR_NO_TOKEN | Exact refusal, no fallback token |
| Restricted token | Remove privilege without restricting SID | TokenHasRestrictions refusal despite IsTokenRestricted false |
| UAC/profile | Use elevated, filtered, low, high, service, AppContainer, or UIAccess token | Separate native result for each cell |
| Folder origin | Substitute environment or request destination | Writes remain absent at injected location |
| Managed discovery | Present valid managed provenance with valid direct candidate | Zero feed requests and no direct-writer handle |
| Established loss | Remove generation while retaining anchor | CorruptOrMissing, no embedded-root bootstrap |
| Enrollment crash | Kill after every anchor/generation publication boundary | Pending or matching established state, never false first use |
| Stale permit | Advance generation before admission lock succeeds | Old permit cannot create any installation-specific parent |
| Expiry wait | Hold lease past monotonic deadline | Refusal before first creation |
| Source replacement | Attempt rename, hardlink, reparse, or writable competing open | OS refusal or identity refusal, original object consumed |
| Archive path reopen | Replace the path after validation with equal-sized foreign bytes | Retained verified handle supplies every copied byte |
| Ancestor swap | Replace any retained destination ancestor | OS refusal or identity refusal, no foreign write |
| Concurrent installers | Synchronize two valid requests at first creation | One committed baseline, loser cannot repair or reset |
| Crash cuts | Kill worker at each listed baseline cut | No false receipt and exact protected restart classification |
| Lost response | Kill after committed provenance before response | Readback finds one committed installation, no duplicate seed |
| Joined migration | Changed SPKI and U key, existing browser data, failed candidate | Correct profile, provenance, retained floor and authorized LKG together |

Use subprocess barriers and OS-visible observations, not sleeps or log messages alone. Native logs record actual SID/token facts, file identities, exact source/artifact hashes, and child exit states. Do not publish a blanket refusal as proof that a deeper check works.

Official references retrieved 2026-11-15:

- [WINTRUST_DATA](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/ns-wintrust-wintrust_data): "Prevents revocation checks over the network." Its code-signature guidance requires `WTD_CACHE_ONLY_URL_RETRIEVAL`; `WTD_REVOKE_NONE` alone is insufficient.

- [WinVerifyTrust](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-winverifytrust): "No other value besides zero should be considered a successful return."
- [WINTRUST_FILE_INFO](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/ns-wintrust-wintrust_file_info): "This parameter cannot be NULL." The page refers to `pcwszFilePath`; `hFile` accepts an open readable file.
- [OpenThreadToken](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-openthreadtoken): "TRUE if the access check is to be made against the process-level security context."
- [Token information classes](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-token_information_class): "nonzero if the token has ever been filtered." `TokenIsRestricted` is reserved.
- [IsTokenRestricted](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-istokenrestricted): "checks only for the list of restricting SIDs."
- [SHGetKnownFolderPath](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath): "the function requests the known folder for the current user." This applies when `hToken` is NULL.
- [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew): "Delete access allows both delete and rename operations." Share flags remain relevant for every retained object.

Context7 discovery used `/websites/learn_microsoft_en-us_windows_win32_api` on 2026-11-15. The query covered current-user known folders and query-only thread-token behavior.
Context7 returned related known-folder APIs and OpenThreadToken. Direct Microsoft documentation confirmed the selected APIs.

## 7. Test plan & evidence: Recovery

### E-60: Primary-source recovery boundary

- **Run:** TUF 1.0.37 sections 1.4, 2.1.1, 5.2, and 6.1, retrieved 2026-11-15.
- **n:** Four normative sections.
- **Environment:** Primary-source review against the product and research pins cited above.
- **Limits:** The source explains trust replacement. It does not approve this new recovery contract or prove native behavior.

Microsoft [Artifact Signing certificate management](https://learn.microsoft.com/en-us/azure/artifact-signing/concept-certificate-management)
(retrieved 2026-11-15) states that leaf signing certificates renew daily,
but each commissioned certificate profile has a durable, unique EKU OID.
The recovery-only profile OID is therefore the durable installed signing
identity, not its short-lived leaf SPKI. Provider documentation does not
prove credential custody, native verifier correctness or real signer
commissioning; those remain separate T-81/AC-66 gates.

### E-61: Recovery implementation qualification

- **Run:** NOT_RUN. Execute AC-60 through AC-68 after implementation and authority commissioning.
- **n:** Zero executed native cases.
- **Environment:** Required Windows x64 direct per-user installation with genuine independent recovery signer evidence.
- **Limits:** No implementation, process-crash, physical-power-loss, compromise-cleanup, or real browser migration pass is claimed.

### E-62: Recovery requested-change negative-control plan

- **Run:** NOT_RUN. This is the follow-up definition plan for PR #1541's three blocking review findings.
- **n:** Three independent control groups, each with a positive and targeted refusal paths.
- **Environment:** Future signed native Windows x64 recovery qualification, not this documentation workspace.
- **Limits:** Proposed tests do not prove product behavior. The 19 original groups and 110 entries remain unchanged.

| Group | Positive | Single-fault refusal and independent oracle |
|---|---|---|
| R1 / AC-66 | Genuinely independent recovery signer accepted under pinned durable profile/issuer, including a renewed leaf with a different SPKI | Ordinary application signer, recovery signer on an ordinary candidate, or a dual-profile leaf with valid Authenticode and directive must fail the role-specific signer predicate; record zero trust mutations |
| R2 / AC-67 | Crash after `prepared` and pointer, expire directive, persist/read `aborted`, retire exact unreferenced successor, clear pointer, then confirm fresh matching attempt | Expired original, mismatched new directive, and crashes on each abort/retirement/pointer edge refuse; read pointer, journal, immutable successor filename, epoch, provenance, app current/LKG independently |
| R3 / AC-68 | Authenticated loss-only root/U1 replacement with otherwise identical repository policy | Mutate recovery digest, recovery signer, catalogue keys, metadata/targets URLs, discovery digest, publisher digest, or root version separately; compare protected current trust before/after |

Keep Windows signature validity, source-state integrity, and unrelated checks valid in each negative control.
The review addenda are new planned coverage, not retroactive passes for any of the 110 product cases.

## 7. Test plan & evidence: Integration

The fixture and review files are in `docs/engineering/evidence/kel270-catalogue-admission-v2/`.

### E-80: Definition and structure checks

- **Run:** `just spec-evidence` and `just spec-structure` in the owning repository.
- **n:** 94 identity cases, nine evidence-capacity cases, and two isolated
  stale-evidence negative controls. The compact receipt pins both report digests.
- **Environment:** The source tree is not modified. Hosted CI regenerates
  synthetic JSON in runner scratch and logs the SHA-256 values. Immutable
  full historical outputs remain in the pinned research archive.
- **Limits:** No product codec, patched client, native signature, crash, power-loss, or release qualification follows from these checks.

### E-81: Independent specification review

- **Run:** Isolated review of the exact snapshot, one lens per reviewer, with independent finding refutation.
- **n:** The review receipt records completed lenses and findings. Pending reviews grant no evidence.
- **Environment:** Exact file hashes and source pins in `review-receipt.json`.
- **Limits:** Design review does not prove implementation, native behavior, or human approval.

### E-82: Original acceptance coverage

- **Run:** NOT_RUN. The preserved matrix defines cases, mutants, and independent oracles for later implementation.
- **n:** 19 groups and 110 planned case entries.
- **Environment:** Original research source plus this specification's exact owner contracts.
- **Limits:** These are planned cases. Count is not coverage evidence or a test pass.

| Group | Owner contract | Implementation tasks |
|---|---|---|
| A01 | Acquisition and independent authentication | T-20, T-22, T-41, T-42 |
| A02 | Literal catalogue signature | T-1, T-22 |
| A03 | Closed catalogue parser | T-1 |
| A04 | Metadata and catalogue role separation | T-20, T-1 |
| A05 | Complete identity and package binding | T-1, T-2, T-41, T-42 |
| A06 | Metadata bounds, time, and admission | T-21, T-22, T-23 |
| A07 | Root progression | T-20, T-21 |
| A08 | Root recovery and durable state | T-20, T-21, T-24, T-62 |
| A09 | Installed update-key authority | T-2, T-22, T-61 |
| A10 | Stable publisher and changed SPKI | T-1, T-2, T-81 |
| A11 | Actual worker token | T-40 |
| A12 | Owner discovery and admitted root | T-40, T-41, T-42 |
| A13 | Retained source and object replacement | T-41, T-42 |
| A14 | Single initializer and concurrency | T-42, T-60 |
| A15 | Commit and interruption | T-42, T-60 |
| A16 | Floor, finite LKG, and recovery | T-2, T-21, T-62 |
| A17 | Deployment boundary | T-42, T-81 |
| A18 | Installed signed behavior | T-2, T-43, T-81 |
| A19 | Joined identity, browser, floor, and LKG | T-2, T-24, T-62 |

Each added AC requires its own discriminating test. A prior research case cannot silently substitute a different oracle.
Use actual OS observations for OS claims. Keep physical power loss separate from process termination.
The new D-9 evidence-capacity check is planned separately from the original
110 product cases. Require a genuine U1!=U2 signed transition with
distinct app/recovery issuer cohorts and the complete 20-object set.
Prove a 25th distinct object and over-limit aggregate bytes refuse
before journal publication. No synthetic count can qualify the native
retained-handle and filesystem acceptance.
The definition-only helper
`python3 -B docs/engineering/evidence/kel270-catalogue-admission-v2/verify_evidence_capacity.py --check`
checks 20 required distinct objects, byte caps, duplicate references and
over-limit mutants. The detailed output is regenerated in CI scratch.
The research archive pins the earlier nine-case output.

## 9. Perf impact: Identity

No measured runtime result exists. The new parsing and hashing remain on cold installation or update paths.
Measure the approved native path against the same signed baseline. Apply the existing architecture performance budget.

## 9. Perf impact: Metadata

No performance improvement is claimed. The metadata profile bounds each object, refresh, request, and retained state operation. Reuse immutable blobs to avoid copying every role on each root transition. Garbage collection cannot delete the current generation, its referenced blobs, or pending recovery evidence.
Keep at most three generation files: current, its immediate predecessor, and one prepared successor. Retain every blob referenced by those files. A root-reset intent stays in current until completion. Delete only unreferenced objects after successful readback, under the store lease.
Ordinary orphan retirement requires every D-24 proof and an absent D-68
pending pointer. Manual recovery cleanup requires an authenticated terminal
`aborted` record under its still-matching pointer. Do not substitute one
rule for the other. If either cleanup fails, refuse new mutation and
preserve evidence for the KEL-53 operator. Never delete enrollment,
restore older current, or permit two prepared successors. Measure native
I/O and cold-start overhead before claiming acceptable latency.

## 9. Perf impact: Windows admission

The cold installation path adds bounded token queries and retained handles. No throughput or latency improvement is claimed. Metadata deadlines and payload bounds remain with D-METADATA.

## 9. Perf impact: Recovery

Manual recovery is cold and user initiated. It starts no persistent service.
One request and confirmation retain the metadata part's deadlines and byte bounds.
UNKNOWN: Native latency and filesystem durability cost require measurement during qualification.

## 9. Perf impact: Integration

The specification itself changes no runtime behavior.
The implementation adds bounded cold acquisition and recovery work.
Measure native overhead against the same signed baseline before making a performance claim.

## 10. Open questions, blockers & unknowns: Identity

### B-1: Actual publisher authority is not commissioned

- **Status:** open
- **Owner:** Release/security with KEL-135 and Guard.
- **Impact:** Production issuer lists, profile OID, and native changed-SPKI qualification cannot pass.
- **Check:** Record the actual qualified Public Trust profile, chain policy, and two genuine different-SPKI artifacts.

### B-2: The legacy bridge is not qualified

- **Status:** open
- **Owner:** KEL-135, KEL-254, and KEL-53.
- **Impact:** The v1-to-v2 support cell remains unavailable.
- **Check:** Verify the original signer/U1 bridge, then pass joined AC-4 on retained legacy data.

### U-1: Product and native evidence

UNKNOWN: Product codecs, provider extraction, actual migration, and power-loss recovery have not run for this draft.
Definition vectors cannot close those gates.

Current-documentation receipt: Microsoft certificate management, retrieved 2026-11-15, confirms profile-specific identity and recreation behavior.
Primary source: https://learn.microsoft.com/en-us/azure/artifact-signing/concept-certificate-management
Its exact excerpt is: "The EKU is associated with the certificate profile".
Context7 returned unrelated DigiCert, webhook, and Microsoft Graph libraries. No matching library was used.
The actual profile and issuer values remain commissioning inputs. Documentation does not prove provider eligibility or native behavior.

## 10. Open questions, blockers & unknowns: Metadata

### B-20: Pinned client qualification remains unexecuted

- **Status:** open
- **Owner:** KEL-53 dependency owner.
- **Impact:** Production metadata acquisition cannot use the candidate.
- **Check:** T-20 passes on its exact reviewed source and supported compiler/targets.

### B-21: Native durability requires execution

- **Status:** open
- **Owner:** KEL-270 Windows transaction owner.
- **Impact:** Store code cannot claim crash or power-loss safety.
- **Check:** T-21 publishes the real Windows cut matrix and retained-state observations.

### B-22: Production authority and trusted clock remain uncommissioned

- **Status:** open
- **Owner:** Release/security owner.
- **Impact:** No production repository, profile, bootstrap root, or clock trust claim is available.
- **Check:** The authority contract has authentic provider, policy, delivery, and deployment receipts.

### U-20: Complete applicable-clause and transitive review

UNKNOWN: This inspection does not establish full TUF 1.0.37 conformance or all dependency risks. T-20 must finish the clause map and dependency review. A required missing rule blocks client qualification.

## 10. Open questions, blockers & unknowns: Windows admission

### B-40: Native qualification
- **Status:** open
- **Owner:** Guard and existing Windows acceptance owners.
- **Impact:** The proposed path cannot receive production or release disposition.
- **Check:** AC-40 through AC-46 and the joined migration cell have exact-source native receipts.

### B-41: Commissioned policy
- **Status:** open
- **Owner:** Release/security with KEL-135 and KEL-254.
- **Impact:** The shipping Host remains closed until genuine authority and signed-policy evidence exists.
- **Check:** Independent review matches commissioned issuer/profile, repository, discovery, and artifact identities.

## 10. Open questions, blockers & unknowns: Recovery

### B-60: Independent recovery authority is not commissioned

- **Status:** open
- **Owner:** Existing release/security owner.
- **Impact:** No manual recovery artifact may ship or grant authority.
- **Check:** Genuine policy, issuer/profile, separated credential custody, recovery origin, and signed directive receipts exist.

### B-61: Native recovery and joined migration remain unqualified

- **Status:** open
- **Owner:** KEL-270 with KEL-135 and independent Windows/security reviewers.
- **Impact:** Initial and manual recovery cannot be advertised as production ready.
- **Check:** Exact-source AC-60 through AC-65 and every applicable joined A19 support cell pass.

### B-62: Compromised-system and unavailable-recovery-authority cells require operator recovery

- **Status:** open
- **Owner:** Release/security and the affected system operator.
- **Impact:** In-place automated continuity is unavailable in those cells.
- **Check:** Clean-system recovery is independently executed with current authentic installation and an explicit data-restoration disposition.

No design approval or release approval is recorded by this draft.

## 10. Open questions, blockers & unknowns: Integration

### B-80: Exact-version human sign-off is pending

- **Status:** open
- **Owner:** Monish Khandelwal, specification sign-off.
- **Impact:** Feature implementation cannot begin from this draft.
- **Check:** A recorded approval names the exact complete specification commit.

### B-81: Independent combined review

- **Status:** open
- **Owner:** Isolated reviewers and the coordinating author.
- **Impact:** The original three R3 design lenses and `review-receipt.json`
  remain historical evidence for their exact earlier source. Amisha's
  requested-change review and later corrective commits introduce a new
  normative snapshot. Neither the old independent receipt nor green
  definition/hosted documentation checks constitute a new independent
  design disposition for that changed contract.
- **Check:** Independently re-review the new complete exact commit for
  recovery signer profile rotation, signed-role exclusivity, expired
  prepared successor retirement, source-current-root continuity and
  field-level policy equality; refute surviving counterexamples,
  reconcile all three original review threads and publish a new
  exact-head receipt before asking the owner for T-80 sign-off.

No unresolved design choice may be hidden as a release gate.
Product and native unknowns remain in the owning parts until their stated execution checks pass.
