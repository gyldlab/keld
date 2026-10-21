# Native negative controls for GH-894. No signature or trust fixture is created.
# An actual Keld-signed release is still required for the positive operator gate.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$gate = Join-Path $PSScriptRoot 'windows_signed_installed_acceptance.ps1'
$powershell = Join-Path $PSHOME 'powershell.exe'
function Assert-GateRefuses([string]$Case, [string[]]$GateArgs, [string]$Reason) {
    # Windows PowerShell surfaces redirected native stderr as a nonterminating
    # NativeCommandError; capture it as evidence rather than promoting it to a
    # harness exception before the child's exit code can be inspected.
    $ErrorActionPreference = 'Continue'
    try {
        $output = (& $powershell '-NoProfile' '-NonInteractive' '-File' $gate @GateArgs 2>&1 | Out-String)
        $exit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
    }
    if ($exit -eq 0 -or $output -notmatch [regex]::Escape($Reason) -or
        $output -match 'KELD_RELEASE_SIGNED_INSTALLED_PASS') {
        throw "GH-894 $Case did not fail closed: exit=$exit output=$output"
    }
    Write-Output "GH894_NEGATIVE_PASS case=$Case"
}
Assert-GateRefuses 'missing-required-signer' @() 'missing required SignedHost'

$git = Join-Path $env:ProgramFiles 'Git/cmd/git.exe'
if (-not (Test-Path -LiteralPath $git -PathType Leaf)) {
    throw 'GH-894 native controls need an installed, independently signed Git for Windows image'
}
$signature = Get-AuthenticodeSignature -LiteralPath $git
if ($signature.Status.ToString() -cne 'Valid' -or
    $signature.SignatureType.ToString() -cne 'Authenticode' -or
    $null -eq $signature.SignerCertificate) {
    throw 'GH-894 input must be a genuinely embedded-signed, valid Git for Windows executable'
}
$sha = (Get-FileHash -LiteralPath $git -Algorithm SHA256).Hash
$cert = $signature.SignerCertificate.Thumbprint
$fakeScope = '0' * 64
$fakeThumbprint = '0' * 40
$missingExe = Join-Path $env:TEMP 'gh894-native-fixture-does-not-exist.exe'
if (Test-Path -LiteralPath $missingExe) {
    throw 'GH-894 chosen absent native fixture path unexpectedly exists'
}
$normal = @(
    '-SignedHost', $git,
    '-ExpectedReleaseSha256', $sha,
    '-ExpectedCertificateThumbprint', $cert,
    '-ExpectedAppId', 'dev.keld.operator',
    '-ExpectedPublisherScope', $fakeScope,
    '-CoreAcceptanceExe', $missingExe,
    '-ExpectedCoreAcceptanceSha256', $fakeScope,
    '-UpdateInstallerExe', $missingExe,
    '-ExpectedUpdateInstallerSha256', $fakeScope,
    '-HostAcceptanceExe', $missingExe,
    '-ExpectedHostAcceptanceSha256', $fakeScope
)
Assert-GateRefuses 'bad-pins' @(
    '-SignedHost', $git,
    '-ExpectedReleaseSha256', $sha,
    '-ExpectedCertificateThumbprint', 'not-a-thumbprint',
    '-ExpectedAppId', 'dev.keld.operator',
    '-ExpectedPublisherScope', $fakeScope,
    '-CoreAcceptanceExe', $missingExe,
    '-ExpectedCoreAcceptanceSha256', $fakeScope,
    '-UpdateInstallerExe', $missingExe,
    '-ExpectedUpdateInstallerSha256', $fakeScope,
    '-HostAcceptanceExe', $missingExe,
    '-ExpectedHostAcceptanceSha256', $fakeScope
) 'malformed pinned digest'
Assert-GateRefuses 'absent-release-image' @(
    '-SignedHost', $missingExe,
    '-ExpectedReleaseSha256', $sha,
    '-ExpectedCertificateThumbprint', $cert,
    '-ExpectedAppId', 'dev.keld.operator',
    '-ExpectedPublisherScope', $fakeScope,
    '-CoreAcceptanceExe', $missingExe,
    '-ExpectedCoreAcceptanceSha256', $fakeScope,
    '-UpdateInstallerExe', $missingExe,
    '-ExpectedUpdateInstallerSha256', $fakeScope,
    '-HostAcceptanceExe', $missingExe,
    '-ExpectedHostAcceptanceSha256', $fakeScope
) 'SignedHost is not an existing file'
Assert-GateRefuses 'release-hash-mismatch' @(
    '-SignedHost', $git,
    '-ExpectedReleaseSha256', $fakeScope,
    '-ExpectedCertificateThumbprint', $cert,
    '-ExpectedAppId', 'dev.keld.operator',
    '-ExpectedPublisherScope', $fakeScope,
    '-CoreAcceptanceExe', $missingExe,
    '-ExpectedCoreAcceptanceSha256', $fakeScope,
    '-UpdateInstallerExe', $missingExe,
    '-ExpectedUpdateInstallerSha256', $fakeScope,
    '-HostAcceptanceExe', $missingExe,
    '-ExpectedHostAcceptanceSha256', $fakeScope
) 'signed release SHA256 differs'
Assert-GateRefuses 'certificate-mismatch' @(
    '-SignedHost', $git,
    '-ExpectedReleaseSha256', $sha,
    '-ExpectedCertificateThumbprint', $fakeThumbprint,
    '-ExpectedAppId', 'dev.keld.operator',
    '-ExpectedPublisherScope', $fakeScope,
    '-CoreAcceptanceExe', $missingExe,
    '-ExpectedCoreAcceptanceSha256', $fakeScope,
    '-UpdateInstallerExe', $missingExe,
    '-ExpectedUpdateInstallerSha256', $fakeScope,
    '-HostAcceptanceExe', $missingExe,
    '-ExpectedHostAcceptanceSha256', $fakeScope
) 'signer certificate thumbprint differs'
Assert-GateRefuses 'absent-native-selector-executable' $normal 'CoreAcceptanceExe is not an existing file'
Assert-GateRefuses 'mismatched-native-test-digest' @(
    '-SignedHost', $git,
    '-ExpectedReleaseSha256', $sha,
    '-ExpectedCertificateThumbprint', $cert,
    '-ExpectedAppId', 'dev.keld.operator',
    '-ExpectedPublisherScope', $fakeScope,
    '-CoreAcceptanceExe', $git,
    '-ExpectedCoreAcceptanceSha256', $fakeScope,
    '-UpdateInstallerExe', $missingExe,
    '-ExpectedUpdateInstallerSha256', $fakeScope,
    '-HostAcceptanceExe', $missingExe,
    '-ExpectedHostAcceptanceSha256', $fakeScope
) 'CoreAcceptanceExe differs from the independently pinned native test artifact'
$catalog = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
$catalogSig = Get-AuthenticodeSignature -LiteralPath $catalog
if ($catalogSig.Status.ToString() -cne 'Valid' -or
    $catalogSig.SignatureType.ToString() -cne 'Catalog') {
    throw 'GH-894 expected a valid catalog-only native Windows binary for this OS negative control'
}
Assert-GateRefuses 'catalog-is-not-embedded-signature' @(
    '-SignedHost', $catalog,
    '-ExpectedReleaseSha256', (Get-FileHash -LiteralPath $catalog -Algorithm SHA256).Hash,
    '-ExpectedCertificateThumbprint', $catalogSig.SignerCertificate.Thumbprint,
    '-ExpectedAppId', 'dev.keld.operator',
    '-ExpectedPublisherScope', $fakeScope,
    '-CoreAcceptanceExe', $missingExe,
    '-ExpectedCoreAcceptanceSha256', $fakeScope,
    '-UpdateInstallerExe', $missingExe,
    '-ExpectedUpdateInstallerSha256', $fakeScope,
    '-HostAcceptanceExe', $missingExe,
    '-ExpectedHostAcceptanceSha256', $fakeScope
) 'host lacks a valid embedded Authenticode certificate'
Write-Output 'GH894_NEGATIVE_SUITE_PASS count=8 genuine_signatures=Git-embedded,Windows-catalog positive_signed_keld=unrun'
