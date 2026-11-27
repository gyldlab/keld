# T81 public path observations only; non-authorizing and UNBOUND.
# Get-FileHash and Get-AuthenticodeSignature reopen by path separately.
# Even hash A -> signature B -> hash A cannot prove one original file handle.
# Only the native D45 retained-hFile verifier can bind signer and exact bytes.
# This never signs, changes trust, installs, or grants Keld release authority.
# Expected hashes and profile OIDs come from an independent release-owner record.
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
    # Same numeric/byte grammar as identity.md:38-46. This checks only syntax,
    # not actual commissioning or membership of an issuer/profile cohort.
    if ($Value.Length -lt 3 -or $Value.Length -gt 255 -or
        $Value -cnotmatch '^(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*))+$' -or
        $Value -eq '1.3.6.1.5.5.7.3.3' -or
        $Value -eq '1.3.6.1.4.1.311.97.1.0' -or
        $Value -cmatch '^1\.3\.6\.1\.4\.1\.311\.97\.1\.3(?:\.|$)') {
        throw "T81-PUBLIC-PREFLIGHT: malformed or generic $Name"
    }
    $arcs = $Value.Split('.')
    if ($arcs.Count -lt 2 -or $arcs.Count -gt 64) {
        throw "T81-PUBLIC-PREFLIGHT: malformed or generic $Name"
    }
    for ($index = 0; $index -lt $arcs.Count; $index++) {
        [uint32]$parsed = 0
        if (-not [uint32]::TryParse($arcs[$index], [ref]$parsed) -or
            ($index -eq 0 -and $parsed -gt 2) -or
            ($index -eq 1 -and [uint32]$arcs[0] -lt 2 -and $parsed -gt 39)) {
            throw "T81-PUBLIC-PREFLIGHT: malformed or generic $Name"
        }
    }
}

function Read-UnboundPathObservations([string]$Role, [string]$Source, [string]$ExpectedSha,
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
    Write-Output "T81_RAW_PATH_HASH_UNBOUND role=$Role phase=before sha256=$firstSha"

    # A second PATH lookup; the signature is NOT bound to the hash above.
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    $thumb = if ($null -ne $signature.SignerCertificate) {
        $signature.SignerCertificate.Thumbprint
    } else {
        'ABSENT'
    }
    Write-Output "T81_RAW_PATH_SIGNATURE_UNBOUND role=$Role status=$($signature.Status) type=$($signature.SignatureType) leaf_thumbprint=$thumb"
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
    # A third PATH lookup is still not a retained-hFile identity proof.
    $lastSha = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
    if ($lastSha -cne $firstSha) {
        throw "T81-PUBLIC-PREFLIGHT: $Role changed during read-only inspection"
    }
    Write-Output "T81_RAW_PATH_HASH_UNBOUND role=$Role phase=after sha256=$lastSha"
    # Windows Status Valid + leaf EKUs are necessary observations only.
    # Full native issuer/root, one-primary/zero-secondary, and commissioned
    # role authority remain UNKNOWN, even if every above observation succeeds.
    Write-Output "T81_NATIVE_D45_ORIGINAL_HANDLE_REQUIRED role=$Role file_signature_binding=UNBOUND root_issuer_spki=UNKNOWN signature_cardinality=UNKNOWN commissioned_role=UNKNOWN"
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
Read-UnboundPathObservations 'application' $ApplicationHost $ApplicationSha256 $ApplicationProfileOid $RecoveryProfileOid
Read-UnboundPathObservations 'recovery' $RecoveryHost $RecoverySha256 $RecoveryProfileOid $ApplicationProfileOid
Write-Output 'T81_PATH_OBSERVATIONS_ONLY_UNBOUND_NONAUTHORIZING'
