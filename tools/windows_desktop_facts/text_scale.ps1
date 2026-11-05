# Read-only WinRT input for GH-1186 feasibility; never substitutes a registry value.
$ErrorActionPreference = 'Stop'
try {
    if ($PSVersionTable.PSEdition -ne 'Desktop' -or $PSVersionTable.PSVersion.Major -ne 5) {
        throw 'This fixture requires Windows PowerShell 5.1 WinRT projection.'
    }
    [Windows.UI.ViewManagement.UISettings, Windows.UI.ViewManagement, ContentType=WindowsRuntime] | Out-Null
    $w4Settings = New-Object Windows.UI.ViewManagement.UISettings
    $w4Scale = [double]$w4Settings.TextScaleFactor
    if ([double]::IsNaN($w4Scale) -or [double]::IsInfinity($w4Scale) -or $w4Scale -lt 1.0 -or $w4Scale -gt 2.25) {
        throw 'Invalid native UISettings.TextScaleFactor; no default is permitted.'
    }
    [pscustomobject]@{
        source = 'Windows.UI.ViewManagement.UISettings.TextScaleFactor'
        value = $w4Scale
        pid = $PID
        powershell = $PSVersionTable.PSVersion.ToString()
        edition = $PSVersionTable.PSEdition
        apartment = [Threading.Thread]::CurrentThread.GetApartmentState().ToString()
    } | ConvertTo-Json -Compress
    exit 0
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
