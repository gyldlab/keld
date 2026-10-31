//! KEL-39 contract check: CODEOWNERS, PR/issue templates, secret scan, Action SHAs.
//!
//! Not a Cargo member (no lockfile change). Compile with:
//! `rustc --edition=2024 -D warnings tools/ci_hygiene.rs`
//! Error text states the fix. Codes are not `KELD-*` so this file does not
//! collide with `docs/engineering/keld-error-codes.md` (owned by a parallel change).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

mod justfile_contract;
use justfile_contract::{declaration_words, dependency_gates, expanded_gates};

#[path = "ci_hygiene_workflow_access.rs"]
mod workflow_access;
use workflow_access::{
    forbidden_workflow_control_key, workflow_direct_named_step_block,
    workflow_direct_named_step_count, workflow_has_checkout_fetch_depth_zero,
    workflow_has_unconditional_named_step, workflow_job_block, workflow_job_level_if,
    workflow_job_level_property, workflow_job_sequence_values, workflow_named_step_contains,
    workflow_named_step_direct_keys, workflow_named_step_direct_value,
    workflow_named_step_has_property, workflow_named_step_mapping,
    workflow_named_step_shell_commands, yaml_content, yaml_mapping_key,
};

const CODEOWNERS: &str = ".github/CODEOWNERS";
const PR_TEMPLATE: &str = ".github/PULL_REQUEST_TEMPLATE.md";
const ISSUE_DIR: &str = ".github/ISSUE_TEMPLATE";
const WORKFLOW: &str = ".github/workflows/ci.yml";
const KELDBOT_WORKFLOW: &str = ".github/workflows/keldbot.yml";
const CI_REQUIRED_EVALUATOR: &str = "tools/ci_required.sh";
const ATOMIC_PROTOCOL_CHECKER: &str = "tools/atomic_protocol.rs";
const AGENT_CONTEXT_CHECKER: &str = "tools/agent_context.rs";
const GITIGNORE: &str = ".gitignore";
const JUSTFILE: &str = "justfile";
const NEXTEST_CONFIG: &str = ".config/nextest.toml";
const MERMAID_CHECKER: &str = "tools/mermaid_docs.rs";
const MERMAID_RENDERER: &str = "tools/mermaid_render_check.sh";
const MERMAID_CONFIG: &str = "tools/mermaid-render-config.json";
/// GH-508 committed kipc channel allocation baseline (append-only).
const CHANNEL_ALLOCATIONS: &str = "crates/keld-ipc/channel_allocations.txt";
/// Comparison base, resolved the way `tools/ci_changes.sh local` resolves it.
const CHANNEL_ALLOCATIONS_BASE_ENV: &str = "KELD_CI_BASE_REF";
const CHANNEL_ALLOCATIONS_DEFAULT_BASE: &str = "origin/main";
/// The hosted step that runs the append-only rule, and the base it binds.
const CHANNEL_ALLOCATIONS_STEP: &str = "Check this checkout";
const CHANNEL_ALLOCATIONS_STEP_IF: &str = "needs.changes.outputs.hygiene == 'true'";
const CHANNEL_ALLOCATIONS_STEP_ENV: &[(&str, &str)] = &[(
    CHANNEL_ALLOCATIONS_BASE_ENV,
    "${{ github.event.pull_request.base.sha || github.event.before }}",
)];
const CHANNEL_ALLOCATIONS_STEP_COMMANDS: &[&str] = &[
    "rustc --edition=2024 -D warnings tools/ci_hygiene.rs -o target/ci-hygiene/ci-hygiene",
    "target/ci-hygiene/ci-hygiene check .",
];
const MERMAID_IMAGE_DIGEST: &str =
    "sha256:29077c6bd02f14bdfdd5fee552d9c00fe68d4fab3cd84952d21e2d1faf2fadaf";

const REQUIRED_OWNER_PATHS: &[&str] = &[
    "crates/keld-guard",
    "crates/keld-ipc",
    // The elevated updater helper (KEL-270 T4d S9c; owner approved in session 2026-10-07).
    "crates/keld-updater-helper",
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
    // The Rust compiler pin has one owner: `checkRustToolchainPins` in
    // tools/ci_workflow_security.ts derives it from rust-toolchain.toml and
    // checks every installer job (run by this same hygiene step).
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

/// The doc-placeholders recipe pair, run by the hosted hygiene job (#650).
const DOC_PLACEHOLDER_COMMANDS: &[&str] = &[
    "mkdir -p target/doc-placeholders",
    "rustc --edition=2024 -D warnings --test tools/doc_placeholders.rs -o target/doc-placeholders/doc-placeholders-test",
    "target/doc-placeholders/doc-placeholders-test",
    "rustc --edition=2024 -D warnings tools/doc_placeholders.rs -o target/doc-placeholders/doc-placeholders",
    "target/doc-placeholders/doc-placeholders check .",
];

/// The spec-structure recipe pair, run by the hosted hygiene job (#1321).
const SPEC_STRUCTURE_COMMANDS: &[&str] = &[
    "mkdir -p target/spec-structure",
    "rustc --edition=2024 -D warnings --test tools/spec_structure.rs -o target/spec-structure/spec-structure-test",
    "target/spec-structure/spec-structure-test",
    "rustc --edition=2024 -D warnings tools/spec_structure.rs -o target/spec-structure/spec-structure",
    "target/spec-structure/spec-structure check .",
];

/// The `hygiene` recipe's hello-command regression, run by the hosted hygiene job (#650).
const HELLO_COMMAND_COMMANDS: &[&str] = &["python3 -B tools/test_hello_command.py"];

const AUDIT_DOC_COMMANDS: &[&str] = &[
    "python3 -B docs/audits/verify.py",
    "python3 -B docs/audits/test_verify.py",
];

const AGENT_CONTEXT_COMMANDS: &[&str] = &[
    "mkdir -p target/agent-context",
    "rustc --edition=2024 -D warnings --test tools/agent_context.rs -o target/agent-context/agent-context-test",
    "target/agent-context/agent-context-test",
    "rustc --edition=2024 -D warnings tools/agent_context.rs -o target/agent-context/agent-context",
    "target/agent-context/agent-context check .",
    "python3 -B tools/test_session_closeout.py",
    "python3 -B tools/test_session_closeout_hook.py",
    "python3 -B tools/workspace.py check",
    "python3 -B tools/test_workspace.py",
];

// The gitleaks `Scan` step scans exactly what each event admits: a pull request's own
// commits (both SHAs must resolve, so an unresolvable range cannot pass with zero
// commits scanned) or the pushed branch's history; any other event refuses.
const GITLEAKS_SCAN_ENV: &[(&str, &str)] = &[
    ("KELD_GITLEAKS_EVENT", "${{ github.event_name }}"),
    (
        "KELD_GITLEAKS_PR_BASE",
        "${{ github.event.pull_request.base.sha }}",
    ),
    (
        "KELD_GITLEAKS_PR_HEAD",
        "${{ github.event.pull_request.head.sha }}",
    ),
];

const GITLEAKS_SCAN_COMMANDS: &[&str] = &[
    "set -euo pipefail",
    "case \"$KELD_GITLEAKS_EVENT\" in",
    "pull_request)",
    "for sha in \"$KELD_GITLEAKS_PR_BASE\" \"$KELD_GITLEAKS_PR_HEAD\"; do",
    "if [ -z \"$sha\" ] || ! git cat-file -e \"${sha}^{commit}\"; then",
    "echo \"gitleaks: a pull_request base or head SHA is missing or not a local commit; refusing an unresolvable range\" >&2",
    "exit 1",
    "fi",
    "done",
    "KELD_GITLEAKS_RANGE=\"$KELD_GITLEAKS_PR_BASE..$KELD_GITLEAKS_PR_HEAD\"",
    ";;",
    "push)",
    "KELD_GITLEAKS_RANGE=HEAD",
    ";;",
    "*)",
    "echo \"gitleaks: unsupported event $KELD_GITLEAKS_EVENT; refusing to choose a scan range\" >&2",
    "exit 1",
    ";;",
    "esac",
    "echo \"gitleaks log range: $KELD_GITLEAKS_RANGE\"",
    "gitleaks detect --source . --verbose --redact --exit-code 1 --log-opts=\"$KELD_GITLEAKS_RANGE\"",
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

/// KEL-53 §4 "Helper launch and self-anchor" (KEL-270 T4d S9c): the release updater
/// helper's loader acceptance and the KEL-19 container amendment's release-helper AC1
/// and AC2 rows.
const RELEASE_HELPER_STEP: &str =
    "Real Windows release updater helper loader and container (KEL-53 T4d S9c)";
const RELEASE_HELPER_CONDITION: &str = "matrix.os == 'windows-latest' && (contains(needs.changes.outputs.packages, 'keld-updater-helper') || contains(needs.changes.outputs.packages, 'keld-pack') || contains(needs.changes.outputs.packages, 'keld-host'))";
const RELEASE_HELPER_ENV: &[(&str, &str)] = &[
    (
        "KELD_UPDATER_HELPER_RELEASE",
        "${{ github.workspace }}/target/release/keld-updater-helper.exe",
    ),
    (
        "KELD_PACK_REAL_HOST",
        "${{ github.workspace }}/target/release/keld-updater-helper.exe",
    ),
    (
        "KELD_PACK_REAL_HELPER",
        "${{ github.workspace }}/target/release/keld-updater-helper.exe",
    ),
    (
        "KELD_PACK_EMBEDDED_HELPER",
        "${{ runner.temp }}/ac2-helper/keld-updater-helper.exe",
    ),
];
const RELEASE_HELPER_COMMANDS: &[&str] = &[
    "set -euo pipefail",
    "cargo build --release -p keld-updater-helper",
    "if ! out=\"$(cargo test -p keld-updater-helper --test release_image -- --ignored 2>&1)\"; then",
    "printf '%s\\n' \"$out\"",
    "exit 1",
    "fi",
    "printf '%s\\n' \"$out\"",
    "grep -q 'test result: ok\\. 2 passed;' <<<\"$out\"",
    "if ! out=\"$(cargo test -p keld-pack --test real_host real_release_host_round_trip -- --ignored --exact 2>&1)\"; then",
    "printf '%s\\n' \"$out\"",
    "exit 1",
    "fi",
    "printf '%s\\n' \"$out\"",
    "grep -q 'test result: ok\\. 1 passed;' <<<\"$out\"",
    "mkdir -p \"$RUNNER_TEMP/ac2-helper\"",
    "if ! out=\"$(cargo test -p keld-pack --test real_host_acceptance -- --ignored --exact launch::ac2_embedded_release_helper_launches_to_its_self_anchor_refusal 2>&1)\"; then",
    "printf '%s\\n' \"$out\"",
    "exit 1",
    "fi",
    "printf '%s\\n' \"$out\"",
    "grep -q 'test result: ok\\. 1 passed;' <<<\"$out\"",
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
    "if (Test-Path -LiteralPath $evidenceRoot) { throw 'Windows media evidence root must be absent before the child oracle starts' }",
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
    "foreach ($field in @('schema', 'source_executable', 'executable', 'executable_sha256', 'system', 'device', 'capture_device', 'scope')) { if ($result.$field -isnot [string]) { throw \"Windows media top-level string field was mistyped: $field\" } }",
    "if ($result.rows -isnot [array]) { throw 'Windows media rows were not encoded as an array' }",
    "if ($result.schema -cne 'keld.windows-media-fixture/v1' -or @($result.rows).Count -ne 10) { throw 'Windows media guard returned the wrong schema or row count' }",
    "$expectedScope = 'raw WebView2 callback receipt: adapter input; removed-product-guard plus fixture-only completion after DEFAULT; adapter-bypass; same-callback explicit state; loopback origin; synthetic capture control; bounded teardown. No physical-device, saved-grant, snapshot, or revocation pass.'",
    "if ($result.system -cne [Environment]::OSVersion.VersionString -or $result.device -cne [Environment]::MachineName -or $result.capture_device -cne 'WebView2 synthetic development device' -or $result.scope -cne $expectedScope) { throw 'Windows media guard returned invalid platform or scope metadata' }",
    "$evidenceExecutable = Join-Path $evidenceRoot 'keld_wv_media_test.exe'",
    "if ($result.source_executable -cne $fixturePath -or $result.executable -cne $evidenceExecutable -or $result.executable_sha256 -cne $fixtureHash -or (Get-FileHash -Algorithm SHA256 -LiteralPath $fixturePath).Hash.ToLowerInvariant() -cne $fixtureHash -or (Get-FileHash -Algorithm SHA256 -LiteralPath $evidenceExecutable).Hash.ToLowerInvariant() -cne $fixtureHash) { throw 'Windows media result does not bind the selected Cargo executable and evidence copy' }",
    "$requiredProbeFields = @('mode', 'exit_code', 'outer_timed_out', 'stdout_sha256', 'stderr_sha256')",
    "$watchdogFields = @($result.watchdog_probe.PSObject.Properties.Name | Sort-Object -Unique -CaseSensitive)",
    "if ($watchdogFields.Count -ne $requiredProbeFields.Count -or @(Compare-Object -CaseSensitive -ReferenceObject $requiredProbeFields -DifferenceObject $watchdogFields).Count -ne 0 -or ($result.watchdog_probe.exit_code -isnot [int] -and $result.watchdog_probe.exit_code -isnot [long]) -or $result.watchdog_probe.outer_timed_out -isnot [bool] -or $result.watchdog_probe.mode -isnot [string] -or $result.watchdog_probe.stdout_sha256 -isnot [string] -or $result.watchdog_probe.stderr_sha256 -isnot [string] -or $result.watchdog_probe.mode -cne 'work' -or $result.watchdog_probe.exit_code -ne 124 -or $result.watchdog_probe.outer_timed_out -ne $false) { throw 'Windows media watchdog probe was absent, mistyped, or invalid' }",
    "$requiredOuterProbeFields = @('exit_code', 'outer_timed_out', 'stdout_sha256', 'stderr_sha256')",
    "$outerProbeFields = @($result.outer_deadline_probe.PSObject.Properties.Name | Sort-Object -Unique -CaseSensitive)",
    "if ($outerProbeFields.Count -ne $requiredOuterProbeFields.Count -or @(Compare-Object -CaseSensitive -ReferenceObject $requiredOuterProbeFields -DifferenceObject $outerProbeFields).Count -ne 0 -or ($result.outer_deadline_probe.exit_code -isnot [int] -and $result.outer_deadline_probe.exit_code -isnot [long]) -or $result.outer_deadline_probe.exit_code -lt [int]::MinValue -or $result.outer_deadline_probe.exit_code -gt [int]::MaxValue -or $result.outer_deadline_probe.outer_timed_out -isnot [bool] -or $result.outer_deadline_probe.stdout_sha256 -isnot [string] -or $result.outer_deadline_probe.stderr_sha256 -isnot [string] -or $result.outer_deadline_probe.exit_code -eq 0 -or $result.outer_deadline_probe.outer_timed_out -ne $true) { throw 'Windows media outer-deadline probe was absent, mistyped, or invalid' }",
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
    "$rowIntegerFields = @('view_id', 'host_pid', 'browser_pid', 'permission_kind', 'initial_state', 'requested_state', 'returned_state', 'adapter_tid', 'exit_code')",
    "$rowStringFields = @('kind', 'mode', 'manifest_case', 'nonce', 'origin_uri', 'manifest_fnv1a64', 'adapter_principal', 'adapter_capability', 'adapter_decision', 'registration_identity', 'sender_identity', 'outcome', 'profile_path', 'receipt', 'log_sha256', 'stderr_sha256')",
    "$receiptPattern = '^KELD_MEDIA_RESULT kind=(?<kind>camera|microphone) mode=(?<mode>guarded|removed-guard|adapter-bypass|force-allow) nonce=(?<nonce>[1-9][0-9]*) runtime=(?<runtime>[^ ]+) host_pid=(?<host>[1-9][0-9]*) browser_pid=(?<browser>[1-9][0-9]*) view_id=(?<view>[1-9][0-9]*) tid=(?<tid>[1-9][0-9]*) manifest_fnv1a64=(?<manifest_hash>[0-9a-f]{16}) adapter_principal=(?<adapter_principal>[^ ]+) adapter_capability=(?<adapter_capability>[^ ]+) adapter_decision=(?<adapter_decision>[^ ]+) adapter_tid=(?<adapter_tid>0|[1-9][0-9]*) permission_kind=(?<permission_kind>0|[1-9][0-9]*) before=(?<before>0|[1-9][0-9]*) requested=(?<requested>0|[1-9][0-9]*) after=(?<after>0|[1-9][0-9]*) origin_uri=(?<origin_uri>[^ ]+) registration_identity=(?<registration_identity>[1-9a-f][0-9a-f]{0,15}) sender_identity=(?<sender_identity>[1-9a-f][0-9a-f]{0,15}) outcome=(?<outcome>[^ ]+) manifest=(?<manifest>true|false) adapter=(?<adapter>true|false) effect=(?<effect>true|false) origin=(?<origin>true|false) js=(?<js>true|false) control=(?<control>true|false) accepted=(?<accepted>true|false) case_ok=(?<case_ok>true|false)$'",
    "$seenNonces = @{}",
    "foreach ($row in $result.rows) {",
    "$rowFields = @($row.PSObject.Properties.Name | Sort-Object -Unique -CaseSensitive)",
    "if ($rowFields.Count -ne $requiredRowFields.Count -or @(Compare-Object -CaseSensitive -ReferenceObject $requiredRowFields -DifferenceObject $rowFields).Count -ne 0) { throw 'Windows media guard returned the wrong row artifact shape' }",
    "$rowStem = \"$($row.kind)-$($row.mode)-$($row.manifest_case)\"",
    "foreach ($field in $rowIntegerFields) { if ($row.$field -isnot [int] -and $row.$field -isnot [long]) { throw \"Windows media row integer field was mistyped: $rowStem/$field\" } }",
    "foreach ($field in $rowStringFields) { if ($row.$field -isnot [string]) { throw \"Windows media row string field was mistyped: $rowStem/$field\" } }",
    "if ($row.profile_removed -isnot [bool]) { throw \"Windows media row Boolean field was mistyped: $rowStem/profile_removed\" }",
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

const FUZZ_WORKSPACE_STEPS: &[(&str, &str)] = &[
    (
        "Check keld-ipc fuzz workspace",
        "cargo check --manifest-path crates/keld-ipc/fuzz/Cargo.toml",
    ),
    (
        "Test keld-ipc fuzzing hook",
        "cargo test -p keld-ipc --features fuzzing --lib attempt::records",
    ),
    (
        "Check keld-wv fuzz workspace",
        "cargo check --manifest-path crates/keld-wv/fuzz/Cargo.toml",
    ),
    (
        "Check keld-update fuzz workspace",
        "cargo check --manifest-path crates/keld-update/fuzz/Cargo.toml",
    ),
];

const ROOT_TEST_RECIPE_COMMANDS: &[&str] = &[
    "#!/usr/bin/env bash",
    "set -euo pipefail",
    "if [[ \"$(uname -s)\" == \"Linux\" ]]; then",
    "xvfb-run -a cargo nextest run --workspace --profile ci",
    "else",
    "cargo nextest run --workspace --profile ci",
    "fi",
];

fn read(root: &Path, relative: &str) -> Result<String, String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|error| {
        format!(
            "CI-HYGIENE: missing `{}`: {error}. Restore the KEL-39 file from git or recreate it.",
            path.display()
        )
    })
}

fn just_recipe_commands(text: &str, recipe: &str) -> Option<Vec<String>> {
    let header = format!("{recipe}:");
    let start = text.lines().position(|line| {
        line.strip_prefix(&header)
            .is_some_and(|tail| !tail.starts_with('='))
    })? + 1;
    let mut commands = Vec::new();
    for line in text.lines().skip(start) {
        if !line.trim().is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            break;
        }
        let command = line.trim();
        if !command.is_empty() && (!command.starts_with('#') || command.starts_with("#!")) {
            commands.push(command.to_owned());
        }
    }
    Some(commands)
}

fn check_root_audit_contract(root: &Path) -> Result<(), String> {
    let justfile = read(root, JUSTFILE)?;
    let ci_gates = expanded_gates(&justfile, "ci")
        .ok_or_else(|| format!("CI-HYGIENE: `{JUSTFILE}` is missing the root `ci:` recipe."))?;
    if !ci_gates.contains(&"audit-docs") {
        return Err(format!(
            "CI-HYGIENE: `{JUSTFILE}` root `ci:` must depend on `audit-docs`."
        ));
    }
    let mut commands = just_recipe_commands(&justfile, "audit-docs")
        .ok_or_else(|| format!("CI-HYGIENE: `{JUSTFILE}` is missing the `audit-docs:` recipe."))?;
    if ci_gates.contains(&"audit-docs-test") {
        commands.extend(
            just_recipe_commands(&justfile, "audit-docs-test").ok_or_else(|| {
                format!("CI-HYGIENE: `{JUSTFILE}` is missing `audit-docs-test:`.")
            })?,
        );
    }
    let expected = [
        "{{python_command}} -B docs/audits/verify.py",
        "{{python_command}} -B docs/audits/test_verify.py",
    ];
    if commands
        .iter()
        .map(String::as_str)
        .ne(expected.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{JUSTFILE}` `audit-docs:` must run the verifier and mutation suite exactly."
        ));
    }
    Ok(())
}

fn check_root_test_display_contract(root: &Path) -> Result<(), String> {
    let justfile = read(root, JUSTFILE)?;
    let commands = just_recipe_commands(&justfile, "test").ok_or_else(|| {
        format!(
            "CI-HYGIENE: `{JUSTFILE}` is missing the root `test:` recipe. Restore the full nextest gate."
        )
    })?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(ROOT_TEST_RECIPE_COMMANDS.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{JUSTFILE}` `test:` must run the full workspace under `xvfb-run -a` on Linux and directly on other OSes. The Linux no-flag host suite creates a real GTK window; without the virtual display, the root gate fails before its lifecycle oracle runs."
        ));
    }
    Ok(())
}

/// Gates that build, test or resolve the Rust workspace. `ci` runs them one at a time
/// after every other gate, because the load-sensitive host tests must not share the
/// machine with a concurrent gate.
const SERIAL_RUST_GATES: [&str; 5] = ["fmt-check", "clippy", "test", "doc", "deny"];

/// Recipes whose attribute lines include `parallel`, in declaration order. `just`
/// rejects anything but further attributes between an attribute and its recipe.
fn parallel_recipes(justfile: &str) -> Vec<&str> {
    let mut recipes = Vec::new();
    let mut parallel = false;
    for line in justfile.lines() {
        if line.starts_with('[') {
            parallel |= declaration_words(line, true).contains(&"parallel");
            continue;
        }
        if parallel {
            if let Some(name) = line
                .split([':', ' '])
                .next()
                .filter(|name| !name.is_empty())
            {
                recipes.push(name);
            }
        }
        parallel = false;
    }
    recipes
}

fn check_serial_rust_gates(root: &Path) -> Result<(), String> {
    let justfile = read(root, JUSTFILE)?;
    if justfile
        .lines()
        .any(|line| line.trim() == "{{python_command}} -B tools/ci_local.py ci-inventory")
    {
        for (recipe, expected) in [
            ("ci", "{{python_command}} -B tools/ci_local.py ci-inventory"),
            (
                "ci-full",
                "{{python_command}} -B tools/ci_local.py ci-full-inventory",
            ),
            ("ci-route", "tools/ci_changes.sh local"),
        ] {
            if just_recipe_commands(&justfile, recipe).as_deref() != Some(&[expected.to_owned()]) {
                return Err(format!(
                    "CI-HYGIENE: `{recipe}` must invoke its declared executor/shared router exactly."
                ));
            }
        }
    }
    let groups = parallel_recipes(&justfile);
    for recipe in ["ci", "ci-full"] {
        let gates = expanded_gates(&justfile, recipe).ok_or_else(|| {
            format!("CI-HYGIENE: `{JUSTFILE}` is missing the `{recipe}:` recipe.")
        })?;
        let once = SERIAL_RUST_GATES
            .iter()
            .all(|gate| gates.iter().filter(|candidate| *candidate == gate).count() == 1);
        if groups.contains(&recipe) || !once || !gates.ends_with(&SERIAL_RUST_GATES) {
            return Err(format!(
                "CI-HYGIENE: `{JUSTFILE}` `{recipe}:` must end with `{}`, each once, run one at a time after every other gate.",
                SERIAL_RUST_GATES.join(" ")
            ));
        }
    }
    for group in groups {
        let members = dependency_gates(&justfile, group, true).unwrap_or_default();
        if let Some(gate) = members
            .iter()
            .find(|member| SERIAL_RUST_GATES.contains(member) || **member == "typescript")
        {
            return Err(format!(
                "CI-HYGIENE: `{JUSTFILE}` `[parallel]` recipe `{group}` reaches `{gate}`; TypeScript and Rust gates must run one at a time after the policy batch."
            ));
        }
    }
    for recipe in ["ci", "ci-full"] {
        if !expanded_gates(&justfile, recipe)
            .is_some_and(|gates| gates.iter().filter(|gate| **gate == "js-fmt-check").count() == 1)
            || just_recipe_commands(&justfile, "js-fmt-check").as_deref()
                != Some(&[
                    "bun packages/@keld/kipc/scripts/echo-codegen.ts check".to_owned(),
                    "bun fmt --no-script --check \"**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}\""
                        .to_owned(),
                ])
        {
            return Err(format!(
                "CI-HYGIENE: `{recipe}` must run js-fmt-check exactly once with codegen freshness and the direct native JS/TS formatter check."
            ));
        }
    }

    Ok(())
}

fn check_mermaid_local_gate(root: &Path) -> Result<(), String> {
    const MERMAID_GATES: [&str; 3] = ["mermaid-test", "mermaid-check", "mermaid-render-check"];
    let justfile = read(root, JUSTFILE)?;
    let ci_gates = expanded_gates(&justfile, "ci")
        .ok_or_else(|| format!("CI-HYGIENE: `{JUSTFILE}` is missing the root `ci:` recipe."))?;
    if !ci_gates.contains(&"mermaid-ci") || MERMAID_GATES.iter().any(|gate| ci_gates.contains(gate))
    {
        return Err(format!(
            "CI-HYGIENE: `{JUSTFILE}` `ci:` must use only the routed `mermaid-ci` target, not unconditional Mermaid targets."
        ));
    }
    let mermaid_full = expanded_gates(&justfile, "mermaid-full")
        .ok_or_else(|| format!("CI-HYGIENE: `{JUSTFILE}` is missing `mermaid-full:`."))?;
    if !MERMAID_GATES.iter().all(|gate| mermaid_full.contains(gate)) {
        return Err(format!(
            "CI-HYGIENE: `{JUSTFILE}` `mermaid-full:` must retain parser tests, structural checks, and pinned rendering."
        ));
    }
    let forced = ci_gates
        .iter()
        .flat_map(|gate| {
            if *gate == "mermaid-ci" {
                mermaid_full.clone()
            } else {
                vec![*gate]
            }
        })
        .collect::<Vec<_>>();
    if expanded_gates(&justfile, "ci-full").as_ref() != Some(&forced) {
        return Err(format!(
            "CI-HYGIENE: `{JUSTFILE}` `ci-full:` must force the full Mermaid path while running the same remaining local gates."
        ));
    }
    let routed = just_recipe_commands(&justfile, "mermaid-ci")
        .ok_or_else(|| format!("CI-HYGIENE: `{JUSTFILE}` is missing `mermaid-ci:`."))?;
    let routed = routed.join("\n");
    for input in [
        "tools/ci_changes.sh local",
        "just mermaid-full",
        "no diagram or renderer input changed",
    ] {
        if !routed.contains(input) {
            return Err(format!(
                "CI-HYGIENE: `{JUSTFILE}` `mermaid-ci:` must route `{input}` and fail closed on unknown applicability."
            ));
        }
    }
    if !routed.contains("*)") || !routed.contains("exit 1 ;;") {
        return Err(format!(
            "CI-HYGIENE: `{JUSTFILE}` `mermaid-ci:` must reject unknown applicability instead of returning a skipped-green success."
        ));
    }
    Ok(())
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
        (
            "Classify changed-path ownership",
            "tools/ci_changes.sh github",
        ),
        // agents-md reads crates/*.rs as well as AGENTS and .agents files, so
        // it runs in the always-created router job (#650).
        (
            "Agent instruction inventory (agents-md)",
            "tools/agents_md.sh",
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

fn check_independent_router_contract_job(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, "router-contracts") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` must retain the unconditional `router-contracts` job so slow regression tests do not block classification."
        ));
    };
    if workflow_job_level_property(&block, "needs").is_some()
        || workflow_job_level_if(&block).is_some()
        || workflow_job_level_property(&block, "continue-on-error").is_some()
        || workflow_job_level_property(&block, "runs-on").as_deref() != Some("ubuntu-latest")
        || !workflow_has_checkout_fetch_depth_zero(&block)
        || !workflow_has_unconditional_named_step(
            &block,
            "Router contract tests",
            "tools/ci_changes_test.sh",
        )
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `router-contracts` must run the exact unconditional `tools/ci_changes_test.sh` on its own full-history Ubuntu runner, never behind changes."
        ));
    }
    if workflow_job_block(text, "changes").is_some_and(|changes| {
        uncommented_line_contains(&changes, "run: tools/ci_changes_test.sh")
            || uncommented_line_contains(&changes, "name: Router contract tests")
    }) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `changes` must not execute the independent router contract test suite and delay every selected native lane."
        ));
    }
    Ok(())
}

fn check_mermaid_job(text: &str) -> Result<(), String> {
    let Some(changes) = workflow_job_block(text, "changes") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `changes` job to own Mermaid applicability."
        ));
    };
    if !uncommented_line_contains(&changes, "mermaid: ${{ steps.classify.outputs.mermaid }}") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `changes` must publish the shared `mermaid` applicability result."
        ));
    }
    let Some(mermaid) = workflow_job_block(text, "mermaid") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no separately routed `mermaid` job. Keep renderer work out of broad docs routing."
        ));
    };
    let expected_if = "needs.changes.outputs.mermaid == 'true'";
    if workflow_job_level_if(&mermaid).as_deref() != Some(expected_if) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `mermaid` job must use exact input condition `{expected_if}`."
        ));
    }
    if workflow_job_sequence_values(&mermaid, "needs").as_deref()
        != Some(["changes".to_owned()].as_slice())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `mermaid` job must depend only on the always-created `changes` router."
        ));
    }
    if workflow_job_level_property(&mermaid, "continue-on-error").is_some() {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `mermaid` job must preserve its failing status for required admission."
        ));
    }
    let source_step = "Mermaid source contracts";
    let source_block = workflow_direct_named_step_block(&mermaid, source_step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `mermaid` must validate the source contracts before rendering.")
    })?;
    let source_commands =
        workflow_named_step_shell_commands(&source_block, source_step).unwrap_or_default();
    let expected_source_commands = [
        "mkdir -p target/mermaid-docs",
        "rustc --edition=2024 -D warnings --test tools/mermaid_docs.rs -o target/mermaid-docs/mermaid-docs-test",
        "target/mermaid-docs/mermaid-docs-test",
        "rustc --edition=2024 -D warnings tools/mermaid_docs.rs -o target/mermaid-docs/mermaid-docs",
        "target/mermaid-docs/mermaid-docs check .",
    ];
    if source_commands
        .iter()
        .map(String::as_str)
        .ne(expected_source_commands)
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{source_step}` must run the parser tests and source check exactly before rendering."
        ));
    }
    let render_step = "Render Mermaid docs in pinned isolated container";
    let render_block =
        workflow_direct_named_step_block(&mermaid, render_step).ok_or_else(|| {
            format!("CI-HYGIENE: `{WORKFLOW}` `mermaid` job has no direct pinned render step.")
        })?;
    if workflow_named_step_direct_value(&render_block, render_step, "run").as_deref()
        != Some("tools/mermaid_render_check.sh")
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{render_step}` must run the renderer directly without wrappers."
        ));
    }
    if let Some(hygiene) = workflow_job_block(text, "hygiene")
        && workflow_direct_named_step_count(&hygiene, render_step) != 0
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` must not couple Mermaid rendering to the broad documentation/hygiene job."
        ));
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

/// The router owns the `check` OS list (#624): every Rust selection gets all
/// three OSes except one reached only through documentation reads. A literal
/// list here would silently ignore that decision in either direction.
const CHECK_OS_MATRIX: &str = "os: ${{ fromJSON(needs.changes.outputs.check_os) }}";

/// A job's `strategy.matrix.os` lines (indent 8, before `steps:`); never text
/// elsewhere in the job, such as an `env` value or a `run` script.
fn workflow_job_matrix_os_lines(text: &str, job: &str) -> Vec<String> {
    let block = workflow_job_block(text, job).unwrap_or_default();
    block
        .lines()
        .filter_map(yaml_content)
        .take_while(|(_, content)| *content != "steps:")
        .filter(|(indent, content)| *indent == 8 && content.starts_with("os:"))
        .map(|(_, content)| content.to_owned())
        .collect()
}

fn check_check_job_os_matrix(text: &str) -> Result<(), String> {
    let os_lines = workflow_job_matrix_os_lines(text, "check");
    if os_lines != [CHECK_OS_MATRIX] {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `check` must take its OS matrix from the router as exactly `{CHECK_OS_MATRIX}`; got `{}`. The router decides when documentation-only reads run on Windows alone.",
            os_lines.join("`, `")
        ));
    }
    Ok(())
}

const DOCTEST_JOB: &str = "doctest";
const DOCTEST_STEP: &str = "cargo test --doc";

/// The doctest lane's OS matrix. rustdoc compiles doctests for the host target
/// only: macOS proves the ungated doctests and Windows the `cfg(windows)` ones
/// (keld-update's activation entry points, KEL-270 T4d S6b2). Neither links
/// WebKitGTK, so the lane has no network apt step; Ubuntu would need one.
const DOCTEST_OS_MATRIX: &str = "os: [macos-latest, windows-latest]";

/// nextest does not run doctests, so this lane does (#632): on exactly the
/// macOS and Windows hosts, gated on its router output, over the router's
/// library-package selection, with an empty selection refused rather than
/// passing as a no-op loop.
fn check_doctest_job(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, DOCTEST_JOB) else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `{DOCTEST_JOB}` job. nextest skips doctests; restore the routed `cargo test --doc` lane."
        ));
    };
    if workflow_job_level_property(&block, "needs").as_deref() != Some("changes")
        || workflow_job_level_if(&block).as_deref()
            != Some("needs.changes.outputs.doctest == 'true'")
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{DOCTEST_JOB}` must declare `needs: changes` and job-level `if: needs.changes.outputs.doctest == 'true'`; the router owns which packages have doctests to run, and without the dependency the job cannot read its outputs."
        ));
    }
    if workflow_job_level_property(&block, "runs-on").as_deref() != Some("${{ matrix.os }}") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{DOCTEST_JOB}` must use `runs-on: ${{{{ matrix.os }}}}`; a fixed runner would run every matrix row on one OS."
        ));
    }
    if workflow_job_matrix_os_lines(text, DOCTEST_JOB) != [DOCTEST_OS_MATRIX] {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{DOCTEST_JOB}` must run on exactly `{DOCTEST_OS_MATRIX}`: rustdoc compiles doctests for the host target only, so Windows proves the cfg(windows) doctests and macOS the rest, and neither needs a network apt step."
        ));
    }
    let env_ok = workflow_named_step_mapping(&block, DOCTEST_STEP, "env").is_some_and(|entries| {
        entries
            == [(
                "KELD_CI_DOCTEST_PACKAGES".to_owned(),
                "${{ needs.changes.outputs.doctest_packages }}".to_owned(),
            )]
    });
    if !env_ok
        || workflow_named_step_direct_keys(&block, DOCTEST_STEP).unwrap_or_default()
            != ["shell", "env", "run"]
        || workflow_named_step_direct_value(&block, DOCTEST_STEP, "shell").as_deref()
            != Some("bash")
        || workflow_named_step_shell_commands(&block, DOCTEST_STEP).unwrap_or_default()
            != [
                "if [ -z \"$KELD_CI_DOCTEST_PACKAGES\" ]; then",
                "echo \"::error::doctest lane ran with no package; check the router's doctest_packages output\"",
                "exit 1",
                "fi",
                "for package in $KELD_CI_DOCTEST_PACKAGES; do",
                "cargo test -p \"$package\" --doc",
                "done",
            ]
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{DOCTEST_JOB}` must run `{DOCTEST_STEP}` with only `shell: bash`, `env: KELD_CI_DOCTEST_PACKAGES: ${{{{ needs.changes.outputs.doctest_packages }}}}` and the exact empty-selection guard plus `cargo test -p \"$package\" --doc` loop, without wrappers or exit suppression."
        ));
    }
    Ok(())
}

/// The hosted runner set for lanes whose contracts differ per OS.
const THREE_OS_MATRIX: &str = "os: [ubuntu-latest, macos-latest, windows-latest]";

const WORKSPACE_JOB: &str = "workspace-contracts";
const WORKSPACE_STEP: &str = "Local workspace path and process contracts";

/// The workspace tool's OS-specific path and process contracts run in their own
/// three-OS job, selected only by its router output (#624).
fn check_workspace_contracts_job(text: &str) -> Result<(), String> {
    let Some(block) = workflow_job_block(text, WORKSPACE_JOB) else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `{WORKSPACE_JOB}` job. Restore the three-OS workspace path and process contracts."
        ));
    };
    if workflow_job_level_property(&block, "runs-on").as_deref() != Some("${{ matrix.os }}") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{WORKSPACE_JOB}` must use `runs-on: ${{{{ matrix.os }}}}`; a fixed runner would run every matrix row on one OS."
        ));
    }
    if workflow_job_level_property(&block, "needs").as_deref() != Some("changes")
        || workflow_job_level_if(&block).as_deref()
            != Some("needs.changes.outputs.workspace == 'true'")
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{WORKSPACE_JOB}` must declare `needs: changes` and job-level `if: needs.changes.outputs.workspace == 'true'`; the router owns which diffs reach it."
        ));
    }
    if workflow_job_matrix_os_lines(text, WORKSPACE_JOB) != [THREE_OS_MATRIX] {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{WORKSPACE_JOB}` must run on `{THREE_OS_MATRIX}`; its path and process contracts differ per OS."
        ));
    }
    if workflow_named_step_direct_keys(&block, WORKSPACE_STEP).unwrap_or_default()
        != ["shell", "run"]
        || workflow_named_step_shell_commands(&block, WORKSPACE_STEP).unwrap_or_default()
            != [
                "if [ \"${{ matrix.os }}\" = windows-latest ]; then",
                "python -B tools/test_workspace.py",
                "else",
                "python3 -B tools/test_workspace.py",
                "fi",
            ]
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{WORKSPACE_JOB}` must run `{WORKSPACE_STEP}` unconditionally with only `shell` and `run`, executing tools/test_workspace.py on every OS."
        ));
    }
    Ok(())
}

fn check_fuzz_workspace_step(text: &str) -> Result<(), String> {
    let Some(check_job) = workflow_job_block(text, "check") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no cross-platform `check` job for the stable fuzz workspaces."
        ));
    };
    let condition = "matrix.os == 'ubuntu-latest' && needs.changes.outputs.rust == 'true'";
    let expected_keys = ["if".to_owned(), "run".to_owned()];
    for (step, command) in FUZZ_WORKSPACE_STEPS {
        let count = workflow_direct_named_step_count(&check_job, step);
        if count != 1 {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `check` must contain exactly one `{step}` step; found {count}."
            ));
        }
        let block = workflow_direct_named_step_block(&check_job, step).ok_or_else(|| {
            format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `check.steps`.")
        })?;
        if workflow_named_step_direct_keys(&block, step).as_deref()
            != Some(expected_keys.as_slice())
        {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `{step}` must contain only its exact Ubuntu/router condition and direct run command."
            ));
        }
        if workflow_named_step_direct_value(&block, step, "if").as_deref() != Some(condition) {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `{step}` must use the exact condition `{condition}` so the stable fuzz build runs only on the rust-routed Ubuntu row."
            ));
        }
        let commands = workflow_named_step_shell_commands(&block, step).ok_or_else(|| {
            format!("CI-HYGIENE: `{WORKFLOW}` `{step}` has no executable multiline run block.")
        })?;
        if commands.len() != 1 || commands[0] != *command {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `{step}` must directly run the stable fuzz-workspace cargo check without a wrapper, campaign, retry, or exit suppression."
            ));
        }
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
            "CI-HYGIENE: `{WORKFLOW}` `{job}` must have at least one `oven-sh/setup-bun` step, and every such step must pin its own `with.bun-version` to `1.4.4`. Use direct block-style steps and inputs; unrelated job text is not a runtime pin."
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
        if pins.len() != 1 || pins[0].1.trim_matches(['\'', '"']) != "1.4.4" {
            return Err(invalid());
        }
    }
    if setups == 0 {
        return Err(invalid());
    }
    Ok(())
}

/// The Bun lane has exactly one static three-OS matrix, not a larger matrix
/// with include/exclude rows that can suppress a required hosted runner. Read
/// only the actual job-level strategy.matrix mapping, never an unrelated
/// indentation-matched `os:` line or a step/script with similar text.
fn bun_job_has_static_three_os_matrix(block: &str) -> bool {
    let mut strategies = 0;
    let mut matrices = 0;
    let mut os_entries = 0;
    let mut in_strategy = false;
    let mut in_matrix = false;

    for (indent, content) in block.lines().filter_map(yaml_content) {
        if indent == 4 {
            in_matrix = false;
            in_strategy = yaml_mapping_key(content).is_some_and(|(key, _)| key == "strategy");
            if in_strategy {
                strategies += 1;
                if content != "strategy:" {
                    return false;
                }
            }
        } else if in_strategy && indent == 6 {
            in_matrix = false;
            if yaml_mapping_key(content).is_some_and(|(key, _)| key == "matrix") {
                matrices += 1;
                in_matrix = true;
                if content != "matrix:" {
                    return false;
                }
            }
        } else if in_matrix && indent > 6 {
            // A static inline OS list needs no other matrix entries. In
            // particular, `exclude:` can silently drop Windows while leaving
            // the literal three-OS list intact. Reject `include:` as well.
            if indent != 8 || content != THREE_OS_MATRIX {
                return false;
            }
            os_entries += 1;
        }
    }

    strategies == 1 && matrices == 1 && os_entries == 1
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
    let format_step = "Native JavaScript formatting";
    let format_block = workflow_job_block(text, "hygiene").and_then(|job| {
        (workflow_direct_named_step_count(&job, format_step) == 1)
            .then(|| workflow_direct_named_step_block(&job, format_step))
            .flatten()
    });
    let format_lines = format_block.as_deref().map(|block| {
        block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect::<Vec<_>>()
    });
    if format_lines
        != Some(vec![
            "- name: Native JavaScript formatting",
            "run: |",
            "bun packages/@keld/kipc/scripts/echo-codegen.ts check",
            "bun fmt --no-script --check \"**/*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}\"",
        ])
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` hygiene needs one unconditional codegen freshness and direct native JS/TS formatting step."
        ));
    }
    let Some(block) = workflow_job_block(text, "bun-test") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `bun-test` job. The `packages/` TypeScript suites need a lane of their own: the Bun install inside `check` exists so Rust tests can spawn Bun children and never runs `bun test`."
        ));
    };
    let job_if_count = block
        .lines()
        .filter_map(yaml_content)
        .filter(|(indent, content)| {
            *indent == 4 && yaml_mapping_key(content).is_some_and(|(key, _)| key == "if")
        })
        .count();
    if job_if_count != 1
        || workflow_job_level_if(&block).as_deref() != Some("needs.changes.outputs.ts == 'true'")
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `bun-test` must be gated on `if: needs.changes.outputs.ts == 'true'`. The router owns which diffs reach this lane; an ungated job wastes runners and a gate on another output silently never runs (contexts: needs, github, vars, inputs only — never `matrix`)."
        ));
    }
    if workflow_job_level_property(&block, "runs-on").as_deref() != Some("${{ matrix.os }}")
        || !bun_job_has_static_three_os_matrix(&block)
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `bun-test` must use `runs-on: ${{{{ matrix.os }}}}` over `{THREE_OS_MATRIX}`. Bun roles run on every OS, and the GH-1095 three-OS rows (AT-B1, AT-B2, AT-API2a) are proven only there (#1111)."
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
            "CI-HYGIENE: `{WORKFLOW}` `bun-test` must not call `apt-get`. Bun ships from `oven-sh/setup-bun` and links no WebKitGTK, so an apt step there only adds Ubuntu mirror exposure (#630)."
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

/// The required gate inherits workflow-level environment, including Bash's
/// startup-file authority. Admit only the three reviewed non-executable values.
fn check_required_workflow_env(text: &str) -> Result<(), String> {
    let expected = [
        ("CARGO_TERM_COLOR", "always"),
        ("RUST_BACKTRACE", "1"),
        ("CARGO_INCREMENTAL", "0"),
    ];
    let mut found = false;
    let mut within_env = false;
    let mut actual = Vec::new();
    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if indent == 0 {
            if yaml_mapping_key(content).is_some_and(|(key, _)| key == "env") {
                if found || content != "env:" {
                    return Err(format!(
                        "CI-HYGIENE: {WORKFLOW} workflow env must have one reviewed block mapping, never a duplicate or override."
                    ));
                }
                found = true;
                within_env = true;
            } else {
                within_env = false;
            }
            continue;
        }
        if !within_env {
            continue;
        }
        if indent != 2 {
            return Err(format!(
                "CI-HYGIENE: {WORKFLOW} workflow env must contain only reviewed scalar bindings."
            ));
        }
        let (key, value) = yaml_mapping_key(content).ok_or_else(|| {
            format!("CI-HYGIENE: {WORKFLOW} workflow env has a malformed binding.")
        })?;
        actual.push((key, value.trim_matches(['\'', '"']).to_owned()));
    }
    if !found
        || actual.len() != expected.len()
        || expected
            .iter()
            .any(|(key, value)| !actual.iter().any(|(name, got)| name == key && got == value))
    {
        return Err(format!(
            "CI-HYGIENE: {WORKFLOW} workflow env must contain only the reviewed CARGO_TERM_COLOR, RUST_BACKTRACE, and CARGO_INCREMENTAL values. Extra BASH_ENV, PATH, or GITHUB_ENV can bypass the required gate."
        ));
    }
    Ok(())
}

fn check_required_job(text: &str) -> Result<(), String> {
    check_required_workflow_env(text)?;
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
    if workflow_job_level_property(&block, "env").is_some() {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` must not set job-level env; inherited BASH_ENV or PATH could bypass the evaluator."
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
        "router-contracts",
        "fmt",
        "check",
        "bun-test",
        "linux-gui-smoke",
        "msrv",
        "deny",
        "secrets",
        "hygiene",
        "mermaid",
        "codeql-rust",
        "codeql-javascript-typescript",
        "codeql-actions",
        "dependency-review",
        "workspace-contracts",
        "doctest",
        "windows-media",
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
    let expected_evaluator_env = [
        ("KELD_RESULT_CHANGES", "${{ needs.changes.result }}"),
        (
            "KELD_RESULT_ROUTER_CONTRACTS",
            "${{ needs['router-contracts'].result }}",
        ),
        ("KELD_RESULT_FMT", "${{ needs.fmt.result }}"),
        ("KELD_RESULT_CHECK", "${{ needs.check.result }}"),
        ("KELD_RESULT_BUN", "${{ needs['bun-test'].result }}"),
        ("KELD_RESULT_GUI", "${{ needs['linux-gui-smoke'].result }}"),
        ("KELD_RESULT_MSRV", "${{ needs.msrv.result }}"),
        ("KELD_RESULT_DENY", "${{ needs.deny.result }}"),
        ("KELD_RESULT_SECRETS", "${{ needs.secrets.result }}"),
        ("KELD_RESULT_HYGIENE", "${{ needs.hygiene.result }}"),
        ("KELD_RESULT_MERMAID", "${{ needs.mermaid.result }}"),
        ("KELD_ROUTE_RUST", "${{ needs.changes.outputs.rust }}"),
        ("KELD_ROUTE_TS", "${{ needs.changes.outputs.ts }}"),
        ("KELD_ROUTE_GUI", "${{ needs.changes.outputs.gui }}"),
        ("KELD_ROUTE_MSRV", "${{ needs.changes.outputs.msrv }}"),
        ("KELD_ROUTE_DENY", "${{ needs.changes.outputs.deny }}"),
        ("KELD_ROUTE_HYGIENE", "${{ needs.changes.outputs.hygiene }}"),
        ("KELD_ROUTE_DOCS", "${{ needs.changes.outputs.docs }}"),
        ("KELD_ROUTE_MERMAID", "${{ needs.changes.outputs.mermaid }}"),
        (
            "KELD_RESULT_CODEQL_RUST",
            "${{ needs['codeql-rust'].result }}",
        ),
        (
            "KELD_RESULT_CODEQL_JAVASCRIPT_TYPESCRIPT",
            "${{ needs['codeql-javascript-typescript'].result }}",
        ),
        (
            "KELD_RESULT_CODEQL_ACTIONS",
            "${{ needs['codeql-actions'].result }}",
        ),
        (
            "KELD_ROUTE_CODEQL_RUST",
            "${{ needs.changes.outputs.codeql_rust }}",
        ),
        (
            "KELD_ROUTE_CODEQL_JAVASCRIPT_TYPESCRIPT",
            "${{ needs.changes.outputs.codeql_javascript_typescript }}",
        ),
        (
            "KELD_ROUTE_CODEQL_ACTIONS",
            "${{ needs.changes.outputs.codeql_actions }}",
        ),
        (
            "KELD_RESULT_DEPENDENCY_REVIEW",
            "${{ needs['dependency-review'].result }}",
        ),
        (
            "KELD_RESULT_WORKSPACE",
            "${{ needs['workspace-contracts'].result }}",
        ),
        (
            "KELD_ROUTE_WORKSPACE",
            "${{ needs.changes.outputs.workspace }}",
        ),
        ("KELD_EVENT_NAME", "${{ github.event_name }}"),
        (
            "KELD_ROUTE_RUST_DOCUMENTATION_ONLY",
            "${{ needs.changes.outputs.rust_documentation_only }}",
        ),
        (
            "KELD_ROUTE_CHECK_OS",
            "${{ needs.changes.outputs.check_os }}",
        ),
        ("KELD_RESULT_DOCTEST", "${{ needs.doctest.result }}"),
        ("KELD_ROUTE_DOCTEST", "${{ needs.changes.outputs.doctest }}"),
        ("KELD_RESULT_MEDIA", "${{ needs['windows-media'].result }}"),
        (
            "KELD_ROUTE_MEDIA",
            "${{ needs.changes.outputs.rust == 'true' && contains(needs.changes.outputs.packages, 'keld-wv') }}",
        ),
    ];
    let actual_evaluator_env =
        workflow_named_step_mapping(&block, "Verify required CI results", "env").ok_or_else(
            || format!("CI-HYGIENE: `{WORKFLOW}` required evaluator env mapping must be a reviewed block mapping."),
        )?;
    for &(key, expression) in &expected_evaluator_env {
        if !actual_evaluator_env
            .iter()
            .any(|(name, value)| name == key && value == expression)
        {
            return Err(format!(
                "CI-HYGIENE: `{WORKFLOW}` `required` must bind `{key}: {expression}` in the evaluator step's `env` mapping. Do not let a missing or spoofed handoff erase the merge gate."
            ));
        }
    }
    if actual_evaluator_env.len() != expected_evaluator_env.len() {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` required evaluator env mapping must contain only the reviewed result and route bindings. Extra BASH_ENV, PATH, or GITHUB_ENV can bypass its exact run script."
        ));
    }
    let expected_check_command = concat!(
        "tools/ci_required.sh check ",
        "\"$KELD_RESULT_CHANGES\" \"$KELD_RESULT_FMT\" \"$KELD_RESULT_CHECK\" ",
        "\"$KELD_RESULT_BUN\" \"$KELD_RESULT_GUI\" \"$KELD_RESULT_MSRV\" ",
        "\"$KELD_RESULT_DENY\" \"$KELD_RESULT_SECRETS\" \"$KELD_RESULT_HYGIENE\" ",
        "\"$KELD_RESULT_MERMAID\" ",
        "\"$KELD_ROUTE_RUST\" \"$KELD_ROUTE_TS\" \"$KELD_ROUTE_GUI\" ",
        "\"$KELD_ROUTE_MSRV\" \"$KELD_ROUTE_DENY\" \"$KELD_ROUTE_HYGIENE\" ",
        "\"$KELD_ROUTE_DOCS\" \"$KELD_ROUTE_MERMAID\" ",
        "\"$KELD_RESULT_CODEQL_RUST\" \"$KELD_RESULT_CODEQL_JAVASCRIPT_TYPESCRIPT\" ",
        "\"$KELD_RESULT_CODEQL_ACTIONS\" ",
        "\"$KELD_ROUTE_CODEQL_RUST\" \"$KELD_ROUTE_CODEQL_JAVASCRIPT_TYPESCRIPT\" ",
        "\"$KELD_ROUTE_CODEQL_ACTIONS\" ",
        "\"$KELD_RESULT_DEPENDENCY_REVIEW\" ",
        "\"$KELD_RESULT_WORKSPACE\" \"$KELD_ROUTE_WORKSPACE\" ",
        "\"$KELD_EVENT_NAME\" \"$KELD_ROUTE_RUST_DOCUMENTATION_ONLY\" \"$KELD_ROUTE_CHECK_OS\" ",
        "\"$KELD_RESULT_DOCTEST\" \"$KELD_ROUTE_DOCTEST\" ",
        "\"$KELD_RESULT_MEDIA\" \"$KELD_ROUTE_MEDIA\" ",
        "\"$KELD_RESULT_ROUTER_CONTRACTS\""
    );
    let expected_commands = [
        "tools/ci_required.sh test".to_owned(),
        expected_check_command.to_owned(),
    ];
    let actual_commands = workflow_named_step_shell_commands(&block, "Verify required CI results")
        .unwrap_or_default();
    if actual_commands != expected_commands {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `required` evaluator run block must contain only its self-test and the exact ordered 35-argument check, without control flow, reassignment, wrappers, or exit-status suppression."
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

/// doc-placeholders runs whenever the hygiene job runs (docs or hygiene
/// selected); every input it scans selects one of them (#650).
fn check_doc_placeholders_step(text: &str) -> Result<(), String> {
    check_hygiene_contract_step(
        text,
        "Documentation placeholder contracts",
        DOC_PLACEHOLDER_COMMANDS,
    )
}

/// spec-structure runs whenever the hygiene job runs; `docs/*` selects it (#1321).
fn check_spec_structure_step(text: &str) -> Result<(), String> {
    check_hygiene_contract_step(text, "Spec structure contracts", SPEC_STRUCTURE_COMMANDS)
}

/// The hello-command regression reads the justfile and onboarding docs (#650).
fn check_hello_command_step(text: &str) -> Result<(), String> {
    check_hygiene_contract_step(text, "Hello command contract", HELLO_COMMAND_COMMANDS)
}

fn check_agent_context_step(text: &str) -> Result<(), String> {
    check_hygiene_contract_step(
        text,
        "Agent instruction context budget",
        AGENT_CONTEXT_COMMANDS,
    )
}

fn check_public_audit_step(text: &str) -> Result<(), String> {
    let Some(hygiene) = workflow_job_block(text, "hygiene") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `hygiene` job for public audit contracts."
        ));
    };
    if !workflow_has_checkout_fetch_depth_zero(&hygiene) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `hygiene` must checkout full history (`fetch-depth: 0`) because public audit verification binds historical commits."
        ));
    }
    let step = "Public audit registry contracts";
    if workflow_direct_named_step_count(&hygiene, step) != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `hygiene` must contain exactly one `{step}` step."
        ));
    }
    let block = workflow_direct_named_step_block(&hygiene, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of hygiene.steps.")
    })?;
    let expected_keys = ["if".to_owned(), "run".to_owned()];
    if workflow_named_step_direct_keys(&block, step).as_deref() != Some(expected_keys.as_slice()) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must contain only its docs condition and direct run block."
        ));
    }
    let condition = "needs.changes.outputs.docs == 'true'";
    if workflow_named_step_direct_value(&block, step, "if").as_deref() != Some(condition) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must use exact condition `{condition}`."
        ));
    }
    let commands = workflow_named_step_shell_commands(&block, step)
        .ok_or_else(|| format!("CI-HYGIENE: `{WORKFLOW}` `{step}` has no executable run block."))?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(AUDIT_DOC_COMMANDS.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must run the public audit verifier and mutation suite exactly."
        ));
    }
    Ok(())
}

fn check_gitleaks_scan_step(text: &str) -> Result<(), String> {
    let Some(secrets) = workflow_job_block(text, "secrets") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no unconditional `secrets` (gitleaks) job."
        ));
    };
    let step = "Scan";
    let count = workflow_direct_named_step_count(&secrets, step);
    if count != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `secrets` must contain exactly one `{step}` step; found {count}."
        ));
    }
    let block = workflow_direct_named_step_block(&secrets, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `secrets.steps`.")
    })?;
    if workflow_named_step_direct_keys(&block, step).as_deref()
        != Some(["env".to_owned(), "run".to_owned()].as_slice())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` gitleaks `{step}` may contain only its env and run keys; conditions and wrappers can skip or soften the secrets gate."
        ));
    }
    let expected_env: Vec<(String, String)> = GITLEAKS_SCAN_ENV
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    if workflow_named_step_mapping(&secrets, step, "env").as_deref()
        != Some(expected_env.as_slice())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` gitleaks `{step}` must bind exactly the event name and the pull request's base and head SHAs in `env`, never interpolate them into `run`."
        ));
    }
    let commands = workflow_named_step_shell_commands(&block, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` gitleaks `{step}` has no multiline run block.")
    })?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(GITLEAKS_SCAN_COMMANDS.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` gitleaks `{step}` must run the exact scoped scan: resolve both pull-request SHAs to local commits, scan push history at HEAD, refuse other events, and pass the range with --log-opts. An unscoped, empty or unresolvable range lets the secrets gate pass without scanning the change."
        ));
    }
    Ok(())
}

/// GH-508: the step that runs `ci-hygiene check` binds the comparison base the
/// append-only rule needs: the PR base, or on push the previous tip. Without it
/// a push to main would compare the baseline with itself.
fn check_channel_allocations_step(text: &str) -> Result<(), String> {
    let step = CHANNEL_ALLOCATIONS_STEP;
    let Some(hygiene) = workflow_job_block(text, "hygiene") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no `hygiene` job for `{step}`."
        ));
    };
    let count = workflow_direct_named_step_count(&hygiene, step);
    if count != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `hygiene` must contain exactly one `{step}` step; found {count}."
        ));
    }
    let block = workflow_direct_named_step_block(&hygiene, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `hygiene.steps`.")
    })?;
    let expected_keys = ["if".to_owned(), "env".to_owned(), "run".to_owned()];
    if workflow_named_step_direct_keys(&block, step).as_deref() != Some(expected_keys.as_slice()) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` may contain only its `if`, `env` and `run` keys; a custom shell or continue-on-error can erase the append-only channel allocation rule."
        ));
    }
    if workflow_named_step_direct_value(&block, step, "if").as_deref()
        != Some(CHANNEL_ALLOCATIONS_STEP_IF)
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must run exactly when the router selects hygiene (`if: {CHANNEL_ALLOCATIONS_STEP_IF}`)."
        ));
    }
    let expected_env: Vec<(String, String)> = CHANNEL_ALLOCATIONS_STEP_ENV
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    if workflow_named_step_mapping(&hygiene, step, "env").as_deref()
        != Some(expected_env.as_slice())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must bind exactly `{CHANNEL_ALLOCATIONS_BASE_ENV}: ${{{{ github.event.pull_request.base.sha || github.event.before }}}}` so {CHANNEL_ALLOCATIONS} is compared with the PR base or the previous push tip, never with itself."
        ));
    }
    let commands = workflow_named_step_shell_commands(&block, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` has no executable multiline `run` block.")
    })?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(CHANNEL_ALLOCATIONS_STEP_COMMANDS.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must compile and run `ci-hygiene check .` exactly, without wrappers or exit suppression."
        ));
    }
    Ok(())
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
    let Some(check_job) = workflow_job_block(text, "windows-media") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no isolated `windows-media` job for KEL-132 Windows media acceptance."
        ));
    };
    let routing = "needs.changes.outputs.rust == 'true' && contains(needs.changes.outputs.packages, 'keld-wv')";
    if workflow_job_level_property(&check_job, "runs-on").as_deref() != Some("windows-latest")
        || workflow_job_level_property(&check_job, "needs").as_deref() != Some("changes")
        || workflow_job_level_if(&check_job).as_deref() != Some(routing)
        || workflow_job_level_property(&check_job, "continue-on-error").is_some()
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `windows-media` must run on Windows, need the router, select only Rust/keld-wv changes, and preserve failures (KEL-132)."
        ));
    }
    if workflow_job_block(text, "check").is_some_and(|job| {
        workflow_direct_named_step_count(&job, "Windows media guard acceptance (KEL-132)") != 0
    }) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` KEL-132 media acceptance must not remain in the serial cross-platform `check` job."
        ));
    }
    let step = "Windows media guard acceptance (KEL-132)";
    let count = workflow_direct_named_step_count(&check_job, step);
    if count != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `windows-media` must contain exactly one `{step}` step; found {count}."
        ));
    }
    let block = workflow_direct_named_step_block(&check_job, step).ok_or_else(|| {
        format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `windows-media.steps`."
        )
    })?;
    let expected_keys = ["shell".to_owned(), "run".to_owned()];
    if workflow_named_step_direct_keys(&block, step).as_deref() != Some(expected_keys.as_slice()) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must contain only `shell: pwsh` and `run` keys, without a step-level skip."
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

fn check_release_updater_helper_step(text: &str) -> Result<(), String> {
    let step = RELEASE_HELPER_STEP;
    let Some(check_job) = workflow_job_block(text, "check") else {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` has no cross-platform `check` job for `{step}`."
        ));
    };
    let count = workflow_direct_named_step_count(&check_job, step);
    if count != 1 {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `check` must contain exactly one `{step}` step; found {count}."
        ));
    }
    let block = workflow_direct_named_step_block(&check_job, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` must be a direct child of `check.steps`.")
    })?;
    let expected_keys = ["if", "shell", "env", "run"].map(str::to_owned);
    if workflow_named_step_direct_keys(&block, step).as_deref() != Some(expected_keys.as_slice()) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must contain only the exact `if`, `shell: bash`, `env`, and `run` keys."
        ));
    }
    if workflow_named_step_direct_value(&block, step, "if").as_deref()
        != Some(RELEASE_HELPER_CONDITION)
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must use the exact condition `{RELEASE_HELPER_CONDITION}`: the helper, its keld-pack writer and the KEL-19 host trigger."
        ));
    }
    if workflow_named_step_direct_value(&block, step, "shell").as_deref() != Some("bash") {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must set `shell: bash`; `set -euo pipefail` must fail the step on a failed build."
        ));
    }
    let expected_env: Vec<(String, String)> = RELEASE_HELPER_ENV
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    if workflow_named_step_mapping(&block, step, "env") != Some(expected_env) {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must name the absolute release helper in exactly `KELD_UPDATER_HELPER_RELEASE`, `KELD_PACK_REAL_HOST` and `KELD_PACK_REAL_HELPER`, and the AC2 output in `KELD_PACK_EMBEDDED_HELPER` under the runner temp directory."
        ));
    }
    let commands = workflow_named_step_shell_commands(&block, step).ok_or_else(|| {
        format!("CI-HYGIENE: `{WORKFLOW}` `{step}` has no executable multiline run block.")
    })?;
    if commands
        .iter()
        .map(String::as_str)
        .ne(RELEASE_HELPER_COMMANDS.iter().copied())
    {
        return Err(format!(
            "CI-HYGIENE: `{WORKFLOW}` `{step}` must build the release helper and require both ignored release-image passes, the AC1 round trip and the AC2 helper launch, without wrappers or suppression."
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
    // No checkout or `run:` step under pull_request_target: owned by the parsed
    // check in tools/ci_workflow_security.ts, which covers every such workflow.
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
                the hygiene job that compiles this file, \
                 generated-doc freshness, and the structural plus digest-pinned Mermaid render gates."
            ));
        }
    }

    check_change_router_job(&text)?;
    check_independent_router_contract_job(&text)?;
    check_mermaid_job(&text)?;
    check_package_loop_shell(&text)?;
    check_check_job_if_avoids_matrix(&text)?;
    check_check_job_os_matrix(&text)?;
    check_workspace_contracts_job(&text)?;
    check_doctest_job(&text)?;
    check_fuzz_workspace_step(&text)?;
    check_msrv_avoids_apt(&text)?;
    check_bun_test_job(&text)?;
    check_linux_media_guard_step(&text)?;
    check_required_job(&text)?;
    check_gitleaks_scan_step(&text)?;
    check_channel_allocations_step(&text)?;
    check_product_status_step(&text)?;
    check_product_status_windows_step(&text)?;
    check_windows_media_acceptance_step(&text)?;
    check_release_updater_helper_step(&text)?;
    check_atomic_protocol_step(&text)?;
    check_doc_placeholders_step(&text)?;
    check_spec_structure_step(&text)?;
    check_hello_command_step(&text)?;
    check_agent_context_step(&text)?;
    check_public_audit_step(&text)?;
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
        "run_with_timeout 8 docker info",
        "run_with_timeout 8 docker image inspect",
        "run_with_timeout 120 docker run",
        "run_with_timeout 300 docker pull",
        "--pull never",
        "--jobs 2",
        ":/input/source.md:ro",
        "docker_host_path",
        "MSYS2_ARG_CONV_EXCL='*'",
        "trap cleanup EXIT",
        r#"workspace=$(cd "$workspace" && pwd -P)"#,
        r#"if [[ ! -x "$mermaid_docs" || "$workspace/tools/mermaid_docs.rs" -nt "$mermaid_docs" ]]; then"#,
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
    let manifest = renderer.find("\"$mermaid_docs\" manifest");
    let empty_guard = renderer.find("if [[ ${#files[@]} -eq 0 ]]");
    let docker_probe = renderer.find("run_with_timeout 8 docker info");
    if !matches!((manifest, empty_guard, docker_probe), (Some(m), Some(g), Some(d)) if m < g && g < d)
        || renderer.contains("git -C \"$workspace\" grep -Ilzi 'mermaid'")
    {
        return Err(format!(
            "CI-HYGIENE: `{MERMAID_RENDERER}` must obtain actual block candidates from the Mermaid parser and return on zero candidates before a bounded Docker probe."
        ));
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
    check_root_audit_contract(root)?;
    check_root_test_display_contract(root)?;
    check_serial_rust_gates(root)?;
    check_mermaid_local_gate(root)?;
    check_codeowners(root)?;
    check_pr_template(root)?;
    check_issue_templates(root)?;
    check_workflow(root)?;
    check_keldbot_workflow(root)?;
    check_mermaid_gate_files(root)?;
    Ok(())
}

/// GH-508 spec criterion 3a: the baseline at the merge base must be an exact
/// line prefix of the current baseline. `None` means the file is absent there.
fn check_allocations_prefix(
    base: Option<&str>,
    current: Option<&str>,
    merge_base: &str,
) -> Result<(), String> {
    let Some(base) = base else {
        return Ok(());
    };
    let Some(current) = current else {
        return Err(format!(
            "CI-HYGIENE: {CHANNEL_ALLOCATIONS} was removed, but it exists at merge base \
             {merge_base}. Restore it: kipc channel ids are append-only \
             (docs/specs/gh508-kipc-channel-table.md §4.4)."
        ));
    };
    let mut current_lines = current.lines();
    for (index, expected) in base.lines().enumerate() {
        if current_lines.next() != Some(expected) {
            return Err(format!(
                "CI-HYGIENE: {CHANNEL_ALLOCATIONS} line {} `{expected}` at merge base \
                 {merge_base} was changed, reordered or removed. Restore every existing \
                 line and append new allocations at the end; ids are never renumbered or \
                 reused (docs/specs/gh508-kipc-channel-table.md §4.4).",
                index + 1
            ));
        }
    }
    Ok(())
}

fn git_output(root: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| {
            format!("CI-HYGIENE: cannot run git for {CHANNEL_ALLOCATIONS}: {error}. Install git and rerun just hygiene.")
        })
}

/// Resolves the merge base of `base_ref` and `HEAD`, failing closed when the
/// base is unavailable instead of passing without a comparison.
fn allocations_merge_base(root: &Path, base_ref: &str) -> Result<String, String> {
    let unresolved = |detail: &str| {
        format!(
            "CI-HYGIENE: cannot resolve the {CHANNEL_ALLOCATIONS} comparison base `{base_ref}` \
             ({detail}). Run `git fetch origin main`, or set {CHANNEL_ALLOCATIONS_BASE_ENV} to an \
             available commit, then rerun just hygiene; the append-only check never passes \
             without a base."
        )
    };
    if base_ref.is_empty() || base_ref.bytes().all(|byte| byte == b'0') {
        return Err(unresolved(
            "empty or all-zero; a first push has no comparison base",
        ));
    }
    let verify = git_output(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{base_ref}^{{commit}}"),
        ],
    )?;
    if !verify.status.success() {
        return Err(unresolved("not a commit in this repository"));
    }
    let merge_base = git_output(root, &["merge-base", base_ref, "HEAD"])?;
    if !merge_base.status.success() {
        return Err(unresolved("no merge base with HEAD"));
    }
    let sha = String::from_utf8_lossy(&merge_base.stdout)
        .trim()
        .to_owned();
    if sha.is_empty() {
        return Err(unresolved("empty merge base"));
    }
    Ok(sha)
}

/// The baseline file at `merge_base`, or `None` when that commit lacks it.
fn allocations_at(root: &Path, merge_base: &str) -> Result<Option<String>, String> {
    let listed = git_output(
        root,
        &[
            "ls-tree",
            "--name-only",
            merge_base,
            "--",
            CHANNEL_ALLOCATIONS,
        ],
    )?;
    if !listed.status.success() {
        return Err(format!(
            "CI-HYGIENE: cannot list {CHANNEL_ALLOCATIONS} at merge base {merge_base}: {}",
            String::from_utf8_lossy(&listed.stderr).trim()
        ));
    }
    if String::from_utf8_lossy(&listed.stdout).trim().is_empty() {
        return Ok(None);
    }
    let shown = git_output(
        root,
        &["show", &format!("{merge_base}:{CHANNEL_ALLOCATIONS}")],
    )?;
    if !shown.status.success() {
        return Err(format!(
            "CI-HYGIENE: cannot read {CHANNEL_ALLOCATIONS} at merge base {merge_base}: {}",
            String::from_utf8_lossy(&shown.stderr).trim()
        ));
    }
    String::from_utf8(shown.stdout).map(Some).map_err(|_| {
        format!("CI-HYGIENE: {CHANNEL_ALLOCATIONS} at merge base {merge_base} is not UTF-8.")
    })
}

/// GH-508 criterion 3a: the committed channel allocation baseline only grows.
fn check_channel_allocations_append_only(root: &Path, base_ref: &str) -> Result<(), String> {
    let merge_base = allocations_merge_base(root, base_ref)?;
    let base = allocations_at(root, &merge_base)?;
    let current = match fs::read_to_string(root.join(CHANNEL_ALLOCATIONS)) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "CI-HYGIENE: cannot read {CHANNEL_ALLOCATIONS}: {error}."
            ));
        }
    };
    check_allocations_prefix(base.as_deref(), current.as_deref(), &merge_base)
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
            let base_ref = env::var(CHANNEL_ALLOCATIONS_BASE_ENV)
                .unwrap_or_else(|_| CHANNEL_ALLOCATIONS_DEFAULT_BASE.to_owned());
            check_channel_allocations_append_only(&root, &base_ref)
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
#[path = "ci_hygiene_tests.rs"]
mod tests;
