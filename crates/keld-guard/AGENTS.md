# keld-guard — adds root AGENTS.md

Spec: `docs/architecture/03-security.md`. Security boundary. Threat model lives in
architecture 03 (and any future dedicated crate threat-model section) — do not claim a
crate-local threat model doc exists until one is checked in.

- Default-deny: unknown cap/channel/scope, missing manifest → `Deny`. No interim allow.
- Principals host-minted, unforgeable; webview principals rotate on navigation.
- Every `DenyReason`: capability/scope + fix (manifest edit). Deny text is API — test it.
- Scope matching destination: resolve `$VARS`, symlinks, `..` before match. Bypass fixtures (traversal, symlink swap, case folding, wildcard-swallow) permanent. New matcher → adversarial tests.
- v0 exception: `$VARS` match as literals; `..` is rejected; symlink canonicalization is not in this slice. That is not an Allow. Host resolution is the destination (spec 03), not a silent weaken of default-deny.
- Wildcard grants loud in `keld doctor`. No dev-mode special-case inside engine — profile composed outside, refused in release.
- Hot path (per kipc frame): no alloc on `Allow`, no locks across handler dispatch.
- v0 public API: `parse_manifest` / `load_manifest` /
  `evaluate(manifest, principal, operation, path) -> Decision`.
  KEL-78 T1 also exports `admit(req, facts, archive) -> Result<ProfileState, AdmissionError>`.
  `HostFacts::observe_uncontained` reports every §4 primitive missing.
  Agents MUST NOT treat `ProfileState::Strict` from a test fixture catalog as
  OS containment. Direct net is OS-deny in the contract; `ConnectionRefused`
  / successful `bind` is not an OS pass. Leaving Strict is `Err` — agents
  MUST NOT start an unsandboxed KEL-70 replacement.
  Non-`AppProcess` principals are `DenyReason::NotAppProcess` (`KELD-GUARD006`)
  before grant lookup — agents MUST NOT apply `/app` scopes to a webview or
  plugin. Webview-originated media (`web.camera` / `web.microphone`) MUST
  present the requesting `Principal::Webview`. Missing identity and
  `AppProcess` are `DenyReason::MediaPrincipalRequired` (`KELD-GUARD007`).
  Missing file is `ManifestError`, not Allow.
- Tests MUST follow repository `.agents/testing.md`.

## KEL-53 T3 package-path FFI
Owner/load: guard/always; trigger `validate_windows_package_paths`. Allow only `NormalizeString` (C) and `CompareStringOrdinal` (ignore-case); use owned UTF-16, checked i32 lengths/results and inline SAFETY for bounds/lifetime. Windows tests; retire with API.

## KEL-270 T4d admin-owner token FFI
Owner/load: guard/always; trigger `require_windows_machine_uac_owner_token`. Production FFI: `GetCurrentProcess`, `GetCurrentThread`, `GetLastError`, `OpenProcessToken(TOKEN_QUERY)`, `OpenThreadToken(TOKEN_QUERY, TRUE)`, `GetTokenInformation(TokenGroups, TokenElevation)` only. Only `ERROR_NO_TOKEN` proves no impersonation; other errors refuse. Keep handles in RAII; bounds-check buffer/group/SID reads with inline SAFETY. Require elevated TokenElevation and BUILTIN Administrators `SE_GROUP_OWNER|SE_GROUP_ENABLED`, not deny-only, before creation. Tests cover SID/flags/errors and live thread impersonation via test-only `ImpersonateSelf`/`RevertToSelf`; operator tests cover elevated and filtered tokens. Retire with API.

## KEL-270 T4d S8 Authenticode FFI
Owner/load: guard/always; trigger `WindowsAuthenticodeImage`. Sole KEL-135 verifier (D4); MUST NOT be copied. `windows_authenticode.rs` only: `WinVerifyTrust` VERIFY then CLOSE on the pinned image; `WTHelperProvDataFromStateData`, `WTHelperGetProvSignerFromChain`, `WTHelperGetProvCertFromChain`, `CryptEncodeObjectEx` (SPKI) and `CryptDecodeObjectEx` (opus) before CLOSE. Bound reads; inline SAFETY. Retire with API.
