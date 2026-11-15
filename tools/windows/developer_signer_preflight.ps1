# Developer-local signer metadata preflight (KEL-96). Read-only: this script
# never signs an image, creates/imports/exports keys or changes trust policy.
[CmdletBinding(DefaultParameterSetName = 'Preflight')]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string]$Thumbprint,

    [Parameter(Mandatory = $true, ParameterSetName = 'Discovery')]
    [switch]$DiscoverPublisherScope,

    [Parameter(Mandatory = $true, ParameterSetName = 'Preflight')]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string]$ExpectedPublisherScope,

    [Parameter(Mandatory = $true, ParameterSetName = 'Preflight')]
    [string]$SignToolPath,

    [Parameter(Mandatory = $true, ParameterSetName = 'Preflight')]
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string]$ExpectedSignToolSha256,

    [Parameter(Mandatory = $true, ParameterSetName = 'Preflight')]
    [string]$ReleasePublicKeyPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Refuse([string]$Reason) {
    throw "KELD-DEVELOPER-SIGNER-PREFLIGHT: $Reason"
}

function Require-AbsoluteFile([string]$Name, [string]$SelectedPath) {
    if ([string]::IsNullOrWhiteSpace($SelectedPath) -or
        -not [System.IO.Path]::IsPathRooted($SelectedPath)) {
        Refuse "$Name requires an explicit absolute file path"
    }
    # The compiler and signer select local Windows files. UNC/device aliases
    # and relative paths never become alternate tool/public-key inputs.
    if ($SelectedPath -notmatch '^[A-Za-z]:\\(?!\\)' -or
        $SelectedPath -match '\.\.') {
        Refuse "$Name requires an ordinary local-drive absolute path"
    }
    if (-not (Test-Path -LiteralPath $SelectedPath -PathType Leaf)) {
        Refuse "$Name does not exist as a regular file"
    }
    $file = Get-Item -LiteralPath $SelectedPath -ErrorAction Stop
    if (($file.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        Refuse "$Name may not be a reparse file"
    }
    return $file.FullName
}

if ($env:OS -cne 'Windows_NT') {
    Refuse 'native Windows certificate store is required'
}

$normalizedThumbprint = $Thumbprint.ToUpperInvariant()
# Explicit current-user personal store only. Do not search machine roots,
# auto-select another certificate, acquire a new key or change key access.
$selected = @(Get-ChildItem -Path 'Cert:\CurrentUser\My' -ErrorAction Stop |
    Where-Object { $_.Thumbprint.ToUpperInvariant() -ceq $normalizedThumbprint })
if ($selected.Count -ne 1) {
    Refuse 'selected certificate is missing or ambiguous in CurrentUser\My'
}
$certificate = $selected[0]
$now = [DateTime]::UtcNow
if ($now -lt $certificate.NotBefore.ToUniversalTime() -or
    $now -gt $certificate.NotAfter.ToUniversalTime()) {
    Refuse 'selected certificate is outside its validity interval'
}
if (-not $certificate.HasPrivateKey) {
    Refuse 'selected certificate has no available private-key indicator'
}

$eku = @($certificate.Extensions | Where-Object { $_.Oid.Value -ceq '2.5.29.37' })
if ($eku.Count -ne 1) {
    Refuse 'selected certificate requires an explicit EKU extension'
}
$codeSigningEku = [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new($eku[0], $eku[0].Critical)
$matchedCodeSigningEku = @($codeSigningEku.EnhancedKeyUsages | Where-Object {
    $_.Value -ceq '1.3.6.1.5.5.7.3.3'
})
if ($matchedCodeSigningEku.Count -ne 1) {
    Refuse 'selected certificate lacks the Code Signing EKU'
}
$keyUsage = @($certificate.Extensions | Where-Object { $_.Oid.Value -ceq '2.5.29.15' })
if ($keyUsage.Count -ne 1) {
    Refuse 'selected certificate requires explicit key usage'
}
$usage = [System.Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new($keyUsage[0], $keyUsage[0].Critical)
if (($usage.KeyUsages -band [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature) -eq 0) {
    Refuse 'selected certificate lacks Digital Signature key usage'
}

# Preserve the exact certificate-encoded public SPKI DER. PowerShell/.NET key
# export helpers can re-encode public keys; KEL-135 hashes original SPKI bytes.
$helper = Join-Path -Path $PSScriptRoot -ChildPath 'developer_signer_spki.py'
if (-not (Test-Path -LiteralPath $helper -PathType Leaf)) {
    Refuse 'read-only SPKI helper is missing'
}
$python = Get-Command -Name 'python' -CommandType Application -ErrorAction SilentlyContinue |
    Select-Object -First 1
if ($null -eq $python) {
    Refuse 'python executable is missing; standard-library SPKI parsing is required'
}
$encodedPublicCertificate = [Convert]::ToBase64String($certificate.RawData)
$scopeOutput = @($encodedPublicCertificate | & $python.Source -B $helper)
if ($LASTEXITCODE -ne 0 -or $scopeOutput.Count -ne 1 -or
    $scopeOutput[0] -cnotmatch '^[0-9a-f]{64}$') {
    Refuse 'certificate SPKI publisher scope could not be derived'
}
$scope = [string]$scopeOutput[0]
if ($DiscoverPublisherScope) {
    Write-Output "DISCOVERY_ONLY publisherScope=$scope"
    return
}
if ($scope -cne $ExpectedPublisherScope.ToLowerInvariant()) {
    Refuse 'selected certificate publisher scope differs from expected scope'
}

$signTool = Require-AbsoluteFile -Name 'SignTool' -SelectedPath $SignToolPath
if ([IO.Path]::GetExtension($signTool) -cne '.exe' -or
    [IO.Path]::GetFileName($signTool) -cne 'signtool.exe') {
    Refuse 'selected SignTool must be signtool.exe'
}
$actualToolDigest = (Get-FileHash -LiteralPath $signTool -Algorithm SHA256).Hash
if ($actualToolDigest -cne $ExpectedSignToolSha256.ToUpperInvariant()) {
    Refuse 'selected SignTool bytes do not match the independently pinned SHA256'
}
$signature = Get-AuthenticodeSignature -LiteralPath $signTool
if ($null -eq $signature.SignerCertificate -or
    $signature.Status.ToString() -cne 'Valid' -or
    $signature.SignatureType.ToString() -cne 'Authenticode') {
    Refuse 'selected SignTool lacks a valid Authenticode signature'
}
$publisher = $signature.SignerCertificate.GetNameInfo(
    [System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
if ($publisher -cne 'Microsoft Corporation') {
    Refuse 'selected SignTool Authenticode publisher is not Microsoft Corporation'
}
$toolMetadata = (Get-Item -LiteralPath $signTool).VersionInfo
if ($toolMetadata.OriginalFilename -ine 'signtool.exe') {
    Refuse 'selected executable has the wrong original SignTool filename'
}

$publicKeyFile = Require-AbsoluteFile -Name 'release public key' -SelectedPath $ReleasePublicKeyPath
# Only read an existing public key. No private signing/update key is accessed.
if ((Get-Item -LiteralPath $publicKeyFile).Length -ne 32) {
    Refuse 'release public key must contain exactly 32 raw bytes'
}
$publicKey = [System.IO.File]::ReadAllBytes($publicKeyFile)
if ($publicKey.Length -ne 32) {
    Refuse 'release public key changed during metadata preflight'
}
if (@($publicKey | Where-Object { $_ -ne 0 }).Count -eq 0) {
    Refuse 'release public key must not be all zeroes'
}

Write-Output "PREFLIGHT_PASS publisherScope=$scope"
