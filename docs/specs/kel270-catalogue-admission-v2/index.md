# Spec: Windows catalogue admission and recovery

- Status: draft
- Tracker: KEL-270
- Owner: Monish Khandelwal, specification sign-off
- Updated: 2026-11-17
- Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302

## 1. Goal & non-goals

Define one implementable contract for Windows direct per-user catalogue admission.
The contract joins signed identity, durable metadata, native admission, and key-loss recovery.
It preserves the existing installation writer, browser store, application floor, and strict process policy.

This is a specification proposal. Human approval authorizes implementation of this exact version.
It does not qualify an implementation, commission signing authority, or authorize a production release.

FACT: The earlier research plan is published at research commit `022d0c5640319c912e0fd34817622d836bf207bd`.
Its source is [note 269](https://github.com/0monish/keld-research/blob/022d0c5640319c912e0fd34817622d836bf207bd/library/auth-distribution/269-pr1531-catalogue-research-adversarial-review.md).
The product inspection source is PR #1531 at `7ff41173d28d58ae61ccbb3382d055c8ef9c9c97`.
This proposal starts from the current main pin above. It does not change that PR.

Non-goals:

- No replacement installer, updater service, browser registry, or cryptographic protocol.
- No machine-seamless installation or direct takeover of managed installations.
- No in-place security claim after suspected system compromise.
- No legacy migration claim without its separately qualified bridge.
- No sharing one commissioned repository ID between different applications in this profile.
- No production permission, signing key, release, or merge disposition from documentation checks.

### Requested decision

Review the exact commit, independent review receipt, and stated support boundaries.
Approve that commit for implementation, or name the specific contract that must change.
The approval record must identify the commit. Silence and a request to continue drafting are not approval.
T-1 and T-20 MUST NOT start until both T-80 exact-version sign-off and T-82 governing-contract reconciliation are complete. Other native implementation also requires its listed owner and unsafe-operation gates.
The release owner commissions actual authority through T-81 before signed production qualification.

## Parts

| Part | Owns | Load when |
|---|---|---|
| [Requirements](requirements.md) | Source pins and all acceptance criteria | Review facts or test requirements |
| [Identity](identity.md) | D-WIRE: bytes, stable publisher, transitions, provenance | Identity or wire behavior changes |
| [Metadata](metadata.md) | D-METADATA: standard client, store, time, bounds | Acquisition or freshness changes |
| [Windows](windows.md) | D-NATIVE: worker, discovery, handles, admission | Native installation behavior changes |
| [Recovery](recovery.md) | D-RECOVERY: initial interruption and independent key replacement | Recovery behavior changes |
| [Integration](integration.md) | Atomic owners, approval order, support boundaries | Integration or approval decisions |
| [Implementation](implementation.md) | Change boundaries, tasks, dependency graph | Schedule or implement an approved task |
| [Qualification](qualification.md) | Evidence, performance, release and approval blockers | Verify support or readiness |

Evidence is stored outside the specification directory:

- [Compact research archive and review index](../../engineering/evidence/kel270-catalogue-admission-v2/review-receipt.json).
- [Non-mutating definition verification](../../engineering/evidence/kel270-catalogue-admission-v2/verify_spec_evidence.py).
- [Original acceptance plan](../../engineering/evidence/kel270-catalogue-admission-v2/acceptance-plan.json).
- [Canonical identity golden vectors](../../engineering/evidence/kel270-catalogue-admission-v2/identity-vectors.json).
- [Evidence capacity definition tests](../../engineering/evidence/kel270-catalogue-admission-v2/verify_evidence_capacity.py).

## Current blockers

- B-1
- B-2
- B-20
- B-21
- B-22
- B-40
- B-41
- B-60
- B-61
- B-62
- B-80
- B-81
- B-82

## 8. Review gates triggered

All five gates apply to the proposed implementation: unsafe, public API, permission model, dependency addition, and wire protocol.
Independent design review checks the specification. Exact implementation review must check the later code and native evidence.
The draft adds no unsafe grant and enables no Host entry point.
