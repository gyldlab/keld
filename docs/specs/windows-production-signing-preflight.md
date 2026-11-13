# Spec: Windows production signing identity preflight

Status: approved (read-only preflight slice only; user authorized production-build implementation 2026-11-13)
Linear: GH-1474 | Owner: KELD build maintainers | Updated: 2026-11-13

## 1. Goal and non-goals

The release process needs to check public signing inputs before it attempts
production signing. The new diagnostic MUST reject test-only and untrusted
identities without modifying keys, certificates, trust stores, artifacts or
release publications. Its success MUST NEVER mean a production build is signed,
timestamped or ready to ship.

Non-goals: signing executables; timestamping; publishing packages; authorizing
a release; downloading a certificate; installing trust; changing the installed
Host, installer, draw.io or WebView2; accepting a provider-specific private key.

## 2. Spec refs

- `docs/architecture/05-webview-and-native.md`: signed Windows image identity.
- `crates/keld-guard/src/windows_authenticode.rs`: verified code identity and
  publisher scope; it is the sole native WinTrust owner.
- GH-1474: unsigned Windows source-build components and dependent draft PR.
- The repository's `AGENTS.md` atomic and security review rules.

## 3. Acceptance criteria

1. Given a closed, bounded, strict UTF-8 flat policy, the diagnostic checks
   syntax without reading a private key, changing machine trust or signing.
2. Duplicate, unknown, secret-bearing, malformed or escaped JSON properties
   MUST be refused before the native certificate store is used.
3. An insecure or ambiguous timestamp URL, source SHA mismatch, invalid tool
   hash, missing signing identity or self-issued TEST certificate MUST refuse
   with a diagnostic code and a nonzero process exit.
4. A native signer availability check MUST require one exact installed
   code-signing certificate with a private-key reference, a non-self-issued
   leaf, the Code Signing EKU, a revocation-checked trusted chain and the
   policy's named root. It MUST pin the selected native SignTool binary by
   SHA-256 and verify that tool's Authenticode signature.
5. The diagnostic MUST NOT sign an image or call the timestamp service. It
   MUST report `productionReleaseQualified=false` even if preflight passes.
6. The existing one-shot signed LOCAL TEST command MUST remain unchanged and
   its old evidence MUST remain immutable.

## 4. Design

Trust boundaries: caller selects a public-only policy, an expected source
commit and a SignTool binary. The selected policy is not itself a release
approval. A real approved release policy signature, trusted production
certificate provisioning, RFC3161 countersignature verification and exact
source/toolchain provenance remain separate security owners.

Schema `keld.windows.production-signing-policy/v1` has exactly eight flat
string values: `schema`, `sourceCommit`, `publisherName`,
`publisherCertificateThumbprint`, `trustedRootThumbprint`,
`rfc3161TimestampUrl`, `signerToolSha256` and
`updatePublicKeySha256`. There are no credential fields or fallback paths.

New or changed types/channels: none. OS grants: read-only certificate and
file access by a non-elevated process. Wire/protocol changes: none.

Runtime seam: separate cold PowerShell diagnostic executed with
`-NoProfile -NonInteractive`. It accepts an exact absolute local path;
it cannot mutate the CLI's local-test signer. The diagnostic also pins its
PowerShell PATH and module search roots before cmdlets are resolved. No
persistent state is produced.

Migration: not required. No existing caller is modified. A later production
build action MUST independently require an authorized release policy and a
real signed timestamp receipt, without treating this preflight as authority.

## 5. Boundaries

Implement in `tools/windows_production_signing_preflight.ps1` and
`tools/test_windows_production_signing_preflight.ps1`. Tests use only a
private D: fixture and the existing TEST certificate for a refusal control.

Must not modify: `crates/keld-cli/src/build/windows_product.rs`,
`local_images.rs`, production signature trust, installer, Host, shared Cargo
cache, old evidence, existing private keys or system trust stores.

## 6. Tasks

- [x] T1: Implement strict policy shape, signer/tool availability and native
  certificate-chain checks without any release side effect.
- [x] T2: Execute independent Windows negative controls for duplicate keys,
  extra private fields, wrong source, missing/insecure timestamp, wrong
  SignTool hash, absent publisher and self-issued TEST publisher.
- [ ] T3: Publish for independent review and gate integration. Do not declare
  production readiness based on T1/T2.

## 7. Test plan

T1/T2: `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass
-File tools/test_windows_production_signing_preflight.ps1` on native Windows.
The test harness executes the diagnostic in child processes and asserts exact
exit codes and diagnostics. It MUST NOT call `signtool sign` or create a
certificate. The known current-user self-issued TEST certificate is a
failure-first negative control, not a production signing substitute.

T3: independent code review, hosted CI and check of existing fail-closed
production mode. The actual private production signer and RFC3161 signature
must be tested separately once explicitly qualified.

## 8. Review gates triggered

Windows signing authority, certificate-chain validation, release policy,
non-elevated PowerShell module resolution and product-security review.
No new dependency, unsafe Rust, permission model or wire protocol.

## 9. Performance impact

Cold read-only diagnostic; no Host runtime impact. No benchmark necessary
unless a provider-backed trust check is later added. URL retrieval is bounded.

## 10. Open questions

None for this preflight-only slice. The genuine production signer provider,
issuer/root policy, release authority and protected timestamp workflow are
independent release-approval gates and are NOT supplied or inferred here.
