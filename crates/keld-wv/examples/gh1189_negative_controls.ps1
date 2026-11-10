# GH-1189 source-reviewed, test-only one-fault native controls.
# This runner does NOT compile, modify a source file, or touch another process.
# -Run requires a new exclusive RAMANI native slot and exact-head independent review.
[CmdletBinding()]
param(
    [switch]$Run,
    [string]$ReviewedCommit,
    [string]$FixtureSha256,
    [string]$ObserverSha256,
    [string]$IssueClaimUrl,
    [string]$NativeSlotUrl
)
$ErrorActionPreference = 'Stop'
$cases = @(
    @{ Name = 'trailing-byte'; Reason = 'extra phase bytes' },
    @{ Name = 'owned-decoy'; Reason = 'unexpected UI-thread SHOW; possible decoy' },
    @{ Name = 'suppress-dismissal'; Reason = 'cleanup observation timeout' },
    @{ Name = 'hold-parent'; Reason = 'parent destruction fence timeout' }
)
if (-not $Run) {
    Write-Output 'PREVIEW ONLY: baseline, four one-fault semantic RED cases, fresh baseline after each.'
    foreach ($case in $cases) { Write-Output ("PREVIEW ONLY: {0} => {1}" -f $case.Name, $case.Reason) }
    Write-Output 'No compiler, Windows GUI or executable was launched.'
    return
}
if ($env:OS -ne 'Windows_NT') { throw 'Windows native slot required' }
if ($IssueClaimUrl -notmatch '^https://github\.com/gyldlab/keld/issues/1189#issuecomment-[0-9]+$' -or
    $NativeSlotUrl -notmatch '^https://github\.com/gyldlab/keld/issues/1188#issuecomment-[0-9]+$') {
    throw 'Provide the fresh GH1189 owner claim and GH1188 exclusive native-lane comment URLs'
}
# Caller MUST independently re-fetch both claims and check expiration/conflicts.
# URLs supplied to this source-only script do not themselves grant a device slot.
foreach ($hash in @($FixtureSha256, $ObserverSha256)) {
    if ($hash -notmatch '^[A-Fa-f0-9]{64}$') { throw 'Both reviewed executable SHA256 values are required' }
}
if ($ReviewedCommit -notmatch '^[a-f0-9]{40}$') { throw 'Exact reviewed commit is required' }
$here = Split-Path -Parent $PSCommandPath
$repo = (Resolve-Path (Join-Path $here '..\..\..')).Path
$head = (& git -C $repo rev-parse HEAD | Select-Object -First 1).Trim()
if ($LASTEXITCODE -ne 0 -or $head -ne $ReviewedCommit) { throw 'Reviewed commit mismatch' }
$dirty = @(& git -C $repo status --porcelain=v1)
if ($LASTEXITCODE -ne 0 -or $dirty.Count -ne 0) { throw 'Native controls require a clean reviewed worktree' }
$metadata = & cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve isolated Cargo target' }
$binDir = Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc\debug\examples'
$fixture = Join-Path $binDir 'gh1189_taskdialog_fixture.exe'
$observer = Join-Path $binDir 'gh1189_winevent_observer.exe'
foreach ($item in @(@{Path=$fixture;Hash=$FixtureSha256}, @{Path=$observer;Hash=$ObserverSha256})) {
    if (-not (Test-Path -LiteralPath $item.Path -PathType Leaf)) { throw "Missing reviewed EXE: $($item.Path)" }
    if ((Get-FileHash -LiteralPath $item.Path -Algorithm SHA256).Hash -ne $item.Hash) {
        throw "Unreviewed native EXE: $($item.Path)"
    }
}
$evidence = Join-Path $metadata.target_directory 'gh1189\negative-controls'
New-Item -ItemType Directory -Path $evidence -Force | Out-Null
$results = New-Object System.Collections.ArrayList
$seen = New-Object 'System.Collections.Generic.HashSet[string]'

function Invoke-ObservedCase([string]$label, [string]$fault, [string]$expected) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $observer
    $psi.Arguments = '"' + $fixture + '"'
    if ($fault) { $psi.Arguments += ' --gh1189-fault ' + $fault }
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $psi
    $start = [Diagnostics.Stopwatch]::StartNew()
    [void]$process.Start()
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(30000)) {
        throw "BLOCKED: owned observer PID $($process.Id) exceeded 30 s; retain custody and investigate, no success credit"
    }
    $start.Stop()
    $record = [pscustomobject]@{
        Case = $label
        Exit = $process.ExitCode
        ElapsedMs = $start.ElapsedMilliseconds
        Stdout = $stdoutTask.GetAwaiter().GetResult()
        Stderr = $stderrTask.GetAwaiter().GetResult()
    }
    $process.Dispose()
    [IO.File]::WriteAllText((Join-Path $evidence ($label + '.stdout.txt')), $record.Stdout)
    [IO.File]::WriteAllText((Join-Path $evidence ($label + '.stderr.txt')), $record.Stderr)
    $identity = [regex]::Match($record.Stderr, 'GH1189 fixture pid=(\d+) creation=(\d+)')
    if (-not $identity.Success) { throw "Missing child identity receipt: $label" }
    $key = $identity.Groups[1].Value + '/' + $identity.Groups[2].Value
    if (-not $seen.Add($key)) { throw "Reused child identity: $label" }
    if ($expected) {
        if ($record.Exit -ne 1 -or -not $record.Stderr.Contains('GH1189 observer FAIL: ' + $expected)) {
            throw "Negative control had no intended semantic rejection: $label (exit $($record.Exit))"
        }
    } else {
        if ($record.Exit -ne 0 -or -not $record.Stdout.Contains('GH1189 observer candidate observed show, destroy, zero census, child exit')) {
            throw "Healthy native witness failed: $label (exit $($record.Exit))"
        }
    }
    # The observer's Child handle owns rescue. Never repair or kill a foreign PID.
    $survivors = @(Get-CimInstance Win32_Process -Filter "Name = 'gh1189_taskdialog_fixture.exe'")
    if ($survivors.Count -ne 0) { throw "Child still live after $label; no recovery/acceptance credit" }
    [void]$results.Add([pscustomobject]@{Case=$label;Exit=$record.Exit;ElapsedMs=$record.ElapsedMs;Identity=$key})
    Write-Output ("GH1189 CONTROL {0} expected={1} exit={2} identity={3}" -f $label, $(if($expected){'RED'}else{'PASS'}), $record.Exit, $key)
}

Invoke-ObservedCase 'healthy-initial' '' ''
foreach ($case in $cases) {
    Invoke-ObservedCase ('fault-' + $case.Name) $case.Name $case.Reason
    Invoke-ObservedCase ('healthy-after-' + $case.Name) '' ''
}
foreach ($item in @(@{Path=$fixture;Hash=$FixtureSha256}, @{Path=$observer;Hash=$ObserverSha256})) {
    if ((Get-FileHash -LiteralPath $item.Path -Algorithm SHA256).Hash -ne $item.Hash) { throw 'Binary changed during controls' }
}
$results | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $evidence 'results.json') -Encoding UTF8
Write-Output 'GH1189 CONTROL SUITE PASS: 4 independent semantic RED cases and 5 healthy children; native product acceptance remains separate.'
