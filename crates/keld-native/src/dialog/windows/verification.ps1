# Test-only UIA observer. Inputs come from the retained native fixture, never desktop search.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try {
    if ([Threading.Thread]::CurrentThread.GetApartmentState() -ne 'MTA') {
        throw 'UI Automation must run on this separate MTA process.'
    }
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    $expectedProcess = [int]$env:KELD_TEST_DIALOG_PROCESS
    $expectedWindow = [long]$env:KELD_TEST_DIALOG_WINDOW
    $expectedBefore = [int]$env:KELD_TEST_DIALOG_INITIAL
    $ownedProcess = [Diagnostics.Process]::GetProcessById($expectedProcess)
    try {
        $null = $ownedProcess.Handle # Retain process identity for all subsequent checks.
        $panel = [Windows.Automation.AutomationElement]::FromHandle([IntPtr]$expectedWindow)
        $panelIdentity = ($panel.GetRuntimeId() -join ',')
        function Assert-Panel {
            if ($ownedProcess.HasExited -or
                $panel.Current.ProcessId -ne $expectedProcess -or
                [long]$panel.Current.NativeWindowHandle -ne $expectedWindow -or
                $panel.Current.Name -ne 'Keld dialog proof' -or
                $panel.Current.IsOffscreen -or
                ($panel.GetRuntimeId() -join ',') -ne $panelIdentity) {
                throw 'The exact owned TaskDialog is no longer current.'
            }
        }
        Assert-Panel
        $kind = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::CheckBox)
        $name = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::NameProperty, 'Remember choice')
        $condition = [Windows.Automation.AndCondition]::new($kind, $name)
        $checkboxMatches = $panel.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($checkboxMatches.Count -ne 1) { throw "Expected exactly one real verification checkbox, got $($checkboxMatches.Count)." }
        $checkbox = $checkboxMatches.Item(0)
        $checkboxIdentity = ($checkbox.GetRuntimeId() -join ',')
        function Assert-Checkbox {
            Assert-Panel
            if ($checkbox.Current.ProcessId -ne $expectedProcess -or
                -not $checkbox.Current.IsEnabled -or $checkbox.Current.IsOffscreen -or
                ($checkbox.GetRuntimeId() -join ',') -ne $checkboxIdentity) {
                throw 'The retained verification control is no longer actionable.'
            }
        }
        Assert-Checkbox
        $toggle = [Windows.Automation.TogglePattern]$checkbox.GetCurrentPattern(
            [Windows.Automation.TogglePattern]::Pattern)
        $before = [int]$toggle.Current.ToggleState
        if ($before -notin @(0, 1)) { throw 'Native checkbox state is not binary.' }
        # DirectUI may report Off before the first action even when initially checked.
        # Separate untouched native-result controls prove initial configuration.
        # Keep before as provider evidence; the actual post-action state must match.
        # Toggle the real provider; never send a requested TDM result or checkbox value.
        Assert-Checkbox
        $toggle.Toggle()
        $deadline = [Diagnostics.Stopwatch]::StartNew()
        do {
            Assert-Checkbox
            $after = [int]$toggle.Current.ToggleState
            if ($after -eq (1 - $expectedBefore)) { break }
            if ($after -notin @(0, 1)) { throw 'Checkbox became indeterminate.' }
            Start-Sleep -Milliseconds 40
        } while ($deadline.ElapsedMilliseconds -lt 2500)
        if ($after -ne (1 - $expectedBefore)) { throw 'The native verification control did not toggle.' }
        [ordered]@{
            process = $expectedProcess
            window = $expectedWindow
            before = $before
            after = $after
            panel_runtime_id = $panelIdentity
            checkbox_runtime_id = $checkboxIdentity
        } | ConvertTo-Json -Compress
    } finally {
        $ownedProcess.Dispose()
    }
    exit 0
} catch {
    [Console]::Error.WriteLine($_.ToString())
    exit 1
}
