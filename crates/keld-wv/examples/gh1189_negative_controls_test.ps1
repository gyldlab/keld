# Source-only regression: actual artifact writers, no observer/fixture executable.
[CmdletBinding()]
param([string]$EvidenceDirectory = (Join-Path $env:TEMP 'gh1189-terminal-status-test'))
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$runner = Join-Path $PSScriptRoot 'gh1189_negative_controls.ps1'
$source = [IO.File]::ReadAllText($runner)
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'Runner parse failed' }
function Find-Assignment([string]$name) {
    $ast.Find({
        param($node)
        $node -is [Management.Automation.Language.AssignmentStatementAst] -and
            $node.Left -is [Management.Automation.Language.VariableExpressionAst] -and
            $node.Left.VariablePath.UserPath -eq $name
    }, $true)
}
$nativeCase = $ast.Find({
    param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-ObservedCase'
}, $true)
$guard = $ast.Find({
    param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Assert-NoReparsePath'
}, $true)
$begin = Find-Assignment 'attemptId'
$caseList = Find-Assignment 'cases'
if (-not $nativeCase -or -not $guard -or -not $begin -or -not $caseList) { throw 'Artifact test seam missing' }
$writers = @($nativeCase.Body.EndBlock.Statements | Where-Object { $_.Extent.Text -match '^Write-CaseTextOnce\b' })
if ($writers.Count -ne 2) { throw 'Expected both real case-log writers' }
$script:caseWriters = [scriptblock]::Create(($writers | ForEach-Object { $_.Extent.Text }) -join [Environment]::NewLine)
. ([scriptblock]::Create($guard.Extent.Text))
. ([scriptblock]::Create($caseList.Extent.Text))
$stub = @'
function Invoke-ObservedCase([string]$label, [string]$fault, [string]$expected) {
    if ($label -eq 'healthy-initial') {
        $script:startPath = Join-Path $evidence 'attempt.json'
        $script:startBytes = [IO.File]::ReadAllText($script:startPath)
    }
    $record = [pscustomobject]@{ Stdout = "source-only $script:marker $label"; Stderr = "source-only $script:marker $label" }
    . $script:caseWriters
    if ($label -eq $script:failOnCase) {
        if ($script:marker -eq 'D') {
            Write-CaseTextOnce (Join-Path $evidence 'failure.json') 'existing receipt must not be replaced'
        }
        throw "INJECTED_CASE_FAILURE:$label"
    }
    [void]$results.Add([pscustomobject]@{ Case=$label; Exit=$(if($expected){1}else{0}); ElapsedMs=0; Identity="source-only-$script:marker-$label" })
}
'@
$offset = $begin.Extent.StartOffset
$programText = $source.Substring($offset)
$relative = $nativeCase.Extent.StartOffset - $offset
$programText = $programText.Remove($relative, $nativeCase.Extent.Text.Length).Insert($relative, $stub)
if ($programText.Contains('New-Object System.Diagnostics.Process')) { throw 'Native launcher survived test seam' }
$program = [scriptblock]::Create($programText)
New-Item -ItemType Directory -Path $EvidenceDirectory -ErrorAction Stop | Out-Null
$actualTarget = Join-Path $EvidenceDirectory 'target'
$fixture = Join-Path $EvidenceDirectory 'fixture.txt'
$observer = Join-Path $EvidenceDirectory 'observer.txt'
[IO.File]::WriteAllText($fixture, 'source-only fixture sentinel; never executable')
[IO.File]::WriteAllText($observer, 'source-only observer sentinel; never executable')
$FixtureSha256 = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash
$ObserverSha256 = (Get-FileHash -LiteralPath $observer -Algorithm SHA256).Hash
$ReviewedCommit = '1' * 40
$IssueClaimUrl = 'source-only-no-native-claim'
$NativeSlotUrl = 'source-only-no-native-slot'
# This is a source-only artifact writer test, not a build receipt or native launch.
$receiptPath = Join-Path $EvidenceDirectory 'source-only-build-receipt.json'
$buildReceiptSha256 = 'A' * 64
$root = Join-Path $actualTarget 'gh1189\negative-controls'

$script:marker = 'A'
$script:failOnCase = ''
& $program | Out-Null
$first = @(Get-ChildItem -LiteralPath $root -Directory)
if ($first.Count -ne 1) { throw 'Successful attempt directory missing' }
$successPath = Join-Path $first[0].FullName 'results.json'
$successBytes = [IO.File]::ReadAllText($successPath)
$success = $successBytes | ConvertFrom-Json
if ($success.status -ne 'NATIVE_TEST_ONLY_CONTROLS_PASS' -or $success.results.Count -ne 9 -or
    $success.build_receipt_path -ne $receiptPath -or $success.build_receipt_sha256 -ne $buildReceiptSha256) {
    throw 'Positive publication control or build provenance failed'
}
if (Test-Path -LiteralPath (Join-Path $first[0].FullName 'failure.json')) { throw 'Healthy attempt published failure' }

$script:marker = 'B'
$script:failOnCase = 'fault-trailing-byte'
$failureMessage = $null
try { & $program | Out-Null } catch { $failureMessage = $_.Exception.Message }
if ($failureMessage -ne 'INJECTED_CASE_FAILURE:fault-trailing-byte') { throw "Original failure was lost: $failureMessage" }
$second = @(Get-ChildItem -LiteralPath $root -Directory | Where-Object { $_.FullName -ne $first[0].FullName })
if ($second.Count -ne 1) { throw 'Failed attempt must have a distinct directory' }
if ([IO.File]::ReadAllText($successPath) -ne $successBytes) { throw 'Earlier success was changed' }
if ([IO.File]::ReadAllText($script:startPath) -ne $script:startBytes) { throw 'Immutable start metadata was changed' }
if (Test-Path -LiteralPath (Join-Path $second[0].FullName 'results.json')) { throw 'Failed attempt published success' }
$failurePath = Join-Path $second[0].FullName 'failure.json'
if (-not (Test-Path -LiteralPath $failurePath)) { throw 'REGRESSION: failed attempt has no terminal FAILED record' }
$failure = Get-Content -LiteralPath $failurePath -Raw | ConvertFrom-Json
$start = $script:startBytes | ConvertFrom-Json
if ($failure.status -ne 'FAILED' -or $failure.attempt_id -ne $start.attempt_id -or
    $failure.reviewed_commit -ne $ReviewedCommit -or $failure.fixture_sha256 -ne $FixtureSha256 -or
    $failure.observer_sha256 -ne $ObserverSha256 -or
    $failure.build_receipt_path -ne $receiptPath -or $failure.build_receipt_sha256 -ne $buildReceiptSha256 -or
    $failure.error -ne $failureMessage -or
    $failure.failed_case -ne 'fault-trailing-byte' -or -not $failure.completed_utc) {
    throw 'Terminal failure identity, provenance, reason, case or completion time is wrong'
}
if ($failure.results.Count -ne 1 -or $failure.results[0].Case -ne 'healthy-initial') { throw 'Failure receipt lost accepted progress' }
if (-not [IO.File]::ReadAllText((Join-Path $first[0].FullName 'healthy-initial.stdout.txt')).Contains('source-only A') -or
    -not [IO.File]::ReadAllText((Join-Path $second[0].FullName 'healthy-initial.stdout.txt')).Contains('source-only B')) {
    throw 'Attempt case logs were mixed'
}
$script:marker = 'C'
$script:failOnCase = ''
& $program | Out-Null
$all = @(Get-ChildItem -LiteralPath $root -Directory)
if ($all.Count -ne 3) { throw 'Healthy follow-up did not create a fresh attempt' }
$third = @($all | Where-Object { $_.FullName -ne $first[0].FullName -and $_.FullName -ne $second[0].FullName })
$followup = Get-Content -LiteralPath (Join-Path $third[0].FullName 'results.json') -Raw | ConvertFrom-Json
if ($followup.status -ne 'NATIVE_TEST_ONLY_CONTROLS_PASS' -or $followup.results.Count -ne 9) { throw 'Healthy follow-up failed' }
$script:marker = 'D'
$script:failOnCase = 'fault-trailing-byte'
$publicationError = $null
try { & $program | Out-Null } catch { $publicationError = $_.Exception.Message }
if (-not $publicationError -or -not $publicationError.Contains('Terminal FAILED receipt could not be published:') -or
    -not $publicationError.Contains('original failure: INJECTED_CASE_FAILURE:fault-trailing-byte')) {
    throw "Receipt publication failure lost its own or the original error: $publicationError"
}
$fourth = @(Get-ChildItem -LiteralPath $root -Directory | Where-Object {
    $_.FullName -notin @($first[0].FullName, $second[0].FullName, $third[0].FullName)
})
if ($fourth.Count -ne 1) { throw 'Receipt-failure attempt directory missing' }
if ([IO.File]::ReadAllText((Join-Path $fourth[0].FullName 'failure.json')) -ne 'existing receipt must not be replaced' -or
    (Test-Path -LiteralPath (Join-Path $fourth[0].FullName 'results.json')) -or
    @(Get-ChildItem -LiteralPath $fourth[0].FullName -Filter '*.partial-*').Count -ne 0) {
    throw 'Receipt publication failure overwrote evidence, published success or left a partial file'
}
Write-Output "GH1189 SOURCE-ONLY TERMINAL STATUS PASS: healthy, failed, healthy, receipt-write failure; evidence=$EvidenceDirectory"
