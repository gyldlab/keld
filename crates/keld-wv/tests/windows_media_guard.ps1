param(
    [Parameter(Mandatory = $true)][string]$BinaryPath,
    [Parameter(Mandatory = $true)][string]$EvidenceDirectory
)

function Write-MediaProcessLogs {
    param(
        [string]$StdoutPath,
        [string]$StderrPath,
        [IO.TextWriter]$Writer = [Console]::Out
    )
    $Writer.WriteLine('KELD_MEDIA_STDOUT_BEGIN')
    $Writer.WriteLine([IO.File]::ReadAllText($StdoutPath))
    $Writer.WriteLine('KELD_MEDIA_STDOUT_END')
    $Writer.WriteLine('KELD_MEDIA_STDERR_BEGIN')
    $Writer.WriteLine([IO.File]::ReadAllText($StderrPath))
    $Writer.WriteLine('KELD_MEDIA_STDERR_END')
}

function Invoke-MediaProcess {
    param(
        [Diagnostics.ProcessStartInfo]$Start,
        [string]$StdoutPath,
        [string]$StderrPath,
        [int]$DeadlineMilliseconds,
        [IO.TextWriter]$Writer = [Console]::Out
    )
    $process = [Diagnostics.Process]::Start($Start)
    [int]$childPid = $process.Id
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $outerTimedOut = $false
    try {
        if (-not $process.WaitForExit($DeadlineMilliseconds)) {
            $outerTimedOut = $true
            try {
                $process.Kill()
            } catch [InvalidOperationException] {
                if (-not $process.HasExited) { throw }
            }
            $process.WaitForExit()
        }
        # Complete redirected-stream draining before reading ExitCode.
        $process.WaitForExit()
        [int]$exitCode = $process.ExitCode
        [IO.File]::WriteAllText($StdoutPath, $stdout.Result)
        [IO.File]::WriteAllText($StderrPath, $stderr.Result)
    } finally {
        $process.Dispose()
    }
    if ($outerTimedOut -or $exitCode -ne 0) {
        Write-MediaProcessLogs -StdoutPath $StdoutPath -StderrPath $StderrPath -Writer $Writer
    }
    [pscustomobject]@{
        child_pid = $childPid
        exit_code = $exitCode
        outer_timed_out = $outerTimedOut
        stdout_text = [IO.File]::ReadAllText($StdoutPath)
        stderr_text = [IO.File]::ReadAllText($StderrPath)
    }
}

function Invoke-MediaWatchdogProbe {
    param([string]$Binary, [string]$EvidenceRoot)
    $stdoutPath = Join-Path $EvidenceRoot 'watchdog-probe.log'
    $stderrPath = Join-Path $EvidenceRoot 'watchdog-probe.stderr.log'
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Binary
    $start.Arguments = 'webview2::media_acceptance::tests::windows_media_acceptance_subprocess --ignored --exact --nocapture --test-threads=1'
    $start.EnvironmentVariables['KELD_MEDIA_WATCHDOG_PROBE'] = 'work'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $writer = [IO.StringWriter]::new()
    $execution = Invoke-MediaProcess -Start $start -StdoutPath $stdoutPath -StderrPath $stderrPath -DeadlineMilliseconds 5000 -Writer $writer
    $consoleText = $writer.ToString()
    $writer.Dispose()
    [Console]::Out.Write($consoleText)
    $expectedPhase = 'KELD_MEDIA_PHASE watchdog-probe-work-block'
    if ($execution.outer_timed_out -or $execution.exit_code -ne 124 -or
        -not $execution.stdout_text.Contains($expectedPhase) -or
        -not $execution.stderr_text.Contains('KELD_MEDIA_TIMEOUT: fixture exceeded 0.1 seconds') -or
        -not $consoleText.Contains('KELD_MEDIA_STDOUT_BEGIN') -or
        -not $consoleText.Contains('KELD_MEDIA_STDOUT_END') -or
        -not $consoleText.Contains('KELD_MEDIA_STDERR_BEGIN') -or
        -not $consoleText.Contains('KELD_MEDIA_STDERR_END') -or
        -not $consoleText.Contains($expectedPhase) -or
        -not $consoleText.Contains('KELD_MEDIA_TIMEOUT: fixture exceeded 0.1 seconds')) {
        throw "Watchdog probe failed: outerTimedOut=$($execution.outer_timed_out) exit=$($execution.exit_code)"
    }
    [pscustomobject]@{
        mode = 'work'
        exit_code = $execution.exit_code
        outer_timed_out = $execution.outer_timed_out
        stdout_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash.ToLowerInvariant()
        stderr_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash.ToLowerInvariant()
    }
}

function Invoke-MediaOuterDeadlineProbe {
    param([string]$EvidenceRoot)
    $stdoutPath = Join-Path $EvidenceRoot 'outer-deadline-probe.log'
    $stderrPath = Join-Path $EvidenceRoot 'outer-deadline-probe.stderr.log'
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
    $start.Arguments = '-NoProfile -Command "[Console]::Out.WriteLine(''KELD_MEDIA_PHASE outer-probe-block''); [Console]::Out.Flush(); $gate = [Threading.ManualResetEventSlim]::new($false); $gate.Wait()"'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $writer = [IO.StringWriter]::new()
    $execution = Invoke-MediaProcess -Start $start -StdoutPath $stdoutPath -StderrPath $stderrPath -DeadlineMilliseconds 5000 -Writer $writer
    $consoleText = $writer.ToString()
    $writer.Dispose()
    [Console]::Out.Write($consoleText)
    if (-not $execution.outer_timed_out -or $execution.exit_code -eq 0 -or
        -not $consoleText.Contains('KELD_MEDIA_PHASE outer-probe-block') -or
        -not $consoleText.Contains('KELD_MEDIA_STDOUT_BEGIN') -or
        -not $consoleText.Contains('KELD_MEDIA_STDERR_END')) {
        throw "Outer deadline probe failed: outerTimedOut=$($execution.outer_timed_out) exit=$($execution.exit_code)"
    }
    [pscustomobject]@{
        exit_code = $execution.exit_code
        outer_timed_out = $execution.outer_timed_out
        stdout_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash.ToLowerInvariant()
        stderr_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash.ToLowerInvariant()
    }
}

$ErrorActionPreference = 'Stop'
$sourceBinary = (Resolve-Path -LiteralPath $BinaryPath).Path
$evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
if (Test-Path -LiteralPath $evidenceRoot) {
    throw 'Evidence directory must be new; previous results must not be overwritten.'
}
[void](New-Item -ItemType Directory -Path $evidenceRoot)
$binary = Join-Path $evidenceRoot 'keld_wv_media_test.exe'
Copy-Item -LiteralPath $sourceBinary -Destination $binary
$sourceBinaryHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceBinary).Hash.ToLowerInvariant()
$binaryHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash.ToLowerInvariant()
if ($binaryHash -ne $sourceBinaryHash) {
    throw "Fixture executable copy mismatch: source=$sourceBinaryHash evidence=$binaryHash"
}
$watchdogProbe = Invoke-MediaWatchdogProbe -Binary $binary -EvidenceRoot $evidenceRoot
$outerDeadlineProbe = Invoke-MediaOuterDeadlineProbe -EvidenceRoot $evidenceRoot
$results = @()
$seenNonces = @{}
$seenRows = @{}
$outerDeadlineMilliseconds = 90000
foreach ($kind in @('camera', 'microphone')) {
    foreach ($mode in @('guarded', 'removed-guard', 'adapter-bypass', 'force-allow')) {
        $manifestCases = if ($mode -eq 'guarded') { @('empty', 'app-grants') } else { @('app-grants') }
        foreach ($manifestCase in $manifestCases) {
            $rowKey = "$kind/$mode/$manifestCase"
            if ($seenRows.ContainsKey($rowKey)) { throw "Duplicate fixture row: $rowKey" }
            $seenRows[$rowKey] = $true
            $log = Join-Path $evidenceRoot "$kind-$mode-$manifestCase.log"
            $stderrLog = Join-Path $evidenceRoot "$kind-$mode-$manifestCase.stderr.log"
            $start = [Diagnostics.ProcessStartInfo]::new()
            $start.FileName = $binary
            $start.Arguments = 'webview2::media_acceptance::tests::windows_media_acceptance_subprocess --ignored --exact --nocapture --test-threads=1'
            $start.EnvironmentVariables['KELD_MEDIA_KIND'] = $kind
            $start.EnvironmentVariables['KELD_MEDIA_MODE'] = $mode
            $start.EnvironmentVariables['KELD_MEDIA_MANIFEST'] = $manifestCase
            $start.EnvironmentVariables['KELD_MEDIA_PARENT_DEADLINE_MS'] = "$outerDeadlineMilliseconds"
            [void]$start.EnvironmentVariables.Remove('KELD_MEDIA_WATCHDOG_PROBE')
            $start.UseShellExecute = $false
            $start.CreateNoWindow = $true
            $start.RedirectStandardOutput = $true
            $start.RedirectStandardError = $true
            $execution = Invoke-MediaProcess -Start $start -StdoutPath $log -StderrPath $stderrLog -DeadlineMilliseconds $outerDeadlineMilliseconds
            [int]$childPid = $execution.child_pid
            $output = @(Get-Content -LiteralPath $log)
            if ($execution.outer_timed_out) {
                throw "Fixture exceeded external deadline: $log; profile may require cleanup."
            }
            if ($execution.exit_code -ne 0) {
                throw "Media fixture failed ($rowKey): exit $($execution.exit_code); see $log"
            }
            $receipts = @($output | Where-Object { "$_" -like 'KELD_MEDIA_RESULT *' })
            if ($receipts.Count -ne 1) { throw "Expected exactly one result receipt: $log" }
            $receipt = "$($receipts[0])"
            $pattern = '^KELD_MEDIA_RESULT kind=(?<kind>camera|microphone) mode=(?<mode>guarded|removed-guard|adapter-bypass|force-allow) nonce=(?<nonce>[0-9]+) runtime=(?<runtime>[^ ]+) host_pid=(?<host>[0-9]+) browser_pid=(?<browser>[0-9]+) view_id=(?<view>[0-9]+) tid=(?<tid>[0-9]+) manifest_fnv1a64=(?<manifest_hash>[0-9a-f]{16}) adapter_principal=(?<adapter_principal>[^ ]+) adapter_capability=(?<adapter_capability>[^ ]+) adapter_decision=(?<adapter_decision>[^ ]+) adapter_tid=(?<adapter_tid>[0-9]+) permission_kind=(?<permission_kind>-?[0-9]+) before=(?<before>-?[0-9]+) requested=(?<requested>-?[0-9]+) after=(?<after>-?[0-9]+) origin_uri=(?<origin_uri>[^ ]+) registration_identity=(?<registration_identity>[0-9a-f]+) sender_identity=(?<sender_identity>[0-9a-f]+) outcome=(?<outcome>[^ ]+) manifest=(?<manifest>true|false) adapter=(?<adapter>true|false) effect=(?<effect>true|false) origin=(?<origin>true|false) js=(?<js>true|false) control=(?<control>true|false) accepted=(?<accepted>true|false) case_ok=(?<case_ok>true|false)$'
            if ($receipt -notmatch $pattern) { throw "Malformed result receipt: $log" }
            $parsed = [pscustomobject]@{
                kind = $Matches.kind; mode = $Matches.mode; nonce = $Matches.nonce
                runtime = $Matches.runtime; host = $Matches.host; browser = $Matches.browser
                view = $Matches.view; tid = $Matches.tid; manifest_hash = $Matches.manifest_hash
                adapter_principal = $Matches.adapter_principal
                adapter_capability = $Matches.adapter_capability
                adapter_decision = $Matches.adapter_decision; adapter_tid = $Matches.adapter_tid
                permission_kind = $Matches.permission_kind; before = $Matches.before
                requested = $Matches.requested; after = $Matches.after; origin_uri = $Matches.origin_uri
                registration_identity = $Matches.registration_identity
                sender_identity = $Matches.sender_identity; outcome = $Matches.outcome
                manifest = $Matches.manifest; adapter = $Matches.adapter; effect = $Matches.effect
                origin = $Matches.origin; js = $Matches.js; control = $Matches.control
                accepted = $Matches.accepted; case_ok = $Matches.case_ok
            }
            if ($parsed.kind -ne $kind -or $parsed.mode -ne $mode) {
                throw "Receipt row identity mismatch: $rowKey"
            }
            if ($seenNonces.ContainsKey($parsed.nonce)) { throw "Reused fixture nonce: $($parsed.nonce)" }
            $seenNonces[$parsed.nonce] = $true
            $profileReceipts = @($output | Where-Object { "$_" -like '*KELD_MEDIA_PROFILE *' })
            if ($profileReceipts.Count -ne 1) { throw "Expected exactly one profile receipt: $log" }
            $profilePath = [IO.Path]::GetFullPath(("$($profileReceipts[0])" -split 'KELD_MEDIA_PROFILE ', 2)[1])
            $expectedProfileLeaf = "keld-media-$childPid-$($parsed.nonce)"
            if ((Split-Path -Leaf $profilePath) -ne $expectedProfileLeaf) {
                throw "Profile identity mismatch: expected=$expectedProfileLeaf actual=$profilePath"
            }
            if (Test-Path -LiteralPath $profilePath) {
                throw "Fixture profile survived successful process teardown: $profilePath"
            }
            $expectedHash = if ($manifestCase -eq 'empty') { 'e117311975d9f419' } else { '1fb6f771494b3631' }
            $expectedView = if ($kind -eq 'camera') { 2 } else { 3 }
            $expectedPermissionKind = if ($kind -eq 'camera') { 2 } else { 1 }
            $expectedRequested = if ($mode -eq 'force-allow') { 1 } else { 2 }
            $expectedEffect = if ($mode -eq 'force-allow') { 'false' } else { 'true' }
            $expectedJs = 'true'
            $expectedAdapter = if ($mode -eq 'guarded') { 'true' } else { 'false' }
            $expectedManifest = $expectedAdapter
            $expectedControl = if ($mode -eq 'guarded') { 'false' } else { 'true' }
            $expectedAccepted = $expectedAdapter
            $expectedPrincipal = if ($mode -eq 'guarded') { "webview:$expectedView`:0" } else { 'none' }
            $expectedCapability = if ($mode -eq 'guarded') { "web.$kind" } else { 'none' }
            $expectedDecision = if ($mode -eq 'guarded') { 'KELD-GUARD006' } else { 'none' }
            $expectedAdapterTid = if ($mode -eq 'guarded') { [int]$parsed.tid } else { 0 }
            if ($parsed.manifest_hash -ne $expectedHash -or
                [int]$parsed.view -ne $expectedView -or
                [int]$parsed.permission_kind -ne $expectedPermissionKind -or
                [int]$parsed.before -ne 0 -or
                [int]$parsed.requested -ne $expectedRequested -or
                [int]$parsed.after -ne $expectedRequested -or
                $parsed.adapter_principal -ne $expectedPrincipal -or
                $parsed.adapter_capability -ne $expectedCapability -or
                $parsed.adapter_decision -ne $expectedDecision -or
                [int]$parsed.adapter_tid -ne $expectedAdapterTid -or
                $parsed.manifest -ne $expectedManifest -or
                $parsed.adapter -ne $expectedAdapter -or
                $parsed.effect -ne $expectedEffect -or
                $parsed.origin -ne 'true' -or
                $parsed.js -ne $expectedJs -or
                $parsed.control -ne $expectedControl -or
                $parsed.accepted -ne $expectedAccepted -or
                $parsed.case_ok -ne 'true') {
                throw "Unexpected exact oracle fields: $rowKey; $receipt"
            }
            if ([int]$parsed.host -ne $childPid -or [int]$parsed.browser -le 0 -or [int]$parsed.tid -le 0) {
                throw "Invalid process/thread identity: $rowKey; $receipt"
            }
            if ($parsed.registration_identity -eq '0' -or
                $parsed.sender_identity -ne $parsed.registration_identity) {
                throw "Permission callback sender did not match the registered view: $rowKey; $receipt"
            }
            if ($parsed.runtime -notmatch '^[0-9]+(\.[0-9]+)+$' -or
                $parsed.origin_uri -notmatch '^http://127\.0\.0\.1:[0-9]+/$') {
                throw "Invalid runtime/origin identity: $rowKey; $receipt"
            }
            if ($mode -eq 'force-allow') {
                $trackKind = if ($kind -eq 'camera') { 'video' } else { 'audio' }
                $escapedNonce = [Regex]::Escape($parsed.nonce)
                if ($parsed.outcome -notmatch "^$escapedNonce`:resolved`:$trackKind`:[1-9][0-9]*`:true$") {
                    throw "Allow row lacked a stopped requested-kind live track: $rowKey; $receipt"
                }
            } elseif ($parsed.outcome -ne "$($parsed.nonce):true:NotAllowedError") {
                throw "Deny row returned the wrong JavaScript outcome: $rowKey; $receipt"
            }
            $results += [pscustomobject]@{
                kind = $kind; mode = $mode; manifest_case = $manifestCase
                nonce = $parsed.nonce; view_id = [int]$parsed.view
                host_pid = [int]$parsed.host; browser_pid = [int]$parsed.browser
                permission_kind = [int]$parsed.permission_kind
                initial_state = [int]$parsed.before; requested_state = [int]$parsed.requested
                returned_state = [int]$parsed.after; origin_uri = $parsed.origin_uri
                manifest_fnv1a64 = $parsed.manifest_hash
                adapter_principal = $parsed.adapter_principal
                adapter_capability = $parsed.adapter_capability
                adapter_decision = $parsed.adapter_decision; adapter_tid = [int]$parsed.adapter_tid
                registration_identity = $parsed.registration_identity
                sender_identity = $parsed.sender_identity; outcome = $parsed.outcome
                profile_path = $profilePath; profile_removed = $true
                exit_code = $execution.exit_code; receipt = $receipt
                log_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $log).Hash.ToLowerInvariant()
                stderr_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrLog).Hash.ToLowerInvariant()
            }
            Write-Output $receipt
        }
    }
}
if ($results.Count -ne 10 -or $seenRows.Count -ne 10 -or $seenNonces.Count -ne 10) {
    throw "Expected exact ten-row matrix; rows=$($results.Count) keys=$($seenRows.Count) nonces=$($seenNonces.Count)"
}
$finalBinaryHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash.ToLowerInvariant()
if ($finalBinaryHash -ne $binaryHash) {
    throw "Fixture executable changed during the matrix: initial=$binaryHash final=$finalBinaryHash"
}
$record = [ordered]@{
    schema = 'keld.windows-media-fixture/v1'
    source_executable = $sourceBinary
    executable = $binary
    executable_sha256 = $binaryHash
    system = [Environment]::OSVersion.VersionString
    device = [Environment]::MachineName
    capture_device = 'WebView2 synthetic development device'
    scope = 'raw WebView2 callback receipt: adapter input; removed-product-guard plus fixture-only completion after DEFAULT; adapter-bypass; same-callback explicit state; loopback origin; synthetic capture control; bounded teardown. No physical-device, saved-grant, snapshot, or revocation pass.'
    watchdog_probe = $watchdogProbe
    outer_deadline_probe = $outerDeadlineProbe
    rows = $results
}
[IO.File]::WriteAllText((Join-Path $evidenceRoot 'result.json'), ($record | ConvertTo-Json -Depth 5))
Write-Output "Windows media fixture: $($results.Count) rows passed; evidence $evidenceRoot"
