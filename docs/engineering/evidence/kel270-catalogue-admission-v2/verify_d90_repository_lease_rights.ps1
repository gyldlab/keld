# KEL-270 D90 disposable Windows handle-sharing oracle. No product authority.
# Run with Windows PowerShell 5.1: powershell.exe -NoProfile -NonInteractive
# -File docs/engineering/evidence/kel270-catalogue-admission-v2/verify_d90_repository_lease_rights.ps1
$ErrorActionPreference = 'Stop'

$rights = [System.Security.AccessControl.FileSystemRights]
$minimal = $rights::ReadAttributes -bor $rights::ReadPermissions -bor $rights::Synchronize
$withData = $minimal -bor $rights::ReadData
if ([int]$minimal -ne 0x00120080 -or [int]$withData -ne 0x00120081) {
    throw 'unexpected Windows FileSystemRights mapping'
}

function Open-Owner([string]$path, [System.Security.AccessControl.FileSystemRights]$access) {
    New-Object System.IO.FileStream(
        $path,
        [System.IO.FileMode]::Open,
        $access,
        [System.IO.FileShare]::None,
        1,
        [System.IO.FileOptions]::None
    )
}

function Open-SecondReader([string]$path) {
    [System.IO.File]::Open(
        $path,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        # Share with the existing owner. Otherwise this second reader could
        # fail because IT denies the owner, even if the owner is incorrectly
        # opened with FILE_SHARE_READ.
        [System.IO.FileShare]::ReadWrite
    )
}

function Assert-ReaderExcluded([string]$path) {
    $second = $null
    try {
        $second = Open-SecondReader $path
        throw 'a second data reader opened despite the lease'
    } catch [System.IO.IOException] {
        if (($_.Exception.HResult -band 0xffff) -ne 32) {
            throw ('unexpected second-open error 0x{0:X8}' -f $_.Exception.HResult)
        }
    } finally {
        if ($null -ne $second) { $second.Dispose() }
    }
}

$fixture = Join-Path ([System.IO.Path]::GetTempPath()) (
    'keld-d90-lease-w3-' + [Guid]::NewGuid().ToString('N')
)
$first = $null
$other = $null
$attribute = $null
try {
    $repoA = Join-Path $fixture ('acquisition-v1\repositories\' + ('aa' * 32))
    $repoB = Join-Path $fixture ('acquisition-v1\repositories\' + ('bb' * 32))
    [System.IO.Directory]::CreateDirectory($repoA) | Out-Null
    [System.IO.Directory]::CreateDirectory($repoB) | Out-Null
    # This leaf is a proposed fixed identifier, not a commissioned D44 schema.
    $lockA = Join-Path $repoA 'repository.lock'
    $lockB = Join-Path $repoB 'repository.lock'
    [System.IO.File]::WriteAllBytes($lockA, [byte[]]@())
    [System.IO.File]::WriteAllBytes($lockB, [byte[]]@())

    # Negative control: an attributes-only source open with FileShare.None
    # DOES NOT exclude a second data reader on Windows.
    $first = Open-Owner $lockA $minimal
    $reader = Open-SecondReader $lockA
    $reader.Dispose()
    $first.Dispose()
    $first = $null
    Write-Output 'D90_LOW_RIGHTS_FALSE_EXCLUSION=PASS 0x00120080'

    # Adding FILE_READ_DATA makes a second conflicting read fail with code 32.
    $first = Open-Owner $lockA $withData
    Assert-ReaderExcluded $lockA
    Write-Output 'D90_SAME_FILE_EXCLUSIVE=PASS 0x00120081 errno=32'

    # The same name under a different physical root is a DIFFERENT lease.
    # D44 authentication must select one repository root for every writer.
    $other = Open-Owner $lockB $withData
    Assert-ReaderExcluded $lockB
    Write-Output 'D90_DIFFERENT_REPOSITORY_STILL_OPENS=PASS'

    # Attribute-only access is not excluded by CreateFileW share flags.
    # This is not permission to edit attributes or a reparse point.
    $attribute = New-Object System.IO.FileStream(
        $lockA,
        [System.IO.FileMode]::Open,
        $rights::WriteAttributes,
        [System.IO.FileShare]::ReadWrite,
        1,
        [System.IO.FileOptions]::None
    )
    Write-Output 'D90_ATTRIBUTE_ONLY_REMAINING_RISK=PASS'
    $attribute.Dispose()
    $attribute = $null
    $first.Dispose()
    $first = $null
    $other.Dispose()
    $other = $null

    # A released real handle permits another writer. An absent lock cannot
    # be implicitly created by FileMode.Open.
    $first = Open-Owner $lockA $withData
    Assert-ReaderExcluded $lockA
    $first.Dispose()
    $first = $null
    $missing = Join-Path $repoA 'missing-repository.lock'
    try {
        $unexpected = Open-Owner $missing $withData
        $unexpected.Dispose()
        throw 'opening an absent repository lock unexpectedly created it'
    } catch [System.IO.FileNotFoundException] {
        if ([System.IO.File]::Exists($missing)) {
            throw 'the absent lock was created'
        }
    }
    if (([System.IO.File]::ReadAllBytes($lockA)).Length -ne 0 -or
        ([System.IO.File]::ReadAllBytes($lockB)).Length -ne 0) {
        throw 'probe altered a fixed lock file'
    }
    Write-Output 'D90_REACQUIRE_AND_MISSING_LOCK=PASS'
} finally {
    if ($null -ne $attribute) { $attribute.Dispose() }
    if ($null -ne $other) { $other.Dispose() }
    if ($null -ne $first) { $first.Dispose() }
    if ([System.IO.Directory]::Exists($fixture)) {
        [System.IO.Directory]::Delete($fixture, $true)
    }
}
