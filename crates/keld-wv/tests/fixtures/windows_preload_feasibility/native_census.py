"""Read-only post-process census for this fixture's exact owned profile prefix."""
import json
import os
from pathlib import Path
import subprocess


def observe(case: Path, profile: Path):
    environment = os.environ.copy()
    environment["KELD_W1_OWNED_PROFILE"] = str(profile)
    script = r"""
$ErrorActionPreference = 'Stop'
$owned = @(Get-CimInstance Win32_Process | Where-Object {
    $_.Name -ieq 'msedgewebview2.exe' -and $_.CommandLine -and
    $_.CommandLine.IndexOf($env:KELD_W1_OWNED_PROFILE,
        [System.StringComparison]::OrdinalIgnoreCase) -ge 0
} | Select-Object ProcessId,ParentProcessId,Name,CreationDate)
ConvertTo-Json -InputObject $owned -Depth 3 -Compress
"""
    completed = subprocess.run(
        ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script],
        env=environment, capture_output=True, text=True, timeout=20, check=False,
    )
    receipt = {
        "profile": str(profile), "profileExists": profile.exists(),
        "exitCode": completed.returncode, "stderr": completed.stderr,
        "scope": "independent Windows process command-line census, including crashpad",
        "matchingProcesses": json.loads(completed.stdout) if completed.returncode == 0 else None,
    }
    (case / "post-exit-process-census.json").write_text(
        json.dumps(receipt, indent=2) + "\n", encoding="utf-8"
    )
    if completed.returncode != 0:
        raise RuntimeError("Owned-process census failed; stop before another native case")
    if receipt["matchingProcesses"] or receipt["profileExists"]:
        raise RuntimeError("Owned native resource remains; preserve it and stop the matrix")
    return receipt
