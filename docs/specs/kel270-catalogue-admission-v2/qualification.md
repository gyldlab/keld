# Qualification, performance, and blockers

Part of: [index](index.md)
Owns: Definition and native evidence, performance limits, and open release or approval blockers.

## 7. Test plan & evidence: Identity

### E-1: Definition-only vectors

- **Run:** `python3 -B docs/engineering/evidence/kel270-catalogue-admission-v2/verify_identity_vectors.py --check`.
- **n:** 94 definition checks. The exact output digest is in the compact review receipt.
- **Environment:** Python 3.10 or newer with test-only `cryptography`.
  Local qualification used 46.0.5. The B-82 candidate workflow pins 50.0.2
  and verifies its wheel hashes. Its hosted run is evidence, not trusted-base
  authorization, until B-82 integration is approved.
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

AC-28 additionally requires the following equal-version matrix before and after restart:

| Stored v100 under current threshold | Fetched valid v100 signed content | Required result |
|---|---|---|
| Invalid after threshold increase | Identical | Refuse before graph selection, retain floor |
| Invalid after threshold increase | Changed snapshot | Refuse before graph selection, retain floor |
| Valid | Identical, including different signature order | Unchanged, retain stored envelope |
| Valid | Changed snapshot or expiry | Refuse before graph selection |

Run isolated mutants for stored-envelope authentication and signed-content equality.
Keep incoming signatures, expiry and all unrelated policies valid for each mutant.
Retain the valid advancing-v101 positive and replay-v1 refusal controls.
Synthetic signature or decision-table checks cannot qualify the patched client or Windows admission.

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
| Role expiry within deadline | Advance OS UTC past a signed role expiry while its lease wait remains within 120 seconds | Fresh role-expiry refusal before creation, despite an open monotonic deadline |
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
| R2 / AC-67 | Expired prepared abort with absent, exact and partial planned objects. Retire only protected, unreferenced objects before retry. | Foreign, unprotected, unlisted or referenced objects halt. See AC-67 and the R2 crash cuts below. |
| R3 / AC-68 | Old U1 signer unavailable. Valid signed recovery-only Host/directive, fresh consent, new root and U2 complete loss-only recovery | Missing independent recovery authority refuses before trust commit. A normal transition without U1 signature refuses under AC-61. Single-field repository-policy mutations refuse. Check current, provenance, and epoch before and after |

For R2, prove every planned path absent before sealing. Exclude existing
shared blobs. Cut after prepared-journal and pointer readback, before the
first preparation blob, during each blob write, before successor
publication and before current replacement. For present canonical
objects, distinguish exact bytes, partial/corrupt bytes, and foreign or
unprotected objects. Exact and partial planned objects permit bounded
retirement only when the exact plan path, regular non-reparse type, expected
owner SID, KEL-53 DACL, retained handle and complete unreachability proof
all pass. Partial bytes are never parsed or adopted. Foreign/wrong-owner,
wrong-DACL, reparse/directory, unlisted or referenced objects MUST halt
without deletion or pointer removal.
Before the first recovery journal, leave an authenticated uncommitted
ordinary-refresh N+1 or D-23 ordinary intent from a prior crash.
Require D-24 retirement and verified absence under the same lease.
Use a separate negative with a foreign, corrupted or wrong-epoch
orphan. It MUST refuse before publishing a recovery journal or pointer.
Cut during ordinary orphan retirement and show that only the matching
D-24 intent may resume. Never interpret it as a D-65 recovery intent.
Also cut ordinary refresh after zero, one, and many blob writes, before
N+1 publication. The blob-only case MUST create its own authenticated
D-23 intent before deletion. Check the retained current, all owner references,
pointer absence, exact blob inventory, and N+1 absence on every restart.
Prepopulate a planned path before sealing and require refusal without deletion.
Mutate planned blob bytes/length while retaining the expected owner/DACL and
no references; cleanup MUST retire that partial object without adopting it.
Mutate owner/DACL, intent field, live reference, or pending pointer
independently; each MUST refuse deletion. Cut after intent publication,
each listed deletion, and intent removal. Prove a new valid N+1 remains writable.
For R2, also cut before retirement-intent publication and after its
journal-digest readback. Cut after every exclusive-blob deletion and after
orphan-generation deletion. Also cut after durable completion-digest
readback, after intent removal, and before pending-pointer removal. With the
intent present, restart MUST finish only its listed cleanup. With the intent
present but a null journal intent digest, first authenticate the sealed
plan and bind that digest before deletion. Reject a mismatched digest.
With the intent absent, restart MUST publish the first intent for both null journal digests,
or verify equal non-null digests and all planned-object paths absent before
pointer-only removal.
Remove or mismatch the completion digest as separate single-fault controls.
Both MUST refuse pointer removal and fresh admission. Unlisted
attempt-owned or newly referenced objects MUST halt without deletion.
For R3, independently mutate recovery digest, recovery signer, catalogue keys, metadata/targets URLs, discovery digest, publisher digest, and root version.
Keep Windows signature validity, source-state integrity, and unrelated checks valid in each negative control.
The review addenda are new planned coverage, not retroactive passes for any of the 110 product cases.
For initial admission, cut after `admitted` plus pointer, before the
durable `creation-armed` fence. Withdraw the target, prove no native
installation effect, abort and clear the pointer, then admit a different
valid target. As a refusal control, create one scaffold before its durable
receipt. Restart MUST keep the pointer and refuse to delete that object.
In this profile, inject a canonical empty root created before `admitted` and crash without any record.
Matching owner/DACL and empty contents MUST NOT authorize `ExactEmptyScaffold`; expect `PartialOrUncertain` refusal.
Any future positive requires independent pre-attempt creator evidence bound to root, owner and DACL.
Mutate that origin, root identity, DACL and contents separately; each MUST refuse. No positive is claimed yet.
Cut after `admitted` but before pointer publication; find the journal under the lease and retain D-9 references through terminal abort.
With no pointer, inject corrupt, wrong-owner or multiple orphan journals and missing D-9 references separately.
Each MUST halt before ordinary mutation or new admission, with no object deletion.
For root catch-up, accept at least three consecutive signed successors.
At every publish and committed-history retirement crash cut, prove no more
than three generation files, protected references remain live and another
valid refresh succeeds. Delete one listed N-2 blob, crash, then remove its durable retirement intent as a negative fault.
Restart MUST refuse further deletion without that exact intent; do not confuse normal completed-intent removal.
Repeat from committed N-2/N-1/N with a valid manual loss-only directive.
Retire unreferenced N-2 before the D-65 journal and D-68 pointer; cut at each retirement edge.
Also crash after listed exclusive-blob deletion and N-2 generation deletion while intent remains; resume without reconstructing absent bytes.
Held or foreign N-2 MUST refuse without deletion or a new pointer. Recovery remains a planned native test.
For the D-27 admission codec, read back one canonical receipt and mutate
each target, policy, generation and expiry binding independently. Each
MUST refuse before the consuming transition journal is written.

## 7. Test plan & evidence: Integration

The fixture and review files are in `docs/engineering/evidence/kel270-catalogue-admission-v2/`.

### E-80: Definition and structure checks

- **Run:** `python3 -B docs/engineering/evidence/kel270-catalogue-admission-v2/verify_spec_evidence.py --check`
  and `just spec-structure` in the owning repository.
- **n:** 94 identity cases, nine evidence-capacity cases, three isolated checker
  negative controls, and 23 receipt-readiness negative controls. The compact receipt pins both report digests.
- **Environment:** A verified local Mac worktree. The check does not modify
  tracked source. Complete historical output remains archived in research.
  Automatic hosted evidence verification is blocked by B-82.
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
`python3 -I -B docs/engineering/evidence/kel270-catalogue-admission-v2/verify_evidence_capacity.py --check`
checks 20 required distinct objects, byte caps, duplicate references and
over-limit mutants. Its proof checks MUST also refuse under `python3 -O`.
Regenerate detailed output locally with `--output-dir`
in a scratch directory. Hosted regeneration requires B-82 approval.
The research archive pins the earlier nine-case output.

## 9. Perf impact: Identity

No measured runtime result exists. The new parsing and hashing remain on cold installation or update paths.
Measure the approved native path against the same signed baseline. Apply the existing architecture performance budget.

## 9. Perf impact: Metadata

No performance improvement is claimed. The metadata profile bounds each object, refresh, request, and retained state operation. Reuse immutable blobs to avoid copying every role on each root transition. Garbage collection cannot delete the current generation, its referenced blobs, or pending recovery evidence.
Keep at most three generation files: current, its immediate predecessor, and one prepared successor. Retain every blob referenced by those files. A root-reset intent stays in current until completion. Delete only proven exclusive unreferenced objects under D-24's matching durable retirement intent and store lease.
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
- **Check:** Exact-source AC-60 through AC-68 and every applicable joined A19 support cell pass. Any missing or failed AC-66, AC-67, or AC-68 keeps this blocker open.

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

### B-82: Trusted-base CI evidence integration

- **Status:** open
- **Owner:** Repository CI/security maintainer with the trusted-base authority.
- **Impact:** The standalone non-mutating evidence check and its two
  failure mutants pass locally. They cannot become a hosted required
  CI gate through this normal specification PR. CI provenance prohibits
  edits to `.github/workflows/ci.yml`, `justfile` and CI executors.
  Its 15 published SHA-256 values come from a receipt in the incoming
  branch. They prove source and receipt consistency, not independent
  security-owner authorization. A coordinated source and receipt update
  can pass these checks without an approved reviewed-source anchor.
  A former draft added an immutable but unapproved upload action.
  The workflow-security gate rejected it, correctly. The action was
  removed without changing the allowlist.
- **Check:** Separately review and land a trusted-base CI policy change
  that runs `verify_spec_evidence.py --check` from the canonical
  documentation job with its qualified test-only Python dependency.
  Bind any claimed approved source to an independent trusted-base
  source SHA or receipt digest, with a security-owner publication decision.
  Protect that authority against coordinated source/receipt repinning.
  Verify all required native workflow-security tests and protected
  CI provenance at the new base. Keep this PR `do-not-merge` until
  the owner records that authorization and its exact check evidence.

No unresolved design choice may be hidden as a release gate.
Product and native unknowns remain in the owning parts until their stated execution checks pass.
