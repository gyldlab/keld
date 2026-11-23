# KEL-270 Windows v2 PerUserDirect NSIS builder (offline, unsigned candidate).
# Takes the W38 five-image stage; never embeds identity into a signed executable.
# The fixed NSIS source invokes the original signed Host's private install role.
# No invocation can compile until the ORIGINAL Host's T81/T20/D90/D46 owners
# commission its existing initial-install contract and the NSIS compiler pin.
# This script never signs, installs, elevates or publishes a release.
#
# Commissioning must be proven by the original Host/Guard/KEL-53 owners, not a
# new parallel verifier. A live D90 census and D46 one-use permit MUST be
# produced later by the admitted Host for the actual target user.
[CmdletBinding()]
param(
    [string]$StageDirectory,
    [string]$ExpectedStageManifestSha256,
    [string]$ExpectedSourceCommit,
    [string]$CommissionedCatalogue,
    [string]$ExpectedCatalogueSha256,
    [string]$MakensisExe,
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# The original KEL-270 Host still returns KELD-HOST-INITIAL-002 until its
# catalogue and native issuer are commissioned. This SHA256 pins only the
# actual NSIS 3.13 Bin/makensis.exe extracted from the official publisher
# archive (nsis-3.13.zip SHA256 BA63DFFC4410EE89193E1CB5A41989991BD77C61068DA17E3156D136B7B0B3D8).
# Independent Windows checks verified the archive, compiler bytes and /VERSION.
# A compiler pin cannot authorize Host, T81, T20, D90, D46, or release signing.
$reviewedMakensisSha256 = '65343C225EC88393CC4453B0963CAF63F763F42F48CB55736D51893FF2E29D4A'
# The exact reviewed NSIS source is a release input, not a caller-supplied pin.
# This does not authorize the uncommissioned original Host or release signing.
$reviewedNsisSourceSha256 = '760EE6ED9E91149A71A333479B655252375250BE530B5579C5452094543A0101'

function Refuse([string]$Reason) {
    throw "KEL270-NSIS-BUILD: $Reason; no installer built or release authority granted"
}

function Require-Hash([string]$Value, [int]$Length, [string]$What) {
    if ($null -eq $Value -or $Value -cnotmatch "^[0-9a-fA-F]{$Length}$") {
        Refuse "$What must be $Length independently pinned hexadecimal characters"
    }
    return $Value.ToUpperInvariant()
}

function Require-Fields($Object, [string[]]$Fields, [string]$What) {
    if ($null -eq $Object -or $Object -isnot [pscustomobject]) {
        Refuse "$What must be an object"
    }
    $actual = @($Object.PSObject.Properties.Name)
    if ($actual.Count -ne $Fields.Count) {
        Refuse "$What has missing or unreviewed fields"
    }
    foreach ($field in $Fields) {
        if ($actual -cnotcontains $field) {
            Refuse "$What lacks $field"
        }
    }
}

# A deliberately restricted local build-path alphabet prevents NSIS !define
# injection. These are build input/output locators only, never install roots.
function Require-LocalPath([string]$Value, [string]$What) {
    if ([string]::IsNullOrWhiteSpace($Value) -or
        $Value -cnotmatch '^[a-zA-Z]:\\' -or
        $Value.Contains('/') -or $Value.Substring(2).Contains(':') -or
        $Value -match '[\r\n]' -or
        $Value -match '[\x22\x24\x21\x25\x3b\x60]' -or
        $Value -match '(^|[\\/])\.\.([\\/]|$)') {
        Refuse "$What must be an absolute local drive path without aliases or NSIS metacharacters"
    }
    $full = [IO.Path]::GetFullPath($Value)
    $drive = New-Object IO.DriveInfo($Value.Substring(0, 1))
    if (-not [string]::Equals($full, $Value, [StringComparison]::OrdinalIgnoreCase) -or
        $drive.DriveType -ne [IO.DriveType]::Fixed) {
        Refuse "$What must use a normalized absolute path"
    }
    if ($Value.Length -gt 3) {
        foreach ($component in $Value.Substring(3).Split('\\')) {
            if ([string]::IsNullOrEmpty($component) -or
                $component -eq '.' -or $component -eq '..' -or
                $component.EndsWith('.') -or $component.EndsWith(' ') -or
                $component -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') {
                Refuse "$What has an ambiguous path component"
            }
        }
    }
    $part = $full
    while (-not [string]::IsNullOrEmpty($part)) {
        $item = Get-Item -LiteralPath $part -Force -ErrorAction SilentlyContinue
        if ($null -ne $item -and
            (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            Refuse "$What has a reparse-point ancestor"
        }
        $parent = [IO.Path]::GetDirectoryName($part)
        if ([string]::IsNullOrEmpty($parent) -or $parent -ceq $part) { break }
        $part = $parent
    }
    return $full
}

function Require-File([string]$Path, [string]$What) {
    $value = Require-LocalPath $Path $What
    if (-not (Test-Path -LiteralPath $value -PathType Leaf)) {
        Refuse "$What is not an existing regular file"
    }
    return $value
}

# Hash the exact opened file with the .NET SHA256 API. An ambient PowerShell
# Get-FileHash function, module or PATH script cannot select the digest source.
function Get-TrustedFileSha256([string]$Path) {
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        $stream = [IO.File]::Open(
            $Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read
        )
        try {
            return [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '')
        } finally {
            $stream.Dispose()
        }
    } finally {
        $sha256.Dispose()
    }
}

function Require-ExactHash([string]$Path, [string]$Expected, [string]$What) {
    if ((Get-TrustedFileSha256 $Path) -cne $Expected) {
        Refuse "$What differs from its independently pinned SHA256"
    }
}

function Require-Amd64Pe([string]$Path, [string]$What) {
    $stream = [IO.File]::OpenRead($Path)
    try {
        $reader = New-Object IO.BinaryReader($stream)
        if ($stream.Length -lt 512 -or $reader.ReadUInt16() -ne 0x5A4D) {
            Refuse "$What must be a complete PE image"
        }
        $stream.Position = 0x3c
        $offset = $reader.ReadInt32()
        if ($offset -lt 0x40 -or [long]$offset -gt $stream.Length - 6) {
            Refuse "$What has an invalid PE header offset"
        }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne 0x8664) {
            Refuse "$What must be a Windows AMD64 PE image"
        }
    } finally {
        $stream.Dispose()
    }
}

# Import the OS-installed Security module by its absolute manifest, then invoke
# its exported binary cmdlet handle. PATH and PSModulePath must not choose the
# signer verifier in an unsigned release-build process.
function Get-TrustedAuthenticodeSignature([string]$Path) {
    $manifest = [IO.Path]::Combine(
        $PSHOME, 'Modules', 'Microsoft.PowerShell.Security',
        'Microsoft.PowerShell.Security.psd1'
    )
    if (-not [IO.File]::Exists($manifest)) {
        Refuse 'built-in Windows PowerShell Security module is absent'
    }
    $modules = @(Microsoft.PowerShell.Core\Import-Module -Name $manifest -PassThru -ErrorAction Stop)
    if ($modules.Count -ne 1 -or
        -not [string]::Equals($modules[0].Path, $manifest,
            [StringComparison]::OrdinalIgnoreCase)) {
        Refuse 'built-in Windows PowerShell Security module was not loaded from its OS path'
    }
    $cmdlet = $modules[0].ExportedCommands['Get-AuthenticodeSignature']
    if ($cmdlet -isnot [System.Management.Automation.CmdletInfo] -or
        $cmdlet.ImplementingType.FullName -cne
            'Microsoft.PowerShell.Commands.GetAuthenticodeSignatureCommand') {
        Refuse 'built-in Authenticode verification cmdlet is unavailable'
    }
    return & $cmdlet -LiteralPath $Path
}

function Require-SignedImage([string]$Path, [string]$Digest, [string]$Signer, [string]$What) {
    Require-ExactHash $Path $Digest $What
    $item = Get-Item -LiteralPath $Path
    if ($item.Extension -cne '.exe') {
        Refuse "$What must be a native .exe"
    }
    Require-Amd64Pe $Path $What
    $signature = Get-TrustedAuthenticodeSignature $Path
    if ($signature.Status.ToString() -cne 'Valid' -or
        $signature.SignatureType.ToString() -cne 'Authenticode' -or
        $null -eq $signature.SignerCertificate -or
        $signature.SignerCertificate.Thumbprint.ToUpperInvariant() -cne $Signer) {
        Refuse "$What differs from its reviewed embedded Authenticode signer"
    }
    Require-ExactHash $Path $Digest $What
}

# Publication of an unsigned compiler candidate is only a custody step. The
# pre-rename hash must also match bytes reopened through the final destination;
# a successful directory move alone cannot establish that the published object
# is the one previously checked. This does NOT authorize signing or installing.
function Require-PublishedUnsignedCandidate([string]$OutputDirectory, [string]$ExpectedSha256) {
    $publishedRoot = Require-LocalPath $OutputDirectory 'published unsigned NSIS directory'
    if (-not (Test-Path -LiteralPath $publishedRoot -PathType Container)) {
        Refuse 'published unsigned NSIS directory is missing'
    }
    $candidateName = 'keld-v2-peruserdirect-UNSIGNED.exe'
    $contents = @(Get-ChildItem -LiteralPath $publishedRoot -Force)
    if ($contents.Count -ne 1 -or $contents[0] -isnot [IO.FileInfo] -or
        $contents[0].Name -cne $candidateName -or
        (($contents[0].Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        Refuse 'published unsigned NSIS directory contains a foreign or reparse object'
    }
    $published = Require-File (Join-Path $publishedRoot $candidateName) 'published unsigned NSIS candidate'
    Require-ExactHash $published $ExpectedSha256 'published unsigned NSIS candidate'
}

# A failed compiler may leave only this script's private unsigned candidate.
# Never descend into or remove an unexpected entry; retain it for diagnosis.
function Remove-OwnedPrivateCandidate([string]$Temporary, [string]$ExpectedParent) {
    if (-not [IO.Directory]::Exists($Temporary)) { return }
    if ([IO.Path]::GetDirectoryName($Temporary) -cne $ExpectedParent -or
        [IO.Path]::GetFileName($Temporary) -cnotmatch '^\.keld-v2-nsis-[0-9a-f]{32}$') {
        Refuse 'compiler cleanup lost its exact private sibling'
    }
    $null = Require-LocalPath $Temporary 'compiler temporary sibling'
    $items = @(Get-ChildItem -LiteralPath $Temporary -Force)
    $candidate = Join-Path $Temporary 'keld-v2-peruserdirect-UNSIGNED.exe'
    foreach ($item in $items) {
        if ($item -isnot [IO.FileInfo] -or
            $item.FullName -cne $candidate -or
            (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            Refuse 'compiler cleanup found a foreign or reparse-point object'
        }
        [IO.File]::Delete($candidate)
    }
    [IO.Directory]::Delete($Temporary, $false)
}

foreach ($field in @(
    'StageDirectory', 'ExpectedStageManifestSha256', 'ExpectedSourceCommit',
    'CommissionedCatalogue', 'ExpectedCatalogueSha256', 'OutputDirectory'
)) {
    if ([string]::IsNullOrWhiteSpace((Get-Variable -Name $field -ValueOnly))) {
        Refuse "missing required $field"
    }
}
if ($env:OS -cne 'Windows_NT') { Refuse 'Windows x64 native tooling is required' }
$manifestHash = Require-Hash $ExpectedStageManifestSha256 64 'W38 stage manifest SHA256'
$catalogueHash = Require-Hash $ExpectedCatalogueSha256 64 'commissioned catalogue SHA256'
if ($ExpectedSourceCommit -cnotmatch '^[0-9a-f]{40}$') {
    Refuse 'source commit must be the exact lowercase reviewed SHA'
}
$stage = Require-LocalPath $StageDirectory 'W38 stage directory'
if (-not (Test-Path -LiteralPath $stage -PathType Container)) {
    Refuse 'W38 stage directory is absent'
}
$destination = Require-LocalPath $OutputDirectory 'output directory'
if (Test-Path -LiteralPath $destination) {
    Refuse 'output directory must be absent; no merge or overwrite'
}
if (-not (Test-Path -LiteralPath (Split-Path -Parent $destination) -PathType Container)) {
    Refuse 'output parent must already exist'
}
$manifestPath = Require-File (Join-Path $stage 'manifest.json') 'W38 manifest'
Require-ExactHash $manifestPath $manifestHash 'W38 manifest'
try {
    $manifest = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($manifestPath)) -ErrorAction Stop
} catch {
    Refuse 'W38 manifest JSON is malformed'
}
Require-Fields $manifest @(
    'schema', 'state', 'sourceCommit', 'artifacts', 'providerRecipePath',
    'providerRecipeSha256', 'nativeKeldProfileAndInstallationAuthority'
) 'W38 manifest'
if ($manifest.schema -cne 'keld.windows-v2-signed-image-stage/v1' -or
    $manifest.state -cne 'non-authorizing-path-observations-only' -or
    $manifest.sourceCommit -cne $ExpectedSourceCommit -or
    $manifest.nativeKeldProfileAndInstallationAuthority -cne 'NOT_VERIFIED') {
    Refuse 'stage is not the exact independently pinned W38 non-authorizing source'
}
$layout = @(
    @('app-host', 'bin/keld-host.exe'),
    @('recovery-host', 'recovery/keld-host.exe'),
    @('cli', 'bin/keld.exe'),
    @('role-launcher', 'bin/keld-role-launcher.exe'),
    @('updater-helper', 'bin/keld-updater-helper.exe')
)
if ($null -eq $manifest.artifacts -or @($manifest.artifacts).Count -ne $layout.Count) {
    Refuse 'W38 stage must have exactly five original signed image roles'
}
$images = @()
for ($i = 0; $i -lt $layout.Count; $i++) {
    $entry = $manifest.artifacts[$i]
    $role = $layout[$i][0]
    $relative = $layout[$i][1]
    Require-Fields $entry @('role', 'relativePath', 'sha256', 'length', 'expectedSignerThumbprint') "stage role $role"
    if ($entry.role -cne $role -or $entry.relativePath -cne $relative) {
        Refuse "stage role $role has foreign role or payload path"
    }
    $digest = Require-Hash $entry.sha256 64 "$role SHA256"
    $signer = Require-Hash $entry.expectedSignerThumbprint 40 "$role signer"
    $original = Require-File (Join-Path $stage $relative) "$role signed original"
    if ($entry.length -isnot [long] -and $entry.length -isnot [int]) {
        Refuse "$role has invalid original image length"
    }
    if ((Get-Item -LiteralPath $original).Length -ne [long]$entry.length) {
        Refuse "$role image length differs from reviewed stage"
    }
    Require-SignedImage $original $digest $signer "$role signed original"
    $images += [pscustomobject]@{ role = $role; source = $original; digest = $digest }
}
if (@($images | Select-Object -ExpandProperty digest -Unique).Count -ne 5) {
    Refuse 'original App, Recovery, CLI, launcher and helper must all have distinct reviewed hashes'
}
if ($manifest.providerRecipePath -cne 'bindings/keld-mcp-stdio.json') {
    Refuse 'W38 provider recipe must use the fixed relative path'
}
$providerHash = Require-Hash $manifest.providerRecipeSha256 64 'W38 provider recipe SHA256'
$provider = Require-File (Join-Path $stage 'bindings/keld-mcp-stdio.json') 'W38 provider recipe'
Require-ExactHash $provider $providerHash 'W38 provider recipe'
$catalogue = Require-File $CommissionedCatalogue 'commissioned catalogue candidate'
Require-ExactHash $catalogue $catalogueHash 'commissioned catalogue candidate'
$source = Require-File (Join-Path $PSScriptRoot 'windows_v2_nsis_installer.nsi') 'fixed NSIS builder source'
Require-ExactHash $source $reviewedNsisSourceSha256 'reviewed NSIS source'

# This is the currently missing ORIGINAL-HOST native commissioning boundary.
# A signed W38 stage or caller-supplied catalogue/hash cannot authorize it.
# The published original Host returns KELD-HOST-INITIAL-002 on this role. No
# safe read-only predicate exists to establish its commissioned T81/T20/D90/D46
# implementations for this build. Never add a second authority executable or
# treat a receipt/boolean from arguments as a permit. The original owners must
# first land a reviewed, non-effectful predicate on their existing boundary.
Refuse 'KEL270-NSIS-OWNER-UNCOMMISSIONED original Host T81/T20/D90/D46 catalogue and first-effect authority are not commissioned'

# Only a future reviewed original-Host integration may reach this toolchain
# branch. The source-controlled compiler pin must also name real reviewed
# makensis bytes; a user-supplied compiler digest is never enough.
if ($reviewedMakensisSha256 -cnotmatch '^[0-9A-F]{64}$') {
    Refuse 'reviewed NSIS compiler binary is not pinned on this source head'
}
$compiler = Require-File $MakensisExe 'reviewed makensis.exe'
Require-ExactHash $compiler $reviewedMakensisSha256 'reviewed makensis.exe'
$parent = Split-Path -Parent $destination
$null = Require-LocalPath $parent 'compiler output parent'
$temporary = Join-Path $parent ('.keld-v2-nsis-' + [guid]::NewGuid().ToString('N'))
$installer = Join-Path $temporary 'keld-v2-peruserdirect-UNSIGNED.exe'
$temporaryCreated = $false
try {
    if (Test-Path -LiteralPath $temporary) {
        Refuse 'private compiler sibling already exists'
    }
    [IO.Directory]::CreateDirectory($temporary) | Out-Null
    $temporaryCreated = $true
    $null = Require-LocalPath $temporary 'private compiler sibling'
    if (@(Get-ChildItem -LiteralPath $temporary -Force).Count -ne 0) {
        Refuse 'private compiler sibling is not empty'
    }
    $defines = @(
        '/NOCONFIG', '/WX', '/INPUTCHARSET', 'UTF8',
        "/DKELD_UNSIGNED_OUTPUT=$installer",
        "/DKELD_APP_HOST=$($images[0].source)",
        "/DKELD_RECOVERY_HOST=$($images[1].source)",
        "/DKELD_CLI=$($images[2].source)",
        "/DKELD_ROLE_LAUNCHER=$($images[3].source)",
        "/DKELD_UPDATER_HELPER=$($images[4].source)",
        "/DKELD_PROVIDER_RECIPE=$provider",
        "/DKELD_CATALOGUE=$catalogue",
        $source
    )
    $null = & $compiler @defines
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $installer -PathType Leaf)) {
        Refuse 'pinned NSIS compilation failed; no installer acceptance can be claimed'
    }
    foreach ($image in $images) {
        Require-ExactHash $image.source $image.digest "$($image.role) after compile"
    }
    Require-ExactHash $catalogue $catalogueHash 'commissioned catalogue after compile'
    Require-ExactHash $manifestPath $manifestHash 'W38 manifest after compile'
    Require-ExactHash $provider $providerHash 'W38 provider recipe after compile'
    $candidateHash = Get-TrustedFileSha256 $installer
    $null = Require-LocalPath $parent 'compiler output parent'
    $null = Require-LocalPath $temporary 'private compiler sibling'
    if (Test-Path -LiteralPath $destination) {
        Refuse 'compiler output destination appeared before publication'
    }
    [IO.Directory]::Move($temporary, $destination)
    $temporaryCreated = $false
    # Never report an unsigned candidate from its pre-move digest alone.
    # A failed final-path observation deliberately retains the published
    # directory for examination, without attempting a destructive rollback.
    Require-PublishedUnsignedCandidate $destination $candidateHash
} catch {
    $failure = $_
    if ($temporaryCreated) {
        try {
            Remove-OwnedPrivateCandidate $temporary $parent
        } catch {
            throw "KEL270-NSIS-BUILD: compiler cleanup refused; original failure: $failure; cleanup failure: $_"
        }
    }
    throw $failure
}
Write-Output "KEL270_NSIS_UNSIGNED_CANDIDATE_COMPILED sha256=$candidateHash signing=NOT_RUN installed=NOT_RUN public_trust_release=NOT_VERIFIED"
