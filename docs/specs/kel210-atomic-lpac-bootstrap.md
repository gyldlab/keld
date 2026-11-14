# Spec: KEL-210 atomic exact-package bootstrap constructor

- Status: implementing
- Tracker: KEL-210
- Owner: GYLDLAB
- Updated: 2026-11-14
- Pin: c42cd1c834fd15a98110ead50f48f850ce916224

## 1. Goal & non-goals

Add a Windows bootstrap constructor that creates its pipe with the exact intended package policy.
Return a listener only after live descriptor readback confirms the policy.
Preserve the published current-user-only constructor and unchanged HELLO authentication.

This slice qualifies the constructor with native Windows callers.
It does not connect the constructor to ordinary Core, CLI, or product startup.
It does not grant Strict admission, complete containment, or full Windows build support.

Non-goals:
- No supervisor or child transport adapter, installer, or inherited client transport.
- No resource grants, package capabilities, lower integrity label, or post-create ACL widening.
- No TCP fallback, new manifest permission, wire field, or HELLO version change.
- No change to Guard, KEL-107 admission, KEL-78 profiles, or the default KEL-101 policy.

The package SID only selects OS access. It does not authenticate a connecting process.

## 2. Spec refs

- `docs/architecture/02-ipc.md` owns authenticated kipc.
- Architecture 03 owns containment and authorization. Architecture 06 owns process and bootstrap ownership.
- `docs/specs/kel210-windows-lpac-primary.md`, lines 3-14 and 46-70, limits this slice to T1 qualification.
- That spec, lines 235-281, records 24 native cells for package ACEs, namespace, and label.
- Its post-creation ACL mutation was a test fixture. It is not a shipping constructor.
- `crates/keld-ipc/src/windows_named_pipe.rs:308-409` and `:1554-1575` own pipe creation and descriptor readback.
- `crates/keld-ipc/src/bootstrap.rs:1497-1551` owns endpoint, token, and listener state.
- KEL-101 retains the default one-TokenUser DACL. KEL-107 and KEL-78 admission remain unchanged.

## 3. Acceptance criteria

### AC-1: Existing listeners keep their current-user policy

- **Given:** The normal `bind()` and keeper `bind_rendezvous()` entry points.
- **When:** Each creates a Windows listener.
- **Then:** Its DACL grants the existing current TokenUser only.
  - Existing first-instance, HELLO, and listener behavior remain unchanged.
- **Negative control:**
  - **Single fault:** Add the prepared package SID to the default constructor.
  - **At-risk precondition:** The default path creates its first native pipe instance.
  - **Expected RED:** Default descriptor and LPAC-denial regressions fail.

### AC-2: Exact package access exists from the first instance

- **Given:** A captured TokenUser and a canonical prepared package SID.
- **When:** The new constructor creates its first Windows pipe instance.
- **Then:** Its protected DACL contains exactly two explicit allow ACEs.
  - The principals are that TokenUser and that package SID.
  - Each ACE uses the existing `PIPE_ACCESS_MASK`.
  - The DACL contains no inherited or broad ACE.
  - Creation retains first-instance, byte-mode, overlapped, remote-rejecting, and noninheritable flags.
- **Negative control:**
  - **Single fault:** Add a broad third ACE during initial descriptor construction.
  - **At-risk precondition:** Exact descriptor readback follows atomic creation.
  - **Expected RED:** The native exact-descriptor assertion rejects the extra ACE.

### AC-3: Invalid or noncanonical package SIDs fail before creation

- **Given:** An untrusted SID string passed to the IPC constructor.
- **When:** IPC validates its syntax, shape, canonical spelling, and principal identity.
- **Then:** Only revision 1, authority 15, and the package SID form are accepted.
  - The subauthorities start with 2 and contain seven subsequent `u32` package components.
  - A valid SID round-trips through the existing SID conversion owner.
  - Malformed text, wrong subauthority counts, and noncanonical text return `InvalidInput`.
  - Non-package, duplicate, and current-user principals return `InvalidInput`.
  - The broad `S-1-15-2-1` and `S-1-15-2-2` principals return `InvalidInput`.
  - No failed validation creates a pipe or interpolates unchecked text into SDDL.
- **Negative control:**
  - **Single fault:** Permit `S-1-15-2-1` through the validator.
  - **At-risk precondition:** Validation runs before the first `CreateNamedPipeW` call.
  - **Expected RED:** The invalid-SID regression fails before any pipe is created.

### AC-4: Readback proves DACL, owner, and preserved label

- **Given:** The first instance was created using the selected descriptor.
- **When:** IPC reads back the live native pipe security descriptor.
- **Then:** Exact DACL, no inheritance, local-only access, and first-instance policy match.
  - The returned owner SID equals the captured TokenUser SID.
  - The normal inherited mandatory label and other metadata match a fresh default pipe.
  - The constructor never lowers or rewrites the mandatory label.
  - A mismatch returns an error, drops owned handles, and publishes no endpoint or token.
- **Negative control:**
  - **Single fault:** Change the owner SID while leaving the two allow ACEs unchanged.
  - **At-risk precondition:** Readback uses a separate owner check after DACL comparison.
  - **Expected RED:** Owner-readback acceptance rejects the mismatched owner.

### AC-5: Native clients still require HELLO authentication

- **Given:** A prepared zero-capability LPAC Bun and the intended package SID.
- **When:** Bun connects to the qualified pipe and performs unchanged HELLO.
- **Then:** HELLO succeeds and two distinct typed echo calls return valid responses.
  - A wrong-package LPAC cannot connect before HELLO.
  - A same-user client with the wrong token receives no authenticated response.
- **Negative control:**
  - **Single fault:** Skip the token equality guard in HELLO.
  - **At-risk precondition:** A same-user client can open the pipe but has a wrong token.
  - **Expected RED:** The wrong-token authentication regression fails.

### AC-6: Cancellation and profile cleanup remain bounded

- **Given:** A live prepared LPAC profile and a newly created listener.
- **When:** Cancellation occurs before acceptance or during pending accept.
- **Then:** Each path finishes within an independent bounded deadline.
  - Owned worker tasks join and all child processes are reaped before profile release.
  - Two complete new setups use fresh endpoint, token, and profile identities.
- **Negative control:**
  - **Single fault:** Omit the pending-accept worker join on cancellation.
  - **At-risk precondition:** The pending native accept has begun before cancellation.
  - **Expected RED:** The lifecycle regression detects an unjoined worker or unreaped child.

### AC-7: Security failures have independent negative controls

- **Given:** Working native tests for exact package access and descriptor readback.
- **When:** Each fault is introduced separately and then reverted.
- **Then:** Removing the package ACE makes its access test fail.
  - Substituting a wrong package SID makes its access test fail.
  - Bypassing exact readback makes its descriptor test fail.
  - Restored source passes the corresponding positive controls.
- **Negative control:**
  - **Single fault:** Bypass exact readback in one temporary test mutation.
  - **At-risk precondition:** A deliberately mismatched descriptor is reachable.
  - **Expected RED:** The exact-readback test fails instead of silently accepting the listener.

### AC-8: Production admission and default policy are untouched

- **Given:** The constructor and qualification test diff for this T1 slice.
- **When:** Independent review audits all changed call sites and security policies.
- **Then:** No ordinary product Core, CLI, or Runtime caller selects the new constructor.
  - There is no Strict verdict, capability grant, TCP fallback, or alternative admission path.
  - No test evidence is reported as complete containment or a released Windows build.
- **Negative control:**
  - **Single fault:** Wire the new constructor into the default product bootstrap.
  - **At-risk precondition:** The changed-callsite review enumerates the new constructor's callers.
  - **Expected RED:** The scope audit rejects the unauthorized product caller.

### AC-9: Native qualification uses pinned Bun bytes

- **Given:** An explicit absolute qualified Bun path and independent expected SHA-256.
- **When:** The new-constructor acceptance harness prepares to launch Bun.
- **Then:** It compares executable bytes with the independent digest before launch.
  - Missing, mismatched, or unqualified input refuses before any Bun process starts.
  - No implicit `PATH` fallback supplies acceptance evidence.
- **Negative control:**
  - **Single fault:** Supply a one-byte mismatched expected digest.
  - **At-risk precondition:** Qualification verifies its pinned input before process creation.
  - **Expected RED:** The harness refuses without launching Bun.

## 4. Design

Proposed public API on Windows only:

`WindowsNamedPipeBootstrapListener::bind_windows_lpac(package_sid: &str) -> io::Result<Self>`

Runtime remains the sole owner of the prepared `WindowsLpacProfile`.
Trusted callers obtain the SID from `WindowsLpacProfile::sid_string()`.
No app configuration, environment variable, renderer request, or CLI SID selector is added.
IPC validates SID syntax and shape. IPC does not claim to prove SID provenance.
The public API documentation must state the trusted-caller obligation.
Do not introduce a circular dependency from IPC to Runtime.

IPC owns descriptor construction, atomic `CreateNamedPipeW`, readback, endpoint/token generation, and listener lifecycle.
Reuse `WindowsNamedPipeServer::bind_with_descriptor`, `read_pipe_descriptor`, and existing mismatch checks.
Reuse the existing owned handles and shared `bind_at` initialization.
Expose only the exact-package policy constructor. Do not expose a generic arbitrary-SDDL API.
Do not create a parallel wire error protocol.

Capture the TokenUser before creation and set that SID as descriptor owner.
Compare the returned owner SID separately after the DACL readback.
The existing `first_mismatch(..., Dacl)` does not check the descriptor owner.
Do not use `OwnerDaclLabel` with a fabricated expected SACL.
That comparator requires a complete expected label descriptor.
Preserve the inherited mandatory label and compare it with a new default pipe under the same process context.
On any mismatch, fail before publishing the endpoint or authentication token.

Decision atoms stay independently falsifiable:
- SID validation must reject invalid input before any native creation.
- The atomic first-instance descriptor must survive exact live readback.
- HELLO must refuse the wrong token even when OS pipe open succeeds.
- Cancellation must close and join owned work without leaked children.
- The old constructor must continue denying the LPAC client.

The new API does not mutate an existing default listener.
Post-creation DACL widening introduces an unsafe policy transition.
Changing the default DACL also breaks its published one-user contract.
The existing constructor remains the permanent compatibility surface.

No manifest capability or wire field is added. The ordinary HELLO version stays unchanged.
Mac and Linux behavior stays unchanged. No throughput or performance improvement is claimed.

## 5. Boundaries

- **Implement in:** `crates/keld-ipc/src/windows_named_pipe.rs` and `crates/keld-ipc/src/bootstrap.rs`.
- **Qualify in:** Existing IPC tests and `crates/keld-runtime/tests/windows_lpac_app_link.rs`.
- **Document in:** The KEL-210 owning spec and generated corpus as the repository rules require.
- **Must not touch:** Guard admission, Core, CLI, Pack, Runtime production supervisor or child roles.
- **Must not touch:** Profile/resource policy, dependencies, wire codec, or manifest permission grants.

Any new production unsafe operation requires exact owner allowlist review.
Do not infer approval for a new SID-validation API from existing unsafe permissions.

## 6. Tasks

### T-1: Add the exact-package constructor and native policy tests

- **Blocked by:** none
- Reuse the IPC descriptor owner and atomic native pipe creation.
- Preserve `bind()` and `bind_rendezvous()` default behavior.
- Cover exact ACEs, SID rejection, owner/label readback, and failed-listener cleanup.
- Convert only the new qualification call sites away from test-only ACL mutation.
- Update source evidence in the owning KEL-210 specification.
- Do not add product bootstrap wiring.

### T-2: Qualify real LPAC Bun, HELLO, cancellation, and negative controls

- **Blocked by:** T-1
- Use the explicit qualified Bun path and independent expected SHA-256 from AC-9.
- Run native HELLO, two typed echoes, wrong-package and wrong-token tests.
- Run both cancellation cases and at least two fresh complete setups.
- Perform each source mutation from AC-7 separately, then rerun the positive tests.
- Retain the historical four-cell `PATH`-discovered fixture as historical evidence only.
- Replace `grant_fixture_pipe` only for new-constructor acceptance paths.
- Preserve current default-denial cases and prohibit shipping ACL mutation fallback.

### T-3: Complete independent review and final source evidence

- **Blocked by:** T-2
- Recheck current platform documentation and exact unsafe operations.
- Review the changed callers, permission boundary, profile lifetime, and negative tests.
- Report format, warning-denied clippy, workspace tests, and required aggregate gates.
- Report native failures, skipped checks, and any unavailable acceptance explicitly.

## 7. Test plan & evidence

### E-1: Default compatibility and exact native descriptor

- **Run:** Windows IPC unit tests plus descriptor/readback tests in `keld-ipc`.
- **n:** At least one valid default and exact-package case per descriptor variant.
- **Environment:** Native Windows on the exact tested source commit.
- **Limits:** Rust checks on another OS do not prove Windows ACL behavior.
- Covers AC-1 through AC-4 and the native portions of AC-7.

### E-2: LPAC Bun acceptance, HELLO, and lifecycle

- **Run:** Windows `keld-runtime` test `windows_lpac_app_link` with pinned Bun input.
- **n:** Two complete fresh setups, plus pre-cancel and pending-accept controls.
- **Environment:** Native Windows, with captured absolute Bun path and SHA-256.
- **Limits:** Historical `PATH`-discovered runs and synthetic descriptors are not pinned native evidence.
- Covers AC-5, AC-6, and AC-9.

### E-3: Independent source mutations and changed-callsite audit

- **Run:** Separate ACE-removal, wrong-package, and readback-bypass mutations.
- **n:** Three independent RED checks, each followed by restored-source PASS.
- **Environment:** Native Windows, exact source and changed-callsite audit.
- **Limits:** A rejection in another layer cannot stand in for the selected mutation.
- Covers AC-7 and AC-8.

### E-4: Repository gates on the final source

- **Run:** Format, workspace clippy with warnings denied, workspace tests, and CI contracts.
- **n:** One final-source full set, repeated only if further source changes occur.
- **Environment:** Exact final branch SHA and required hosted CI platforms.
- **Limits:** Passing CI alone does not qualify new Windows LPAC admission or complete product startup.

The current document normalization records no new native execution.
Use independent deadlines and observed lifecycle signals, not sleeps.
All native outcomes must name their exact binaries, revision, and evidence receipts.
The historical caller locations near lines 691-693 and 857-858 remain qualification references.
Keep the previous default-denial control near line 105.

## 8. Review gates triggered

The public API, permission model, and exact touched unsafe operations require independent review.
The diff adds no planned dependency or wire protocol change.
The reviewer must inspect default invariance, trusted SID source, atomic creation, and readback.
The reviewer must also inspect cancellation cleanup and every negative control.
Owner approval and the repository's winning claim remain required for source integration.

## 9. Perf impact

Cold startup adds SID validation and exact descriptor readback.
These operations reuse existing native creation and verification code.
No hot framing path or steady-state path changes.
No measured throughput improvement or performance budget waiver is claimed.

## 10. Open questions, blockers & unknowns

### B-1: Native final-source evidence is outstanding

- **Status:** open
- **Owner:** Windows IPC and Runtime acceptance owners
- **Impact:** The constructor cannot claim qualified native LPAC acceptance or product readiness.
- **Check:** Record a pinned-Bun native test receipt with the required positive and negative cases.

### B-2: Exact unsafe review and source acceptance are outstanding

- **Status:** open
- **Owner:** Independent KEL-210 security and architecture reviewer
- **Impact:** The public API and permission changes remain unapproved for source integration.
- **Check:** Review the exact final changed operations and record the review disposition.

### B-3: KEL-107 downstream proof semantics remain separate

- **Status:** open
- **Owner:** KEL-107 policy owner
- **Impact:** Full Windows product activation cannot infer symbolic policy proof from this constructor.
- **Check:** Publish an independently reviewed reusable KEL-107 policy-proof contract.

### U-1: Platform documentation and exact native outcomes

The current platform documentation receipt must be refreshed before final source acceptance.
The current source's real native acceptance outcomes remain unverified in this spec edit.

The recorded KEL-210 decision `15eaa3fc-b2f1-412c-a91d-e671ccfcc007` uses delegated technical authority.
It does not establish user approval of exact implementation bytes.
Final native proof and independent review remain necessary.
