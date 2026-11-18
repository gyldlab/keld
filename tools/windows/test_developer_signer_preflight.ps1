# Native read-only acceptance using existing, separately selected metadata.
# Never generates a signing certificate, private key, trust entry or tool.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string]$Thumbprint,
    [Parameter(Mandatory = $true)] [string]$PythonPath,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-fA-F]{64}$')] [string]$ExpectedPythonSha256,
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
$spkiParser = Join-Path $PSScriptRoot 'developer_signer_spki.ps1'
if (-not (Test-Path -LiteralPath $spkiParser -PathType Leaf)) {
    throw 'KELD-PREFLIGHT-TEST: in-process SPKI parser missing'
}
. $spkiParser
$powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
if (-not ('KeldCrypt32SpkiProbe' -as [type])) {
    $crypt32Source = @'
using System;
using System.Runtime.InteropServices;
public static class KeldCrypt32SpkiProbe {
    [DllImport("crypt32.dll", SetLastError = true)]
    public static extern bool CryptDecodeObjectEx(uint encoding, IntPtr structType, byte[] encoded, uint encodedSize, uint flags, IntPtr decodePara, out IntPtr decoded, out uint decodedSize);
    [DllImport("crypt32.dll", SetLastError = true)]
    public static extern bool CryptEncodeObjectEx(uint encoding, IntPtr structType, IntPtr decoded, uint flags, IntPtr encodePara, [Out] byte[] encoded, ref uint encodedSize);
    [DllImport("kernel32.dll")]
    public static extern IntPtr LocalFree(IntPtr memory);
}
'@
    Add-Type -TypeDefinition $crypt32Source -ErrorAction Stop
}

function Assert-Crypt32SpkiParity([byte[]]$CertificateBytes) {
    $raw = Get-KeldSubjectPublicKeyInfo $CertificateBytes
    $decoded = [IntPtr]::Zero
    [uint32]$decodedSize = 0
    if (-not [KeldCrypt32SpkiProbe]::CryptDecodeObjectEx(
        1, [IntPtr]8, $raw, [uint32]$raw.Length, [uint32]0x8000,
        [IntPtr]::Zero, [ref]$decoded, [ref]$decodedSize)) {
        throw ('KELD-PREFLIGHT-TEST: CryptDecodeObjectEx failed: ' +
            [Runtime.InteropServices.Marshal]::GetLastWin32Error())
    }
    try {
        [uint32]$encodedSize = 0
        if (-not [KeldCrypt32SpkiProbe]::CryptEncodeObjectEx(
            1, [IntPtr]8, $decoded, 0, [IntPtr]::Zero, $null, [ref]$encodedSize) -or
            $encodedSize -eq 0) {
            throw ('KELD-PREFLIGHT-TEST: CryptEncodeObjectEx size failed: ' +
                [Runtime.InteropServices.Marshal]::GetLastWin32Error())
        }
        $encoded = New-Object byte[] $encodedSize
        $actualSize = $encodedSize
        if (-not [KeldCrypt32SpkiProbe]::CryptEncodeObjectEx(
            1, [IntPtr]8, $decoded, 0, [IntPtr]::Zero, $encoded, [ref]$actualSize)) {
            throw ('KELD-PREFLIGHT-TEST: CryptEncodeObjectEx failed: ' +
                [Runtime.InteropServices.Marshal]::GetLastWin32Error())
        }
        if ($actualSize -ne $raw.Length -or
            [Convert]::ToBase64String($encoded) -cne [Convert]::ToBase64String($raw)) {
            throw 'KELD-PREFLIGHT-TEST: raw certificate SPKI differs from runtime Crypt32 encoding'
        }
    } finally {
        [void][KeldCrypt32SpkiProbe]::LocalFree($decoded)
    }
}

function Invoke-Case([string]$Name, [string[]]$Arguments, [bool]$ExpectSuccess, [string]$Marker) {
    # A deliberate refusal writes native stderr; capture it as data even
    # under this harness's otherwise terminating-error policy.
    $priorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = (& $powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $subject @Arguments 2>&1 | Out-String)
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

$python = [IO.Path]::GetFullPath($PythonPath)
if (-not [IO.Path]::IsPathRooted($PythonPath) -or
    -not (Test-Path -LiteralPath $python -PathType Leaf)) {
    throw 'KELD-PREFLIGHT-TEST: Python requires an existing explicit absolute path'
}
if ([IO.Path]::GetExtension($python) -ine '.exe') {
    throw 'KELD-PREFLIGHT-TEST: Python interpreter must be an executable image (.exe)'
}

$pythonHandle = $null
$sha256 = $null
$pythonFixture = Join-Path $env:TEMP ('keld-python-startup-' + [guid]::NewGuid().ToString('N'))
$marker = Join-Path $pythonFixture 'startup.marker'
$pythonShimRoot = Join-Path $env:TEMP ('keld-python-path-shim-' + [guid]::NewGuid().ToString('N'))
$pythonShim = Join-Path $pythonShimRoot 'python.cmd'
$pythonLaunchMarker = Join-Path $pythonShimRoot 'python-launch.marker'
$savedPythonUserBase = $env:PYTHONUSERBASE
$savedPythonPath = $env:PYTHONPATH
$savedPythonNoUserSite = $env:PYTHONNOUSERSITE
$savedPythonHome = $env:PYTHONHOME
$savedPath = $env:PATH
$savedPathExt = $env:PATHEXT
$savedLaunchMarker = $env:KELD_TEST_PYTHON_LAUNCH_MARKER
try {
    $pythonHandle = [IO.File]::Open(
        $python,
        [IO.FileMode]::Open,
        [IO.FileAccess]::Read,
        [IO.FileShare]::Read)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    $actualPythonSha256 = [BitConverter]::ToString($sha256.ComputeHash($pythonHandle)).Replace('-', '')
    if ($actualPythonSha256 -cne $ExpectedPythonSha256.ToUpperInvariant()) {
        throw 'KELD-PREFLIGHT-TEST: selected Python image does not match its supplied SHA256'
    }
    $pythonHandle.Position = 0

    New-Item -ItemType Directory -Path $pythonShimRoot -Force | Out-Null
    $shimContents = @'
@echo off
echo launched>"%KELD_TEST_PYTHON_LAUNCH_MARKER%"
exit /b 99
'@
    Set-Content -LiteralPath $pythonShim -Encoding ASCII -NoNewline -Value $shimContents
    $env:PATH = "$pythonShimRoot;$env:SystemRoot\System32"
    $env:PATHEXT = '.COM;.EXE;.BAT;.CMD'
    $env:KELD_TEST_PYTHON_LAUNCH_MARKER = $pythonLaunchMarker
    $shim = Get-Command -Name python -CommandType Application -ErrorAction Stop |
        Select-Object -First 1
    if ($shim.Source -cne $pythonShim) {
        throw 'KELD-PREFLIGHT-TEST: ambient python launcher shim was not selected first on PATH'
    }
    $priorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $null = & $shim.Source 2>&1
        $shimExit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $priorPreference
    }
    if ($shimExit -eq 0 -or -not (Test-Path -LiteralPath $pythonLaunchMarker)) {
        throw 'KELD-PREFLIGHT-TEST: ambient python launcher positive control did not execute'
    }
    Remove-Item -LiteralPath $pythonLaunchMarker -Force
    Write-Output 'KELD_PREFLIGHT_CASE: ambient Python launcher control'

    $versionOutput = @(& $python -I -S -B -c "import sys; print('%d%d' % sys.version_info[:2])" 2>&1)
    $versionExit = $LASTEXITCODE
    if ($versionExit -ne 0 -or $versionOutput.Count -ne 1 -or
        [string]$versionOutput[0] -notmatch '^\d{2,3}$') {
        throw 'KELD-PREFLIGHT-TEST: isolated Python version query failed'
    }
    $pythonSite = Join-Path $pythonFixture ("Python{0}\site-packages" -f [string]$versionOutput[0])
    New-Item -ItemType Directory -Path $pythonSite -Force | Out-Null
    $markerBase64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($marker))
    $pthCode = "import base64, pathlib; pathlib.Path(base64.b64decode('$markerBase64').decode('utf-8')).open('a').write('PTH\n')"
    $siteCode = "import base64, pathlib; pathlib.Path(base64.b64decode('$markerBase64').decode('utf-8')).open('a').write('SITECUSTOMIZE\n')"
    Set-Content -LiteralPath (Join-Path $pythonSite 'keld-startup-hook.pth') -Value $pthCode -Encoding ASCII -NoNewline
    Set-Content -LiteralPath (Join-Path $pythonSite 'sitecustomize.py') -Value $siteCode -Encoding ASCII -NoNewline

    $env:PYTHONUSERBASE = $pythonFixture
    $env:PYTHONPATH = $pythonSite
    Remove-Item Env:PYTHONNOUSERSITE -ErrorAction SilentlyContinue
    Remove-Item Env:PYTHONHOME -ErrorAction SilentlyContinue

    # Positive control: prove both ambient startup hooks run without isolation.
    # The actual preflight must leave this marker absent.
    $controlOutput = @(& $python -B -c 'pass' 2>&1)
    $controlExit = $LASTEXITCODE
    if ($controlExit -ne 0 -or -not (Test-Path -LiteralPath $marker)) {
        throw 'KELD-PREFLIGHT-TEST: ambient Python startup controls did not execute'
    }
    $controlMarker = [IO.File]::ReadAllText($marker)
    if ($controlMarker -notmatch 'PTH' -or $controlMarker -notmatch 'SITECUSTOMIZE') {
        throw 'KELD-PREFLIGHT-TEST: .pth and sitecustomize controls were not both observed'
    }
    Write-Output 'KELD_PREFLIGHT_CASE: ambient startup hook control'
    Remove-Item -LiteralPath $marker -Force

    $helper = Join-Path $PSScriptRoot 'developer_signer_spki.py'
    $priorPreference = $ErrorActionPreference
    try {
        # Windows PowerShell 5.1 turns redirected native stderr into ErrorRecord.
        # Keep that diagnostic as data so the helper's fail-closed exit is checked.
        $ErrorActionPreference = 'Continue'
        $malformedOutput = @('not-base64' | & $python -I -S -B $helper 2>&1)
        $malformedExit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $priorPreference
    }
    if ($malformedExit -eq 0 -or ($malformedOutput -join "`n") -notmatch 'SPKI_INPUT_REJECTED') {
        throw 'KELD-PREFLIGHT-TEST: malformed certificate input was not rejected'
    }
    Write-Output 'KELD_PREFLIGHT_CASE: malformed certificate input'

    if ($Thumbprint -ceq ('0' * 40)) { $missingThumbprint = '1' * 40 } else { $missingThumbprint = '0' * 40 }
    $missingCertificateArgs = @('-Thumbprint', $missingThumbprint,
        '-DiscoverPublisherScope')
    [void](Invoke-Case 'incorrect or missing certificate' $missingCertificateArgs $false '')
    $selectedCertificate = @(Get-ChildItem -Path 'Cert:\CurrentUser\My' -ErrorAction Stop |
        Where-Object { $_.Thumbprint.ToUpperInvariant() -ceq $Thumbprint.ToUpperInvariant() })
    if ($selectedCertificate.Count -ne 1) {
        throw 'KELD-PREFLIGHT-TEST: selected certificate is missing or ambiguous'
    }
    $certificateBytes = $selectedCertificate[0].RawData
    $truncatedCertificate = New-Object byte[] ($certificateBytes.Length - 1)
    [Array]::Copy($certificateBytes, 0, $truncatedCertificate, 0, $truncatedCertificate.Length)
    $malformedDerRejected = $false
    try {
        [void](Get-KeldPublisherScope $truncatedCertificate)
    } catch {
        if ($_.Exception.Message -match 'KELD-DEVELOPER-SIGNER-PREFLIGHT:') {
            $malformedDerRejected = $true
        } else {
            throw
        }
    }
    if (-not $malformedDerRejected) {
        throw 'KELD-PREFLIGHT-TEST: in-process parser accepted truncated certificate DER'
    }
    Write-Output 'KELD_PREFLIGHT_CASE: malformed certificate DER'
    Assert-Crypt32SpkiParity $certificateBytes
    Write-Output 'KELD_PREFLIGHT_CASE: Crypt32 SPKI DER parity'
    $encodedCertificate = [Convert]::ToBase64String($selectedCertificate[0].RawData)
    $priorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $oracleOutput = @($encodedCertificate | & $python -I -S -B $helper 2>&1)
        $oracleExit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $priorPreference
    }
    if ($oracleExit -ne 0 -or $oracleOutput.Count -ne 1 -or
        [string]$oracleOutput[0] -notmatch '^[0-9a-f]{64}$') {
        throw 'KELD-PREFLIGHT-TEST: independent Python SPKI oracle failed'
    }
    $independentScope = [string]$oracleOutput[0]
    if ((Test-Path -LiteralPath $marker) -or (Test-Path -LiteralPath $pythonLaunchMarker)) {
        throw 'KELD-PREFLIGHT-TEST: isolated SPKI oracle executed ambient Python startup hooks'
    }

    $discoveryArgs = @('-Thumbprint', $Thumbprint, '-DiscoverPublisherScope')
    $discovery = Invoke-Case 'discovery' $discoveryArgs $true 'DISCOVERY_ONLY publisherScope=[0-9a-f]{64}'
    if ((Test-Path -LiteralPath $marker) -or (Test-Path -LiteralPath $pythonLaunchMarker)) {
        throw 'KELD-PREFLIGHT-TEST: discovery launched ambient Python or ran startup hooks'
    }
    $scopeMatches = [regex]::Matches($discovery, 'DISCOVERY_ONLY publisherScope=(?<scope>[0-9a-f]{64})')
    if ($scopeMatches.Count -ne 1) { throw 'KELD-PREFLIGHT-TEST: non-unique discovery scope' }
    $scope = $scopeMatches[0].Groups['scope'].Value
    if ($scope -cne $independentScope) {
        throw 'KELD-PREFLIGHT-TEST: discovery scope differs from independent Python SPKI oracle'
    }
    Write-Output 'KELD_PREFLIGHT_CASE: independent SPKI discovery digest'
    $validArgs = @('-Thumbprint', $Thumbprint,
        '-ExpectedPublisherScope', $scope,
        '-SignToolPath', $SignToolPath,
        '-ExpectedSignToolSha256', $ExpectedSignToolSha256,
        '-ReleasePublicKeyPath', $ReleasePublicKeyPath)
    [void](Invoke-Case 'valid metadata' $validArgs $true 'PREFLIGHT_PASS publisherScope=[0-9a-f]{64}')
    if ((Test-Path -LiteralPath $marker) -or (Test-Path -LiteralPath $pythonLaunchMarker)) {
        throw 'KELD-PREFLIGHT-TEST: preflight launched ambient Python or executed startup hooks'
    }
    Write-Output 'KELD_PREFLIGHT_CASE: no ambient Python launch or startup hooks'

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
    Write-Output 'KELD_DEVELOPER_SIGNER_PREFLIGHT_TEST: 15/15'
} finally {
    if ($null -eq $savedPythonUserBase) {
        Remove-Item Env:PYTHONUSERBASE -ErrorAction SilentlyContinue
    } else {
        $env:PYTHONUSERBASE = $savedPythonUserBase
    }
    if ($null -eq $savedPythonPath) {
        Remove-Item Env:PYTHONPATH -ErrorAction SilentlyContinue
    } else {
        $env:PYTHONPATH = $savedPythonPath
    }
    if ($null -eq $savedPythonNoUserSite) {
        Remove-Item Env:PYTHONNOUSERSITE -ErrorAction SilentlyContinue
    } else {
        $env:PYTHONNOUSERSITE = $savedPythonNoUserSite
    }
    if ($null -eq $savedPythonHome) {
        Remove-Item Env:PYTHONHOME -ErrorAction SilentlyContinue
    } else {
        $env:PYTHONHOME = $savedPythonHome
    }
    if ($null -eq $savedPath) {
        Remove-Item Env:PATH -ErrorAction SilentlyContinue
    } else {
        $env:PATH = $savedPath
    }
    if ($null -eq $savedPathExt) {
        Remove-Item Env:PATHEXT -ErrorAction SilentlyContinue
    } else {
        $env:PATHEXT = $savedPathExt
    }
    if ($null -eq $savedLaunchMarker) {
        Remove-Item Env:KELD_TEST_PYTHON_LAUNCH_MARKER -ErrorAction SilentlyContinue
    } else {
        $env:KELD_TEST_PYTHON_LAUNCH_MARKER = $savedLaunchMarker
    }
    Remove-Item -LiteralPath $pythonFixture -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $pythonShimRoot -Recurse -Force -ErrorAction SilentlyContinue
    if ($null -ne $sha256) { $sha256.Dispose() }
    if ($null -ne $pythonHandle) { $pythonHandle.Dispose() }
}
