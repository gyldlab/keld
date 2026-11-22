# KEL-270 D42 / T82 — Guard native-owner decision and Windows proof matrix

**Status: REVIEW PACKET ONLY. NOT AN FFI GRANT, INSTALL PERMIT, OR ACCEPTANCE RECEIPT.**

Exact source: Draft PR #1593 at
`4b064e8851d4b2112231bfdb26414f7562ae9d6c` (2026-11-22).
This file is not an `AGENTS.md` amendment and cannot approve or replace one.
No production source, permissions, dependencies, trust store, or protected CI are
changed by this packet. The worktree was started from the exact published head.

## Decision already established by current repository source

| Boundary | Actual current state | Required owner decision |
| --- | --- | --- |
| Guard T4d native permission | `crates/keld-guard/AGENTS.md` restricts production `GetTokenInformation` to `TokenGroups`, `TokenElevation`, `TokenSessionId`, `TokenPrivileges`; `OpenThreadToken(QUERY,TRUE)` and `OpenProcessToken(QUERY)` are already allowed | **T82 Guard unsafe owner** must review and explicitly add the *exact new classes*, not reinterpret the old `FFI only` list |
| Actual worker | `windows_machine/uac_token.rs` contains crate-private `WindowsPerUserWorkerPreflight`: retained `OwnedHandle`, current thread ID, query-only session/elevation, no impersonation, and sticky revalidation | Do not upgrade this partial observer to `WindowsPerUserInstallContext` or issue `D46` authority until all predicates and effect-time rechecks pass |
| SID | Current `current_process_sid()` is a safe **fresh-process-token** reader, not a retained-token `TokenUser` reader | Guard must bind a validated binary SID to the exact retained token and independently prove it matches current process identity |
| COM / LocalAppData | No approved Guard `known_folder.rs` owner or COM calls; `keld-wv` is a separate owner | Require an explicit Guard COM/Shell owner, reviewed lifetime proof, and `Win32_UI_Shell` + `Win32_System_Com` feature approval |
| Known-folder contract | `windows.md` D42 requires `SHGetKnownFolderPath(FOLDERID_LocalAppData,0,NULL)` on the actual admitted worker, followed by revalidation | Neither environment-variable fallback nor `KF_FLAG_DONT_VERIFY`, `KF_FLAG_CREATE`, `KF_FLAG_DEFAULT_PATH`, Default User token, or path substitution is permitted |
| B82 / release | T82 grant, signed issuer/profile evidence, native Windows acceptance, and independent gates are not established by a review packet | Keep the shipping Host and KEL-53/D46 initial-install effect closed |

The current Guard `AGENTS.md` has a 3,840-byte manifest cap and was near
that limit on the reviewed branch. Do not enlarge the budget, silently compress
unrelated security rules, or treat this document as a new normative FFI owner.
After an independent owner approves a precise grant, update the *nearest Guard
AGENTS* in its separately reviewed instruction change and run the mandatory
instruction-loading gates.

## Candidate minimum read-only Guard grant for independent acceptance

**Not yet permitted in production:**

- `windows_machine/uac_token.rs`: only the existing
  `OpenProcessToken(TOKEN_QUERY)` retained handle and
  `GetTokenInformation(TokenUser, TokenType, TokenStatistics,
  TokenIntegrityLevel, TokenIsAppContainer, TokenUIAccess,
  TokenHasRestrictions, TokenRestrictedSids)`. Keep its existing
  `TokenElevation`, `TokenSessionId` and
  `OpenThreadToken(TOKEN_QUERY, TRUE)` checks. No `TOKEN_ADJUST_*`,
  `TOKEN_DUPLICATE`, extra impersonation access, `SetTokenInformation`,
  `AdjustTokenPrivileges`, or alternate owner.
- `windows_machine/known_folder.rs`, owned by **Guard only**:
  `CoInitializeEx(NULL, COINIT_MULTITHREADED)`,
  `CoUninitialize()`, `SHGetKnownFolderPath(FOLDERID_LocalAppData,
  dwFlags=0, hToken=NULL, &out)`, and `CoTaskMemFree(out)`.
  Require separate approval of the exact `windows-sys`
  `Win32_UI_Shell` and `Win32_System_Com` features in
  `crates/keld-guard/Cargo.toml`. Existing WebView COM initialization
  cannot establish the Guard worker's apartment.

### Token validation and revalidation contract

1. Open `TOKEN_QUERY` once on the actual post-dispatch worker. Record the
   worker's `ThreadId`, retain an owned process-token handle, and require
   `OpenThreadToken(QUERY,TRUE)` to return exactly `ERROR_NO_TOKEN`.
   A token present or any other error is a refusal.
2. Parse `TokenUser` as a **binary** SID *inside its returned buffer*.
   Validate non-null pointer, alignment, revision, bounded subauthority count,
   allocation bounds, and size before dereferencing. Refuse SYSTEM,
   LocalService, and NetworkService. Never use a SID string or a fresh safe
   reader as proof of the retained handle's identity.
3. Require `TokenType == TokenPrimary`; compare complete
   `TOKEN_STATISTICS.TokenId`, `AuthenticationId`, and `ModifiedId`
   LUIDs. Bind retained and freshly opened current-process token identity,
   plus the initiating `TokenSessionId`.
4. Require `TokenIntegrityLevel`'s mandatory-label SID final RID exactly
   `SECURITY_MANDATORY_MEDIUM_RID == 0x2000`, `TokenElevation == 0`,
   `TokenIsAppContainer == 0`, `TokenUIAccess == 0`,
   `TokenHasRestrictions == 0`, and an **empty**
   `TokenRestrictedSids` list. These are separate predicates. Do not query
   reserved `TokenIsRestricted` or infer `TokenHasRestrictions` from
   `IsTokenRestricted(FALSE)`.
5. Preserve the **1 MiB maximum** token-query buffer. Use checked lengths,
   alignment and count arithmetic; reject truncated, oversized, overlapping,
   misaligned or out-of-buffer pointers, count overflow, unexpected query
   failures, and invalid structures. Release every query-only handle through
   the owning Rust RAII type, including error paths.
6. Before folder query and every protected creation, publication and final
   commit, recheck executing thread, no impersonation, same current-process
   token `TokenId`/`AuthenticationId`/`ModifiedId`, session, SID and the
   entire profile. Any mismatch or query failure permanently invalidates
   the context and unconsumed permit. A later clean result cannot revive it.
   No callback, async dispatch, or unrelated foreign call may intervene
   between final revalidation and the protected effect. No current D46
   issuer may be enabled by this candidate.

### Guard Shell/COM ownership and adverse-path contract

1. Initialize COM **on the executing Guard worker thread** immediately
   before the Shell lookup. `S_OK` and `S_FALSE` each require one matching
   `CoUninitialize` on that same thread via a non-`Send` RAII owner;
   `RPC_E_CHANGED_MODE` and other failed HRESULTs deny without an
   unmatched `CoUninitialize`. No activation, apartment substitution, or
   borrowed WebView COM owner. Review whether MTA is safe for the actual
   worker; an STA collision is a **refusal**, not a retry on another thread.
2. Query exactly `SHGetKnownFolderPath(FOLDERID_LocalAppData,0,NULL,&out)`.
   Reject failed HRESULT and null/invalid/unterminated/oversized path results.
   Copy only a validated bounded UTF-16 path. `CoTaskMemFree` releases a
   non-null Shell-owned allocation **on every exit, even if the Shell call
   fails**; COM is uninitialized after the allocation is released.
3. The returned folder may be redirected, including to a network location.
   Reject UNC, remote, removable, otherwise non-local, unknown-volume and
   reparse/symlink paths before any protected effect. Use the existing KEL-53
   retained-handle/no-follow ancestor owner for physical path admission; a
   textual `C:\\` prefix or environment variable is not proof.
4. Revalidate the same worker's retained token and session again **after**
   the Shell call, before opening destination ancestors. A known-folder
   string is data and cannot mint a D46 or filesystem-effect capability.
   If the Shell call itself causes unexpected network I/O on a redirected
   path, count this as a failed zero-effect cell; do not relabel it harmless.

### D90 / D64 / D68 cross-owner effect-time join (separate grants)

- The first protected filesystem effect requires **both** the full Guard D42
  native worker recheck and a live, independently qualified **D90 repository
  owner under its one exclusive real lease**. Capture the same repository,
  app, user SID, attempt and lease identity before effect, and refuse any
  mismatch, released lease or stale/partial membership. The Guard token
  preflight cannot itself mint or retain a repository write authority.
- Worker20's owner finding identifies two separate fixed-target KEL-53
  native rights requiring their **own** precise review:
  (a) durable replacement of
  `admissions/<recorded-attempt>.json`, and
  (b) **terminal-only** deletion of `pending-operation.json` after a
  proved matching operation end. Both must bind the **same actual D90
  exclusive lease**, not a generic file slot or borrowed token permission.
  Do not repurpose an existing generic path/right or issue an additional
  Guard-side write capability.
- At each D64/D68 write or removal phase, the KEL-53 owner must verify its
  exact typed target, attempt, prior state and recovery evidence, then obtain
  the Guard worker's immediately preceding D42 recheck without intervening
  foreign dispatch. A protected effect after a failed worker or lease check
  is an unconditional refusal. This document proposes **no** new D64/D68
  write, delete, or recovery operation.

## Real Windows adversarial test and independent ETW observability matrix

All cases are **NOT RUN** on this exact source head. Run only under a reviewed
native test-owner grant and isolated Windows SID/VM. Preserve signed Host
refusal and keep zero install authority until the complete join is accepted.

| Cell / independently controlled condition | Expected native outcome | Independent observable / fault mutant |
| --- | --- | --- |
| Ordinary non-elevated Medium primary token, no restrictions | **Read-only snapshot** only after the complete token profile is implemented | OS TokenStatistics LUIDs and SID from retained token agree; removing the retained-token check must make negative controls fail |
| Same logon session but different `TokenId`, `AuthenticationId` or `ModifiedId` | Refusal before the first protected phase; **sticky** after restoration | Native external test harness changes one property at a time; exact typed refusal, zero created targets |
| Impersonating current thread or foreign executing worker | Refusal even if process token appears valid; sticky after `RevertToSelf` | Observe real thread ID and `OpenThreadToken` outcome; removal of either check fails cell |
| Wrong session, elevated, Low/Medium-Plus/High, SYSTEM or service SID | Exact predicate refusal | OS source independently establishes actual token; other failing predicates must not cause a false-positive test |
| AppContainer, UIAccess, `TokenHasRestrictions`, nonempty restricted SIDs | Exact profile refusal; do not infer from absence of restricting SIDs | Separate native fixtures; flip each check independently, never make a filtered administrator a positive |
| Zero/1 MiB/max+1 byte, null/out-of-buffer/misaligned SID, truncated `TOKEN_STATISTICS` or list | Checked typed refusal before dereference or effect | Bounded crafted buffer and OS API abnormal-return coverage; no generic `.is_err()` oracle |
| `CoInitializeEx` first success `S_OK`, repeat success `S_FALSE` | Each successful call has exactly one same-thread `CoUninitialize` | Independent COM apartment reference-count probe across harness/Guard lifetime |
| Existing incompatible STA or NTA transition (`RPC_E_CHANGED_MODE`) | Refusal, **no** unmatched `CoUninitialize`, no Shell lookup | Actual per-thread COM apartment probe; no retry on hidden new worker |
| Shell failure with non-null output pointer, invalid UTF-16 or overbound length | Refusal and exactly one `CoTaskMemFree` before COM cleanup | Allocation owner leak/failure probe; independent injected failure test cannot substitute for native positive |
| LocalAppData redirected to UNC/network, cloud or remote volume | Refuse before network, repository or filesystem writes; if Shell query initiates network I/O, **fail** zero-I/O criterion | ETW kernel network/SMB + file operations correlated by process and thread; before/after network and filesystem census |
| Local drive path with reparse/symlink component or ancestor swapped after Shell result | Refusal before file effect, no path fallback | Native retained-parent no-follow/reparse oracle, recorded handle identities, created-object census |
| D90 real lease lost or a different recorded attempt used for D64/D68 fixed target | Refuse before `admissions/<recorded-attempt>.json` replacement or `pending-operation.json` terminal removal | Independent repository-owner lease trace, exact target/attempt comparison, zero file-write/delete census; no generic slot substitute |
| Any denial followed by a clean new read or COM reinitialization | Sticky denial; no permit, no retry/activation | Repeated call returns the **prior-invalidated** typed operation, zero protected I/O |

ETW trace design: correlate process ID, thread ID, source timestamp and the
known-folder call interval. Capture kernel file create/write/rename,
network/TCP, SMB client and relevant Shell/registry activity using provider
names and versions verified on that **actual Windows installation**.
Record exact ETW session manifest, drop/loss counters, filters, active
probes, before/after directory/object and network census, and stop at the
**first forbidden operation**. An empty trace with lost events or incorrect
PID/TID scope is **UNKNOWN**, never PASS. Registry/OS read-only lookups may
be expected; the prohibited observable is delegated network I/O or
unauthorized protected writes/creates before completed admission. Never
disable telemetry or use `DONT_VERIFY` to bypass a failing cell.

## Approval sequence and stopping conditions

1. Independent **Guard owner T82 technical decision** identifies each
   exact API/class, type layout and owning source file. Approval must name
   the nearest `crates/keld-guard/AGENTS.md` change and its retained
   `FFI only` exclusion. This packet is not approval. Do not force
   extra rules into the 3,840-byte always-loaded cap by deleting unrelated
   invariants, raising the cap, or introducing a parallel FFI owner.
2. Independent instruction review and its executable gates
   (`just agent-context`, `just atomic-protocol`, `just llms-test`,
   `just llms-check`, `codex debug prompt-input`) must pass on the
   actual instruction diff; include measured route/byte/token effect,
   negative controls and rollback.
3. Independent Guard unsafe + dependency review must accept
   `Win32_UI_Shell`/`Win32_System_Com`, code-pointer/lifetime proof,
   one-MiB token read and exact Windows ABI. Only then may a private,
   non-authorizing native reader be implemented in an isolated source
   child and compiled on the authorized Windows RAM/build slot.
4. Run the native test cells, actual redirected-folder ETW traces and
   independent first-write/object census; then prove full effect-time
   revalidation under the KEL-53 repository writer, plus the separate
   D64/D68 fixed-target replacement and terminal-deletion right grants.
   T81/B82, D90 completeness and D46 legitimate issuer remain separate gates.
   Do not infer release readiness from a private read-only success.

**Current resource gate:** Rushil Windows free RAM was below
`1,572,864 KiB` when this packet was prepared. Cargo and all native
acceptance cells remain **NOT RUN**; no build is allowed until the
required sustained RAM samples and exclusive slot are observed.

Official primary references, checked 2026-11-22:

- [Microsoft GetTokenInformation / TOKEN_QUERY](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation)
- [Microsoft TOKEN_STATISTICS fields and ModifiedId](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-token_statistics)
- [Microsoft Known Folders / COM calling-thread prerequisite](https://learn.microsoft.com/en-us/windows/win32/shell/known-folders)
- [Microsoft CoInitializeEx / S_FALSE / RPC_E_CHANGED_MODE](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex)
- [Microsoft SHGetKnownFolderPath / flags, NULL token, CoTaskMemFree](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath)
- [Microsoft known-folder network redirection](https://learn.microsoft.com/en-us/windows/win32/shell/working-with-known-folders)
