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
$workspaceManifest = Join-Path $repo 'Cargo.toml'
$metadata = & cargo metadata --manifest-path $workspaceManifest --locked --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve isolated Cargo target' }
$expectedWorkspace = [IO.Path]::GetFullPath($repo).TrimEnd('\')
$actualWorkspace = [IO.Path]::GetFullPath([string]$metadata.workspace_root).TrimEnd('\')
$expectedTarget = [IO.Path]::GetFullPath((Join-Path $repo 'target')).TrimEnd('\')
$actualTarget = [IO.Path]::GetFullPath([string]$metadata.target_directory).TrimEnd('\')
if (-not [string]::Equals($actualWorkspace, $expectedWorkspace, [StringComparison]::OrdinalIgnoreCase) -or
    -not [string]::Equals($actualTarget, $expectedTarget, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Cargo workspace/target is not this exact task-owned worktree; shared target refused'
}
$binDir = Join-Path $actualTarget 'x86_64-pc-windows-msvc\debug\examples'
$fixture = Join-Path $binDir 'gh1189_taskdialog_fixture.exe'
$observer = Join-Path $binDir 'gh1189_winevent_observer.exe'
foreach ($item in @(@{Path=$fixture;Hash=$FixtureSha256}, @{Path=$observer;Hash=$ObserverSha256})) {
    if (-not (Test-Path -LiteralPath $item.Path -PathType Leaf)) { throw "Missing reviewed EXE: $($item.Path)" }
    if ((Get-FileHash -LiteralPath $item.Path -Algorithm SHA256).Hash -ne $item.Hash) {
        throw "Unreviewed native EXE: $($item.Path)"
    }
}
# Never reuse a prior PASS receipt. Every invocation gets a new exclusive
# attempt directory and an immutable start manifest in this worktree's target.
$attemptId = [guid]::NewGuid().ToString('N')
$evidenceRoot = Join-Path $actualTarget 'gh1189\negative-controls'
New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
$evidence = Join-Path $evidenceRoot $attemptId
if (Test-Path -LiteralPath $evidence) { throw 'Attempt identifier collision; refused' }
New-Item -ItemType Directory -Path $evidence -ErrorAction Stop | Out-Null

function Write-DurableJsonOnce([string]$destination, $value) {
    # Stage with exclusive access, fsync, then atomically rename on this volume.
    # The final path is never exposed as a partially flushed JSON document.
    $staged = $destination + '.partial-' + [guid]::NewGuid().ToString('N')
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes((ConvertTo-Json -InputObject $value -Depth 8))
    try {
        $stream = [IO.File]::Open($staged, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try {
            $stream.Write($bytes, 0, $bytes.Length)
            $stream.Flush($true)
        } finally {
            $stream.Dispose()
        }
        [IO.File]::Move($staged, $destination)
    } catch {
        # Delete only this call's task-owned staging file; never touch the
        # previously published final record or a foreign attempt.
        try { if ([IO.File]::Exists($staged)) { [IO.File]::Delete($staged) } } catch { }
        throw
    }
}
$provenance = [ordered]@{
    attempt_id = $attemptId
    status = 'STARTED_NOT_ACCEPTED'
    created_utc = [DateTime]::UtcNow.ToString('o')
    reviewed_commit = $ReviewedCommit
    fixture_sha256 = $FixtureSha256.ToUpperInvariant()
    observer_sha256 = $ObserverSha256.ToUpperInvariant()
    fixture_exe = $fixture
    observer_exe = $observer
    issue_claim_url = $IssueClaimUrl
    native_slot_url = $NativeSlotUrl
    evidence_directory = $evidence
}
Write-DurableJsonOnce (Join-Path $evidence 'attempt.json') $provenance
$results = New-Object System.Collections.ArrayList
$seen = New-Object 'System.Collections.Generic.HashSet[string]'

function Hold-ExactObserverOnFailure($process, [string]$label, $psi, [string]$reason, [string]$startedUtc, [string]$spawnAcknowledgedUtc) {
    # The caller owns this exact Process object/handle. Never reopen by PID or
    # terminate by name. Every catchable receipt/wait error remains BLOCKED.
    $handoff = '(receipt unavailable)'
    $receiptError = $null
    $observerPid = $null
    $observerStartError = $null
    $observerHandle = $null
    try { $observerPid = $process.Id } catch { $observerStartError = "PID: $($_.Exception.Message)" }
    if (-not $startedUtc) {
        try { $startedUtc = $process.StartTime.ToUniversalTime().ToString('o') }
        catch { $observerStartError = "StartTime: $($_.Exception.Message)" }
    }
    try { $observerHandle = $process.Handle.ToInt64() }
    catch {
        $observerStartError = "$observerStartError; HANDLE: $($_.Exception.Message)"
    }
    try {
        $receiptDestination = Join-Path $evidence ("BLOCKED-" + $label + "-" + [guid]::NewGuid().ToString('N') + ".json")
        $receipt = [ordered]@{
            status = 'BLOCKED_OBSERVER_CUSTODY'
            attempt_id = $attemptId
            reason = $reason
            case = $label
            observer_pid = $observerPid
            observer_started_utc = $(if ($startedUtc) { $startedUtc } else { 'UNKNOWN' })
            observer_start_verified = (-not [string]::IsNullOrWhiteSpace($startedUtc))
            observer_identity_error = $observerStartError
            spawn_acknowledged_utc = $spawnAcknowledgedUtc
            observer_handle_in_runner = $observerHandle
            runner_pid = $PID
            observer_exe = $observer
            observer_sha256 = $ObserverSha256.ToUpperInvariant()
            fixture_exe = $fixture
            fixture_sha256 = $FixtureSha256.ToUpperInvariant()
            command_arguments = $psi.Arguments
            wait_deadline_ms = 30000
            reviewed_commit = $ReviewedCommit
            issue_claim_url = $IssueClaimUrl
            native_slot_url = $NativeSlotUrl
            operator_handoff = 'This suite is BLOCKED, not PASS. Keep the exact observer process owned by this runner; investigate its Child. Never kill by bare PID/name.'
        }
        Write-DurableJsonOnce $receiptDestination $receipt
        # Report a final receipt path ONLY after a completed, flushed, atomic rename.
        $handoff = $receiptDestination
    } catch { $receiptError = $_.Exception.Message }
    try {
        [Console]::Error.WriteLine("GH1189 BLOCKED case=$label pid=$observerPid reason=$reason handoff=$handoff receipt_error=$receiptError")
    } catch { } # Logging failure must not release the exact observer handle.

    $cooldown = $null
    try { $cooldown = [System.Threading.ManualResetEvent]::new($false) } catch { }
    try {
        while ($true) {
            try {
                # A WaitForExit error does not prove exit; a false result means
                # retain custody. Only the OS-signaled true result ends this loop.
                if ($process.WaitForExit(30000)) { break }
            } catch {
                try { [Console]::Error.WriteLine("GH1189 BLOCKED exact-observer WaitForExit error: $($_.Exception.Message)") }
                catch { }
                # Exception-only throttling avoids a hot retry; not an OS
                # cleanup oracle and never releases this Process object.
                if ($null -ne $cooldown) {
                    try { [void]$cooldown.WaitOne(250) } catch { }
                }
            }
        }
    } finally {
        if ($null -ne $cooldown) { try { $cooldown.Dispose() } catch { } }
    }
    $exitValue = '(unavailable)'
    try { $exitValue = $process.ExitCode } catch { }
    try { $process.Dispose() } catch { }
    throw "BLOCKED: $reason; exact observer exited/reaped with $exitValue; handoff=$handoff receipt_error=$receiptError (never acceptance)"
}

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
    $spawnAcknowledgedUtc = $null
    $observerStartedUtc = $null
    try {
        if (-not $process.Start()) { throw 'Process.Start returned false' }
        $spawnAcknowledgedUtc = [DateTime]::UtcNow.ToString('o')
    } catch {
        $startError = $_.Exception.Message
        # A partially launched Process may still have an owned OS handle.
        $possiblyStarted = $false
        try { $possiblyStarted = ($process.Id -gt 0) } catch { }
        if ($possiblyStarted) {
            Hold-ExactObserverOnFailure $process $label $psi "observer start error: $startError" $null $spawnAcknowledgedUtc
        }
        try { $process.Dispose() } catch { }
        throw "BLOCKED: observer could not establish a launched Process: $startError"
    }
    try {
        # All operations after a successful Start belong to ONE lifetime
        # fence. Any thrown exception is BLOCKED and reaps the exact object.
        $observerStartedUtc = $process.StartTime.ToUniversalTime().ToString('o')
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) { throw 'observer exceeded 30000 ms' }
        $start.Stop()
        $record = [pscustomobject]@{
            Case = $label
            Exit = $process.ExitCode
            ElapsedMs = $start.ElapsedMilliseconds
            Stdout = $stdoutTask.GetAwaiter().GetResult()
            Stderr = $stderrTask.GetAwaiter().GetResult()
        }
    } catch {
        $start.Stop()
        Hold-ExactObserverOnFailure $process $label $psi ("observer lifetime failure: " + $_.Exception.Message) $observerStartedUtc $spawnAcknowledgedUtc
    }
    # WaitForExit returned true and both redirected output streams have been
    # consumed. Any failure to dispose is still a non-acceptance error.
    try { $process.Dispose() } catch { throw "BLOCKED: exact observer handle disposal failed: $($_.Exception.Message)" }
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
# Commit the suite result ONLY after every expected semantic RED, every fresh
# healthy child and both final binary hashes have passed this exact invocation.
if ($results.Count -ne 9) { throw "Incomplete control suite: $($results.Count)/9; no PASS result written" }
$completion = [ordered]@{
    status = 'NATIVE_TEST_ONLY_CONTROLS_PASS'
    attempt_id = $attemptId
    completed_utc = [DateTime]::UtcNow.ToString('o')
    reviewed_commit = $ReviewedCommit
    fixture_sha256 = $FixtureSha256.ToUpperInvariant()
    observer_sha256 = $ObserverSha256.ToUpperInvariant()
    issue_claim_url = $IssueClaimUrl
    native_slot_url = $NativeSlotUrl
    controls_expected = 4
    fresh_healthy_children_expected = 5
    results = @($results.ToArray())
    production_drawio_acceptance = $false
}
Write-DurableJsonOnce (Join-Path $evidence 'results.json') $completion
Write-Output "GH1189 CONTROL SUITE PASS: 4 independent semantic RED cases and 5 healthy children; attempt=$attemptId; native product acceptance remains separate."
