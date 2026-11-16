# Legacy evidence ledger
Part of: [index](index.md)
Owns: Impact, open blockers and approval history

## 9. Perf impact
One optional field decode, one version/declaration check, and one immutable selected-state value belong on the cold startup path. The existing exact-byte permissions digest is reused. No per-message profile parsing, manifest read, additional runtime, or new reaper is permitted.

No speedup or numeric overhead is claimed. If the integration materially changes launch allocation or process setup, compare the existing attributed macOS startup path using the repository's established budget rules. Strict and legacy behavior must be semantically correct before comparing timings.

## 10. Open questions, blockers & unknowns
### 10.1 Prior approval and R1 decisions

Under the repository owner's continuing 2026-10-23 delegation, this revision approves the following target after independent review of the corrected technical content. The new R1 approval, not the old eba60d22 approval, covers the schema correction and the added public admission entry:

1. Root `keld.permissions.jsonc.profile` is the sole authoring location.
2. Only strict/legacy are configurable; absence remains unverified/default and never implies legacy.
3. Schema 2 remains GH #446's no-profile facade contract. Explicit profiles use 3 for renderer boots and 4 for facade boots; profile-free renderer inputs remain 1. Unsupported combinations refuse (§4.4).
4. Strict admission and all independent guard/identity/lifecycle/renderer/native/updater owners remain intact.
5. Legacy explicitly forfeits OS containment and is qualified first for the macOS primary role.
6. KEL-74's current unverified evidence/scoring contract is preserved; stale #586/#588 exclusions are superseded.
7. The staged PR sequence and test matrix govern implementation; build/migrate integration is reported pending until those commands are real.

No further user choice is requested. KEL-78 remains the profile owner; GH #446 remains the facade Ready owner. The independent review found and
  resolved the earlier literal-API obligations that contradicted `admit_uncontained`; AC-6/AC-7 and §4.5 now state the same routing rule. The
  reviewed content digest and new delegated approval are published on #1120/KEL-78, with the matrix correction handed to #446/#588. Each
  implementation PR still requires independent review of its exact final public contract, permission boundary and compiled-boot seam. Design
  approval neither changes the explicit no-merge instruction nor supplies any implementation or OS evidence.

### 10.2 Verified blockers and first completion checks

### B-1: #586 integration pending
- **Status:** open
- **Owner:** KEL-78/#586 publish the exact reviewed amendment and reconcile old briefs; preserve do-not-merge
- **Impact:** Approved design is not yet integrated into main
- **Check:** Repository PR and consistent tracker decision receipts; main integration remains separately gated

### B-2: Key currently ignored
- **Status:** open
- **Owner:** #588 extend existing parser and actual host preflight atomically
- **Impact:** Merely editing permissions does not select Legacy/Strict
- **Check:** Explicit declaration changes real admitted receipt

### B-3: Old host ignores profile
- **Status:** open
- **Owner:** #588 plus KEL-96/KEL-102 implement 3/4 pairing without repurposing Ready's 2
- **Impact:** New strict request could be ignored on unsupported runtime
- **Check:** Previous host refuses new artifact before app execution

### B-4: Competing schema-2 meanings
- **Status:** open
- **Owner:** #586 corrects its own assignment; #446 keeps Ready ownership; independent review covers §4.4
- **Impact:** Renderer/profile and facade/no-profile cannot share a closed version
- **Check:** Reviewed exact four-row matrix and consistent producer/consumer tests

### B-5: Product receipt collector absent
- **Status:** open
- **Owner:** #587 defines trusted host-origin collection and exact artifact binding, then #588 consumes it
- **Impact:** Forwarded application output can forge a diagnostic receipt lookalike
- **Check:** Forged-app-output negative control fails while a real bound-host receipt succeeds

### B-6: Generic admit is not a live proof
- **Status:** open
- **Owner:** #588 wire existing host/session path
- **Impact:** Returning Legacy alone proves no launch/auth/guard behavior
- **Check:** Real macOS bound child plus permitted/denied calls

### B-7: macOS strict T2c unqualified
- **Status:** open
- **Owner:** Existing KEL-78 strict owner continues signed-helper hostile proof separately
- **Impact:** Explicit strict cannot be promised for draw.io today
- **Check:** Exact artifact/profile archive passes strict checks

### B-8: Build/migrate reserved
- **Status:** open
- **Owner:** Existing command owners consume this contract; use hand-authored fixture first
- **Impact:** Their future output cannot be claimed complete now
- **Check:** Real commands execute and report honestly

### B-9: Draw.io dependency graph still open
- **Status:** open
- **Owner:** #499/#589 plus #541/#552/#542 and API owners
- **Impact:** Profile alone does not provide IPC/window/preload/dialog behavior or pending renderer controls
- **Check:** Actual Mac workflow with denied-channel/stale-generation controls

### B-10: Install/product denominator incomplete
- **Status:** open
- **Owner:** #589/KEL-141 and corpus owners
- **Impact:** Development launch cannot establish installed/product compatibility
- **Check:** Qualifying install evidence and committed denominator

### B-11: Doctor entry assumption
- **Status:** open
- **Owner:** #497/config owner
- **Impact:** A valid profile does not fix layout validation for draw.io
- **Check:** Configured src/main/electron.js passes intended layout check


The specification and delegated decisions are published through the #586 PR and KEL-78/#588 handoffs. Runtime implementation, native qualification, reserved-command integration, and merging remain open. Issue comments and PR records carry execution status; this document defines the approved target contract.

### 10.3 Evidence ledger

All source links below are pinned to the inspected commit. Main advanced from `442f3ebc` during research; the direct comparison to `3503a8aa383427dcf50ccbf6653c4298e7fb8aad` changed only Windows foreign-app admission tests and the KEL-254 spec. The profile/parser/reporting findings are unchanged.

[S1]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/kel78-strict-profile-sandbox.md
[S2]: https://linear.app/gyldlab-keld/issue/KEL-78/spec-strict-profile-os-sandbox-for-bun-roles-and-native-addon-workers
[S3]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/specs/gh532-first-proof-evidence-rules.md
[S4]: https://github.com/gyldlab/keld/issues/586
[S5]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/lib.rs
[S6]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/admit.rs
[S7]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/boot.rs
[S8]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session.rs
[S9]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/verb.rs
[S10]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-cli/src/doctor.rs
[S11]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/engineering/compat-scoreboard.md
[S12]: https://github.com/gyldlab/keld/pull/638
[S13]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/architecture/03-security.md
[S14]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-guard/src/verified_manifest.rs
[S15]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session/boot_contract.rs
[S16]: https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/crates/keld-core/src/app_session.rs#L1846-L1888
[S17]: https://github.com/gyldlab/keld/issues/420#issuecomment-6034980014
[S18]: https://github.com/gyldlab/keld/issues/497#issuecomment-6026217139
[S19]: https://github.com/gyldlab/keld/issues/589
[S20]: https://github.com/gyldlab/keld/issues/420#issuecomment-6022061800
[S21]: https://github.com/gyldlab/keld/blob/bb43e474577297f0792740676994aa153e5df1dd/docs/specs/gh446-facade-boot-ready.md
[S22]: https://github.com/gyldlab/keld/compare/3503a8aa383427dcf50ccbf6653c4298e7fb8aad...f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f
[S23]: https://github.com/gyldlab/keld/issues/587#issuecomment-6371500644

Additional owning links: [GH #588](https://github.com/gyldlab/keld/issues/588), [GH #499](https://github.com/gyldlab/keld/issues/499), [GH
  #587](https://github.com/gyldlab/keld/issues/587), [GH #500](https://github.com/gyldlab/keld/issues/500), [approved vocabulary PR
  #612](https://github.com/gyldlab/keld/pull/612), [direct-update legacy
  refusal](https://github.com/gyldlab/keld/blob/3503a8aa383427dcf50ccbf6653c4298e7fb8aad/docs/architecture/06-runtime-and-tooling.md#L1098-L1103).

Decision history: KEL-78 owner comment `99767a07-b061-497d-89c1-2381032bad95`, 2026-10-07, selects unverified evidence option (b). The tracker returned no comment-specific URL; the KEL-78 issue link and the approved merged spec provide the verifiable references.
