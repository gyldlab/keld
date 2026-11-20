# GH541 performance and remaining gates

Part of: [index](index.md)
Owns: §§9–10 and the nine historical selections

## 9. Perf impact

No measured performance gain is claimed. The policy allow path remains allocation-free after cold parsing/indexing. The trusted window
lookup and revocation check use existing host ownership. Reuse the current counting-allocator harness and benchmark the actual
permitted/denied end-to-end route after implementation. Queue/codec allocation and native callback latency are separate measurements. Do not
justify a second policy cache or registry with an unmeasured performance assumption.

## 10. Open questions, blockers & unknowns: exact-content approval selections and remaining gates

The recorded owner-delegated decision approves these nine selections together with the exact technical content manifest:

1. Decoded exact `el:` literals; reject U+002A and controls; strict windows shape; no wildcard/normalization authority.
2. Incoming channel grants only; authenticated outbound reply/send remains destination/lifetime-gated.
3. One existing keld-wv mint, checked u32, +1 renderer commit and +2 successful host-load-plus-commit; no same-document/cancelled-renderer increment; terminal exhaustion.
4. AwaitingCommit closes old-world rebind; failed host load never rolls back authority; ambiguous host same-document classification remains unshipped until #459/#542 qualifies it.
5. Final pre-Bun revocation ordering and original-document delivery validation; no replay and no promise of rollback after handoff.
6. The narrow new core forwarding call to the existing dispatcher; typed distinction between channel/native queries; implementation API representation remains under #552's separate gate.
7. Exact ChannelForbidden text preserved, exact manifest pointer supplied in trusted diagnostics under the public API gate, no accidental CallError wire extension.
8. KEL-142 wording reconciliation, KEL-143 recovery separation, the KEL-102/T4 media-integration gate, and coordination with approved/unmerged #1096/#1105.
9. Assign #542 the narrow first-proof trusted application-document binding gate and native attempt correlation, and #552 the core forwarding adapter plus composed zero-write test. These are explicit proposed scope clarifications to approve; full #460 policy and #538 surface ownership stay separate. Fresh navigation identity alone does not prove content eligibility.

Approval artifact: KEL-102 comment `8fa29b34-bdb6-470f-9f97-9cdf0aa1d0eb`, decision SHA-256
`2da2c23a53848d618f08612a1106d53283b7488601be7fd3eaa5ed95038df5af`, binds technical commit `7e0c1b89c4767f2e6625cd2bfc4d36d86b30fc26` and
all five file blobs/hashes. Its permitted metadata follow-up does not change requirements or waive gates. Open implementation artifacts:
#552 and #542 approved specifications, their failing-first executed tests, final source/CI gates, and real macOS evidence. These missing
artifacts prevent implementation/security qualification. #541 remains open until its actual merge acceptance; no merge is authorized.

### Assessment

**GO for the approved contract and parallel #540/#464 specification/test-corpus work. NO-GO for enabling arbitrary Electron IPC, claiming
#552/#542 complete, or claiming the MVP security boundary qualified.** This documentation change supplies the approved contract and
acceptance plan. It supplies no implementation or native qualification, and each implementing owner must satisfy its separate specification
and evidence gates.

### B-1: implementation and security qualification

- **Status:** open
- **Owner:** #552 and #542 implementation owners.
- **Impact:** The missing implementation artifacts above prevent implementation and security qualification.
- **Check:** Verify the separately approved specifications, executed failing-first tests, final source and CI gates, and real macOS evidence listed above.
