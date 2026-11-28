# Spec: Windows A00 administrative scheduling

- Status: draft
- Tracker: GH-1182
- Owner: GYLDLAB (KELD maintainers)
- Updated: 2026-11-28
- Pin: 2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a

## 1. Goal & non-goals

Allow one fresh startup-only A00 diagnostic on Ramani K10 after verified administrative recovery and formal approval.
Do not change product behavior, Guard admission, OS authority, or ordinary lifecycle acceptance.
Do not credit V3, A24, final CI, or any of the ten authentic workflows.

## 2. Spec refs

- FACT: The [N7 council handoff](https://github.com/gyldlab/keld/issues/1182#issuecomment-7153099684) says “It keeps two separate predicates:” (retrieved 2026-11-27). These are baseline eligibility and launch admissibility, not policy approval.
- FACT: The [historical evidence search](https://github.com/gyldlab/keld/issues/1182#issuecomment-7227592989) records “N2 normal authorized old-logon lifecycle and N3 correlated terminal/new identity evidence” (retrieved 2026-11-27). It also records the retained council revision’s N5 ordering.
  The retained `round2-kel53-revision.md` SHA-256 is `36aec1c8de5ee3cf8a8f27cb243a8cbb8d341b31635f050ecea78f3b49229c0a`. These are program predicates, not numbered clauses in KEL-53.
- FACT: KEL-96 retains Host startup and lifecycle ownership. See §4.4 `docs/specs/kel96-no-flag-host-boot.md:360@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a` and T4 `docs/specs/kel96-no-flag-host-boot.md:651@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
  Its exact-object dev-stage sentinel remains unchanged: `docs/specs/kel96-no-flag-host-boot.md:543@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
- FACT: KEL-135 retains exclusive leases and marker-validated Host scavenging. See `docs/specs/kel135-persistent-profile-identity.md:457@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
- FACT: KEL-102 retains authenticated dispatch, quiescing, and revocation. See `docs/specs/kel102-host-guard-enforcement.md:522@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a` and `docs/specs/kel102-host-guard-enforcement.md:578@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
- FACT: KEL-53 Machine-UAC owner-loss retirement requires independent writer exclusion and exact logon retirement. See `docs/specs/kel53-full-package-activation.md:1243@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
  The pinned clause identifies exact-content approval as pending. It governs MachineUacDirect. This proposal does not apply to an activation or update.
- FACT: GH-1188 retains normal browser release evidence. See A10 `docs/specs/gh1188-windows-window-adapter.md:955@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`, A24 `docs/specs/gh1188-windows-window-adapter.md:969@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a` and D8 `docs/specs/gh1188-windows-window-adapter.md:1044@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
- FACT: Open tracker [GH-1187](https://github.com/gyldlab/keld/issues/1187) requires “actual Windows primary-role explicit profile admission” and descendant custody (retrieved 2026-11-28).
- FACT: Open tracker [GH-1183](https://github.com/gyldlab/keld/issues/1183) states “Native OS acceptance remains **BLOCKED** until a verified Windows operator runs it.” (retrieved 2026-11-28). These tracker requirements are not approved specifications.
- FACT: GH-499 owns the B2 identity check. The [exact Host identity review packet](https://github.com/gyldlab/keld/issues/499#issuecomment-7210114330) says “A qualified new Host image needs a reviewed identity binding followed by rebuilt CLI and native qualification.” (retrieved 2026-11-27).
- FACT: The [P6 physical preflight](https://github.com/gyldlab/keld/issues/1182#issuecomment-7226623191) pins the reviewed combined driver and replay evidence. It says “These checks establish present empty-state and byte custody, not N2/N3 lifecycle acceptance.” (retrieved 2026-11-27). It does not admit launch.
- FACT: `MAINTAINERS.md:20@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a` and `docs/agents/spec-template.md:3@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a` require independent review and human sign-off.

## 3. Acceptance criteria

### AC-1: Baseline evidence is independent and complete

- **Given:** A reviewed amendment and an authorized operator supply an exact administrative recovery record.
- **When:** An independent verifier checks the bound evidence and records its limits.
- **Then:** The baseline can become eligible only if all D-2 checks pass. Missing, inaccessible, contradictory, or stale required evidence leaves it on HOLD.
- **Negative control:**
  - **Single fault:** Substitute a name-only process list for exact custody reconciliation.
  - **At-risk precondition:** Every other baseline check passes.
  - **Expected RED:** No eligible baseline receipt is issued.

### AC-2: The ordinary lifecycle path is unchanged

- **Given:** The original logon remains active and the original Job/sentinel receipt is absent.
- **When:** An acceptance evaluator consumes an administrative baseline receipt.
- **Then:** V3 remains FAIL. A24 remains UNQUALIFIED. Ordinary N2, N3 and N5 get no PASS from this receipt.
- **Negative control:**
  - **Single fault:** Map the administrative baseline receipt to ordinary N3.
  - **At-risk precondition:** Use a valid administrative receipt with no ordinary retirement evidence.
  - **Expected RED:** The ordinary acceptance evaluator rejects the attempted PASS.

### AC-3: Baseline eligibility does not admit launch

- **Given:** The administrative baseline is eligible.
- **When:** P6 requests a fresh startup-only diagnostic.
- **Then:** D-3's independent gates must all pass under the same exclusive slot. Otherwise no diagnostic launches.
- **Negative control:**
  - **Single fault:** Omit the exclusive slot check.
  - **At-risk precondition:** Use a valid baseline and unspent key. W08, B2, exact preflight and a fresh seal otherwise pass.
  - **Expected RED:** The nonlaunching test observes an allowed admission decision during a conflicting slot. An independent witness verifies zero real spawns.

## 4. Design

### D-1: Two dispositions

Keep ordinary N2 (authorized old-logon lifecycle) and N3 (correlated terminal evidence and new identity) unchanged.
Keep ordinary N5 dependent on completion of N1 through N4.
Create a distinct administrative receipt type. Never write it into an ordinary N2, N3, N5 or acceptance field.
Keep ordinary N1 through N7 intact. Apply these cross-references to the new branch:

- N1: Require preservation and independent hash verification. Distinguish immutable evidence from live records. Record missing fields as unknown.
- N2 and N3: Do not claim retirement. Use D-2's exact same-logon administrative evidence only for new scheduling.
- N4: Require actual old owner and HWND retirement. Exclude cross-session owners and competing build, UI and profile owners. Require a named exclusive scheduling owner.
- N5: Do not award an ordinary pass. Evaluate only the already recorded Guarded Host recovery as a distinct historical input under D-2. Do not repeat maintenance.
- N6: Reverify unchanged original namespace, expected contents, supported stage absence and failed-run hashes. Require exact clean source, strict offline preflight and independent report binding.
- N7: Require independent technical review, authorized human approval, and evidence-specific disposition. No unresolved predicate counts as PASS.

Keep the ordinary N5 receipt and ordering unchanged. This proposal supplies only the prospective scheduling decision.

### D-2: Administrative eligibility

Require the following evidence before issuing a baseline receipt:

1. Match the failed run and exact device and boot. Independently bind the effective token's SID, authentication logon ID and logon start time. Refuse a different principal or logon. Record the observed same-logon state.
2. Preserve the original request, result, failure handoff and recovery receipt by digest. Preserve missing fields as unknown.
3. Correlate prior owners by immutable process identity, including creation time, not by PID or name alone.
4. Prove actual retirement of each retained controller, observer, application family and predecessor owner. Reconcile the original HWNDs and cross-session ownership. Refuse incomplete observations.
5. Verify no old endpoint, lease, handle or delayed operation can act on the fresh attempt. Use only an existing, authority-owned revocation or exclusion witness. Do not substitute fencing for process retirement. Hold if any required custody is unproved.
6. Bind the authorized operator, admitted Host image and guarded recovery record. Verify the volume, root, leaf, marker, DACL, lease and browser release. Refuse foreign, busy, reparse or unproved ownership. Verify no unrelated session or resources were disrupted.
7. Verify original namespace identity and ownership, valid protection, guarded release, an empty namespace and no residual predecessor stage.
8. Recheck all 15 source bindings. Bind the reviewed driver, its replay evidence and the baseline verifier's inputs to the receipt.
9. Require independent architecture and Guard review of the exact change and recorded approval from an authorized human specification owner.
10. Require an independent verifier to sign off the exact evidence set. Unknown historical exits cannot supply any required positive fact.

The independent verifier must differ from the recovery operator. The receipt binds its verifier and approval references.
Do not infer custody from a dashboard, an empty directory, PID absence or denied Security log access.
Do not repeat recovery, sign out an active logon, change audit settings, or reconstruct a historical receipt.

### D-3: Separate startup admission

Require a qualified W08 native Host-to-Guard delivery witness and the B2 matched Host, CLI and provider identity.
Verify that W08 covers the exact B2-approved runtime bytes and the selected Host-to-Guard interface identity.
Keep the W08 qualification run identity in its own witness. Give A00 a fresh run identity.
Refuse admission before spending the key if the evidence binding is missing, unknown or mismatched.
Verify exact source, binary and driver bytes. Follow original preflight and fresh seal rules.
Under the same exclusive Ramani K10 lease, P6 must reverify all live D-2 custody and namespace facts before sealing.
The verified exclusions and lease must remain valid through spawn. No transfer or slot change is allowed after the final checks.
Use one durable, canonical admission key for the verified recovery of the original failed attempt on the verified machine. Every request for that recovery shares one admission budget.
Bind the single approved diagnostic target to the immutable record. Refuse an unknown or changed target or identity without allocating another key. New seals and caller labels cannot change the key.
The existing admission owner must atomically spend the key and seal before spawn. Serialize parallel attempts. Preserve spent state across restart.
A fresh seal cannot unspend the key after abort, crash or spawn failure. Do not auto-retry or issue a substitute key.
A mismatch, unknown, owner conflict, lost lease, stale evidence or spent key refuses launch.
If the existing owner cannot supply these guarantees, keep HOLD. This specification does not authorize a second admission owner.
Give the new attempt fresh identities and independent startup and teardown evidence. Preserve its actual result.
Do not grant product acceptance, a later workflow, or an installation/activation/recovery action.

## 5. Boundaries

This draft changes only prospective scheduling policy for GH-1182. It adds no execution path.
Do not change production source, tests, manifests, trust policy, binaries, ACLs, or the OS.
Do not change KEL-53 activation or KEL-135 profile recovery.

## 6. Tasks

### T-1: Review and approve the exact specification candidate

- **Blocked by:** B-3
- Reconcile the complete retained N1–N7 text. Obtain independent architecture and Guard dispositions on the final candidate.
- Before formal approval, run Section 7 controls only on a nonlaunching surface approved by the existing admission owner. Record the reviewed head, evidence digest and authorized human approval.
- Use fixtures to test the D-2 rules. Verify the actual D-2 evidence in T-2 after T-1.

### T-2: Disposition the baseline and hand off to P6

- **Blocked by:** T-1
- Verify the exact evidence against D-2. Record N7 in GH-1182. Keep HOLD until every applicable fact is verified.
- The existing P6 owner checks D-3 before acting. No new execution claim is needed.

## 7. Test plan & evidence

Exercise AC-1 through AC-3 with one fault per case. Include a positive fixture for each path.
Use only the existing admission owner's approved nonlaunching surface and isolated disposable state. Observe its actual custody checks and durable spend logic.
Keep real spawn unavailable through a test-harness fence independent of admission logic in every case. Use an independent witness to record the admission decision and verify zero real spawns.
The positive admission fixture must reach the decision boundary without launch. No fixture grants real authority or credit. If the surface or witness is unavailable, keep T-1 and operational admission on HOLD.
For AC-1 also test wrong machine, wrong boot/logon, reused PID, surviving owner, wrong namespace, and changed source hash.
For AC-1 also test stale delayed writers on the same logon, recovery replay, fabricated exit, verifier/operator overlap and missing approval.
For AC-2 test administrative credit to V3, A24, N2 and N5 separately.
For AC-3 separately drop W08, B2, exact preflight and seal.
Use a qualified W08 witness and valid B2 evidence with matching runtime and interface identities.
Use distinct W08 and A00 run identities. Verify that the matched evidence reaches the admission decision boundary.
Keep all other D-3 checks valid. Change only the evidence binding in each negative case.
Test an absent binding, a witness for different runtime bytes, and a witness for a different interface separately.
Verify refusal before the key is spent. Verify zero real spawns for the matched case and each negative case.
Test valid second seals against the same recovery, in parallel and after restart.
A changed target or caller label must not grant another spend. Verify refusal for a target mismatch and a spent decision on repeat use of the approved target.
Lose custody after seal or lose exclusivity between spend and spawn. A crash or failed spawn after spend must not permit reuse.
Record the exact head, test-surface authority, fixture identity, at-risk precondition, decision before dispatch, zero-spawn witness and cleanup. Synthetic tests cannot qualify Windows.

### E-1: Existing recovery evidence

- **Run:** The [recovery readback](https://github.com/gyldlab/keld/issues/1182#issuecomment-7189077469) states “Missing recovery Host exit code remains null.” (retrieved 2026-11-27). See §2 for the physical preflight and archive search.
- **n:** One original V3 failure and one administrative recovery.
- **Environment:** Ramani K10. Evidence binds the original attempt; the draft Pin governs repository clause references.
- **Limits:** Does not prove ordinary logon retirement, historical Host exit, a current exclusive slot, or final acceptance.
  FACT: The [archive search](https://github.com/gyldlab/keld/issues/1182#issuecomment-7227592989) records the original logon as “still present” (retrieved 2026-11-28).
  FACT: The same search records a boot observation difference of approximately two seconds. The difference remains unreconciled. Historical exit codes remain unknown.
  FACT: The [fresh D-2 readback](https://github.com/gyldlab/keld/issues/1182#issuecomment-7235993733) again observes logon `1060514` with its original start time and boot. Logon `1060445` is also present (retrieved 2026-11-28).
  The readback receipt SHA-256 is `d19fa66c5bd609a69e332e310d64cfe8b8d6392602a36bf320aaa324a7e1f40d`.
  The active original logon leaves ordinary N2/N3 retirement unproved. Its presence alone does not exclude the proposed same-logon N7 route.
  That route still requires independent D-2 proof of predecessor process/HWND retirement and old-authority exclusion under items 3–5. It also requires all remaining D-2 facts.
  The fresh readback does not establish those facts or a current exclusive slot.
  The same readback's ACL result covers `LOCALAPPDATA` and its parent only. It does not establish a fixture-child ACL, fixture lifetime or cleanup. It supplies no N7 admission evidence.
  FACT: The [N7 disposition](https://github.com/gyldlab/keld/issues/1182#issuecomment-7229381087) states “No actual admission test or startup launch was authorized by these readbacks.” (retrieved 2026-11-28).
  The historical replay and offline model do not qualify the actual nonlaunching surface or Section 7 controls.

## 8. Review gates triggered

Independent architecture and Guard security review, exact-content human sign-off, and independent PR review.
No wire, dependency, public API or production permission changes.

## 9. Perf impact

None. Scheduling evidence only.

## 10. Open questions, blockers & unknowns

### B-1: Exact-content approval

- **Status:** open
- **Owner:** Authorized human KELD specification owner and independent maintainers.
- **Impact:** This draft grants no N7 eligibility and no P6 launch.
- **Check:** Close B-3. Record independent dispositions, Section 7 acceptance evidence and formal approval bound to the reviewed head.

### B-2: Baseline and independent launch checks

- **Status:** open
- **Owner:** Existing P6 owner, with the W08, B2 and K10 slot owners.
- **Impact:** Current evidence does not prove all D-2 facts or admit execution.
- **Check:** Reconcile the boot observation difference in E-1. Verify D-2 after T-1. Verify each D-3 gate. Preserve the ordinary acceptance contract.

### B-3: Actual nonlaunching test surface

- **Status:** open
- **Owner:** Existing P6 admission owner and independent witness.
- **Impact:** T-1 cannot complete. The actual test surface and witness remain unqualified. See E-1.
- **Check:** Record the existing admission owner's approved nonlaunching surface and independent witness. If unavailable, retain HOLD and route a separate test design for owner approval.
