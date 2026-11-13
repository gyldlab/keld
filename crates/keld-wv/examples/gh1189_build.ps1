# GH-1189 test-only Windows build. This script MUST NOT run either native executable.
# Independent exact-target review and an exclusive Windows operator slot are required
# before -Build. Source and manifest hashes bind the reviewed input set.
[CmdletBinding()]
param([switch]$Build)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Requires native Windows' }

$here = Split-Path -Parent $PSCommandPath
$repo = (Resolve-Path (Join-Path $here '..\..\..')).Path
$manifest = Join-Path $here 'gh1189_taskdialog_v6.manifest'
$target = 'x86_64-pc-windows-msvc'
$feature = 'gh1189-dialog-fixture'

# Freeze each independently reviewed input. Reject substitutions before and after a build.
$inputs = @(
    @{ Path = (Join-Path $here 'gh1189_taskdialog_fixture.rs'); Hash = '7BBF3A6C46DC170393218CD8FEF68D7DB8E576CA3F2FB819885E0493D76F1040' }
    @{ Path = (Join-Path $here 'gh1189_winevent_observer.rs'); Hash = 'FD10CA0186B3477CB75276B1D5D851A2F011542FAB78ACAFDF113646E6959411' }
    @{ Path = $manifest; Hash = '603CCDA7A6EB46E45DC97F9007D6AC1159DDEB5997648C36B9BDA6DE79E0E981' }
    @{ Path = (Join-Path $here 'AGENTS.md'); Hash = 'FAF612F367BB7ED93AF08CD3536C5AB5D10A18942A9ADBF446E743E78F8E2B9D' }
    @{ Path = (Join-Path $here '../AGENTS.md'); Hash = '03B9209D0BE29D4F73EBB15ADF798912BD3606A503D7A2553968B7BBA1781C1E' }
    @{ Path = (Join-Path $here '../Cargo.toml'); Hash = '2FD5837FBDD1B03D7B85CAAEC69D59BF183DC454ABC6FCAB2A4BF3B4D752916A' }
)
function Assert-ReviewedInputs {
    foreach ($input in $inputs) {
        if (-not (Test-Path -LiteralPath $input.Path -PathType Leaf)) {
            throw "Reviewed input is missing: $($input.Path)"
        }
        $current = (Get-FileHash -LiteralPath $input.Path -Algorithm SHA256).Hash
        if ($current -ne $input.Hash) {
            throw "Source hash differs from reviewed input: $($input.Path)"
        }
    }
}
Assert-ReviewedInputs
if (-not $Build) {
    Write-Output 'PREVIEW ONLY: reviewed source, FFI ownership, Cargo target and manifest hashes match.'
    Write-Output "PREVIEW ONLY: build example gh1189_taskdialog_fixture for $target, feature $feature, with Common-Controls v6 manifest."
    Write-Output "PREVIEW ONLY: build example gh1189_winevent_observer for $target, feature $feature."
    Write-Output 'No compiler, native UI, or test process was started.'
    return
}

# The default PowerShell PATH can select Git's link.exe and omit the Windows SDK mt.exe.
# Load a detected Visual Studio x64 developer environment in this process only.
$programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
if ([string]::IsNullOrWhiteSpace($programFilesX86)) { throw 'ProgramFiles(x86) is unavailable' }
$vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { throw 'Visual Studio discovery tool unavailable' }
$install = @(& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
if ($LASTEXITCODE -ne 0 -or $install.Count -ne 1 -or [string]::IsNullOrWhiteSpace($install[0])) {
    throw 'Cannot identify exactly one supported Visual Studio installation'
}
$vsRoot = (Resolve-Path -LiteralPath $install[0]).Path
$vcvars = Join-Path $vsRoot 'VC/Auxiliary/Build/vcvars64.bat'
if (-not (Test-Path -LiteralPath $vcvars -PathType Leaf)) { throw 'MSVC x64 environment script missing' }
$cmdLine = 'call "' + $vcvars + '" >nul && set'
$environment = @(& $env:ComSpec /d /s /c $cmdLine)
if ($LASTEXITCODE -ne 0) { throw 'MSVC environment setup failed' }
foreach ($entry in $environment) {
    $separator = $entry.IndexOf('=')
    if ($separator -gt 0) {
        [Environment]::SetEnvironmentVariable(
            $entry.Substring(0, $separator), $entry.Substring($separator + 1), 'Process')
    }
}
$link = (Get-Command link.exe -ErrorAction Stop).Source
$msvcPrefix = $vsRoot.TrimEnd('\') + '\'
if (-not $link.StartsWith($msvcPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "MSVC linker was not selected: $link"
}
$mt = (Get-Command mt.exe -ErrorAction Stop).Source
$kitPrefix = (Join-Path $programFilesX86 'Windows Kits/10/bin').TrimEnd('\') + '\'
if (-not $mt.StartsWith($kitPrefix, [StringComparison]::OrdinalIgnoreCase) -or
    -not $mt.EndsWith('\x64\mt.exe', [StringComparison]::OrdinalIgnoreCase)) {
    throw "Windows SDK x64 manifest tool was not selected: $mt"
}
if ($manifest.Length -ge 260) { throw 'Manifest input path exceeds linker limit' }
Assert-ReviewedInputs

Push-Location $repo
try {
    & cargo rustc -p keld-wv --locked --target $target --features $feature --example gh1189_taskdialog_fixture -- -C 'link-arg=/MANIFEST:EMBED' -C "link-arg=/MANIFESTINPUT:$manifest"
    if ($LASTEXITCODE -ne 0) { throw 'TaskDialog fixture build failed' }
    & cargo build -p keld-wv --locked --target $target --features $feature --example gh1189_winevent_observer
    if ($LASTEXITCODE -ne 0) { throw 'WinEvent observer build failed' }
    Assert-ReviewedInputs
    $metadata = & cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cargo target lookup failed' }
    $binDir = Join-Path $metadata.target_directory "$target\debug\examples"
    $fixture = Join-Path $binDir 'gh1189_taskdialog_fixture.exe'
    $observer = Join-Path $binDir 'gh1189_winevent_observer.exe'
    if (-not (Test-Path -LiteralPath $fixture -PathType Leaf) -or
        -not (Test-Path -LiteralPath $observer -PathType Leaf)) { throw 'Expected executables missing' }
    $probeDir = Join-Path $metadata.target_directory 'gh1189'
    New-Item -ItemType Directory -Path $probeDir -Force | Out-Null
    $probe = Join-Path $probeDir 'fixture-manifest-resource1.xml'
    & $mt "-inputresource:$fixture;#1" "-out:$probe"
    if ($LASTEXITCODE -ne 0) { throw 'Manifest resource extraction failed' }
    $embedded = [IO.File]::ReadAllText($probe)
    if ($embedded -notmatch 'Microsoft\.Windows\.Common-Controls' -or
        $embedded -notmatch '6\.0\.0\.0') { throw 'Required v6 Common-Controls manifest absent' }
    Write-Output "BUILT, NOT RUN: $fixture"
    Write-Output "BUILT, NOT RUN: $observer"
    foreach ($exe in @($fixture, $observer)) {
        $digest = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        Write-Output "EXECUTABLE SHA256: $digest $exe"
    }
    Write-Output "EMBEDDED MANIFEST VERIFIED: $probe"
} finally {
    Pop-Location
}
