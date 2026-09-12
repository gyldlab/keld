//! KEL-130 regression oracles use owned OS resources and literal contract limits.
#![allow(clippy::expect_used)] // Integration assertions and fixture setup.

use keld_guard::{PermissionsManifest, Principal, parse_manifest};
use keld_native::fs::{fs_read, fs_write};
use std::path::{Path, PathBuf};

fn owned_root(case: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keld-kel130-{case}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir(&root).expect("create owned root");
    root
}

fn spelling(path: &Path) -> String {
    path.to_str().expect("UTF-8 fixture").replace('\\', "/")
}

fn manifest(root: &Path) -> PermissionsManifest {
    parse_manifest(&format!(
        r#"{{"app":{{"fs":{{"read":["{0}/**"],"write":["{0}/**"]}}}}}}"#,
        spelling(root)
    ))
    .expect("manifest")
}

#[test]
fn content_limit_rejects_read_and_write_without_mutating_target() {
    let root = owned_root("content-limit");
    let path = root.join("file");
    let grants = manifest(&root);
    let content = vec![0x79; 8 * 1024 * 1024 + 1];
    std::fs::write(&path, &content).expect("seed oversized file");
    let read = fs_read(&grants, Principal::AppProcess, &spelling(&path));
    std::fs::write(&path, b"unchanged").expect("seed write sentinel");
    let write = fs_write(&grants, Principal::AppProcess, &spelling(&path), &content);
    let after = std::fs::read(&path).expect("observe sentinel");
    println!(
        "oversized read={:?}; write={write:?}; final_length={}",
        read.as_ref().map(Vec::len),
        after.len()
    );
    let control = fs_write(
        &grants,
        Principal::AppProcess,
        &spelling(&root.join("fresh")),
        b"fresh",
    )
    .and_then(|()| {
        fs_read(
            &grants,
            Principal::AppProcess,
            &spelling(&root.join("fresh")),
        )
    });
    std::fs::remove_dir_all(&root).expect("cleanup owned root");
    assert_eq!(control.expect("fresh allowed operation"), b"fresh");
    assert_eq!(
        read.as_ref().err().map(|e| e.code()),
        Some("KELD-NATIVE-004")
    );
    assert_eq!(
        write.expect_err("oversized write must reject").code(),
        "KELD-NATIVE-004"
    );
    assert_eq!(after, b"unchanged");
}

#[cfg(windows)]
#[test]
fn junction_cannot_read_or_write_outside_grant() {
    let root = owned_root("junction");
    let inside = root.join("inside");
    let outside = root.join("outside");
    std::fs::create_dir(&inside).expect("inside");
    std::fs::create_dir(&outside).expect("outside");
    std::fs::write(inside.join("sentinel"), b"inside-original").expect("inside sentinel");
    std::fs::write(outside.join("sentinel"), b"outside-original").expect("outside sentinel");
    let link = inside.join("link");
    let junction = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:KEL130_LINK -Target $env:KEL130_TARGET | Out-Null"])
        .env("KEL130_LINK", &link)
        .env("KEL130_TARGET", &outside)
        .output().expect("junction command");
    assert!(
        junction.status.success(),
        "junction setup failed: {}",
        String::from_utf8_lossy(&junction.stderr)
    );
    let grants = manifest(&inside);
    let denied = fs_read(
        &grants,
        Principal::AppProcess,
        &spelling(&outside.join("sentinel")),
    );
    let read = fs_read(
        &grants,
        Principal::AppProcess,
        &spelling(&link.join("sentinel")),
    );
    let write = fs_write(
        &grants,
        Principal::AppProcess,
        &spelling(&link.join("sentinel")),
        b"escaped-write",
    );
    let outside_after = std::fs::read(outside.join("sentinel")).expect("outside oracle");
    let inside_after = std::fs::read(inside.join("sentinel")).expect("inside oracle");
    println!(
        "direct={denied:?}; junction_read={read:?}; junction_write={write:?}; outside={outside_after:?}; inside={inside_after:?}"
    );
    let control = fs_read(
        &grants,
        Principal::AppProcess,
        &spelling(&inside.join("sentinel")),
    );
    std::fs::remove_dir(&link).expect("remove only owned junction");
    std::fs::remove_dir_all(&root).expect("cleanup owned root after removing junction");
    assert_eq!(control.expect("fresh allowed read"), b"inside-original");
    assert_eq!(
        denied.expect_err("outside lexical path denies").code(),
        "KELD-GUARD002"
    );
    assert_eq!(
        read.expect_err("junction read must reject").code(),
        "KELD-NATIVE-002"
    );
    assert_eq!(
        write.expect_err("junction write must reject").code(),
        "KELD-NATIVE-002"
    );
    assert_eq!(outside_after, b"outside-original");
    assert_eq!(inside_after, b"inside-original");
}

#[cfg(windows)]
#[test]
fn reserved_device_is_not_a_regular_file() {
    let root = owned_root("device");
    let grants = manifest(&root);
    let result = fs_read(&grants, Principal::AppProcess, &spelling(&root.join("NUL")));
    println!("reserved NUL read={result:?}");
    let control = fs_write(
        &grants,
        Principal::AppProcess,
        &spelling(&root.join("fresh")),
        b"fresh",
    )
    .and_then(|()| {
        fs_read(
            &grants,
            Principal::AppProcess,
            &spelling(&root.join("fresh")),
        )
    });
    std::fs::remove_dir_all(&root).expect("cleanup owned root");
    assert_eq!(control.expect("fresh allowed operation"), b"fresh");
    assert!(
        result.is_err(),
        "reserved device cannot return successful file bytes"
    );
}
