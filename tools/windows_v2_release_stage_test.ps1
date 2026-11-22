# Focused native refusals for the offline KEL-270 release-stage tool.
# No signer, installer, certificate, trust anchor, or positive release is created.
[CmdletBinding()]
param([string]$StageToolOverride)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$stageTool = Join-Path $PSScriptRoot 'windows_v2_release_stage.ps1'
if (-not [string]::IsNullOrWhiteSpace($StageToolOverride)) {
    $stageTool = $StageToolOverride
}
$powershell = Join-Path $PSHOME 'powershell.exe'
$fixture = Join-Path $env:TEMP ('keld270-stage-negative-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $fixture

function Require-Refusal([string]$Case, $Pins, [string]$Fragment) {
    $pinFile = Join-Path $fixture 'pins.json'
    $outputDirectory = Join-Path $fixture 'output'
    [IO.File]::WriteAllText($pinFile, ($Pins | ConvertTo-Json -Depth 6))
    $ErrorActionPreference = 'Continue'
    try {
        $output = (& $powershell '-NoProfile' '-NonInteractive' '-File' $stageTool `
            '-PinsFile' $pinFile '-OutputDirectory' $outputDirectory 2>&1 | Out-String)
        $exit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
    }
    if ($exit -eq 0 -or $output -notmatch [regex]::Escape($Fragment) -or
        $output -match 'KEL270_STAGE_IMAGES_PREPARED' -or
        (Test-Path -LiteralPath $outputDirectory)) {
        throw "KEL270_STAGE_NEGATIVE_FAILURE case=$Case exit=$exit output=$output"
    }
    Write-Output "KEL270_STAGE_NEGATIVE_PASS case=$Case"
}

function Require-PathRefusal([string]$Case, [string]$PinPath,
                             [string]$OutputPath, [string]$Fragment) {
    $ErrorActionPreference = 'Continue'
    try {
        $output = (& $powershell '-NoProfile' '-NonInteractive' '-File' $stageTool '-PinsFile' $PinPath '-OutputDirectory' $OutputPath 2>&1 | Out-String)
        $exit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
    }
    if ($exit -eq 0 -or $output -notmatch [regex]::Escape($Fragment) -or
        (Test-Path -LiteralPath $OutputPath)) {
        throw "KEL270_STAGE_NEGATIVE_FAILURE case=$Case exit=$exit output=$output"
    }
    Write-Output "KEL270_STAGE_NEGATIVE_PASS case=$Case"
}

try {
    $fake = Join-Path $fixture 'unsigned.exe'
    $bytes = New-Object byte[] 512
    $bytes[0] = 0x4d
    $bytes[1] = 0x5a
    [IO.File]::WriteAllBytes($fake, $bytes)
    $hash = (Get-FileHash -LiteralPath $fake -Algorithm SHA256).Hash
    $roles = @('app-host', 'recovery-host', 'cli', 'role-launcher', 'updater-helper')
    $images = @($roles | ForEach-Object {
        [ordered]@{
            role = $_
            path = $fake
            sha256 = $hash
            signerThumbprint = '0' * 40
        }
    })
    $pins = [ordered]@{
        schema = 'keld.windows-v2-stage-pins/v1'
        sourceCommit = 'a1eb38172712963483920635ad830c304987f9e2'
        images = $images
    }
    Require-Refusal 'invalid-source-PE' $pins 'invalid PE header offset'

    $mismatch = ConvertFrom-Json -InputObject ($pins | ConvertTo-Json -Depth 6)
    $mismatch.images[0].sha256 = '0' * 64
    Require-Refusal 'wrong-independent-SHA256' $mismatch 'differs from independently pinned SHA256'

    $missing = ConvertFrom-Json -InputObject ($pins | ConvertTo-Json -Depth 6)
    $missing.images = @($images | Select-Object -First 4)
    Require-Refusal 'missing-original-role' $missing 'five fixed image roles'

    $unknown = ConvertFrom-Json -InputObject ($pins | ConvertTo-Json -Depth 6)
    $unknown.images[0].role = 'app-host-foreign'
    Require-Refusal 'foreign-image-role' $unknown 'exact role and absolute source path'

    # Five DISTINCT, structurally correct x64 PE headers, with no signature.
    # A removed Authenticode gate must turn this specific oracle RED.
    $unsignedImages = @()
    for ($index = 0; $index -lt $roles.Count; $index++) {
        $structural = New-Object byte[] 512
        $structural[0] = 0x4d; $structural[1] = 0x5a
        $structural[0x3c] = 0x80
        $structural[0x80] = 0x50; $structural[0x81] = 0x45
        $structural[0x84] = 0x64; $structural[0x85] = 0x86
        $structural[0x90] = [byte]($index + 1)
        $unsigned = Join-Path $fixture ("unsigned-valid-header-$index.exe")
        [IO.File]::WriteAllBytes($unsigned, $structural)
        $unsignedImages += [ordered]@{
            role = $roles[$index]
            path = $unsigned
            sha256 = (Get-FileHash -LiteralPath $unsigned -Algorithm SHA256).Hash
            signerThumbprint = '0' * 40
        }
    }
    $unsignedPins = [ordered]@{
        schema = 'keld.windows-v2-stage-pins/v1'
        sourceCommit = 'a1eb38172712963483920635ad830c304987f9e2'
        images = $unsignedImages
    }
    Require-Refusal 'unsigned-valid-PE-requires-signature' $unsignedPins 'lacks its independently pinned valid embedded signature'

    $validPinsFile = Join-Path $fixture 'pins.json'
    Require-PathRefusal 'drive-relative-output' $validPinsFile 'C:relative-stage' 'requires a fully qualified local drive path'
    Require-PathRefusal 'root-relative-output' $validPinsFile '\relative-stage' 'requires a fully qualified local drive path'
    $junction = Join-Path $fixture 'via-junction'
    $null = New-Item -ItemType Junction -Path $junction -Target $fixture -ErrorAction Stop
    Require-PathRefusal 'reparse-ancestor-pins' (Join-Path $junction 'pins.json') (Join-Path $fixture 'no-stage') 'reparse-point path or ancestor'

    # Exercise cleanup at a post-copy cut. This uses a private TEST COPY of
    # the script with signer validation removed ONLY to reach the cut with
    # unsigned structural inputs; it cannot qualify any Keld release.
    $stageSource = [IO.File]::ReadAllText($stageTool)
    $authStart = $stageSource.IndexOf('    $authenticode = Get-AuthenticodeSignature -LiteralPath $original.FullName')
    $authEnd = $stageSource.IndexOf('    if ((Get-FileHash -LiteralPath $original.FullName -Algorithm SHA256).Hash -cne $digest)', $authStart)
    if ($authStart -lt 0 -or $authEnd -le $authStart) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE signer mutation anchors missing'
    }
    $testOnly = $stageSource.Remove($authStart, $authEnd - $authStart)
    $copyAnchor = '    [IO.File]::Copy($entry.source, $target, $false)'
    if ($testOnly.IndexOf($copyAnchor) -lt 0 -or
        $testOnly.IndexOf($copyAnchor) -ne $testOnly.LastIndexOf($copyAnchor)) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE postcopy injection anchor missing/ambiguous'
    }
    $testOnly = $testOnly.Replace($copyAnchor, $copyAnchor + [Environment]::NewLine + "    throw 'INJECTED_POST_COPY_FAULT'")
    $faultScript = Join-Path $fixture 'fault-stage-copy.ps1'
    [IO.File]::WriteAllText($faultScript, $testOnly, (New-Object System.Text.UTF8Encoding($false)))
    [IO.File]::WriteAllText($validPinsFile, ($unsignedPins | ConvertTo-Json -Depth 6))
    $faultOutput = Join-Path $fixture 'fault-output'
    $ErrorActionPreference = 'Continue'
    try {
        $faultResult = (& $powershell '-NoProfile' '-NonInteractive' '-File' $faultScript '-PinsFile' $validPinsFile '-OutputDirectory' $faultOutput 2>&1 | Out-String)
        $faultExit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
    }
    $leftovers = @(Get-ChildItem -LiteralPath $fixture -Directory -Force |
        Where-Object { $_.Name -match '^\.keld-v2-stage-[0-9a-f]{32}$' })
    if ($faultExit -eq 0 -or $faultResult -notmatch 'INJECTED_POST_COPY_FAULT' -or
        (Test-Path -LiteralPath $faultOutput) -or $leftovers.Count -ne 0) {
        throw "KEL270_STAGE_NEGATIVE_FAILURE case=postcopy-cleanup exit=$faultExit temps=$($leftovers.Count) output=$faultResult"
    }
    Write-Output 'KEL270_STAGE_NEGATIVE_PASS case=postcopy-cleanup'

    # An atomic rename does not replace exact published-path readback.
    # In a disposable test-only script, bypass signer verification solely
    # to exercise a synthetic cut, then corrupt the moved manifest.
    $publishTestOnly = $stageSource.Remove($authStart, $authEnd - $authStart)
    $moveAnchor = '[IO.Directory]::Move($temporary, $OutputDirectory)'
    if ($publishTestOnly.IndexOf($moveAnchor) -lt 0 -or
        $publishTestOnly.IndexOf($moveAnchor) -ne $publishTestOnly.LastIndexOf($moveAnchor)) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE publish mutation anchor missing/ambiguous'
    }
    $postMoveCorrupt = '[IO.File]::WriteAllText((Join-Path $OutputDirectory ''manifest.json''), ''INJECTED_FOREIGN_MANIFEST_BYTES'')'
    $publishTestOnly = $publishTestOnly.Replace(
        $moveAnchor, $moveAnchor + [Environment]::NewLine + $postMoveCorrupt)
    $publishFaultScript = Join-Path $fixture 'fault-stage-after-move.ps1'
    [IO.File]::WriteAllText($publishFaultScript, $publishTestOnly,
        (New-Object System.Text.UTF8Encoding($false)))
    $publishFaultOutput = Join-Path $fixture 'postmove-output'
    $ErrorActionPreference = 'Continue'
    try {
        $publishFaultResult = (& $powershell '-NoProfile' '-NonInteractive' '-File' $publishFaultScript '-PinsFile' $validPinsFile '-OutputDirectory' $publishFaultOutput 2>&1 | Out-String)
        $publishFaultExit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
    }
    $publishedFaultManifest = Join-Path $publishFaultOutput 'manifest.json'
    if ($publishFaultExit -eq 0 -or
        $publishFaultResult -notmatch 'published manifest changed after atomic rename' -or
        $publishFaultResult -match 'KEL270_STAGE_IMAGES_PREPARED' -or
        -not (Test-Path -LiteralPath $publishedFaultManifest) -or
        [IO.File]::ReadAllText($publishedFaultManifest) -cne 'INJECTED_FOREIGN_MANIFEST_BYTES') {
        throw "KEL270_STAGE_NEGATIVE_FAILURE case=postmove-readback exit=$publishFaultExit output=$publishFaultResult"
    }
    Write-Output 'KEL270_STAGE_NEGATIVE_PASS case=postmove-readback'

    # The same post-rename boundary must independently protect provider
    # binding and binary bytes, not just the manifest describing them.
    foreach ($publicationCase in @(
        @{ name = 'postmove-provider-readback'; relative = 'bindings/keld-mcp-stdio.json'; expected = 'published provider recipe changed after atomic rename' },
        @{ name = 'postmove-image-readback'; relative = 'bin/keld-host.exe'; expected = 'published image app-host changed after atomic rename' }
    )) {
        $changedBytes = 'INJECTED_FOREIGN_PUBLISHED_BYTES'
        $slotCorrupt = '[IO.File]::WriteAllText((Join-Path $OutputDirectory ''{0}''), ''{1}'')' -f $publicationCase.relative, $changedBytes
        $slotTestOnly = $stageSource.Remove($authStart, $authEnd - $authStart)
        $slotTestOnly = $slotTestOnly.Replace(
            $moveAnchor, $moveAnchor + [Environment]::NewLine + $slotCorrupt)
        $slotScript = Join-Path $fixture ($publicationCase.name + '.ps1')
        [IO.File]::WriteAllText($slotScript, $slotTestOnly,
            (New-Object System.Text.UTF8Encoding($false)))
        $slotOutput = Join-Path $fixture $publicationCase.name
        $ErrorActionPreference = 'Continue'
        try {
            $slotResult = (& $powershell '-NoProfile' '-NonInteractive' '-File' $slotScript '-PinsFile' $validPinsFile '-OutputDirectory' $slotOutput 2>&1 | Out-String)
            $slotExit = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = 'Stop'
        }
        $corruptedSlot = Join-Path $slotOutput $publicationCase.relative
        if ($slotExit -eq 0 -or
            $slotResult -notmatch [regex]::Escape($publicationCase.expected) -or
            $slotResult -match 'KEL270_STAGE_IMAGES_PREPARED' -or
            -not (Test-Path -LiteralPath $corruptedSlot) -or
            [IO.File]::ReadAllText($corruptedSlot) -cne $changedBytes) {
            throw "KEL270_STAGE_NEGATIVE_FAILURE case=$($publicationCase.name) exit=$slotExit output=$slotResult"
        }
        Write-Output "KEL270_STAGE_NEGATIVE_PASS case=$($publicationCase.name)"
    }

    # Same published bytes through a post-move junction are NOT acceptable.
    # This independent alias oracle guards the canonical final-path check:
    # a digest-only readback would otherwise accept the foreign parent.
    $aliasTestOnly = $stageSource.Remove($authStart, $authEnd - $authStart)
    $aliasInjection = @(
        '$slot = Join-Path $OutputDirectory ''bindings''',
        '$aliasTarget = Join-Path $parent ''test-only-binding-alias-target''',
        '[IO.Directory]::CreateDirectory($aliasTarget) | Out-Null',
        '[IO.File]::Copy((Join-Path $slot ''keld-mcp-stdio.json''),(Join-Path $aliasTarget ''keld-mcp-stdio.json''),$false)',
        '[IO.File]::Delete((Join-Path $slot ''keld-mcp-stdio.json''))',
        '[IO.Directory]::Delete($slot,$false)',
        '$null = New-Item -ItemType Junction -Path $slot -Target $aliasTarget -ErrorAction Stop'
    ) -join [Environment]::NewLine
    $aliasTestOnly = $aliasTestOnly.Replace(
        $moveAnchor, $moveAnchor + [Environment]::NewLine + $aliasInjection)
    $aliasFaultScript = Join-Path $fixture 'postmove-junction-stage.ps1'
    [IO.File]::WriteAllText($aliasFaultScript, $aliasTestOnly,
        (New-Object System.Text.UTF8Encoding($false)))
    $aliasOutput = Join-Path $fixture 'postmove-junction-output'
    $aliasLink = Join-Path $aliasOutput 'bindings'
    try {
        $ErrorActionPreference = 'Continue'
        try {
            $aliasResult = (& $powershell '-NoProfile' '-NonInteractive' '-File' $aliasFaultScript '-PinsFile' $validPinsFile '-OutputDirectory' $aliasOutput 2>&1 | Out-String)
            $aliasExit = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = 'Stop'
        }
        $aliasTargetFile = Join-Path $fixture 'test-only-binding-alias-target/keld-mcp-stdio.json'
        $aliasManifestFile = Join-Path $aliasOutput 'manifest.json'
        if (-not (Test-Path -LiteralPath $aliasManifestFile -PathType Leaf) -or
            -not (Test-Path -LiteralPath $aliasTargetFile -PathType Leaf)) {
            throw "KEL270_STAGE_NEGATIVE_FAILURE case=postmove-samebytes-reparse missing test fixtures exit=$aliasExit output=$aliasResult"
        }
        $aliasManifest = Get-Content -LiteralPath $aliasManifestFile -Raw | ConvertFrom-Json
        $aliasDigestMatches = ((Get-FileHash -LiteralPath $aliasTargetFile -Algorithm SHA256).Hash -ceq
            $aliasManifest.providerRecipeSha256)
        $aliasLinkItem = Get-Item -LiteralPath $aliasLink -Force -ErrorAction Stop
        if ($aliasExit -eq 0 -or
            $aliasResult -notmatch 'published provider recipe has a reparse-point path or ancestor' -or
            $aliasResult -match 'KEL270_STAGE_IMAGES_PREPARED' -or
            -not $aliasDigestMatches -or
            (($aliasLinkItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0)) {
            throw "KEL270_STAGE_NEGATIVE_FAILURE case=postmove-samebytes-reparse exit=$aliasExit sameDigest=$aliasDigestMatches output=$aliasResult"
        }
        Write-Output 'KEL270_STAGE_NEGATIVE_PASS case=postmove-samebytes-reparse'
    } finally {
        # Delete only the self-created junction, never its target. The outer
        # owned fixture cleanup removes remaining regular files afterwards.
        $item = Get-Item -LiteralPath $aliasLink -Force -ErrorAction SilentlyContinue
        if ($null -ne $item -and
            (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            [IO.Directory]::Delete($aliasLink, $false)
        }
    }
    if ((Get-FileHash -LiteralPath $fake -Algorithm SHA256).Hash -cne $hash) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE: source changed during refusals'
    }
    Write-Output 'KEL270_STAGE_NEGATIVE_SUITE_PASS count=13 source_unchanged=true positive_signed_release=NOT_RUN'
} finally {
    # Only this invocation's newly allocated disposable negative fixture.
    Remove-Item -LiteralPath $fixture -Recurse -Force
}
