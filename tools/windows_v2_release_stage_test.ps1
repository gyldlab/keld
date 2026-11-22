# Focused native refusals for the offline KEL-270 release-stage tool.
# No signer, installer, certificate, trust anchor, or positive release is created.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$stageTool = Join-Path $PSScriptRoot 'windows_v2_release_stage.ps1'
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

    if ((Get-FileHash -LiteralPath $fake -Algorithm SHA256).Hash -cne $hash) {
        throw 'KEL270_STAGE_NEGATIVE_FAILURE: source changed during refusals'
    }
    Write-Output 'KEL270_STAGE_NEGATIVE_SUITE_PASS count=4 source_unchanged=true positive_signed_release=NOT_RUN'
} finally {
    # Only this invocation's newly allocated disposable negative fixture.
    Remove-Item -LiteralPath $fixture -Recurse -Force
}
