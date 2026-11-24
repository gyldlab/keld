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

# DEV signing observation only: exact byte relationship, not Authenticode
# trust. Deliberately support only the ordinary aligned PE32+ layout.
# Keep the admitted unsigned and signed streams open for the entire comparison.
function Read-ExactRange($Stream, [long]$Offset, [int]$Count) {
    if ($Offset -lt 0 -or $Count -lt 0 -or $Offset -gt ($Stream.Length - $Count)) {
        Refuse 'unsupported signing PE range'
    }
    $bytes = New-Object byte[] $Count
    $Stream.Position = $Offset
    $done = 0
    while ($done -lt $Count) {
        $n = $Stream.Read($bytes, $done, $Count - $done)
        if ($n -le 0) { Refuse 'short signing PE read' }
        $done += $n
    }
    return ,$bytes
}

function Get-SigningLayout($Stream) {
    $dos = Read-ExactRange $Stream 0 64
    if ([BitConverter]::ToUInt16($dos, 0) -ne 0x5a4d) { Refuse 'unsupported signing PE DOS header' }
    $nt = [long][BitConverter]::ToUInt32($dos, 60)
    if ($nt -lt 64) { Refuse 'unsupported signing PE offset' }
    $header = Read-ExactRange $Stream $nt 264
    $count = [BitConverter]::ToUInt16($header, 6)
    if ([BitConverter]::ToUInt32($header, 0) -ne 0x4550 -or
        [BitConverter]::ToUInt16($header, 4) -ne 0x8664 -or
        $count -lt 1 -or $count -gt 96 -or
        [BitConverter]::ToUInt16($header, 20) -ne 240 -or
        [BitConverter]::ToUInt16($header, 24) -ne 0x20b -or
        [BitConverter]::ToUInt32($header, 132) -ne 16) {
        Refuse 'unsupported signing PE32+ headers'
    }
    $alignment = [long][BitConverter]::ToUInt32($header, 60)
    $headers = [long][BitConverter]::ToUInt32($header, 84)
    if ($alignment -lt 512 -or $alignment -gt 65536 -or
        ($alignment -band ($alignment - 1)) -ne 0 -or
        $headers % $alignment -ne 0 -or $headers -gt $Stream.Length -or
        ($nt + 264 + 40 * $count) -gt $headers) {
        Refuse 'unsupported signing PE header geometry'
    }
    $sections = Read-ExactRange $Stream ($nt + 264) (40 * $count)
    $ranges = @()
    $rawEnd = $headers
    for ($i = 0; $i -lt $count; $i++) {
        $size = [long][BitConverter]::ToUInt32($sections, ($i * 40 + 16))
        $start = [long][BitConverter]::ToUInt32($sections, ($i * 40 + 20))
        if ($size -eq 0 -and $start -eq 0) { continue }
        if ($size -eq 0 -or $start -lt $headers -or $start % $alignment -ne 0 -or
            $size % $alignment -ne 0 -or ($start + $size) -gt $Stream.Length) {
            Refuse 'unsupported signing PE section range'
        }
        foreach ($range in $ranges) {
            if ($start -lt $range.End -and ($start + $size) -gt $range.Start) {
                Refuse 'overlapping signing PE sections'
            }
        }
        $ranges += @{ Start = $start; End = $start + $size }
        $rawEnd = [Math]::Max($rawEnd, ($start + $size))
    }
    return @{ Nt = $nt; RawEnd = $rawEnd; Certificate = [long][BitConverter]::ToUInt32($header, 168);
        CertificateSize = [long][BitConverter]::ToUInt32($header, 172) }
}

function Require-SignedSourceRelation($Unsigned, $Signed) {
    $before = Get-SigningLayout $Unsigned
    $after = Get-SigningLayout $Signed
    if ($before.Certificate -ne 0 -or $before.CertificateSize -ne 0 -or
        $before.RawEnd -ne $Unsigned.Length -or $after.RawEnd -ne $before.RawEnd -or
        $before.Nt -ne $after.Nt -or $after.Certificate -ne $Unsigned.Length -or
        $after.Certificate % 8 -ne 0 -or $after.CertificateSize -le 8 -or
        $after.CertificateSize -gt 1048576 -or $after.CertificateSize % 8 -ne 0 -or
        ($after.Certificate + $after.CertificateSize) -ne $Signed.Length) {
        Refuse 'unsupported signing certificate or overlay geometry'
    }
    $certificate = Read-ExactRange $Signed $after.Certificate 8
    if ([BitConverter]::ToUInt32($certificate, 0) -ne $after.CertificateSize -or
        [BitConverter]::ToUInt16($certificate, 4) -ne 0x0200 -or
        [BitConverter]::ToUInt16($certificate, 6) -ne 2) {
        Refuse 'unsupported signing certificate entry'
    }
    # Compare all original bytes, including any debug bytes and internal gaps.
    # Only the two documented signing-owned fields are excluded.
    $checksum = $before.Nt + 88
    $security = $before.Nt + 168
    foreach ($range in @(
        @{ Start = 0; End = $checksum },
        @{ Start = $checksum + 4; End = $security },
        @{ Start = $security + 8; End = $Unsigned.Length }
    )) {
        for ($offset = [long]$range.Start; $offset -lt $range.End; $offset += $take) {
            $take = [int][Math]::Min(65536, ($range.End - $offset))
            $a = Read-ExactRange $Unsigned $offset $take
            $b = Read-ExactRange $Signed $offset $take
            if ([Convert]::ToBase64String($a) -cne [Convert]::ToBase64String($b)) {
                Refuse 'signed bytes differ from reviewed source'
            }
        }
    }
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
    $signedStream = [IO.File]::Open($destination, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
    Require-SignedSourceRelation $source $signedStream
    $observed = Get-ObservedAuthenticode $destination
    if ($null -eq $observed.SignerCertificate -or
        (Get-Sha256 $observed.SignerCertificate.RawData) -cne (Get-Sha256 $leaf.RawData) -or
        $observed.SignatureType.ToString() -cne 'Authenticode' -or
        $observed.Status.ToString() -cnotin @('Valid', 'UnknownError', 'NotTrusted')) {
        Refuse 'signed copy does not contain expected embedded signer'
    }
    $signedStream.Position = 0
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $signedHash = [BitConverter]::ToString($sha.ComputeHash($signedStream)).Replace('-', '') }
    finally { $sha.Dispose() }
    $receipt = [ordered]@{
        schema = 'keld.dev-signing-observation/v1'; scope = 'DEVELOPMENT_ONLY_NONAUTHORIZING'
        role = $Role; appId = $AppId; unsignedSha256 = $sourcePin
        signedSha256 = $signedHash
        leafCertificateSha256 = Get-Sha256 $leaf.RawData
        rootCertificateSha256 = Get-Sha256 $root.RawData
        osPathObservation = [string]$observed.Status; file = [IO.Path]::GetFileName($destination)
    }
    } finally { $signedStream.Dispose() }
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
