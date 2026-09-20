//! KEL-130 regression oracles use owned OS resources and literal contract limits.
#![allow(clippy::expect_used)] // Integration assertions and fixture setup.

use keld_guard::Principal;
use keld_guard::verified_manifest::{VerifiedManifest, load_verified_manifest};
use keld_native::fs::FsBroker;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

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

fn verified_from_text(root: &Path, name: &str, text: &str) -> VerifiedManifest {
    let path = root.join(name);
    std::fs::write(&path, text).expect("write manifest");
    let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    load_verified_manifest(
        std::fs::File::open(&path).expect("open manifest"),
        path,
        digest,
    )
    .expect("verified manifest")
}

fn manifest(root: &Path) -> VerifiedManifest {
    let text = format!(
        r#"{{"app":{{"fs":{{"read":["{0}/**"],"write":["{0}/**"]}}}}}}"#,
        spelling(root)
    );
    verified_from_text(root, "keld.permissions.jsonc", &text)
}

#[test]
fn content_limit_rejects_read_and_write_without_mutating_target() {
    let root = owned_root("content-limit");
    let path = root.join("file");
    let grants = manifest(&root);
    let broker = FsBroker::prepare(&grants).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    let content = vec![0x79; 8 * 1024 * 1024 + 1];
    std::fs::write(&path, &content).expect("seed oversized file");
    let read = broker.read(&grants, Principal::AppProcess, &spelling(&path), &cancelled);
    std::fs::write(&path, b"unchanged").expect("seed write sentinel");
    let write = broker.write(
        &grants,
        Principal::AppProcess,
        &spelling(&path),
        &content,
        &cancelled,
    );
    let after = std::fs::read(&path).expect("observe sentinel");
    println!(
        "oversized read={:?}; write={write:?}; final_length={}",
        read.as_ref().map(Vec::len),
        after.len()
    );
    let control = broker
        .write(
            &grants,
            Principal::AppProcess,
            &spelling(&root.join("fresh")),
            b"fresh",
            &cancelled,
        )
        .and_then(|()| {
            broker.read(
                &grants,
                Principal::AppProcess,
                &spelling(&root.join("fresh")),
                &cancelled,
            )
        });
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup owned root");
    assert_eq!(control.expect("fresh allowed operation"), b"fresh");
    assert_eq!(
        read.as_ref().err().map(keld_native::fs::FsError::code),
        Some("KELD-NATIVE-004")
    );
    assert_eq!(
        write.expect_err("oversized write must reject").code(),
        "KELD-NATIVE-004"
    );
    assert_eq!(after, b"unchanged");
}

#[test]
fn zero_and_maximum_content_round_trip_exactly() {
    let root = owned_root("content-boundaries");
    let verified = manifest(&root);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    for (name, bytes) in [
        ("zero", Vec::new()),
        ("maximum", vec![0x5a; keld_native::fs::MAX_FS_CONTENT_BYTES]),
    ] {
        let path = root.join(name);
        broker
            .write(
                &verified,
                Principal::AppProcess,
                &spelling(&path),
                &bytes,
                &cancelled,
            )
            .expect("boundary write");
        let actual = broker
            .read(
                &verified,
                Principal::AppProcess,
                &spelling(&path),
                &cancelled,
            )
            .expect("boundary read");
        assert_eq!(actual, bytes, "{name} boundary bytes");
    }
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup root");
}

#[test]
fn empty_manifest_prepares_without_roots_and_stays_default_deny() {
    let root = owned_root("empty-manifest");
    let verified = verified_from_text(&root, "empty.jsonc", "{}");
    let broker = FsBroker::prepare(&verified).expect("empty broker");
    let error = broker
        .read(
            &verified,
            Principal::AppProcess,
            &spelling(&root.join("missing")),
            &AtomicBool::new(false),
        )
        .expect_err("empty manifest denies");
    assert_eq!(error.code(), "KELD-GUARD001");
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup root");
}

#[test]
fn exact_absent_leaf_can_be_created_without_following_an_alias() {
    let root = owned_root("exact-create");
    let path = root.join("created");
    let text = format!(
        r#"{{"app":{{"fs":{{"write":["{}"],"read":["{}"]}}}}}}"#,
        spelling(&path),
        spelling(&path)
    );
    let verified = verified_from_text(&root, "exact-create.jsonc", &text);
    let broker = FsBroker::prepare(&verified).expect("prepare exact broker");
    let cancelled = AtomicBool::new(false);
    broker
        .write(
            &verified,
            Principal::AppProcess,
            &spelling(&path),
            b"created",
            &cancelled,
        )
        .expect("create exact leaf");
    assert_eq!(
        broker
            .read(
                &verified,
                Principal::AppProcess,
                &spelling(&path),
                &cancelled,
            )
            .expect("read exact leaf"),
        b"created"
    );
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup root");
}

#[test]
fn volume_root_scope_reaches_an_owned_descendant() {
    let root = owned_root("volume-root");
    let path = root.join("file");
    std::fs::write(&path, b"volume-root").expect("seed file");
    #[cfg(not(windows))]
    let scope = "/**".to_owned();
    #[cfg(windows)]
    let scope = format!("{}/**", &spelling(&root)[..2]);
    let text = format!(r#"{{"app":{{"fs":{{"read":["{scope}"],"write":["{scope}"]}}}}}}"#);
    let verified = verified_from_text(&root, "volume-root.jsonc", &text);
    let broker = FsBroker::prepare(&verified).expect("prepare volume-root broker");
    let bytes = broker
        .read(
            &verified,
            Principal::AppProcess,
            &spelling(&path),
            &AtomicBool::new(false),
        )
        .expect("read volume-root descendant");
    assert_eq!(bytes, b"volume-root");
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup root");
}

#[cfg(unix)]
#[test]
fn partial_prepare_failure_releases_already_opened_roots() {
    let fixture = owned_root("prepare-unwind");
    let valid = fixture.join("valid");
    let moved = fixture.join("moved");
    let missing = fixture.join("missing");
    std::fs::create_dir(&valid).expect("valid root");
    let text = format!(
        r#"{{"app":{{"fs":{{"read":["{}/**","{}/**"]}}}}}}"#,
        spelling(&valid),
        spelling(&missing)
    );
    let verified = verified_from_text(&fixture, "unwind.jsonc", &text);
    let error = FsBroker::prepare(&verified).expect_err("second scope open fails");
    assert_eq!(error.code(), "KELD-NATIVE-008");
    std::fs::rename(&valid, &moved).expect("first provisional root was released");
    std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
}

#[test]
#[cfg(unix)]
fn retained_root_survives_ambient_path_replacement() {
    let fixture = owned_root("retained-root");
    let root = fixture.join("granted");
    let moved = fixture.join("moved");
    std::fs::create_dir(&root).expect("granted root");
    std::fs::write(root.join("sentinel"), b"retained-object").expect("seed retained object");
    let verified = manifest(&root);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);

    std::fs::rename(&root, &moved).expect("rename retained root");
    std::fs::create_dir(&root).expect("replacement root");
    std::fs::write(root.join("sentinel"), b"ambient-replacement").expect("replacement sentinel");
    let bytes = broker
        .read(
            &verified,
            Principal::AppProcess,
            &spelling(&root.join("sentinel")),
            &cancelled,
        )
        .expect("read through retained root");
    assert_eq!(bytes, b"retained-object");
    assert_eq!(
        std::fs::read(root.join("sentinel")).expect("ambient replacement"),
        b"ambient-replacement"
    );
    drop(broker);
    std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
}

#[cfg(unix)]
#[test]
fn subtree_internal_link_passes_while_exact_and_external_links_deny() {
    use std::os::unix::fs::symlink;

    let fixture = owned_root("link-policy");
    let root = fixture.join("granted");
    let outside = fixture.join("outside");
    std::fs::create_dir(&root).expect("granted root");
    std::fs::create_dir(&outside).expect("outside root");
    std::fs::write(root.join("target"), b"inside").expect("inside target");
    std::fs::write(outside.join("sentinel"), b"outside").expect("outside sentinel");
    symlink("target", root.join("internal")).expect("internal link");
    symlink(outside.join("sentinel"), root.join("external")).expect("external link");

    let subtree = manifest(&root);
    let broker = FsBroker::prepare(&subtree).expect("prepare subtree");
    let cancelled = AtomicBool::new(false);
    assert_eq!(
        broker
            .read(
                &subtree,
                Principal::AppProcess,
                &spelling(&root.join("internal")),
                &cancelled,
            )
            .expect("internal link read"),
        b"inside"
    );
    let escaped = broker
        .write(
            &subtree,
            Principal::AppProcess,
            &spelling(&root.join("external")),
            b"changed",
            &cancelled,
        )
        .expect_err("external link must deny");
    assert_eq!(escaped.code(), "KELD-NATIVE-002");
    assert_eq!(
        std::fs::read(outside.join("sentinel")).expect("outside unchanged"),
        b"outside"
    );
    drop(broker);

    let exact_text = format!(
        r#"{{"app":{{"fs":{{"read":["{}"]}}}}}}"#,
        spelling(&root.join("internal"))
    );
    let exact = verified_from_text(&fixture, "exact.jsonc", &exact_text);
    let exact_broker = FsBroker::prepare(&exact).expect("prepare exact");
    let denied = exact_broker
        .read(
            &exact,
            Principal::AppProcess,
            &spelling(&root.join("internal")),
            &cancelled,
        )
        .expect_err("exact final alias must deny");
    assert_eq!(denied.code(), "KELD-NATIVE-002");
    drop(exact_broker);
    std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
}

#[test]
fn hard_link_write_preserves_shared_object_identity() {
    let root = owned_root("hard-link");
    let first = root.join("first");
    let alias = root.join("alias");
    std::fs::write(&first, b"before").expect("seed file");
    std::fs::hard_link(&first, &alias).expect("hard link");
    #[cfg(unix)]
    let before = std::fs::metadata(&first).expect("first metadata");
    let verified = manifest(&root);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    broker
        .write(
            &verified,
            Principal::AppProcess,
            &spelling(&alias),
            b"after",
            &cancelled,
        )
        .expect("in-place hard-link write");
    assert_eq!(std::fs::read(&first).expect("first bytes"), b"after");
    assert_eq!(std::fs::read(&alias).expect("alias bytes"), b"after");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let after = std::fs::metadata(&alias).expect("alias metadata");
        assert_eq!(before.ino(), after.ino(), "write must not replace inode");
    }
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup root");
}

#[test]
fn cancellation_snapshot_and_request_shape_precede_effects() {
    let root = owned_root("precedence");
    let path = root.join("sentinel");
    std::fs::write(&path, b"unchanged").expect("seed sentinel");
    let verified = manifest(&root);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(true);
    let error = broker
        .write(
            &verified,
            Principal::AppProcess,
            &spelling(&path),
            b"changed",
            &cancelled,
        )
        .expect_err("pre-set cancellation");
    assert_eq!(error.code(), "KELD-NATIVE-006");
    assert_eq!(std::fs::read(&path).expect("sentinel"), b"unchanged");

    let different_text = format!(
        r#"{{"app":{{"fs":{{"read":["{0}/**"],"write":["{0}/**"]}}}},"audit":true}}"#,
        spelling(&root)
    );
    let different = verified_from_text(&root, "different.jsonc", &different_text);
    let oversized_path = format!("/{}", "x".repeat(4097));
    let mismatch = broker
        .read(
            &different,
            Principal::AppProcess,
            &oversized_path,
            &AtomicBool::new(false),
        )
        .expect_err("snapshot mismatch precedes path validation");
    assert_eq!(mismatch.code(), "KELD-NATIVE-008");
    let shape = broker
        .read(
            &verified,
            Principal::AppProcess,
            &oversized_path,
            &AtomicBool::new(false),
        )
        .expect_err("same snapshot reaches path validation");
    assert_eq!(shape.code(), "KELD-NATIVE-004");
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup root");
}

#[test]
fn directories_are_rejected_without_content_io_and_control_still_passes() {
    let root = owned_root("directory");
    let directory = root.join("not-a-file");
    std::fs::create_dir(&directory).expect("directory target");
    let verified = manifest(&root);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    let error = broker
        .read(
            &verified,
            Principal::AppProcess,
            &spelling(&directory),
            &cancelled,
        )
        .expect_err("directory must reject");
    assert_eq!(error.code(), "KELD-NATIVE-003");
    #[cfg(unix)]
    {
        let socket = root.join("socket");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("unix socket");
        let special = broker
            .read(
                &verified,
                Principal::AppProcess,
                &spelling(&socket),
                &cancelled,
            )
            .expect_err("socket must reject without a content read");
        assert_eq!(special.code(), "KELD-NATIVE-003");
        drop(listener);
        std::fs::remove_file(&socket).expect("remove socket");
    }
    broker
        .write(
            &verified,
            Principal::AppProcess,
            &spelling(&root.join("fresh")),
            b"fresh",
            &cancelled,
        )
        .expect("fresh operation");
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup root");
}

#[cfg(unix)]
#[test]
fn post_expansion_component_limit_passes_256_and_denies_257() {
    use std::os::unix::fs::symlink;

    let root = owned_root("components");
    std::fs::write(root.join("file"), b"boundary").expect("boundary file");
    let pass_target = format!("{}file", "./".repeat(254));
    let deny_target = format!("{}file", "./".repeat(255));
    symlink(&pass_target, root.join("pass")).expect("pass link");
    symlink(&deny_target, root.join("deny")).expect("deny link");
    let verified = manifest(&root);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    let pass = broker.read(
        &verified,
        Principal::AppProcess,
        &spelling(&root.join("pass")),
        &cancelled,
    );
    assert_eq!(pass.expect("256 processed components"), b"boundary");
    let denied = broker
        .read(
            &verified,
            Principal::AppProcess,
            &spelling(&root.join("deny")),
            &cancelled,
        )
        .expect_err("257 processed components");
    assert_eq!(denied.code(), "KELD-NATIVE-004");
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup root");
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
    let broker = FsBroker::prepare(&grants).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    let denied = broker.read(
        &grants,
        Principal::AppProcess,
        &spelling(&outside.join("sentinel")),
        &cancelled,
    );
    let read = broker.read(
        &grants,
        Principal::AppProcess,
        &spelling(&link.join("sentinel")),
        &cancelled,
    );
    let write = broker.write(
        &grants,
        Principal::AppProcess,
        &spelling(&link.join("sentinel")),
        b"escaped-write",
        &cancelled,
    );
    let outside_after = std::fs::read(outside.join("sentinel")).expect("outside oracle");
    let inside_after = std::fs::read(inside.join("sentinel")).expect("inside oracle");
    println!(
        "direct={denied:?}; junction_read={read:?}; junction_write={write:?}; outside={outside_after:?}; inside={inside_after:?}"
    );
    let control = broker.read(
        &grants,
        Principal::AppProcess,
        &spelling(&inside.join("sentinel")),
        &cancelled,
    );
    drop(broker);
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
    let broker = FsBroker::prepare(&grants).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    let result = broker.read(
        &grants,
        Principal::AppProcess,
        &spelling(&root.join("NUL")),
        &cancelled,
    );
    println!("reserved NUL read={result:?}");
    let control = broker
        .write(
            &grants,
            Principal::AppProcess,
            &spelling(&root.join("fresh")),
            b"fresh",
            &cancelled,
        )
        .and_then(|()| {
            broker.read(
                &grants,
                Principal::AppProcess,
                &spelling(&root.join("fresh")),
                &cancelled,
            )
        });
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup owned root");
    assert_eq!(control.expect("fresh allowed operation"), b"fresh");
    assert!(
        result.is_err(),
        "reserved device cannot return successful file bytes"
    );
}
