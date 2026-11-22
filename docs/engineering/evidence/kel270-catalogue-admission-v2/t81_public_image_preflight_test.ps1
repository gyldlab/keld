# PowerShell 5.1 negative controls for the non-authorizing T81 public observer.
# No signing, certificate import, trust change, app execution or installation.
[CmdletBinding()]
param(
    [ValidateSet('oid', 'unbound', 'ordinary', 'localtest', 'all')]
    [string]$Case = 'all',
    [string]$LocalTestFixture
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path $PSScriptRoot 't81_public_image_preflight.ps1'
$docPath = Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) 'kel270-windows-t81-release-operator.md'
$nativePowerShell = Join-Path $PSHOME 'powershell.exe'
$absent = Join-Path $PSScriptRoot 't81-absent-image-for-negative-control.exe'
$zeros = '0' * 64

if (-not (Test-Path -LiteralPath $nativePowerShell -PathType Leaf)) {
    throw 'T81-TEST: Windows PowerShell 5.1 executable unavailable'
}
if (Test-Path -LiteralPath $absent) {
    throw 'T81-TEST: test absent-file path is occupied'
}

function Invoke-PublicObserver([string]$AppOid, [string]$RecoveryOid,
                               [string]$AppImage, [string]$AppSha) {
    # Windows PowerShell 5.1 surfaces a redirected native stderr line as a
    # nonterminating error. Preserve the child's real exit code as the oracle.
    $ErrorActionPreference = 'Continue'
    try {
        $output = (& $nativePowerShell -NoProfile -NonInteractive -File $scriptPath -ApplicationHost $AppImage -ApplicationSha256 $AppSha -ApplicationProfileOid $AppOid -RecoveryHost $absent -RecoverySha256 $zeros -RecoveryProfileOid $RecoveryOid 2>&1 | Out-String)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
    }
    return [pscustomobject]@{ ExitCode = $exitCode; Output = $output }
}

function Assert-Refusal([string]$Name, [string]$AppOid,
                        [string]$ExpectedText) {
    # Keep the two grammar-only test OIDs distinct, including the 1.39.3 edge.
    $result = Invoke-PublicObserver $AppOid '1.38.3' $absent $zeros
    if ($result.ExitCode -eq 0 -or
        $result.Output -notmatch [regex]::Escape($ExpectedText) -or
        $result.Output -match 'T81_PUBLIC_IMAGE_OBSERVED|T81_ORDINARY_SID_OBSERVED') {
        throw "T81-TEST: $Name not refused at its required boundary (exit $($result.ExitCode)): $($result.Output)"
    }
    Write-Output "T81_NEGATIVE_PASS name=$Name"
}

function Assert-RecoveryOidRefusal([string]$Name, [string]$RecoveryOid) {
    $result = Invoke-PublicObserver '2.4294967295.1' $RecoveryOid $absent $zeros
    if ($result.ExitCode -eq 0 -or
        $result.Output -notmatch 'malformed or generic RecoveryProfileOid' -or
        $result.Output -match 'T81_PUBLIC_IMAGE_OBSERVED') {
        throw "T81-TEST: recovery OID $Name was not refused (exit $($result.ExitCode)): $($result.Output)"
    }
    Write-Output "T81_NEGATIVE_PASS name=recovery-$Name"
}

if ($Case -in @('oid', 'all')) {
    # The named test values check grammar only. They commission no issuer/OID.
    $invalid = @(
        @{ Name = 'first-1-second-40'; Oid = '1.40.3' },
        @{ Name = 'first-0-second-99'; Oid = '0.99.3' },
        @{ Name = 'uint32-overflow'; Oid = '2.4294967296.1' },
        @{ Name = 'private-trust-profile'; Oid = '1.3.6.1.4.1.311.97.1.3.1' },
        @{ Name = 'code-signing-generic'; Oid = '1.3.6.1.5.5.7.3.3' },
        @{ Name = 'public-trust-generic'; Oid = '1.3.6.1.4.1.311.97.1.0' },
        @{ Name = 'invalid-first-arc'; Oid = '3.1.3' },
        @{ Name = 'leading-zero-arc'; Oid = '2.01.3' },
        @{ Name = 'empty-arc'; Oid = '2..3' },
        @{ Name = 'private-trust-profile-descendant'; Oid = '1.3.6.1.4.1.311.97.1.3.1.2' },
        @{ Name = 'too-many-arcs'; Oid = ('2.' + ('1.' * 63) + '1') }
    )
    foreach ($item in $invalid) {
        Assert-Refusal $item.Name $item.Oid 'malformed or generic ApplicationProfileOid'
    }
    foreach ($oid in @(
        @{ Name = 'first-1-second-40'; Value = '1.40.3' },
        @{ Name = 'uint32-overflow'; Value = '2.4294967296.1' },
        @{ Name = 'private-trust-profile'; Value = '1.3.6.1.4.1.311.97.1.3.1' },
        @{ Name = 'first-0-second-99'; Value = '0.99.3' }
    )) {
        Assert-RecoveryOidRefusal $oid.Name $oid.Value
    }

    # Distinguish grammar admission from issuer commissioning: legal edge
    # values reach the absent-file rejection, never signer authorization.
    foreach ($item in @(
        @{ Name = 'first-0-second-39'; Oid = '0.39.3' },
        @{ Name = 'first-1-second-39'; Oid = '1.39.3' },
        @{ Name = 'max-uint32-arc'; Oid = '2.4294967295.1' },
        @{ Name = 'max-64-arcs'; Oid = ('2.' + ('1.' * 62) + '1') }
    )) {
        Assert-Refusal $item.Name $item.Oid 'application image missing'
    }
}

if ($Case -in @('unbound', 'all')) {
    $source = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
    foreach ($required in @(
        'T81_RAW_PATH_HASH_UNBOUND',
        'T81_RAW_PATH_SIGNATURE_UNBOUND',
        'T81_NATIVE_D45_ORIGINAL_HANDLE_REQUIRED',
        'root_issuer_spki=UNKNOWN',
        'signature_cardinality=UNKNOWN',
        'commissioned_role=UNKNOWN'
    )) {
        if ($source -notmatch [regex]::Escape($required)) {
            throw "T81-TEST: missing explicit unbound/unknown contract $required"
        }
    }
    if ($source -match 'T81_PUBLIC_IMAGE_OBSERVED|T81_PUBLIC_PREFLIGHT_ONLY_NOT_RELEASE_AUTHORITY') {
        throw 'T81-TEST: old combined image/preflight marker still present'
    }
    Write-Output 'T81_SOURCE_CONTRACT_PASS name=unbound-observations'
}

if ($Case -in @('ordinary', 'all')) {
    $doc = Get-Content -LiteralPath $docPath -Raw -Encoding UTF8
    if ($doc -match 'T81_ORDINARY_SID_OBSERVED|\.IsInRole\(') {
        throw 'T81-TEST: filtered Admin token can still be called ordinary'
    }
    foreach ($required in @('whoami /all', 'deny-only', 'account membership',
                            'T81_RAW_TOKEN_SNAPSHOT_ONLY_NOT_SID_ACCEPTANCE',
                            'one-primary/zero-secondary')) {
        if ($doc -notmatch [regex]::Escape($required)) {
            throw "T81-TEST: missing independent token/signature-count caveat $required"
        }
    }
    Write-Output 'T81_SOURCE_CONTRACT_PASS name=ordinary-sid-and-cardinality'
}

if ($Case -in @('localtest', 'all')) {
    if ([string]::IsNullOrWhiteSpace($LocalTestFixture)) {
        Write-Output 'T81_NEGATIVE_NOT_RUN name=real-retained-localtest-carrier reason=no_fixture_supplied'
    } else {
        if (-not (Test-Path -LiteralPath $LocalTestFixture -PathType Leaf)) {
            throw 'T81-TEST: requested retained LocalTest fixture does not exist'
        }
        $digest = (Get-FileHash -LiteralPath $LocalTestFixture -Algorithm SHA256).Hash
        $result = Invoke-PublicObserver '2.4294967295.1' '1.39.3' $LocalTestFixture $digest
        if ($result.ExitCode -eq 0 -or
            $result.Output -notmatch 'application (lacks valid embedded Authenticode|has self-issued signer)' -or
            $result.Output -match 'T81_PUBLIC_IMAGE_OBSERVED') {
            throw "T81-TEST: supplied LocalTest signer not refused (exit $($result.ExitCode)): $($result.Output)"
        }
        Write-Output 'T81_NEGATIVE_PASS name=real-retained-localtest-carrier'
    }
}

Write-Output "T81_READONLY_NEGATIVE_SUITE_COMPLETE scope=$Case genuine_release=UNRUN"
