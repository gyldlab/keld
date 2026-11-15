//! Independent Windows stage ACL observation.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

pub(crate) fn acl_observation(path: &Path) -> Value {
    let script = r"
$acl = Get-Acl -LiteralPath $env:KELD_TEST_ACL_PATH
$current = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$rules = @($acl.Access)
$sid = ''
$rights = ''
$kind = ''
$inherited = $false
$inheritance = ''
$propagation = ''
if ($rules.Count -eq 1) {
  $sid = $rules[0].IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value
  $rights = $rules[0].FileSystemRights.ToString()
  $kind = $rules[0].AccessControlType.ToString()
  $inherited = $rules[0].IsInherited
  $inheritance = $rules[0].InheritanceFlags.ToString()
  $propagation = $rules[0].PropagationFlags.ToString()
}
[pscustomobject]@{
  protected = $acl.AreAccessRulesProtected
  current = $current
  count = $rules.Count
  sid = $sid
  rights = $rights
  kind = $kind
  inherited = $inherited
  inheritance = $inheritance
  propagation = $propagation
} | ConvertTo-Json -Compress
";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("KELD_TEST_ACL_PATH", path)
        .output()
        .expect("query stage ACL with the Windows security API through PowerShell");
    assert!(
        output.status.success(),
        "ACL query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("PowerShell ACL observation is JSON")
}
