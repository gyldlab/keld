# KEL-270 Windows x64 release staging for the original Host + CLI/provider.
# Reads an independently pinned release-input record and stages signed files.
# This is OFFLINE CANDIDATE CUSTODY only: it never installs, registers a provider,
# signs an image, verifies a Keld profile/OID, or grants the Host initial role.
# Native T81/T82/D90/T20/D46 admission and installer signing remain separate.
# Usage: powershell.exe -NoProfile -NonInteractive -File tools/windows_v2_release_stage.ps1
#        -PinsFile <owner-reviewed-json> -OutputDirectory <absent-directory>
[CmdletBinding()]
param(
    [string]$PinsFile,
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Refuse([string]$Reason) {
    throw "KEL270-STAGE: $Reason; no release/install authority granted"
}

function Require-Fields($Object, [string[]]$Names, [string]$Where) {
    if ($null -eq $Object -or $Object -isnot [pscustomobject]) {
        Refuse "$Where must be a JSON object"
    }
    $actual = @($Object.PSObject.Properties.Name)
    if ($actual.Count -ne $Names.Count) {
        Refuse "$Where has missing or unknown fields"
    }
    foreach ($name in $Names) {
        if ($actual -cnotcontains $name) {
            Refuse "$Where lacks required $name"
        }
    }
}

function Require-Hash([string]$Value, [string]$Where, [int]$Length) {
    if ($null -eq $Value -or $Value.Length -ne $Length -or
        $Value -cnotmatch '^[0-9a-fA-F]+$') {
        Refuse "$Where must be $Length hexadecimal characters"
    }
    return $Value.ToUpperInvariant()
}

# Hash the file using an explicit .NET stream and SHA256 implementation. Ambient
# PATH scripts, functions and PSModulePath exports must not choose digest bytes.
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

# Expected generated metadata hashes must come from the intended serialized
# bytes, not from a first read of the writable staging path.
function Get-TrustedBytesSha256([byte[]]$Bytes) {
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString($sha256.ComputeHash($Bytes)).Replace('-', '')
    } finally { $sha256.Dispose() }
}

function Write-NewStageBytes([string]$Path, [byte[]]$Bytes) {
    $null = Require-CanonicalLocalPath $Path 'generated stage metadata'
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew,
        [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($Bytes, 0, $Bytes.Length) }
    finally { $stream.Dispose() }
}

# Use the actual PSHOME module manifest and its exported binary CmdletInfo,
# independent of command discovery through PATH, aliases or PSModulePath.
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

# IsPathRooted also accepts drive-relative "C:foo" and root-relative "\foo".
# Allow only canonical local fixed-drive paths with no reparse ancestors.
# No release-owner staging root is commissioned here: the caller-selected
# local parent and these path observations remain NONAUTHORIZING custody.
function Require-CanonicalLocalPath([string]$Value, [string]$Where) {
    if ([string]::IsNullOrWhiteSpace($Value) -or
        $Value -cnotmatch '^[A-Za-z]:\\' -or $Value.Contains('/') -or
        $Value.Substring(2).Contains(':')) {
        Refuse "$Where requires a fully qualified local drive path"
    }
    try {
        $canonical = [IO.Path]::GetFullPath($Value)
        $drive = New-Object IO.DriveInfo($Value.Substring(0, 1))
    } catch {
        Refuse "$Where requires a valid fully qualified local drive path"
    }
    if (-not [string]::Equals($canonical, $Value,
            [StringComparison]::OrdinalIgnoreCase) -or
        $drive.DriveType -ne [IO.DriveType]::Fixed) {
        Refuse "$Where requires a canonical fully qualified local drive path"
    }
    if ($Value.Length -gt 3) {
        foreach ($part in $Value.Substring(3).Split('\')) {
            if ([string]::IsNullOrEmpty($part) -or $part -eq '.' -or $part -eq '..' -or
                $part.EndsWith('.') -or $part.EndsWith(' ') -or
                $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') {
                Refuse "$Where contains an ambiguous Windows path component"
            }
        }
    }
    $cursor = $canonical
    while (-not [string]::IsNullOrEmpty($cursor)) {
        $item = Get-Item -LiteralPath $cursor -Force -ErrorAction SilentlyContinue
        if ($null -ne $item -and
            (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            Refuse "$Where has a reparse-point path or ancestor"
        }
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if ([string]::IsNullOrEmpty($parent) -or $parent -ceq $cursor) { break }
        $cursor = $parent
    }
    return $canonical
}

function Require-Amd64Pe([string]$Path, [string]$Role) {
    $stream = [IO.File]::OpenRead($Path)
    try {
        $reader = New-Object IO.BinaryReader($stream)
        if ($stream.Length -lt 512 -or $reader.ReadUInt16() -ne 0x5A4D) {
            Refuse "image $Role is not a complete Windows PE"
        }
        $stream.Position = 0x3C
        $offset = $reader.ReadInt32()
        if ($offset -lt 0x40 -or [long]$offset -gt $stream.Length - 6) {
            Refuse "image $Role has an invalid PE header offset"
        }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne 0x8664) {
            Refuse "image $Role is not a Windows AMD64 PE"
        }
    } finally {
        $stream.Dispose()
    }
}

# A closed inventory is an exact observation at this cut, not immutable
# native directory custody. Do not recurse through arbitrary child paths.
function Require-StageInventory([string]$Root, [object[]]$Images, [string]$Where) {
    $null = Require-CanonicalLocalPath $Root "$Where directory"
    $rootItem = Get-Item -LiteralPath $Root -Force -ErrorAction Stop
    if ($rootItem -isnot [IO.DirectoryInfo]) { Refuse "$Where inventory root is not a directory" }
    $directories = @('bin', 'recovery', 'bindings')
    $rootEntries = @(Get-ChildItem -LiteralPath $Root -Force -ErrorAction Stop)
    if ($rootEntries.Count -ne ($directories.Count + 1)) {
        Refuse "$Where inventory has missing or foreign root entries"
    }
    foreach ($item in $rootEntries) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            ($item.Name -ceq 'manifest.json' -and $item -isnot [IO.FileInfo]) -or
            ($directories -ccontains $item.Name -and $item -isnot [IO.DirectoryInfo]) -or
            ($item.Name -cne 'manifest.json' -and $directories -cnotcontains $item.Name)) {
            Refuse "$Where inventory has an unexpected root entry"
        }
    }
    foreach ($directory in $directories) {
        $path = Join-Path $Root $directory
        $null = Require-CanonicalLocalPath $path "$Where inventory $directory"
        $expected = @($Images | Where-Object {
            $_.destination.StartsWith(($directory + '/'), [StringComparison]::Ordinal)
        } | ForEach-Object { [IO.Path]::GetFileName($_.destination) })
        if ($directory -ceq 'bindings') { $expected += 'keld-mcp-stdio.json' }
        $children = @(Get-ChildItem -LiteralPath $path -Force -ErrorAction Stop)
        if ($children.Count -ne $expected.Count) {
            Refuse "$Where inventory has missing or foreign $directory entries"
        }
        foreach ($item in $children) {
            if ($item -isnot [IO.FileInfo] -or $expected -cnotcontains $item.Name -or
                (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
                Refuse "$Where inventory has an unexpected $directory entry"
            }
        }
    }
}

function Read-PinnedImage($Entry, [string]$Role, [string]$Target) {
    Require-Fields $Entry @('role', 'path', 'sha256', 'signerThumbprint') "image $Role"
    if ($Entry.role -cne $Role -or $Entry.path -isnot [string] -or
        [string]::IsNullOrWhiteSpace($Entry.path) -or
        $Entry.path -cnotmatch '^[A-Za-z]:\\') {
        Refuse "image $Role requires its exact role and absolute source path"
    }
    $sourcePath = Require-CanonicalLocalPath $Entry.path "image $Role"
    $digest = Require-Hash $Entry.sha256 "image $Role SHA256" 64
    $thumbprint = Require-Hash $Entry.signerThumbprint "image $Role signer" 40
    $original = Get-Item -LiteralPath $sourcePath -ErrorAction Stop
    if ($original -isnot [IO.FileInfo] -or $original.Extension -ine '.exe' -or
        (($original.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        Refuse "image $Role must be a regular non-reparse .exe"
    }
    $before = Get-TrustedFileSha256 $original.FullName
    if ($before -cne $digest) {
        Refuse "image $Role differs from independently pinned SHA256"
    }
    Require-Amd64Pe $original.FullName $Role
    $authenticode = Get-TrustedAuthenticodeSignature $original.FullName
    if ($authenticode.Status.ToString() -cne 'Valid' -or
        $authenticode.SignatureType.ToString() -cne 'Authenticode' -or
        $null -eq $authenticode.SignerCertificate -or
        $authenticode.SignerCertificate.Thumbprint.ToUpperInvariant() -cne $thumbprint) {
        Refuse "image $Role lacks its independently pinned valid embedded signature"
    }
    if ((Get-TrustedFileSha256 $original.FullName) -cne $digest) {
        Refuse "image $Role changed during read-only staging preflight"
    }
    return [pscustomobject]@{
        role = $Role
        source = $original.FullName
        destination = $Target
        sha256 = $digest
        expectedSignerThumbprint = $thumbprint
        length = [long]$original.Length
    }
}

if ($env:OS -cne 'Windows_NT') { Refuse 'requires native Windows' }
if ([string]::IsNullOrWhiteSpace($PinsFile) -or
    [string]::IsNullOrWhiteSpace($OutputDirectory)) {
    Refuse 'required PinsFile and OutputDirectory'
}
$PinsFile = Require-CanonicalLocalPath $PinsFile 'owner pins'
$OutputDirectory = Require-CanonicalLocalPath $OutputDirectory 'output directory'
if (Test-Path -LiteralPath $OutputDirectory) {
    Refuse 'destination already exists; no overwrite, repair or merge'
}
$parent = Split-Path -Parent $OutputDirectory
if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
    Refuse 'destination parent does not exist'
}
$null = Require-CanonicalLocalPath $parent 'destination parent'
$pinFileItem = Get-Item -LiteralPath $PinsFile -ErrorAction Stop
if ($pinFileItem -isnot [IO.FileInfo] -or
    (($pinFileItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
    Refuse 'owner pins must be a regular non-reparse file'
}
$rawPins = [IO.File]::ReadAllText($pinFileItem.FullName)
try { $pins = ConvertFrom-Json -InputObject $rawPins -ErrorAction Stop }
catch { Refuse 'owner pin JSON cannot be parsed' }
Require-Fields $pins @('schema', 'sourceCommit', 'images') 'owner pins'
if ($pins.schema -cne 'keld.windows-v2-stage-pins/v1' -or
    $pins.sourceCommit -isnot [string] -or
    $pins.sourceCommit -cnotmatch '^[0-9a-f]{40}$') {
    Refuse 'pin schema or exact source commit is invalid'
}

# Five exact binaries correspond to existing Rust executable names/owners.
# Recovery is a separately signed original Host image, not a new executable role.
# The staged names are internal artifact slots, not published installer names.
$layout = @(
    @('app-host', 'bin/keld-host.exe'),
    @('recovery-host', 'recovery/keld-host.exe'),
    @('cli', 'bin/keld.exe'),
    @('role-launcher', 'bin/keld-role-launcher.exe'),
    @('updater-helper', 'bin/keld-updater-helper.exe')
)
if ($null -eq $pins.images -or $pins.images -isnot [array] -or
    $pins.images.Count -ne $layout.Count) {
    Refuse 'the five fixed image roles must all have independent pins'
}
$images = @()
for ($i = 0; $i -lt $layout.Count; $i++) {
    $images += Read-PinnedImage $pins.images[$i] $layout[$i][0] $layout[$i][1]
}
if (@($images | Select-Object -ExpandProperty sha256 -Unique).Count -ne $images.Count) {
    Refuse 'each original executable role requires distinct independently pinned bytes'
}

# All observations precede staging writes. Nothing created here can authorize
# an installed root, process launch, catalogue or recovered application.
# A unique sibling is never merged into a pre-existing destination. The
# final same-volume directory rename publishes only completely checked files.
$temporary = Join-Path $parent ('.keld-v2-stage-' + [guid]::NewGuid().ToString('N'))
$temporaryCreated = $false
try {
$null = Require-CanonicalLocalPath $parent 'destination parent'
if (Test-Path -LiteralPath $temporary) {
    Refuse 'private temporary sibling already exists'
}
[IO.Directory]::CreateDirectory($temporary) | Out-Null
$temporaryCreated = $true
$null = Require-CanonicalLocalPath $temporary 'temporary stage'
if (@(Get-ChildItem -LiteralPath $temporary -Force).Count -ne 0) {
    Refuse 'private temporary sibling must be empty before staging'
}
foreach ($relative in @('bin', 'recovery', 'bindings')) {
    [IO.Directory]::CreateDirectory((Join-Path $temporary $relative)) | Out-Null
}
foreach ($entry in $images) {
    $target = Join-Path $temporary $entry.destination
    [IO.File]::Copy($entry.source, $target, $false)
    if ((Get-TrustedFileSha256 $target) -cne $entry.sha256 -or
        (Get-Item -LiteralPath $target).Length -ne $entry.length) {
        Refuse "staged $($entry.role) differs from the independently pinned source"
    }
}

$utf8 = New-Object System.Text.UTF8Encoding($false)
$provider = [ordered]@{
    schema = 'keld.windows-cli-provider-binding/v1'
    state = 'recipe-only-install-root-not-bound'
    executableRelativeToReleaseRoot = 'bin/keld.exe'
    arguments = @('mcp', 'serve')
}
$recipePath = Join-Path $temporary 'bindings/keld-mcp-stdio.json'
$recipeBytes = $utf8.GetBytes(($provider | ConvertTo-Json -Depth 4) + "`n")
$recipeDigest = Get-TrustedBytesSha256 $recipeBytes
Write-NewStageBytes $recipePath $recipeBytes
$manifestImages = @($images | ForEach-Object {
    [ordered]@{
        role = $_.role
        relativePath = $_.destination
        sha256 = $_.sha256
        length = $_.length
        expectedSignerThumbprint = $_.expectedSignerThumbprint
    }
})
$manifest = [ordered]@{
    schema = 'keld.windows-v2-signed-image-stage/v1'
    state = 'non-authorizing-path-observations-only'
    sourceCommit = $pins.sourceCommit
    artifacts = $manifestImages
    providerRecipePath = 'bindings/keld-mcp-stdio.json'
    providerRecipeSha256 = $recipeDigest
    nativeKeldProfileAndInstallationAuthority = 'NOT_VERIFIED'
}
$manifestPath = Join-Path $temporary 'manifest.json'
$manifestBytes = $utf8.GetBytes(($manifest | ConvertTo-Json -Depth 8) + "`n")
$manifestSha256 = Get-TrustedBytesSha256 $manifestBytes
Write-NewStageBytes $manifestPath $manifestBytes
foreach ($entry in $images) {
    if ((Get-TrustedFileSha256 (Join-Path $temporary $entry.destination)) -cne $entry.sha256) {
        Refuse "staged $($entry.role) changed before manifest closeout"
    }
}
if ((Get-TrustedFileSha256 $recipePath) -cne $recipeDigest) {
    Refuse 'CLI/provider recipe changed before manifest closeout'
}
if ((Get-TrustedFileSha256 $manifestPath) -cne $manifestSha256) {
    Refuse 'staged manifest changed before publication'
}
$null = Require-CanonicalLocalPath $parent 'destination parent'
$null = Require-CanonicalLocalPath $temporary 'temporary stage'
if (Test-Path -LiteralPath $OutputDirectory) {
    Refuse 'destination appeared during stage preparation'
}
Require-StageInventory $temporary $images 'temporary stage'
[IO.Directory]::Move($temporary, $OutputDirectory)
$temporaryCreated = $false

# A successful rename is not proof that the published bytes are still the
# checked bytes. Reopen every published slot through its final path before
# reporting nonauthorizing candidate custody. A failed readback deliberately
# leaves the final directory for inspection; it must not be reported as ready,
# silently overwritten, or recursively deleted from a caller-selected parent.
$null = Require-CanonicalLocalPath $OutputDirectory 'published stage directory'
$publishedManifest = Join-Path $OutputDirectory 'manifest.json'
$null = Require-CanonicalLocalPath $publishedManifest 'published manifest'
$publishedManifestItem = Get-Item -LiteralPath $publishedManifest -ErrorAction Stop
if ($publishedManifestItem -isnot [IO.FileInfo] -or
    (Get-TrustedFileSha256 $publishedManifest) -cne $manifestSha256) {
    Refuse 'published manifest changed after atomic rename'
}
$publishedRecipe = Join-Path $OutputDirectory 'bindings/keld-mcp-stdio.json'
$null = Require-CanonicalLocalPath $publishedRecipe 'published provider recipe'
if ((Get-TrustedFileSha256 $publishedRecipe) -cne $recipeDigest) {
    Refuse 'published provider recipe changed after atomic rename'
}
foreach ($entry in $images) {
    $publishedImage = Join-Path $OutputDirectory $entry.destination
    $null = Require-CanonicalLocalPath $publishedImage "published image $($entry.role)"
    $publishedItem = Get-Item -LiteralPath $publishedImage -ErrorAction Stop
    if ($publishedItem -isnot [IO.FileInfo] -or
        $publishedItem.Length -ne $entry.length -or
        (Get-TrustedFileSha256 $publishedImage) -cne $entry.sha256) {
        Refuse "published image $($entry.role) changed after atomic rename"
    }
}
Require-StageInventory $OutputDirectory $images 'published stage'
} catch {
    $failure = $_
    if ($temporaryCreated) {
        # A random name and path-based reparse checks do not prove ownership
        # at deletion time. Without retained parent/child identity handles,
        # even a previously regular directory can be replaced by a junction.
        # Never traverse or delete the failed stage. Its path is a diagnostic,
        # not a safe cleanup authorization; retain it for inspection.
        throw "KEL270-STAGE: partial stage retained for inspection at '$temporary'; automatic cleanup refused without retained filesystem identity; original failure: $failure; no release/install authority granted"
    }
    throw $failure
}
Write-Output "KEL270_STAGE_IMAGES_PREPARED_NONAUTHORIZING source=$($pins.sourceCommit) manifest_sha256=$manifestSha256 roles=5 native_d45=NOT_RUN installer=NOT_BUILT install=NOT_RUN provider_registration=NOT_RUN"
