# KEL-270 D-44/D-90 native Win32 negative evidence, NOT repository authority.
# Proves that share_mode(0) is not an exclusive namespace or attribute fence.
# Uses newly generated disposable native NTFS directories and lock files ONLY.
# Does not touch a Keld installation, repository, trust store, or registry.
#
# Windows PowerShell 5.1:
# powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File
#   docs/engineering/evidence/kel270-catalogue-admission-v2/verify_d90_repository_identity_mutations.ps1
#   -EvidenceParent <existing reviewed fixed-drive directory>
#
# This is not an operative D-90 census, D-44 root or D-46 permit.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$EvidenceParent
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Require-FixedParent([string]$Value) {
    if ($env:OS -cne 'Windows_NT' -or
        $Value -cnotmatch '^[A-Za-z]:\\' -or
        $Value.Contains('/') -or $Value.Substring(2).Contains(':') -or
        $Value -match '[\r\n\x22\x24\x21\x3b\x60\x5b\x5d]' -or
        $Value -match '(^|\\)\.\.($|\\)' -or
        [IO.Path]::GetFullPath($Value) -cne $Value -or
        -not (Test-Path -LiteralPath $Value -PathType Container)) {
        throw 'evidence parent must be an existing canonical fixed local directory'
    }
    $drive = New-Object IO.DriveInfo($Value.Substring(0,1))
    if ($drive.DriveType -ne [IO.DriveType]::Fixed -or
        $drive.DriveFormat -cne 'NTFS') {
        throw 'evidence parent must be on a fixed NTFS volume'
    }
    $current = $Value
    while ($current) {
        $item = Get-Item -LiteralPath $current -Force -ErrorAction SilentlyContinue
        if ($null -eq $item -or
            (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            throw 'evidence parent has a missing or reparse-point ancestor'
        }
        $next = [IO.Path]::GetDirectoryName($current)
        if (!$next -or $next -ceq $current) { break }
        $current = $next
    }
    return $Value
}
$parent = Require-FixedParent $EvidenceParent
$fixture = Join-Path $parent ('kel270-d90-native-' + [Guid]::NewGuid().ToString('N'))
if (Test-Path -LiteralPath $fixture) { throw 'new disposable fixture unexpectedly exists' }
$fixtureName = [IO.Path]::GetFileName($fixture)
New-Item -Path $parent -Name $fixtureName -ItemType Directory -ErrorAction Stop | Out-Null
if (((Get-Item -LiteralPath $fixture -Force).Attributes -band
        [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'new disposable fixture unexpectedly became a reparse point'
}

# These declarations use only Windows' documented kernel32 contracts:
# GetFileInformationByHandle, SetFileInformationByHandle(FileBasicInfo),
# CreateFileW, and CreateHardLinkW. Every HANDLE is wrapped by SafeFileHandle
# and disposed even when an assertion refuses. No Keld production FFI added.
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class KeldD90NativeNegative {
    [StructLayout(LayoutKind.Sequential)]
    public struct FileInfo {
        public uint Attributes, CreationLow, CreationHigh, AccessLow, AccessHigh;
        public uint WriteLow, WriteHigh, Volume, SizeHigh, SizeLow, Links, IdHigh, IdLow;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct BasicInfo {
        public long Creation, Access, Write, Change;
        public uint Attributes;
    }
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    public static extern SafeFileHandle CreateFileW(
        string path, uint desired, uint share, IntPtr security, uint disposition,
        uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool GetFileInformationByHandle(SafeFileHandle handle, out FileInfo info);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool SetFileInformationByHandle(
        SafeFileHandle handle, int informationClass, ref BasicInfo info, uint length);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    public static extern bool CreateHardLinkW(
        string newLink, string existingFile, IntPtr security);
    public static SafeFileHandle Open(string path, uint rights, uint sharing, uint flags) {
        var handle = CreateFileW(path, rights, sharing, IntPtr.Zero, 3, flags, IntPtr.Zero);
        if (handle.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error());
        return handle;
    }
    public static FileInfo Read(SafeFileHandle handle) {
        FileInfo value;
        if (!GetFileInformationByHandle(handle, out value))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        return value;
    }
    public static string Identity(FileInfo info) {
        return String.Format("{0:X8}:{1:X8}{2:X8}", info.Volume, info.IdHigh, info.IdLow);
    }
    public static void SetHidden(SafeFileHandle handle) {
        BasicInfo info = new BasicInfo();
        info.Attributes = 2; // FILE_ATTRIBUTE_HIDDEN, disposable scratch only
        if (!SetFileInformationByHandle(handle, 0, ref info,
            (uint)Marshal.SizeOf(typeof(BasicInfo))))
            throw new Win32Exception(Marshal.GetLastWin32Error());
    }
    public static void CreateHardLink(string newLink, string existingFile) {
        if (!CreateHardLinkW(newLink, existingFile, IntPtr.Zero))
            throw new Win32Exception(Marshal.GetLastWin32Error());
    }
}
'@

$handles = New-Object 'System.Collections.Generic.List[Microsoft.Win32.SafeHandles.SafeFileHandle]'
function Hold([string]$Name,[uint32]$Rights,[uint32]$Share,[uint32]$Flags) {
    $h = [KeldD90NativeNegative]::Open($Name,$Rights,$Share,$Flags)
    $handles.Add($h)
    return $h
}
$receipt = [ordered]@{
    schema = 'keld.d90.disposable-win32-identity-negative/v1'
    fixture = $fixture
    source_kind = 'DISPOSABLE_NONAUTHORIZING_WINDOWS_SCRATCH'
    d44_native_root = 'NOT_QUALIFIED'
    d90_repository_census = 'NOT_QUALIFIED'
    d46_one_use_permit = 'NOT_QUALIFIED'
    original_installer = 'NOT_BUILT'
    claims = @()
}
try {
    $claim = 'aa' * 32
    $repoA = Join-Path $fixture ('A\repositories\' + $claim)
    $repoB = Join-Path $fixture ('B\repositories\' + $claim)
    [IO.Directory]::CreateDirectory($repoA) | Out-Null
    [IO.Directory]::CreateDirectory($repoB) | Out-Null
    $lockA = Join-Path $repoA 'repository.lock'
    $lockB = Join-Path $repoB 'repository.lock'
    [IO.File]::WriteAllBytes($lockA, [byte[]]@())
    [IO.File]::WriteAllBytes($lockB, [byte[]]@())

    # The two physically distinct roots can both hold an exclusive handle
    # for the SAME logical repository claim. This is a D-44 failure witness.
    $rootA = Hold $repoA 0x80 7 0x02200000
    $rootB = Hold $repoB 0x80 7 0x02200000
    $ownerA = Hold $lockA 0x00120081 0 0x00200000
    $ownerB = Hold $lockB 0x00120081 0 0x00200000
    $a = [KeldD90NativeNegative]::Read($rootA)
    $b = [KeldD90NativeNegative]::Read($rootB)
    $before = [KeldD90NativeNegative]::Read($ownerA)
    $other = [KeldD90NativeNegative]::Read($ownerB)
    $idA = [KeldD90NativeNegative]::Identity($a)
    $idB = [KeldD90NativeNegative]::Identity($b)
    $lockId = [KeldD90NativeNegative]::Identity($before)
    if ($idA -ceq $idB -or $lockId -ceq [KeldD90NativeNegative]::Identity($other)) {
        throw 'different physical roots or locks accidentally have the same native ID'
    }
    $receipt.claims += 'same-logical-claim-two-physical-roots'
    Write-Output "PASS_D90_TWO_ROOTS_SAME_LOGICAL_CLAIM rootA=$idA rootB=$idB"

    # Even while the lock is FILE_SHARE_NONE, a second WriteAttributes-only
    # HANDLE can change native metadata on the EXACT SAME FILE identity.
    $attributes = Hold $lockA 0x100 7 0x00200000
    [KeldD90NativeNegative]::SetHidden($attributes)
    $after = [KeldD90NativeNegative]::Read($ownerA)
    if ([KeldD90NativeNegative]::Identity($after) -cne $lockId -or
        ($before.Attributes -band 2) -ne 0 -or
        ($after.Attributes -band 2) -eq 0) {
        throw 'attribute-only change did not affect original held file identity'
    }
    $receipt.claims += 'attribute-only-mutates-exclusively-held-file'
    $receipt['attributes_before'] = [int64]$before.Attributes
    $receipt['attributes_after'] = [int64]$after.Attributes
    Write-Output "PASS_D90_ATTRIBUTE_MUTATION sameFile=$lockId before=$($before.Attributes) after=$($after.Attributes)"

    # A late hardlink under the SAME volume changes the retained FILE's
    # NumberOfLinks without requiring another data-reader handle. Prove by
    # reading the SAME original HANDLE before/after, not by name alone.
    $lateAlias = Join-Path $repoA 'repository-hardlink-alias.lock'
    $beforeHardlink = [KeldD90NativeNegative]::Read($ownerA)
    if ($beforeHardlink.Links -ne 1) { throw 'hardlink fixture already has multiple links' }
    [KeldD90NativeNegative]::CreateHardLink($lateAlias, $lockA)
    $afterHardlink = [KeldD90NativeNegative]::Read($ownerA)
    if ($afterHardlink.Links -ne 2 -or
        [KeldD90NativeNegative]::Identity($afterHardlink) -cne $lockId -or
        -not [IO.File]::Exists($lateAlias)) {
        throw 'new hardlink did not mutate existing exclusive lock namespace'
    }
    $receipt.claims += 'late-hardlink-mutates-held-link-count'
    Write-Output "PASS_D90_LATE_HARDLINK_SAME_HANDLE sameFile=$lockId links=$($beforeHardlink.Links)->$($afterHardlink.Links)"

    # Repoint a disposable junction while the old target DIRECTORY handle
    # stays alive: path re-resolution observes B but retained handle stays A.
    # This proves why path strings cannot select a D-44 admitted root.
    $alias = Join-Path $fixture 'current-path-alias'
    New-Item -ItemType Junction -Path $alias -Target $repoA -ErrorAction Stop | Out-Null
    $old = Hold $alias 0x80 7 0x02000000
    $oldId = [KeldD90NativeNegative]::Identity(
        [KeldD90NativeNegative]::Read($old))
    if ($oldId -cne $idA) { throw 'initial junction does not target expected native A' }
    [IO.Directory]::Delete($alias)
    New-Item -ItemType Junction -Path $alias -Target $repoB -ErrorAction Stop | Out-Null
    $new = Hold $alias 0x80 7 0x02000000
    $newId = [KeldD90NativeNegative]::Identity(
        [KeldD90NativeNegative]::Read($new))
    $retained = [KeldD90NativeNegative]::Identity(
        [KeldD90NativeNegative]::Read($old))
    if ($newId -cne $idB -or $retained -cne $oldId) {
        throw 'reparse alias swap was not visible while original directory stayed held'
    }
    $receipt.claims += 'reparse-alias-swap-retained-original-handle'
    Write-Output "PASS_D90_JUNCTION_SWAP pathNow=$newId heldBefore=$retained"

    if ($receipt.claims.Count -ne 4) { throw 'native D90 negative coverage incomplete' }
    $receipt['root_a_id'] = $idA
    $receipt['root_b_id'] = $idB
    $receipt['original_lock_id'] = $lockId
    $receipt['original_before_hardlink_links'] = [int]$beforeHardlink.Links
    $receipt['original_after_hardlink_links'] = [int]$afterHardlink.Links
    $receipt['path_after_swap_id'] = $newId
    $receipt['retained_before_swap_id'] = $retained
} finally {
    foreach ($handle in $handles) { $handle.Dispose() }
    # Retain only this unique scratch fixture for independent physical review.
    # Do not recursively delete a root or path whose identity might have
    # changed during an adversarial alias swap.
}
if ($receipt.claims.Count -ne 4) { throw 'no success receipt for incomplete native tests' }
$output = Join-Path $fixture 'd90-nonauthorizing-receipt.json'
if (Test-Path -LiteralPath $output) { throw 'receipt destination appeared unexpectedly' }
($receipt | ConvertTo-Json -Depth 8) |
    Set-Content -LiteralPath $output -Encoding UTF8
Write-Output "D90_NATIVE_NONAUTHORIZING_4_OF_4_PASS evidence=$output"
