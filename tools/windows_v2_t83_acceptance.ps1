# KEL-270 T83 Windows v2 installed-positive operator preflight.
# Reads genuine signed image candidates and the real runner token. This is an
# independent, NONAUTHORIZING observation; it never installs or launches Keld.
# The native D45/T81/T82/D90/D46/TUF/AC66-68 owners must supply their joined
# retained-handle, one-shot permit and installed lifecycle acceptance before
# this tool can be changed to issue a positive T83 result.
# The legacy GH894 selector proves a different v1 signed-install fixture.
[CmdletBinding()]
param(
    [string]$ApplicationHost,
    [string]$ApplicationSha256,
    [string]$ApplicationSignerThumbprint,
    [string]$RecoveryHost,
    [string]$RecoverySha256,
    [string]$RecoverySignerThumbprint,
    [string]$InstallerExe,
    [string]$InstallerSha256,
    [string]$InstallerSignerThumbprint,
    [string]$ExpectedOrdinarySid,
    [string]$InstalledRoot,
    [string]$InstallerExecutionEvidence
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Refuse([string]$Reason) {
    throw "KEL270-T83: $Reason; signed Windows v2 installed acceptance NOT_RUN"
}

function Require-Input([string]$Name, [string]$Value) {
    if ([string]::IsNullOrWhiteSpace($Value)) {
        Refuse "missing required $Name"
    }
}

function Require-Pin([string]$Name, [string]$Value, [int]$Length) {
    if ($Value -cnotmatch "^[0-9a-fA-F]{$Length}$") {
        Refuse "$Name must be an independently pinned $Length-character hex value"
    }
    return $Value.ToUpperInvariant()
}

function Require-LocalFile([string]$Name, [string]$Path) {
    if ($Path -cnotmatch '^[a-zA-Z]:\\' -or $Path.Contains('/') -or
        -not [IO.Path]::IsPathRooted($Path) -or
        [IO.Path]::GetFullPath($Path) -ine $Path) {
        Refuse "$Name must be an absolute normalized Windows file path"
    }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
    if ($null -eq $item -or $item -isnot [IO.FileInfo] -or
        (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        Refuse "$Name is not an existing regular non-reparse file"
    }
    return $item.FullName
}

function Get-LocalSha256([string]$Path) {
    # Do not let an ambient Get-FileHash function choose a release digest.
    $hasher = [Security.Cryptography.SHA256]::Create()
    try {
        $stream = [IO.File]::Open($Path, [IO.FileMode]::Open,
            [IO.FileAccess]::Read, [IO.FileShare]::Read)
        try {
            return [BitConverter]::ToString($hasher.ComputeHash($stream)).Replace('-', '')
        } finally {
            $stream.Dispose()
        }
    } finally {
        $hasher.Dispose()
    }
}

function Require-Amd64Image([string]$Path, [string]$Name) {
    $stream = [IO.File]::OpenRead($Path)
    try {
        $reader = New-Object IO.BinaryReader($stream)
        if ($stream.Length -lt 512 -or $reader.ReadUInt16() -ne 0x5A4D) {
            Refuse "$Name is not a complete Windows PE image"
        }
        $stream.Position = 0x3C
        $offset = $reader.ReadInt32()
        if ($offset -lt 0x40 -or [long]$offset -gt ($stream.Length - 6)) {
            Refuse "$Name has a malformed PE header"
        }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550 -or
            $reader.ReadUInt16() -ne 0x8664) {
            Refuse "$Name is not an AMD64 PE image"
        }
    } finally {
        $stream.Dispose()
    }
}

function Get-OsSignature([string]$Path) {
    $manifest = [IO.Path]::Combine($PSHOME, 'Modules',
        'Microsoft.PowerShell.Security', 'Microsoft.PowerShell.Security.psd1')
    if (-not [IO.File]::Exists($manifest)) {
        Refuse 'Windows Security module is unavailable at its OS path'
    }
    $modules = @(Microsoft.PowerShell.Core\Import-Module -Name $manifest -PassThru -ErrorAction Stop)
    if ($modules.Count -ne 1 -or
        -not [string]::Equals($modules[0].Path, $manifest,
            [StringComparison]::OrdinalIgnoreCase)) {
        Refuse 'OS Security module provenance mismatch'
    }
    $command = $modules[0].ExportedCommands['Get-AuthenticodeSignature']
    if ($command -isnot [System.Management.Automation.CmdletInfo] -or
        $command.ImplementingType.FullName -cne
        'Microsoft.PowerShell.Commands.GetAuthenticodeSignatureCommand') {
        Refuse 'OS Authenticode cmdlet provenance mismatch'
    }
    return (& $command -LiteralPath $Path)
}

function Require-ObservedSignedImage([string]$Name, [string]$Path,
    [string]$Digest, [string]$Signer) {
    $file = Require-LocalFile $Name $Path
    if ([IO.Path]::GetExtension($file) -cne '.exe') {
        Refuse "$Name must be a native .exe image"
    }
    Require-Amd64Image $file $Name
    $digestPin = Require-Pin "$Name SHA256" $Digest 64
    $signerPin = Require-Pin "$Name signer" $Signer 40
    if ((Get-LocalSha256 $file) -cne $digestPin) {
        Refuse "$Name differs from independently supplied SHA256"
    }
    $signature = Get-OsSignature $file
    if ($signature.Status.ToString() -cne 'Valid' -or
        $signature.SignatureType.ToString() -cne 'Authenticode' -or
        $null -eq $signature.SignerCertificate) {
        Refuse "$Name lacks a valid embedded Authenticode signature"
    }
    if ($signature.SignerCertificate.Thumbprint.ToUpperInvariant() -cne $signerPin) {
        Refuse "$Name differs from independently supplied signer thumbprint"
    }
    if ((Get-LocalSha256 $file) -cne $digestPin) {
        Refuse "$Name changed between signature and hash observations"
    }
    # These two path reads DO NOT bind a retained native hFile to the signer.
    return [pscustomobject]@{ Path = $file; Digest = $digestPin; Signer = $signerPin }
}

function Read-TokenDword([IntPtr]$Token, [int]$InformationClass) {
    [int]$value = 0
    [int]$returned = 0
    # Windows returns BOOLEAN (one byte) for TokenHasRestrictions (21) on
    # this native OS; the other queried token DWORD classes return four.
    $length = if ($InformationClass -eq 21) { 1 } else { 4 }
    if (-not [KeldT83ReadOnlyToken]::GetTokenInformation(
            $Token, $InformationClass, [ref]$value, 4, [ref]$returned) -or
        $returned -ne $length -or
        ($InformationClass -eq 21 -and $value -ne 0 -and $value -ne 1)) {
        Refuse "native token class $InformationClass was not readable (bytes=$returned win32=$([Runtime.InteropServices.Marshal]::GetLastWin32Error()))"
    }
    return $value
}

function Require-OrdinaryRunner([string]$ExpectedSid) {
    if ($ExpectedSid -cnotmatch '^S-1-5-21-(?:[0-9]+-){3}[0-9]+$') {
        Refuse 'ExpectedOrdinarySid must be an independently recorded account SID'
    }
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    try {
        if ($null -eq $identity.User -or $identity.User.Value -cne $ExpectedSid) {
            Refuse 'actual Windows account SID differs from pinned ordinary SID'
        }
        $principal = New-Object Security.Principal.WindowsPrincipal($identity)
        $groupSids = @($identity.Groups | ForEach-Object { $_.Value })
        if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) -or
            $groupSids -contains 'S-1-5-32-544' -or
            $ExpectedSid -match '-(?:500|501)$') {
            Refuse 'runner is an administrator or filtered administrator account'
        }
        # TokenHasRestrictions detects UAC filtering even if IsInRole is false.
        # The current process token is queried only; no token duplication/adjustment.
        $token = $identity.Token
        if ((Read-TokenDword $token 8) -ne 1 -or
            (Read-TokenDword $token 20) -ne 0 -or
            (Read-TokenDword $token 21) -ne 0 -or
            (Read-TokenDword $token 26) -ne 0 -or
            (Read-TokenDword $token 29) -ne 0) {
            Refuse 'runner has a nonordinary primary, elevated, filtered, UIAccess or AppContainer token'
        }
        if ((Read-TokenDword $token 12) -ne
            [Diagnostics.Process]::GetCurrentProcess().SessionId) {
            Refuse 'runner token session differs from its process session'
        }
        # OS-supplied group SID; locale-independent (unlike group display names).
        $whoami = Join-Path ([Environment]::GetFolderPath('System')) 'whoami.exe'
        if (-not [IO.File]::Exists($whoami)) {
            Refuse 'OS whoami.exe is missing for medium integrity observation'
        }
        $rows = (& $whoami /groups /fo csv /nh | Out-String)
        if ($LASTEXITCODE -ne 0 -or
            $rows -notmatch '(?<!\d)S-1-16-8192(?!\d)' -or
            $rows -match '(?<!\d)S-1-5-32-544(?!\d)') {
            Refuse 'runner lacks observed medium integrity or retains administrator group'
        }
    } finally {
        $identity.Dispose()
    }
}

if ($env:OS -cne 'Windows_NT' -or $PSVersionTable.PSVersion.Major -ne 5) {
    Refuse 'native Windows PowerShell 5.1 is required'
}
foreach ($name in @(
    'ApplicationHost', 'ApplicationSha256', 'ApplicationSignerThumbprint',
    'RecoveryHost', 'RecoverySha256', 'RecoverySignerThumbprint',
    'InstallerExe', 'InstallerSha256', 'InstallerSignerThumbprint',
    'ExpectedOrdinarySid', 'InstalledRoot', 'InstallerExecutionEvidence'
)) {
    Require-Input $name (Get-Variable -Name $name -ValueOnly)
}

$app = Require-ObservedSignedImage 'ApplicationHost' $ApplicationHost $ApplicationSha256 $ApplicationSignerThumbprint
$recovery = Require-ObservedSignedImage 'RecoveryHost' $RecoveryHost $RecoverySha256 $RecoverySignerThumbprint
$installer = Require-ObservedSignedImage 'InstallerExe' $InstallerExe $InstallerSha256 $InstallerSignerThumbprint

# This reads OS token facts. It cannot commission an account or validate D42's
# actual installer worker token, which the original Guard must admit natively.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class KeldT83ReadOnlyToken {
    [DllImport("advapi32.dll", SetLastError = true)]
    public static extern bool GetTokenInformation(IntPtr token, int type,
        out int value, int length, out int returned);
}
'@ -ErrorAction Stop
Require-OrdinaryRunner $ExpectedOrdinarySid

if ($app.Digest -ceq $recovery.Digest -or
    $app.Digest -ceq $installer.Digest -or
    $recovery.Digest -ceq $installer.Digest -or
    $app.Signer -ceq $recovery.Signer) {
    Refuse 'APP, RECOVERY and installer lack independent roles and distinct signed bytes'
}

# An installed path or a caller-written JSON receipt grants no TUF, D46 or
# protected-provenance authority. Observe missing files only; never execute an
# NSIS installer, original Host, CLI, provider or recovery process here.
$installedHost = Join-Path $InstalledRoot 'bin\keld-host.exe'
$installedRecovery = Join-Path $InstalledRoot 'recovery\keld-host.exe'
$installedCli = Join-Path $InstalledRoot 'bin\keld.exe'
foreach ($name in @('installedHost', 'installedRecovery', 'installedCli')) {
    $null = Require-LocalFile $name (Get-Variable -Name $name -ValueOnly)
}
if ((Get-LocalSha256 $installedHost) -cne $app.Digest -or
    (Get-LocalSha256 $installedRecovery) -cne $recovery.Digest) {
    Refuse 'installed original Host/Recovery bytes differ from independently supplied images'
}
$null = Require-LocalFile 'InstallerExecutionEvidence' $InstallerExecutionEvidence

Write-Output 'KEL270_T83_PATH_AND_TOKEN_OBSERVATIONS_ONLY native_d45=NOT_RUN tuf_permit=NOT_VERIFIED provider_permanent_binding=NOT_VERIFIED no_flag=NOT_RUN drawio=0/10 crash_recovery=NOT_RUN ac66_68=NOT_RUN'
Refuse 'native retained-handle APP/RECOVERY authority, signed TUF/one-use permit, installed installer readback, permanent CLI/provider registration, no-flag launch, ten draw.io cells and crash/recovery evidence are unavailable'
