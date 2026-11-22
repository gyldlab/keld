# Native Windows source-only negative controls for the KEL-270 NSIS builder.
# Five distinct Microsoft-signed SDK binaries serve ONLY as non-Keld test data;
# no Keld signature, PublicTrust profile or catalogue authority is simulated.
# Every invocation must refuse before creating a compiled installer or output.
[CmdletBinding()]
param([string]$BuilderOverride)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$builder = Join-Path $PSScriptRoot 'windows_v2_nsis_build.ps1'
if (-not [string]::IsNullOrWhiteSpace($BuilderOverride)) {
    $builder = $BuilderOverride
}
$nsis = Join-Path $PSScriptRoot 'windows_v2_nsis_installer.nsi'
$powershell = Join-Path $PSHOME 'powershell.exe'
$fixture = Join-Path $env:TEMP ('keld270-nsis-negative-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $fixture

function Run-Builder([string]$Entrypoint, [hashtable]$Values) {
    $arguments = @('-NoProfile', '-NonInteractive', '-File', $Entrypoint)
    foreach ($name in @(
        'StageDirectory', 'ExpectedStageManifestSha256', 'ExpectedSourceCommit',
        'CommissionedCatalogue', 'ExpectedCatalogueSha256',
        'MakensisExe', 'OutputDirectory'
    )) {
        if ($Values.ContainsKey($name)) {
            $arguments += ('-' + $name)
            $arguments += [string]$Values[$name]
        }
    }
    $ErrorActionPreference = 'Continue'
    try {
        $output = (& $powershell @arguments 2>&1 | Out-String)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
    }
    return [pscustomobject]@{ Output = $output; ExitCode = $exitCode }
}

function Assert-Refusal([string]$Case, [string]$Entrypoint,
                        [hashtable]$Values, [string]$Fragment) {
    $result = Run-Builder $Entrypoint $Values
    if ($result.ExitCode -eq 0 -or
        $result.Output -notmatch [regex]::Escape($Fragment) -or
        $result.Output -match 'KEL270_NSIS_UNSIGNED_CANDIDATE_COMPILED' -or
        (Test-Path -LiteralPath $Values.OutputDirectory)) {
        throw "KEL270_NSIS_NEGATIVE_FAILED case=$Case exit=$($result.ExitCode) output=$($result.Output)"
    }
    Write-Output "KEL270_NSIS_NEGATIVE_PASS case=$Case"
}

try {
    $output = Join-Path $fixture 'compiled-must-not-exist'
    Assert-Refusal 'missing-inputs' $builder @{
        OutputDirectory = $output
    } 'missing required StageDirectory'

    $stage = Join-Path $fixture 'stage'
    foreach ($dir in @('bin', 'recovery', 'bindings')) {
        $null = New-Item -ItemType Directory -Path (Join-Path $stage $dir) -Force
    }
    $sdk = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64'
    $layout = @(
        @('app-host', 'bin/keld-host.exe', 'accevent.exe'),
        @('recovery-host', 'recovery/keld-host.exe', 'adpcmencode3.exe'),
        @('cli', 'bin/keld.exe', 'betest.exe'),
        @('role-launcher', 'bin/keld-role-launcher.exe', 'cameraprofiletool.exe'),
        @('updater-helper', 'bin/keld-updater-helper.exe', 'cert2spc.exe')
    )
    $artifacts = @()
    foreach ($row in $layout) {
        $sdkFile = Join-Path $sdk $row[2]
        if (-not (Test-Path -LiteralPath $sdkFile -PathType Leaf)) {
            throw "KEL270_NSIS_TEST_UNAVAILABLE: reviewed native Windows SDK signed fixture $sdkFile is unavailable"
        }
        $signature = Get-AuthenticodeSignature -LiteralPath $sdkFile
        if ($signature.Status.ToString() -cne 'Valid' -or
            $signature.SignatureType.ToString() -cne 'Authenticode') {
            throw "KEL270_NSIS_TEST_UNAVAILABLE: OS-signed SDK fixture cannot be verified"
        }
        $target = Join-Path $stage $row[1]
        [IO.File]::Copy($sdkFile, $target, $false)
        $artifacts += [ordered]@{
            role = $row[0]
            relativePath = $row[1]
            sha256 = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
            length = [long](Get-Item -LiteralPath $target).Length
            expectedSignerThumbprint = $signature.SignerCertificate.Thumbprint
        }
    }
    $recipe = Join-Path $stage 'bindings/keld-mcp-stdio.json'
    [IO.File]::WriteAllText($recipe, '{"schema":"keld.windows-cli-provider-binding/v1","state":"recipe-only-install-root-not-bound"}')
    $catalogue = Join-Path $fixture 'UNCOMMISSIONED-CATALOGUE.bin'
    [IO.File]::WriteAllBytes($catalogue, [byte[]](0x4b, 0x45, 0x4c, 0x44))
    $manifest = [ordered]@{
        schema = 'keld.windows-v2-signed-image-stage/v1'
        state = 'non-authorizing-path-observations-only'
        sourceCommit = '26fbefd74c6e087daace6a4cc9eaf186727bc09f'
        artifacts = $artifacts
        providerRecipePath = 'bindings/keld-mcp-stdio.json'
        providerRecipeSha256 = (Get-FileHash -LiteralPath $recipe -Algorithm SHA256).Hash
        nativeKeldProfileAndInstallationAuthority = 'NOT_VERIFIED'
    }
    $manifestPath = Join-Path $stage 'manifest.json'
    function Write-Manifest($Content) {
        [IO.File]::WriteAllText($manifestPath, ($Content | ConvertTo-Json -Depth 8))
        return (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash
    }
    $pin = Write-Manifest $manifest
    $arguments = @{
        StageDirectory = $stage
        ExpectedStageManifestSha256 = $pin
        ExpectedSourceCommit = $manifest.sourceCommit
        CommissionedCatalogue = $catalogue
        ExpectedCatalogueSha256 = (Get-FileHash -LiteralPath $catalogue -Algorithm SHA256).Hash
        MakensisExe = Join-Path $fixture 'makensis-not-approved.exe'
        OutputDirectory = $output
    }
    $missingHostAuthority = 'KEL270-NSIS-OWNER-UNCOMMISSIONED'
    Assert-Refusal 'missing-original-host-commissioning' $builder $arguments $missingHostAuthority
    $wrongHash = $arguments.Clone()
    $wrongHash.ExpectedStageManifestSha256 = '0' * 64
    Assert-Refusal 'stage-manifest-hash-mismatch' $builder $wrongHash 'differs from its independently pinned SHA256'
    $wrongCatalogue = $arguments.Clone()
    $wrongCatalogue.ExpectedCatalogueSha256 = '0' * 64
    Assert-Refusal 'catalogue-hash-mismatch' $builder $wrongCatalogue 'differs from its independently pinned SHA256'

    # The final output does not exist yet. Its reparse ancestor must still be
    # checked before the compiler has any chance to create a private sibling.
    $outside = Join-Path $fixture 'outside'
    $alias = Join-Path $fixture 'alias'
    [IO.Directory]::CreateDirectory($outside) | Out-Null
    $null = New-Item -ItemType Junction -Path $alias -Target $outside
    $reparseArgs = $arguments.Clone()
    $reparseArgs.OutputDirectory = Join-Path $alias 'compiled-must-not-exist'
    Assert-Refusal 'absent-output-reparse-ancestor' $builder $reparseArgs 'reparse-point ancestor'
    if (Test-Path -LiteralPath $reparseArgs.OutputDirectory) {
        throw 'KEL270_NSIS_REPARSE_FAULT: output appeared under outside directory'
    }

    # A fresh -NoProfile child must not run caller-owned scripts that shadow
    # autoloaded digest/signature commands. All three cases still reach the
    # original Host's uncommissioned refusal after checking real fixture bytes.
    $shadowHash = Join-Path $fixture 'shadow-hash'
    [IO.Directory]::CreateDirectory($shadowHash) | Out-Null
    [IO.File]::WriteAllText((Join-Path $shadowHash 'Get-FileHash.ps1'),
        "throw 'KEL270_NSIS_SHADOW_HASH_CALLED'")
    $originalPath = $env:PATH
    try {
        $env:PATH = $shadowHash + ';' + $originalPath
        Assert-Refusal 'path-shadowed-digest' $builder $arguments $missingHostAuthority
    } finally {
        $env:PATH = $originalPath
    }

    $shadowSigner = Join-Path $fixture 'shadow-signer'
    [IO.Directory]::CreateDirectory($shadowSigner) | Out-Null
    [IO.File]::WriteAllText((Join-Path $shadowSigner 'Get-AuthenticodeSignature.ps1'),
        "throw 'KEL270_NSIS_SHADOW_SIGNER_CALLED'")
    try {
        $env:PATH = $shadowSigner + ';' + $originalPath
        Assert-Refusal 'path-shadowed-signer' $builder $arguments $missingHostAuthority
    } finally {
        $env:PATH = $originalPath
    }

    $shadowModuleRoot = Join-Path $fixture 'shadow-modules'
    $shadowSecurityModule = Join-Path $shadowModuleRoot 'Microsoft.PowerShell.Security'
    [IO.Directory]::CreateDirectory($shadowSecurityModule) | Out-Null
    [IO.File]::WriteAllText((Join-Path $shadowSecurityModule 'Microsoft.PowerShell.Security.psm1'),
        "function Get-AuthenticodeSignature { throw 'KEL270_NSIS_SHADOW_MODULE_CALLED' }; Export-ModuleMember -Function Get-AuthenticodeSignature")
    $originalModulePath = $env:PSModulePath
    try {
        $env:PSModulePath = $shadowModuleRoot + ';' + $originalModulePath
        Assert-Refusal 'module-shadowed-signer' $builder $arguments $missingHostAuthority
    } finally {
        $env:PSModulePath = $originalModulePath
    }

    $manifest.artifacts[0].expectedSignerThumbprint = '0' * 40
    $arguments.ExpectedStageManifestSha256 = Write-Manifest $manifest
    Assert-Refusal 'signer-thumbprint-mutation' $builder $arguments 'differs from its reviewed embedded Authenticode signer'

    # RED control: only in this disposable test source, remove the exact signer
    # thumbprint comparison. The independent signer-refusal oracle must turn RED.
    # This mutant still cannot compile: the original Host contract is missing.
    $original = [IO.File]::ReadAllText($builder)
    $needle = '$signature.SignerCertificate.Thumbprint.ToUpperInvariant() -cne $Signer'
    if (-not $original.Contains($needle)) {
        throw 'KEL270_NSIS_MUTANT_NOT_APPLIED: missing original signer predicate'
    }
    $mutant = Join-Path $fixture 'mutant-builder.ps1'
    [IO.File]::WriteAllText($mutant, $original.Replace($needle, '$false'))
    $mutantResult = Run-Builder $mutant $arguments
    if ($mutantResult.ExitCode -eq 0 -or
        $mutantResult.Output -match 'differs from its reviewed embedded Authenticode signer' -or
        $mutantResult.Output -notmatch [regex]::Escape($missingHostAuthority) -or
        (Test-Path -LiteralPath $output)) {
        throw "KEL270_NSIS_SIGNER_MUTANT_UNEXPECTED exit=$($mutantResult.ExitCode) output=$($mutantResult.Output)"
    }
    Write-Output 'KEL270_NSIS_SIGNER_MUTATION_RED expected_signer_refusal_removed=true original_host_gate_still_closed=true'

    # Actual Windows filesystem cleanup oracle for an injected compiler fault.
    # Load ONLY the three private production functions from their AST. Running
    # the builder as a whole would bypass the uncommissioned Host authority;
    # this controlled fault does not run makensis or claim compiler acceptance.
    $parseTokens = $null
    $parseErrors = $null
    $builderAst = [System.Management.Automation.Language.Parser]::ParseFile(
        $builder, [ref]$parseTokens, [ref]$parseErrors
    )
    if ($parseErrors.Count -ne 0) {
        throw 'KEL270_NSIS_TEST_SOURCE: builder must parse before cleanup test'
    }
    foreach ($name in @(
        'Refuse', 'Require-LocalPath', 'Remove-OwnedPrivateCandidate', 'Require-ExactHash'
    )) {
        $functions = @($builderAst.FindAll({
            param($node)
            $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -ceq $name
        }, $true))
        if ($functions.Count -ne 1) {
            throw "KEL270_NSIS_TEST_SOURCE: missing unique original $name function"
        }
        . ([scriptblock]::Create($functions[0].Extent.Text))
    }
    # Use the exact production hash helpers with a same-scope malicious
    # Get-FileHash function. The independently recorded manifest digest is
    # the oracle; success must not depend on command lookup in this scope.
    $hashHelpers = @($builderAst.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -ceq 'Get-TrustedFileSha256'
    }, $true))
    if ($hashHelpers.Count -gt 1) {
        throw 'KEL270_NSIS_TEST_SOURCE: duplicate SHA256 helper'
    }
    if ($hashHelpers.Count -eq 1) {
        . ([scriptblock]::Create($hashHelpers[0].Extent.Text))
    }
    & {
        function Get-FileHash { throw 'KEL270_NSIS_SHADOW_HASH_FUNCTION_CALLED' }
        Require-ExactHash $manifestPath $arguments.ExpectedStageManifestSha256 'W38 manifest shadow control'
    }
    Write-Output 'KEL270_NSIS_SHA256_SHADOW_RESISTANCE_PASS real_file_bytes_verified=true'

    $compilerParent = Join-Path $fixture 'compiler-parent'
    [IO.Directory]::CreateDirectory($compilerParent) | Out-Null
    $private = Join-Path $compilerParent ('.keld-v2-nsis-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($private) | Out-Null
    $partial = Join-Path $private 'keld-v2-peruserdirect-UNSIGNED.exe'
    [IO.File]::WriteAllBytes($partial, [byte[]](0x4d, 0x5a, 0x00))
    $fakePublished = Join-Path $compilerParent 'refused-final-output'
    try {
        throw 'INJECTED_NSIS_COMPILER_NONZERO_AFTER_PARTIAL_OUTPUT'
    } catch {
        Remove-OwnedPrivateCandidate $private $compilerParent
    }
    if ((Test-Path -LiteralPath $private) -or (Test-Path -LiteralPath $fakePublished)) {
        throw 'KEL270_NSIS_COMPILER_FAULT: partial unsigned output survived refusal'
    }
    Write-Output 'KEL270_NSIS_COMPILER_PARTIAL_CLEANUP_PASS native_file_removed=true final_output_absent=true'

    # Static source checks: no caller-chosen installation root, persistence
    # side effect, elevation or NSIS shell execution; only original Host role.
    $active = @([IO.File]::ReadAllLines($nsis) | Where-Object { $_ -notmatch '^\s*;' })
    $sourceText = $active -join "`n"
    foreach ($mandatory in @('RequestExecutionLevel user', 'InitPluginsDir',
                              '--keld-install-initial-v3', 'ExecWait', 'CRCCheck force')) {
        if (-not $sourceText.Contains($mandatory)) {
            throw "KEL270_NSIS_SOURCE_MISSING: $mandatory"
        }
    }
    if ($sourceText -match '(?i)\$INSTDIR|WriteReg|WriteUninstaller|ExecShell|RequestExecutionLevel\s+admin|File\s+/r|!system|\$PROGRAMFILES') {
        throw 'KEL270_NSIS_SOURCE_FORBIDDEN: native installer source contains an unapproved install or authority action'
    }
    $builderText = [IO.File]::ReadAllText($builder)
    foreach ($contract in @(
        'Remove-OwnedPrivateCandidate',
        'Remove-OwnedPrivateCandidate $temporary $parent',
        '[IO.Directory]::Move($temporary, $destination)',
        'KEL270_NSIS_UNSIGNED_CANDIDATE_COMPILED'
    )) {
        if (-not $builderText.Contains($contract)) {
            throw "KEL270_NSIS_BUILD_SOURCE_MISSING: $contract"
        }
    }
    if ($builderText.Contains('NativeCommissioningGateExe') -or
        $builderText.Contains('--verify-windows-v2-nsis-inputs-v1')) {
        throw 'KEL270_NSIS_BUILD_SOURCE_FORBIDDEN: uncommissioned parallel native authority'
    }
    Write-Output 'KEL270_NSIS_SOURCE_PASS original_host=true user_level=true persistent_target_not_selected=true'
    Write-Output 'KEL270_NSIS_NEGATIVE_SUITE_PASS count=9 source_only=true installer=NOT_BUILT signed_release=NOT_RUN'
} finally {
    $expectedFixture = '^keld270-nsis-negative-[a-f0-9]{32}$'
    if ([IO.Path]::GetDirectoryName($fixture) -cne $env:TEMP -or
        [IO.Path]::GetFileName($fixture) -cnotmatch $expectedFixture) {
        throw 'KEL270_NSIS_TEST_CLEANUP: refusing an unowned fixture path'
    }
    # Delete only the exact disposable files this test created. Unknown
    # entries and reparse points cause cleanup refusal, never recursive travel.
    foreach ($relative in @(
        'stage/bin/keld-host.exe', 'stage/bin/keld.exe',
        'stage/bin/keld-role-launcher.exe', 'stage/bin/keld-updater-helper.exe',
        'stage/recovery/keld-host.exe', 'stage/bindings/keld-mcp-stdio.json',
        'stage/manifest.json', 'UNCOMMISSIONED-CATALOGUE.bin', 'mutant-builder.ps1',
        'shadow-hash/Get-FileHash.ps1',
        'shadow-signer/Get-AuthenticodeSignature.ps1',
        'shadow-modules/Microsoft.PowerShell.Security/Microsoft.PowerShell.Security.psm1'
    )) {
        $itemPath = Join-Path $fixture $relative
        $item = Get-Item -LiteralPath $itemPath -Force -ErrorAction SilentlyContinue
        if ($null -ne $item) {
            if ($item -isnot [IO.FileInfo] -or
                (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
                throw 'KEL270_NSIS_TEST_CLEANUP: unexpected file type or reparse point'
            }
            [IO.File]::Delete($itemPath)
        }
    }
    $link = Join-Path $fixture 'alias'
    $linkItem = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
    if ($null -ne $linkItem) {
        if (($linkItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) {
            throw 'KEL270_NSIS_TEST_CLEANUP: alias is no longer a reparse point'
        }
        [IO.Directory]::Delete($link, $false)
    }
    foreach ($relative in @(
        'stage/bin', 'stage/recovery', 'stage/bindings', 'stage',
        'compiler-parent', 'outside', 'shadow-hash', 'shadow-signer',
        'shadow-modules/Microsoft.PowerShell.Security', 'shadow-modules', ''
    )) {
        $directory = if ($relative -eq '') { $fixture } else { Join-Path $fixture $relative }
        $item = Get-Item -LiteralPath $directory -Force -ErrorAction SilentlyContinue
        if ($null -ne $item) {
            if ($item -isnot [IO.DirectoryInfo] -or
                (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
                throw 'KEL270_NSIS_TEST_CLEANUP: unexpected directory or reparse point'
            }
            [IO.Directory]::Delete($directory, $false)
        }
    }
}
