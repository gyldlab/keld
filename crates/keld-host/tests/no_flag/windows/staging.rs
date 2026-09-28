//! Native stage admission, ACL, descriptor, namespace and junction contracts.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::support::product::StageFixture;
use crate::support::stage::acl_observation;

#[test]
fn windows_stage_is_current_user_protected_and_byte_consistent() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("KEL-96/T4 must stage the Windows no-flag host");

    assert_eq!(
        stage.host().file_name().and_then(|name| name.to_str()),
        Some("keld-host.exe")
    );
    assert_eq!(
        fs::read(stage.host()).expect("read staged host"),
        fs::read(env!("CARGO_BIN_EXE_keld-host")).expect("read source host"),
        "the staged executable must copy the exact already-built host bytes"
    );
    assert!(stage.root().join("keld.boot.json").is_file());
    assert!(stage.root().join("keld.permissions.jsonc").is_file());
    assert!(stage.root().join("src/main.ts").is_file());
    assert!(stage.root().join("index.html").is_file());

    let acl = acl_observation(stage.root());
    assert_eq!(acl["protected"], true, "stage DACL must reject inheritance");
    assert_eq!(acl["count"], 1, "stage DACL must contain one ACE: {acl}");
    assert_eq!(
        acl["sid"], acl["current"],
        "only TokenUser may access the stage"
    );
    assert_eq!(acl["rights"], "FullControl");
    assert_eq!(acl["kind"], "Allow");
    assert_eq!(acl["inherited"], false);
    assert_eq!(acl["inheritance"], "ContainerInherit, ObjectInherit");
    assert_eq!(acl["propagation"], "None");
    for parent in [
        fixture.project.join(".keld"),
        fixture.project.join(".keld/dev"),
    ] {
        let parent_acl = acl_observation(&parent);
        assert_eq!(parent_acl["protected"], true, "{parent_acl}");
        assert_eq!(parent_acl["count"], 1, "{parent_acl}");
        assert_eq!(parent_acl["sid"], parent_acl["current"], "{parent_acl}");
        assert_eq!(parent_acl["rights"], "FullControl", "{parent_acl}");
    }
}

#[test]
fn windows_stage_namespace_is_pinned_until_the_host_owner_releases_it() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage namespace-pinning fixture");
    let keld_root = fixture.project.join(".keld");
    let moved = fixture.project.join(".keld-moved");

    fs::rename(&keld_root, &moved)
        .expect_err("a retained no-share-delete handle must pin the staged pathname chain");
    assert!(stage.host().is_file(), "pinned host path disappeared");

    drop(stage);
    fs::rename(&keld_root, &moved).expect("releasing the stage must release namespace guards");
    fs::rename(&moved, &keld_root).expect("restore fixture namespace");
}

#[test]
fn windows_stage_rejects_a_dev_junction_before_writing_through_it() {
    let fixture = StageFixture::new();
    let external = tempfile::tempdir().expect("external junction target");
    let initial = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("create protected .keld parent");
    drop(initial);
    fs::remove_dir_all(fixture.project.join(".keld/dev")).expect("remove initial dev root");
    let junction = fixture.project.join(".keld/dev");
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "New-Item -ItemType Junction -Path $env:KELD_TEST_JUNCTION -Target $env:KELD_TEST_TARGET | Out-Null",
        ])
        .env("KELD_TEST_JUNCTION", &junction)
        .env("KELD_TEST_TARGET", external.path())
        .output()
        .expect("create dev junction");
    assert!(
        output.status.success(),
        "junction creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let error = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect_err("a reparse-point dev root must fail before staging");
    assert!(error.to_string().contains("launch namespace"), "{error}");
    assert_eq!(
        fs::read_dir(external.path())
            .expect("read external target")
            .filter_map(Result::ok)
            .count(),
        0,
        "staging wrote through the rejected junction"
    );
}

#[test]
fn windows_host_validates_the_staged_descriptor_before_platform_session_start() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage Windows no-flag host");
    fs::write(
        stage.root().join("keld.boot.json"),
        br#"{"schema":1,"name":"invalid","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"foreign":true}"#,
    )
    .expect("replace descriptor with an invalid closed-schema document");

    let output = Command::new(stage.host())
        .current_dir(stage.root())
        .output()
        .expect("launch staged Windows host");
    assert!(!output.status.success(), "invalid boot became success");
    let stderr = String::from_utf8(output.stderr).expect("host stderr is UTF-8");
    assert!(stderr.contains("KELD-CORE-035"), "{stderr}");
    assert!(stderr.contains("unknown field"), "{stderr}");
    assert!(!stderr.contains("KELD-CORE-034"), "{stderr}");
}

#[test]
fn windows_host_rejects_an_added_stage_acl_principal() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage ACL-negative host");
    let script = r"
$acl = New-Object System.Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true, $false)
$current = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$ownerRule = New-Object System.Security.AccessControl.FileSystemAccessRule(
  $current,
  [System.Security.AccessControl.FileSystemRights]::FullControl,
  [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
  [System.Security.AccessControl.PropagationFlags]::None,
  [System.Security.AccessControl.AccessControlType]::Allow)
$world = New-Object System.Security.Principal.SecurityIdentifier('S-1-1-0')
$worldRule = New-Object System.Security.AccessControl.FileSystemAccessRule(
  $world,
  [System.Security.AccessControl.FileSystemRights]::ReadAndExecute,
  [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
  [System.Security.AccessControl.PropagationFlags]::None,
  [System.Security.AccessControl.AccessControlType]::Allow)
$acl.AddAccessRule($ownerRule)
$acl.AddAccessRule($worldRule)
[System.IO.Directory]::SetAccessControl($env:KELD_TEST_ACL_PATH, $acl)
";
    let mutation = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("KELD_TEST_ACL_PATH", stage.root())
        .output()
        .expect("add foreign stage ACE");
    assert!(
        mutation.status.success(),
        "ACL mutation failed: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );

    let output = Command::new(stage.host())
        .current_dir(stage.root())
        .output()
        .expect("launch ACL-negative host");
    assert!(!output.status.success(), "foreign stage ACE became success");
    let stderr = String::from_utf8(output.stderr).expect("ACL-negative stderr UTF-8");
    assert!(stderr.contains("KELD-CORE-036"), "{stderr}");
    assert!(stderr.contains("expected one access rule"), "{stderr}");
}
