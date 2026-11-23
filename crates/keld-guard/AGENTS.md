# keld-guard — adds root AGENTS.md

Spec/threat: `docs/architecture/03-security.md`. No local doc yet.

- Default-deny: unknown cap/channel/scope, missing manifest → `Deny`. No interim allow.
- Host-mint unforgeable principals; rotate webview principals on navigation.
- `DenyReason`: capability/scope + manifest fix; test denial text as API.
- Scope: resolve `$VARS`, symlinks, `..` before match; retain traversal, symlink-swap, case-fold and wildcard-swallow bypass tests. Test new matchers adversarially.
- v0: `$VARS` literal, `..` rejected, symlinks not canonicalized. No implied Allow; host resolves destination per spec 03.
- Warn on wildcard grants in `keld doctor`; compose dev profile outside engine, refuse in release.
- Hot kipc `Allow`: no alloc; no dispatch-held locks.
- v0 API: `parse_manifest`/`load_manifest`/`evaluate(manifest, principal, operation, path) -> Decision`; KEL-78 T1 `admit(req, facts, archive) -> Result<ProfileState, AdmissionError>`. `HostFacts::observe_uncontained` reports all missing §4 primitives. MUST NOT equate fixture `ProfileState::Strict` with OS containment; direct net MUST be OS-deny. `ConnectionRefused`/successful `bind` are not OS passes. Leaving Strict is `Err` — MUST NOT start unsandboxed KEL-70. Before lookup non-`AppProcess` MUST get `DenyReason::NotAppProcess` (`KELD-GUARD006`); MUST NOT apply `/app` scopes to webview/plugin. `web.camera`/`web.microphone` MUST use requester `Principal::Webview`; no identity or `AppProcess` MUST get `DenyReason::MediaPrincipalRequired` (`KELD-GUARD007`). Missing file MUST be `ManifestError`, not Allow.
- Tests MUST follow `.agents/testing.md`.

## KEL-53 T3 package-path FFI
Owner/load: guard/always; trigger `validate_windows_package_paths`. Allow only `NormalizeString` (C) and `CompareStringOrdinal` (ignore-case); use owned UTF-16, checked i32 lengths/results and inline SAFETY for bounds/lifetime. Windows tests; retire with API.

## KEL-270 T4d token FFI
Owner/load: guard/always; trigger `{uac_token,initiating_token,logon_session}.rs`. Production FFI only: `GetCurrentProcess`, `GetCurrentThread`, `GetLastError`, `OpenThreadToken(QUERY, TRUE)`; own token: `OpenProcessToken(QUERY)`, `GetTokenInformation(TokenGroups, TokenElevation, TokenSessionId, TokenPrivileges)`, `LookupPrivilegeValueW`; host token: `OpenProcessToken(QUERY|DUPLICATE|ASSIGN_PRIMARY)`, `DuplicateTokenEx(NULL, SecurityImpersonation)` (primary same rights, impersonation `IMPERSONATE`), `SetThreadToken(NULL)`, `RevertToSelf`; `LsaGetLogonSessionData`, `LsaFreeReturnBuffer`. Only `ERROR_NO_TOKEN` proves no impersonation, only `STATUS_NO_SUCH_LOGON_SESSION` no session. RAII handles/revert (failure aborts); bound reads; inline SAFETY. Installer: elevated, BUILTIN Administrators `SE_GROUP_OWNER|SE_GROUP_ENABLED`, not deny-only, before creation. Retire with API.

## KEL-270 D4/D45 DEV Authenticode FFI
Owner/load: guard/always; trigger `WindowsAuthenticodeImage`; sole KEL-135 verifier `windows_authenticode.rs`. Only original retained-hFile `WinVerifyTrust` VERIFY/CLOSE all paths; live `WTHelperProvDataFromStateData`,`WTHelperGetProvSignerFromChain`,`WTHelperGetProvCertFromChain`,`CryptEncodeObjectEx` SPKI,`CryptDecodeObjectEx` Opus/DEV leaf `X509_ENHANCED_KEY_USAGE`. DEV build-pinned (provider,root)=(2,3) ONLY: `WTD_REVOKE_NONE`,`WTD_REVOCATION_CHECK_NONE|WTD_CACHE_ONLY_URL_RETRIEVAL`; no bypass/online retry. Before CLOSE: bounded selected-primary leaf EKUs/DER/SPKI, root DER, issuer SPKI, leaf/CA present-time validity and timestamp/countersigner. Untimestamped, unexpired only; unknown denies. Independent APP/RECOVERY role+CA+leaf hashes, signed app ID; one primary/zero secondary; no Public Trust/any-EKU or role overlap. No permit, path reopen or verifier copy. Full/v1/preflight unchanged; bounds, inline SAFETY. Retire with API.
