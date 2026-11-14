# Spec: KEL-210 atomic exact-package bootstrap constructor
Status: implementing under user-delegated technical decision; independent review and native acceptance pending
Linear: KEL-210 · Proposed owner: GYLDLAB · Updated: 2026-11-10

## 1. Goal & non-goals
Add an explicit Windows bootstrap constructor whose pipe is created atomically with a protected DACL for the current TokenUser and one prepared AppContainer package SID. Return a listener only after live readback proves the selected policy. Preserve the published current-user-only constructor and unchanged HELLO authentication.

This is a production-quality primitive exercised only by qualification callers in this slice. No Core/CLI startup wiring, Strict verdict, KEL-107 admission change, supervisor/child adapter, installer, resource grants, capability grants, lower integrity label, inherited client transport, TCP fallback, or post-creation ACL widening. The package SID does not authenticate the connecting process.

## 2. Spec refs
- Architecture 02 authenticated kipc; architecture 03 containment versus authorization; architecture 06 process/bootstrap ownership.
- `docs/specs/kel210-windows-lpac-primary.md:3–14,46–70`: only T1 qualification is approved; distinct client policy required; absent admission never allows product fallback.
- Same spec `:235–281`: 24-cell native evidence supports exact package ACE with existing namespace/label; test-only post-creation ACL mutation is explicitly not a shipping constructor.
- `crates/keld-ipc/src/windows_named_pipe.rs:308–409,1554–1575`: existing atomic creation, ownership, descriptor reader.
- `crates/keld-ipc/src/bootstrap.rs:1497–1551`: existing endpoint/token generation and listener state.
- KEL-101 default one-TokenUser policy remains unchanged. KEL-107 and KEL-78 admission remain unchanged.

## 3. Acceptance criteria
1. Default `bind()` and keeper `bind_rendezvous()` retain exactly the current one-TokenUser DACL and current behavior.
2. New constructor creates its first and only instance with exactly two explicit allow ACEs, current TokenUser and the validated package SID, each existing PIPE_ACCESS_MASK; protected DACL, no inherited/broad ACE. No window exists with another DACL.
3. Invalid SID text, non-package SID, broad S-1-15-2-1/S-1-15-2-2 principals, malformed subauthority count, noncanonical textual representation, and duplicate/current-user principals fail InvalidInput before pipe creation. Accepted package SID is canonical revision 1 / authority 15 / subauthorities 2 plus seven u32 package components. Round-trip through the existing SID conversion owner and verify shape; do not interpolate unchecked text into SDDL.
4. Live readback verifies exact DACL, noninheritance and local-only/first-instance policy before listener return. Set owner to the captured TokenUser at creation, then compare the returned owner SID separately against that SID: existing first_mismatch(..., Dacl) does not check owner. Do not select OwnerDaclLabel with a fabricated expected SACL; that comparison requires a complete expected label descriptor. The new mode preserves the normal inherited mandatory label and never lowers or rewrites it; native comparison with a fresh default pipe proves identical label/metadata under the same process context. Readback failure returns error and drops all owned handles; no endpoint/token is returned.
5. Intended zero-capability LPAC Bun connects, unchanged HELLO succeeds, and two distinct typed echo calls succeed. Wrong-package LPAC fails before HELLO. Same-user wrong-token client never receives authenticated response.
6. Pre-cancellation and pending-accept cancellation finish under independent deadlines and join workers; repeated complete setups pass with fresh endpoint/token/profile. Every child is reaped before profile release.
7. Independently remove the package ACE, substitute a wrong package SID, or bypass exact readback: each corresponding acceptance test fails. Restore source and repeat the positive case.
8. No ordinary product caller is added and no test result is reported as Strict, complete containment, or full Windows build support.

## 4. Design
Proposed public Windows-only API:
`WindowsNamedPipeBootstrapListener::bind_windows_lpac(package_sid: &str) -> io::Result<Self>`.

Trusted callers obtain the SID directly from the Runtime-owned prepared `WindowsLpacProfile::sid_string()`. No app config, environment variable, renderer request, or command-line SID selector is added. IPC validates syntax/shape, not provenance; its documentation states the trusted-caller obligation. Runtime remains sole profile owner. No circular IPC-to-Runtime dependency is introduced.

IPC owns descriptor construction, atomic CreateNamedPipeW, exact live readback, endpoint/token generation, and listener lifecycle. Reuse `WindowsNamedPipeServer::bind_with_descriptor`, `read_pipe_descriptor`, existing mismatch checks and owned handles. Add a narrow exact-package policy constructor rather than a public arbitrary-SDDL API. Share listener initialization with existing bind_at without changing existing policy selection. Preserve first-instance, byte-mode, overlapped, remote-rejecting and noninheritable creation flags. Propagate existing I/O errors and actionable readback mismatch diagnostics; do not create a parallel wire error protocol.

Decision-bearing atoms: (a) SID validation, invalid input must never create; (b) atomic descriptor and exact readback, descriptor mutation must fail; (c) HELLO, wrong token must fail despite OS-open rights; (d) lifecycle, cancellation must close/join without leaked children; (e) compatibility, old default must still deny LPAC. These have separate tests, so authentication success cannot stand in for ACL proof or cleanup proof.

No new manifest capabilities or wire fields; ordinary HELLO version unchanged. Mac/Linux unchanged. No performance claim. Reject mutating a default listener because it creates a policy transition and contradicts KEL-210; reject changing default DACL because existing callers require one-user policy. Existing constructor is the permanent compatibility surface, not a fallback from a failed LPAC constructor.

## 5. Boundaries
Implement only in `crates/keld-ipc/src/windows_named_pipe.rs`, `crates/keld-ipc/src/bootstrap.rs` and their existing test owners; qualification consumption in `crates/keld-runtime/tests/windows_lpac_app_link.rs`. Update owning KEL-210 spec and generated corpus as repository tooling requires.

No changes to Guard admission, Core, CLI, Pack, Runtime production role/supervisor, profile/resource policy, dependencies, or wire codec. Any new unsafe operation outside the currently sanctioned owner requires a separate exact allowlist review before implementation; do not assume SID-validation API permission.

## 6. Tasks
- T1 (one bounded PR after approval and winning claim): implement the atomic constructor, default-regression/native descriptor tests, qualification caller conversion and lifecycle/authentication negatives. Update specification evidence and generated corpus. No product wiring accompanies this PR.

## 7. Test plan
Use the repository managed worktree/work-run route. Run existing IPC Windows unit tests plus Runtime `windows_lpac_app_link` native integration. The current fixture discovers Bun through PATH; it is not a pinned-input test. Before reporting qualified acceptance for this amendment, supply an explicit absolute qualified Bun path and independently supplied expected SHA-256, verify the exact executable bytes against that digest before launch, and record both in evidence. Missing path, mismatch, or unqualified binary must fail the qualification invocation rather than silently fall back to PATH. Keep this explicit qualification input in the test owner, not production configuration. Historical PATH-discovered runs remain honestly labeled as such and cannot prove the new pinned-input acceptance. Preserve the historical four-cell qualification fixture separately; replace `grant_fixture_pipe` use only in new-constructor acceptance paths (current callers near lines 691–693 and 857–858). Do not leave a shipping ACL-mutation fallback. Preserve default-denial cases near line105.

Map AC1 to default descriptor and LPAC-denial regressions; AC2–4 to native creation/readback and invalid-input tests; AC5 to actual Bun HELLO/two echoes/wrong-token/wrong-package; AC6 to deterministic pending witness, cancel/join, and two complete fresh repetitions; AC7 to independent source mutations with restored positive runs; AC8 to changed-callsite audit. Use owned objects, bounded waits and explicit witnesses, not sleeps. Run/report format, warning-denied workspace clippy, workspace tests and required aggregate gates; disclose every unavailable native/aggregate result. A synthetic descriptor test is not native LPAC evidence.

## 8. Review gates triggered
Public API, permission model, and unsafe review for exact touched native operations. No dependency or wire protocol change. Independent reviewer must inspect default invariance, trusted-SID boundary, atomic creation/readback, cleanup, and the negative controls. Approval and repository claim must precede source edits.

## 9. Perf impact
Cold bootstrap adds SID validation and exact descriptor verification, reusing existing creation/readback. No hot framing or steady-state path change; no throughput improvement claimed.

## 10. Open questions
Decision recorded in KEL-210 comment 15eaa3fc-b2f1-412c-a91d-e671ccfcc007: adopted under explicit user delegation of technical decisions; not user review of exact bytes. Winning claim re-fetched before source edits. The native final-source evidence does not yet exist. Current platform documentation receipt and exact unsafe-operation permission review must be refreshed before implementation. These are explicit gates, not presumed approval.

Separate downstream dependency: KEL-107 must define any reusable symbolic policy proof semantics before full Windows product activation. This constructor neither resolves nor bypasses it.
