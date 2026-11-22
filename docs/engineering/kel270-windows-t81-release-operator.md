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

The separate metadata files name actual matching-region Endpoint,
CodeSigningAccountName, and CertificateProfileName. Microsoft requires a
supported SDK SignTool, matching dlib, .NET 8, and signer role access. The
owner must identify and sign every real required installer/helper executable
under its reviewed role contract. Do not invent an installer filename or
assume Host signature covers other PE files. Issue the detached catalogue only
after the final Host signature and hash, then freeze and attest every release
artifact and its signer evidence. The command is not executed by this handoff.

## Independent public read-only preflight

Use only OWNER-PINNED hashes/OIDs. This reads file signatures and role EKUs;
it does not prove Keld native retained-handle signature binding, full
issuer/root cohorts, revocation, or installed acceptance:

~~~powershell
powershell.exe -NoProfile -NonInteractive -File docs/engineering/evidence/kel270-catalogue-admission-v2/t81_public_image_preflight.ps1 -ApplicationHost '<original signed app Host.exe>' -ApplicationSha256 '<independent SHA256>' -ApplicationProfileOid '<commissioned application OID>' -RecoveryHost '<original signed recovery Host.exe>' -RecoverySha256 '<independent SHA256>' -RecoveryProfileOid '<commissioned recovery OID>'
~~~

Only the marker T81_PUBLIC_PREFLIGHT_ONLY_NOT_RELEASE_AUTHORITY can appear on
a successful preliminary check. It cannot mean T81 or release PASS.

## Isolated ordinary SID native acceptance

After authentic artifacts and owner review, log in INTERACTIVELY as a fresh
separate standard Windows user with an independent user SID and disposable
profile. Do not run against or change the existing shared Admin installation.
Run this read-only identity precheck in the ordinary-user session:

~~~powershell
$Sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$Principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
$SharedAdminSid = '<independently recorded shared Admin SID>'
if ($SharedAdminSid -notmatch '^S-1-' -or $Sid -eq $SharedAdminSid) { throw 'Ordinary SID is not distinct' }
if ($Principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Administrative token refused' }
Write-Output "T81_ORDINARY_SID_OBSERVED sid=$Sid profile=$env:USERPROFILE"
~~~

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
