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

function Read-PinnedImage($Entry, [string]$Role, [string]$Target) {
    Require-Fields $Entry @('role', 'path', 'sha256', 'signerThumbprint') "image $Role"
    if ($Entry.role -cne $Role -or $Entry.path -isnot [string] -or
        [string]::IsNullOrWhiteSpace($Entry.path) -or
        -not [IO.Path]::IsPathRooted($Entry.path)) {
        Refuse "image $Role requires its exact role and absolute source path"
    }
    $digest = Require-Hash $Entry.sha256 "image $Role SHA256" 64
    $thumbprint = Require-Hash $Entry.signerThumbprint "image $Role signer" 40
    $original = Get-Item -LiteralPath $Entry.path -ErrorAction Stop
    if ($original -isnot [IO.FileInfo] -or $original.Extension -ine '.exe' -or
        (($original.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        Refuse "image $Role must be a regular non-reparse .exe"
    }
    $before = (Get-FileHash -LiteralPath $original.FullName -Algorithm SHA256).Hash
    if ($before -cne $digest) {
        Refuse "image $Role differs from independently pinned SHA256"
    }
    Require-Amd64Pe $original.FullName $Role
    $authenticode = Get-AuthenticodeSignature -LiteralPath $original.FullName
    if ($authenticode.Status.ToString() -cne 'Valid' -or
        $authenticode.SignatureType.ToString() -cne 'Authenticode' -or
        $null -eq $authenticode.SignerCertificate -or
        $authenticode.SignerCertificate.Thumbprint.ToUpperInvariant() -cne $thumbprint) {
        Refuse "image $Role lacks its independently pinned valid embedded signature"
    }
    if ((Get-FileHash -LiteralPath $original.FullName -Algorithm SHA256).Hash -cne $digest) {
        Refuse "image $Role changed during read-only staging preflight"
    }
    return [pscustomobject]@{
        role = $Role
        source = $original.FullName
        destination = $Target
        sha256 = $digest
        signerThumbprint = $thumbprint
        length = [long]$original.Length
    }
}

if ($env:OS -cne 'Windows_NT') { Refuse 'requires native Windows' }
if ([string]::IsNullOrWhiteSpace($PinsFile) -or
    [string]::IsNullOrWhiteSpace($OutputDirectory)) {
    Refuse 'required PinsFile and OutputDirectory'
}
if (-not [IO.Path]::IsPathRooted($PinsFile) -or
    -not [IO.Path]::IsPathRooted($OutputDirectory)) {
    Refuse 'source and destination must be absolute'
}
if (Test-Path -LiteralPath $OutputDirectory) {
    Refuse 'destination already exists; no overwrite, repair or merge'
}
$parent = Split-Path -Parent $OutputDirectory
if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
    Refuse 'destination parent does not exist'
}
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
$null = New-Item -ItemType Directory -Path $OutputDirectory -ErrorAction Stop
foreach ($relative in @('bin', 'recovery', 'bindings')) {
    $null = New-Item -ItemType Directory -Path (Join-Path $OutputDirectory $relative) -ErrorAction Stop
}
foreach ($entry in $images) {
    $target = Join-Path $OutputDirectory $entry.destination
    [IO.File]::Copy($entry.source, $target, $false)
    if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -cne $entry.sha256 -or
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
$recipePath = Join-Path $OutputDirectory 'bindings/keld-mcp-stdio.json'
[IO.File]::WriteAllText($recipePath, ($provider | ConvertTo-Json -Depth 4) + "`n", $utf8)
$recipeDigest = (Get-FileHash -LiteralPath $recipePath -Algorithm SHA256).Hash
$manifestImages = @($images | ForEach-Object {
    [ordered]@{
        role = $_.role
        relativePath = $_.destination
        sha256 = $_.sha256
        length = $_.length
        observedSignerThumbprint = $_.signerThumbprint
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
$manifestPath = Join-Path $OutputDirectory 'manifest.json'
[IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 8) + "`n", $utf8)
foreach ($entry in $images) {
    if ((Get-FileHash -LiteralPath (Join-Path $OutputDirectory $entry.destination) -Algorithm SHA256).Hash -cne $entry.sha256) {
        Refuse "staged $($entry.role) changed before manifest closeout"
    }
}
if ((Get-FileHash -LiteralPath $recipePath -Algorithm SHA256).Hash -cne $recipeDigest) {
    Refuse 'CLI/provider recipe changed before manifest closeout'
}
$manifestSha256 = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash
Write-Output "KEL270_STAGE_IMAGES_PREPARED_NONAUTHORIZING source=$($pins.sourceCommit) manifest_sha256=$manifestSha256 roles=5 native_d45=NOT_RUN installer=NOT_BUILT install=NOT_RUN provider_registration=NOT_RUN"
