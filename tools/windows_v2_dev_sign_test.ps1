# Explicit native operator test, not an installed product or trust qualification.
# Uses an independently pinned unsigned debug fixture. No trust-store imports.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$UnsignedFixture,
    [Parameter(Mandatory)][string]$FixtureSha256,
    [Parameter(Mandatory)][string]$EvidenceDirectory
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$scriptPath = Join-Path $PSScriptRoot 'windows_v2_dev_sign.ps1'
$pin = $FixtureSha256.ToUpperInvariant()
if ($pin -cnotmatch '^[0-9A-F]{64}$' -or
    (Get-FileHash -LiteralPath $UnsignedFixture -Algorithm SHA256).Hash -cne $pin -or
    (Test-Path -LiteralPath $EvidenceDirectory)) { throw 'invalid fixture pin or existing evidence directory' }
New-Item -ItemType Directory -Path $EvidenceDirectory -ErrorAction Stop | Out-Null
$label = 'Keld DEV helper test ' + [Guid]::NewGuid().ToString('N')
$owned = @{}
$trustPaths = @('Cert:\CurrentUser\Root', 'Cert:\CurrentUser\TrustedPublisher',
    'Cert:\LocalMachine\Root', 'Cert:\LocalMachine\TrustedPublisher')
function Get-TrustInventory {
    $rows = @()
    foreach ($p in $trustPaths) {
        $rows += @((Get-ChildItem -LiteralPath $p | ForEach-Object { $p + '|' + $_.Thumbprint }) | Sort-Object)
    }
    return ($rows -join [char]10)
}
$trustBefore = Get-TrustInventory
function Require-Refusal($Arguments, [string]$Expected, [string]$Name) {
    $refused = $false
    try { & $scriptPath @Arguments | Out-Null }
    catch {
        if (!$_.Exception.Message.Contains($Expected)) { throw }
        $refused = $true
    }
    if (!$refused) { throw "expected refusal: $Name" }
    if ($Name -ne 'existing-output' -and (Test-Path -LiteralPath $Arguments.OutputDirectory)) {
        throw "unexpected output: $Name"
    }
    Write-Output "PASS_NEGATIVE $Name"
}

try {
    $ca = New-SelfSignedCertificate -Type Custom -Subject ('CN=' + $label + ' ROOT') `
        -CertStoreLocation 'Cert:\CurrentUser\My' -KeyAlgorithm RSA -KeyLength 3072 `
        -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage CertSign,CRLSign `
        -TextExtension @('2.5.29.19={critical}{text}ca=1&pathlength=0') -NotAfter (Get-Date).AddDays(3)
    $owned['ROOT'] = $ca
    foreach ($role in @('APP', 'RECOVERY')) {
        $owned[$role] = New-SelfSignedCertificate -Type CodeSigningCert -Subject ('CN=' + $label + ' ' + $role) `
            -Signer $ca -CertStoreLocation 'Cert:\CurrentUser\My' -KeyAlgorithm RSA -KeyLength 3072 `
            -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage DigitalSignature -NotAfter (Get-Date).AddDays(2)
    }
    $paths = @{}; $pins = @{}
    foreach ($role in @('ROOT', 'APP', 'RECOVERY')) {
        $paths[$role] = Join-Path $EvidenceDirectory ($role + '.cer')
        Export-Certificate -Cert $owned[$role] -FilePath $paths[$role] -Type CERT | Out-Null
        $pins[$role] = (Get-FileHash -Algorithm SHA256 -LiteralPath $paths[$role]).Hash
    }
    $common = @{
        UnsignedImage = $UnsignedFixture; ExpectedUnsignedSha256 = $pin; Role = 'APP'
        AppId = 'com.keld.dev.probe'; ApplicationCertificate = $paths['APP']
        ExpectedApplicationCertificateSha256 = $pins['APP']; RecoveryCertificate = $paths['RECOVERY']
        ExpectedRecoveryCertificateSha256 = $pins['RECOVERY']; RootCertificate = $paths['ROOT']
        ExpectedRootCertificateSha256 = $pins['ROOT']; OutputDirectory = (Join-Path $EvidenceDirectory 'app-output')
    }
    foreach ($role in @('APP', 'RECOVERY')) {
        $a = $common.Clone(); $a.Role = $role
        $a.OutputDirectory = Join-Path $EvidenceDirectory ($role.ToLowerInvariant() + '-output')
        & $scriptPath @a
        $receipt = Get-Content -Raw -LiteralPath (Join-Path $a.OutputDirectory 'dev-signing-observation.json') | ConvertFrom-Json
        if ($receipt.scope -cne 'DEVELOPMENT_ONLY_NONAUTHORIZING' -or $receipt.role -cne $role -or
            $receipt.leafCertificateSha256 -cne $pins[$role]) { throw 'receipt role or pin mismatch' }
        $signed = Join-Path $a.OutputDirectory $receipt.file
        $sig = Microsoft.PowerShell.Security\Get-AuthenticodeSignature -LiteralPath $signed
        if ($sig.SignerCertificate.Thumbprint -cne $owned[$role].Thumbprint -or $sig.Status -eq 'Valid' -or
            (Get-FileHash -Algorithm SHA256 -LiteralPath $signed).Hash -cne $receipt.signedSha256) {
            throw 'wrong signer, digest, or unexpected shared trust'
        }
        Write-Output "PASS_POSITIVE $role"
    }
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'bad-source'
    $a.ExpectedUnsignedSha256 = '0' * 64; Require-Refusal $a 'unsigned image differs' 'source-pin-mismatch'
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'bad-cert'
    $a.ExpectedRecoveryCertificateSha256 = '0' * 64; Require-Refusal $a 'certificate pin mismatch' 'cert-pin-mismatch'
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'same-role'
    $a.RecoveryCertificate = $paths['APP']; $a.ExpectedRecoveryCertificateSha256 = $pins['APP']
    Require-Refusal $a 'APP and RECOVERY keys must differ' 'same-role-key'
    # Same issuer subject, different CA key: name equality alone would accept
    # this substitution. The independent oracle is refusal before any output.
    $owned['WRONG_ROOT'] = New-SelfSignedCertificate -Type Custom -Subject $ca.Subject `
        -CertStoreLocation 'Cert:\CurrentUser\My' -KeyAlgorithm RSA -KeyLength 3072 `
        -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage CertSign,CRLSign `
        -TextExtension @('2.5.29.19={critical}{text}ca=1&pathlength=0') -NotAfter (Get-Date).AddDays(3)
    $wrongRoot = $owned['WRONG_ROOT']
    if ($wrongRoot.Subject -cne $ca.Subject -or $wrongRoot.GetPublicKeyString() -ceq $ca.GetPublicKeyString()) {
        throw 'wrong-root test requires same subject and distinct CA keys'
    }
    $wrongRootPath = Join-Path $EvidenceDirectory 'WRONG_ROOT.cer'
    Export-Certificate -Cert $wrongRoot -FilePath $wrongRootPath -Type CERT | Out-Null
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'wrong-root'
    $a.RootCertificate = $wrongRootPath
    $a.ExpectedRootCertificateSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $wrongRootPath).Hash
    Require-Refusal $a 'leaf chain does not match the exact pinned local CA' 'same-subject-wrong-root'
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'production-app'
    $a.AppId = 'com.keld.production'; Require-Refusal $a 'development-only app ID' 'production-app-id'
    $a = $common.Clone(); Require-Refusal $a 'absent output directory' 'existing-output'
    $signedReceipt = Get-Content -Raw -LiteralPath (Join-Path $common.OutputDirectory 'dev-signing-observation.json') | ConvertFrom-Json
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'resign'
    $a.UnsignedImage = Join-Path $common.OutputDirectory $signedReceipt.file; $a.ExpectedUnsignedSha256 = $signedReceipt.signedSha256
    Require-Refusal $a 'source image already carries a signature' 'already-signed-input'
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $UnsignedFixture).Hash -cne $pin) { throw 'source fixture changed' }
    Write-Output 'POSITIVE_2_NEGATIVE_7_SOURCE_PRESERVED'
} finally {
    foreach ($role in @('RECOVERY', 'APP', 'WRONG_ROOT', 'ROOT')) {
        if ($owned.ContainsKey($role)) {
            $cert = $owned[$role]; $p = 'Cert:\CurrentUser\My\' + $cert.Thumbprint
            $actual = Get-Item -LiteralPath $p -ErrorAction SilentlyContinue
            if ($actual -and $actual.Subject -ceq $cert.Subject -and
                [Convert]::ToBase64String($actual.RawData) -ceq [Convert]::ToBase64String($cert.RawData)) {
                Remove-Item -LiteralPath $p -DeleteKey -Force -ErrorAction Stop
                if (Test-Path -LiteralPath $p) { throw 'owned certificate remains' }
            } elseif ($actual) { throw 'unexpected certificate; preserve it' }
        }
    }
    if ((Get-TrustInventory) -ne $trustBefore) { throw 'trust inventory changed' }
    Write-Output 'TEMPORARY_KEYS_REMOVED_TRUST_STORES_UNCHANGED'
}
