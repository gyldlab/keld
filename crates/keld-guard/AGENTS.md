# keld-guard — adds root AGENTS.md

Threat: `docs/architecture/03-security.md`; no local doc.

- Unknown cap/channel/scope/missing manifest → `Deny`; never Allow.
- Host-mint unforgeable principals; rotate webviews on navigation.
- `DenyReason`: cap/scope + manifest fix; test API text.
- Resolve `$VARS`/symlinks/`..` before scope match; keep traversal/symlink-swap/case-fold/wildcard-swallow bypass tests; adversarial-test new matchers.
- v0: literal `$VARS`, reject `..`, no symlink canonicalization/Allow; host resolves destination (spec 03).
- `keld doctor` warns on wildcard; dev profile outside engine, refused in release.
- Hot kipc `Allow`: no alloc/dispatch-held locks.
- v0: `parse_manifest`/`load_manifest`/`evaluate(manifest,principal,operation,path)->Decision`; T1 `admit(req,facts,archive)->Result<ProfileState,AdmissionError>`. HostFacts::observe_uncontained lists missing §4. Fixture ProfileState::Strict is no OS containment; direct net MUST OS-deny (ConnectionRefused/successful bind not proof). Exit Strict is Err; MUST NOT start unsandboxed KEL-70. Non-AppProcess MUST get DenyReason::NotAppProcess (KELD-GUARD006) before grants; no webview/plugin `/app`. web.camera/web.microphone requires requester Principal::Webview; missing/AppProcess gets DenyReason::MediaPrincipalRequired (KELD-GUARD007). Missing file ManifestError, not Allow.
- Tests MUST follow `.agents/testing.md`.

## KEL-53 T3 package-path FFI
Owner/load: guard/always; trigger `validate_windows_package_paths`. FFI ONLY `NormalizeString` (C),`CompareStringOrdinal` (ignore-case); owned UTF-16, checked i32 lengths/results, inline SAFETY for bounds/lifetime. Windows tests; retire with API.

## KEL-270 T4d token FFI
Owner/load: guard/always; trigger `{uac_token,initiating_token,logon_session}.rs`. Production FFI ONLY: GetCurrentProcess,GetCurrentThread,GetLastError,OpenThreadToken(QUERY,TRUE); own OpenProcessToken(QUERY),GetTokenInformation(TokenGroups,TokenElevation,TokenSessionId,TokenPrivileges),LookupPrivilegeValueW; host OpenProcessToken(QUERY|DUPLICATE|ASSIGN_PRIMARY),DuplicateTokenEx(NULL,SecurityImpersonation) (primary same rights; impersonation IMPERSONATE),SetThreadToken(NULL),RevertToSelf; LsaGetLogonSessionData,LsaFreeReturnBuffer. ERROR_NO_TOKEN alone proves no impersonation; STATUS_NO_SUCH_LOGON_SESSION alone proves no session. RAII handles/revert; abort on failure; bounded reads, inline SAFETY. Installer: elevated BUILTIN Administrators SE_GROUP_OWNER|SE_GROUP_ENABLED, not deny-only, precreation. Retire with API.

## KEL-270 D4/D45 DEV FFI
Owner/load: guard/always; trigger `WindowsAuthenticodeImage`. sole KEL-135 `windows_authenticode.rs`, no copies: original hFile `WinVerifyTrust` VERIFY/CLOSE all paths; only `WTHelperProvDataFromStateData`,`WTHelperGetProvSignerFromChain`,`WTHelperGetProvCertFromChain`,`CryptEncodeObjectEx` SPKI,`CryptDecodeObjectEx` Opus/leaf `X509_ENHANCED_KEY_USAGE`; bounds/inline SAFETY. DEV build-pinned provider2/root3 ONLY: `WTD_REVOKE_NONE`+`WTD_REVOCATION_CHECK_NONE|WTD_CACHE_ONLY_URL_RETRIEVAL`; no bypass/network. Before CLOSE: 1 primary/0 secondary; bounded leaf/root/issuer DER/SPKI+EKUs, CodeSigning EKU exactly once, no PublicTrust/anyEKU; leaf+CA valid now, no timestamp/countersigner; unknown denies. SAME retained signed hFile: authenticated canonical Pack DEV D-WIRE `.keldea2` all D4: appId, APP SPKI, repoId, issuer/repo/discovery policy SHA, channel/target MUST equal independent immutable build/installed cohort + signer before trust/D46. Authenticated operation selects distinct APP/RECOVERY CA+leaf cert/SPKI pins; RECOVERY needs separately signed `.keldrec`/RecoveryPolicy same hFile. Deny missing DEV Pack producer/cohort/record, mismatch, caller role/path/hash/env, preflight/legacy fallback. No D46 permit/reopen; Full/v1/preflight unchanged. Retire with API.
