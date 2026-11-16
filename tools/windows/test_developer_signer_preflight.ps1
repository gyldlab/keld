# Native read-only acceptance using existing, separately selected metadata.
# Never generates a signing certificate, private key, trust entry or tool.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string]$Thumbprint,
    [Parameter(Mandatory = $true)] [string]$PythonPath,
    [Parameter(Mandatory = $true)] [string]$ExpectedPythonSha256,
    [Parameter(Mandatory = $true)] [string]$SignToolPath,
    [Parameter(Mandatory = $true)] [string]$ExpectedSignToolSha256,
    [Parameter(Mandatory = $true)] [string]$ReleasePublicKeyPath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$subject = Join-Path $PSScriptRoot 'developer_signer_preflight.ps1'
if (-not (Test-Path -LiteralPath $subject -PathType Leaf)) {
    throw 'KELD-PREFLIGHT-TEST: preflight script missing'
}

function Invoke-Case([string]$Name, [string[]]$Arguments, [bool]$ExpectSuccess, [string]$Marker) {
    # A deliberate refusal writes native stderr; capture it as data even
    # under this harness's otherwise terminating-error policy.
    $priorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = (& powershell.exe -NoProfile -NonInteractive -File $subject @Arguments 2>&1 | Out-String)
        $exit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $priorPreference
    }
    if ($ExpectSuccess) {
        if ($exit -ne 0 -or $output -notmatch $Marker) {
            throw "KELD-PREFLIGHT-TEST: $Name expected success; exit=$exit output=$output"
        }
    } elseif ($exit -eq 0 -or $output -match 'PREFLIGHT_PASS' -or
        $output -notmatch 'KELD-DEVELOPER-SIGNER-PREFLIGHT:') {
        throw "KELD-PREFLIGHT-TEST: $Name accepted invalid inputs: $output"
    }
    Write-Output "KELD_PREFLIGHT_CASE: $Name"
    return $output
}

if ($Thumbprint -ceq ('0' * 40)) { $missingThumbprint = '1' * 40 } else { $missingThumbprint = '0' * 40 }
$pythonArgs = @('-PythonPath', $PythonPath, '-ExpectedPythonSha256', $ExpectedPythonSha256)
[void](Invoke-Case 'incorrect or missing certificate' (@('-Thumbprint', $missingThumbprint, '-DiscoverPublisherScope') + $pythonArgs) $false '')
$discovery = Invoke-Case 'discovery' (@('-Thumbprint', $Thumbprint, '-DiscoverPublisherScope') + $pythonArgs) $true 'DISCOVERY_ONLY publisherScope=[0-9a-f]{64}'
$scopeMatches = [regex]::Matches($discovery, 'DISCOVERY_ONLY publisherScope=(?<scope>[0-9a-f]{64})')
if ($scopeMatches.Count -ne 1) { throw 'KELD-PREFLIGHT-TEST: non-unique discovery scope' }
$scope = $scopeMatches[0].Groups['scope'].Value
$validArgs = @('-Thumbprint', $Thumbprint,
    '-ExpectedPublisherScope', $scope,
    '-SignToolPath', $SignToolPath,
    '-ExpectedSignToolSha256', $ExpectedSignToolSha256,
    '-ReleasePublicKeyPath', $ReleasePublicKeyPath,
    '-PythonPath', $PythonPath,
    '-ExpectedPythonSha256', $ExpectedPythonSha256)
[void](Invoke-Case 'valid metadata' $validArgs $true 'PREFLIGHT_PASS publisherScope=[0-9a-f]{64}')

$wrongScope = @($validArgs)
$wrongScope[3] = '0' * 64
if ($wrongScope[3] -ceq $scope) { $wrongScope[3] = '1' * 64 }
[void](Invoke-Case 'wrong publisher scope' $wrongScope $false '')
$wrongHash = @($validArgs)
$wrongHash[7] = '0' * 64
if ($wrongHash[7] -ceq $ExpectedSignToolSha256) { $wrongHash[7] = '1' * 64 }
[void](Invoke-Case 'wrong tool SHA256' $wrongHash $false '')
$missingTool = @($validArgs)
$missingTool[5] = Join-Path $env:TEMP 'keld-nonexistent-signtool.exe'
[void](Invoke-Case 'missing SignTool' $missingTool $false '')
$wrongPythonHash = @($validArgs)
$wrongPythonHash[13] = '0' * 64
if ($wrongPythonHash[13] -ceq $ExpectedPythonSha256) { $wrongPythonHash[13] = '1' * 64 }
[void](Invoke-Case 'wrong Python SHA256' $wrongPythonHash $false '')
$missingPython = @($validArgs)
$missingPython[11] = Join-Path $env:TEMP 'keld-nonexistent-python.exe'
[void](Invoke-Case 'missing Python interpreter' $missingPython $false '')

$fixture = Join-Path $env:TEMP ('keld-invalid-update-public-' + [guid]::NewGuid().ToString('N') + '.bin')
try {
    [System.IO.File]::WriteAllBytes($fixture, [byte[]](1..31))
    $invalidKey = @($validArgs)
    $invalidKey[9] = $fixture
    [void](Invoke-Case 'invalid release public-key length' $invalidKey $false '')
    [System.IO.File]::WriteAllBytes($fixture, (New-Object 'byte[]' 32))
    [void](Invoke-Case 'invalid all-zero public key' $invalidKey $false '')
} finally {
    Remove-Item -LiteralPath $fixture -Force -ErrorAction SilentlyContinue
}
# First prove that an unisolated Python startup would execute the fixture.
# Then require the exact pinned interpreter's -I -S startup to leave no mark.
$siteRoot = Join-Path $env:TEMP ('keld-ambient-python-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $siteRoot -ErrorAction Stop | Out-Null
$siteMarker = Join-Path $siteRoot 'startup-hook.marker'
$siteMarkerLiteral = ConvertTo-Json -InputObject $siteMarker -Compress
[System.IO.File]::WriteAllText(
    (Join-Path $siteRoot 'sitecustomize.py'),
    "from pathlib import Path`nPath($siteMarkerLiteral).write_text('AMBIENT_PYTHON_HOOK_USED')`n")
$previousPythonPath = $env:PYTHONPATH
try {
    $env:PYTHONPATH = $siteRoot
    & $PythonPath -B -c 'pass' | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $siteMarker)) {
        throw 'KELD-PREFLIGHT-TEST: unisolated Python startup negative control was inert'
    }
    Remove-Item -LiteralPath $siteMarker -Force
    [void](Invoke-Case 'ambient Python startup hook refused' $validArgs $true 'PREFLIGHT_PASS publisherScope=[0-9a-f]{64}')
    if (Test-Path -LiteralPath $siteMarker) {
        throw 'KELD-PREFLIGHT-TEST: preflight ran an ambient Python startup hook'
    }
} finally {
    $env:PYTHONPATH = $previousPythonPath
    Remove-Item -LiteralPath $siteRoot -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output 'KELD_DEVELOPER_SIGNER_PREFLIGHT_TEST: 11/11'
