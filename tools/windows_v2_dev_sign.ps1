# Local development signing only. Does not provision certificates or trust,
# authorize a Keld installation, execute an image, or qualify public release.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$UnsignedImage,
    [Parameter(Mandatory)][string]$ExpectedUnsignedSha256,
    [Parameter(Mandatory)][ValidateSet('APP', 'RECOVERY')][string]$Role,
    [Parameter(Mandatory)][string]$AppId,
    [Parameter(Mandatory)][string]$ApplicationCertificate,
    [Parameter(Mandatory)][string]$ExpectedApplicationCertificateSha256,
    [Parameter(Mandatory)][string]$RecoveryCertificate,
    [Parameter(Mandatory)][string]$ExpectedRecoveryCertificateSha256,
    [Parameter(Mandatory)][string]$RootCertificate,
    [Parameter(Mandatory)][string]$ExpectedRootCertificateSha256,
    [Parameter(Mandatory)][string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Refuse([string]$Reason) {
    throw "KEL270-DEV-SIGN: $Reason; no installation or production authority granted"
}

function Require-Hash([string]$Value) {
    if ($Value -cnotmatch '^[0-9A-Fa-f]{64}$') { Refuse 'expected independent SHA256 pin' }
    return $Value.ToUpperInvariant()
}

function Require-LocalPath([string]$Path) {
    if ($Path -cnotmatch '^[A-Za-z]:\\' -or $Path -match '[\r\n*?\[\]"]' -or $Path.Contains('/') -or
        $Path.Substring(2).Contains(':') -or [IO.Path]::GetFullPath($Path) -cne $Path) {
        Refuse 'expected an absolute normalized local path'
    }
    $drive = New-Object IO.DriveInfo($Path.Substring(0, 1))
    if ($drive.DriveType -ne [IO.DriveType]::Fixed) { Refuse 'expected a fixed local volume' }
    foreach ($part in $Path.Substring(3).Split('\\')) {
        if (!$part -or $part.EndsWith('.') -or $part.EndsWith(' ') -or
            $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') {
            Refuse 'ambiguous path component'
        }
    }
    $part = $Path
    while ($part) {
        $item = Get-Item -LiteralPath $part -Force -ErrorAction SilentlyContinue
        if ($null -ne $item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            Refuse 'reparse-point path component'
        }
        $parent = [IO.Path]::GetDirectoryName($part)
        if (!$parent -or $parent -ceq $part) { break }
        $part = $parent
    }
    return $Path
}

function Get-Sha256([byte[]]$Bytes) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($sha.ComputeHash($Bytes)).Replace('-', '') }
    finally { $sha.Dispose() }
}

function Get-FileSha256([string]$Path) {
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '') }
    finally { $sha.Dispose(); $stream.Dispose() }
}

function Get-ObservedAuthenticode([string]$Path) {
    $manifest = [IO.Path]::Combine($PSHOME, 'Modules',
        'Microsoft.PowerShell.Security', 'Microsoft.PowerShell.Security.psd1')
    if (-not [IO.File]::Exists($manifest)) { Refuse 'OS Security module unavailable' }
    $modules = @(Microsoft.PowerShell.Core\Import-Module -Name $manifest -PassThru -ErrorAction Stop)
    if ($modules.Count -ne 1 -or $modules[0].Path -ine $manifest) {
        Refuse 'OS Security module provenance mismatch'
    }
    $cmdlet = $modules[0].ExportedCommands['Get-AuthenticodeSignature']
    if ($cmdlet -isnot [System.Management.Automation.CmdletInfo] -or
        $cmdlet.ImplementingType.FullName -cne 'Microsoft.PowerShell.Commands.GetAuthenticodeSignatureCommand') {
        Refuse 'OS Authenticode cmdlet provenance mismatch'
    }
    return (& $cmdlet -LiteralPath $Path)
}

function Read-PublicCertificate([string]$Path, [string]$Expected) {
    $path = Require-LocalPath $Path
    $stream = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -lt 1 -or $stream.Length -gt 65536) { Refuse 'oversized or empty certificate' }
        $reader = New-Object IO.BinaryReader($stream)
        $bytes = $reader.ReadBytes([int]$stream.Length)
        if ($bytes.Length -ne $stream.Length -or (Get-Sha256 $bytes) -cne (Require-Hash $Expected)) {
            Refuse 'certificate pin mismatch'
        }
    } finally { $stream.Dispose() }
    if ([Security.Cryptography.X509Certificates.X509Certificate2]::GetCertContentType($bytes) -ne
        [Security.Cryptography.X509Certificates.X509ContentType]::Cert) {
        Refuse 'public DER certificate required, not PFX or key material'
    }
    $cert = New-Object Security.Cryptography.X509Certificates.X509Certificate2 -ArgumentList (,$bytes)
    if ($cert.HasPrivateKey -or (Get-Sha256 $cert.RawData) -cne (Require-Hash $Expected)) {
        Refuse 'expected exact public certificate DER bytes'
    }
    return $cert
}

function Require-DevLeaf($Certificate, $Root) {
    $eku = @($Certificate.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' })
    if ($eku.Count -ne 1) { Refuse 'expected one explicit enhanced key usage extension' }
    $oids = @($eku[0].EnhancedKeyUsages | ForEach-Object { $_.Value })
    if ($oids -cnotcontains '1.3.6.1.5.5.7.3.3' -or
        $oids -ccontains '1.3.6.1.4.1.311.97.1.0' -or
        $Certificate.Subject -ceq $Certificate.Issuer -or $Certificate.Issuer -cne $Root.Subject) {
        Refuse 'expected a local CA-issued code-signing leaf, not a Public Trust identity'
    }
    $now = Get-Date
    if ($now -lt $Certificate.NotBefore -or $now -ge $Certificate.NotAfter -or
        $now -lt $Root.NotBefore -or $now -ge $Root.NotAfter) { Refuse 'certificate outside validity period' }
    # A matching issuer name is not a cryptographic chain. This preflight may
    # observe an untrusted development root but grants no Windows trust or
    # installation authority. The product must verify the actual retained image.
    $chain = New-Object Security.Cryptography.X509Certificates.X509Chain
    try {
        $chain.ChainPolicy.VerificationTime = $now
        $chain.ChainPolicy.RevocationMode = [Security.Cryptography.X509Certificates.X509RevocationMode]::NoCheck
        $chain.ChainPolicy.VerificationFlags = [Security.Cryptography.X509Certificates.X509VerificationFlags]::AllowUnknownCertificateAuthority
        [void]$chain.ChainPolicy.ExtraStore.Add($Root)
        [void]$chain.ChainPolicy.ApplicationPolicy.Add((New-Object Security.Cryptography.Oid('1.3.6.1.5.5.7.3.3')))
        if (!$chain.Build($Certificate) -or $chain.ChainElements.Count -ne 2 -or
            (Get-Sha256 $chain.ChainElements[0].Certificate.RawData) -cne (Get-Sha256 $Certificate.RawData) -or
            (Get-Sha256 $chain.ChainElements[1].Certificate.RawData) -cne (Get-Sha256 $Root.RawData) -or
            @($chain.ChainStatus | Where-Object { $_.Status -notin @(
                [Security.Cryptography.X509Certificates.X509ChainStatusFlags]::NoError,
                [Security.Cryptography.X509Certificates.X509ChainStatusFlags]::UntrustedRoot) }).Count -ne 0) {
            Refuse 'leaf chain does not match the exact pinned local CA'
        }
    } finally { $chain.Dispose() }
}

if ($AppId -cnotmatch '^com\.keld\.dev\.[a-z0-9][a-z0-9.-]{0,239}$' -or
    $AppId.Contains('..') -or $AppId.EndsWith('.')) { Refuse 'expected an explicitly development-only app ID' }
$inputPath = Require-LocalPath $UnsignedImage
$output = Require-LocalPath $OutputDirectory
$sourcePin = Require-Hash $ExpectedUnsignedSha256
if ([IO.Path]::GetExtension($inputPath) -ine '.exe' -or (Test-Path -LiteralPath $output)) {
    Refuse 'expected an unsigned EXE and an absent output directory'
}
$parent = [IO.Path]::GetDirectoryName($output)
if (!(Test-Path -LiteralPath $parent -PathType Container)) { Refuse 'output parent must exist' }
$root = Read-PublicCertificate $RootCertificate $ExpectedRootCertificateSha256
$app = Read-PublicCertificate $ApplicationCertificate $ExpectedApplicationCertificateSha256
$recovery = Read-PublicCertificate $RecoveryCertificate $ExpectedRecoveryCertificateSha256
$rootConstraints = @($root.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.19' })
if ($root.Subject -cne $root.Issuer -or $rootConstraints.Count -ne 1 -or
    !$rootConstraints[0].CertificateAuthority) { Refuse 'expected a local CA root certificate' }
Require-DevLeaf $app $root
Require-DevLeaf $recovery $root
if ($app.Thumbprint -ceq $recovery.Thumbprint -or
    $app.GetPublicKeyString() -ceq $recovery.GetPublicKeyString()) { Refuse 'APP and RECOVERY keys must differ' }
$leaf = if ($Role -eq 'APP') { $app } else { $recovery }
$owned = Get-Item -LiteralPath ('Cert:\CurrentUser\My\' + $leaf.Thumbprint) -ErrorAction SilentlyContinue
if ($null -eq $owned -or !$owned.HasPrivateKey -or
    (Get-Sha256 $owned.RawData) -cne (Get-Sha256 $leaf.RawData)) { Refuse 'exact role signing key unavailable in CurrentUser My' }

$signTool = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe'
$signToolSha = '431EE314C83988CACDA86606356FD321B75AE0093481B97E3B738E99C412F2A0'
if ((Get-FileSha256 $signTool) -cne $signToolSha) {
    Refuse 'reviewed SDK SignTool unavailable or changed'
}

$source = [IO.File]::Open($inputPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
$temporary = $null
try {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $actual = [BitConverter]::ToString($sha.ComputeHash($source)).Replace('-', '') }
    finally { $sha.Dispose() }
    if ($actual -cne $sourcePin) { Refuse 'unsigned image differs from independent source pin' }
    if ((Get-ObservedAuthenticode $inputPath).Status -ne 'NotSigned') {
        Refuse 'source image already carries a signature or cannot be inspected'
    }
    # Stage in a unique sibling, never write directly into the final slot.
    # Path observations are not native directory custody. On failure retain
    # ambiguous staging rather than delete through an unretained pathname.
    $candidate = Join-Path $parent ('.keld-dev-sign-' + [guid]::NewGuid().ToString('N'))
    $null = Require-LocalPath $parent
    if (Test-Path -LiteralPath $candidate) { Refuse 'temporary signing sibling already exists' }
    New-Item -ItemType Directory -Path $candidate -ErrorAction Stop | Out-Null
    $temporary = $candidate
    $null = Require-LocalPath $temporary
    if (@(Get-ChildItem -LiteralPath $temporary -Force).Count -ne 0) {
        Refuse 'temporary signing sibling is not empty'
    }
    $destination = Join-Path $temporary ('dev-' + $Role.ToLowerInvariant() + '-' + [IO.Path]::GetFileName($inputPath))
    $copy = [IO.File]::Open($destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $source.Position = 0; $source.CopyTo($copy); $copy.Flush($true) }
    finally { $copy.Dispose() }
    if ((Get-FileSha256 $destination) -cne $sourcePin) {
        Refuse 'copy differs from reviewed source'
    }
    $publicRoot = Join-Path $temporary 'dev-root-public.cer'
    $rootCopy = [IO.File]::Open($publicRoot, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $rootCopy.Write($root.RawData, 0, $root.RawData.Length); $rootCopy.Flush($true) }
    finally { $rootCopy.Dispose() }
    & $signTool sign /s My /sha1 $leaf.Thumbprint /fd SHA256 /ac $publicRoot /d ('keld.app-id/v1:' + $AppId) $destination
    if ($LASTEXITCODE -ne 0) { Refuse 'SignTool failed' }
    $observed = Get-ObservedAuthenticode $destination
    if ($null -eq $observed.SignerCertificate -or
        (Get-Sha256 $observed.SignerCertificate.RawData) -cne (Get-Sha256 $leaf.RawData) -or
        $observed.SignatureType.ToString() -cne 'Authenticode' -or
        $observed.Status.ToString() -cnotin @('Valid', 'UnknownError', 'NotTrusted')) {
        Refuse 'signed copy does not contain expected embedded signer'
    }
    $receipt = [ordered]@{
        schema = 'keld.dev-signing-observation/v1'; scope = 'DEVELOPMENT_ONLY_NONAUTHORIZING'
        role = $Role; appId = $AppId; unsignedSha256 = $sourcePin
        signedSha256 = Get-FileSha256 $destination
        leafCertificateSha256 = Get-Sha256 $leaf.RawData
        rootCertificateSha256 = Get-Sha256 $root.RawData
        osPathObservation = [string]$observed.Status; file = [IO.Path]::GetFileName($destination)
    }
    $receiptPath = Join-Path $temporary 'dev-signing-observation.json'
    $record = [Text.Encoding]::UTF8.GetBytes(($receipt | ConvertTo-Json -Compress))
    $recordFile = [IO.File]::Open($receiptPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $recordFile.Write($record, 0, $record.Length); $recordFile.Flush($true) }
    finally { $recordFile.Dispose() }
    $recordHash = Get-Sha256 $record
    $null = Require-LocalPath $parent
    $null = Require-LocalPath $temporary
    foreach ($entry in @(
        @{ Name = $receipt.file; Hash = $receipt.signedSha256 },
        @{ Name = 'dev-root-public.cer'; Hash = $receipt.rootCertificateSha256 },
        @{ Name = 'dev-signing-observation.json'; Hash = $recordHash }
    )) {
        $path = Require-LocalPath (Join-Path $temporary $entry.Name)
        if ((Get-FileSha256 $path) -cne $entry.Hash) {
            Refuse 'staged signing artifact changed before publication'
        }
    }
    if (Test-Path -LiteralPath $output) { Refuse 'destination appeared before publication' }
    [IO.Directory]::Move($temporary, $output)
    $temporary = $null
    # Publication is not a native custody proof. Recheck final paths and retain
    # any ambiguous publication for inspection without reporting success.
    $null = Require-LocalPath $output
    foreach ($entry in @(
        @{ Name = $receipt.file; Hash = $receipt.signedSha256 },
        @{ Name = 'dev-root-public.cer'; Hash = $receipt.rootCertificateSha256 },
        @{ Name = 'dev-signing-observation.json'; Hash = $recordHash }
    )) {
        $path = Require-LocalPath (Join-Path $output $entry.Name)
        if ((Get-FileSha256 $path) -cne $entry.Hash) {
            Refuse 'published signing artifact changed after rename'
        }
    }
    Write-Output 'KEL270_DEV_SIGNED_COPY_ONLY: no native trust, admission, or release qualification claimed'
} catch {
    if ($null -ne $temporary) {
        Write-Warning "Signing attempt failed; staging retained without path-based cleanup: $temporary"
    }
    throw
} finally { $source.Dispose() }
