# Explicit native operator test, not an installed product or trust qualification.
# Uses an independently pinned unsigned debug fixture. No trust-store imports.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$UnsignedFixture,
    [Parameter(Mandatory)][string]$FixtureSha256,
    [Parameter(Mandatory)][string]$EvidenceDirectory
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$scriptPath = Join-Path $PSScriptRoot 'windows_v2_dev_sign.ps1'
$pin = $FixtureSha256.ToUpperInvariant()
if ($pin -cnotmatch '^[0-9A-F]{64}$' -or
    (Get-FileHash -LiteralPath $UnsignedFixture -Algorithm SHA256).Hash -cne $pin -or
    (Test-Path -LiteralPath $EvidenceDirectory)) { throw 'invalid fixture pin or existing evidence directory' }
New-Item -ItemType Directory -Path $EvidenceDirectory -ErrorAction Stop | Out-Null
$label = 'Keld DEV helper test ' + [Guid]::NewGuid().ToString('N')
$owned = @{}
$trustPaths = @('Cert:\CurrentUser\Root', 'Cert:\CurrentUser\TrustedPublisher',
    'Cert:\LocalMachine\Root', 'Cert:\LocalMachine\TrustedPublisher')
function Get-TrustInventory {
    $rows = @()
    foreach ($p in $trustPaths) {
        $rows += @((Get-ChildItem -LiteralPath $p | ForEach-Object { $p + '|' + $_.Thumbprint }) | Sort-Object)
    }
    return ($rows -join [char]10)
}
$trustBefore = Get-TrustInventory
function Require-Refusal($Arguments, [string]$Expected, [string]$Name) {
    $refused = $false
    try { & $scriptPath @Arguments | Out-Null }
    catch {
        if (!$_.Exception.Message.Contains($Expected)) { throw }
        $refused = $true
    }
    if (!$refused) { throw "expected refusal: $Name" }
    if ($Name -ne 'existing-output' -and (Test-Path -LiteralPath $Arguments.OutputDirectory)) {
        throw "unexpected output: $Name"
    }
    Write-Output "PASS_NEGATIVE $Name"
}

try {
    $ca = New-SelfSignedCertificate -Type Custom -Subject ('CN=' + $label + ' ROOT') `
        -CertStoreLocation 'Cert:\CurrentUser\My' -KeyAlgorithm RSA -KeyLength 3072 `
        -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage CertSign,CRLSign `
        -TextExtension @('2.5.29.19={critical}{text}ca=1&pathlength=0') -NotAfter (Get-Date).AddDays(3)
    $owned['ROOT'] = $ca
    foreach ($role in @('APP', 'RECOVERY')) {
        $owned[$role] = New-SelfSignedCertificate -Type CodeSigningCert -Subject ('CN=' + $label + ' ' + $role) `
            -Signer $ca -CertStoreLocation 'Cert:\CurrentUser\My' -KeyAlgorithm RSA -KeyLength 3072 `
            -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage DigitalSignature -NotAfter (Get-Date).AddDays(2)
    }
    $paths = @{}; $pins = @{}
    foreach ($role in @('ROOT', 'APP', 'RECOVERY')) {
        $paths[$role] = Join-Path $EvidenceDirectory ($role + '.cer')
        Export-Certificate -Cert $owned[$role] -FilePath $paths[$role] -Type CERT | Out-Null
        $pins[$role] = (Get-FileHash -Algorithm SHA256 -LiteralPath $paths[$role]).Hash
    }
    $common = @{
        UnsignedImage = $UnsignedFixture; ExpectedUnsignedSha256 = $pin; Role = 'APP'
        AppId = 'com.keld.dev.probe'; ApplicationCertificate = $paths['APP']
        ExpectedApplicationCertificateSha256 = $pins['APP']; RecoveryCertificate = $paths['RECOVERY']
        ExpectedRecoveryCertificateSha256 = $pins['RECOVERY']; RootCertificate = $paths['ROOT']
        ExpectedRootCertificateSha256 = $pins['ROOT']; OutputDirectory = (Join-Path $EvidenceDirectory 'app-output')
    }
    foreach ($role in @('APP', 'RECOVERY')) {
        $a = $common.Clone(); $a.Role = $role
        $a.OutputDirectory = Join-Path $EvidenceDirectory ($role.ToLowerInvariant() + '-output')
        & $scriptPath @a
        $receipt = Get-Content -Raw -LiteralPath (Join-Path $a.OutputDirectory 'dev-signing-observation.json') | ConvertFrom-Json
        if ($receipt.scope -cne 'DEVELOPMENT_ONLY_NONAUTHORIZING' -or $receipt.role -cne $role -or
            $receipt.leafCertificateSha256 -cne $pins[$role]) { throw 'receipt role or pin mismatch' }
        $signed = Join-Path $a.OutputDirectory $receipt.file
        $sig = Microsoft.PowerShell.Security\Get-AuthenticodeSignature -LiteralPath $signed
        if ($sig.SignerCertificate.Thumbprint -cne $owned[$role].Thumbprint -or $sig.Status -eq 'Valid' -or
            (Get-FileHash -Algorithm SHA256 -LiteralPath $signed).Hash -cne $receipt.signedSha256) {
            throw 'wrong signer, digest, or unexpected shared trust'
        }
        Write-Output "PASS_POSITIVE $role"
    }
    # Bind the signer to the retained signed image rather than a separately
    # resolved pathname. Read only the exact verifier functions from this
    # script; this test never invokes the signing entrypoint or grants trust.
    $tokens = $null; $parseErrors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile(
        $scriptPath, [ref]$tokens, [ref]$parseErrors)
    if (@($parseErrors).Count -ne 0) { throw 'DEV signer source cannot be parsed' }
    foreach ($name in @('Refuse', 'Get-Sha256', 'Read-ExactRange',
            'Get-SigningLayout', 'Require-RetainedDevSigner')) {
        $definitions = @($ast.FindAll({
            param($node)
            $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
                $node.Name -ceq $name
        }, $true))
        if ($definitions.Count -ne 1) { throw "missing or duplicate retained signer helper $name" }
        . ([scriptblock]::Create($definitions[0].Extent.Text))
    }
    $recoverySigned = Join-Path $EvidenceDirectory 'recovery-output/dev-recovery-'
    $recoverySigned += [IO.Path]::GetFileName($UnsignedFixture)
    $retained = [IO.File]::Open($recoverySigned,
        [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        Require-RetainedDevSigner $retained $owned['RECOVERY']
        $foreignRefused = $false
        try { Require-RetainedDevSigner $retained $owned['APP'] }
        catch {
            if (!$_.Exception.Message.Contains('retained signed image has a foreign DEV role signer')) { throw }
            $foreignRefused = $true
        }
        if (!$foreignRefused) { throw 'foreign embedded role was accepted from retained hFile' }
    } finally { $retained.Dispose() }
    Write-Output 'PASS_RETAINED_HFILE_FOREIGN_SIGNER_REFUSED'
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'bad-source'
    $a.ExpectedUnsignedSha256 = '0' * 64; Require-Refusal $a 'unsigned image differs' 'source-pin-mismatch'
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'bad-cert'
    $a.ExpectedRecoveryCertificateSha256 = '0' * 64; Require-Refusal $a 'certificate pin mismatch' 'cert-pin-mismatch'
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'same-role'
    $a.RecoveryCertificate = $paths['APP']; $a.ExpectedRecoveryCertificateSha256 = $pins['APP']
    Require-Refusal $a 'APP and RECOVERY keys must differ' 'same-role-key'
    # Same issuer subject, different CA key: name equality alone would accept
    # this substitution. The independent oracle is refusal before any output.
    $owned['WRONG_ROOT'] = New-SelfSignedCertificate -Type Custom -Subject $ca.Subject `
        -CertStoreLocation 'Cert:\CurrentUser\My' -KeyAlgorithm RSA -KeyLength 3072 `
        -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -KeyUsage CertSign,CRLSign `
        -TextExtension @('2.5.29.19={critical}{text}ca=1&pathlength=0') -NotAfter (Get-Date).AddDays(3)
    $wrongRoot = $owned['WRONG_ROOT']
    if ($wrongRoot.Subject -cne $ca.Subject -or $wrongRoot.GetPublicKeyString() -ceq $ca.GetPublicKeyString()) {
        throw 'wrong-root test requires same subject and distinct CA keys'
    }
    $wrongRootPath = Join-Path $EvidenceDirectory 'WRONG_ROOT.cer'
    Export-Certificate -Cert $wrongRoot -FilePath $wrongRootPath -Type CERT | Out-Null
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'wrong-root'
    $a.RootCertificate = $wrongRootPath
    $a.ExpectedRootCertificateSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $wrongRootPath).Hash
    Require-Refusal $a 'leaf chain does not match the exact pinned local CA' 'same-subject-wrong-root'
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'production-app'
    $a.AppId = 'com.keld.production'; Require-Refusal $a 'development-only app ID' 'production-app-id'
    $a = $common.Clone(); Require-Refusal $a 'absent output directory' 'existing-output'
    # Inject a deterministic failure after the real signed-copy observation but
    # before the receipt is written. The test-only script copy is never shipped.
    $retry = $common.Clone()
    $retry.OutputDirectory = Join-Path $EvidenceDirectory 'retry-output'
    $injected = Join-Path $EvidenceDirectory 'fail-before-receipt.ps1'
    $scriptText = [IO.File]::ReadAllText($scriptPath)
    $anchor = '    $receiptPath = Join-Path '
    if ([regex]::Matches($scriptText, [regex]::Escape($anchor)).Count -ne 1) {
        throw 'receipt failure injection anchor changed'
    }
    $scriptText = $scriptText.Replace($anchor, "    throw 'CONTROLLED_RECEIPT_FAILURE'`r`n" + $anchor)
    [IO.File]::WriteAllText($injected, $scriptText)
    $refused = $false
    try { & $injected @retry | Out-Null }
    catch {
        if (!$_.Exception.Message.Contains('CONTROLLED_RECEIPT_FAILURE')) { throw }
        $refused = $true
    }
    if (!$refused -or (Test-Path -LiteralPath $retry.OutputDirectory)) {
        throw 'failed signing attempt published partial final output'
    }
    & $scriptPath @retry
    $retryReceipt = Get-Content -Raw -LiteralPath (Join-Path $retry.OutputDirectory 'dev-signing-observation.json') | ConvertFrom-Json
    if ((Get-FileHash -LiteralPath (Join-Path $retry.OutputDirectory $retryReceipt.file) -Algorithm SHA256).Hash -cne $retryReceipt.signedSha256) {
        throw 'retry did not publish the complete signed observation'
    }
    Write-Output 'PASS_FAILURE_THEN_RETRY_SAME_DESTINATION'
    # Change an ordinary covered byte after the initial copy hash, before
    # real SignTool invocation. Never execute either disposable image.
    $substitution = $common.Clone()
    $substitution.OutputDirectory = Join-Path $EvidenceDirectory 'substituted-output'
    $injected = Join-Path $EvidenceDirectory 'substitute-before-sign.ps1'
    $scriptText = [IO.File]::ReadAllText($scriptPath)
    $anchor = '    & $signTool sign '
    if ([regex]::Matches($scriptText, [regex]::Escape($anchor)).Count -ne 1) {
        throw 'signing substitution injection anchor changed'
    }
    $mutation = @'
    $changed = [IO.File]::ReadAllBytes($destination)
    $nt = [BitConverter]::ToUInt32($changed, 60)
    $table = $nt + 24 + [BitConverter]::ToUInt16($changed, $nt + 20)
    $raw = [BitConverter]::ToUInt32($changed, $table + 20)
    $changed[$raw + 1] = $changed[$raw + 1] -bxor 1
    [IO.File]::WriteAllBytes($destination, $changed)
'@
    $scriptText = $scriptText.Replace($anchor, $mutation + "`r`n" + $anchor)
    [IO.File]::WriteAllText($injected, $scriptText)
    $refused = $false
    try { & $injected @substitution | Out-Null }
    catch {
        if (!$_.Exception.Message.Contains('signed bytes differ from reviewed source')) { throw }
        $refused = $true
    }
    if (!$refused -or (Test-Path -LiteralPath $substitution.OutputDirectory)) {
        throw 'substituted signed image was published'
    }
    Write-Output 'PASS_SIGNED_SOURCE_SUBSTITUTION_REFUSED'
    # The publication contains exactly the three intended files. A disposable
    # unexpected member must prevent a success receipt; retain uncertain output.
    foreach ($phase in @('before', 'after')) {
        $inventoryArgs = $common.Clone()
        $inventoryArgs.OutputDirectory = Join-Path $EvidenceDirectory ('inventory-' + $phase)
        $injected = Join-Path $EvidenceDirectory ('inventory-' + $phase + '.ps1')
        $scriptText = [IO.File]::ReadAllText($scriptPath)
        if ($phase -eq 'before') {
            $anchor = '    if (Test-Path -LiteralPath $output) { Refuse ''destination appeared before publication'' }'
            $mutation = '    [IO.File]::WriteAllText((Join-Path $temporary ''unexpected-test-member.txt''), ''fixture-only'')'
        } else {
            $anchor = '    $temporary = $null' + [Environment]::NewLine + '    # Publication is not a native custody proof.'
            $scriptText = $scriptText.Replace("`r`n", "`n")
            $anchor = $anchor.Replace("`r`n", "`n")
            $mutation = '    [IO.File]::WriteAllText((Join-Path $output ''unexpected-test-member.txt''), ''fixture-only'')'
        }
        if ([regex]::Matches($scriptText, [regex]::Escape($anchor)).Count -ne 1) {
            throw "inventory test anchor changed: $phase"
        }
        $scriptText = $scriptText.Replace($anchor, $mutation + "`n" + $anchor)
        [IO.File]::WriteAllText($injected, $scriptText)
        $refused = $false
        try { & $injected @inventoryArgs | Out-Null }
        catch {
            if (!$_.Exception.Message.Contains('unexpected DEV signing inventory count')) { throw }
            $refused = $true
        }
        if (!$refused) { throw "unexpected DEV inventory accepted: $phase" }
        if ($phase -eq 'before' -and (Test-Path -LiteralPath $inventoryArgs.OutputDirectory)) {
            throw 'unexpected DEV inventory reached final output'
        }
        if ($phase -eq 'after' -and !(Test-Path -LiteralPath (Join-Path $inventoryArgs.OutputDirectory 'unexpected-test-member.txt') -PathType Leaf)) {
            throw 'uncertain DEV output was not retained'
        }
        Write-Output "PASS_UNEXPECTED_DEV_INVENTORY_REFUSED $phase"
    }
    $signedReceipt = Get-Content -Raw -LiteralPath (Join-Path $common.OutputDirectory 'dev-signing-observation.json') | ConvertFrom-Json
    $a = $common.Clone(); $a.OutputDirectory = Join-Path $EvidenceDirectory 'resign'
    $a.UnsignedImage = Join-Path $common.OutputDirectory $signedReceipt.file; $a.ExpectedUnsignedSha256 = $signedReceipt.signedSha256
    Require-Refusal $a 'source image already carries a signature' 'already-signed-input'
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $UnsignedFixture).Hash -cne $pin) { throw 'source fixture changed' }
    Write-Output 'POSITIVE_2_NEGATIVE_7_SOURCE_PRESERVED'
} finally {
    foreach ($role in @('RECOVERY', 'APP', 'WRONG_ROOT', 'ROOT')) {
        if ($owned.ContainsKey($role)) {
            $cert = $owned[$role]; $p = 'Cert:\CurrentUser\My\' + $cert.Thumbprint
            $actual = Get-Item -LiteralPath $p -ErrorAction SilentlyContinue
            if ($actual -and $actual.Subject -ceq $cert.Subject -and
                [Convert]::ToBase64String($actual.RawData) -ceq [Convert]::ToBase64String($cert.RawData)) {
                Remove-Item -LiteralPath $p -DeleteKey -Force -ErrorAction Stop
                if (Test-Path -LiteralPath $p) { throw 'owned certificate remains' }
            } elseif ($actual) { throw 'unexpected certificate; preserve it' }
        }
    }
    if ((Get-TrustInventory) -ne $trustBefore) { throw 'trust inventory changed' }
    Write-Output 'TEMPORARY_KEYS_REMOVED_TRUST_STORES_UNCHANGED'
}
