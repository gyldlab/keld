# KEL-270: independent native Windows same-CA/foreign-leaf signing falsifier.
# NONAUTHORIZING: sign separate copies of one caller-pinned unsigned PE fixture.
# This may be a disposable NON-KELD fixture. Never launch the signed images.
# Never import a trust root or commission DEV Guard/Pack/D46 authority here.
# Output is public evidence only; owned temporary CurrentUser/My keys are deleted.
#
# Usage (explicit independently pinned, existing unsigned fixture):
# powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File
#   docs/engineering/evidence/kel270-catalogue-admission-v2/verify_dev_same_ca_third_leaf.ps1
#   -UnsignedFixture <absolute local unsigned PE path>
#   -ExpectedUnsignedSha256 <64 hexadecimal digits>
#   -SignToolExe <absolute independently pinned SDK signtool.exe path>
#   -ExpectedSignToolSha256 <64 hexadecimal digits>
#   -EvidenceDirectory <NEW absolute local directory>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$UnsignedFixture,
    [Parameter(Mandatory)][string]$ExpectedUnsignedSha256,
    [Parameter(Mandatory)][string]$SignToolExe,
    [Parameter(Mandatory)][string]$ExpectedSignToolSha256,
    [Parameter(Mandatory)][string]$EvidenceDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -cne 'Windows_NT') { throw 'native Windows only' }

function Require-LocalFile([string]$Value, [string]$What) {
    if ($Value -cnotmatch '^[A-Za-z]:\\' -or $Value.Contains('/') -or
        $Value -match '[\r\n\x22\x24\x21\x3b\x60\x5b\x5d]' -or
        $Value.Substring(2).Contains(':') -or
        [IO.Path]::GetFullPath($Value) -cne $Value -or
        $Value -match '(^|\\)\.\.($|\\)' -or
        -not (Test-Path -LiteralPath $Value -PathType Leaf)) {
        throw "$What must be one existing normalized absolute fixed-drive file path"
    }
    if ((New-Object IO.DriveInfo($Value.Substring(0,1))).DriveType -ne
        [IO.DriveType]::Fixed) { throw "$What must reside on a fixed drive" }
    $current = $Value
    while ($current) {
        $item = Get-Item -LiteralPath $current -Force -ErrorAction SilentlyContinue
        if ($item -and (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            throw "$What has a reparse-point ancestor"
        }
        $parent = [IO.Path]::GetDirectoryName($current)
        if (!$parent -or $parent -ceq $current) { break }
        $current = $parent
    }
    return $Value
}
function Require-NewLocalDirectory([string]$Value) {
    if ($Value -cnotmatch '^[A-Za-z]:\\' -or $Value.Contains('/') -or
        $Value -match '[\r\n\x22\x24\x21\x3b\x60\x5b\x5d]' -or
        $Value.Substring(2).Contains(':') -or
        [IO.Path]::GetFullPath($Value) -cne $Value -or
        $Value -match '(^|\\)\.\.($|\\)' -or
        (Test-Path -LiteralPath $Value)) {
        throw 'evidence directory must be a NEW normalized absolute local path'
    }
    $parent = [IO.Path]::GetDirectoryName($Value)
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
        throw 'evidence parent must already exist'
    }
    if ((New-Object IO.DriveInfo($Value.Substring(0,1))).DriveType -ne
        [IO.DriveType]::Fixed) { throw 'evidence parent must be on a fixed drive' }
    $current = $parent
    while ($current) {
        $item = Get-Item -LiteralPath $current -Force -ErrorAction SilentlyContinue
        if ($null -eq $item -or
            (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            throw 'evidence parent or an ancestor is missing or is a reparse point'
        }
        $ancestor = [IO.Path]::GetDirectoryName($current)
        if (!$ancestor -or $ancestor -ceq $current) { break }
        $current = $ancestor
    }
    return $Value
}
if ($ExpectedUnsignedSha256 -cnotmatch '^[0-9a-fA-F]{64}$' -or
    $ExpectedSignToolSha256 -cnotmatch '^[0-9a-fA-F]{64}$') {
    throw 'both independently reviewed SHA256 values are required'
}
$fixture = Require-LocalFile $UnsignedFixture 'unsigned PE fixture'
$signTool = Require-LocalFile $SignToolExe 'SDK SignTool'
$evidence = Require-NewLocalDirectory $EvidenceDirectory
$fixtureSha = $ExpectedUnsignedSha256.ToUpperInvariant()
$signToolSha = $ExpectedSignToolSha256.ToUpperInvariant()
if ((Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash -cne $fixtureSha -or
    (Get-FileHash -LiteralPath $signTool -Algorithm SHA256).Hash -cne $signToolSha) {
    throw 'independently pinned unsigned PE or SDK SignTool bytes differ'
}
if ((Microsoft.PowerShell.Security\Get-AuthenticodeSignature -LiteralPath $fixture).Status.ToString() -cne 'NotSigned') {
    throw 'the fixture already carries an Authenticode signature'
}

$label = 'KEL270 temporary third leaf ' + [Guid]::NewGuid().ToString('N')
$owned = @{}
$trustPaths = @('Cert:\CurrentUser\Root', 'Cert:\CurrentUser\TrustedPublisher',
                'Cert:\LocalMachine\Root', 'Cert:\LocalMachine\TrustedPublisher')
function Get-TrustInventory {
    $rows = @()
    foreach ($path in $trustPaths) {
        $rows += @((Get-ChildItem -LiteralPath $path |
            ForEach-Object { $path + '|' + $_.Thumbprint }) | Sort-Object)
    }
    return ($rows -join [char]10)
}
function CertificateSha256([System.Security.Cryptography.X509Certificates.X509Certificate2]$certificate) {
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString($hasher.ComputeHash($certificate.RawData)).Replace('-', '')
    } finally {
        $hasher.Dispose()
    }
}
function Check-ExactCaChain($certificate, $root) {
    $chain = New-Object System.Security.Cryptography.X509Certificates.X509Chain
    try {
        $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::NoCheck
        $chain.ChainPolicy.VerificationFlags = [System.Security.Cryptography.X509Certificates.X509VerificationFlags]::AllowUnknownCertificateAuthority
        $chain.ChainPolicy.ExtraStore.Add($root)
        $chain.ChainPolicy.ApplicationPolicy.Add(
            (New-Object System.Security.Cryptography.Oid('1.3.6.1.5.5.7.3.3')))
        if (-not $chain.Build($certificate) -or $chain.ChainElements.Count -ne 2) {
            throw 'same-CA test fixture did not build a two-element code-signing chain'
        }
        $actualRoot = $chain.ChainElements[1].Certificate
        if ((CertificateSha256 $actualRoot) -cne (CertificateSha256 $root)) {
            throw 'code-signing leaf does not chain to exact pinned root DER'
        }
    } finally {
        $chain.Dispose()
    }
}

# This is a deliberately NONAUTHORIZING test predicate. Expected pins come
# from APP and RECOVERY public certificates commissioned BEFORE signing any
# fixture. The observed leaf comes from the actual signed PE, not an input
# label, certificate-store query, or caller-selected role. This does NOT call
# Keld Guard or establish an installed role.
function Require-ObservedRolePin([string]$Operation, [string]$ActualSignerDerSha256,
                                 [hashtable]$ExpectedPins) {
    if (-not $ExpectedPins.ContainsKey($Operation)) {
        throw 'KEL270-TEST-UNKNOWN-OPERATION'
    }
    if ($ActualSignerDerSha256 -cne $ExpectedPins[$Operation]) {
        throw 'KEL270-TEST-ROLE-PIN-MISMATCH'
    }
    return $true
}
function Require-RoleRefusal([string]$Case, [string]$Operation,
                             [string]$ActualSignerDerSha256, [hashtable]$ExpectedPins) {
    $refused = $false
    try {
        $null = Require-ObservedRolePin $Operation $ActualSignerDerSha256 $ExpectedPins
    } catch {
        if ($_.Exception.Message -cne 'KEL270-TEST-ROLE-PIN-MISMATCH') { throw }
        $refused = $true
    }
    if (-not $refused) {
        throw "KEL270-NATIVE-NEGATIVE FAILED: $Case admitted the foreign PE signer"
    }
    Write-Output ("PASS_NEGATIVE_TEST_PIN " + $Case)
}

$trustBefore = Get-TrustInventory
$output = @()
$observed = @{}
$record = $null
$cleanupComplete = $false
try {
    # New-Item without -Force refuses an existing leaf. This is a disposable
    # test directory, not a durable installer destination or owner witness.
    $parent = [IO.Path]::GetDirectoryName($evidence)
    $leafDirectoryName = [IO.Path]::GetFileName($evidence)
    New-Item -Path $parent -Name $leafDirectoryName `
        -ItemType Directory -ErrorAction Stop | Out-Null
    if (((Get-Item -LiteralPath $evidence -Force).Attributes -band
            [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'new evidence directory unexpectedly became a reparse point'
    }
    $root = New-SelfSignedCertificate -Type Custom -Subject ('CN=' + $label + ' CA') `
        -CertStoreLocation 'Cert:\CurrentUser\My' -KeyAlgorithm RSA -KeyLength 3072 `
        -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage CertSign,CRLSign `
        -TextExtension @('2.5.29.19={critical}{text}ca=1&pathlength=0') `
        -NotAfter (Get-Date).AddDays(3)
    $owned['ROOT'] = $root
    foreach ($role in @('APP','RECOVERY','THIRD')) {
        $owned[$role] = New-SelfSignedCertificate -Type CodeSigningCert `
            -Subject ('CN=' + $label + ' ' + $role) -Signer $root `
            -CertStoreLocation 'Cert:\CurrentUser\My' -KeyAlgorithm RSA -KeyLength 3072 `
            -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage DigitalSignature `
            -NotAfter (Get-Date).AddDays(2)
    }
    foreach ($role in @('ROOT','APP','RECOVERY','THIRD')) {
        $certPath = Join-Path $evidence ($role + '.cer')
        Export-Certificate -Cert $owned[$role] -FilePath $certPath -Type CERT | Out-Null
        if ((Get-FileHash -LiteralPath $certPath -Algorithm SHA256).Hash -cne
            (CertificateSha256 $owned[$role])) {
            throw 'exported public certificate bytes differ from exact owned DER'
        }
    }
    $expectedRolePins = @{
        APP = (Get-FileHash -LiteralPath (Join-Path $evidence 'APP.cer') -Algorithm SHA256).Hash
        RECOVERY = (Get-FileHash -LiteralPath (Join-Path $evidence 'RECOVERY.cer') -Algorithm SHA256).Hash
    }
    $caHash = CertificateSha256 $root
    foreach ($role in @('APP','RECOVERY','THIRD')) {
        $leaf = $owned[$role]
        Check-ExactCaChain $leaf $root
        $signed = Join-Path $evidence ($role.ToLowerInvariant() + '-SIGNED-TEST-ONLY.exe')
        if ((Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash -cne $fixtureSha -or
            (Get-FileHash -LiteralPath $signTool -Algorithm SHA256).Hash -cne $signToolSha) {
            throw 'original input or actual SDK signer changed before signing'
        }
        [IO.File]::Copy($fixture,$signed)
        if ((Get-FileHash -LiteralPath $signed -Algorithm SHA256).Hash -cne $fixtureSha) {
            throw 'copied unsigned input changed before signing'
        }
        & $signTool sign /fd SHA256 /s My /sha1 $leaf.Thumbprint `
            /d 'KEL270 NON-PRODUCT same-CA third-leaf test' $signed | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'actual native SignTool signing failed' }
        $sig = Microsoft.PowerShell.Security\Get-AuthenticodeSignature -LiteralPath $signed
        if ($null -eq $sig.SignerCertificate -or
            $sig.SignerCertificate.Thumbprint -cne $leaf.Thumbprint -or
            $sig.Status.ToString() -cne 'UnknownError') {
            throw 'actual PE signer mismatch, hash failure or unexpected trust status'
        }
        $observed[$role] = CertificateSha256 $sig.SignerCertificate
        # A signed image with this intentionally UNTRUSTED CA must fail
        # /pa verification. Do not turn expected native stderr into a
        # terminating PowerShell error while $ErrorActionPreference=Stop.
        $verifyStdout = Join-Path $evidence ($role + '-verify-stdout.txt')
        $verifyStderr = Join-Path $evidence ($role + '-verify-stderr.txt')
        $verify = Start-Process -FilePath $signTool -ArgumentList @(
            'verify', '/pa', '/all', ('"' + $signed + '"')
        ) -Wait -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput $verifyStdout -RedirectStandardError $verifyStderr
        $verifyError = Get-Content -LiteralPath $verifyStderr -Raw
        if ($verify.ExitCode -eq 0 -or
            $verifyError -notmatch '(?is)certificate chain processed.*terminated in a root.*not trusted by the trust provider') {
            throw 'native Public Trust failure was not specifically an untrusted CA root'
        }
        if ((Get-FileHash -LiteralPath $signTool -Algorithm SHA256).Hash -cne $signToolSha) {
            throw 'reviewed SDK signer bytes changed during the native invocation'
        }
        $leafHash = CertificateSha256 $leaf
        $output += [ordered]@{
            role = $role
            signer_leaf_sha256 = $observed[$role]
            ca_root_sha256 = $caHash
            signed_image_sha256 = (Get-FileHash -LiteralPath $signed -Algorithm SHA256).Hash
            sign_tool_exit = 0
            shared_os_trust_status = $sig.Status.ToString()
            public_trust_verification = 'REFUSED_UNTRUSTED_ROOT'
        }
        Write-Output ('PASS_NATIVE_SIGNED_PE_UNTRUSTED_CA role=' + $role +
            ' root=' + $caHash + ' leaf=' + $leafHash)
    }
    $app = $observed['APP']
    $recovery = $observed['RECOVERY']
    $third = $observed['THIRD']
    if ($app -ceq $recovery -or $app -ceq $third -or $recovery -ceq $third) {
        throw 'three independently commissioned leaf certificates are not distinct'
    }
    if (-not (Require-ObservedRolePin 'APP' $app $expectedRolePins) -or
        -not (Require-ObservedRolePin 'RECOVERY' $recovery $expectedRolePins)) {
        throw 'an independently pinned test role refused its actual embedded signer'
    }
    Require-RoleRefusal 'app-leaf-as-recovery' 'RECOVERY' $app $expectedRolePins
    Require-RoleRefusal 'recovery-leaf-as-app' 'APP' $recovery $expectedRolePins
    Require-RoleRefusal 'third-leaf-as-app' 'APP' $third $expectedRolePins
    Require-RoleRefusal 'third-leaf-as-recovery' 'RECOVERY' $third $expectedRolePins
    Write-Output 'PASS_ACTUAL_EMBEDDED_THIRD_LEAF_REJECTED_BY_ROLE_SPECIFIC_TEST_PIN'
    $record = [ordered]@{
        schema = 'kel270-native-same-ca-third-leaf-NONAUTHORIZING/v1'
        fixture_sha256 = $fixtureSha
        signtool_sha256 = $signToolSha
        source_file = $fixture
        signer_file = $signTool
        root_sha256 = $caHash
        signed = $output
        third_leaf_matches_app_or_recovery_pin = $false
        source_kind = 'CALLER_PINNED_UNSIGNED_PE_TEST_FIXTURE'
        exact_test_only_expected_app_leaf_sha256 = $expectedRolePins['APP']
        exact_test_only_expected_recovery_leaf_sha256 = $expectedRolePins['RECOVERY']
        test_role_swap_negatives = 2
        test_foreign_third_leaf_negatives = 2
        native_guard_dev_role_check = 'NOT_RUN'
        original_keld_host = 'NOT_BUILT'
        installed_product = 'NOT_RUN'
    }
} finally {
    $cleanupErrors = New-Object 'System.Collections.Generic.List[string]'
    foreach ($role in @('THIRD','RECOVERY','APP','ROOT')) {
        if ($owned.ContainsKey($role)) {
            try {
                $certificate = $owned[$role]
                $storePath = 'Cert:\CurrentUser\My\' + $certificate.Thumbprint
                $found = Get-Item -LiteralPath $storePath -ErrorAction SilentlyContinue
                if ($null -eq $found) {
                    throw 'owned certificate is unexpectedly absent before private-key retirement'
                }
                if ((CertificateSha256 $found) -cne (CertificateSha256 $certificate)) {
                    throw 'certificate identity changed: refusing foreign-key deletion'
                }
                Remove-Item -LiteralPath $storePath -DeleteKey -Force -ErrorAction Stop
                if (Test-Path -LiteralPath $storePath) {
                    throw 'an owned temporary private key/certificate remains'
                }
            } catch {
                $cleanupErrors.Add($role + ': ' + $_.Exception.Message)
            }
        }
    }
    try {
        if ((Get-TrustInventory) -cne $trustBefore) {
            throw 'CurrentUser/LocalMachine trust store inventory changed'
        }
    } catch {
        $cleanupErrors.Add('trust inventory: ' + $_.Exception.Message)
    }
    if ($cleanupErrors.Count -ne 0) {
        throw ('owned key cleanup incomplete: ' + ($cleanupErrors -join '; '))
    }
    $cleanupComplete = $true
    Write-Output 'PASS_OWNED_NONEXPORTABLE_CERTS_DELETED_TRUST_STORES_UNCHANGED'
}
if (-not $cleanupComplete -or $null -eq $record) {
    throw 'test output cannot become a success receipt without completed key cleanup'
}
$record['owned_private_keys_retired'] = $true
$record['trust_store_inventory_unchanged'] = $true
($record | ConvertTo-Json -Depth 8) |
    Set-Content -LiteralPath (Join-Path $evidence 'receipt.json') -Encoding UTF8
Write-Output 'PASS_PUBLIC_EVIDENCE_PUBLISHED_ONLY_AFTER_COMPLETE_OWNED_KEY_CLEANUP'
