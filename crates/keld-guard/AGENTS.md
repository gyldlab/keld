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
Owner/load: guard/always;trigger WindowsAuthenticodeImage. KEL-135 windows_authenticode.rs ONLY: original hFile WinVerifyTrust VERIFY/CLOSE all paths (status=0);WTHelperProvDataFromStateData,WTHelperGetProvSignerFromChain,WTHelperGetProvCertFromChain,CryptEncodeObjectEx SPKI,CryptDecodeObjectEx Opus/X509_ENHANCED_KEY_USAGE;inline SAFETY, no copy. DEV build-pinned(2,3): WTD_REVOKE_NONE+WTD_REVOCATION_CHECK_NONE|WTD_CACHE_ONLY_URL_RETRIEVAL;no bypass/net. Before CLOSE 1 primary/0 secondary, bound leaf/root/issuer DER/SPKI/EKUs;CodeSigning EKU exactly once, no PublicTrust/anyEKU;leaf+CA valid now, no timestamp/countersigner. Pack parses UNTRUSTED .keldea2 on SAME hFile;ENTIRE canonical D4 incl updatePublicKeyEd25519,fixed owner/installMode/principalModel/provider/rootPolicy == independent immutable trusted build/installed D4 pre-trust/D46. Authenticated op picks APP/RECOVERY distinct leaf cert/SPKI;pinned CA/issuer may share. RECOVERY D4 names APP SPKI;signer MUST be RECOVERY leaf. .keldrec/.keldea2 SAME-file Authenticode;verify RecoveryPolicy/role independently, not detached. DENY unknown/missing Pack DEV producer/cohort/sections, mismatch, caller role/path/hash/env, preflight/legacy fallback;no D46 permit/reopen;Full/v1/preflight unchanged;retire with API.
