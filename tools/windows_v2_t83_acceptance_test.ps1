# KEL-270 T83 native PowerShell 5.1 source-only negative controls.
# No Keld installer, Host, CLI, provider or recovery role is launched.
# The signed Git and Windows catalog images are third-party OS witnesses only;
# they cannot commission a Keld publisher or create a positive T83 receipt.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$gate = Join-Path $PSScriptRoot 'windows_v2_t83_acceptance.ps1'
$powershell = Join-Path $PSHOME 'powershell.exe'
if ($env:OS -cne 'Windows_NT' -or $PSVersionTable.PSVersion.Major -ne 5) {
    throw 'KEL270-T83-TEST requires native Windows PowerShell 5.1'
}

function Assert-Refuses([string]$Case, [string[]]$Arguments, [string]$Expected) {
    $ErrorActionPreference = 'Continue'
    try {
        $output = (& $powershell '-NoProfile' '-NonInteractive' '-File' $gate @Arguments 2>&1 | Out-String)
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
    }
    if ($code -eq 0 -or $output -notmatch [regex]::Escape($Expected) -or
        $output -match 'KEL270_T83_SIGNED_INSTALLED_PASS|KELD_RELEASE_SIGNED_INSTALLED_PASS') {
        throw "KEL270-T83-TEST $Case did not refuse at its named boundary: exit=$code output=$output"
    }
    Write-Output "KEL270_T83_NEGATIVE_PASS case=$Case"
}

Assert-Refuses 'missing-application-signature' @() 'missing required ApplicationHost'

$git = Join-Path $env:ProgramFiles 'Git\cmd\git.exe'
if (-not (Test-Path -LiteralPath $git -PathType Leaf)) {
    throw 'KEL270-T83-TEST needs the existing signed Git for Windows binary'
}
$sig = Get-AuthenticodeSignature -LiteralPath $git
if ($sig.Status.ToString() -cne 'Valid' -or
    $sig.SignatureType.ToString() -cne 'Authenticode' -or
    $null -eq $sig.SignerCertificate) {
    throw 'KEL270-T83-TEST Git witness must have a valid embedded signature'
}
$sha = (Get-FileHash -LiteralPath $git -Algorithm SHA256).Hash
$thumbprint = $sig.SignerCertificate.Thumbprint
$currentSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value

$application = @('-ApplicationHost', $git, '-ApplicationSha256', $sha,
    '-ApplicationSignerThumbprint', $thumbprint)
$recovery = @('-RecoveryHost', $git, '-RecoverySha256', $sha,
    '-RecoverySignerThumbprint', $thumbprint)
$installer = @('-InstallerExe', $git, '-InstallerSha256', $sha,
    '-InstallerSignerThumbprint', $thumbprint)
$installed = @('-ExpectedOrdinarySid', $currentSid,
    '-InstalledRoot', (Split-Path -Parent $git),
    '-InstallerExecutionEvidence', $git)

Assert-Refuses 'missing-recovery-signature' $application 'missing required RecoveryHost'
Assert-Refuses 'missing-installer-image' ($application + $recovery) 'missing required InstallerExe'
Assert-Refuses 'missing-installer-execution-evidence' (
    $application + $recovery + $installer +
    @('-ExpectedOrdinarySid', $currentSid, '-InstalledRoot', (Split-Path -Parent $git))
) 'missing required InstallerExecutionEvidence'

$catalog = Join-Path $PSHOME 'powershell.exe'
$catalogSig = Get-AuthenticodeSignature -LiteralPath $catalog
if ($catalogSig.Status.ToString() -cne 'Valid' -or
    $catalogSig.SignatureType.ToString() -cne 'Catalog') {
    throw 'KEL270-T83-TEST expected a valid OS catalog-only image'
}
$catalogArgs = @(
    '-ApplicationHost', $catalog,
    '-ApplicationSha256', (Get-FileHash -LiteralPath $catalog -Algorithm SHA256).Hash,
    '-ApplicationSignerThumbprint', $catalogSig.SignerCertificate.Thumbprint
) + $recovery + $installer + $installed
Assert-Refuses 'catalog-only-is-not-embedded-app' $catalogArgs 'ApplicationHost lacks a valid embedded Authenticode signature'

$wrongSigner = @('-ApplicationHost', $git, '-ApplicationSha256', $sha,
    '-ApplicationSignerThumbprint', ('0' * 40)) + $recovery + $installer + $installed
Assert-Refuses 'wrong-app-signer' $wrongSigner 'ApplicationHost differs from independently supplied signer thumbprint'

# The expected SID is a public account identifier, not a username/password.
# A deliberately foreign SID must fail even when all three input images have
# valid embedded Windows signatures. No fake or altered signer is created.
$foreignSid = 'S-1-5-21-1-2-3-1001'
if ($currentSid -ceq $foreignSid) {
    $foreignSid = 'S-1-5-21-1-2-3-1002'
}
$foreignRunner = @('-ExpectedOrdinarySid', $foreignSid,
    '-InstalledRoot', (Split-Path -Parent $git),
    '-InstallerExecutionEvidence', $git)
Assert-Refuses 'foreign-ordinary-sid' (
    $application + $recovery + $installer + $foreignRunner
) 'actual Windows account SID differs from pinned ordinary SID'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
$groups = @($identity.Groups | ForEach-Object { $_.Value })
if (($currentSid -match '^S-1-5-21-(?:[0-9]+-){3}[0-9]+$') -and
    ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) -or
     $groups -contains 'S-1-5-32-544' -or $currentSid -match '-500$')) {
    Assert-Refuses 'current-admin-or-filtered-admin-refused' (
        $application + $recovery + $installer + $installed
    ) 'runner is an administrator or filtered administrator account'
} else {
    Write-Output 'KEL270_T83_NEGATIVE_NOT_RUN case=filtered-admin-token owner=distinct-admin-account'
    # If this is an actual ordinary medium-integrity token, all three real
    # embedded-signed input reads succeed and the reused-role gate must refuse.
    # An unadmitted token itself is also a valid refusal but not role coverage.
    $roleArgs = $application + $recovery + $installer + $installed
    $ErrorActionPreference = 'Continue'
    try {
        $roleOutput = (& $powershell '-NoProfile' '-NonInteractive' '-File' $gate @roleArgs 2>&1 | Out-String)
        $roleExit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = 'Stop'
    }
    if ($roleExit -eq 0 -or
        $roleOutput -match 'KEL270_T83_SIGNED_INSTALLED_PASS|KELD_RELEASE_SIGNED_INSTALLED_PASS') {
        throw 'KEL270-T83-TEST repeated APP/RECOVERY/installer images reached acceptance'
    }
    if ($roleOutput -match 'APP, RECOVERY and installer lack independent roles') {
        Write-Output 'KEL270_T83_NEGATIVE_PASS case=identical-signed-roles-rejected'
    } else {
        Write-Output 'KEL270_T83_NEGATIVE_NOT_RUN case=identical-signed-roles runner_token=not_qualified'
    }
}
$identity.Dispose()

Write-Output 'KEL270_T83_NEGATIVE_SUITE_PASS source_only=1 installed_positive=NOT_RUN signed_native_d45=NOT_RUN no_flag=NOT_RUN drawio=0/10 crash_recovery=NOT_RUN'
