# KEL-270 T81: Windows signer commissioning and acceptance handoff

Source: Draft PR #1593 at 4b064e8851d4b2112231bfdb26414f7562ae9d6c.
This is an operator handoff, not specification approval, signing authority,
or a positive release receipt. The release owner must first satisfy T80/T81.

## Responsible owner and required evidence

MAINTAINERS.md:29-35 names @0monish the release coordinator;
MAINTAINERS.md:8-12 and .github/CODEOWNERS name the
@gyldlab/keld-maintainers team for independent reviews. Neither document
identifies the Azure tenant administrator or grants Artifact Signing roles.

The real Microsoft Artifact Signing account owner commissions TWO distinct
Public Trust certificate profiles: application and recovery-only. The Azure
resource owner records the exact full unique profile EKU OIDs, validated
organization, tenant/account/profile resource IDs, signing role membership,
actual access controls, and audit entries. Recovery credentials and access
must be separate from application, TUF, catalogue, and installed U1 signing.
A different label without distinct access control is insufficient.
Never create a substitute self-signed certificate or import new local trust.

For BOTH profiles the release owner records real PublisherPolicy and IssuerPolicy
bytes, including provider microsoft-artifact-signing-public-trust, the complete
profile OID, windows-generic-verify-v2, issuer-policy SHA256, 1..8 trusted
rootCertificateSha256 hashes and 1..8 leafIssuerSpkiSha256 hashes from the
same Windows-verified primary signer. Both signed leaves need Code Signing EKU
1.3.6.1.5.5.7.3.3 and Public Trust EKU 1.3.6.1.4.1.311.97.1.0.
Application and recovery OIDs must be mutually absent from the opposite role.
Record two genuine signed app artifacts with changed leaf SPKI and constant
application profile OID; likewise two real recovery leaves with changed SPKI
and constant recovery profile OID. A leaf thumbprint is not durable identity.

Also commission one release-wide channel, canonical app ID, repository ID,
bootstrap metadata root/hash/version, catalogue public key IDs and public
keys, metadata/targets URLs, discovery and recovery policy digests, trusted
clock, and the separate genuine HTTPS recovery origin. Record artifact
SHA256/length, exact source and build inputs, publisher/issuer policies,
role-specific signing audit and independent release-approval provenance.
Never use candidate files to invent or select these expected values.

Official current Microsoft procedures:

- https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart
- https://learn.microsoft.com/en-us/azure/artifact-signing/tutorial-assign-roles
- https://learn.microsoft.com/en-us/azure/artifact-signing/concept-certificate-management
- https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-signing-integrations

Identity verification requires the applicable provider validation; Microsoft
assigns the Identity Verifier and Certificate Profile Signer roles separately.
The Artifact Signing service holds private keys. The release team never gives
keys to a reviewer or moves them into Windows certificate stores.

## Signing owner: executable command after commissioned authority exists

Build and independently review the exact Windows PE images first. Embed the
application .keldea2 section before signing; embed recovery-only .keldrec
before final .keldea2 in the dedicated Recovery Host before signing. The
native spec requires one primary signature and zero secondary signatures.
These commands sign ONLY reviewed, approved unsigned artifacts using the
real associated Azure profile metadata; the signer runs them in the authorized
release-owner signing environment, not on shared Admin or a development fixture.

~~~powershell
$SignTool = '<approved Windows SDK x64 signtool.exe>'
$Dlib = '<approved x64 Azure.CodeSigning.Dlib.dll>'
$AppMetadata = '<actual application profile metadata.json>'
$RecoveryMetadata = '<actual recovery profile metadata.json>'
$UnsignedAppHost = '<approved unsigned application Host .exe>'
$UnsignedRecoveryHost = '<approved unsigned recovery Host .exe>'
& $SignTool sign /v /debug /fd SHA256 /tr 'http://timestamp.acs.microsoft.com' /td SHA256 /dlib $Dlib /dmdf $AppMetadata $UnsignedAppHost
if ($LASTEXITCODE -ne 0) { throw 'App sign failed' }
& $SignTool sign /v /debug /fd SHA256 /tr 'http://timestamp.acs.microsoft.com' /td SHA256 /dlib $Dlib /dmdf $RecoveryMetadata $UnsignedRecoveryHost
if ($LASTEXITCODE -ne 0) { throw 'Recovery sign failed' }
& $SignTool verify /v /pa /all $UnsignedAppHost
if ($LASTEXITCODE -ne 0) { throw 'App VerifyTrust failed' }
& $SignTool verify /v /pa /all $UnsignedRecoveryHost
if ($LASTEXITCODE -ne 0) { throw 'Recovery VerifyTrust failed' }
~~~

The two SignTool verification commands are independent diagnostics.
The /pa /all option does **not** prove one-primary/zero-secondary signature
cardinality, that the inspected signer is the sole approved KELD profile,
or that the exact image bytes were checked through a retained original hFile.
The native D45 verifier must enforce these properties on its retained handle.
Until then, signature cardinality, commissioned role, root certificate and
leaf-issuer SPKI cohorts remain UNKNOWN. A valid Windows signature alone
does not commission any KELD publisher or grant a recovery role.

The separate metadata files name actual matching-region Endpoint,
CodeSigningAccountName, and CertificateProfileName. Microsoft requires a
supported SDK SignTool, matching dlib, .NET 8, and signer role access. The
owner must identify and sign every real required installer/helper executable
under its reviewed role contract. Do not invent an installer filename or
assume Host signature covers other PE files. Issue the detached catalogue only
after the final Host signature and hash, then freeze and attest every release
artifact and its signer evidence. The command is not executed by this handoff.

## Independent public read-only preflight

Use only OWNER-PINNED hashes/OIDs. This script makes **separate, unbound
path observations**. Get-FileHash A, Get-AuthenticodeSignature B and
Get-FileHash A can observe different underlying files through the same
mutable path, even when the first and last digest match. The SHA256
observation never authenticates the separately observed signature.
Treat every mutable or untrusted path as entirely nonauthorizing. File
authenticity and identity require D45's native retained original-hFile
verification. Windows Authenticode status and selected leaf EKUs do not
establish a commissioned role, approved root/issuer-SPKI cohorts, exact
signature cardinality, native chain/revocation rules or installed acceptance:

~~~powershell
powershell.exe -NoProfile -NonInteractive -File docs/engineering/evidence/kel270-catalogue-admission-v2/t81_public_image_preflight.ps1 -ApplicationHost '<original signed app Host.exe>' -ApplicationSha256 '<independent SHA256>' -ApplicationProfileOid '<commissioned application OID>' -RecoveryHost '<original signed recovery Host.exe>' -RecoverySha256 '<independent SHA256>' -RecoveryProfileOid '<commissioned recovery OID>'
~~~

The script prints separate T81_RAW_PATH_HASH_UNBOUND and
T81_RAW_PATH_SIGNATURE_UNBOUND observations and
T81_NATIVE_D45_ORIGINAL_HANDLE_REQUIRED with
root_issuer_spki=UNKNOWN, signature_cardinality=UNKNOWN and
commissioned_role=UNKNOWN. Its final
T81_PATH_OBSERVATIONS_ONLY_UNBOUND_NONAUTHORIZING marker conveys no
release status. Even an exit code of zero does not qualify T81, an artifact
or a positive Windows release. Subsequent native D45 identity, root/issuer,
cardinality and role checks are mandatory on original retained handles.

Run the read-only PowerShell 5.1 negative controls separately:

~~~powershell
powershell.exe -NoProfile -NonInteractive -File docs/engineering/evidence/kel270-catalogue-admission-v2/t81_public_image_preflight_test.ps1 -Case all
~~~

This uses nonexistent file names to prove canonical OID refusal before I/O
and checks the two documentation/source claims. It reports
T81_NEGATIVE_NOT_RUN for the optional local-test Authenticode case unless
the operator supplies a real existing, separately retained LocalTest Host
through -LocalTestFixture. No fixture or test may produce commissioning
authority, and no local certificate should be created to make it pass.

## Isolated ordinary SID native acceptance

After authentic artifacts and owner review, the Windows operator must use
a separately commissioned disposable standard-user ACCOUNT with its own
SID and owner-private profile. Do not run against or change the shared Admin
installation. A filtered Admin token can report a disabled administrator
role while retaining the SAME administrator account SID. It is not an
ordinary-user acceptance account. Run this read-only **raw token snapshot**
in the candidate session:

~~~powershell
# Absolute whoami /all output includes user SID, group SIDs, attributes and
# deny-only entries. The snapshot gives no identity or acceptance grant.
& (Join-Path $env:SystemRoot 'System32\whoami.exe') /all
if ($LASTEXITCODE -ne 0) { throw 'Raw Windows token snapshot failed' }
Write-Output 'T81_RAW_TOKEN_SNAPSHOT_ONLY_NOT_SID_ACCEPTANCE'
~~~

An independent Windows operator must compare the raw account SID with the
separately recorded shared Admin account SID, confirm the new account's
local SAM/LSA and nested account membership, and assess all group attributes
including disabled or deny-only Administrators SIDs. The operator also
verifies the disposable LocalAppData root and profile owner. No output from
this snippet alone qualifies the account as ordinary; filtered Admin tokens
and shared Admin profile reuse must be rejected before starting an installer.

The current GH894 command is executable only with its required independent
pins, but it proves the legacy signed-installed KEL135/KEL254 path, not
complete KEL270 v2 authority:

~~~powershell
powershell.exe -NoProfile -NonInteractive -File tools/windows_signed_installed_acceptance.ps1 -SignedHost '<approved original signed app Host>' -ExpectedReleaseSha256 '<release SHA256>' -ExpectedCertificateThumbprint '<actual signed leaf thumbprint>' -ExpectedAppId '<commissioned app ID>' -ExpectedPublisherScope '<independent verified publisher scope>' -CoreAcceptanceExe '<pinned native core libtest exe>' -ExpectedCoreAcceptanceSha256 '<core exe SHA256>' -UpdateInstallerExe '<pinned native keld-update libtest exe>' -ExpectedUpdateInstallerSha256 '<update exe SHA256>' -HostAcceptanceExe '<pinned native host libtest exe>' -ExpectedHostAcceptanceSha256 '<host exe SHA256>'
~~~

T82 must reconcile this GH894 tool's legacy leaf-SPKI-based publisher scope
with the v2 complete profile-OID-derived scope. Then the KEL135/Guard owner
must validate original retained handles and native chain/issuer/role policies.
Run real v2 installer, PerUserDirect owner-private root, native WebView2/Bun,
profile/restart/rollback and recovery AC66-68, and the full T83 joined matrix
only under a disposable ordinary-user SID. Retain all exact source, signed
file hashes, device/SID, native results, mutants, restored state, and final
cleanup evidence. T83 cannot be marked passed from GH894 or this document.

Source requirements: implementation.md:189-227, identity.md:42-120,
recovery.md:24-83, requirements.md:394-412, and integration.md:303-320
under docs/specs/kel270-catalogue-admission-v2.
