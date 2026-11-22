# KEL-270 Windows v2 PerUserDirect NSIS builder (offline, unsigned candidate).
# Takes the W38 five-image stage; never embeds identity into a signed executable.
# The fixed NSIS source invokes the original signed Host's private install role.
# No invocation can compile until the native T81/T20/D90/D46 owner publishes and
# independently reviews a real verifier and the NSIS compiler binary pins below.
# This script never signs, installs, elevates or publishes a release.
#
# Once native ownership is commissioned, the native verifier must independently
# prove the exact signed App/Recovery roles and catalogue and the presence of
# reviewed T20/D90/D46 native admission implementations in those original
# binaries. A live D90 census and D46 one-use permit MUST be produced later by
# the admitted original Host for the actual target user, not at build time.
[CmdletBinding()]
param(
    [string]$StageDirectory,
    [string]$ExpectedStageManifestSha256,
    [string]$ExpectedSourceCommit,
    [string]$CommissionedCatalogue,
    [string]$ExpectedCatalogueSha256,
    [string]$NativeCommissioningGateExe,
    [string]$MakensisExe,
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# The original KEL-270 Host always returns KELD-HOST-INITIAL-002 until its
# catalogue and native issuer are commissioned. No source-approved gate or
# compiler digests exist on PR1593 a609eb0b. These are deliberately EMPTY.
# Only the actual owners may approve exact original binary hashes on a later
# independently reviewed source change; caller-provided booleans/digests cannot
# fill or override these pins.
$reviewedNativeGateSha256 = ''
$reviewedNativeGateSignerThumbprint = ''
$reviewedMakensisSha256 = ''

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

function Require-ExactHash([string]$Path, [string]$Expected, [string]$What) {
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -cne $Expected) {
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

function Require-SignedImage([string]$Path, [string]$Digest, [string]$Signer, [string]$What) {
    Require-ExactHash $Path $Digest $What
    $item = Get-Item -LiteralPath $Path
    if ($item.Extension -cne '.exe') {
        Refuse "$What must be a native .exe"
    }
    Require-Amd64Pe $Path $What
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status.ToString() -cne 'Valid' -or
        $signature.SignatureType.ToString() -cne 'Authenticode' -or
        $null -eq $signature.SignerCertificate -or
        $signature.SignerCertificate.Thumbprint.ToUpperInvariant() -cne $Signer) {
        Refuse "$What differs from its reviewed embedded Authenticode signer"
    }
    Require-ExactHash $Path $Digest $What
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

# This is the mandatory native commissioning boundary. A signed stage manifest,
# user-supplied catalogue/hash, or environment toggle cannot construct it.
# The actual owner must provide an independently reviewed versioned verifier
# with real T81 signer/profile, T20 catalogue, D90 census and D46 issuer proof.
# Until that owner pins its genuine verifier in SOURCE, refuse before writing.
if ($reviewedNativeGateSha256 -cnotmatch '^[0-9A-F]{64}$' -or
    $reviewedNativeGateSignerThumbprint -cnotmatch '^[0-9A-F]{40}$') {
    Refuse 'native T81/T20/D90/D46 commissioning gate is unavailable on this source head'
}
$gate = Require-File $NativeCommissioningGateExe 'reviewed native commissioning gate'
Require-SignedImage $gate $reviewedNativeGateSha256 $reviewedNativeGateSignerThumbprint 'native commissioning gate'
# The owner of this gate must review and implement this PRIVATE verify-only call
# before its pins can be approved. The result binds immutable manifest/catalogue
# hashes and the shipped original Host's native enforcement; it does not mint
# an installation-specific D46 permit or attest a future user's D90 census.
$gateOutput = @(& $gate '--verify-windows-v2-nsis-inputs-v1' $manifestPath $catalogue)
$gateExit = $LASTEXITCODE
$gateSuccess = "KEL270_NSIS_INPUTS_ADMITTED stage_sha256=$manifestHash catalogue_sha256=$catalogueHash"
if ($gateExit -ne 0 -or $gateOutput.Count -ne 1 -or $gateOutput[0] -cne $gateSuccess) {
    Refuse 'native commissioning gate did not issue the exact build-admission receipt'
}
if ($reviewedMakensisSha256 -cnotmatch '^[0-9A-F]{64}$') {
    Refuse 'reviewed NSIS compiler binary is not pinned on this source head'
}
$compiler = Require-File $MakensisExe 'reviewed makensis.exe'
Require-ExactHash $compiler $reviewedMakensisSha256 'reviewed makensis.exe'
$source = Require-File (Join-Path $PSScriptRoot 'windows_v2_nsis_installer.nsi') 'fixed NSIS builder source'
$installer = Join-Path $destination 'keld-v2-peruserdirect-UNSIGNED.exe'

# Only now, after the same native owner's signed admission, may the offline
# packager produce an unsigned candidate. File paths are constrained above;
# executable and catalogue contents are copied unchanged by NSIS File.
$null = New-Item -ItemType Directory -Path $destination -ErrorAction Stop
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
$candidateHash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
Write-Output "KEL270_NSIS_UNSIGNED_CANDIDATE_COMPILED sha256=$candidateHash signing=NOT_RUN installed=NOT_RUN public_trust_release=NOT_VERIFIED"
