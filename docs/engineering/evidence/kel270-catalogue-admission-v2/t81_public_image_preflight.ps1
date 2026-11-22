# T81 public, non-authorizing Windows signature preflight.
# This reads published files and their public signer certificates. It does not
# sign, import a certificate, modify trust, install, or grant Keld authority.
# The values must come from the independent release-owner record.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ApplicationHost,
    [Parameter(Mandatory = $true)][string]$ApplicationSha256,
    [Parameter(Mandatory = $true)][string]$ApplicationProfileOid,
    [Parameter(Mandatory = $true)][string]$RecoveryHost,
    [Parameter(Mandatory = $true)][string]$RecoverySha256,
    [Parameter(Mandatory = $true)][string]$RecoveryProfileOid
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-Oid([string]$Name, [string]$Value) {
    if ($Value -cnotmatch '^[0-2]\.([0-9]+\.)+[0-9]+$' -or $Value.Length -gt 255 -or
        $Value -match '(^|\.)0[0-9]+' -or
        $Value -eq '1.3.6.1.5.5.7.3.3' -or
        $Value -eq '1.3.6.1.4.1.311.97.1.0') {
        throw "T81-PUBLIC-PREFLIGHT: malformed or generic $Name"
    }
}

function Test-Image([string]$Role, [string]$Source, [string]$ExpectedSha,
                    [string]$RequiredProfile, [string]$ForbiddenProfile) {
    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "T81-PUBLIC-PREFLIGHT: $Role image missing"
    }
    $path = (Resolve-Path -LiteralPath $Source).ProviderPath
    if ([IO.Path]::GetExtension($path) -cne '.exe') {
        throw "T81-PUBLIC-PREFLIGHT: $Role is not an exe"
    }
    $firstSha = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
    if ($firstSha -cne $ExpectedSha.ToUpperInvariant()) {
        throw "T81-PUBLIC-PREFLIGHT: $Role independent SHA256 mismatch"
    }

    $signature = Get-AuthenticodeSignature -LiteralPath $path
    if ($signature.Status.ToString() -cne 'Valid' -or
        $signature.SignatureType.ToString() -cne 'Authenticode' -or
        $null -eq $signature.SignerCertificate) {
        throw "T81-PUBLIC-PREFLIGHT: $Role lacks valid embedded Authenticode"
    }
    $cert = $signature.SignerCertificate
    if ($cert.Subject -ceq $cert.Issuer) {
        throw "T81-PUBLIC-PREFLIGHT: $Role has self-issued signer"
    }
    $ekus = @($cert.EnhancedKeyUsageList | ForEach-Object { $_.ObjectId })
    foreach ($oid in @('1.3.6.1.5.5.7.3.3', '1.3.6.1.4.1.311.97.1.0', $RequiredProfile)) {
        if ($ekus -cnotcontains $oid) {
            throw "T81-PUBLIC-PREFLIGHT: $Role missing required EKU $oid"
        }
    }
    if ($ekus -ccontains $ForbiddenProfile) {
        throw "T81-PUBLIC-PREFLIGHT: $Role carries forbidden opposite-role EKU"
    }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -cne $firstSha) {
        throw "T81-PUBLIC-PREFLIGHT: $Role changed during read-only inspection"
    }
    # Thumbprints describe observed leaves. They are not durable profile pins.
    Write-Output "T81_PUBLIC_IMAGE_OBSERVED role=$Role sha256=$firstSha profile_oid=$RequiredProfile leaf_thumbprint=$($cert.Thumbprint)"
}

if ($env:OS -cne 'Windows_NT') {
    throw 'T81-PUBLIC-PREFLIGHT: native Windows required'
}
foreach ($sha in @($ApplicationSha256, $RecoverySha256)) {
    if ($sha -cnotmatch '^[0-9a-fA-F]{64}$') {
        throw 'T81-PUBLIC-PREFLIGHT: malformed independently recorded SHA256'
    }
}
Assert-Oid 'ApplicationProfileOid' $ApplicationProfileOid
Assert-Oid 'RecoveryProfileOid' $RecoveryProfileOid
if ($ApplicationProfileOid -ceq $RecoveryProfileOid) {
    throw 'T81-PUBLIC-PREFLIGHT: application/recovery profile OIDs must differ'
}
Test-Image 'application' $ApplicationHost $ApplicationSha256 $ApplicationProfileOid $RecoveryProfileOid
Test-Image 'recovery' $RecoveryHost $RecoverySha256 $RecoveryProfileOid $ApplicationProfileOid
Write-Output 'T81_PUBLIC_PREFLIGHT_ONLY_NOT_RELEASE_AUTHORITY'
