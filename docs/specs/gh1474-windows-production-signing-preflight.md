# Spec: Windows production signing identity preflight

- Status: approved
- Tracker: GH-1474
- Owner: KELD build maintainers
- Updated: 2026-11-13
- Pin: 151776c72f3615f6c87d8a03ccbab00a0c08e955

## 1. Goal & non-goals

The release process needs to check public signing inputs before attempting
production signing. This approved scope covers **only a read-only preflight**.
The owner authorized subsequent production-build implementation on 2026-11-13;
that authorization is not a signed, timestamped, or release-qualified image.

The diagnostic MUST reject test-only or untrusted identities. It MUST NOT
modify keys, certificates, trust stores, signed artifacts or publications.
Successful preflight MUST NOT be interpreted as production-release approval.

Non-goals: executable signing, timestamping, publishing, release authorization,
certificate download or import, trust installation, or provider-private-key
handling. No change to installed Host, installer, draw.io, or WebView2.

## 2. Spec refs

- GH-1474 owns the Windows source-build and signing-preflight boundary.
- `docs/architecture/05-webview-and-native.md` covers signed Windows identity.
- `crates/keld-guard/src/windows_authenticode.rs` owns verified WinTrust
  code identity and publisher scope; this diagnostic does not replace it.
- Root `AGENTS.md` owns the atomic reasoning and security-review rules.
- `tools/windows_production_signing_preflight.ps1` implements this preflight.

## 3. Acceptance criteria

### AC-1: Syntax-only preflight cannot qualify a release

- **Given:** An exact local UTF-8 policy with the eight permitted string fields.
- **When:** The diagnostic runs with `-SyntaxOnly`.
- **Then:** Exit zero reports `productionReleaseQualified=false`, without
  reading private keys, signing, timestamping, or changing machine trust.
- **Negative control:**
  - **Single fault:** Remove the `productionReleaseQualified=false` result.
  - **At-risk precondition:** The closed policy passes syntax validation.
  - **Expected RED:** The syntax-only child receipt no longer contains the
    required false result.

### AC-2: Unreviewed policy properties and encodings refuse

- **Given:** A closed flat production-signing policy with one value per key.
- **When:** A duplicate, unknown, secret-bearing, malformed or escaped JSON
  property appears before native certificate lookup.
- **Then:** The diagnostic exits nonzero with its precise refusal. Duplicate
  and private-key properties produce `PROD-PRE-009`; escaped JSON produces
  `PROD-PRE-008`.
- **Negative control:**
  - **Single fault:** Accept a duplicate decoded `sourceCommit` key.
  - **At-risk precondition:** The policy otherwise has valid bounded fields.
  - **Expected RED:** `duplicate-policy-key-rejected` no longer observes
    `PROD-PRE-009` with exit code 1.

### AC-3: Identity and timestamp inputs must match the closed policy

- **Given:** A strict policy and an independently selected source commit.
- **When:** The source SHA differs, the timestamp URL is insecure or ambiguous,
  the signing identity is absent, or a tool hash differs.
- **Then:** The diagnostic refuses with a nonzero exit and its corresponding
  typed code. It cannot select an unsigned or unpinned signer.
- **Negative control:**
  - **Single fault:** Permit a plain HTTP timestamp service.
  - **At-risk precondition:** The other seven policy fields remain valid.
  - **Expected RED:** `non-https-timestamp-rejected` no longer observes
    `PROD-PRE-020` with exit code 1.

### AC-4: Native certificate and tool availability require real trust

- **Given:** One exact installed signing certificate and an explicitly pinned
  native SignTool binary, on a qualified Windows test system.
- **When:** The diagnostic performs its native availability preflight.
- **Then:** It checks the selected tool SHA-256 and Authenticode signature.
  The selected certificate must have a private-key reference, be non-self-issued,
  have Code Signing EKU, and build a revocation-checked chain to the named root.
- **Negative control:**
  - **Single fault:** Admit a self-issued TEST publisher as production identity.
  - **At-risk precondition:** That certificate and the selected SignTool are
    available on the native test machine.
  - **Expected RED:** `self-signed-test-publisher-rejected` no longer observes
    `PROD-PRE-027` with exit code 1.

### AC-5: A successful native check remains read-only

- **Given:** A policy and tool/certificate pair that pass native preflight.
- **When:** The diagnostic reports the result.
- **Then:** No image is signed and no RFC3161 request is submitted.
  `productionReleaseQualified` stays false; actual production release still
  requires independently approved signing, countersignature and provenance.
- **Negative control:**
  - **Single fault:** Change the successful result to claim release qualification.
  - **At-risk precondition:** A native preflight reaches the result emitter.
  - **Expected RED:** The result's false-release flag and no-side-effect
    check fail, even though certificate availability passed.

## 4. Design

Trust boundaries: the caller selects only a **public** policy, an expected source
commit and an existing SignTool binary. The policy is not release approval.
An approved release-policy signature, real production certificate provisioning,
RFC3161 countersignature verification and exact source/toolchain provenance
remain independent security gates.

The exact schema `keld.windows.production-signing-policy/v1` has eight flat
string fields: `schema`, `sourceCommit`, `publisherName`,
`publisherCertificateThumbprint`, `trustedRootThumbprint`,
`rfc3161TimestampUrl`, `signerToolSha256` and
`updatePublicKeySha256`. No credential fields or fallback paths exist.

The owner is a cold PowerShell diagnostic invoked with
`-NoProfile -NonInteractive` and an absolute local input path. It pins the
PowerShell PATH and module lookup roots before resolving cmdlets.
Its native grants are read-only certificate and file access by a non-elevated
process. No persistent output, new type, channel or wire protocol is needed.

No existing caller changes. The one-shot signed LOCAL TEST route and existing
receipts remain unchanged. Future production actions MUST independently obtain
authorized release policy and a verified signed timestamp receipt.

## 5. Boundaries

Implementation belongs to `tools/windows_production_signing_preflight.ps1`.
The independent harness is
`tools/test_windows_production_signing_preflight.ps1`.
Tests use a private D: fixture on the authorized Windows machine.
The existing TEST certificate serves only as a refusal control.

MUST NOT alter `crates/keld-cli/src/build/windows_product.rs`,
`local_images.rs`, private keys, trust stores, installer, installed Host,
shared Cargo cache, old evidence or release publications.

## 6. Tasks

### T-1: Implement the read-only signing-preflight boundary

- **Blocked by:** none
- Completed implementation scope: strict policy shape, exact source/tool
  identity, native certificate availability and trusted-chain checks.
- No release side effect or private-key import is part of this task.

### T-2: Prove native preflight refusals independently

- **Blocked by:** T-1
- The source test matrix covers duplicate keys, extra private fields, wrong
  source, insecure timestamp, wrong SignTool hash, unavailable identity and
  the self-issued TEST certificate.
- It also covers reparse, Win32 path aliases and hidden publisher controls.

### T-3: Complete independent review and CI integration

- **Blocked by:** T-1, T-2
- Review the exact source, native Windows results and existing fail-closed
  production mode. Do not infer a production signer, signed release or
  installed-runtime qualification from T-1/T-2.

## 7. Test plan & evidence

On native Windows, run:

```powershell
powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File tools/test_windows_production_signing_preflight.ps1
```

The harness starts separate child processes and asserts exit codes and typed
refusal text. AC-1 uses `syntax-check-never-grants-release`; AC-2 uses
`duplicate-policy-key-rejected`, `private-key-property-rejected` and
`unreviewed-json-escape-rejected`. AC-3 uses the source and timestamp
negative controls, plus signer-dependent checks when a pinned tool is available.
AC-4 requires the native certificate/signer controls. If the TEST certificate
or SignTool is absent, the harness reports those cases as **not run**, not pass.

AC-5 additionally requires inspection that no `signtool sign`, timestamp
request, certificate creation or trust mutation occurs. A native preflight
success is never an image-signing or installed Host acceptance receipt.

The test-only fixture MUST NOT modify existing certificates or previous
signed LOCAL TEST evidence. Production signing and RFC3161 verification
require separate authenticated release evidence.

## 8. Review gates triggered

Independent Windows signing-authority, certificate-chain, PowerShell module
resolution, release-policy and product-security review are required.
No new unsafe Rust, dependency, wire protocol or permission grant is proposed.

## 9. Perf impact

Cold read-only diagnostic, with bounded certificate-chain URL retrieval.
No Host steady-state work changes. A provider-backed production trust workflow
would need its own performance and timeout verification.

## 10. Open questions, blockers & unknowns

None for the read-only preflight slice. The real production signer provider,
approved issuer/root, release authority, protected timestamp workflow,
signed-image verification and installed-app acceptance remain **separate**
release gates. This spec neither supplies nor infers those approvals.
