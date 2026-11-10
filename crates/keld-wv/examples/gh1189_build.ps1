# GH-1189 test-only Windows build. This script MUST NOT run either native executable.
# Independent exact-target review and an exclusive Windows operator slot are required
# before -Build. Source and manifest hashes bind the reviewed input set.
[CmdletBinding()]
param([switch]$Build, [switch]$Preflight, [string]$ReviewedCommit)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Requires native Windows' }

$here = Split-Path -Parent $PSCommandPath
$repo = (Resolve-Path (Join-Path $here '..\..\..')).Path
$manifest = Join-Path $here 'gh1189_taskdialog_v6.manifest'
$target = 'x86_64-pc-windows-msvc'
$feature = 'gh1189-dialog-fixture'

# Freeze each independently reviewed input. Reject substitutions before and after a build.
$inputs = @(
    @{ Path = (Join-Path $here 'gh1189_taskdialog_fixture.rs'); Hash = 'BD92FA05F3EE09070A022D27D9503D0594CE55268E43830930690686724FB496' }
    @{ Path = (Join-Path $here 'gh1189_winevent_observer.rs'); Hash = 'F9A265AE12D6CEEF79D51E67AC3E525DD9580EBEE2383EE82BCAEB8721314246' }
    @{ Path = $manifest; Hash = '3CBF2DF66E7AACB0A4992FF2F50C2F2A6010C20BF133CD6E6E29C1822B56D84E' }
    @{ Path = (Join-Path $here 'AGENTS.md'); Hash = '2032F4FF2D26B50BEB6C3B626C95DFCEA2D5153B814D46519BE3C32C41F69BEB' }
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
if (-not $Build -and -not $Preflight) {
    Write-Output 'PREVIEW ONLY: reviewed source, FFI ownership, Cargo target and manifest hashes match.'
    Write-Output "PREVIEW ONLY: build example gh1189_taskdialog_fixture for $target, feature $feature, with Common-Controls v6 manifest."
    Write-Output "PREVIEW ONLY: build example gh1189_winevent_observer for $target, feature $feature."
    Write-Output 'No compiler, native UI, or test process was started.'
    return
}
if ($Build -and $Preflight) { throw 'Choose either -Build or -Preflight, never both' }
if ($ReviewedCommit -notmatch '^[a-f0-9]{40}$') { throw 'Exact reviewed commit is required before any build preflight' }
$head = (& git -C $repo rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $head -ne $ReviewedCommit) { throw 'Reviewed HEAD mismatch before build' }
$tree = (& git -C $repo rev-parse 'HEAD^{tree}').Trim()
if ($LASTEXITCODE -ne 0 -or $tree -notmatch '^[a-f0-9]{40}$') { throw 'Cannot attest committed source tree' }
$dirty = @(& git -C $repo status --porcelain=v1 --untracked-files=all)
if ($LASTEXITCODE -ne 0 -or $dirty.Count -ne 0) { throw 'Unreviewed dirty checkout; build refused' }

# The scoped native builder must not inherit unreviewed Cargo output locations,
# compiler overrides, wrapper binaries or injected build flags.
function Assert-NoBuildOverrides {
    # Cargo accepts target-specific LINKER/RUSTFLAGS, profile and registry
    # overrides under arbitrary CARGO_* names, not only CARGO_TARGET_DIR.
    # Refuse every ambient Cargo environment key rather than maintaining a
    # list that silently misses a future flag. Apply before and after build.
    $blocked = @(Get-ChildItem Env: | Where-Object {
        $_.Name -match '^(CARGO($|_)|RUSTC($|_)|RUSTFLAGS$|RUSTDOC($|_)|RUSTUP_TOOLCHAIN$|CC($|_)|CXX($|_)|AR($|_)|CFLAGS$|CXXFLAGS$|CL$|_CL_$|LINK$|_LINK_$)'
    })
    if ($blocked.Count -ne 0) {
        $names = (($blocked | ForEach-Object { $_.Name } | Sort-Object -Unique) -join ',')
        throw "Unreviewed build environment override refused: $names"
    }
}
Assert-NoBuildOverrides
# Reject optional ambient Cargo config. Only committed workspace inputs
# and the pinned toolchain are admitted for this single-purpose witness.
$configRoots = @($repo, $HOME)
$cursor = [IO.Directory]::GetParent($repo)
while ($null -ne $cursor) {
    $configRoots += $cursor.FullName
    $cursor = $cursor.Parent
}
function Assert-NoExternalCargoConfig {
    foreach ($configRoot in ($configRoots | Select-Object -Unique)) {
        foreach ($name in @('config','config.toml')) {
            $config = Join-Path $configRoot ('.cargo/' + $name)
            if (Test-Path -LiteralPath $config -PathType Leaf) {
                throw "Unreviewed Cargo configuration file refused: $config"
            }
        }
    }
}
Assert-NoExternalCargoConfig
function Assert-NoReparsePath([string]$path) {
    $cursor = [IO.Path]::GetFullPath($path)
    while ($true) {
        $item = $null
        try { $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop }
        catch [System.Management.Automation.ItemNotFoundException] { }
        if ($null -ne $item -and
            ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Build path aliases another owner: $cursor"
        }
        $parent = [IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName
    }
}
# Pin BOTH rustup PATH proxies and the compiler/cargo executables selected
# behind the proxy. Version text alone is forgeable by an unreviewed wrapper.
# These identities are reviewed for this RAMANI x64 MSVC test-only toolchain.
function Assert-ReviewedRustTools {
    $shimRoot = Join-Path $HOME '.cargo\bin'
    $toolRoot = Join-Path $HOME '.rustup\toolchains\1.99.1-x86_64-pc-windows-msvc\bin'
    $shimHash = '00F139DA614DCED87D3C3D4121C43EEBDAC3C1BF8A5EE86B9D80DFD71C8D70FD'
    $cargoHash = 'AA5B8980BE3681899FDE765BA47E2ED361463AE9DD8EE7CD1963C18D23743A0F'
    $rustcHash = 'BA03E3BA03A73D9FEF19AB3659140F94B43615EC1D587FE9BEFC9AED3BD5AA72'
    foreach ($name in @('cargo.exe','rustc.exe','rustup.exe')) {
        $expected = [IO.Path]::GetFullPath((Join-Path $shimRoot $name))
        $selected = [IO.Path]::GetFullPath((Get-Command $name -ErrorAction Stop).Source)
        if (-not [string]::Equals($selected,$expected,[StringComparison]::OrdinalIgnoreCase)) {
            throw "Unreviewed Rust PATH shim selected: $selected"
        }
        Assert-NoReparsePath $selected
        if ((Get-FileHash -LiteralPath $selected -Algorithm SHA256).Hash -ne $shimHash) {
            throw "Unreviewed Rust PATH shim bytes: $selected"
        }
    }
    $rustup = Join-Path $shimRoot 'rustup.exe'
    foreach ($name in @('cargo','rustc')) {
        $expected = [IO.Path]::GetFullPath((Join-Path $toolRoot ($name+'.exe')))
        $selected = [IO.Path]::GetFullPath(((& $rustup which $name) -join ''))
        if ($LASTEXITCODE -ne 0 -or
            -not [string]::Equals($selected,$expected,[StringComparison]::OrdinalIgnoreCase)) {
            throw "Unreviewed rustup-selected compiler tool: $name"
        }
        Assert-NoReparsePath $selected
        $pin = $(if ($name -eq 'cargo') { $cargoHash } else { $rustcHash })
        if ((Get-FileHash -LiteralPath $selected -Algorithm SHA256).Hash -ne $pin) {
            throw "Unreviewed effective Rust tool bytes: $selected"
        }
    }
    $active = ((& $rustup show active-toolchain) -join [Environment]::NewLine)
    if ($LASTEXITCODE -ne 0 -or
        -not $active.StartsWith('1.99.1-x86_64-pc-windows-msvc', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Unreviewed active Rust toolchain'
    }
    return [ordered]@{
        cargo_shim_path = (Join-Path $shimRoot 'cargo.exe')
        rustc_shim_path = (Join-Path $shimRoot 'rustc.exe')
        rustup_shim_path = $rustup
        shim_sha256 = $shimHash
        effective_cargo_path = (Join-Path $toolRoot 'cargo.exe')
        effective_rustc_path = (Join-Path $toolRoot 'rustc.exe')
        effective_cargo_sha256 = $cargoHash
        effective_rustc_sha256 = $rustcHash
        active_toolchain = $active
    }
}
$rustToolPins = Assert-ReviewedRustTools
$rootInputs = [ordered]@{
    cargo_toml_sha256 = (Get-FileHash -LiteralPath (Join-Path $repo 'Cargo.toml') -Algorithm SHA256).Hash
    cargo_lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $repo 'Cargo.lock') -Algorithm SHA256).Hash
    rust_toolchain_sha256 = (Get-FileHash -LiteralPath (Join-Path $repo 'rust-toolchain.toml') -Algorithm SHA256).Hash
}
$manifestPath = Join-Path $repo 'Cargo.toml'
Assert-NoReparsePath $manifestPath
Assert-NoReparsePath (Join-Path $repo 'Cargo.lock')
Assert-NoReparsePath (Join-Path $repo 'rust-toolchain.toml')
$metadata = & cargo metadata --manifest-path $manifestPath --locked --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata preflight failed' }
$expectedWorkspace = [IO.Path]::GetFullPath($repo).TrimEnd('\')
$actualWorkspace = [IO.Path]::GetFullPath([string]$metadata.workspace_root).TrimEnd('\')
$expectedTarget = [IO.Path]::GetFullPath((Join-Path $repo 'target')).TrimEnd('\')
$actualTarget = [IO.Path]::GetFullPath([string]$metadata.target_directory).TrimEnd('\')
if (-not [string]::Equals($actualWorkspace, $expectedWorkspace, [StringComparison]::OrdinalIgnoreCase) -or
    -not [string]::Equals($actualTarget, $expectedTarget, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Configured Cargo target is not task-owned; build refused BEFORE compilation'
}
Assert-NoReparsePath $actualTarget
# Cargo may also write host build-script and dependency artifacts outside the
# two example EXE paths. Disallow ALL existing target-local reparse entries
# before compilation, not just the known binary and receipt destinations.
function Assert-NoTargetAliases {
    if (-not (Test-Path -LiteralPath $actualTarget -PathType Container)) { return }
    $pending = New-Object 'System.Collections.Generic.Stack[string]'
    $pending.Push($actualTarget)
    $count = 0
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        foreach ($item in @(Get-ChildItem -LiteralPath $directory -Force -ErrorAction Stop)) {
            $count++
            if ($count -gt 100000) { throw 'Too many target entries for safe ownership scan' }
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Cargo output subtree contains a reparse-point alias: $($item.FullName)"
            }
            if ($item.PSIsContainer) { $pending.Push($item.FullName) }
        }
    }
}
Assert-NoTargetAliases
$binDir = Join-Path $actualTarget 'x86_64-pc-windows-msvc/debug/examples'
Assert-NoReparsePath $binDir
$probeDir = Join-Path $actualTarget 'gh1189'
Assert-NoReparsePath $probeDir
$cargoVersion = (& cargo --version) -join [Environment]::NewLine
if ($LASTEXITCODE -ne 0) { throw 'Cargo version query failed' }
$rustcVersion = (& rustc -vV) -join [Environment]::NewLine
if ($LASTEXITCODE -ne 0) { throw 'Rust compiler version query failed' }

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
if ($Preflight) {
    Write-Output "BUILD PREFLIGHT PASS, NOT COMPILED: reviewed_commit=$ReviewedCommit"
    Write-Output "BUILD PREFLIGHT PASS: owned_target=$actualTarget"
    Write-Output "BUILD PREFLIGHT PASS: cargo=$cargoVersion; rustc=$($rustcVersion.Split([Environment]::NewLine)[0])"
    return
}

# Recheck review inputs and ownership directly before the first compiler.
Assert-NoReparsePath $binDir
Assert-NoReparsePath $probeDir
$fixture = Join-Path $binDir 'gh1189_taskdialog_fixture.exe'
$observer = Join-Path $binDir 'gh1189_winevent_observer.exe'
Assert-NoReparsePath $fixture
Assert-NoReparsePath $observer
$buildId = [guid]::NewGuid().ToString('N')
$buildDir = Join-Path $probeDir ('builds\' + $buildId)
Assert-NoReparsePath $buildDir
$expectedRootSha = ($rootInputs | ConvertTo-Json -Compress)
function Assert-ReviewedSnapshot {
    Assert-ReviewedInputs
    if ((& git -C $repo rev-parse HEAD).Trim() -ne $ReviewedCommit) {
        throw 'Reviewed HEAD changed during build'
    }
    if (@(& git -C $repo status --porcelain=v1 --untracked-files=all).Count -ne 0) {
        throw 'Checkout changed during build'
    }
    Assert-NoBuildOverrides
    Assert-NoExternalCargoConfig
    $currentTools = Assert-ReviewedRustTools
    if (($currentTools | ConvertTo-Json -Compress) -ne ($rustToolPins | ConvertTo-Json -Compress)) {
        throw 'Effective Rust compiler/Cargo identity changed during build'
    }
    $currentRoot = [ordered]@{
        cargo_toml_sha256 = (Get-FileHash -LiteralPath (Join-Path $repo 'Cargo.toml') -Algorithm SHA256).Hash
        cargo_lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $repo 'Cargo.lock') -Algorithm SHA256).Hash
        rust_toolchain_sha256 = (Get-FileHash -LiteralPath (Join-Path $repo 'rust-toolchain.toml') -Algorithm SHA256).Hash
    }
    if (($currentRoot | ConvertTo-Json -Compress) -ne $expectedRootSha) {
        throw 'Reviewed root Cargo/lock/toolchain inputs changed during build'
    }
}
Assert-ReviewedSnapshot
Assert-NoTargetAliases
Push-Location $repo
try {
    & cargo rustc --manifest-path $manifestPath -p keld-wv --locked --target $target --features $feature --example gh1189_taskdialog_fixture -- -C 'link-arg=/MANIFEST:EMBED' -C "link-arg=/MANIFESTINPUT:$manifest"
    if ($LASTEXITCODE -ne 0) { throw 'TaskDialog fixture build failed' }
    & cargo build --manifest-path $manifestPath -p keld-wv --locked --target $target --features $feature --example gh1189_winevent_observer
    if ($LASTEXITCODE -ne 0) { throw 'WinEvent observer build failed' }
    Assert-ReviewedSnapshot
    Assert-NoTargetAliases
    $postMetadata = & cargo metadata --manifest-path $manifestPath --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or
        -not [string]::Equals([IO.Path]::GetFullPath([string]$postMetadata.target_directory).TrimEnd('\'), $actualTarget,
            [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Cargo target changed after compilation'
    }
    foreach ($exe in @($fixture, $observer)) {
        Assert-NoReparsePath $exe
        if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "Expected EXE missing: $exe" }
    }
    Assert-NoReparsePath $buildDir
    New-Item -ItemType Directory -Path $buildDir -Force -ErrorAction Stop | Out-Null
    Assert-NoReparsePath $buildDir
    $probe = Join-Path $buildDir 'fixture-manifest-resource1.xml'
    Assert-NoReparsePath $probe
    & $mt "-inputresource:$fixture;#1" "-out:$probe"
    if ($LASTEXITCODE -ne 0) { throw 'Manifest resource extraction failed' }
    Assert-NoReparsePath $probe
    $embedded = [IO.File]::ReadAllText($probe)
    if ($embedded -notmatch 'Microsoft\.Windows\.Common-Controls' -or
        $embedded -notmatch '6\.0\.0\.0') { throw 'Required v6 Common-Controls manifest absent' }
    $fixtureSha = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash
    $observerSha = (Get-FileHash -LiteralPath $observer -Algorithm SHA256).Hash
    Assert-ReviewedSnapshot

    $receipt = [ordered]@{
        status = 'BUILT_NOT_RUN_PROVENANCE'
        reviewed_commit = $ReviewedCommit
        reviewed_tree = $tree
        built_utc = [DateTime]::UtcNow.ToString('o')
        build_id = $buildId
        workspace_root = $expectedWorkspace
        target_directory = $actualTarget
        root_inputs = $rootInputs
        pinned_inputs = @($inputs | ForEach-Object {
            $canonicalInput = (Resolve-Path -LiteralPath $_.Path).Path
            [ordered]@{ path=$canonicalInput.Substring($repo.Length + 1).Replace('\','/'); sha256=$_.Hash }
        })
        cargo_version = $cargoVersion
        rustc_version = $rustcVersion
        rust_tools = $rustToolPins
        vcvars64_path = $vcvars
        vcvars64_sha256 = (Get-FileHash -LiteralPath $vcvars -Algorithm SHA256).Hash
        linker_exe_sha256 = (Get-FileHash -LiteralPath $link -Algorithm SHA256).Hash
        mt_exe_sha256 = (Get-FileHash -LiteralPath $mt -Algorithm SHA256).Hash
        linker_exe = $link
        mt_exe = $mt
        build_script_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash
        fixture_sha256 = $fixtureSha
        observer_sha256 = $observerSha
        manifest_resource1_sha256 = (Get-FileHash -LiteralPath $probe -Algorithm SHA256).Hash
        no_external_cargo_config_or_flag_override = $true
    }
    $final = Join-Path $buildDir 'receipt.json'
    $staged = $final + '.partial-' + [guid]::NewGuid().ToString('N')
    Assert-NoReparsePath $final
    Assert-NoReparsePath $staged
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes((ConvertTo-Json -InputObject $receipt -Depth 8))
    try {
        $stream = [IO.File]::Open($staged, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try {
            $stream.Write($bytes, 0, $bytes.Length)
            $stream.Flush($true)
        } finally {
            $stream.Dispose()
        }
        Assert-NoReparsePath $staged
        Assert-NoReparsePath $final
        [IO.File]::Move($staged, $final)
    } catch {
        try { if ([IO.File]::Exists($staged)) { [IO.File]::Delete($staged) } } catch { }
        throw
    }
    Write-Output "BUILT, NOT RUN: $fixture"
    Write-Output "BUILT, NOT RUN: $observer"
    Write-Output "EXECUTABLE SHA256: $fixtureSha $fixture"
    Write-Output "EXECUTABLE SHA256: $observerSha $observer"
    Write-Output "EMBEDDED MANIFEST VERIFIED: $probe"
    Write-Output "BUILD_RECEIPT: $final"
    Write-Output "BUILD_RECEIPT_SHA256: $((Get-FileHash -LiteralPath $final -Algorithm SHA256).Hash)"
} finally {
    Pop-Location
}
