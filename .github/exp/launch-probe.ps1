# GH-814 THROWAWAY EXPERIMENT. Lives only on agent/gh-814-exp-image-* refs; never merged.
# Runs the ignored launch probe N times on one VM as separate processes, with a
# process census during each launch and a hard outer kill. Kills only the probe's
# own process tree. Always exits 0: results are the KELD_EXP_* log lines.
param(
    [Parameter(Mandatory = $true)][string]$BinaryPath,
    [int]$Launches = 3,
    [int]$OuterKillMs = 150000
)

$ErrorActionPreference = 'Continue'
$Case = 'webview2::media_acceptance::tests::gh814_launch_timing_probe_subprocess'
$Image = if ($env:ImageVersion) { $env:ImageVersion } else { 'local' }
# The inner stage deadline is 120 s, so the last checkpoint precedes it.
$Checkpoints = @(5000, 14000, 30000, 60000, 115000)
$WatchPattern = '^(msedgewebview2|msedge|MicrosoftEdge.*|edgeupdate.*|setup|MsMpEng|MpDefenderCoreService|NisSrv|MpCmdRun|TiWorker|TrustedInstaller|mscorsvw|ngen.*|CompatTelRunner|SearchIndexer|wuauclt|usocoreworker|MoUsoCoreWorker|WerFault.*)$'

function Get-CpuSnapshot {
    $snapshot = @{}
    foreach ($process in Get-Process) {
        try {
            $snapshot[[int]$process.Id] = [pscustomobject]@{
                name = $process.ProcessName
                seconds = $process.TotalProcessorTime.TotalSeconds
            }
        } catch {
            $snapshot[[int]$process.Id] = [pscustomobject]@{ name = $process.ProcessName; seconds = 0.0 }
        }
    }
    $snapshot
}

function Get-WebViewPids {
    @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue | ForEach-Object { [int]$_.Id })
}

function Write-Census {
    param([int]$Launch, [string]$Label, [long]$ElapsedMs, [int]$ChildPid, [hashtable]$Previous)
    $cost = [Diagnostics.Stopwatch]::StartNew()
    $tag = "launch=$Launch label=$Label t_ms=$ElapsedMs"
    $current = Get-CpuSnapshot
    $deltas = foreach ($id in $current.Keys) {
        $before = if ($Previous.ContainsKey($id)) { $Previous[$id].seconds } else { 0.0 }
        [pscustomobject]@{
            id = $id
            name = $current[$id].name
            delta = [math]::Round($current[$id].seconds - $before, 2)
            total = [math]::Round($current[$id].seconds, 2)
        }
    }
    $deltas | Sort-Object delta -Descending | Select-Object -First 8 | Where-Object { $_.delta -ge 0.1 } |
        ForEach-Object { "KELD_EXP_CPU $tag name=$($_.name) pid=$($_.id) delta_s=$($_.delta) total_s=$($_.total)" }
    $deltas | Where-Object { $_.name -match $WatchPattern -and $_.name -ne 'msedgewebview2' } |
        Sort-Object name, id |
        ForEach-Object { "KELD_EXP_WATCH $tag name=$($_.name) pid=$($_.id) delta_s=$($_.delta) total_s=$($_.total)" }
    $all = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $tree = @{ $ChildPid = $true }
    # Each pass extends the tree one parent link; four covers probe, browser, utility, child.
    foreach ($pass in 1..4) {
        foreach ($row in $all) {
            if ($tree.ContainsKey([int]$row.ParentProcessId)) { $tree[[int]$row.ProcessId] = $true }
        }
    }
    foreach ($row in @($all | Where-Object { $_.Name -eq 'msedgewebview2.exe' })) {
        $type = if ("$($row.CommandLine)" -match '--type=([a-z-]+)') { $Matches[1] } else { 'browser' }
        "KELD_EXP_WV $tag pid=$($row.ProcessId) ppid=$($row.ParentProcessId) type=$type in_probe_tree=$($tree.ContainsKey([int]$row.ProcessId))"
    }
    "KELD_EXP_CENSUS_DONE $tag cost_ms=$($cost.ElapsedMilliseconds)"
    [void]$Previous.Clear()
    foreach ($id in $current.Keys) { $Previous[$id] = $current[$id] }
}

function Write-RunningTasks {
    param([string]$Label)
    try {
        Get-ScheduledTask -ErrorAction Stop | Where-Object State -eq 'Running' |
            ForEach-Object { "KELD_EXP_TASK label=$Label task=$($_.TaskPath)$($_.TaskName)" }
    } catch {
        "KELD_EXP_TASK label=$Label error=$($_.Exception.Message)"
    }
}

function Get-StageMs {
    param([string[]]$Lines, [string]$Stage)
    foreach ($line in $Lines) {
        if ($line -match "KELD_EXP_STAGE stage=$Stage t_ms=([0-9]+)") { return [long]$Matches[1] }
    }
    $null
}

function Invoke-Launch {
    param([int]$Launch)
    $stdoutPath = Join-Path $env:TEMP "keld-exp-launch-$PID-$Launch.out"
    $stderrPath = Join-Path $env:TEMP "keld-exp-launch-$PID-$Launch.err"
    $baseline = Get-WebViewPids
    $previous = Get-CpuSnapshot
    $pending = [Collections.Generic.Queue[int]]::new()
    foreach ($checkpoint in $Checkpoints) { $pending.Enqueue($checkpoint) }
    "KELD_EXP_LAUNCH_START launch=$Launch utc=$((Get-Date).ToUniversalTime().ToString('HH:mm:ss.fff')) baseline_webview_processes=$($baseline.Count)"

    $clock = [Diagnostics.Stopwatch]::StartNew()
    $child = Start-Process -FilePath $BinaryPath -NoNewWindow -PassThru `
        -ArgumentList "$Case --ignored --exact --nocapture --test-threads=1" `
        -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    # Caches the handle so ExitCode stays readable after exit.
    $null = $child.Handle
    [int]$childPid = $child.Id
    $firstBrowserMs = 'never'
    $maxWebView = 0
    $killed = $false
    while ($true) {
        if ($child.WaitForExit(250)) { break }
        $elapsed = $clock.ElapsedMilliseconds
        $fresh = @(Get-WebViewPids | Where-Object { $baseline -notcontains $_ })
        if ($fresh.Count -gt 0 -and $firstBrowserMs -eq 'never') { $firstBrowserMs = $elapsed }
        if ($fresh.Count -gt $maxWebView) {
            $maxWebView = $fresh.Count
            "KELD_EXP_WVCOUNT launch=$Launch t_ms=$elapsed new_webview_processes=$maxWebView"
        }
        if ($pending.Count -gt 0 -and $elapsed -ge $pending.Peek()) {
            $checkpoint = $pending.Dequeue()
            Write-Census -Launch $Launch -Label "t$checkpoint" -ElapsedMs $elapsed -ChildPid $childPid -Previous $previous
        }
        if ($elapsed -ge $OuterKillMs) {
            Write-Census -Launch $Launch -Label 'before-kill' -ElapsedMs $elapsed -ChildPid $childPid -Previous $previous
            Write-RunningTasks -Label "launch$Launch-before-kill"
            & taskkill.exe /PID $childPid /T /F | ForEach-Object { "KELD_EXP_KILL launch=$Launch $_" }
            $killed = $true
            [void]$child.WaitForExit(10000)
            break
        }
    }
    $wallMs = $clock.ElapsedMilliseconds
    $exitCode = if ($child.HasExited) { $child.ExitCode } else { 'running' }
    Write-Census -Launch $Launch -Label 'completion' -ElapsedMs $wallMs -ChildPid $childPid -Previous $previous
    if ($Launch -eq 1 -and -not $killed) { Write-RunningTasks -Label 'launch1-completion' }

    # Launches stay independent: let this launch's browser leave before the next.
    $waitClock = [Diagnostics.Stopwatch]::StartNew()
    do {
        $survivors = @(Get-WebViewPids | Where-Object { $baseline -notcontains $_ })
        if ($survivors.Count -eq 0) { break }
        Start-Sleep -Milliseconds 200
    } while ($waitClock.ElapsedMilliseconds -lt 10000)
    "KELD_EXP_SURVIVORS launch=$Launch count=$($survivors.Count) waited_ms=$($waitClock.ElapsedMilliseconds)"

    $stdout = @(Get-Content -LiteralPath $stdoutPath -ErrorAction SilentlyContinue)
    $stderr = @(Get-Content -LiteralPath $stderrPath -ErrorAction SilentlyContinue)
    $stdout | ForEach-Object { "KELD_EXP_OUT launch=$Launch $_" }
    $stderr | ForEach-Object { "KELD_EXP_ERR launch=$Launch $_" }

    $eventLoop = Get-StageMs -Lines $stdout -Stage 'event-loop-built'
    $environment = Get-StageMs -Lines $stdout -Stage 'environment-completed'
    $window = Get-StageMs -Lines $stdout -Stage 'window-built'
    $controller = Get-StageMs -Lines $stdout -Stage 'controller-completed'
    $environmentMs = if ($null -ne $eventLoop -and $null -ne $environment) { $environment - $eventLoop } else { 'none' }
    $controllerMs = if ($null -ne $window -and $null -ne $controller) { $controller - $window } else { 'none' }
    $outcome = 'none'
    $browser = 'none'
    foreach ($line in $stdout) {
        if ($line -match 'KELD_EXP_RESULT outcome=([a-z-]+)') { $outcome = $Matches[1] }
        if ($line -match 'KELD_EXP_BROWSER pid=([a-z0-9]+)') { $browser = $Matches[1] }
    }
    "KELD_EXP_SUMMARY image=$Image launch=$Launch outcome=$outcome exit=$exitCode killed=$killed wall_ms=$wallMs environment_ms=$environmentMs controller_ms=$controllerMs first_browser_ms=$firstBrowserMs max_webview_processes=$maxWebView browser_pid=$browser survivors=$($survivors.Count)"

    foreach ($line in $stdout) {
        if ($line -match '^KELD_EXP_ROOT (.+keld-profile-acceptance-gh814-[0-9]+-[0-9]+)\s*$') {
            Remove-Item -LiteralPath $Matches[1] -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

$BinaryPath = (Resolve-Path -LiteralPath $BinaryPath).Path
"KELD_EXP_IMAGE image=$Image os=$env:ImageOS build=$([Environment]::OSVersion.Version) machine=$env:COMPUTERNAME cpus=$env:NUMBER_OF_PROCESSORS"
"KELD_EXP_BOOT last_boot_utc=$((Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('HH:mm:ss')) now_utc=$((Get-Date).ToUniversalTime().ToString('HH:mm:ss'))"
foreach ($pair in @(
        @('webview2-hklm', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'),
        @('webview2-hkcu', 'HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'),
        @('edge-hklm', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{56EB18F8-B008-4CBD-B6D2-8C97FE7E9062}'))) {
    "KELD_EXP_REGISTRY key=$($pair[0]) pv=$((Get-ItemProperty $pair[1] -ErrorAction SilentlyContinue).pv)"
}
Get-ChildItem 'C:\Program Files (x86)\Microsoft\EdgeWebView\Application' -ErrorAction SilentlyContinue |
    ForEach-Object { "KELD_EXP_RUNTIME_DIR $($_.Name)" }
try {
    Get-MpComputerStatus -ErrorAction Stop | ForEach-Object {
        "KELD_EXP_DEFENDER mode=$($_.AMRunningMode) realtime=$($_.RealTimeProtectionEnabled) onaccess=$($_.OnAccessProtectionEnabled)"
    }
} catch {
    "KELD_EXP_DEFENDER error=$($_.Exception.Message)"
}
Write-RunningTasks -Label 'before-launch1'
foreach ($launch in 1..$Launches) { Invoke-Launch -Launch $launch }
"KELD_EXP_DONE image=$Image launches=$Launches"
exit 0
