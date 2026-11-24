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
$fixtureCleanupReady = $false

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

    # Each fresh child has a hostile command lookup surface. A proper signer
    # path must still inspect the real unsigned PE and reject its signature.
    $shadowDigestDir = Join-Path $fixture 'shadow-digest'
    [IO.Directory]::CreateDirectory($shadowDigestDir) | Out-Null
    [IO.File]::WriteAllText((Join-Path $shadowDigestDir 'Get-FileHash.ps1'),
        "throw 'KEL270_STAGE_SHADOW_DIGEST_CALLED'")
    $savedPath = $env:PATH
    try {
        $env:PATH = $shadowDigestDir + ';' + $savedPath
        Require-Refusal 'path-shadowed-digest' $unsignedPins 'lacks its independently pinned valid embedded signature'
    } finally { $env:PATH = $savedPath }

    $shadowSignerDir = Join-Path $fixture 'shadow-signer'
    [IO.Directory]::CreateDirectory($shadowSignerDir) | Out-Null
    [IO.File]::WriteAllText((Join-Path $shadowSignerDir 'Get-AuthenticodeSignature.ps1'),
        "throw 'KEL270_STAGE_SHADOW_SIGNER_CALLED'")
    try {
        $env:PATH = $shadowSignerDir + ';' + $savedPath
        Require-Refusal 'path-shadowed-signer' $unsignedPins 'lacks its independently pinned valid embedded signature'
    } finally { $env:PATH = $savedPath }

    $shadowModuleRoot = Join-Path $fixture 'shadow-modules'
    $shadowModule = Join-Path $shadowModuleRoot 'Microsoft.PowerShell.Security'
    [IO.Directory]::CreateDirectory($shadowModule) | Out-Null
    [IO.File]::WriteAllText((Join-Path $shadowModule 'Microsoft.PowerShell.Security.psm1'),
        "function Get-AuthenticodeSignature { throw 'KEL270_STAGE_SHADOW_MODULE_CALLED' }; Export-ModuleMember -Function Get-AuthenticodeSignature")
    $savedModulePath = $env:PSModulePath
    try {
        $env:PSModulePath = $shadowModuleRoot + ';' + $savedModulePath
        Require-Refusal 'module-shadowed-signer' $unsignedPins 'lacks its independently pinned valid embedded signature'
    } finally { $env:PSModulePath = $savedModulePath }

    # Runtime fault injection proves the trusted signer helper is called on
    # the actual preflight path. This private source copy can never qualify a
    # signed release or reach any successful stage output.
    $signerSource = [IO.File]::ReadAllText($stageTool)
    $signerAnchor = '    $authenticode = Get-TrustedAuthenticodeSignature $original.FullName'
    if ($signerSource.IndexOf($signerAnchor) -lt 0 -or
        $signerSource.IndexOf($signerAnchor) -ne $signerSource.LastIndexOf($signerAnchor)) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE signer helper call anchor missing/ambiguous'
    }
    $signerProbe = $signerSource.Replace($signerAnchor,
        $signerAnchor.Replace('Get-TrustedAuthenticodeSignature $original.FullName',
            "(& { throw 'INJECTED_TRUSTED_SIGNER_HELPER_CALLED' })"))
    $signerProbeScript = Join-Path $fixture 'trusted-signer-probe.ps1'
    [IO.File]::WriteAllText($signerProbeScript, $signerProbe,
        (New-Object System.Text.UTF8Encoding($false)))
    $originalTool = $stageTool
    try {
        $stageTool = $signerProbeScript
        Require-Refusal 'trusted-signer-helper-executed' $unsignedPins 'INJECTED_TRUSTED_SIGNER_HELPER_CALLED'
    } finally { $stageTool = $originalTool }

    # Import the production SHA helper in a test-only scope. A known digest
    # from Windows PowerShell's independent SHA256 cmdlet is the oracle, and a
    # hostile same-scope function must not alter the helper's result.
    $stageAst = [System.Management.Automation.Language.Parser]::ParseFile(
        $stageTool, [ref]$null, [ref]$null)
    $shaHelpers = @($stageAst.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -ceq 'Get-TrustedFileSha256'
    }, $true))
    if ($shaHelpers.Count -ne 1) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE unique SHA256 helper missing'
    }
    . ([scriptblock]::Create($shaHelpers[0].Extent.Text))
    & {
        function Get-FileHash { throw 'KEL270_STAGE_SHADOW_FUNCTION_CALLED' }
        if ((Get-TrustedFileSha256 $fake) -cne $hash) {
            throw 'KEL270_STAGE_NEGATIVE_FAILURE SHA256 helper disagrees with direct oracle'
        }
    }
    Write-Output 'KEL270_STAGE_SHA256_SHADOW_RESISTANCE_PASS real_file_bytes_verified=true'

    $validPinsFile = Join-Path $fixture 'pins.json'
    Require-PathRefusal 'drive-relative-output' $validPinsFile 'C:relative-stage' 'requires a fully qualified local drive path'
    Require-PathRefusal 'root-relative-output' $validPinsFile '\relative-stage' 'requires a fully qualified local drive path'
    $junction = Join-Path $fixture 'via-junction'
    $null = New-Item -ItemType Junction -Path $junction -Target $fixture -ErrorAction Stop
    Require-PathRefusal 'reparse-ancestor-pins' (Join-Path $junction 'pins.json') (Join-Path $fixture 'no-stage') 'reparse-point path or ancestor'
    # This is a deliberate test-created junction, not a recursively owned tree.
    [IO.Directory]::Delete($junction, $false)

    # Exercise cleanup at a post-copy cut. This uses a private TEST COPY of
    # the script with signer validation removed ONLY to reach the cut with
    # unsigned structural inputs; it cannot qualify any Keld release.
    $stageSource = [IO.File]::ReadAllText($stageTool)
    $authStart = $stageSource.IndexOf('    $authenticode = Get-TrustedAuthenticodeSignature $original.FullName')
    $authEnd = $stageSource.IndexOf('    if ((Get-TrustedFileSha256 $original.FullName) -cne $digest)', $authStart)
    if ($authStart -lt 0 -or $authEnd -le $authStart) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE signer mutation anchors missing'
    }
    $testOnly = $stageSource.Remove($authStart, $authEnd - $authStart)
    $copyAnchor = '    [IO.File]::Copy($entry.source, $target, $false)'
    if ($testOnly.IndexOf($copyAnchor) -lt 0 -or
        $testOnly.IndexOf($copyAnchor) -ne $testOnly.LastIndexOf($copyAnchor)) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE postcopy injection anchor missing/ambiguous'
    }
    # Force each child-directory substitution immediately before the catch.
    # The sentinel is outside the staging tree, but inside this disposable
    # test fixture. The original stage directory is moved aside, never deleted
    # by the injection. The original cleanup implementation deletes the
    # sentinel through the junction before inspecting that directory.
    foreach ($cleanupCase in @(
        @{ child = 'bin'; leaf = 'keld-host.exe' },
        @{ child = 'bindings'; leaf = 'keld-mcp-stdio.json' },
        @{ child = 'recovery'; leaf = 'keld-host.exe' }
    )) {
        $child = $cleanupCase.child
        $caseParent = Join-Path $fixture ("cleanup-alias-$child")
        [IO.Directory]::CreateDirectory($caseParent) | Out-Null
        $outside = Join-Path $caseParent 'outside-stage'
        [IO.Directory]::CreateDirectory($outside) | Out-Null
        $sentinel = Join-Path $outside $cleanupCase.leaf
        [IO.File]::WriteAllText($sentinel, "INDEPENDENT_OUTSIDE_SENTINEL_$child")
        $sentinelHash = (Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash
        $aliasInjection = @(
            ('$slot = Join-Path $temporary ''{0}''' -f $child),
            '[IO.Directory]::Move($slot, ($slot + ''-original''))',
            '$aliasTarget = Join-Path $parent ''outside-stage''',
            '$null = New-Item -ItemType Junction -Path $slot -Target $aliasTarget -ErrorAction Stop',
            'throw ''INJECTED_POST_COPY_ALIAS_FAULT'''
        ) -join [Environment]::NewLine
        $aliasSource = $testOnly.Replace(
            $copyAnchor, $copyAnchor + [Environment]::NewLine + $aliasInjection)
        $aliasScript = Join-Path $fixture ("fault-cleanup-$child.ps1")
        [IO.File]::WriteAllText($aliasScript, $aliasSource, (New-Object System.Text.UTF8Encoding($false)))
        $validPinsFile = Join-Path $fixture 'pins.json'
        [IO.File]::WriteAllText($validPinsFile, ($unsignedPins | ConvertTo-Json -Depth 6))
        $aliasOutput = Join-Path $caseParent 'output'
        $aliasCasePassed = $false
        try {
            $ErrorActionPreference = 'Continue'
            try {
                $aliasResult = (& $powershell '-NoProfile' '-NonInteractive' '-File' $aliasScript '-PinsFile' $validPinsFile '-OutputDirectory' $aliasOutput 2>&1 | Out-String)
                $aliasExit = $LASTEXITCODE
            } finally { $ErrorActionPreference = 'Stop' }
            $aliasTemps = @(Get-ChildItem -LiteralPath $caseParent -Directory -Force |
                Where-Object { $_.Name -match '^\.keld-v2-stage-[0-9a-f]{32}$' })
            if (-not [IO.File]::Exists($sentinel) -or
                (Get-FileHash -LiteralPath $sentinel -Algorithm SHA256).Hash -cne $sentinelHash) {
                throw "KEL270_STAGE_NEGATIVE_FAILURE case=postcopy-alias-$child outside sentinel deleted or changed"
            }
            if ($aliasExit -eq 0 -or
                $aliasResult -notmatch 'INJECTED_POST_COPY_ALIAS_FAULT' -or
                $aliasResult -notmatch 'partial stage retained' -or
                $aliasResult -match 'KEL270_STAGE_IMAGES_PREPARED' -or
                (Test-Path -LiteralPath $aliasOutput) -or $aliasTemps.Count -ne 1) {
                throw "KEL270_STAGE_NEGATIVE_FAILURE case=postcopy-alias-$child exit=$aliasExit output=$aliasResult"
            }
            $aliasSlot = Join-Path $aliasTemps[0].FullName $child
            if (((Get-Item -LiteralPath $aliasSlot -Force).Attributes -band
                    [IO.FileAttributes]::ReparsePoint) -eq 0) {
                throw "KEL270_STAGE_NEGATIVE_FAILURE case=postcopy-alias-$child substitution absent"
            }
            Write-Output "KEL270_STAGE_NEGATIVE_PASS case=postcopy-alias-$child outside_sentinel_unchanged=true"
            $aliasCasePassed = $true
        } finally {
            # Unlink only the exact junction established by the successful
            # oracle. On any failure the outer finally must retain the fixture.
            if ($aliasCasePassed) {
                [IO.Directory]::Delete($aliasSlot, $false)
            }
        }
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
        $faultResult -notmatch 'partial stage retained' -or
        $faultResult -match 'KEL270_STAGE_IMAGES_PREPARED' -or
        (Test-Path -LiteralPath $faultOutput) -or $leftovers.Count -ne 1) {
        throw "KEL270_STAGE_NEGATIVE_FAILURE case=postcopy-cleanup exit=$faultExit temps=$($leftovers.Count) output=$faultResult"
    }
    $retainedImage = Join-Path $leftovers[0].FullName 'bin/keld-host.exe'
    if (-not [IO.File]::Exists($retainedImage) -or
        (Get-FileHash -LiteralPath $retainedImage -Algorithm SHA256).Hash -cne $unsignedPins.images[0].sha256) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE case=postcopy-cleanup staged bytes not retained'
    }
    Write-Output 'KEL270_STAGE_NEGATIVE_PASS case=postcopy-cleanup partial_stage_retained=true'

    # Substitute generated bytes immediately after writing them.
    foreach ($metadataCase in @(
        @{ name = 'preclose-recipe-substitution'; variable = '$recipePath'; bytes = '$recipeBytes'; relative = 'bindings/keld-mcp-stdio.json'; expected = 'CLI/provider recipe changed before manifest closeout' },
        @{ name = 'preclose-manifest-substitution'; variable = '$manifestPath'; bytes = '$manifestBytes'; relative = 'manifest.json'; expected = 'staged manifest changed before publication' }
    )) {
        $caseParent = Join-Path $fixture $metadataCase.name
        [IO.Directory]::CreateDirectory($caseParent) | Out-Null
        $metadataSource = $stageSource.Remove($authStart, $authEnd - $authStart)
        $anchor = 'Write-NewStageBytes ' + $metadataCase.variable + ' ' + $metadataCase.bytes
        if ($metadataSource.IndexOf($anchor) -lt 0 -or $metadataSource.IndexOf($anchor) -ne $metadataSource.LastIndexOf($anchor)) {
            throw 'KEL270_STAGE_NEGATIVE_FAILURE metadata write anchor missing/ambiguous'
        }
        $foreign = 'INJECTED_FOREIGN_METADATA_BYTES'
        $inject = '[IO.File]::WriteAllText({0}, ''{1}'')' -f $metadataCase.variable, $foreign
        $metadataSource = $metadataSource.Replace($anchor, $anchor + [Environment]::NewLine + $inject)
        $metadataScript = Join-Path $fixture ($metadataCase.name + '.ps1')
        [IO.File]::WriteAllText($metadataScript, $metadataSource, (New-Object System.Text.UTF8Encoding($false)))
        $metadataOutput = Join-Path $caseParent 'output'
        $ErrorActionPreference = 'Continue'
        try {
            $metadataResult = (& $powershell '-NoProfile' '-NonInteractive' '-File' $metadataScript '-PinsFile' $validPinsFile '-OutputDirectory' $metadataOutput 2>&1 | Out-String)
            $metadataExit = $LASTEXITCODE
        } finally { $ErrorActionPreference = 'Stop' }
        $metadataTemps = @(Get-ChildItem -LiteralPath $caseParent -Directory -Force | Where-Object { $_.Name -match '^\.keld-v2-stage-[0-9a-f]{32}$' })
        if ($metadataExit -eq 0 -or $metadataResult -match 'KEL270_STAGE_IMAGES_PREPARED' -or
            $metadataResult -notmatch [regex]::Escape($metadataCase.expected) -or $metadataResult -notmatch 'partial stage retained' -or
            (Test-Path -LiteralPath $metadataOutput) -or $metadataTemps.Count -ne 1) {
            throw "KEL270_STAGE_NEGATIVE_FAILURE case=$($metadataCase.name) exit=$metadataExit output=$metadataResult"
        }
        if ([IO.File]::ReadAllText((Join-Path $metadataTemps[0].FullName $metadataCase.relative)) -cne $foreign) {
            throw 'KEL270_STAGE_NEGATIVE_FAILURE injected metadata bytes not retained'
        }
        Write-Output "KEL270_STAGE_NEGATIVE_PASS case=$($metadataCase.name)"
    }

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

    # Closed inventory at both publication cuts. The test source bypasses
    # signature validation only for these synthetic unsigned byte fixtures.
    $preInventoryAnchor = "Require-StageInventory `$temporary `$images 'temporary stage'"
    foreach ($inventoryCase in @(
        @{ name = 'prepublish-foreign-root'; anchor = $preInventoryAnchor; root = '$temporary'; relative = 'foreign.txt'; expected = 'temporary stage inventory has missing or foreign root entries'; before = $true },
        @{ name = 'postpublish-foreign-child'; anchor = $moveAnchor; root = '$OutputDirectory'; relative = 'bin/foreign.txt'; expected = 'published stage inventory has missing or foreign bin entries'; before = $false }
    )) {
        $inventorySource = $stageSource.Remove($authStart, $authEnd - $authStart)
        $anchor = $inventoryCase.anchor
        if ($inventorySource.IndexOf($anchor) -lt 0 -or $inventorySource.IndexOf($anchor) -ne $inventorySource.LastIndexOf($anchor)) {
            throw 'KEL270_STAGE_NEGATIVE_FAILURE inventory injection anchor missing/ambiguous'
        }
        $inject = '[IO.File]::WriteAllText((Join-Path {0} ''{1}''), ''INJECTED_FOREIGN_INVENTORY_ENTRY'')' -f $inventoryCase.root, $inventoryCase.relative
        $replacement = if ($inventoryCase.before) { $inject + [Environment]::NewLine + $anchor } else { $anchor + [Environment]::NewLine + $inject }
        $inventorySource = $inventorySource.Replace($anchor, $replacement)
        $inventoryScript = Join-Path $fixture ($inventoryCase.name + '.ps1')
        [IO.File]::WriteAllText($inventoryScript, $inventorySource, (New-Object System.Text.UTF8Encoding($false)))
        $caseParent = Join-Path $fixture $inventoryCase.name
        [IO.Directory]::CreateDirectory($caseParent) | Out-Null
        $inventoryOutput = Join-Path $caseParent 'output'
        $ErrorActionPreference = 'Continue'
        try {
            $inventoryResult = (& $powershell '-NoProfile' '-NonInteractive' '-File' $inventoryScript '-PinsFile' $validPinsFile '-OutputDirectory' $inventoryOutput 2>&1 | Out-String)
            $inventoryExit = $LASTEXITCODE
        } finally { $ErrorActionPreference = 'Stop' }
        if ($inventoryExit -eq 0 -or $inventoryResult -match 'KEL270_STAGE_IMAGES_PREPARED' -or $inventoryResult -notmatch [regex]::Escape($inventoryCase.expected)) {
            throw "KEL270_STAGE_NEGATIVE_FAILURE case=$($inventoryCase.name) exit=$inventoryExit output=$inventoryResult"
        }
        $observedRoot = $inventoryOutput
        if ($inventoryCase.before) {
            $temps = @(Get-ChildItem -LiteralPath $caseParent -Directory -Force | Where-Object { $_.Name -match '^\.keld-v2-stage-[0-9a-f]{32}$' })
            if ($temps.Count -ne 1 -or (Test-Path -LiteralPath $inventoryOutput)) { throw 'KEL270_STAGE_NEGATIVE_FAILURE prepublish inventory not retained' }
            $observedRoot = $temps[0].FullName
        }
        if ([IO.File]::ReadAllText((Join-Path $observedRoot $inventoryCase.relative)) -cne 'INJECTED_FOREIGN_INVENTORY_ENTRY') {
            throw 'KEL270_STAGE_NEGATIVE_FAILURE foreign inventory entry not retained'
        }
        Write-Output "KEL270_STAGE_NEGATIVE_PASS case=$($inventoryCase.name)"
    }
    $exactSource = $stageSource.Remove($authStart, $authEnd - $authStart)
    $exactScript = Join-Path $fixture 'exact-inventory-test-only.ps1'
    [IO.File]::WriteAllText($exactScript, $exactSource, (New-Object System.Text.UTF8Encoding($false)))
    $exactOutput = Join-Path $fixture 'exact-inventory-output'
    $ErrorActionPreference = 'Continue'
    try {
        $exactResult = (& $powershell '-NoProfile' '-NonInteractive' '-File' $exactScript '-PinsFile' $validPinsFile '-OutputDirectory' $exactOutput 2>&1 | Out-String)
        $exactExit = $LASTEXITCODE
    } finally { $ErrorActionPreference = 'Stop' }
    $exactManifest = Join-Path $exactOutput 'manifest.json'
    if ($exactExit -ne 0 -or $exactResult -notmatch 'KEL270_STAGE_IMAGES_PREPARED_NONAUTHORIZING' -or -not [IO.File]::Exists($exactManifest)) {
        throw "KEL270_STAGE_NEGATIVE_FAILURE exact synthetic inventory exit=$exactExit output=$exactResult"
    }
    $expectedManifestHash = (Get-FileHash -LiteralPath $exactManifest -Algorithm SHA256).Hash
    $exactMetadata = Get-Content -LiteralPath $exactManifest -Raw | ConvertFrom-Json
    if ($exactResult -notmatch "manifest_sha256=$expectedManifestHash" -or $exactMetadata.providerRecipeSha256 -cne (Get-FileHash -LiteralPath (Join-Path $exactOutput 'bindings/keld-mcp-stdio.json') -Algorithm SHA256).Hash -or
        @(Get-ChildItem -LiteralPath $exactOutput -Force).Count -ne 4 -or @(Get-ChildItem -LiteralPath (Join-Path $exactOutput 'bin') -Force).Count -ne 4 -or
        @(Get-ChildItem -LiteralPath (Join-Path $exactOutput 'recovery') -Force).Count -ne 1 -or @(Get-ChildItem -LiteralPath (Join-Path $exactOutput 'bindings') -Force).Count -ne 1) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE exact inventory/digest readback differed'
    }
    Write-Output 'KEL270_STAGE_SYNTHETIC_INVENTORY_PASS exact_slots=true unsigned_test_only=true'

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
    $aliasCasePassed = $false
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
        $aliasCasePassed = $true
    } finally {
        # Never attempt recursive fixture cleanup if this unlink fails.
        if ($aliasCasePassed) {
            [IO.Directory]::Delete($aliasLink, $false)
        }
    }
    if ((Get-FileHash -LiteralPath $fake -Algorithm SHA256).Hash -cne $hash) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE: source changed during refusals'
    }
    $fixtureCleanupReady = $true
} finally {
    if ($fixtureCleanupReady) {
        # Only after every test and deliberate junction unlink succeeded.
        Remove-Item -LiteralPath $fixture -Recurse -Force -ErrorAction Stop
    } else {
        Write-Warning "KEL270_STAGE_TEST_FIXTURE_RETAINED: $fixture; test or junction cleanup failed"
    }
}
Write-Output 'KEL270_STAGE_NEGATIVE_SUITE_PASS count=25 source_unchanged=true positive_signed_release=NOT_RUN'
