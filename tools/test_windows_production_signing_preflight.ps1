# Independent Windows regression of the read-only production signing preflight.
# No SignTool sign, certificate creation/import, private-key export or trust edits.
[CmdletBinding()]
param(
    [string]$FixtureRoot,
    [string]$TestCertificateThumbprint,
    [string]$PinnedSignToolPath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$check = Join-Path $PSScriptRoot 'windows_production_signing_preflight.ps1'
if ([string]::IsNullOrWhiteSpace($FixtureRoot)) {
    $root = Join-Path ([IO.Path]::GetTempPath()) ('keld-production-policy-'+[Guid]::NewGuid().ToString('N'))
} else {
    $root = $FixtureRoot
}
if (Test-Path -LiteralPath $root) { throw 'Preflight fixture exists; never overwrite an earlier test' }
[IO.Directory]::CreateDirectory($root) | Out-Null
$head = 'a' * 40
$testCert = if ($TestCertificateThumbprint) { $TestCertificateThumbprint } else { 'F' * 40 }
$signer = $PinnedSignToolPath
$hasSigner = [bool]($signer -and [IO.File]::Exists($signer))
if ($hasSigner) {
    $sha = [Security.Cryptography.SHA256]::Create().ComputeHash([IO.File]::ReadAllBytes($signer))
    $hash = [BitConverter]::ToString($sha).Replace('-','').ToLowerInvariant()
} else {
    $hash = 'c' * 64
}
$fixture = [ordered]@{
    schema = 'keld.windows.production-signing-policy/v1'
    sourceCommit = $head
    publisherName = 'Keld Company'
    publisherCertificateThumbprint = $testCert
    trustedRootThumbprint = 'A' * 40
    rfc3161TimestampUrl = 'https://timestamp.example.com'
    signerToolSha256 = $hash
    updatePublicKeySha256 = 'b' * 64
}
function Save-Case([string]$name, [string]$json) {
    $p = Join-Path $root ($name+'.json')
    [IO.File]::WriteAllText($p,$json+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
    return $p
}
$script:caseCount = 0
function Run-Case([string]$name, [string]$policy, [bool]$syntaxOnly, [int]$expectedExit, [string]$expectedText) {
    $ps = Join-Path $PSHOME 'powershell.exe'
    $extra = if ($syntaxOnly) { '-SyntaxOnly' } else { '-SignToolPath "'+$signer+'"' }
    $args = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "'+$check+'" -PolicyPath "'+$policy+'" -ExpectedSourceCommit "'+$head+'" '+$extra
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $ps
    $start.Arguments = $args
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.CreateNoWindow = $true
    $child = [Diagnostics.Process]::Start($start)
    $stdout = $child.StandardOutput.ReadToEnd()
    $stderr = $child.StandardError.ReadToEnd()
    if (-not $child.WaitForExit(20000)) {
        $child.Kill()
        throw ('Preflight child timed out: '+$name)
    }
    $result = $stdout + [Environment]::NewLine + $stderr
    if ($child.ExitCode -ne $expectedExit -or $result -notmatch [regex]::Escape($expectedText)) {
        throw ('Case '+$name+' expected exit '+$expectedExit+', got '+$child.ExitCode+': '+$result.Substring(0,[Math]::Min($result.Length,700)))
    }
    $script:caseCount++
    Write-Output ('R78_CASE_PASS '+$name+' exit='+$child.ExitCode)
}
$goodJson = $fixture | ConvertTo-Json -Depth 4
$good = Save-Case 'closed-policy' $goodJson
Run-Case 'syntax-check-never-grants-release' $good $true 0 '"productionReleaseQualified":false'
$duplicate = Save-Case 'duplicate-key' ($goodJson.Replace('"sourceCommit":',(' "sourceCommit": "'+$head+'", "sourceCommit":')))
Run-Case 'duplicate-policy-key-rejected' $duplicate $true 1 'PROD-PRE-009'
$secret = Save-Case 'injected-private-key' ($goodJson.Replace('"schema":','"privateKey": "unsafe", "schema":'))
Run-Case 'private-key-property-rejected' $secret $true 1 'PROD-PRE-009'
$http = Save-Case 'http-timestamp' ($goodJson.Replace('https://timestamp.example.com','http://timestamp.example.com'))
Run-Case 'non-https-timestamp-rejected' $http $true 1 'PROD-PRE-020'
$badCommit = Save-Case 'commit-mismatch' ($goodJson.Replace($head,('0'*40)))
Run-Case 'wrong-commit-rejected' $badCommit $true 1 'PROD-PRE-015'
$badRoot = Save-Case 'same-root-leaf' ($goodJson.Replace(('A'*40),$testCert))
Run-Case 'leaf-equals-root-rejected' $badRoot $true 1 'PROD-PRE-019'
$escaped = Save-Case 'json-escape' ($goodJson.Replace('Keld Company','Keld\u0020Company'))
Run-Case 'unreviewed-json-escape-rejected' $escaped $true 1 'PROD-PRE-008'
# A lexical D:\ path can still traverse an NTFS junction (reparse ancestor).
# The policy verifier must reject it before opening the apparently local file.
$realParent = Join-Path $root 'real-policy-parent'
[IO.Directory]::CreateDirectory($realParent) | Out-Null
[IO.File]::Copy($good, (Join-Path $realParent 'policy.json'))
$junctionParent = Join-Path $root 'junction-policy-parent'
New-Item -ItemType Junction -Path $junctionParent -Target $realParent -ErrorAction Stop | Out-Null
Run-Case 'junction-ancestor-rejected' (Join-Path $junctionParent 'policy.json') $true 1 'PROD-PRE-004'
Run-Case 'trailing-dot-Win32-alias-rejected' ($good+'.') $true 1 'PROD-PRE-002'
Run-Case 'trailing-space-Win32-alias-rejected' ($good+' ') $true 1 'PROD-PRE-002'
Run-Case 'missing-policy-rejected' (Join-Path $root 'absent.json') $true 1 'PROD-PRE-003'
if ($hasSigner) {
    $wrongTool = Save-Case 'wrong-signtool-hash' ($goodJson.Replace($hash,('c'*64)))
    Run-Case 'signtool-pin-drift-rejected' $wrongTool $false 1 'PROD-PRE-022'
    $absentCert = Save-Case 'missing-signing-cert' ($goodJson.Replace($testCert,('B'*40)))
    Run-Case 'unavailable-publisher-rejected' $absentCert $false 1 'PROD-PRE-024'
    $present = @(
        Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue |
            Where-Object { $_.Thumbprint -ceq $testCert -and $_.Subject -ceq $_.Issuer -and $_.NotAfter -gt (Get-Date) }
    )
    if ($TestCertificateThumbprint -and $present.Count -eq 1) {
        Run-Case 'self-signed-test-publisher-rejected' $good $false 1 'PROD-PRE-027'
    } else {
        Write-Output 'R78_CASE_NOT_RUN self-issued test cert not available on this machine'
    }
} else {
    Write-Output 'R78_CASE_NOT_RUN signer-dependent controls need an explicitly selected TEST SignTool'
}
Write-Output ('R78_PRODUCTION_POLICY_CASES_PASS_'+$script:caseCount)
