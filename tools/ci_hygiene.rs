//! KEL-39 contract check: CODEOWNERS, PR/issue templates, secret scan, Action SHAs.
//!
//! Not a Cargo member (no lockfile change). Compile with:
//! `rustc --edition=2024 -D warnings tools/ci_hygiene.rs`
//! Error text states the fix. Codes are not `KELD-*` so this file does not
//! collide with `docs/engineering/keld-error-codes.md` (owned by a parallel change).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const CODEOWNERS: &str = ".github/CODEOWNERS";
const PR_TEMPLATE: &str = ".github/PULL_REQUEST_TEMPLATE.md";
const ISSUE_DIR: &str = ".github/ISSUE_TEMPLATE";
const WORKFLOW: &str = ".github/workflows/ci.yml";
const KELDBOT_WORKFLOW: &str = ".github/workflows/keldbot.yml";
const CI_REQUIRED_EVALUATOR: &str = "tools/ci_required.sh";
const ATOMIC_PROTOCOL_CHECKER: &str = "tools/atomic_protocol.rs";
const AGENT_CONTEXT_CHECKER: &str = "tools/agent_context.rs";
const GITIGNORE: &str = ".gitignore";
const NEXTEST_CONFIG: &str = ".config/nextest.toml";
const MERMAID_CHECKER: &str = "tools/mermaid_docs.rs";
const MERMAID_RENDERER: &str = "tools/mermaid_render_check.sh";
const MERMAID_CONFIG: &str = "tools/mermaid-render-config.json";
const MERMAID_IMAGE_DIGEST: &str =
    "sha256:29077c6bd02f14bdfdd5fee552d9c00fe68d4fab3cd84952d21e2d1faf2fadaf";

const REQUIRED_OWNER_PATHS: &[&str] = &[
    "crates/keld-guard",
    "crates/keld-ipc",
    "Cargo.toml",
    ".github",
    "AGENTS.md",
    ".agents",
    ".codex",
    "docs/agents",
    "keld-error-codes.md",
    "justfile",
    "tools",
];

const PR_NEEDLES: &[&str] = &[
    "## Summary",
    "## Spec refs",
    "## Review gates",
    "## Tests",
    "## Platforms",
    "## Perf impact",
    "No boundary change",
    "agent/kel-",
    "cargo fmt",
    "clippy",
    "nextest",
    "mermaid-render-check",
];

const WORKFLOW_TEXT_NEEDLES: &[&str] = &[
    "gitleaks detect",
    "sha256sum -c",
    "--test tools/ci_hygiene.rs",
    "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb",
    "toolchain: 1.97.1",
    "--test tools/product_status.rs",
    "product-status check .",
    "--test tools/llms_docs.rs",
    "tools/llms_docs.rs",
    "llms-docs check",
];

const WORKFLOW_RUN_NEEDLES: &[&str] = &[
    "--test tools/mermaid_docs.rs",
    "mermaid-docs check .",
    "tools/mermaid_render_check.sh",
];

const ATOMIC_PROTOCOL_COMMANDS: &[&str] = &[
    "mkdir -p target/atomic-protocol",
    "rustc --edition=2024 -D warnings --test tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol-test",
    "target/atomic-protocol/atomic-protocol-test",
    "rustc --edition=2024 -D warnings tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol",
    "target/atomic-protocol/atomic-protocol check .",
];

const AGENT_CONTEXT_COMMANDS: &[&str] = &[
    "mkdir -p target/agent-context",
    "rustc --edition=2024 -D warnings --test tools/agent_context.rs -o target/agent-context/agent-context-test",
    "target/agent-context/agent-context-test",
    "rustc --edition=2024 -D warnings tools/agent_context.rs -o target/agent-context/agent-context",
    "target/agent-context/agent-context check .",
    "python3 -B tools/test_session_closeout.py",
    "python3 -B tools/test_session_closeout_hook.py",
];

const PRODUCT_STATUS_COMMANDS: &[&str] = &[
    "mkdir -p target/product-status",
    "rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test",
    "target/product-status/product-status-test",
    "rustc --edition=2024 -D warnings tools/product_status.rs -o target/product-status/product-status",
    "target/product-status/product-status check .",
];

const PRODUCT_STATUS_WINDOWS_COMMANDS: &[&str] = &[
    "mkdir -p target/product-status",
    "rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test",
    "target/product-status/product-status-test",
];

const WINDOWS_MEDIA_ACCEPTANCE_COMMANDS: &[&str] = &[
    "cargo clippy -p keld-wv --all-targets --features media-acceptance -- -D warnings",
    "if ($LASTEXITCODE -ne 0) { throw 'media-acceptance Clippy failed' }",
    "cargo nextest run -p keld-wv --features media-acceptance --profile ci --no-tests=pass",
    "if ($LASTEXITCODE -ne 0) { throw 'media-acceptance tests failed' }",
    "$artifacts = @(cargo test -p keld-wv --features media-acceptance --lib --no-run --message-format=json | ConvertFrom-Json)",
    "if ($LASTEXITCODE -ne 0) { throw 'media-acceptance test build failed' }",
    "$fixture = @($artifacts | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'keld_wv' -and $_.profile.test -eq $true -and $_.executable })",
    "if ($fixture.Count -ne 1) { throw \"expected one keld_wv libtest executable, found $($fixture.Count)\" }",
    "$fixturePath = (Resolve-Path -LiteralPath $fixture[0].executable).Path",
    "$fixtureHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $fixturePath).Hash.ToLowerInvariant()",
    "$evidenceRoot = Join-Path $env:RUNNER_TEMP 'keld-windows-media'",
    "$fixtureTempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar",
    "$sharedProfileRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'dev.keld')).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar",
    "if ($fixtureTempRoot.StartsWith($sharedProfileRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Windows media fixture temp root overlaps the shared dev.keld profile root' }",
    "$powerShellHost = (Get-Process -Id $PID).Path",
    "& $powerShellHost -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File crates/keld-wv/tests/windows_media_guard.ps1 -BinaryPath $fixturePath -EvidenceDirectory $evidenceRoot",
    "if ($LASTEXITCODE -ne 0) { throw 'Windows media guard acceptance failed' }",
    "$resultPath = Join-Path $evidenceRoot 'result.json'",
    "if (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) { throw 'Windows media guard produced no result.json' }",
    "$result = Get-Content -Raw -LiteralPath $resultPath | ConvertFrom-Json",
    "$requiredTopLevel = @('schema', 'source_executable', 'executable', 'executable_sha256', 'system', 'device', 'capture_device', 'scope', 'watchdog_probe', 'outer_deadline_probe', 'rows')",
    "$actualTopLevel = @($result.PSObject.Properties.Name | Sort-Object -Unique -CaseSensitive)",
    "if ($actualTopLevel.Count -ne $requiredTopLevel.Count -or @(Compare-Object -CaseSensitive -ReferenceObject $requiredTopLevel -DifferenceObject $actualTopLevel).Count -ne 0) { throw 'Windows media guard returned the wrong top-level artifact shape' }",
    "if ($result.schema -cne 'keld.windows-media-fixture/v1' -or @($result.rows).Count -ne 10) { throw 'Windows media guard returned the wrong schema or row count' }",
    "$expectedScope = 'raw WebView2 callback receipt: adapter input; removed-product-guard plus fixture-only completion after DEFAULT; adapter-bypass; same-callback explicit state; loopback origin; synthetic capture control; bounded teardown. No physical-device, saved-grant, snapshot, or revocation pass.'",
    "if ($result.system -cne [Environment]::OSVersion.VersionString -or $result.device -cne [Environment]::MachineName -or $result.capture_device -cne 'WebView2 synthetic development device' -or $result.scope -cne $expectedScope) { throw 'Windows media guard returned invalid platform or scope metadata' }",
    "$evidenceExecutable = Join-Path $evidenceRoot 'keld_wv_media_test.exe'",
    "if ($result.source_executable -cne $fixturePath -or $result.executable -cne $evidenceExecutable -or $result.executable_sha256 -cne $fixtureHash -or (Get-FileHash -Algorithm SHA256 -LiteralPath $fixturePath).Hash.ToLowerInvariant() -cne $fixtureHash -or (Get-FileHash -Algorithm SHA256 -LiteralPath $evidenceExecutable).Hash.ToLowerInvariant() -cne $fixtureHash) { throw 'Windows media result does not bind the selected Cargo executable and evidence copy' }",
    "$requiredProbeFields = @('mode', 'exit_code', 'outer_timed_out', 'stdout_sha256', 'stderr_sha256')",
    "$watchdogFields = @($result.watchdog_probe.PSObject.Properties.Name | Sort-Object -Unique -CaseSensitive)",
    "if ($watchdogFields.Count -ne $requiredProbeFields.Count -or @(Compare-Object -CaseSensitive -ReferenceObject $requiredProbeFields -DifferenceObject $watchdogFields).Count -ne 0 -or $result.watchdog_probe.mode -cne 'work' -or $result.watchdog_probe.exit_code -ne 124 -or $result.watchdog_probe.outer_timed_out -ne $false) { throw 'Windows media watchdog probe was absent or invalid' }",
    "$requiredOuterProbeFields = @('exit_code', 'outer_timed_out', 'stdout_sha256', 'stderr_sha256')",
    "$outerProbeFields = @($result.outer_deadline_probe.PSObject.Properties.Name | Sort-Object -Unique -CaseSensitive)",
    "if ($outerProbeFields.Count -ne $requiredOuterProbeFields.Count -or @(Compare-Object -CaseSensitive -ReferenceObject $requiredOuterProbeFields -DifferenceObject $outerProbeFields).Count -ne 0 -or $result.outer_deadline_probe.exit_code -eq 0 -or $result.outer_deadline_probe.outer_timed_out -ne $true) { throw 'Windows media outer-deadline probe was absent or invalid' }",
    "$hashPattern = '^[0-9a-f]{64}$'",
    "foreach ($probe in @(@($result.watchdog_probe, 'watchdog-probe'), @($result.outer_deadline_probe, 'outer-deadline-probe'))) {",
    "$stdoutPath = Join-Path $evidenceRoot \"$($probe[1]).log\"",
    "$stderrPath = Join-Path $evidenceRoot \"$($probe[1]).stderr.log\"",
    "if ($probe[0].stdout_sha256 -cnotmatch $hashPattern -or $probe[0].stderr_sha256 -cnotmatch $hashPattern -or (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash.ToLowerInvariant() -cne $probe[0].stdout_sha256 -or (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash.ToLowerInvariant() -cne $probe[0].stderr_sha256) { throw \"Windows media probe logs were absent or did not match: $($probe[1])\" }",
    "}",
    "$watchdogStdout = Get-Content -Raw -LiteralPath (Join-Path $evidenceRoot 'watchdog-probe.log')",
    "$watchdogStderr = Get-Content -Raw -LiteralPath (Join-Path $evidenceRoot 'watchdog-probe.stderr.log')",
    "$outerStdout = Get-Content -Raw -LiteralPath (Join-Path $evidenceRoot 'outer-deadline-probe.log')",
    "if (-not $watchdogStdout.Contains('KELD_MEDIA_PHASE watchdog-probe-work-block') -or -not $watchdogStderr.Contains('KELD_MEDIA_TIMEOUT: fixture exceeded 0.1 seconds') -or -not $outerStdout.Contains('KELD_MEDIA_PHASE outer-probe-block')) { throw 'Windows media probe phase or timeout evidence was absent' }",
    "$expectedRows = @('camera/adapter-bypass/app-grants', 'camera/force-allow/app-grants', 'camera/guarded/app-grants', 'camera/guarded/empty', 'camera/removed-guard/app-grants', 'microphone/adapter-bypass/app-grants', 'microphone/force-allow/app-grants', 'microphone/guarded/app-grants', 'microphone/guarded/empty', 'microphone/removed-guard/app-grants')",
    "$rowKeys = @($result.rows | ForEach-Object { \"$($_.kind)/$($_.mode)/$($_.manifest_case)\" } | Sort-Object -Unique -CaseSensitive)",
    "if ($rowKeys.Count -ne 10 -or @(Compare-Object -CaseSensitive -ReferenceObject $expectedRows -DifferenceObject $rowKeys).Count -ne 0) { throw 'Windows media guard returned the wrong or duplicate row set' }",
    "$requiredRowFields = @('kind', 'mode', 'manifest_case', 'nonce', 'view_id', 'host_pid', 'browser_pid', 'permission_kind', 'initial_state', 'requested_state', 'returned_state', 'origin_uri', 'manifest_fnv1a64', 'adapter_principal', 'adapter_capability', 'adapter_decision', 'adapter_tid', 'registration_identity', 'sender_identity', 'outcome', 'profile_path', 'profile_removed', 'exit_code', 'receipt', 'log_sha256', 'stderr_sha256')",
    "$receiptPattern = '^KELD_MEDIA_RESULT kind=(?<kind>camera|microphone) mode=(?<mode>guarded|removed-guard|adapter-bypass|force-allow) nonce=(?<nonce>[1-9][0-9]*) runtime=(?<runtime>[^ ]+) host_pid=(?<host>[1-9][0-9]*) browser_pid=(?<browser>[1-9][0-9]*) view_id=(?<view>[1-9][0-9]*) tid=(?<tid>[1-9][0-9]*) manifest_fnv1a64=(?<manifest_hash>[0-9a-f]{16}) adapter_principal=(?<adapter_principal>[^ ]+) adapter_capability=(?<adapter_capability>[^ ]+) adapter_decision=(?<adapter_decision>[^ ]+) adapter_tid=(?<adapter_tid>0|[1-9][0-9]*) permission_kind=(?<permission_kind>0|[1-9][0-9]*) before=(?<before>0|[1-9][0-9]*) requested=(?<requested>0|[1-9][0-9]*) after=(?<after>0|[1-9][0-9]*) origin_uri=(?<origin_uri>[^ ]+) registration_identity=(?<registration_identity>[1-9a-f][0-9a-f]{0,15}) sender_identity=(?<sender_identity>[1-9a-f][0-9a-f]{0,15}) outcome=(?<outcome>[^ ]+) manifest=(?<manifest>true|false) adapter=(?<adapter>true|false) effect=(?<effect>true|false) origin=(?<origin>true|false) js=(?<js>true|false) control=(?<control>true|false) accepted=(?<accepted>true|false) case_ok=(?<case_ok>true|false)$'",
    "$seenNonces = @{}",
    "foreach ($row in $result.rows) {",
    "$rowFields = @($row.PSObject.Properties.Name | Sort-Object -Unique -CaseSensitive)",
    "if ($rowFields.Count -ne $requiredRowFields.Count -or @(Compare-Object -CaseSensitive -ReferenceObject $requiredRowFields -DifferenceObject $rowFields).Count -ne 0) { throw 'Windows media guard returned the wrong row artifact shape' }",
    "$rowStem = \"$($row.kind)-$($row.mode)-$($row.manifest_case)\"",
    "$rowLog = Join-Path $evidenceRoot \"$rowStem.log\"",
    "$rowStderr = Join-Path $evidenceRoot \"$rowStem.stderr.log\"",
    "$logReceipts = @(Get-Content -LiteralPath $rowLog | Where-Object { \"$_\" -clike 'KELD_MEDIA_RESULT *' })",
    "if ($row.log_sha256 -cnotmatch $hashPattern -or $row.stderr_sha256 -cnotmatch $hashPattern -or (Get-FileHash -Algorithm SHA256 -LiteralPath $rowLog).Hash.ToLowerInvariant() -cne $row.log_sha256 -or (Get-FileHash -Algorithm SHA256 -LiteralPath $rowStderr).Hash.ToLowerInvariant() -cne $row.stderr_sha256 -or $logReceipts.Count -ne 1 -or \"$($logReceipts[0])\" -cne $row.receipt -or (Test-Path -LiteralPath $row.profile_path)) { throw \"Windows media row logs, receipt, or teardown did not match: $rowStem\" }",
    "$profileReceipts = @(Get-Content -LiteralPath $rowLog | Where-Object { \"$_\" -clike '*KELD_MEDIA_PROFILE *' })",
    "$expectedProfileLeaf = \"keld-media-$($row.host_pid)-$($row.nonce)\"",
    "$expectedProfilePath = [IO.Path]::GetFullPath((Join-Path $fixtureTempRoot $expectedProfileLeaf))",
    "if ($profileReceipts.Count -ne 1) { throw \"Windows media row profile receipt was absent or duplicated: $rowStem\" }",
    "$loggedProfilePath = [IO.Path]::GetFullPath((\"$($profileReceipts[0])\" -split 'KELD_MEDIA_PROFILE ', 2)[1])",
    "if ($loggedProfilePath -cne $row.profile_path -or $loggedProfilePath -cne $expectedProfilePath -or (Split-Path -Leaf $loggedProfilePath) -cne $expectedProfileLeaf) { throw \"Windows media row profile identity did not bind its isolated temp root, process, and nonce: $rowStem\" }",
    "if ($row.receipt -cmatch $receiptPattern) {",
    "$receiptFields = [pscustomobject]@{ kind = $Matches.kind; mode = $Matches.mode; nonce = $Matches.nonce; runtime = $Matches.runtime; host = $Matches.host; browser = $Matches.browser; view = $Matches.view; tid = $Matches.tid; manifest_hash = $Matches.manifest_hash; adapter_principal = $Matches.adapter_principal; adapter_capability = $Matches.adapter_capability; adapter_decision = $Matches.adapter_decision; adapter_tid = $Matches.adapter_tid; permission_kind = $Matches.permission_kind; before = $Matches.before; requested = $Matches.requested; after = $Matches.after; origin_uri = $Matches.origin_uri; registration_identity = $Matches.registration_identity; sender_identity = $Matches.sender_identity; outcome = $Matches.outcome; manifest = $Matches.manifest; adapter = $Matches.adapter; effect = $Matches.effect; origin = $Matches.origin; js = $Matches.js; control = $Matches.control; accepted = $Matches.accepted; case_ok = $Matches.case_ok }",
    "} else {",
    "throw \"Windows media row receipt was malformed: $rowStem\"",
    "}",
    "if ($receiptFields.kind -cne $row.kind -or $receiptFields.mode -cne $row.mode -or $receiptFields.nonce -cne \"$($row.nonce)\" -or [int]$receiptFields.host -ne $row.host_pid -or [int]$receiptFields.browser -ne $row.browser_pid -or [int]$receiptFields.view -ne $row.view_id -or [int]$receiptFields.permission_kind -ne $row.permission_kind -or [int]$receiptFields.before -ne $row.initial_state -or [int]$receiptFields.requested -ne $row.requested_state -or [int]$receiptFields.after -ne $row.returned_state -or $receiptFields.origin_uri -cne $row.origin_uri -or $receiptFields.manifest_hash -cne $row.manifest_fnv1a64 -or $receiptFields.adapter_principal -cne $row.adapter_principal -or $receiptFields.adapter_capability -cne $row.adapter_capability -or $receiptFields.adapter_decision -cne $row.adapter_decision -or [int]$receiptFields.adapter_tid -ne $row.adapter_tid -or $receiptFields.registration_identity -cne $row.registration_identity -or $receiptFields.sender_identity -cne $row.sender_identity -or $receiptFields.outcome -cne $row.outcome) { throw \"Windows media receipt did not bind its result row: $rowStem\" }",
    "if ($row.origin_uri -cmatch '^http://127\\.0\\.0\\.1:(?<origin_port>[1-9][0-9]{0,4})/$') {",
    "$originPort = [uint32]$Matches.origin_port",
    "} else {",
    "throw \"Windows media row origin was not canonical loopback: $rowStem\"",
    "}",
    "if ($originPort -gt 65535) { throw \"Windows media row origin port was out of range: $rowStem\" }",
    "if ($seenNonces.ContainsKey($receiptFields.nonce)) { throw \"Windows media receipt nonce was reused: $rowStem\" }",
    "$seenNonces[$receiptFields.nonce] = $true",
    "$expectedView = if ($row.kind -ceq 'camera') { 2 } else { 3 }",
    "$expectedPermissionKind = if ($row.kind -ceq 'camera') { 2 } else { 1 }",
    "$expectedManifestHash = if ($row.manifest_case -ceq 'empty') { 'e117311975d9f419' } else { '1fb6f771494b3631' }",
    "$expectedRequested = if ($row.mode -ceq 'force-allow') { 1 } else { 2 }",
    "$expectedAdapter = if ($row.mode -ceq 'guarded') { 'true' } else { 'false' }",
    "$expectedEffect = if ($row.mode -ceq 'force-allow') { 'false' } else { 'true' }",
    "$expectedControl = if ($row.mode -ceq 'guarded') { 'false' } else { 'true' }",
    "$expectedPrincipal = if ($row.mode -ceq 'guarded') { \"webview:$expectedView`:0\" } else { 'none' }",
    "$expectedCapability = if ($row.mode -ceq 'guarded') { \"web.$($row.kind)\" } else { 'none' }",
    "$expectedDecision = if ($row.mode -ceq 'guarded') { 'KELD-GUARD006' } else { 'none' }",
    "$expectedAdapterTid = if ($row.mode -ceq 'guarded') { [int]$receiptFields.tid } else { 0 }",
    "if ($row.view_id -ne $expectedView -or $row.permission_kind -ne $expectedPermissionKind -or $row.initial_state -ne 0 -or $row.requested_state -ne $expectedRequested -or $row.returned_state -ne $expectedRequested -or $row.manifest_fnv1a64 -cne $expectedManifestHash -or $row.adapter_principal -cne $expectedPrincipal -or $row.adapter_capability -cne $expectedCapability -or $row.adapter_decision -cne $expectedDecision -or $row.adapter_tid -ne $expectedAdapterTid -or $receiptFields.manifest -cne $expectedAdapter -or $receiptFields.adapter -cne $expectedAdapter -or $receiptFields.effect -cne $expectedEffect -or $receiptFields.origin -cne 'true' -or $receiptFields.js -cne 'true' -or $receiptFields.control -cne $expectedControl -or $receiptFields.accepted -cne $expectedAdapter -or $receiptFields.case_ok -cne 'true') { throw \"Windows media row violated the exact permission oracle: $rowStem\" }",
    "$expectedTrackKind = if ($row.kind -ceq 'camera') { 'video' } else { 'audio' }",
    "$expectedOutcome = if ($row.mode -ceq 'force-allow') { \"^$([Regex]::Escape($receiptFields.nonce)):resolved:$expectedTrackKind`:[1-9][0-9]*`:true$\" } else { \"$($receiptFields.nonce):true:NotAllowedError\" }",
    "if (($row.mode -ceq 'force-allow' -and $row.outcome -cnotmatch $expectedOutcome) -or ($row.mode -cne 'force-allow' -and $row.outcome -cne $expectedOutcome) -or $receiptFields.runtime -cnotmatch '^(0|[1-9][0-9]*)(\\.(0|[1-9][0-9]*))+$' -or $row.host_pid -le 0 -or $row.browser_pid -le 0 -or [int]$receiptFields.tid -le 0 -or [Convert]::ToUInt64($row.registration_identity, 16) -eq 0) { throw \"Windows media row lacked live runtime, process, origin, sender, or outcome evidence: $rowStem\" }",
    "}",
    "$invalidRows = @($result.rows | Where-Object { $_.exit_code -ne 0 -or $_.profile_removed -ne $true -or $_.registration_identity -cne $_.sender_identity -or $_.receipt -cnotlike 'KELD_MEDIA_RESULT * case_ok=true' -or $_.log_sha256 -cnotmatch $hashPattern -or $_.stderr_sha256 -cnotmatch $hashPattern })",
    "if ($invalidRows.Count -ne 0) { throw 'Windows media guard returned an invalid acceptance row' }",
];

const FUZZ_WORKSPACE_COMMANDS: &[&str] =
    &["cargo check --manifest-path crates/keld-ipc/fuzz/Cargo.toml"];

fn read(root: &Path, relative: &str) -> Result<String, String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|error| {
        format!(
            "CI-HYGIENE: missing `{}`: {error}. Restore the KEL-39 file from git or recreate it.",
            path.display()
        )
    })
}

fn github_dir_is_ignored(gitignore: &str) -> bool {
    gitignore.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return false;
        }
        // `/.github/*` ignores children of `.github/` (CODEOWNERS, workflows/,
        // ISSUE_TEMPLATE/), which is enough for GitHub to never see CI files.
        matches!(
            line,
            "/.github/"
                | "/.github"
                | ".github/"
                | ".github"
                | "/.github/**"
                | "/.github/*"
                | ".github/**"
                | ".github/*"
        )
    })
}

fn uncommented_codeowners_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    })
}

fn codeowners_covers(text: &str, needle: &str) -> bool {
    uncommented_codeowners_lines(text).any(|line| line.contains(needle) && line.contains('@'))
}

fn uncommented_line_contains(text: &str, needle: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#') && trimmed.contains(needle)
    })
}

fn active_shell_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

fn shell_line_position(lines: &[&str], exact: &str) -> Option<usize> {
    lines.iter().position(|line| *line == exact)
}

fn check_mermaid_msys_structure(renderer: &str) -> Result<(), String> {
    let lines = active_shell_lines(renderer);
    let required = [
        "running_under_msys() {",
        "docker_host_path() {",
        "prepare_docker_output_dir() {",
        "windows_native_command() {",
        "MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*' \"$@\"",
        "restore_windows_owner_only_dacl() {",
        "for command in cygpath powershell.exe; do",
        "$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()",
        "function Set-KeldOwnerOnlyDacl($item) {",
        "windows_native_command powershell.exe -NoProfile -NonInteractive -Command \"$powershell_script\"; then",
        "restore_docker_output_dir() {",
        "if ! running_under_msys; then",
        "\"$render_parent\"/keld-mermaid-render.*) ;;",
        "restore_windows_owner_only_dacl \"$path\"",
        "cleanup() {",
        "local cleanup_status=$?",
        "local cleanup_failed=0",
        "trap - EXIT",
        "if restore_docker_output_dir \"$render_dir\"; then",
        "exit \"$cleanup_status\"",
        "render_dir=$(mktemp -d \"$render_parent/keld-mermaid-render.XXXXXX\")",
        "prepare_docker_output_dir \"$render_dir\"",
        "docker_render_dir=$(docker_host_path \"$render_dir\")",
        "export MSYS2_ARG_CONV_EXCL='*'",
    ];
    let [
        running,
        docker_path,
        prepare,
        native_command,
        native_conversion,
        native_restore,
        native_tools,
        native_identity,
        native_dacl_function,
        native_powershell,
        restore,
        restore_guard,
        restore_allowlist,
        native_call,
        cleanup,
        cleanup_status,
        cleanup_failed,
        disable_exit_trap,
        restore_call,
        exit_with_status,
        render_create,
        prepare_call,
        render_path,
        exclusion,
    ] = required.map(|line| {
        shell_line_position(&lines, line).ok_or_else(|| {
            format!(
                "CI-HYGIENE: `{MERMAID_RENDERER}` is missing executable shell line `{line}`. Restore the PowerShell-launched Git-Bash detection, host-path conversion, writable isolated output bind, and owner-only retained-output cleanup."
            )
        })
    });
    let (
        running,
        docker_path,
        prepare,
        native_command,
        native_conversion,
        native_restore,
        native_tools,
        native_identity,
        native_dacl_function,
        native_powershell,
        restore,
        restore_guard,
        restore_allowlist,
        native_call,
        cleanup,
        cleanup_status,
        cleanup_failed,
        disable_exit_trap,
        restore_call,
        exit_with_status,
        render_create,
        prepare_call,
        render_path,
        exclusion,
    ) = (
        running?,
        docker_path?,
        prepare?,
        native_command?,
        native_conversion?,
        native_restore?,
        native_tools?,
        native_identity?,
        native_dacl_function?,
        native_powershell?,
        restore?,
        restore_guard?,
        restore_allowlist?,
        native_call?,
        cleanup?,
        cleanup_status?,
        cleanup_failed?,
        disable_exit_trap?,
        restore_call?,
        exit_with_status?,
        render_create?,
        prepare_call?,
        render_path?,
        exclusion?,
    );
    for line in [
        "try { $ownerSid = $identity.User } finally { $identity.Dispose() }",
        "$acl.SetAccessRuleProtection($true, $false)",
        "foreach ($rule in @($acl.Access)) { [void]$acl.RemoveAccessRuleAll($rule) }",
        "$ownerRule = New-Object @ownerRuleParams",
    ] {
        if shell_line_position(&lines, line).is_none() {
            return Err(format!(
                "CI-HYGIENE: `{MERMAID_RENDERER}` is missing native owner-only DACL operation `{line}`. Restore protected inheritance, removal of prior explicit grants, and the one-SID replacement rule."
            ));
        }
    }
    if !(running < docker_path
        && docker_path < prepare
        && prepare < native_command
        && native_command < native_conversion
        && native_conversion < native_restore
        && native_restore < native_tools
        && native_tools < native_identity
        && native_identity < native_dacl_function
        && native_dacl_function < native_powershell
        && native_powershell < restore
        && restore < restore_guard
        && restore_guard < restore_allowlist
        && restore_allowlist < native_call
        && native_call < cleanup
        && cleanup < cleanup_status
        && cleanup_status < cleanup_failed
        && cleanup_failed < disable_exit_trap
        && disable_exit_trap < restore_call
        && restore_call < exit_with_status
        && exit_with_status < render_create
        && render_create < prepare_call
        && prepare_call < render_path)
    {
        return Err(format!(
            "CI-HYGIENE: `{MERMAID_RENDERER}` has the MSYS detector, converter, output preparation/restoration, cleanup, or render-directory calls out of order. Restore definition-before-use, owner-only failure cleanup, and preparation before conversion/mounting it."
        ));
    }
    let uname = lines
        .iter()
        .position(|line| line.starts_with("case \"$(uname -s "));
    let msys_conditionals: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (*line == "if running_under_msys; then").then_some(index))
        .collect();
    let cygpath = shell_line_position(&lines, "cygpath -am \"$path\"");
    let chmod = shell_line_position(&lines, "chmod 0777 -- \"$path\" || {");
    let restore_chmod = shell_line_position(&lines, "chmod 0700 -- \"$path\" || {");
    let docker_run = lines
        .iter()
        .position(|line| line.starts_with("run_with_timeout 120 docker run"));
    if !uname.is_some_and(|index| running < index && index < docker_path)
        || !msys_conditionals
            .iter()
            .any(|index| docker_path < *index && *index < prepare)
        || !cygpath.is_some_and(|index| docker_path < index && index < prepare)
        || !chmod.is_some_and(|index| prepare < index && index < restore)
        || !restore_chmod.is_some_and(|index| restore < index && index < native_call)
        || !docker_run.is_some_and(|index| exclusion < index)
    {
        return Err(format!(
            "CI-HYGIENE: `{MERMAID_RENDERER}` has inert or reordered MSYS handling. Detection, 0777 preparation, 0700 retained-output restoration, failure-status preservation, and path-conversion exclusion must remain executable and ordered."
        ));
    }
    Ok(())
}

fn yaml_content(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let content = trimmed
        .split_once(" #")
        .map_or(trimmed, |(before, _)| before)
        .trim_end();
    Some((indent, content))
}

fn yaml_mapping_key(content: &str) -> Option<(String, &str)> {
    let content = content.strip_prefix("- ").unwrap_or(content);
    let (key, value) = content.split_once(':')?;
    Some((
        key.trim().trim_matches(['\'', '"']).to_owned(),
        value.trim(),
    ))
}

fn forbidden_workflow_control_key(text: &str) -> Option<String> {
    let mut run_block_indent = None;
    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if let Some(run_indent) = run_block_indent {
            if indent > run_indent {
                continue;
            }
            run_block_indent = None;
        }
        let Some((key, value)) = yaml_mapping_key(content) else {
            continue;
        };
        if key == "run" && (value.starts_with('|') || value.starts_with('>')) {
            run_block_indent = Some(indent);
            continue;
        }
        if key == "continue-on-error" || key == "defaults" {
            return Some(key);
        }
    }
    None
}

fn workflow_has_checkout_fetch_depth_zero(text: &str) -> bool {
    let mut checkout_indent = None;
    let mut with_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content.starts_with("- uses:") {
            checkout_indent = content.contains("actions/checkout@").then_some(indent);
            with_indent = None;
            continue;
        }
        let Some(checkout) = checkout_indent else {
            continue;
        };
        if indent <= checkout {
            checkout_indent = None;
            with_indent = None;
            continue;
        }
        if content == "with:" {
            with_indent = Some(indent);
            continue;
        }
        if let Some(with) = with_indent {
            if indent <= with {
                with_indent = None;
            } else if let Some(value) = content.strip_prefix("fetch-depth:") {
                let value = value
                    .trim()
                    .trim_matches(|character| character == '\'' || character == '"');
                if value == "0" {
                    return true;
                }
            }
        }
    }
    false
}

fn workflow_has_unconditional_named_step(text: &str, step_name: &str, command: &str) -> bool {
    let expected_name = format!("- name: {step_name}");
    let expected_command = format!("run: {command}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == expected_name {
            step_indent = Some(indent);
            continue;
        }
        let Some(step) = step_indent else {
            continue;
        };
        if indent <= step {
            return false;
        }
        if content.starts_with("if:") || content.starts_with("- if:") {
            return false;
        }
        if content == expected_command {
            return true;
        }
    }
    false
}

fn workflow_named_step_has_property(text: &str, step_name: &str, property: &str) -> bool {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == expected_name {
            step_indent = Some(indent);
            continue;
        }
        let Some(step) = step_indent else {
            continue;
        };
        if indent <= step {
            return false;
        }
        if content == property {
            return true;
        }
    }
    false
}

fn workflow_named_step_contains(text: &str, step_name: &str, needle: &str) -> bool {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == expected_name {
            step_indent = Some(indent);
            continue;
        }
        let Some(step) = step_indent else {
            continue;
        };
        if indent <= step {
            return false;
        }
        if content.contains(needle) {
            return true;
        }
    }
    false
}

fn workflow_job_block<'a>(text: &'a str, job_name: &str) -> Option<String> {
    let target = format!("{job_name}:");
    let mut found = false;
    let mut block = String::new();

    for line in text.lines() {
        let parsed = yaml_content(line);
        if !found {
            if matches!(parsed, Some((2, ref content)) if content == &target) {
                found = true;
                block.push_str(line);
                block.push('\n');
            }
            continue;
        }
        if matches!(parsed, Some((indent, _)) if indent <= 2) {
            break;
        }
        block.push_str(line);
        block.push('\n');
    }

    found.then_some(block)
}

fn workflow_direct_named_step_block(text: &str, step_name: &str) -> Option<String> {
    let expected_name = format!("- name: {step_name}");
    let mut found = false;
    let mut block = String::new();

    for line in text.lines() {
        let parsed = yaml_content(line);
        if !found {
            if matches!(parsed, Some((6, ref content)) if content == &expected_name) {
                found = true;
                block.push_str(line);
                block.push('\n');
            }
            continue;
        }
        if matches!(parsed, Some((indent, _)) if indent <= 6) {
            break;
        }
        block.push_str(line);
        block.push('\n');
    }

    found.then_some(block)
}

fn workflow_direct_named_step_count(text: &str, step_name: &str) -> usize {
    let expected_name = format!("- name: {step_name}");
    text.lines()
        .filter_map(yaml_content)
        .filter(|(indent, content)| *indent == 6 && *content == expected_name)
        .count()
}

fn workflow_job_sequence_values(block: &str, key: &str) -> Option<Vec<String>> {
    let expected_key = format!("{key}:");
    let mut sequence_indent = None;
    let mut values = Vec::new();

    for line in block.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if sequence_indent.is_none() {
            if indent == 4 && content == expected_key {
                sequence_indent = Some(indent);
            }
            continue;
        }
        let sequence = sequence_indent?;
        if indent <= sequence {
            break;
        }
        if indent == sequence + 2 {
            values.push(content.strip_prefix("- ")?.to_owned());
        }
    }

    sequence_indent.map(|_| values)
}

fn workflow_named_step_mapping(
    text: &str,
    step_name: &str,
    mapping: &str,
) -> Option<Vec<(String, String)>> {
    let block = workflow_direct_named_step_block(text, step_name)?;
    let mut found = false;
    let mut entries = Vec::new();
    for line in block.lines().skip(1) {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if !found {
            if indent == 8 {
                let (key, value) = yaml_mapping_key(content)?;
                if key == mapping {
                    if !value.is_empty() {
                        return None;
                    }
                    found = true;
                }
            }
            continue;
        }
        if indent <= 8 {
            break;
        }
        if indent != 10 {
            return None;
        }
        let (key, value) = yaml_mapping_key(content)?;
        if entries.iter().any(|(existing, _)| existing == &key) {
            return None;
        }
        entries.push((key, value.trim_matches(['\'', '"']).to_owned()));
    }
    found.then_some(entries)
}

fn workflow_named_step_shell_commands(text: &str, step_name: &str) -> Option<Vec<String>> {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;
    let mut run_indent = None;
    let mut command_text = None::<String>;
    let mut commands = Vec::new();

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if step_indent.is_none() {
            if content == expected_name {
                step_indent = Some(indent);
            }
            continue;
        }
        let step = step_indent?;
        if indent <= step {
            break;
        }
        if run_indent.is_none() {
            if content.starts_with("run: |") {
                run_indent = Some(indent);
                continue;
            }
            continue;
        }
        let run = run_indent?;
        if indent <= run {
            break;
        }
        if let Some(current) = command_text.as_mut() {
            current.push(' ');
            current.push_str(content.trim_end_matches('\\').trim_end());
            if !content.ends_with('\\') {
                commands.push(command_text.take()?);
            }
            continue;
        }
        let continued = content.ends_with('\\');
        let command = content.trim_end_matches('\\').trim_end().to_owned();
        if continued {
            command_text = Some(command);
        } else {
            commands.push(command);
        }
    }
    if let Some(command) = command_text {
        if command.is_empty() {
            return None;
        }
        commands.push(command);
    }
    run_indent.map(|_| commands)
}

fn workflow_job_level_property(block: &str, key: &str) -> Option<String> {
    let expected_key = format!("{key}:");
    for line in block.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == "steps:" {
            return None;
        }
        if indent == 4 {
            if let Some(value) = content.strip_prefix(&expected_key) {
                return Some(value.trim().to_owned());
            }
        }
    }
    None
}

fn workflow_named_step_direct_keys(text: &str, step_name: &str) -> Option<Vec<String>> {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;
    let mut keys = Vec::new();

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if step_indent.is_none() {
            if content == expected_name {
                step_indent = Some(indent);
            }
            continue;
        }
        let step = step_indent?;
        if indent <= step {
            break;
        }
        if indent == step + 2 {
            let (key, _) = content.split_once(':')?;
            keys.push(key.trim().trim_matches(['\'', '"']).to_owned());
        }
    }
    step_indent.map(|_| keys)
}

fn workflow_named_step_direct_value(text: &str, step_name: &str, key: &str) -> Option<String> {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if step_indent.is_none() {
            if content == expected_name {
                step_indent = Some(indent);
            }
            continue;
        }
        let step = step_indent?;
        if indent <= step {
            break;
        }
        if indent == step + 2 {
            let (candidate, value) = yaml_mapping_key(content)?;
            if candidate == key {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn workflow_job_level_if(block: &str) -> Option<String> {
    for line in block.lines() {
        let Some((_indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == "steps:" {
            return None;
        }
        if let Some(value) = content.strip_prefix("if:") {
            return Some(value.trim().to_owned());
        }
    }
    None
}

fn check_change_router_job(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "changes") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `changes` job. Restore the always-created change router; do not use workflow-level path filters for required CI."
        ));
    };
    if !uncommented_line_contains(&block, "name: change router") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `changes` job is missing `name: change router`. Keep the router's owner visible in that job."
        ));
    }
    if !workflow_has_checkout_fetch_depth_zero(&block) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `changes` job must use `actions/checkout` with `fetch-depth: 0`. The router needs both PR/push comparison commits; do not borrow a full-history checkout from another job."
        ));
    }
    for (step_name, command) in [
        ("Router contract tests", "tools/ci_changes_test.sh"),
        (
            "Classify changed-path ownership",
            "tools/ci_changes.sh github",
        ),
    ] {
        if !workflow_has_unconditional_named_step(&block, step_name, command) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `changes` job must have an unconditional `{step_name}` step whose exact command is `{command}`. Put it in the router job; a shell guard, GitHub `if`, or identical command in another job cannot prove routing."
            ));
        }
    }
    Ok(())
}

fn check_package_loop_shell(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "check") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `check` job. Restore the cross-platform package verification job."
        ));
    };
    for step_name in ["clippy (warnings deny)", "test"] {
        if !workflow_named_step_has_property(&block, step_name, "shell: bash") {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `{step_name}` must set `shell: bash`. Its package loop is Bash syntax; the Windows default PowerShell shell rejects it before Cargo runs."
            ));
        }
    }
    if !workflow_named_step_contains(&block, "test", "--no-tests=pass") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `test` must use `cargo nextest --no-tests=pass` for selected zero-test packages. Package routing must preserve the workspace suite's valid zero-test behavior, not fail before consumer tests run."
        ));
    }
    Ok(())
}

fn check_check_job_if_avoids_matrix(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "check") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `check` job. Restore the cross-platform package verification job."
        ));
    };
    let mut if_indent = None;
    let mut if_text = String::new();
    for line in block.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == "steps:" {
            break;
        }
        if let Some(if_at) = if_indent {
            if indent <= if_at {
                break;
            }
            if_text.push_str(content);
            if_text.push('\n');
            continue;
        }
        if let Some(value) = content.strip_prefix("if:") {
            if_indent = Some(indent);
            if_text.push_str(value.trim());
            if_text.push('\n');
        }
    }
    if if_text.contains("matrix.") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `check` job-level `if` must not use `matrix`. GitHub evaluates `jobs.<job_id>.if` before matrix expansion (contexts: github, needs, vars, inputs only). Referencing `matrix.os` invalidates the workflow so rustc never starts on any OS. Keep OS filters on steps, which do have `matrix`."
        ));
    }
    Ok(())
}

fn check_fuzz_workspace_step(text: &str) -> Result<(), String> {
    let Some(check_job) = workflow_job_block(text, "check") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no cross-platform `check` job for the keld-ipc fuzz workspace."
        ));
    };
    let step = "Check keld-ipc fuzz workspace";
    let count = workflow_direct_named_step_count(&check_job, step);
    if count != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `check` must contain exactly one `{step}` step; found {count}."
        ));
    }
    let block = workflow_direct_named_step_block(&check_job, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `check.steps`.")
    })?;
    let expected_keys = ["if".to_owned(), "run".to_owned()];
    if workflow_named_step_direct_keys(&block, step).as_deref() != Some(expected_keys.as_slice()) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must contain only its exact Ubuntu/router condition and direct run command."
        ));
    }
    let condition = "matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'";
    if workflow_named_step_direct_value(&block, step, "if").as_deref() != Some(condition) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must use the exact condition `{condition}` so the stable fuzz build runs only on the rust-routed Ubuntu row."
        ));
    }
    let commands = workflow_named_step_shell_commands(&block, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` has no executable multiline run block.")
    })?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(FUZZ_WORKSPACE_COMMANDS.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must directly run the stable fuzz-workspace cargo check without a wrapper, campaign, retry, or exit suppression."
        ));
    }
    Ok(())
}

fn check_msrv_avoids_apt(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "msrv") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `msrv` job. Restore MSRV `cargo check`; do not drop the rustc version gate to avoid Ubuntu apt."
        ));
    };
    if !uncommented_line_contains(&block, "runs-on: macos-latest") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `msrv` must `runs-on: macos-latest`. MSRV is a rustc version gate; do not install WebKitGTK via `apt-get` on Ubuntu for it."
        ));
    }
    if uncommented_line_contains(&block, "apt-get") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `msrv` must not call `apt-get`. WebKitGTK belongs on Linux GUI smoke / Ubuntu clippy when `keld-wv` actually links, not on the MSRV rustc job."
        ));
    }
    Ok(())
}

/// Checks the repository's block-style direct action steps, not shell text.
fn check_bun_setup_steps(text: &str, job: &str) -> Result<(), String> {
    let invalid = || {
        format!(
            "CI-HYGIENE: `{WORKFLOW}` `{job}` must have at least one `oven-sh/setup-bun` step, and every such step must pin its own `with.bun-version` to `1.4.2`. Use direct block-style steps and inputs; unrelated job text is not a runtime pin."
        )
    };
    let literal_key = |key: &str| {
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    };
    let block = workflow_job_block(text, job).ok_or_else(invalid)?;
    let mut steps: Vec<Vec<(usize, &str)>> = Vec::new();
    let mut in_steps = false;
    for (indent, content) in block.lines().filter_map(yaml_content) {
        if indent <= 4 {
            in_steps = indent == 4 && content == "steps:";
            continue;
        }
        if !in_steps {
            continue;
        }
        if indent == 6 {
            let first = content.strip_prefix("- ").ok_or_else(invalid)?;
            // Do not silently miss actions hidden in unsupported flow/alias syntax.
            if first.starts_with('{') || first.starts_with('*') || first.starts_with('&') {
                return Err(invalid());
            }
            steps.push(Vec::new());
        }
        if indent >= 6
            && let Some(step) = steps.last_mut()
        {
            step.push((indent, content));
        }
    }
    let mut setups = 0;
    for step in steps {
        let mut property_count = 0;
        let properties: Vec<_> = step
            .iter()
            .enumerate()
            .filter(|(index, (indent, _))| *index == 0 || *indent == 8)
            .filter_map(|(index, (_, content))| {
                property_count += 1;
                yaml_mapping_key(content).map(|(key, value)| (index, key, value))
            })
            .collect();
        // A valid GitHub step cannot combine `run` and `uses`. In a run step,
        // block-scalar contents must never be interpreted as action properties.
        if properties.iter().any(|(_, key, _)| key == "run") {
            continue;
        }
        // YAML aliases, tags, escaped keys and folded/escaped action values
        // require decoding we do not own. Reject them instead of mistaking a
        // setup-bun action for an unrelated step under this narrow grammar.
        if properties.len() != property_count
            || properties.iter().any(|(_, key, _)| !literal_key(key))
        {
            return Err(invalid());
        }
        let uses: Vec<_> = properties
            .iter()
            .filter(|(_, key, _)| key == "uses")
            .collect();
        if uses.iter().any(|(_, _, value)| {
            let literal = value.trim_matches(['\'', '"']);
            literal.is_empty()
                || !literal.chars().all(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '@' | ':')
                })
        }) {
            return Err(invalid());
        }
        if !uses.iter().any(|(_, _, value)| {
            value
                .trim_matches(['\'', '"'])
                .to_ascii_lowercase()
                .starts_with("oven-sh/setup-bun@")
        }) {
            continue;
        }
        setups += 1;
        let with: Vec<_> = properties
            .iter()
            .filter(|(_, key, _)| key == "with")
            .collect();
        if uses.len() != 1 || with.len() != 1 || !with[0].2.is_empty() {
            return Err(invalid());
        }
        let inputs = step[with[0].0 + 1..]
            .iter()
            .take_while(|(indent, _)| *indent > 8)
            .filter(|(indent, _)| *indent == 10)
            .map(|(_, content)| yaml_mapping_key(content).ok_or_else(invalid))
            .collect::<Result<Vec<_>, _>>()?;
        if inputs.iter().any(|(key, _)| !literal_key(key)) {
            return Err(invalid());
        }
        let pins: Vec<_> = inputs
            .iter()
            .filter(|(key, _)| key == "bun-version")
            .collect();
        if pins.len() != 1 || pins[0].1.trim_matches(['\'', '"']) != "1.4.2" {
            return Err(invalid());
        }
    }
    if setups == 0 {
        return Err(invalid());
    }
    Ok(())
}

/// Asserts the `bun-test` lane's decidable properties and the exact Bun pin for
/// every current Bun consumer: `check`, `bun-test`, and `hygiene`. The `bun-test`
/// lane must exist, be gated on the router's own output, and avoid a second live
/// apt lane. Each of those is a fact about the workflow text.
///
/// What this deliberately does not assert — that the lane really runs the
/// suites the router selected — is tracked in KEL-115, because it is a
/// behavioural property and no reading of the text settles it.
fn check_bun_test_job(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "bun-test") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `bun-test` job. The `packages/` TypeScript suites need a lane of their own: the Bun install inside `check` exists so Rust tests can spawn Bun children and never runs `bun test`."
        ));
    };
    if !uncommented_line_contains(&block, "if: needs.changes.outputs.ts == 'true'") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `bun-test` must be gated on `if: needs.changes.outputs.ts == 'true'`. The router owns which diffs reach this lane; an ungated job wastes runners and a gate on another output silently never runs (contexts: needs, github, vars, inputs only — never `matrix`)."
        ));
    }
    for job in ["bun-test", "check", "hygiene"] {
        check_bun_setup_steps(text, job)?;
    }
    // Deliberately NOT checked here: that the lane actually executes the suite
    // over the router's selection. Whether a shell script runs a command is not
    // decidable from its text, and every attempt to assert it by pattern was
    // broken from outside the pattern's frame — a piped `echo`, `if: false`,
    // `continue-on-error`, `|| true`, a `for` loop whose body ignores the loop
    // variable, the selection variable reassigned inside the script being
    // parsed, and `continue` / `exit 0` making the suite unreachable. Each of
    // those passed a check written specifically to catch the previous one.
    //
    // A guard that looks authoritative while being bypassable is worse than an
    // absent one, because it is read as proof. KEL-115 replaces this with a
    // receipt: the lane reports which suites it ran, and that is compared
    // against the router's `ts_packages` output. Behaviour, not text.
    if uncommented_line_contains(&block, "apt-get") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `bun-test` must not call `apt-get`. Bun ships from `oven-sh/setup-bun`; a second live Ubuntu apt lane contends with Linux GUI smoke on the Azure mirrors."
        ));
    }
    Ok(())
}

fn check_linux_media_guard_step(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "linux-gui-smoke") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` must keep the `linux-gui-smoke` job that owns real WebKitGTK media and window evidence."
        ));
    };
    let build_step = "Build Linux media guard probe";
    let gui_step = "Xvfb GUI smoke test — title, controls, and clean close";
    for step in [build_step, gui_step] {
        if workflow_direct_named_step_count(&block, step) != 1 {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` must contain exactly one direct `{step}` step for KEL-132."
            ));
        }
        let step_block = workflow_direct_named_step_block(&block, step).ok_or_else(|| {
            format!("CI-HYGIENE: `{WORKFLOW}` cannot parse the direct `{step}` step for KEL-132.")
        })?;
        if workflow_named_step_direct_keys(&step_block, step) != Some(vec![String::from("run")]) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `{step}` must have only an unconditional direct `run` key for KEL-132."
            ));
        }
    }
    let build_block = workflow_direct_named_step_block(&block, build_step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` cannot parse `{build_step}` for KEL-132.")
    })?;
    let expected_build = vec![
        String::from("cargo build -p keld-wv --example linux_media_guard"),
        String::from(
            "cc -shared -fPIC -Wall -Wextra -Werror -Wpedantic $(pkg-config --cflags webkit2gtk-4.1) crates/keld-wv/tests/fixtures/linux_media_interpose.c -o \"$RUNNER_TEMP/linux_media_interpose.so\" -ldl $(pkg-config --libs webkit2gtk-4.1)",
        ),
    ];
    if workflow_named_step_shell_commands(&build_block, build_step) != Some(expected_build) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{build_step}` must contain exactly KEL-132's two executable build commands and no wrappers."
        ));
    }
    let gui_block = workflow_direct_named_step_block(&block, gui_step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` cannot parse `{gui_step}` for KEL-132.")
    })?;
    let expected_gui = "xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host";
    if workflow_named_step_direct_value(&gui_block, gui_step, "run").as_deref()
        != Some(expected_gui)
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{gui_step}` must directly execute KEL-132's tracked Linux GUI oracle as its only command; conditional or early-success wrappers are forbidden."
        ));
    }
    Ok(())
}

fn check_required_job(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "required") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `required` job. Add one always-created merge-admission result that consumes every conditional lane plus unconditional gitleaks."
        ));
    };
    if !uncommented_line_contains(&block, "name: CI required") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` must publish the stable check name `CI required` for branch protection."
        ));
    }
    if workflow_job_level_if(&block).as_deref() != Some("${{ always() }}") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` must use job-level `if: ${{{{ always() }}}}`. Otherwise a failed/cancelled dependency skips the merge decision instead of making it fail."
        ));
    }
    if workflow_job_level_property(&block, "continue-on-error").is_some() {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` must not set `continue-on-error`; the merge decision must preserve a failing exit status."
        ));
    }
    let evaluator_keys =
        workflow_named_step_direct_keys(&block, "Verify required CI results").unwrap_or_default();
    if evaluator_keys != ["env", "run"] {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` evaluator step may contain only direct `env` and `run` keys; got `{}`. Conditions, custom shells, and continue-on-error can erase its failing status.",
            evaluator_keys.join(", ")
        ));
    }
    let expected_needs = [
        "changes",
        "fmt",
        "check",
        "bun-test",
        "linux-gui-smoke",
        "msrv",
        "deny",
        "secrets",
        "hygiene",
        "codeql",
        "dependency-review",
    ];
    let actual_needs = workflow_job_sequence_values(&block, "needs").ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `required` must declare a structured `needs` sequence.")
    })?;
    if actual_needs != expected_needs {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required.needs` must be exactly `{}`; got `{}`. The result must observe every routed lane plus unconditional gitleaks.",
            expected_needs.join(", "),
            actual_needs.join(", ")
        ));
    }
    for (key, expression) in [
        ("KELD_RESULT_CHANGES", "${{ needs.changes.result }}"),
        ("KELD_RESULT_FMT", "${{ needs.fmt.result }}"),
        ("KELD_RESULT_CHECK", "${{ needs.check.result }}"),
        ("KELD_RESULT_BUN", "${{ needs['bun-test'].result }}"),
        ("KELD_RESULT_GUI", "${{ needs['linux-gui-smoke'].result }}"),
        ("KELD_RESULT_MSRV", "${{ needs.msrv.result }}"),
        ("KELD_RESULT_DENY", "${{ needs.deny.result }}"),
        ("KELD_RESULT_SECRETS", "${{ needs.secrets.result }}"),
        ("KELD_RESULT_HYGIENE", "${{ needs.hygiene.result }}"),
        ("KELD_ROUTE_RUST", "${{ needs.changes.outputs.rust }}"),
        ("KELD_ROUTE_TS", "${{ needs.changes.outputs.ts }}"),
        ("KELD_ROUTE_GUI", "${{ needs.changes.outputs.gui }}"),
        ("KELD_ROUTE_MSRV", "${{ needs.changes.outputs.msrv }}"),
        ("KELD_ROUTE_DENY", "${{ needs.changes.outputs.deny }}"),
        ("KELD_ROUTE_HYGIENE", "${{ needs.changes.outputs.hygiene }}"),
        ("KELD_ROUTE_DOCS", "${{ needs.changes.outputs.docs }}"),
        ("KELD_RESULT_CODEQL", "${{ needs.codeql.result }}"),
        (
            "KELD_RESULT_DEPENDENCY_REVIEW",
            "${{ needs['dependency-review'].result }}",
        ),
    ] {
        if !workflow_named_step_mapping(&block, "Verify required CI results", "env").is_some_and(
            |entries| {
                entries
                    .iter()
                    .any(|(name, value)| name == key && value == expression)
            },
        ) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `required` must bind `{key}: {expression}` in the evaluator step's `env` mapping. Do not let a missing or spoofed handoff erase the merge gate."
            ));
        }
    }
    let expected_check_command = concat!(
        "tools/ci_required.sh check ",
        "\"$KELD_RESULT_CHANGES\" \"$KELD_RESULT_FMT\" \"$KELD_RESULT_CHECK\" ",
        "\"$KELD_RESULT_BUN\" \"$KELD_RESULT_GUI\" \"$KELD_RESULT_MSRV\" ",
        "\"$KELD_RESULT_DENY\" \"$KELD_RESULT_SECRETS\" \"$KELD_RESULT_HYGIENE\" ",
        "\"$KELD_ROUTE_RUST\" \"$KELD_ROUTE_TS\" \"$KELD_ROUTE_GUI\" ",
        "\"$KELD_ROUTE_MSRV\" \"$KELD_ROUTE_DENY\" \"$KELD_ROUTE_HYGIENE\" ",
        "\"$KELD_ROUTE_DOCS\" \"$KELD_RESULT_CODEQL\" \"$KELD_RESULT_DEPENDENCY_REVIEW\""
    );
    let expected_commands = [
        "tools/ci_required.sh test".to_owned(),
        expected_check_command.to_owned(),
    ];
    let actual_commands = workflow_named_step_shell_commands(&block, "Verify required CI results")
        .unwrap_or_default();
    if actual_commands != expected_commands {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` evaluator run block must contain only its self-test and the exact ordered 18-argument check, without control flow, reassignment, wrappers, or exit-status suppression."
        ));
    }

    Ok(())
}

fn check_hygiene_contract_step(
    text: &str,
    step: &str,
    expected_commands: &[&str],
) -> Result<(), String> {
    let Some(hygiene) = workflow_job_block(text, "hygiene") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `hygiene` job for `{step}`."
        ));
    };
    let expected_if =
        "needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'";
    if workflow_job_level_if(&hygiene).as_deref() != Some(expected_if) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `hygiene` must run for both hygiene and docs router outputs so `{step}` cannot disappear on a docs-only edit."
        ));
    }
    if workflow_job_level_property(&hygiene, "continue-on-error").is_some() {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `hygiene` must not set `continue-on-error`; `CI required` needs the atomic check's failing status."
        ));
    }
    let expected_name = format!("- name: {step}");
    let name_count = hygiene
        .lines()
        .filter_map(yaml_content)
        .filter(|(indent, content)| *indent == 6 && *content == expected_name)
        .count();
    if name_count != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `hygiene` must contain exactly one `{step}` step; found {name_count}."
        ));
    }
    let contract_step = workflow_direct_named_step_block(&hygiene, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `hygiene.steps`.")
    })?;
    let expected_keys = ["run".to_owned()];
    if workflow_named_step_direct_keys(&contract_step, step).as_deref()
        != Some(expected_keys.as_slice())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` may contain only its `run` key. A condition, custom shell, or continue-on-error can erase enforcement."
        ));
    }
    let commands = workflow_named_step_shell_commands(&contract_step, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` has no executable multiline `run` block.")
    })?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(expected_commands.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must compile and run checker tests and the real checkout exactly, without wrappers or exit suppression."
        ));
    }
    Ok(())
}

fn check_atomic_protocol_step(text: &str) -> Result<(), String> {
    check_hygiene_contract_step(
        text,
        "Atomic problem-solving protocol contract",
        ATOMIC_PROTOCOL_COMMANDS,
    )
}

fn check_agent_context_step(text: &str) -> Result<(), String> {
    check_hygiene_contract_step(
        text,
        "Agent instruction context budget",
        AGENT_CONTEXT_COMMANDS,
    )
}

fn check_product_status_step(text: &str) -> Result<(), String> {
    let Some(changes) = workflow_job_block(text, "changes") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no always-created `changes` job for product status."
        ));
    };
    let step = "Product status contract";
    let count = workflow_direct_named_step_count(&changes, step);
    if count != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `changes` must contain exactly one unconditional `{step}` step; found {count}."
        ));
    }
    let block = workflow_direct_named_step_block(&changes, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `changes.steps`.")
    })?;
    if workflow_named_step_direct_keys(&block, step).as_deref()
        != Some(["run".to_owned()].as_slice())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` may contain only its run key; conditions and wrappers can skip canonical evidence validation."
        ));
    }
    let commands = workflow_named_step_shell_commands(&block, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` has no executable multiline run block.")
    })?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(PRODUCT_STATUS_COMMANDS.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must run the exact product-status tests and real-checkout check without echo, wrappers, or suppression."
        ));
    }
    Ok(())
}

fn check_product_status_windows_step(text: &str) -> Result<(), String> {
    let Some(check_job) = workflow_job_block(text, "check") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no cross-platform `check` job for Windows status paths."
        ));
    };
    let step = "Product status Windows path contracts";
    let count = workflow_direct_named_step_count(&check_job, step);
    if count != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `check` must contain exactly one `{step}` step; found {count}."
        ));
    }
    let block = workflow_direct_named_step_block(&check_job, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `check.steps`.")
    })?;
    let expected_keys = ["if".to_owned(), "shell".to_owned(), "run".to_owned()];
    if workflow_named_step_direct_keys(&block, step).as_deref() != Some(expected_keys.as_slice()) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must contain only the exact `if`, `shell: bash`, and `run` keys."
        ));
    }
    if workflow_named_step_direct_value(&block, step, "if").as_deref()
        != Some("matrix.os == 'windows-latest'")
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must use the exact condition `matrix.os == 'windows-latest'`."
        ));
    }
    if workflow_named_step_direct_value(&block, step, "shell").as_deref() != Some("bash") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must set `shell: bash`; its commands use POSIX syntax and must remain idempotent on a warm Windows cache."
        ));
    }
    let commands = workflow_named_step_shell_commands(&block, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` has no executable multiline run block.")
    })?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(PRODUCT_STATUS_WINDOWS_COMMANDS.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must run the exact Windows path mutation suite."
        ));
    }
    Ok(())
}

fn check_windows_media_acceptance_step(text: &str) -> Result<(), String> {
    let Some(check_job) = workflow_job_block(text, "check") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no cross-platform `check` job for KEL-132 Windows media acceptance."
        ));
    };
    let step = "Windows media guard acceptance (KEL-132)";
    let count = workflow_direct_named_step_count(&check_job, step);
    if count != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `check` must contain exactly one `{step}` step; found {count}."
        ));
    }
    let block = workflow_direct_named_step_block(&check_job, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `check.steps`.")
    })?;
    let expected_keys = ["if".to_owned(), "shell".to_owned(), "run".to_owned()];
    if workflow_named_step_direct_keys(&block, step).as_deref() != Some(expected_keys.as_slice()) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must contain only the exact `if`, `shell: pwsh`, and `run` keys."
        ));
    }
    let expected_condition =
        "matrix.os == 'windows-latest' && contains(needs.changes.outputs.packages, 'keld-wv')";
    if workflow_named_step_direct_value(&block, step, "if").as_deref() != Some(expected_condition) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must use the exact Windows and routed keld-wv condition."
        ));
    }
    if workflow_named_step_direct_value(&block, step, "shell").as_deref() != Some("pwsh") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must set `shell: pwsh` for its tracked PowerShell oracle."
        ));
    }
    let commands = workflow_named_step_shell_commands(&block, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` has no executable multiline run block.")
    })?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(WINDOWS_MEDIA_ACCEPTANCE_COMMANDS.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must run the exact feature Clippy/tests, Cargo JSON executable selection, and tracked media oracle without wrappers or suppression."
        ));
    }
    Ok(())
}

fn check_keldbot_workflow(root: &Path) -> Result<(), String> {
    let text = read(root, KELDBOT_WORKFLOW)?;
    for needle in [
        "pull_request_target:",
        "types: [opened, edited, synchronize]",
        "permissions: {}",
    ] {
        if !uncommented_line_contains(&text, needle) {
            return Err(format!(
                "CI-HYGIENE: `{KELDBOT_WORKFLOW}` is missing `{needle}`. Keep final-head metadata validation on opened, edited, and synchronize with a default-deny token."
            ));
        }
    }
    if uncommented_line_contains(&text, "actions/checkout@")
        || uncommented_line_contains(&text, "run:")
    {
        return Err(format!(
            "CI-HYGIENE: `{KELDBOT_WORKFLOW}` uses `pull_request_target` with a repository checkout or shell `run:` step. Keep PR-controlled code off this secret-bearing workflow; validate proposed workflow bytes in unprivileged CI instead."
        ));
    }
    for job in ["gatekeeper", "title-lint"] {
        let Some(block) = workflow_job_block(&text, job) else {
            return Err(format!(
                "CI-HYGIENE: `{KELDBOT_WORKFLOW}` has no `{job}` job. Restore the required PR metadata check."
            ));
        };
        if let Some(condition) = workflow_job_level_if(&block) {
            return Err(format!(
                "CI-HYGIENE: `{KELDBOT_WORKFLOW}` `{job}` has job-level `if: {condition}`. Required metadata checks must run again on synchronize; a skipped job satisfies branch protection without revalidating the final head."
            ));
        }
    }
    for heading in [
        "Summary",
        "Spec refs",
        "Review gates",
        "Tests",
        "Platforms",
        "Perf impact",
    ] {
        if !text.contains(&format!("\"{heading}\"")) {
            return Err(format!(
                "CI-HYGIENE: `{KELDBOT_WORKFLOW}` gatekeeper omits required PR heading `{heading}`. Keep it aligned with `{PR_TEMPLATE}`."
            ));
        }
    }
    let unpinned = action_uses_unpinned(&text);
    if !unpinned.is_empty() {
        return Err(format!(
            "CI-HYGIENE: `{KELDBOT_WORKFLOW}` has unpinned `uses:` entries. Pin every action to a 40-character commit SHA; offenders: {unpinned:?}"
        ));
    }
    Ok(())
}

fn executable_run_fragment_contains(fragment: &str, needle: &str) -> bool {
    if !fragment.contains(needle) {
        return false;
    }
    let command = fragment.trim();
    !(command.starts_with("echo ") && !command.contains('|'))
        && !command.starts_with("Write-Output ")
}

fn workflow_has_executable_run_needle(text: &str, needle: &str) -> bool {
    let mut run_indent = None;
    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content.starts_with("- run:") || content.starts_with("run:") {
            run_indent = Some(indent);
            let command = content
                .strip_prefix("- run:")
                .or_else(|| content.strip_prefix("run:"))
                .unwrap_or_default();
            if executable_run_fragment_contains(command, needle) {
                return true;
            }
            continue;
        }
        if let Some(run) = run_indent {
            if indent <= run {
                run_indent = None;
            } else if executable_run_fragment_contains(content, needle) {
                return true;
            }
        }
    }
    false
}

fn action_uses_unpinned(workflow: &str) -> Vec<(usize, String)> {
    let mut bad = Vec::new();
    for (idx, line) in workflow.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(rest) = uses_spec(trimmed) else {
            continue;
        };
        if rest.starts_with("./") || rest.starts_with("docker://") {
            continue;
        }
        if !is_pinned_sha(rest) {
            bad.push((idx + 1, rest.to_owned()));
        }
    }
    bad
}

fn uses_spec(trimmed: &str) -> Option<&str> {
    let (key, value) = yaml_mapping_key(trimmed)?;
    (key == "uses").then_some(value)
}

fn uses_action_ref(spec: &str) -> &str {
    let without_comment = spec
        .split_once('#')
        .map(|(before, _)| before)
        .unwrap_or(spec)
        .trim();
    without_comment
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
}

fn is_pinned_sha(spec: &str) -> bool {
    let action_ref = uses_action_ref(spec);
    let Some((_, sha)) = action_ref.rsplit_once('@') else {
        return false;
    };
    sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The verification gate AGENTS.md mandates is `--profile ci`, so that profile
/// must not retry: a retry reports a flaky test as green and hides the first
/// failure from the summary a human or agent reads (KEL-112).
///
/// Deliberately narrow, and its limits are the point.
///
/// It catches a plainly-spelled `retries` in the section family that binds the
/// gate — see [`classify_gate_section`]: either profile's own table, its
/// `[[...overrides]]`, or its `.retries` sub-table, with a trailing comment on
/// the header tolerated. It does NOT catch:
///
/// - `cargo nextest run --profile ci --retries 2`
/// - `NEXTEST_RETRIES=2` in the job's environment
/// - `--profile local` pointed at a profile that does retry
/// - a key spelled so as to evade a line reader: `"retries"`, an escape-encoded
///   `"\U00000072etries"`, or a header hidden inside a multi-line string
///
/// The first three were reproduced against this exact config with the guard
/// green, and no reading of this file can catch them: nextest resolves retries
/// from config UNION flag UNION env, flag and env winning, so the deciding
/// inputs are not in the file at all.
///
/// The enumeration above is what has been *measured*, not a proof of
/// completeness. nextest owns profile resolution, and a future version may add
/// another binding form; this guard would not know. That asymmetry is the
/// reason KEL-115 replaces it rather than growing it.
///
/// The fourth is decidable but deliberately not chased. Three rounds of
/// hardening a parser for it each produced a new spelling from outside the
/// previous frame, and an evasive spelling is not the regression this guard
/// exists to prevent — an ordinary edit re-adding `retries` is. KEL-115
/// replaces the whole approach with a behavioural check: run the real gate
/// command against a deliberately failing test and assert exactly one attempt.
///
/// Extend this only to a section that genuinely binds the gate and is verified
/// to retry. Do not extend it to chase an evasive spelling: that makes it look
/// more authoritative without making it more correct.
fn check_ci_profile_does_not_retry(root: &Path) -> Result<(), String> {
    let text = read(root, NEXTEST_CONFIG)?;
    if let Some((section, parent)) = gate_section_with_unfollowable_parent(&text) {
        return Err(format!(
            "CI-HYGIENE: `{NEXTEST_CONFIG}` has `{section}` inheriting from `{parent}`, and \
             this check cannot follow that chain to prove the mandated gate does not retry — \
             nextest inheritance is transitive, so `retries` anywhere up the chain binds \
             `--profile ci`. Inherit from `default` (the implicit parent) instead, or move \
             the settings into `{section}` directly."
        ));
    }
    if let Some(section) = gate_section_setting_retries(&text) {
        return Err(format!(
            "CI-HYGIENE: `{NEXTEST_CONFIG}` sets `retries` under `{section}`, which binds \
             `--profile ci` — the profile AGENTS.md mandates as the verification gate. A flaky \
             test would report green and its first failure would never be shown. Remove the \
             `retries` key and fix the non-deterministic test instead (await the condition, \
             bind port 0, use a temp dir)."
        ));
    }
    Ok(())
}

/// How a section header relates to the profile `--profile ci` resolves.
#[derive(PartialEq)]
enum GateSection {
    /// The section itself *is* the retries table: `[profile.ci.retries]`.
    IsRetries,
    /// A section whose `retries` key binds the gate.
    MayDeclareRetries,
    /// Not this contract.
    Unrelated,
}

/// Classifies a TOML section header against the profile the mandated gate uses.
///
/// Structural rather than a list of literal spellings, because the shapes that
/// bind the gate are a *family*, not a fixed set: `retries` is a key under the
/// profile, and also a legitimate sub-table (`[profile.ci.retries]`, nextest's
/// documented retries-with-backoff form), and both `ci` and `default` carry it
/// because `--profile ci` inherits `default`.
///
/// Verified against cargo-nextest 0.9.140 by counting `TRY` lines from a
/// deliberately failing test. Binding: `[profile.ci]`, `[profile.default]`,
/// either profile's `[[...overrides]]`, and either profile's `.retries`
/// sub-table. Not binding, and correctly ignored: `[profile.local]`,
/// `[profile.ci-extra]`, `[profile.default-miri]` — measured, all run a single
/// attempt under `--profile ci`.
fn classify_gate_section(header: &str) -> GateSection {
    // A trailing comment is ordinary TOML: `[profile.ci] # keep empty` is still
    // the `ci` section, and comparing the whole line would miss it.
    let header = header.split('#').next().unwrap_or(header).trim();
    let inner = match header
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
    {
        Some(inner) => inner,
        None => match header.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            Some(inner) => inner,
            None => return GateSection::Unrelated,
        },
    };
    let mut parts = inner.split('.').map(str::trim);
    if parts.next() != Some("profile") {
        return GateSection::Unrelated;
    }
    // `--profile ci` resolves against `ci` and the `default` it inherits.
    if !matches!(parts.next(), Some("ci" | "default")) {
        return GateSection::Unrelated;
    }
    match (parts.next(), parts.next()) {
        (None, _) | (Some("overrides"), None) => GateSection::MayDeclareRetries,
        (Some("retries"), None) => GateSection::IsRetries,
        _ => GateSection::Unrelated,
    }
}

/// A gate-binding section whose `inherits` points somewhere this check cannot
/// follow, as `(section, parent)`.
///
/// nextest lets any profile name a parent (`inherits = "shared"`), and the
/// chain is transitive: measured, `[profile.ci] inherits = "a"`, `[profile.a]
/// inherits = "b"`, `[profile.b] retries = 2` runs a failing test 3 times under
/// `--profile ci`. Following an arbitrary chain is more parsing than this guard
/// is willing to own, so an unfollowable parent FAILS CLOSED: silence about a
/// chain it cannot read must not be reported as "no retries".
///
/// `inherits = "default"` is allowed because it is the implicit parent anyway,
/// and `[profile.default]` is already classified.
fn gate_section_with_unfollowable_parent(text: &str) -> Option<(String, String)> {
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let header = line.split('#').next().unwrap_or(line).trim().to_owned();
            current = match classify_gate_section(line) {
                GateSection::Unrelated => None,
                _ => Some(header),
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "inherits" {
            continue;
        }
        let parent = value
            .split('#')
            .next()
            .unwrap_or(value)
            .trim()
            .trim_matches(['"', '\''])
            .to_owned();
        if parent != "default" {
            if let Some(section) = current.clone() {
                return Some((section, parent));
            }
        }
    }
    None
}

/// The gate-binding section that declares `retries`, if any.
///
/// Returns the header so the error can name the section it actually found: a
/// message hard-coding `[profile.ci]` sends a reader to the wrong line when the
/// key is under `[profile.default]`.
fn gate_section_setting_retries(text: &str) -> Option<String> {
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let header = line.split('#').next().unwrap_or(line).trim().to_owned();
            match classify_gate_section(line) {
                // The section header *is* the declaration; there is no key to find.
                GateSection::IsRetries => return Some(header),
                GateSection::MayDeclareRetries => current = Some(header),
                GateSection::Unrelated => current = None,
            }
            continue;
        }
        if line
            .split('=')
            .next()
            .is_some_and(|key| key.trim() == "retries")
        {
            if let Some(header) = current.clone() {
                return Some(header);
            }
        }
    }
    None
}

fn check_gitignore(root: &Path) -> Result<(), String> {
    let text = read(root, GITIGNORE)?;
    if github_dir_is_ignored(&text) {
        return Err(
            "CI-HYGIENE: `.gitignore` ignores `/.github/`, so GitHub never sees workflows, \
             CODEOWNERS, or templates. Remove that ignore rule (KEL-39 publishes CI)."
                .to_owned(),
        );
    }
    Ok(())
}

fn check_codeowners(root: &Path) -> Result<(), String> {
    let text = read(root, CODEOWNERS)?;
    for needle in REQUIRED_OWNER_PATHS {
        if !codeowners_covers(&text, needle) {
            return Err(format!(
                "CI-HYGIENE: `{CODEOWNERS}` has no owned path containing `{needle}`. \
                 Add that path with at least one `@user` or `@org/team` owner."
            ));
        }
    }
    Ok(())
}

fn check_pr_template(root: &Path) -> Result<(), String> {
    let text = read(root, PR_TEMPLATE)?;
    for needle in PR_NEEDLES {
        if !text.contains(needle) {
            return Err(format!(
                "CI-HYGIENE: `{PR_TEMPLATE}` is missing `{needle}`. \
                 Restore the six required PR headings from .agents/ci.md § KeldBot \
                 and the verification-gate commands from the justfile `ci` recipe."
            ));
        }
    }
    Ok(())
}

fn check_issue_templates(root: &Path) -> Result<(), String> {
    let dir = root.join(ISSUE_DIR);
    let entries = fs::read_dir(&dir).map_err(|error| {
        format!(
            "CI-HYGIENE: missing `{ISSUE_DIR}`: {error}. \
             Add at least one GitHub issue template that mentions the verification gate."
        )
    })?;
    let mut found = false;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("CI-HYGIENE: cannot read `{ISSUE_DIR}`: {error}. Check directory permissions.")
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "config.yml" || name == "config.yaml" {
            continue;
        }
        if name.ends_with(".md") || name.ends_with(".yml") || name.ends_with(".yaml") {
            found = true;
            break;
        }
    }
    if !found {
        return Err(format!(
            "CI-HYGIENE: `{ISSUE_DIR}` has no bug/feature template. \
             Add a `.yml` or `.md` template (not only `config.yml`)."
        ));
    }
    Ok(())
}

fn check_workflow(root: &Path) -> Result<(), String> {
    let text = read(root, WORKFLOW)?;
    let _required_evaluator = read(root, CI_REQUIRED_EVALUATOR)?;
    let _atomic_protocol_checker = read(root, ATOMIC_PROTOCOL_CHECKER)?;
    let _agent_context_checker = read(root, AGENT_CONTEXT_CHECKER)?;
    if let Some(key) = forbidden_workflow_control_key(&text) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` must not set `{key}` at workflow, job, or step scope. Required jobs use the default failure-preserving shell and must expose every failing exit status."
        ));
    }
    for needle in WORKFLOW_TEXT_NEEDLES {
        if !uncommented_line_contains(&text, needle) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` is missing `{needle}`. \
                 Restore the gitleaks job (checksummed CLI, not the org-licensed Action), \
                 `with: toolchain:` on dtolnay/rust-toolchain, \
                the hygiene job that compiles this file, \
                 generated-doc freshness, and the structural plus digest-pinned Mermaid render gates."
            ));
        }
    }

    check_change_router_job(&text)?;
    check_package_loop_shell(&text)?;
    check_check_job_if_avoids_matrix(&text)?;
    check_fuzz_workspace_step(&text)?;
    check_msrv_avoids_apt(&text)?;
    check_bun_test_job(&text)?;
    check_linux_media_guard_step(&text)?;
    check_required_job(&text)?;
    check_product_status_step(&text)?;
    check_product_status_windows_step(&text)?;
    check_windows_media_acceptance_step(&text)?;
    check_atomic_protocol_step(&text)?;
    check_agent_context_step(&text)?;
    for needle in WORKFLOW_RUN_NEEDLES {
        if !workflow_has_executable_run_needle(&text, needle) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` is missing executable Mermaid gate `{needle}`. Restore it in a `run:` step; comments and echo text do not execute the gate."
            ));
        }
    }

    Ok(())
}

fn check_mermaid_gate_files(root: &Path) -> Result<(), String> {
    let _checker = read(root, MERMAID_CHECKER)?;
    let renderer = read(root, MERMAID_RENDERER)?;
    for needle in [
        MERMAID_IMAGE_DIGEST,
        "--network none",
        "--read-only",
        "--cap-drop ALL",
        "--security-opt no-new-privileges",
        "--memory 2g",
        "--pids-limit 256",
        "run_with_timeout 120 docker run",
        "run_with_timeout 300 docker pull",
        "--pull never",
        "--jobs 2",
        ":/input/source.md:ro",
        "docker_host_path",
        "MSYS2_ARG_CONV_EXCL='*'",
        "trap cleanup EXIT",
        r#"workspace=$(cd "$workspace" && pwd -P)"#,
        r#"if [[ -L "$workspace/target" ]]; then"#,
        r#"render_parent=$(cd "$workspace/target" && pwd -P)"#,
        r#"[[ "$render_parent" == "$workspace/target" ]] || {"#,
        r#""$render_parent"/keld-mermaid-render.*)"#,
        r#"if ! rm -rf -- "$render_dir"; then"#,
    ] {
        if !uncommented_line_contains(&renderer, needle) {
            return Err(format!(
                "CI-HYGIENE: `{MERMAID_RENDERER}` is missing `{needle}`. Restore the pinned, network-disabled, read-only, resource-bounded renderer contract."
            ));
        }
    }
    check_mermaid_msys_structure(&renderer)?;
    let config = read(root, MERMAID_CONFIG)?;
    for needle in [
        "\"securityLevel\": \"strict\"",
        "\"maxTextSize\"",
        "\"maxEdges\"",
        "\"deterministicIds\": true",
    ] {
        if !config.contains(needle) {
            return Err(format!(
                "CI-HYGIENE: `{MERMAID_CONFIG}` is missing `{needle}`. Restore the strict deterministic resource-limit configuration."
            ));
        }
    }
    Ok(())
}

fn check(root: &Path) -> Result<(), String> {
    check_gitignore(root)?;
    check_ci_profile_does_not_retry(root)?;
    check_codeowners(root)?;
    check_pr_template(root)?;
    check_issue_templates(root)?;
    check_workflow(root)?;
    check_keldbot_workflow(root)?;
    check_mermaid_gate_files(root)?;
    Ok(())
}

fn run_cli() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        "CI-HYGIENE: missing command. Run `ci-hygiene check [workspace]`.".to_owned()
    })?;
    let root = args
        .next()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if args.next().is_some() {
        return Err(
            "CI-HYGIENE: too many arguments. Run `ci-hygiene check [workspace]`.".to_owned(),
        );
    }
    match command.as_str() {
        "check" => {
            check(&root)?;
            let status = std::process::Command::new("bun")
                .arg("--no-install")
                .arg(root.join("tools/ci_workflow_security.ts"))
                .arg("check")
                .arg(&root)
                .stdin(std::process::Stdio::null())
                .status()
                .map_err(|error| format!("CI-HYGIENE: cannot run workflow semantic check: {error}. Install the repository Bun prerequisite and rerun just hygiene."))?;
            if !status.success() {
                return Err(format!(
                    "CI-HYGIENE: workflow semantic check failed ({status}); restore the reported security contract before rerunning."
                ));
            }
            Ok(())
        }
        _ => Err(format!(
            "CI-HYGIENE: unknown command `{command}`. Use `check` to verify KEL-39 files."
        )),
    }
}

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!("keld-ci-hygiene-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create isolated fixture root");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.path.join(relative);
            fs::create_dir_all(path.parent().expect("fixture path has parent"))
                .expect("create fixture parent");
            fs::write(path, contents).expect("write fixture");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    const PINNED_CHECKOUT: &str =
        "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0\n";

    fn windows_media_step() -> String {
        let commands = WINDOWS_MEDIA_ACCEPTANCE_COMMANDS
            .iter()
            .map(|command| format!("          {command}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "      - name: Windows media guard acceptance (KEL-132)\n        if: matrix.os == 'windows-latest' && contains(needs.changes.outputs.packages, 'keld-wv')\n        shell: pwsh\n        run: |\n{commands}"
        )
    }

    // Fixture for Rust-owned contracts; parsed security cases use the real workflow in Bun.
    fn valid_workflow() -> String {
        let windows_media_step = windows_media_step();
        [
            "name: CI",
            "jobs:",
            "  changes:",
            "    name: change router",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          fetch-depth: 0",
            "          persist-credentials: false",
            "      - name: Router contract tests",
            "        run: tools/ci_changes_test.sh",
            "      - name: Classify changed-path ownership",
            "        run: tools/ci_changes.sh github",
            "      - name: Product status contract",
            "        run: |",
            "          mkdir -p target/product-status",
            "          rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test",
            "          target/product-status/product-status-test",
            "          rustc --edition=2024 -D warnings tools/product_status.rs -o target/product-status/product-status",
            "          target/product-status/product-status check .",
            "  check:",
            "    steps:",
            "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6",
            "        with:",
            "          bun-version: '1.4.2'",
            "      - name: Check keld-ipc fuzz workspace",
            "        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'",
            "        run: |",
            "          cargo check --manifest-path crates/keld-ipc/fuzz/Cargo.toml",
            "      - name: clippy (warnings deny)",
            "        shell: bash",
            "        run: cargo clippy -p fixture --all-targets -- -D warnings",
            "      - name: test",
            "        shell: bash",
            "        run: cargo nextest run -p fixture --profile ci --no-tests=pass",
            "      - name: Product status Windows path contracts",
            "        if: matrix.os == 'windows-latest'",
            "        shell: bash",
            "        run: |",
            "          mkdir -p target/product-status",
            "          rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test",
            "          target/product-status/product-status-test",
            windows_media_step.as_str(),
            "  bun-test:",
            "    if: needs.changes.outputs.ts == 'true'",
            "    steps:",
            "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0",
            "        with:",
            "          bun-version: \"1.4.2\"",
            "      - name: bun test",
            "        shell: bash",
            "        env:",
            "          KELD_CI_TS_PACKAGES: ${{ needs.changes.outputs.ts_packages }}",
            "        run: cd fixture && bun test",
            "  linux-gui-smoke:",
            "    steps:",
            "      - name: Build Linux media guard probe",
            "        run: |",
            "          cargo build -p keld-wv --example linux_media_guard",
            "          cc -shared -fPIC -Wall -Wextra -Werror -Wpedantic $(pkg-config --cflags webkit2gtk-4.1) crates/keld-wv/tests/fixtures/linux_media_interpose.c -o \"$RUNNER_TEMP/linux_media_interpose.so\" -ldl $(pkg-config --libs webkit2gtk-4.1)",
            "      - name: Xvfb GUI smoke test — title, controls, and clean close",
            "        run: xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host",
            "  msrv:",
            "    runs-on: macos-latest",
            "    steps:",
            "      - run: cargo +1.97 check -p fixture --all-targets",
            "  secrets:",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          persist-credentials: false",
            "      - uses: dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772 # master 2026-08-05",
            "        with:",
            "          toolchain: 1.97.1",
            "      - run: echo 551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb | sha256sum -c -",
            "      - run: gitleaks detect --source . --exit-code 1",
            "  hygiene:",
            "    if: needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          persist-credentials: false",
            "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0",
            "        with:",
            "          bun-version: \"1.4.2\"",
            "      - name: Atomic problem-solving protocol contract",
            "        run: |",
            "          mkdir -p target/atomic-protocol",
            "          rustc --edition=2024 -D warnings --test tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol-test",
            "          target/atomic-protocol/atomic-protocol-test",
            "          rustc --edition=2024 -D warnings tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol",
            "          target/atomic-protocol/atomic-protocol check .",
            "      - name: Agent instruction context budget",
            "        run: |",
            "          mkdir -p target/agent-context",
            "          rustc --edition=2024 -D warnings --test tools/agent_context.rs -o target/agent-context/agent-context-test",
            "          target/agent-context/agent-context-test",
            "          rustc --edition=2024 -D warnings tools/agent_context.rs -o target/agent-context/agent-context",
            "          target/agent-context/agent-context check .",
            "          python3 -B tools/test_session_closeout.py",
            "          python3 -B tools/test_session_closeout_hook.py",
            "      - run: rustc --edition=2024 --test tools/ci_hygiene.rs",
            "      - run: rustc --edition=2024 --test tools/product_status.rs",
            "      - run: product-status check .",
            "      - run: rustc --edition=2024 --test tools/llms_docs.rs",
            "      - run: rustc --edition=2024 tools/llms_docs.rs",
            "      - run: llms-docs check .",
            "      - run: rustc --edition=2024 --test tools/mermaid_docs.rs",
            "      - run: mermaid-docs check .",
            "      - run: tools/mermaid_render_check.sh # sha256:29077c6bd02f14bdfdd5fee552d9c00fe68d4fab3cd84952d21e2d1faf2fadaf",
            "  required:",
            "    name: CI required",
            "    if: ${{ always() }}",
            "    needs:",
            "      - changes",
            "      - fmt",
            "      - check",
            "      - bun-test",
            "      - linux-gui-smoke",
            "      - msrv",
            "      - deny",
            "      - secrets",
            "      - hygiene",
            "      - codeql",
            "      - dependency-review",
            "    steps:",
            "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "        with:",
            "          persist-credentials: false",
            "      - name: Verify required CI results",
            "        env:",
            "          KELD_RESULT_CHANGES: ${{ needs.changes.result }}",
            "          KELD_RESULT_FMT: ${{ needs.fmt.result }}",
            "          KELD_RESULT_CHECK: ${{ needs.check.result }}",
            "          KELD_RESULT_BUN: ${{ needs['bun-test'].result }}",
            "          KELD_RESULT_GUI: ${{ needs['linux-gui-smoke'].result }}",
            "          KELD_RESULT_MSRV: ${{ needs.msrv.result }}",
            "          KELD_RESULT_DENY: ${{ needs.deny.result }}",
            "          KELD_RESULT_SECRETS: ${{ needs.secrets.result }}",
            "          KELD_RESULT_HYGIENE: ${{ needs.hygiene.result }}",
            "          KELD_ROUTE_RUST: ${{ needs.changes.outputs.rust }}",
            "          KELD_ROUTE_TS: ${{ needs.changes.outputs.ts }}",
            "          KELD_ROUTE_GUI: ${{ needs.changes.outputs.gui }}",
            "          KELD_ROUTE_MSRV: ${{ needs.changes.outputs.msrv }}",
            "          KELD_ROUTE_DENY: ${{ needs.changes.outputs.deny }}",
            "          KELD_ROUTE_HYGIENE: ${{ needs.changes.outputs.hygiene }}",
            "          KELD_ROUTE_DOCS: ${{ needs.changes.outputs.docs }}",
            "          KELD_RESULT_CODEQL: ${{ needs.codeql.result }}",
            "          KELD_RESULT_DEPENDENCY_REVIEW: ${{ needs['dependency-review'].result }}",
            "        run: |",
            "          tools/ci_required.sh test",
            "          tools/ci_required.sh check \\",
            "            \"$KELD_RESULT_CHANGES\" \"$KELD_RESULT_FMT\" \"$KELD_RESULT_CHECK\" \\",
            "            \"$KELD_RESULT_BUN\" \"$KELD_RESULT_GUI\" \"$KELD_RESULT_MSRV\" \\",
            "            \"$KELD_RESULT_DENY\" \"$KELD_RESULT_SECRETS\" \"$KELD_RESULT_HYGIENE\" \\",
            "            \"$KELD_ROUTE_RUST\" \"$KELD_ROUTE_TS\" \"$KELD_ROUTE_GUI\" \\",
            "            \"$KELD_ROUTE_MSRV\" \"$KELD_ROUTE_DENY\" \\",
            "            \"$KELD_ROUTE_HYGIENE\" \"$KELD_ROUTE_DOCS\" \\",
            "            \"$KELD_RESULT_CODEQL\" \"$KELD_RESULT_DEPENDENCY_REVIEW\"",
            "",
        ]
        .join("\n")
    }

    fn valid_keldbot_workflow() -> String {
        [
            "name: KeldBot",
            "on:",
            "  pull_request_target:",
            "    types: [opened, edited, synchronize]",
            "permissions: {}",
            "jobs:",
            "  gatekeeper:",
            "    runs-on: ubuntu-latest",
            "    steps:",
            "      - uses: actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3 # v9.0.0",
            "        with:",
            "          script: |",
            "            const REQUIRED = [\"Summary\", \"Spec refs\", \"Review gates\", \"Tests\", \"Platforms\", \"Perf impact\"];",
            "  title-lint:",
            "    runs-on: ubuntu-latest",
            "    steps:",
            "      - uses: actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3 # v9.0.0",
            "        with:",
            "          script: |",
            "            core.info('title');",
            "",
        ]
        .join("\n")
    }

    fn valid_codeowners() -> &'static str {
        "/Cargo.toml @alice\n\
         /crates/keld-guard/ @alice\n\
         /crates/keld-ipc/ @alice\n\
         /.github/ @alice\n\
         /AGENTS.md @alice\n\
         /.agents/ @alice\n\
         /.codex/ @alice\n\
         /docs/agents/ @alice\n\
         /docs/engineering/keld-error-codes.md @alice\n\
         /justfile @alice\n\
         /tools/ @alice\n"
    }

    fn valid_pr() -> &'static str {
        "## Summary\n## Spec refs\nNo boundary change\n## Review gates\n\
         ## Tests\nRun cargo fmt, clippy, nextest, and mermaid-render-check.\n\
         ## Platforms\n## Perf impact\nagent/kel-\n"
    }

    fn complete_fixture() -> TempDir {
        let temp = TempDir::new();
        temp.write(".gitignore", "/target\n/.claude\n");
        temp.write(CODEOWNERS, valid_codeowners());
        temp.write(PR_TEMPLATE, valid_pr());
        temp.write(
            ".github/ISSUE_TEMPLATE/bug.yml",
            "name: Bug\nbody:\n  - type: markdown\n",
        );
        temp.write(WORKFLOW, &valid_workflow());
        temp.write(KELDBOT_WORKFLOW, &valid_keldbot_workflow());
        temp.write(CI_REQUIRED_EVALUATOR, "#!/usr/bin/env bash\nexit 0\n");
        temp.write(ATOMIC_PROTOCOL_CHECKER, "fn main() {}\n");
        temp.write(AGENT_CONTEXT_CHECKER, "fn main() {}\n");
        temp.write(MERMAID_CHECKER, "fn main() {}\n");
        temp.write(NEXTEST_CONFIG, "[profile.ci]\n");
        temp.write(MERMAID_RENDERER, include_str!("mermaid_render_check.sh"));
        temp.write(
            MERMAID_CONFIG,
            "{\"securityLevel\": \"strict\", \"maxTextSize\": 50000, \"maxEdges\": 500, \"deterministicIds\": true}\n",
        );
        temp
    }

    #[test]
    fn complete_fixture_passes() {
        let temp = complete_fixture();
        check(temp.path()).expect("complete KEL-39 fixture must pass");
    }

    #[test]
    fn linux_media_guard_commands_are_mandatory_in_the_gui_lane() {
        for (needle, replacement) in [
            (
                "          cargo build -p keld-wv --example linux_media_guard\n",
                "          removed-media-build\n",
            ),
            (
                "crates/keld-wv/tests/fixtures/linux_media_interpose.c",
                "removed-media-interposer.c",
            ),
            ("-Wall -Wextra -Werror -Wpedantic", "-Wall -Wextra"),
            (
                "xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host",
                "removed-media-run",
            ),
        ] {
            let temp = complete_fixture();
            let workflow = valid_workflow().replacen(needle, replacement, 1);
            temp.write(WORKFLOW, &workflow);
            let error = check(temp.path()).expect_err("missing media command must fail");
            assert!(error.contains("KEL-132"), "{needle}: {error}");
        }
    }

    #[test]
    fn linux_media_guard_rejects_inert_or_conditional_commands() {
        for (needle, replacement) in [
            (
                "          cargo build -p keld-wv --example linux_media_guard\n",
                "          echo cargo build -p keld-wv --example linux_media_guard\n",
            ),
            (
                "      - name: Build Linux media guard probe\n        run: |\n",
                "      - name: Build Linux media guard probe\n        if: false\n        run: |\n",
            ),
            (
                "        run: xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host\n",
                "        run: |\n          exit\n          xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host\n",
            ),
            (
                "        run: xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host\n",
                "        run: |\n          if [ 1 -eq 0 ]; then\n            xvfb-run --auto-servernum --server-args='-screen 0 1024x768x24' crates/keld-wv/tests/linux_gui_smoke.sh \"$RUNNER_TEMP/linux_media_interpose.so\" target/debug/examples/linux_media_guard ./target/release/keld-host\n          fi\n",
            ),
        ] {
            let temp = complete_fixture();
            let workflow = valid_workflow().replacen(needle, replacement, 1);
            temp.write(WORKFLOW, &workflow);
            let error = check(temp.path()).expect_err("inert media command must fail");
            assert!(error.contains("KEL-132"), "{needle}: {error}");
        }
    }

    #[test]
    fn required_result_job_is_mandatory_and_always_created() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("  required:\n", "  removed-required:\n", 1),
        );
        let error = check(temp.path()).expect_err("missing required result must fail");
        assert!(error.contains("required"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("    if: ${{ always() }}", "    if: ${{ success() }}", 1),
        );
        let error = check(temp.path()).expect_err("non-always required result must fail");
        assert!(error.contains("always"), "{error}");
    }

    #[test]
    fn atomic_protocol_step_is_mandatory_and_failure_preserving() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: Atomic problem-solving protocol contract\n",
                "      - name: Removed atomic step\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("missing atomic step must fail");
        assert!(error.contains("Atomic problem-solving"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "          target/atomic-protocol/atomic-protocol check .",
                "          echo target/atomic-protocol/atomic-protocol check .",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("echoed atomic check must fail");
        assert!(error.contains("without wrappers"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: Atomic problem-solving protocol contract\n        run:",
                "      - name: Atomic problem-solving protocol contract\n        if: ${{ false }}\n        run:",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("skipped atomic step must fail");
        assert!(error.contains("may contain only"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "    if: needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'",
                "    if: needs.changes.outputs.hygiene == 'true'",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("docs-only atomic skip must fail");
        assert!(error.contains("both hygiene and docs"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "  hygiene:\n    if: needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'",
                "  hygiene:\n    if: needs.changes.outputs.hygiene == 'true' || needs.changes.outputs.docs == 'true'\n    continue-on-error: true",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("hygiene failure suppression must fail");
        assert!(error.contains("continue-on-error"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "  hygiene:\n    if:",
                "  hygiene:\n    \"continue-on-error\": true\n    if:",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("quoted failure suppression must fail");
        assert!(error.contains("continue-on-error"), "{error}");

        for (needle, replacement, label) in [
            (
                "  hygiene:\n    if:",
                "  hygiene:\n    defaults:\n      run:\n        shell: bash {0} || true\n    if:",
                "job inherited shell",
            ),
            (
                "jobs:\n",
                "defaults:\n  run:\n    shell: bash {0} || true\njobs:\n",
                "workflow inherited shell",
            ),
        ] {
            temp.write(WORKFLOW, &valid_workflow().replacen(needle, replacement, 1));
            let error = check(temp.path()).expect_err(label);
            assert!(error.contains("defaults"), "{error}");
        }

        let real_step = concat!(
            "      - name: Atomic problem-solving protocol contract\n",
            "        run: |\n",
            "          mkdir -p target/atomic-protocol\n",
            "          rustc --edition=2024 -D warnings --test tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol-test\n",
            "          target/atomic-protocol/atomic-protocol-test\n",
            "          rustc --edition=2024 -D warnings tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol\n",
            "          target/atomic-protocol/atomic-protocol check .\n",
        );
        let decoy_step = concat!(
            "      - name: Disabled decoy\n",
            "        if: ${{ false }}\n",
            "        run: |\n",
            "          - name: Atomic problem-solving protocol contract\n",
            "            run: |\n",
            "              mkdir -p target/atomic-protocol\n",
            "              rustc --edition=2024 -D warnings --test tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol-test\n",
            "              target/atomic-protocol/atomic-protocol-test\n",
            "              rustc --edition=2024 -D warnings tools/atomic_protocol.rs -o target/atomic-protocol/atomic-protocol\n",
            "              target/atomic-protocol/atomic-protocol check .\n",
        );
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(real_step, decoy_step, 1),
        );
        let error = check(temp.path()).expect_err("nested decoy step must fail");
        assert!(error.contains("exactly one"), "{error}");
    }

    #[test]
    fn agent_context_step_is_mandatory_and_failure_preserving() {
        let temp = complete_fixture();
        let context_step = concat!(
            "      - name: Agent instruction context budget\n",
            "        run: |\n",
            "          mkdir -p target/agent-context\n",
            "          rustc --edition=2024 -D warnings --test tools/agent_context.rs -o target/agent-context/agent-context-test\n",
            "          target/agent-context/agent-context-test\n",
            "          rustc --edition=2024 -D warnings tools/agent_context.rs -o target/agent-context/agent-context\n",
            "          target/agent-context/agent-context check .\n",
            "          python3 -B tools/test_session_closeout.py\n",
            "          python3 -B tools/test_session_closeout_hook.py\n",
        );
        temp.write(WORKFLOW, &valid_workflow().replacen(context_step, "", 1));
        let error = check(temp.path()).expect_err("missing context gate must fail");
        assert!(
            error.contains("Agent instruction context budget"),
            "{error}"
        );

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "          target/agent-context/agent-context check .",
                "          echo target/agent-context/agent-context check .",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("echoed context gate must fail");
        assert!(error.contains("without wrappers"), "{error}");
    }

    #[test]
    fn hosted_closeout_suites_cannot_be_omitted_or_echoed() {
        for command in [
            "python3 -B tools/test_session_closeout.py",
            "python3 -B tools/test_session_closeout_hook.py",
        ] {
            for replacement in [
                String::new(),
                format!("echo {command}"),
                format!("{command} || true"),
            ] {
                let temp = complete_fixture();
                temp.write(
                    WORKFLOW,
                    &valid_workflow().replacen(command, &replacement, 1),
                );
                let error =
                    check(temp.path()).expect_err("missing or swallowed closeout tests must fail");
                assert!(error.contains("without wrappers"), "{error}");
            }
        }
    }

    #[test]
    fn required_result_must_observe_unconditional_gitleaks() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("      - secrets\n", "", 1),
        );
        let error = check(temp.path()).expect_err("missing gitleaks dependency must fail");
        assert!(error.contains("secrets"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow()
                .replacen("      - secrets\n", "", 1)
                .replacen(
                    "      - name: Verify required CI results\n        env:\n",
                    "      - name: Verify required CI results\n        env:\n          SPOOF: '- secrets'\n",
                    1,
                ),
        );
        let error = check(temp.path()).expect_err("needs text in env must not count");
        assert!(error.contains("secrets"), "{error}");
    }

    #[test]
    fn required_result_must_receive_router_applicability() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "          KELD_ROUTE_RUST: ${{ needs.changes.outputs.rust }}\n",
                "",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("missing router output must fail");
        assert!(error.contains("KELD_ROUTE_RUST"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("\"$KELD_ROUTE_TS\"", "false", 1),
        );
        let error = check(temp.path()).expect_err("unused router output must fail");
        assert!(error.contains("18-argument"), "{error}");
    }

    #[test]
    fn required_result_must_observe_security_jobs() {
        for job in ["codeql", "dependency-review"] {
            let workflow = valid_workflow().replacen(&format!("      - {job}\n"), "", 1);
            let error = check_required_job(&workflow).expect_err("missing security job must fail");
            assert!(error.contains(job), "{error}");
        }
    }

    #[test]
    fn required_result_must_receive_security_results_without_spoofing() {
        for (key, expression) in [
            ("KELD_RESULT_CODEQL", "${{ needs.codeql.result }}"),
            (
                "KELD_RESULT_DEPENDENCY_REVIEW",
                "${{ needs['dependency-review'].result }}",
            ),
        ] {
            let workflow = valid_workflow().replacen(expression, "success", 1);
            let error =
                check_required_job(&workflow).expect_err("spoofed security result must fail");
            assert!(error.contains(key), "{error}");
            let workflow = valid_workflow().replacen(&format!("\"${key}\""), "success", 1);
            let error =
                check_required_job(&workflow).expect_err("unused security result must fail");
            assert!(error.contains("18-argument"), "{error}");
        }
    }

    #[test]
    fn required_evaluator_text_must_be_executed() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "          tools/ci_required.sh check \\",
                "          echo tools/ci_required.sh check \\",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("echoed evaluator text must fail");
        assert!(error.contains("run block"), "{error}");
    }

    #[test]
    fn required_evaluator_rejects_false_green_shell_shapes() {
        let temp = complete_fixture();

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "          KELD_RESULT_SECRETS: ${{ needs.secrets.result }}",
                "          KELD_RESULT_SECRETS: success",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("literal gitleaks success must fail");
        assert!(error.contains("KELD_RESULT_SECRETS"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "            \"$KELD_ROUTE_HYGIENE\" \"$KELD_ROUTE_DOCS\"",
                "            \"$KELD_ROUTE_HYGIENE\" \"$KELD_ROUTE_DOCS\" || true",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("exit-status suppression must fail");
        assert!(error.contains("run block"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: Verify required CI results\n        env:",
                "      - name: Verify required CI results\n        continue-on-error: true\n        env:",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("continue-on-error must fail");
        assert!(error.contains("continue-on-error"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "  required:\n    name: CI required",
                "  required:\n    name: CI required\n    continue-on-error: true",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("job continue-on-error must fail");
        assert!(error.contains("continue-on-error"), "{error}");

        for (property, label) in [
            ("if: ${{ false }}", "skipped evaluator"),
            ("shell: bash {0} || true", "failure-suppressing shell"),
            ("\"if\": ${{ false }}", "quoted skipped evaluator"),
            (
                "\"shell\": bash {0} || true",
                "quoted failure-suppressing shell",
            ),
        ] {
            temp.write(
                WORKFLOW,
                &valid_workflow().replacen(
                    "      - name: Verify required CI results\n        env:",
                    &format!(
                        "      - name: Verify required CI results\n        {property}\n        env:"
                    ),
                    1,
                ),
            );
            let error = check(temp.path()).expect_err(label);
            assert!(error.contains("only direct"), "{error}");
        }

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "          tools/ci_required.sh check \\",
                "          if false; then\n          tools/ci_required.sh check \\",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("unreachable check must fail");
        assert!(error.contains("run block"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "          tools/ci_required.sh check \\",
                "          KELD_RESULT_SECRETS=success\n          tools/ci_required.sh check \\",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("result reassignment must fail");
        assert!(error.contains("run block"), "{error}");
    }

    #[test]
    fn keldbot_required_jobs_revalidate_synchronized_heads() {
        let temp = complete_fixture();
        temp.write(
            KELDBOT_WORKFLOW,
            &valid_keldbot_workflow().replacen(
                "  gatekeeper:\n",
                "  gatekeeper:\n    if: github.event.action != 'synchronize'\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("skipped gatekeeper must fail");
        assert!(error.contains("synchronize"), "{error}");

        temp.write(
            KELDBOT_WORKFLOW,
            &valid_keldbot_workflow().replace(", synchronize", ""),
        );
        let error = check(temp.path()).expect_err("missing synchronize trigger must fail");
        assert!(error.contains("synchronize"), "{error}");
    }

    #[test]
    fn keldbot_pull_request_target_never_checks_out_or_runs_pr_code() {
        let temp = complete_fixture();
        temp.write(
            KELDBOT_WORKFLOW,
            &valid_keldbot_workflow().replacen(
                "    steps:\n",
                "    steps:\n      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("secret-bearing checkout must fail");
        assert!(error.contains("checkout"), "{error}");
    }

    #[test]
    fn ci_profile_that_retries_is_rejected() {
        let temp = complete_fixture();
        temp.write(NEXTEST_CONFIG, "[profile.ci]\nretries = 1\n");
        let error = check(temp.path())
            .expect_err("the mandated verification profile must not retry (KEL-112)");
        assert!(error.contains("retries"), "{error}");
        assert!(error.contains(NEXTEST_CONFIG), "{error}");
    }

    #[test]
    fn the_inherited_default_profile_is_the_gate_too() {
        // `--profile ci` inherits `default`, and nextest's own documentation
        // puts `retries` there — so this is the likeliest accidental
        // regression, not an exotic one. Measured on cargo-nextest 0.9.140:
        // `[profile.default] retries = 2` with an empty `[profile.ci]` runs a
        // failing test 3 times under `--profile ci`.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.default]\nretries = 2\n\n[profile.ci]\n",
        );
        let error = check(temp.path())
            .expect_err("`ci` inherits `default`, so retries there bind the mandated gate");
        assert!(error.contains("retries"), "{error}");
    }

    #[test]
    fn an_override_that_retries_binds_the_gate() {
        // An override's `retries` applies to every test its filter matches.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci]\n\n[[profile.ci.overrides]]\nfilter = \"all()\"\nretries = 2\n",
        );
        let error = check(temp.path())
            .expect_err("an override under the mandated profile still retries it");
        assert!(error.contains("retries"), "{error}");
    }

    #[test]
    fn the_retries_sub_table_binds_the_gate() {
        // nextest's documented retries-with-backoff form is a sub-table, not a
        // key. Measured: `[profile.ci.retries]` with `count = 2` runs a failing
        // test 3 times under `--profile ci`. A guard matching literal section
        // names missed it entirely, because the section *is* the declaration.
        for header in ["[profile.ci.retries]", "[profile.default.retries]"] {
            let temp = complete_fixture();
            temp.write(
                NEXTEST_CONFIG,
                &format!("[profile.ci]\n\n{header}\ncount = 2\nbackoff = \"fixed\"\n"),
            );
            let error =
                check(temp.path()).expect_err("a retries sub-table retries the mandated gate");
            assert!(error.contains(header), "{error}");
        }
    }

    #[test]
    fn a_trailing_comment_does_not_hide_the_section() {
        // `[profile.ci] # keep this empty` is ordinary TOML and still the `ci`
        // section. Comparing the whole trimmed line against a literal header
        // silently skipped it while nextest retried.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci] # keep this empty\nretries = 2\n",
        );
        let error =
            check(temp.path()).expect_err("a commented header is still the mandated profile");
        assert!(error.contains("[profile.ci]"), "{error}");
    }

    #[test]
    fn an_inherited_override_that_retries_binds_the_gate() {
        // The fourth binding section. Its absence from the tests was why the
        // doc comment could claim all four were verified while one was not.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci]\n\n[[profile.default.overrides]]\nfilter = \"all()\"\nretries = 2\n",
        );
        let error = check(temp.path())
            .expect_err("an override on the inherited profile still retries the gate");
        assert!(error.contains("[[profile.default.overrides]]"), "{error}");
    }

    #[test]
    fn the_error_names_the_section_it_found() {
        // A message hard-coding `[profile.ci]` sends the reader to the wrong
        // line when the key is under the profile `ci` inherits.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.default]\nretries = 2\n\n[profile.ci]\n",
        );
        let error = check(temp.path()).expect_err("inherited retries bind the gate");
        assert!(error.contains("[profile.default]"), "{error}");
        assert!(
            !error.contains("under `[profile.ci]`"),
            "must not name a section it did not find: {error}"
        );
    }

    #[test]
    fn an_unfollowable_parent_fails_closed() {
        // nextest inheritance is transitive. Measured: `[profile.ci] inherits =
        // "a"`, `[profile.a] inherits = "b"`, `[profile.b] retries = 2` runs a
        // failing test 3 times under `--profile ci`. This guard does not follow
        // chains, so it must refuse rather than report silence as no-retries.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.shared]\nretries = 2\n\n[profile.ci]\ninherits = \"shared\"\n",
        );
        let error =
            check(temp.path()).expect_err("a parent this check cannot follow must fail closed");
        assert!(error.contains("shared"), "{error}");
        assert!(error.contains("cannot follow"), "{error}");
    }

    #[test]
    fn inheriting_the_implicit_default_parent_is_allowed() {
        // `default` is the implicit parent and is already classified, so naming
        // it explicitly must not be a false positive.
        let temp = complete_fixture();
        temp.write(NEXTEST_CONFIG, "[profile.ci]\ninherits = \"default\"\n");
        check(temp.path()).expect("inheriting the implicit parent is a no-op");
    }

    #[test]
    fn a_profile_that_does_not_bind_the_gate_may_retry() {
        // The no-false-positive side: measured, `[profile.local]` runs a single
        // attempt under `--profile ci`, so rejecting it would block a
        // legitimate local convenience for no gain.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci]\n\n[profile.local]\nretries = 3\n",
        );
        check(temp.path()).expect("a profile the gate does not inherit is not this contract");
    }

    #[test]
    fn ci_profile_without_retries_passes() {
        let temp = complete_fixture();
        temp.write(NEXTEST_CONFIG, "[profile.ci]\nfail-fast = false\n");
        check(temp.path()).expect("a ci profile with no retries is the contract");
    }

    #[test]
    fn retries_under_a_different_profile_is_not_this_contract() {
        // Guards over-matching: only the profile AGENTS.md mandates is bound.
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.local]\nretries = 3\n\n[profile.ci]\n",
        );
        check(temp.path()).expect("only `[profile.ci]` is the verification gate");
    }

    #[test]
    fn commented_out_retries_is_not_a_retry() {
        let temp = complete_fixture();
        temp.write(
            NEXTEST_CONFIG,
            "[profile.ci]\n# retries = 1 (removed, KEL-112)\n",
        );
        check(temp.path()).expect("a comment is documentation, not configuration");
    }

    #[test]
    fn router_test_in_another_job_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replacen("        run: tools/ci_changes_test.sh\n", "", 1)
            .replace(
                "  secrets:\n",
                "  unrelated:\n    steps:\n      - run: tools/ci_changes_test.sh\n  secrets:\n",
            );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("router test must run in changes job");
        assert!(error.contains("changes` job"), "{error}");
        assert!(error.contains("tools/ci_changes_test.sh"), "{error}");
    }

    #[test]
    fn router_command_in_another_job_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replacen("        run: tools/ci_changes.sh github\n", "", 1)
            .replace(
                "  secrets:\n",
                "  unrelated:\n    steps:\n      - run: tools/ci_changes.sh github\n  secrets:\n",
            );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("router command must run in changes job");
        assert!(error.contains("changes` job"), "{error}");
        assert!(error.contains("tools/ci_changes.sh github"), "{error}");
    }

    #[test]
    fn full_history_checkout_in_another_job_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replacen("          fetch-depth: 0\n", "", 1)
            .replacen(
                "  secrets:\n    steps:\n",
                &format!("  secrets:\n    steps:\n{PINNED_CHECKOUT}        with:\n          persist-credentials: false\n          fetch-depth: 0\n"),
                1,
            );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("changes job must fetch its own history");
        assert!(error.contains("changes` job"), "{error}");
        assert!(error.contains("fetch-depth: 0"), "{error}");
    }

    #[test]
    fn quoted_fetch_depth_in_changes_job_is_accepted() {
        for value in ["'0'", "\"0\""] {
            let temp = complete_fixture();
            temp.write(
                WORKFLOW,
                &valid_workflow().replacen("fetch-depth: 0", &format!("fetch-depth: {value}"), 1),
            );
            check(temp.path()).expect("quoted fetch depth is the same YAML scalar");
        }
    }

    #[test]
    fn shell_guarded_router_test_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "run: tools/ci_changes_test.sh",
                "run: false && tools/ci_changes_test.sh",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("disabled shell command must fail");
        assert!(error.contains("unconditional"), "{error}");
        assert!(error.contains("tools/ci_changes_test.sh"), "{error}");
    }

    #[test]
    fn github_if_guarded_router_command_does_not_satisfy_changes_contract() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: Classify changed-path ownership\n        run:",
                "      - name: Classify changed-path ownership\n        if: ${{ false }}\n        run:",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("disabled GitHub step must fail");
        assert!(error.contains("unconditional"), "{error}");
        assert!(error.contains("tools/ci_changes.sh github"), "{error}");
    }

    #[test]
    fn package_loop_requires_bash_on_windows_matrix() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("        shell: bash\n", "", 1),
        );
        let error = check(temp.path()).expect_err("package loop needs Bash on Windows");
        assert!(error.contains("clippy (warnings deny)"), "{error}");
        assert!(error.contains("shell: bash"), "{error}");
    }

    #[test]
    fn selected_zero_test_package_must_not_fail_nextest() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(" --no-tests=pass", "", 1),
        );
        let error = check(temp.path()).expect_err("zero-test package policy must remain explicit");
        assert!(error.contains("--no-tests=pass"), "{error}");
    }

    #[test]
    fn check_job_if_must_not_use_matrix() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "  check:\n    steps:",
                "  check:\n    if: needs.changes.outputs.rust == 'true' && matrix.os != 'ubuntu-latest'\n    steps:",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("job-level matrix.os must fail hygiene");
        assert!(error.contains("matrix"), "{error}");
        assert!(error.contains("check"), "{error}");
    }

    #[test]
    fn missing_fuzz_workspace_step_fails() {
        let temp = complete_fixture();
        let step = "      - name: Check keld-ipc fuzz workspace\n        if: matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'\n        run: |\n          cargo check --manifest-path crates/keld-ipc/fuzz/Cargo.toml\n";
        temp.write(WORKFLOW, &valid_workflow().replace(step, ""));
        let error = check(temp.path()).expect_err("missing fuzz workspace check must fail");
        assert!(error.contains("Check keld-ipc fuzz workspace"), "{error}");
    }

    #[test]
    fn fuzz_workspace_step_must_be_ubuntu_and_rust_routed() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replace(
                "matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'",
                "matrix.os == 'ubuntu-latest'",
            ),
        );
        let error = check(temp.path()).expect_err("fuzz workspace step needs both routing gates");
        assert!(error.contains("exact condition"), "{error}");
    }

    #[test]
    fn fuzz_workspace_step_must_run_the_exact_stable_check() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replace(
                "cargo check --manifest-path crates/keld-ipc/fuzz/Cargo.toml",
                "cargo check",
            ),
        );
        let error = check(temp.path()).expect_err("fuzz workspace step needs its direct command");
        assert!(
            error.contains("stable fuzz-workspace cargo check"),
            "{error}"
        );
    }

    #[test]
    fn msrv_must_not_apt_get_on_ubuntu() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "    runs-on: macos-latest\n",
                "    runs-on: ubuntu-latest\n    steps:\n      - run: sudo apt-get update\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("MSRV apt-get must fail hygiene");
        assert!(error.contains("msrv"), "{error}");
        assert!(
            error.contains("macos-latest") || error.contains("apt-get"),
            "{error}"
        );
    }

    #[test]
    fn deleting_the_bun_lane_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow();
        let start = workflow
            .find("  bun-test:")
            .expect("fixture has the Bun lane");
        let end = workflow.find("  msrv:").expect("fixture has the MSRV lane");
        let mut without_lane = workflow.clone();
        without_lane.replace_range(start..end, "");
        temp.write(WORKFLOW, &without_lane);
        let error = check(temp.path()).expect_err("removing the only bun test lane must fail");
        assert!(error.contains("bun-test"), "{error}");
    }

    #[test]
    fn bun_ci_accepts_requested_142_and_rejects_other_versions() {
        let requested = valid_workflow();
        assert!(requested.contains("bun-version: \"1.4.2\""));
        check_bun_test_job(&requested).expect("the requested stable CI pin must pass");
        for version in ["1.4.0", "latest", "1.4.2-canary.1", "1.4.20"] {
            let changed = requested.replace("\"1.4.2\"", &format!("\"{version}\""));
            assert!(
                check_bun_test_job(&changed).is_err(),
                "unexpected runtime {version} must not satisfy the CI pin"
            );
        }
    }

    #[test]
    fn bun_step_pins_accept_named_actions_and_ignore_job_strategy() {
        let workflow = valid_workflow()
            .replace("  check:\n    steps:", "  check:\n    strategy:\n      fail-fast: false\n      matrix:\n        os: [ubuntu-latest, macos-latest]\n    steps:")
            .replace("      - uses: oven-sh/setup-bun@", "      - name: install Bun\n        uses: oven-sh/setup-bun@");
        let temp = complete_fixture();
        temp.write(WORKFLOW, &workflow);
        check(temp.path()).expect("only direct steps own action inputs");
    }

    #[test]
    fn bun_pin_is_bound_to_the_action_input_not_job_text() {
        let correct = "          bun-version: \"1.4.2\"\n";
        for replacement in [
            "          bun-version: latest\n",
            "          # missing bun-version\n",
            "        env:\n          bun-version: \"1.4.2\"\n",
        ] {
            let workflow = valid_workflow().replace(correct, replacement).replace(
                "      - name: bun test\n",
                "      - run: |\n          echo 'bun-version: \"1.4.2\"'\n      - name: bun test\n",
            );
            let temp = complete_fixture();
            temp.write(WORKFLOW, &workflow);
            let error = check(temp.path()).expect_err("unrelated text cannot pin an action");
            assert!(error.contains("bun-test"), "{error}");
            assert!(error.contains("1.4.2"), "{error}");
        }
    }

    #[test]
    fn every_bun_setup_in_each_bun_consumer_requires_its_own_pin() {
        for job in ["check", "bun-test", "hygiene"] {
            let workflow = valid_workflow();
            let block = workflow_job_block(&workflow, job).expect("fixture job");
            for extra in [
                "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n        with:\n          bun-version: latest\n",
                "      - name: another Bun setup\n        uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n",
                "      - name: folded Bun action\n        'uses': >-\n          oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n        with:\n          bun-version: latest\n",
                "      - uses: \"oven-sh/setup-b\\u0075n@0c5077e51419868618aeaa5fe8019c62421857d6\"\n        with:\n          bun-version: latest\n",
                "      - uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n        with:\n          bun-version: '1.4.2'\n          \"bun-\\u0076ersion\": latest\n",
                "      - ? uses\n        : oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6\n        with:\n          bun-version: latest\n",
            ] {
                let temp = complete_fixture();
                temp.write(
                    WORKFLOW,
                    &workflow.replace(&block, &format!("{block}{extra}")),
                );
                let error = check(temp.path()).expect_err("every actual setup must be pinned");
                assert!(error.contains(job), "{error}");
                assert!(error.contains("1.4.2"), "{error}");
            }
        }
    }

    #[test]
    fn every_bun_consumer_requires_a_setup_action() {
        for job in ["check", "bun-test", "hygiene"] {
            let workflow = valid_workflow();
            let block = workflow_job_block(&workflow, job).expect("fixture job");
            let changed = block.replace("oven-sh/setup-bun@", "some-other/action@");
            let temp = complete_fixture();
            temp.write(WORKFLOW, &workflow.replace(&block, &changed));
            let error = check(temp.path()).expect_err("a version on another action is not Bun");
            assert!(error.contains(job), "{error}");
        }
    }

    #[test]
    fn floating_bun_version_in_bun_lane_fails() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen("bun-version: \"1.4.2\"", "bun-version: latest", 1),
        );
        let error = check(temp.path()).expect_err("an unpinned Bun runtime must fail");
        assert!(error.contains("bun-test"), "{error}");
        assert!(error.contains("1.4.2"), "{error}");
    }

    #[test]
    fn bun_lane_gated_on_another_router_output_fails() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "    if: needs.changes.outputs.ts == 'true'\n",
                "    if: needs.changes.outputs.rust == 'true'\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("the Bun lane must follow its own router output");
        assert!(error.contains("bun-test"), "{error}");
        assert!(error.contains("outputs.ts"), "{error}");
    }

    #[test]
    fn bun_lane_must_not_apt_get() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "      - name: bun test\n",
                "      - run: sudo apt-get update\n      - name: bun test\n",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("a second live apt lane must fail");
        assert!(error.contains("bun-test"), "{error}");
        assert!(error.contains("apt-get"), "{error}");
    }

    #[test]
    fn ignoring_github_dir_fails() {
        let temp = complete_fixture();
        temp.write(".gitignore", "/target\n/.github/\n");
        let error = check(temp.path()).expect_err("ignored .github must fail");
        assert!(error.contains("CI-HYGIENE"), "{error}");
        assert!(error.contains("/.github/"), "{error}");
        assert!(error.contains("Remove that ignore"), "{error}");
    }

    #[test]
    fn ignoring_github_star_pattern_fails() {
        let temp = complete_fixture();
        temp.write(".gitignore", "/target\n/.github/*\n");
        let error = check(temp.path()).expect_err("/.github/* must count as ignoring .github");
        assert!(error.contains("CI-HYGIENE"), "{error}");
        assert!(error.contains("Remove that ignore"), "{error}");
    }

    #[test]
    fn github_dir_ignore_patterns() {
        for pattern in [
            "/.github/",
            "/.github",
            ".github/",
            ".github",
            "/.github/**",
            "/.github/*",
            ".github/**",
            ".github/*",
        ] {
            assert!(
                github_dir_is_ignored(&format!("/target\n{pattern}\n")),
                "{pattern} must be treated as ignoring .github"
            );
        }
        assert!(!github_dir_is_ignored("/target\n/.claude\n"));
        assert!(!github_dir_is_ignored("# /.github/*\n/target\n"));
        assert!(!github_dir_is_ignored("/.github/workflows/ci.yml\n"));
    }

    #[test]
    fn comment_mentioning_github_ignore_is_not_an_ignore_rule() {
        let temp = complete_fixture();
        temp.write(
            ".gitignore",
            "# formerly /.github/ — CI is tracked (KEL-39)\n/target\n",
        );
        check(temp.path()).expect("comment must not count as an ignore rule");
    }

    #[test]
    fn echoed_mermaid_gate_does_not_satisfy_workflow_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "- run: tools/mermaid_render_check.sh",
            "- run: echo tools/mermaid_render_check.sh",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("echoed gate must not satisfy hygiene");
        assert!(error.contains("executable Mermaid gate"), "{error}");
        assert!(error.contains("mermaid_render_check.sh"), "{error}");
    }

    #[test]
    fn inline_comment_mermaid_gate_does_not_satisfy_workflow_contract() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "- run: tools/mermaid_render_check.sh",
            "- run: true # tools/mermaid_render_check.sh",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("commented gate must not satisfy hygiene");
        assert!(error.contains("executable Mermaid gate"), "{error}");
    }

    #[test]
    fn missing_guard_codeowners_path_fails() {
        let temp = complete_fixture();
        temp.write(
            CODEOWNERS,
            "/Cargo.toml @alice\n/crates/keld-ipc/ @alice\n/.github/ @alice\n",
        );
        let error = check(temp.path()).expect_err("missing guard path must fail");
        assert!(error.contains("keld-guard"), "{error}");
        assert!(error.contains("@user"), "{error}");
    }

    #[test]
    fn commented_codeowners_path_does_not_count() {
        let temp = complete_fixture();
        temp.write(
            CODEOWNERS,
            "# /crates/keld-guard/ @alice\n\
             /Cargo.toml @alice\n\
             /crates/keld-ipc/ @alice\n\
             /.github/ @alice\n",
        );
        let error = check(temp.path()).expect_err("commented owner line must fail");
        assert!(error.contains("keld-guard"), "{error}");
    }

    #[test]
    fn codeowners_path_without_owner_fails() {
        let temp = complete_fixture();
        temp.write(
            CODEOWNERS,
            "/Cargo.toml @alice\n\
             /crates/keld-guard/\n\
             /crates/keld-ipc/ @alice\n\
             /.github/ @alice\n",
        );
        let error = check(temp.path()).expect_err("path with no @owner must fail");
        assert!(error.contains("keld-guard"), "{error}");
    }

    #[test]
    fn missing_pr_gate_fails() {
        let temp = complete_fixture();
        temp.write(PR_TEMPLATE, "## Summary\n\nNo gates here.\n");
        let error = check(temp.path()).expect_err("incomplete PR template must fail");
        assert!(
            error.contains("Spec refs")
                || error.contains("Review gates")
                || error.contains("cargo fmt"),
            "{error}"
        );
        assert!(
            error.contains("verification-gate") || error.contains("required PR headings"),
            "{error}"
        );
    }

    #[test]
    fn missing_toolchain_pin_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace("toolchain: 1.97.1", "");
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("workflow without toolchain pin must fail");
        assert!(error.contains("toolchain: 1.97.1"), "{error}");
    }

    #[test]
    fn missing_llms_docs_check_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replace("--test tools/llms_docs.rs", "")
            .replace("tools/llms_docs.rs", "")
            .replace("llms-docs check .", "");
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("workflow without llms-docs check must fail");
        assert!(
            error.contains("tools/llms_docs.rs") || error.contains("llms-docs check"),
            "{error}"
        );
    }

    #[test]
    fn missing_product_status_check_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replace("--test tools/product_status.rs", "")
            .replace("product-status check .", "");
        temp.write(WORKFLOW, &workflow);
        let error =
            check(temp.path()).expect_err("workflow without product-status check must fail");
        assert!(
            error.contains("tools/product_status.rs") || error.contains("product-status check"),
            "{error}"
        );
    }

    #[test]
    fn conditional_product_status_step_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "      - name: Product status contract\n        run: |",
            "      - name: Product status contract\n        if: ${{ false }}\n        run: |",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("conditional product status must fail");
        assert!(error.contains("Product status contract"), "{error}");
    }

    #[test]
    fn echoed_product_status_command_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "          target/product-status/product-status check .",
            "          echo 'target/product-status/product-status check .'",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("echoed product status must fail");
        assert!(error.contains("Product status contract"), "{error}");
    }

    #[test]
    fn missing_product_status_windows_step_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "      - name: Product status Windows path contracts\n        if: matrix.os == 'windows-latest'\n        shell: bash\n        run: |\n          mkdir -p target/product-status\n          rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test\n          target/product-status/product-status-test\n",
            "",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("missing Windows status step must fail");
        assert!(error.contains("Windows path contracts"), "{error}");
    }

    #[test]
    fn product_status_windows_step_requires_bash() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "        if: matrix.os == 'windows-latest'\n        shell: bash\n        run: |",
            "        if: matrix.os == 'windows-latest'\n        run: |",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("the POSIX command block requires Bash");
        assert!(error.contains("shell: bash"), "{error}");
    }

    #[test]
    fn duplicate_product_status_windows_step_fails() {
        let temp = complete_fixture();
        let step = "      - name: Product status Windows path contracts\n        if: matrix.os == 'windows-latest'\n        shell: bash\n        run: |\n          mkdir -p target/product-status\n          rustc --edition=2024 -D warnings --test tools/product_status.rs -o target/product-status/product-status-test\n          target/product-status/product-status-test\n";
        let workflow = valid_workflow().replace(step, &format!("{step}{step}"));
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("duplicate Windows status steps must fail");
        assert!(error.contains("exactly one"), "{error}");
    }

    #[test]
    fn broadened_product_status_windows_condition_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "if: matrix.os == 'windows-latest'",
            "if: matrix.os == 'windows-latest' || true",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("broadened Windows condition must fail");
        assert!(error.contains("exact condition"), "{error}");
    }

    #[test]
    fn windows_media_acceptance_step_is_unique_and_direct() {
        let temp = complete_fixture();
        let windows_media_step = windows_media_step();
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(&windows_media_step, "", 1),
        );
        let error = check(temp.path()).expect_err("missing Windows media step must fail");
        assert!(error.contains("KEL-132"), "{error}");

        let duplicated = format!("{windows_media_step}\n{windows_media_step}");
        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(&windows_media_step, &duplicated, 1),
        );
        let error = check(temp.path()).expect_err("duplicate Windows media step must fail");
        assert!(error.contains("exactly one"), "{error}");

        temp.write(
            WORKFLOW,
            &valid_workflow().replacen(
                "        shell: pwsh\n        run: |\n          cargo clippy -p keld-wv",
                "        continue-on-error: true\n        shell: pwsh\n        run: |\n          cargo clippy -p keld-wv",
                1,
            ),
        );
        let error = check(temp.path()).expect_err("failure suppression must fail");
        assert!(
            error.contains("KEL-132") || error.contains("continue-on-error"),
            "{error}"
        );
    }

    #[test]
    fn windows_media_acceptance_condition_and_shell_are_exact() {
        for (needle, replacement) in [
            (
                "matrix.os == 'windows-latest' && contains(needs.changes.outputs.packages, 'keld-wv')",
                "matrix.os == 'windows-latest'",
            ),
            ("        shell: pwsh", "        shell: bash"),
        ] {
            let temp = complete_fixture();
            temp.write(WORKFLOW, &valid_workflow().replacen(needle, replacement, 1));
            let error = check(temp.path()).expect_err("weakened Windows media routing must fail");
            assert!(error.contains("KEL-132"), "{needle}: {error}");
        }
    }

    #[test]
    fn windows_media_acceptance_commands_cannot_be_removed_or_made_inert() {
        for (needle, replacement) in [
            ("--features media-acceptance", "--features default"),
            (
                "& $powerShellHost -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File crates/keld-wv/tests/windows_media_guard.ps1",
                "Write-Output crates/keld-wv/tests/windows_media_guard.ps1",
            ),
            (
                "& $powerShellHost -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File crates/keld-wv/tests/windows_media_guard.ps1",
                "& crates/keld-wv/tests/windows_media_guard.ps1",
            ),
            (
                "if ($LASTEXITCODE -ne 0) { throw 'media-acceptance Clippy failed' }",
                "Write-Output 'ignored Clippy status'",
            ),
            ("$fixture.Count -ne 1", "$fixture.Count -lt 0"),
            (
                "Test-Path -LiteralPath $resultPath -PathType Leaf",
                "Test-Path -LiteralPath $resultPath -PathType Container",
            ),
            (
                "@($result.rows).Count -ne 10",
                "@($result.rows).Count -lt 0",
            ),
            (
                "$result.source_executable -cne $fixturePath",
                "$result.source_executable -cne $result.source_executable",
            ),
            (
                "$result.executable_sha256 -cne $fixtureHash",
                "$result.executable_sha256 -cne $result.executable_sha256",
            ),
            (
                "$result.watchdog_probe.exit_code -ne 124",
                "$result.watchdog_probe.exit_code -ne $result.watchdog_probe.exit_code",
            ),
            (
                "$result.outer_deadline_probe.outer_timed_out -ne $true",
                "$result.outer_deadline_probe.outer_timed_out -eq $true",
            ),
            (
                "Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath",
                "Get-FileHash -Algorithm SHA256 -LiteralPath $evidenceExecutable",
            ),
            (
                "$watchdogStderr.Contains('KELD_MEDIA_TIMEOUT: fixture exceeded 0.1 seconds')",
                "$watchdogStderr.Contains('anything')",
            ),
            (
                "microphone/removed-guard/app-grants",
                "camera/removed-guard/app-grants",
            ),
            (
                "$_.profile_removed -ne $true",
                "$_.profile_removed -eq $true",
            ),
            ("Sort-Object -Unique -CaseSensitive", "Sort-Object -Unique"),
            ("Compare-Object -CaseSensitive", "Compare-Object"),
            (
                "$result.schema -cne 'keld.windows-media-fixture/v1'",
                "$result.schema -ne 'keld.windows-media-fixture/v1'",
            ),
            (
                "$result.capture_device -cne 'WebView2 synthetic development device'",
                "$result.capture_device -cne $result.capture_device",
            ),
            (
                "$result.system -cne [Environment]::OSVersion.VersionString",
                "$result.system -cne $result.system",
            ),
            (
                "$result.device -cne [Environment]::MachineName",
                "$result.device -cne $result.device",
            ),
            (
                "$_.receipt -cnotlike 'KELD_MEDIA_RESULT * case_ok=true'",
                "$_.receipt -notlike 'KELD_MEDIA_RESULT * case_ok=true'",
            ),
            (
                "$row.stderr_sha256 -cnotmatch $hashPattern",
                "$row.stderr_sha256 -cmatch $hashPattern",
            ),
            (
                "\"$($logReceipts[0])\" -cne $row.receipt",
                "\"$($logReceipts[0])\" -cne \"$($logReceipts[0])\"",
            ),
            (
                "Test-Path -LiteralPath $row.profile_path",
                "Test-Path -LiteralPath $resultPath",
            ),
            (
                "$profileReceipts.Count -ne 1",
                "$profileReceipts.Count -lt 0",
            ),
            (
                "(?<nonce>[1-9][0-9]*)",
                "(?<nonce>[0-9]+)",
            ),
            (
                "(?<adapter_tid>0|[1-9][0-9]*)",
                "(?<adapter_tid>[0-9]+)",
            ),
            (
                "$originPort -gt 65535",
                "$originPort -gt [uint32]::MaxValue",
            ),
            (
                "$fixtureTempRoot.StartsWith($sharedProfileRoot, [StringComparison]::OrdinalIgnoreCase)",
                "$false",
            ),
            (
                "$row.receipt -cmatch $receiptPattern",
                "$row.receipt -cnotmatch $receiptPattern",
            ),
            (
                "[int]$receiptFields.permission_kind -ne $row.permission_kind",
                "[int]$receiptFields.permission_kind -ne [int]$receiptFields.permission_kind",
            ),
            (
                "$seenNonces.ContainsKey($receiptFields.nonce)",
                "$false",
            ),
            (
                "$row.permission_kind -ne $expectedPermissionKind",
                "$row.permission_kind -ne $row.permission_kind",
            ),
            (
                "$row.outcome -cnotmatch $expectedOutcome",
                "$row.outcome -cnotmatch $row.outcome",
            ),
        ] {
            let temp = complete_fixture();
            temp.write(WORKFLOW, &valid_workflow().replacen(needle, replacement, 1));
            let error = check(temp.path()).expect_err("inert Windows media command must fail");
            assert!(error.contains("KEL-132"), "{needle}: {error}");
        }
    }

    #[test]
    fn missing_mermaid_render_workflow_fails() {
        let temp = complete_fixture();
        let workflow = valid_workflow()
            .replace("tools/mermaid_render_check.sh", "")
            .replace(MERMAID_IMAGE_DIGEST, "");
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("workflow without pinned render must fail");
        assert!(
            error.contains("mermaid_render_check.sh") || error.contains(MERMAID_IMAGE_DIGEST),
            "{error}"
        );
    }

    #[test]
    fn commented_mermaid_render_workflow_does_not_pass() {
        let temp = complete_fixture();
        let workflow = valid_workflow().replace(
            "- run: tools/mermaid_render_check.sh",
            "# - run: tools/mermaid_render_check.sh",
        );
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("commented render step must fail");
        assert!(error.contains("mermaid_render_check.sh"), "{error}");
    }

    #[test]
    fn weakened_mermaid_renderer_isolation_fails() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace("--network none", ""),
        );
        let error = check(temp.path()).expect_err("network-enabled renderer must fail");
        assert!(error.contains("--network none"), "{error}");
    }

    #[test]
    #[cfg(unix)]
    fn mermaid_renderer_rejects_target_symlink_before_creating_output() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};
        use std::process::Command;

        let temp = TempDir::new();
        let checkout = temp.path().join("checkout with spaces");
        let external = temp.path().join("external");
        fs::create_dir(&checkout).expect("checkout directory");
        fs::create_dir(&external).expect("external directory");
        symlink(&external, checkout.join("target")).expect("escaping target symlink");
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .arg(&checkout)
                .status()
                .expect("initialize fixture")
                .success()
        );
        fs::write(
            checkout.join("diagram.md"),
            "```mermaid\nflowchart LR\n A-->B\n```\n",
        )
        .expect("tracked diagram");
        assert!(
            Command::new("git")
                .args(["add", "diagram.md"])
                .current_dir(&checkout)
                .status()
                .expect("track diagram")
                .success()
        );
        fs::create_dir(checkout.join("tools")).expect("fixture tools");
        fs::write(checkout.join(MERMAID_CONFIG), "{}").expect("config fixture");
        temp.write("renderer.sh", include_str!("mermaid_render_check.sh"));
        temp.write("bin/docker", "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image) exit 0 ;;\ncontext) printf 'unix:///unused-kel188.sock\\n' ;;\n*) exit 42 ;;\nesac\n");
        let docker = temp.path().join("bin/docker");
        fs::set_permissions(&docker, fs::Permissions::from_mode(0o700)).expect("driver executable");
        let mut paths = vec![temp.path().join("bin")];
        paths.extend(env::split_paths(&env::var_os("PATH").expect("tool PATH")));
        let output = Command::new("bash")
            .arg(temp.path().join("renderer.sh"))
            .current_dir(&checkout)
            .env("PATH", env::join_paths(paths).expect("fixture PATH"))
            .output()
            .expect("execute real renderer script");
        assert!(!output.status.success());
        assert!(
            fs::read_dir(&external)
                .expect("external observation")
                .next()
                .is_none(),
            "renderer created output outside its checkout"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("non-symlink directory"),
            "wrong failure boundary: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    #[cfg(windows)]
    fn mermaid_native_dacl_does_not_depend_on_path_whoami() {
        use std::process::Command;

        let temp = TempDir::new();
        let parent = temp.path().join("target");
        let output_dir = parent.join("keld-mermaid-render.fixture");
        fs::create_dir_all(output_dir.join("child")).expect("retained directory fixture");
        fs::write(output_dir.join("child/diagnostic.txt"), "diagnostic")
            .expect("retained file fixture");
        // Exercise the owning script with real Windows ACL APIs. A colliding GNU-shaped
        // executable must not supply identity to the native restoration process.
        let renderer = include_str!("mermaid_render_check.sh");
        let start = renderer.find("running_under_msys() {").expect("MSYS owner");
        let end = renderer
            .find("\ndocker info >/dev/null")
            .expect("helper boundary");
        let script = format!(
            "set -euo pipefail\n{}\nrender_parent=$(cd \"$1\" && pwd -P)\nwhoami.exe() {{ echo 'GNU whoami rejects Windows arguments' >&2; return 64; }}\nrestore_docker_output_dir \"$render_parent/keld-mermaid-render.fixture\"\n",
            &renderer[start..end]
        );
        let output = Command::new("bash")
            .args(["-c", &script, "kel152-native-test"])
            .arg(&parent)
            .output()
            .expect("execute native restoration");
        assert!(
            output.status.success(),
            "native restoration failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let oracle = r#"
$ErrorActionPreference = 'Stop'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$root = Get-Item -LiteralPath $env:KELD_TEST_OUTPUT
$items = @($root) + @(Get-ChildItem -LiteralPath $root.FullName -Recurse -Force)
if ($items.Count -ne 3) { throw 'incomplete native census' }
foreach ($item in $items) {
    $acl = Get-Acl -LiteralPath $item.FullName
    $rules = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
    if (-not $acl.AreAccessRulesProtected -or $rules.Count -ne 1) { throw 'DACL not protected single-owner' }
    $rule = $rules[0]
    $inheritance = if ($item.PSIsContainer) { 3 } else { 0 }
    if ($rule.IdentityReference -ne $sid -or $rule.IsInherited -or
        $rule.AccessControlType -ne 'Allow' -or $rule.FileSystemRights -ne 'FullControl' -or
        [int]$rule.InheritanceFlags -ne $inheritance -or [int]$rule.PropagationFlags -ne 0) {
        throw 'native ACE differs from owner-only contract'
    }
}
'native DACL census: 3/3'
"#;
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", oracle])
            .env("KELD_TEST_OUTPUT", &output_dir)
            .output()
            .expect("read native DACL inventory");
        assert!(
            output.status.success(),
            "native DACL oracle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("native DACL census: 3/3"));
    }

    #[cfg(unix)]
    fn write_msys_renderer_fixture(
        temp: &TempDir,
        docker: &str,
        chmod: Option<&str>,
        remove: Option<&str>,
        native_dacl: Option<&str>,
    ) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        use std::process::Command;

        let checkout = temp.path().join("checkout");
        fs::create_dir(&checkout).expect("checkout directory");
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .arg(&checkout)
                .status()
                .expect("initialize fixture")
                .success()
        );
        fs::write(
            checkout.join("diagram.md"),
            "```mermaid\nflowchart LR\n A-->B\n```\n",
        )
        .expect("tracked diagram");
        assert!(
            Command::new("git")
                .args(["add", "diagram.md"])
                .current_dir(&checkout)
                .status()
                .expect("track diagram")
                .success()
        );
        fs::create_dir(checkout.join("tools")).expect("fixture tools");
        fs::write(checkout.join(MERMAID_CONFIG), "{}\n").expect("renderer config");
        temp.write("renderer.sh", include_str!("mermaid_render_check.sh"));
        temp.write("bin/docker", docker);
        temp.write(
            "bin/uname",
            "#!/usr/bin/env bash\nprintf 'MSYS_NT-10.0\\n'\n",
        );
        temp.write(
            "bin/cygpath",
            "#!/usr/bin/env bash\n[[ \"$1\" == '-am' || \"$1\" == '-aw' ]] || exit 64\nprintf '%s\\n' \"$2\"\n",
        );
        temp.write(
            "bin/whoami.exe",
            "#!/usr/bin/env bash\necho 'GNU whoami rejects Windows arguments' >&2\nexit 64\n",
        );
        temp.write(
            "bin/powershell.exe",
            native_dacl.unwrap_or("#!/usr/bin/env bash\nexit 0\n"),
        );
        temp.write(
            "bin/chmod",
            chmod.unwrap_or(
                "#!/usr/bin/env bash\nmode=$1\nshift\n[[ \"${1:-}\" == -- ]] && shift\nexec /bin/chmod \"$mode\" \"$@\"\n",
            ),
        );
        if let Some(remove) = remove {
            temp.write("bin/rm", remove);
        }
        for shim in [
            "docker",
            "uname",
            "cygpath",
            "whoami.exe",
            "powershell.exe",
            "chmod",
        ] {
            let path = temp.path().join("bin").join(shim);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("shim executable");
        }
        for shim in ["rm"] {
            let path = temp.path().join("bin").join(shim);
            if path.exists() {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                    .expect("shim executable");
            }
        }
        checkout
    }

    #[cfg(unix)]
    fn run_msys_renderer_fixture(temp: &TempDir, checkout: &Path) -> std::process::Output {
        use std::process::Command;

        let mut paths = vec![temp.path().join("bin")];
        paths.extend(env::split_paths(&env::var_os("PATH").expect("tool PATH")));
        Command::new("bash")
            .arg(temp.path().join("renderer.sh"))
            .current_dir(checkout)
            .env_remove("MSYSTEM")
            .env("PATH", env::join_paths(paths).expect("fixture PATH"))
            .output()
            .expect("execute real renderer script")
    }

    #[cfg(unix)]
    fn retained_render_dirs(checkout: &Path) -> Vec<PathBuf> {
        fs::read_dir(checkout.join("target"))
            .expect("checkout-local output directory")
            .map(|entry| entry.expect("retained output entry").path())
            .collect()
    }

    #[test]
    #[cfg(unix)]
    fn mermaid_renderer_retains_msys_failure_output_with_owner_only_mode() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = TempDir::new();
        let checkout = write_msys_renderer_fixture(
            &temp,
            "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image|rm) exit 0 ;;\ncontext) printf 'unix:///unused-kel152.sock\\n' ;;\nrun) exit 42 ;;\n*) exit 99 ;;\nesac\n",
            None,
            None,
            None,
        );
        let output = run_msys_renderer_fixture(&temp, &checkout);
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("failed-render output retained"),
            "missing retained-output receipt: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("with owner-only host access"),
            "native restoration must not depend on PATH whoami: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let retained = retained_render_dirs(&checkout);
        assert_eq!(retained.len(), 1, "retained output: {retained:?}");
        assert!(
            retained[0]
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("keld-mermaid-render.")),
            "wrong retained output path: {:?}",
            retained[0]
        );
        assert_eq!(
            fs::metadata(&retained[0])
                .expect("retained output metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[test]
    #[cfg(unix)]
    fn mermaid_renderer_does_not_claim_owner_only_when_restoration_fails() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = TempDir::new();
        let checkout = write_msys_renderer_fixture(
            &temp,
            "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image|rm) exit 0 ;;\ncontext) printf 'unix:///unused-kel152.sock\\n' ;;\nrun) exit 42 ;;\n*) exit 99 ;;\nesac\n",
            Some(
                "#!/usr/bin/env bash\nmode=$1\nshift\n[[ \"${1:-}\" == -- ]] && shift\nif [[ \"$mode\" == 0700 ]]; then exit 44; fi\nexec /bin/chmod \"$mode\" \"$@\"\n",
            ),
            None,
            None,
        );
        let output = run_msys_renderer_fixture(&temp, &checkout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1));
        assert!(stderr.contains("owner-only restoration failed"), "{stderr}");
        assert!(!stderr.contains("with owner-only host access"), "{stderr}");
        let retained = retained_render_dirs(&checkout);
        assert_eq!(retained.len(), 1, "retained output: {retained:?}");
        assert_eq!(
            fs::metadata(&retained[0])
                .expect("retained output metadata")
                .permissions()
                .mode()
                & 0o777,
            0o777
        );
    }

    #[test]
    #[cfg(unix)]
    fn mermaid_renderer_does_not_claim_owner_only_when_native_dacl_restoration_fails() {
        let temp = TempDir::new();
        let checkout = write_msys_renderer_fixture(
            &temp,
            "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image|rm) exit 0 ;;\ncontext) printf 'unix:///unused-kel152.sock\\n' ;;\nrun) exit 42 ;;\n*) exit 99 ;;\nesac\n",
            None,
            None,
            Some("#!/usr/bin/env bash\nexit 45\n"),
        );
        let output = run_msys_renderer_fixture(&temp, &checkout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1));
        assert!(stderr.contains("owner-only restoration failed"), "{stderr}");
        assert!(!stderr.contains("with owner-only host access"), "{stderr}");
    }

    #[test]
    #[cfg(unix)]
    fn mermaid_renderer_restores_output_when_success_cleanup_cannot_remove_it() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = TempDir::new();
        let checkout = write_msys_renderer_fixture(
            &temp,
            "#!/usr/bin/env bash\ncase \"$1\" in\ninfo|image) exit 0 ;;\ncontext) printf 'unix:///unused-kel152.sock\\n' ;;\nrm) exit 0 ;;\nrun)\n  out=''\n  shift\n  while [[ $# -gt 0 ]]; do\n    if [[ \"$1\" == --volume ]]; then\n      case \"$2\" in *:/out) out=\"${2%:/out}\" ;; esac\n      shift 2\n    else\n      shift\n    fi\n  done\n  [[ -n \"$out\" ]] || exit 64\n  printf '<svg><title>fixture</title><desc>fixture</desc></svg>' >\"$out/fixture.svg\"\n  ;;\n*) exit 99 ;;\nesac\n",
            None,
            Some("#!/usr/bin/env bash\nexit 66\n"),
            None,
        );
        let output = run_msys_renderer_fixture(&temp, &checkout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1));
        assert!(
            stderr.contains("successful-render output could not be removed; retained"),
            "{stderr}"
        );
        let retained = retained_render_dirs(&checkout);
        assert_eq!(retained.len(), 1, "retained output: {retained:?}");
        assert_eq!(
            fs::metadata(&retained[0])
                .expect("retained output metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[test]
    fn mermaid_output_outside_the_shared_checkout_fails() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace(
                    "$render_parent/keld-mermaid-render.",
                    "/tmp/keld-mermaid-render.",
                ),
        );
        let error = check(temp.path()).expect_err("output must share the checkout mount boundary");
        assert!(error.contains("render_dir"), "{error}");
    }
    #[test]
    fn missing_mermaid_msys_path_exclusion_fails() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace("MSYS2_ARG_CONV_EXCL='*'", ""),
        );
        let error = check(temp.path()).expect_err("MSYS path rewriting must stay disabled");
        assert!(error.contains("MSYS2_ARG_CONV_EXCL"), "{error}");
    }

    #[test]
    fn missing_mermaid_msys_shell_detection_fails() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace("running_under_msys", ""),
        );
        let error = check(temp.path()).expect_err("Git Bash detection must remain explicit");
        assert!(error.contains("running_under_msys"), "{error}");
    }

    #[test]
    fn missing_mermaid_msys_output_permission_fix_fails() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace("prepare_docker_output_dir \"$render_dir\"", ""),
        );
        let error = check(temp.path()).expect_err("MSYS output bind must remain writable");
        assert!(error.contains("prepare_docker_output_dir"), "{error}");
    }

    #[test]
    fn missing_mermaid_msys_permission_restoration_fails() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace("if restore_docker_output_dir \"$render_dir\"; then", ""),
        );
        let error =
            check(temp.path()).expect_err("retained output must return to owner-only access");
        assert!(error.contains("restore_docker_output_dir"), "{error}");
    }

    #[test]
    fn mermaid_permission_restoration_must_stay_bounded_and_preserve_status() {
        for (old, replacement) in [
            ("if ! running_under_msys; then", "if false; then"),
            ("\"$render_parent\"/keld-mermaid-render.*) ;;", "*) ;;"),
            ("restore_windows_owner_only_dacl \"$path\"", "true"),
            ("MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'", "true"),
            ("$acl.SetAccessRuleProtection($true, $false)", "true"),
            (
                "chmod 0700 -- \"$path\" || {",
                "chmod 0777 -- \"$path\" || {",
            ),
            ("local cleanup_status=$?", "local cleanup_status=0"),
            ("trap - EXIT", "true"),
            ("exit \"$cleanup_status\"", "exit 0"),
        ] {
            let temp = complete_fixture();
            temp.write(
                MERMAID_RENDERER,
                &read(temp.path(), MERMAID_RENDERER)
                    .expect("renderer fixture")
                    .replace(old, replacement),
            );
            let error = check(temp.path())
                .expect_err("retained-output restoration must remain bounded and failure-safe");
            assert!(error.contains("CI-HYGIENE"), "{old}: {error}");
        }
    }

    #[test]
    fn inert_or_reordered_mermaid_msys_shell_text_fails() {
        for (old, replacement) in [
            ("running_under_msys() {", "echo 'running_under_msys() {'"),
            (
                "prepare_docker_output_dir \"$render_dir\"",
                "echo 'prepare_docker_output_dir \"$render_dir\"'",
            ),
            (
                "export MSYS2_ARG_CONV_EXCL='*'",
                "echo \"export MSYS2_ARG_CONV_EXCL='*'\"",
            ),
            (
                "prepare_docker_output_dir \"$render_dir\"\ndocker_render_dir=$(docker_host_path \"$render_dir\")",
                "docker_render_dir=$(docker_host_path \"$render_dir\")\nprepare_docker_output_dir \"$render_dir\"",
            ),
        ] {
            let temp = complete_fixture();
            temp.write(
                MERMAID_RENDERER,
                &read(temp.path(), MERMAID_RENDERER)
                    .expect("renderer fixture")
                    .replace(old, replacement),
            );
            check(temp.path()).expect_err("inert or reordered MSYS shell text must fail");
        }
    }

    #[test]
    fn commented_mermaid_isolation_flag_does_not_pass() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_RENDERER,
            &read(temp.path(), MERMAID_RENDERER)
                .expect("renderer fixture")
                .replace("--network none", "# --network none"),
        );
        let error = check(temp.path()).expect_err("commented network isolation must fail");
        assert!(error.contains("--network none"), "{error}");
    }

    #[test]
    fn weakened_mermaid_resource_config_fails() {
        let temp = complete_fixture();
        temp.write(
            MERMAID_CONFIG,
            &read(temp.path(), MERMAID_CONFIG)
                .expect("config fixture")
                .replace("\"maxEdges\"", "\"removedMaxEdges\""),
        );
        let error = check(temp.path()).expect_err("config without edge limit must fail");
        assert!(error.contains("maxEdges"), "{error}");
    }

    #[test]
    fn missing_tools_codeowner_fails() {
        let temp = complete_fixture();
        temp.write(
            CODEOWNERS,
            &valid_codeowners().replace("/tools/ @alice\n", ""),
        );
        let error = check(temp.path()).expect_err("gate tools without owner must fail");
        assert!(error.contains("tools"), "{error}");
    }

    #[test]
    fn workflow_mentioning_hygiene_file_without_test_flag_fails() {
        let temp = complete_fixture();
        let workflow =
            valid_workflow().replace("--test tools/ci_hygiene.rs", "tools/ci_hygiene.rs");
        temp.write(WORKFLOW, &workflow);
        let error = check(temp.path()).expect_err("compile-only hygiene step must fail");
        assert!(error.contains("--test tools/ci_hygiene.rs"), "{error}");
    }

    #[test]
    fn comment_at_sign_does_not_false_report_unpinned() {
        assert!(is_pinned_sha(
            "actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # pin @v4.4.0"
        ));
        let workflow = format!("jobs:\n  x:\n    steps:\n{PINNED_CHECKOUT}");
        let with_comment_at = workflow.replace("# v4.4.0", "# pin @v4.4.0");
        assert!(
            action_uses_unpinned(&with_comment_at).is_empty(),
            "{:?}",
            action_uses_unpinned(&with_comment_at)
        );
    }

    #[test]
    fn quoted_uses_spec_sha_is_pinned() {
        assert!(is_pinned_sha(
            r#""actions/checkout@11d5960a326750d5838078e36cf38b85af677262""#
        ));
        let workflow = "jobs:\n  x:\n    steps:\n      - uses: \"actions/checkout@11d5960a326750d5838078e36cf38b85af677262\"\n";
        assert!(
            action_uses_unpinned(workflow).is_empty(),
            "{:?}",
            action_uses_unpinned(workflow)
        );
    }

    #[test]
    fn missing_gitleaks_job_fails() {
        let temp = complete_fixture();
        temp.write(
            WORKFLOW,
            "name: CI\njobs:\n  hygiene:\n    steps:\n      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262\n        run: rustc tools/ci_hygiene.rs\n",
        );
        let error = check(temp.path()).expect_err("workflow without gitleaks must fail");
        assert!(error.contains("gitleaks detect"), "{error}");
    }

    #[test]
    fn empty_issue_template_dir_fails() {
        let temp = complete_fixture();
        temp.write(
            ".github/ISSUE_TEMPLATE/config.yml",
            "blank_issues_enabled: true\n",
        );
        let bug = temp.path().join(".github/ISSUE_TEMPLATE/bug.yml");
        fs::remove_file(bug).expect("remove bug template");
        let error = check(temp.path()).expect_err("config-only issue templates must fail");
        assert!(error.contains("ISSUE_TEMPLATE"), "{error}");
    }

    #[test]
    fn deleting_codeowners_file_fails() {
        let temp = complete_fixture();
        fs::remove_file(temp.path().join(CODEOWNERS)).expect("remove CODEOWNERS");
        let error = check(temp.path()).expect_err("missing CODEOWNERS must fail");
        assert!(error.contains("CODEOWNERS"), "{error}");
        assert!(error.contains("Restore"), "{error}");
    }
}
