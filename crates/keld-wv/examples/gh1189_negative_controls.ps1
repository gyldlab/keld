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
    [string]$NativeSlotUrl,
    [string]$BuildReceiptPath
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
$head = (& git -C $repo rev-parse HEAD).Trim()
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
# Lexical workspace/target equality does NOT rule out child-directory
# junctions into foreign Cargo caches or evidence directories. Walk every
# existing component, from the exact path through the volume root, before
# reading or writing a task-owned path. Absent future children are allowed;
# unreadable paths fail closed rather than silently becoming "absent".
function Assert-NoReparsePath([string]$path) {
    $cursor = [IO.Path]::GetFullPath($path)
    while ($true) {
        $entry = $null
        try {
            $entry = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
        } catch [System.Management.Automation.ItemNotFoundException] {
            # A new output directory or file may not exist yet.
        }
        if ($null -ne $entry -and
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Reparse-point path refused (not task-owned): $cursor"
        }
        $parentPath = [IO.Directory]::GetParent($cursor)
        if ($null -eq $parentPath) { break }
        $cursor = $parentPath.FullName
    }
}
Assert-NoReparsePath $actualTarget
$binDir = Join-Path $actualTarget 'x86_64-pc-windows-msvc\debug\examples'
$fixture = Join-Path $binDir 'gh1189_taskdialog_fixture.exe'
$observer = Join-Path $binDir 'gh1189_winevent_observer.exe'
foreach ($item in @(@{Path=$fixture;Hash=$FixtureSha256}, @{Path=$observer;Hash=$ObserverSha256})) {
    Assert-NoReparsePath $item.Path
    if (-not (Test-Path -LiteralPath $item.Path -PathType Leaf)) { throw "Missing reviewed EXE: $($item.Path)" }
    if ((Get-FileHash -LiteralPath $item.Path -Algorithm SHA256).Hash -ne $item.Hash) {
        throw "Unreviewed native EXE: $($item.Path)"
    }
}
# The compiler is independently attested. A caller-provided executable hash
# alone does not prove it came from the reviewed source/config/toolchain.
if ([string]::IsNullOrWhiteSpace($BuildReceiptPath)) {
    throw 'An exact reviewed build receipt is mandatory before native execution'
}
$receiptPath = [IO.Path]::GetFullPath($BuildReceiptPath)
$ownedBuildPrefix = [IO.Path]::GetFullPath((Join-Path $actualTarget 'gh1189\builds')).TrimEnd('\') + '\'
if (-not $receiptPath.StartsWith($ownedBuildPrefix, [StringComparison]::OrdinalIgnoreCase) -or
    [IO.Path]::GetFileName($receiptPath) -ne 'receipt.json') {
    throw 'Build receipt must be inside the task-owned GH1189 build directory'
}
Assert-NoReparsePath $receiptPath
if (-not (Test-Path -LiteralPath $receiptPath -PathType Leaf)) { throw 'Build receipt missing' }
$buildReceipt = Get-Content -LiteralPath $receiptPath -Raw -ErrorAction Stop | ConvertFrom-Json
$gitTree = (& git -C $repo rev-parse 'HEAD^{tree}').Trim()
if ($LASTEXITCODE -ne 0 -or $buildReceipt.status -ne 'BUILT_NOT_RUN_PROVENANCE' -or
    $buildReceipt.reviewed_commit -ne $ReviewedCommit -or
    $buildReceipt.reviewed_tree -ne $gitTree -or
    -not [string]::Equals([string]$buildReceipt.workspace_root, $expectedWorkspace, [StringComparison]::OrdinalIgnoreCase) -or
    -not [string]::Equals([string]$buildReceipt.target_directory, $actualTarget, [StringComparison]::OrdinalIgnoreCase) -or
    $buildReceipt.fixture_sha256 -ne $FixtureSha256 -or
    $buildReceipt.observer_sha256 -ne $ObserverSha256 -or
    $buildReceipt.no_external_cargo_config_or_flag_override -ne $true) {
    throw 'Build receipt does not attest this exact source, target and executables'
}
foreach ($item in @(
    @{Name='Cargo.toml';Hash=$buildReceipt.root_inputs.cargo_toml_sha256},
    @{Name='Cargo.lock';Hash=$buildReceipt.root_inputs.cargo_lock_sha256},
    @{Name='rust-toolchain.toml';Hash=$buildReceipt.root_inputs.rust_toolchain_sha256}
)) {
    $path = Join-Path $repo $item.Name
    Assert-NoReparsePath $path
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $item.Hash) {
        throw "Build root input changed: $($item.Name)"
    }
}
if (@($buildReceipt.pinned_inputs).Count -ne 6) { throw 'Build receipt lacks complete pinned source set' }
foreach ($item in @($buildReceipt.pinned_inputs)) {
    if ([string]::IsNullOrWhiteSpace([string]$item.path) -or
        [IO.Path]::IsPathRooted([string]$item.path) -or [string]$item.path -match '(^|/)\.\.(/|$)') {
        throw 'Invalid build receipt source path'
    }
    $source = Join-Path $repo ([string]$item.path).Replace('/', '\')
    Assert-NoReparsePath $source
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $item.sha256) {
        throw "Build receipt source digest changed: $($item.path)"
    }
}
if ($buildReceipt.build_script_sha256 -ne (Get-FileHash -LiteralPath (Join-Path $here 'gh1189_build.ps1') -Algorithm SHA256).Hash -or
    $buildReceipt.cargo_version -ne ((& cargo --version) -join [Environment]::NewLine) -or
    $buildReceipt.rustc_version -ne ((& rustc -vV) -join [Environment]::NewLine)) {
    throw 'Build provenance script/toolchain mismatch'
}
$manifestProbe = Join-Path (Split-Path -Parent $receiptPath) 'fixture-manifest-resource1.xml'
Assert-NoReparsePath $manifestProbe
if ((Get-FileHash -LiteralPath $manifestProbe -Algorithm SHA256).Hash -ne $buildReceipt.manifest_resource1_sha256) {
    throw 'Embedded manifest extraction digest does not match the pinned build receipt'
}
$buildReceiptSha256 = (Get-FileHash -LiteralPath $receiptPath -Algorithm SHA256).Hash

# Never reuse a prior PASS receipt. Every invocation gets a new exclusive
# attempt directory and an immutable start manifest in this worktree's target.
$attemptId = [guid]::NewGuid().ToString('N')
$evidenceRoot = Join-Path $actualTarget 'gh1189\negative-controls'
Assert-NoReparsePath $evidenceRoot
New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null
Assert-NoReparsePath $evidenceRoot
$evidence = Join-Path $evidenceRoot $attemptId
Assert-NoReparsePath $evidence
if (Test-Path -LiteralPath $evidence) { throw 'Attempt identifier collision; refused' }
New-Item -ItemType Directory -Path $evidence -ErrorAction Stop | Out-Null
Assert-NoReparsePath $evidence

function Write-DurableJsonOnce([string]$destination, $value) {
    # Stage with exclusive access, fsync, then atomically rename on this volume.
    # The final path is never exposed as a partially flushed JSON document.
    $staged = $destination + '.partial-' + [guid]::NewGuid().ToString('N')
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes((ConvertTo-Json -InputObject $value -Depth 8))
    try {
        Assert-NoReparsePath $destination
        Assert-NoReparsePath $staged
        $stream = [IO.File]::Open($staged, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try {
            $stream.Write($bytes, 0, $bytes.Length)
            $stream.Flush($true)
        } finally {
            $stream.Dispose()
        }
        Assert-NoReparsePath $staged
        Assert-NoReparsePath $destination
        [IO.File]::Move($staged, $destination)
    } catch {
        # Delete only this call's task-owned staging file; never touch the
        # previously published final record or a foreign attempt.
        try { if ([IO.File]::Exists($staged)) { [IO.File]::Delete($staged) } } catch { }
        throw
    }
}
function Write-CaseTextOnce([string]$destination, [string]$value) {
    # A fresh attempt must never overwrite an existing file or follow a link
    # planted at the log pathname.
    Assert-NoReparsePath $destination
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($value)
    $stream = [IO.File]::Open($destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
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
    build_receipt_path = $receiptPath
    build_receipt_sha256 = $buildReceiptSha256
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
    # Revalidate the exact executable paths and digests just before launch,
    # not only at suite start. Never follow a nested target junction.
    foreach ($item in @(@{Path=$fixture;Hash=$FixtureSha256}, @{Path=$observer;Hash=$ObserverSha256})) {
        Assert-NoReparsePath $item.Path
        if ((Get-FileHash -LiteralPath $item.Path -Algorithm SHA256).Hash -ne $item.Hash) {
            throw "Executable changed before case: $label"
        }
    }
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
        # An exception after OS process creation may leave an owned handle.
        # Check BOTH the exact wrapper's PID and handle before disposal.
        $handleOwned = $false
        try { $handleOwned = ($process.Id -gt 0) } catch { }
        if (-not $handleOwned) {
            try {
                $candidateHandle = $process.Handle
                if ($null -ne $candidateHandle) {
                    $handleOwned = ([IntPtr]$candidateHandle -ne [IntPtr]::Zero)
                }
            } catch { }
        }
        if ($handleOwned) {
            Hold-ExactObserverOnFailure $process $label $psi "observer start error: $startError" $null $spawnAcknowledgedUtc
        }
        # Start failed without any accessible OS handle. Never infer a PASS;
        # do not enter an endless wait on a never-started Process wrapper.
        try { $process.Dispose() } catch { }
        throw "BLOCKED: observer start failed without an owned process handle: $startError"
    }
    try {
        # All operations after a successful Start belong to ONE lifetime
        # fence. Any thrown exception is BLOCKED and reaps the exact object.
        $observerStartedUtc = $process.StartTime.ToUniversalTime().ToString('o')
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) { throw 'observer exceeded 30000 ms' }
        # Process exit does NOT guarantee inherited stdout/stderr pipe handles
        # are closed: a descendant may keep them open indefinitely. Bound both
        # async readers before GetResult; expiry enters exact-handle BLOCKED.
        if (-not $stdoutTask.Wait(10000)) { throw 'observer stdout pipe remained open after exit' }
        if (-not $stderrTask.Wait(10000)) { throw 'observer stderr pipe remained open after exit' }
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
    Write-CaseTextOnce (Join-Path $evidence ($label + '.stdout.txt')) $record.Stdout
    Write-CaseTextOnce (Join-Path $evidence ($label + '.stderr.txt')) $record.Stderr
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
    Assert-NoReparsePath $item.Path
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
    build_receipt_path = $receiptPath
    build_receipt_sha256 = $buildReceiptSha256
    controls_expected = 4
    fresh_healthy_children_expected = 5
    results = @($results.ToArray())
    production_drawio_acceptance = $false
}
Write-DurableJsonOnce (Join-Path $evidence 'results.json') $completion
Write-Output "GH1189 CONTROL SUITE PASS: 4 independent semantic RED cases and 5 healthy children; attempt=$attemptId; native product acceptance remains separate."
