# Standalone owned fixture lifecycle and custody checks. No installer, signing, SDK
# fixtures or legacy negative suite. The Rust test owns fixture retirement.
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'Ordinary NSIS fixture lifecycle tests require Windows'
}

$prefix = 'windows_fs::tests::nsis_fixture::nsis_fixture_'
$expected = @(
    ($prefix + 'ordinary_cleanup_after_powershell_payload_refusal')
    ($prefix + 'ordinary_cleanup_after_powershell_smoke')
    ($prefix + 'ordinary_cleanup_retains_independent_extra_file')
    ($prefix + 'ordinary_cleanup_uses_only_owned_handles')
    ($prefix + 'ordinary_lifecycle_exclusive_creation_and_in_place_rewrite')
    ($prefix + 'ordinary_lifecycle_move_then_retire_child_before_parent')
    ($prefix + 'ordinary_staging_cleans_partial_population')
    ($prefix + 'separate_generation_record_preserves_ordinary_members')
    ($prefix + 'cleanup_refuses_same_name_foreign_identity')
    ($prefix + 'cleanup_refuses_live_reparse_object')
)
$expectedRegistrations = @($expected | ForEach-Object { $_ + ': test' })
$expectedResults = @($expected | ForEach-Object { 'test ' + $_ + ' ... ok' })

# Resolve an actual native application once; do not invoke a shell alias.
$cargoPath = (Get-Command cargo.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Path

Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    # Cargo can exit zero when a filter matches nothing. Refuse any change to
    # this exact owned fixture suite before a test executes.
    $listing = @(& $cargoPath test --locked --color never -p keld-update --lib $prefix -- --list)
    $discoveryExit = $LASTEXITCODE
    if ($discoveryExit -ne 0) {
        throw "Ordinary NSIS fixture discovery failed (cargo exit $discoveryExit)"
    }
    $registered = @($listing | Where-Object { $_ -cmatch ': (test|benchmark)$' })
    if ($registered.Count -ne $expectedRegistrations.Count -or
        @($expectedRegistrations | Where-Object { $_ -cnotin $registered }).Count -ne 0) {
        throw 'Ordinary NSIS fixture discovery did not match the exact ten expected tests'
    }

    $results = @(& $cargoPath test --locked --color never -p keld-update --lib $prefix -- --test-threads=1)
    $executionExit = $LASTEXITCODE
    if ($executionExit -ne 0) {
        throw "Ordinary NSIS fixture tests failed (cargo exit $executionExit)"
    }
    $caseResults = @($results | Where-Object { $_ -cmatch '^test .+ \.\.\. ' })
    $summaries = @($results | Where-Object { $_ -cmatch '^test result:' })
    if ($caseResults.Count -ne $expectedResults.Count -or
        @($expectedResults | Where-Object { $_ -cnotin $caseResults }).Count -ne 0 -or
        $summaries.Count -ne 1 -or
        $summaries[0] -cnotmatch '^test result: ok\. 10 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out;') {
        throw 'Ordinary NSIS fixture lifecycle did not pass all ten exact tests'
    }
} finally {
    Pop-Location
}

# Expose only validated outcome lines, not child output resembling a verdict.
$caseResults | Write-Output
$summaries | Write-Output

# Includes the Rust cases' explicit retirement/absence assertions. It is not
# a verdict for the legacy default suite, installer or release.
Write-Output 'KEL270_NSIS_ORDINARY_FIXTURE_LIFECYCLE_PASS count=10 owned_retirement=VERIFIED installer=NOT_RUN'
