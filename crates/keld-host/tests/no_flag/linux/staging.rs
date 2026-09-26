//! Stage integrity and rejection before app resources.

use crate::support::{project::StageFixture, stage::assert_imported_kipc_sidecar_exists};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

#[test]
fn linux_stage_is_owner_private_new_inode_and_byte_consistent() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("KEL-96/T4 must stage the Linux no-flag host");

    assert_eq!(
        stage.host().file_name().and_then(|name| name.to_str()),
        Some("keld-host")
    );
    assert_eq!(
        fs::metadata(stage.root())
            .expect("stage metadata")
            .permissions()
            .mode()
            & 0o7777,
        0o700
    );
    let source = fs::metadata(env!("CARGO_BIN_EXE_keld-host")).expect("source host metadata");
    let copied = fs::metadata(stage.host()).expect("staged host metadata");
    assert_ne!(
        (source.dev(), source.ino()),
        (copied.dev(), copied.ino()),
        "the stage must contain a copy, never a hard link"
    );
    assert_eq!(
        fs::read(stage.host()).expect("read staged host"),
        fs::read(env!("CARGO_BIN_EXE_keld-host")).expect("read source host")
    );
    assert_ne!(copied.permissions().mode() & 0o100, 0);
    assert_eq!(copied.permissions().mode() & 0o222, 0);
}

#[test]
fn linux_stock_create_entry_is_self_contained_after_staging() {
    let root = tempfile::tempdir().expect("stock create root");
    let project = keld_cli::create::create_project(root.path(), "stock-app")
        .expect("create untouched stock app");
    let stage =
        keld_cli::boot::stage_dev_boot(&project, Path::new(env!("CARGO_BIN_EXE_keld-host")))
            .expect("stage untouched stock app");
    assert_imported_kipc_sidecar_exists(&project);
    assert_imported_kipc_sidecar_exists(stage.root());

    let output = Command::new("bun")
        .arg(stage.root().join("src/main.ts"))
        .current_dir(stage.root())
        .env_remove("KELD_APP_LINK")
        .output()
        .expect("run the staged stock entry with Bun");
    assert!(
        !output.status.success(),
        "missing app link must fail closed"
    );
    let stderr = String::from_utf8(output.stderr).expect("stock entry stderr UTF-8");
    assert!(
        stderr.contains("KELD-CLI-010: KELD_APP_LINK is unset"),
        "the staged entry must parse and reach its own missing-link guard: {stderr}"
    );
    assert!(
        !stderr.contains("Cannot find module"),
        "the stock entry must not depend on an unstaged source module: {stderr}"
    );
}

#[test]
fn linux_invalid_boot_and_lease_fail_before_app_resources() {
    let fixture = StageFixture::new();
    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage invalid-boot host");
    fs::set_permissions(
        stage.root().join("keld.boot.json"),
        fs::Permissions::from_mode(0o600),
    )
    .expect("make descriptor mutable for negative fixture");
    fs::write(
        stage.root().join("keld.boot.json"),
        br#"{"schema":1,"name":"invalid","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"},"foreign":true}"#,
    )
    .expect("write invalid descriptor");
    let invalid_boot = Command::new(stage.host())
        .current_dir(stage.root())
        .output()
        .expect("launch invalid boot");
    assert!(!invalid_boot.status.success());
    let stderr = String::from_utf8(invalid_boot.stderr).expect("invalid boot stderr");
    assert!(stderr.contains("KELD-CORE-035"), "{stderr}");
    assert!(stderr.contains("listener=0 child=0 window=0"), "{stderr}");

    let stage = keld_cli::boot::stage_dev_boot(
        &fixture.project,
        Path::new(env!("CARGO_BIN_EXE_keld-host")),
    )
    .expect("stage invalid-lease host");
    let invalid_lease = Command::new(stage.host())
        .current_dir(stage.root())
        .env("KELD_DEV_LEASE", "stdin-v1")
        .stdin(Stdio::null())
        .output()
        .expect("launch invalid lease");
    assert!(!invalid_lease.status.success());
    let stderr = String::from_utf8(invalid_lease.stderr).expect("invalid lease stderr");
    assert!(stderr.contains("KELD-CORE-037"), "{stderr}");
    assert!(
        stderr.contains("requires the CLI-owned pipe reader"),
        "{stderr}"
    );
    assert!(stderr.contains("listener=0 child=0 window=0"), "{stderr}");
}
