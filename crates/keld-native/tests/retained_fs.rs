//! KEL-130 regression oracles use owned OS resources and literal contract limits.
#![allow(clippy::expect_used)] // Integration assertions and fixture setup.

use keld_guard::Principal;
use keld_guard::verified_manifest::{VerifiedManifest, load_verified_manifest};
use keld_native::fs::FsBroker;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[cfg(windows)]
#[path = "support/windows_handle_census.rs"]
mod windows_handle_census;
#[cfg(windows)]
use windows_handle_census::owner_handle_count;

fn owned_root_under(base: &Path, case: &str) -> PathBuf {
    let root = base.join(format!(
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

fn owned_root(case: &str) -> PathBuf {
    owned_root_under(&std::env::temp_dir(), case)
}

#[cfg(target_os = "linux")]
fn volume_root_owned_root(case: &str) -> PathBuf {
    use std::os::unix::fs::MetadataExt;

    let base = Path::new("/var/tmp");
    assert_eq!(
        std::fs::metadata(base)
            .expect("fixture base metadata")
            .dev(),
        std::fs::metadata("/").expect("volume root metadata").dev(),
        "volume-root positive requires a writable fixture on the retained root device"
    );
    owned_root_under(base, case)
}

#[cfg(not(target_os = "linux"))]
fn volume_root_owned_root(case: &str) -> PathBuf {
    owned_root(case)
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

#[cfg(unix)]
fn file_symlink(target: impl AsRef<Path>, link: impl AsRef<Path>) {
    std::os::unix::fs::symlink(target, link).expect("create file symlink");
}

#[cfg(windows)]
fn file_symlink(target: impl AsRef<Path>, link: impl AsRef<Path>) {
    std::os::windows::fs::symlink_file(target, link).expect("create file symlink");
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
    let root = volume_root_owned_root("volume-root");
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
fn owner_handle_count() -> usize {
    std::fs::read_dir("/dev/fd")
        .expect("open owner handle census")
        .count()
}

#[cfg(unix)]
#[test]
fn partial_prepare_handle_census_child() {
    if std::env::var_os("KELD_KEL130_HANDLE_CENSUS_CHILD").is_none() {
        return;
    }
    let fixture = owned_root("prepare-unwind");
    let valid = fixture.join("valid");
    let missing = fixture.join("missing");
    std::fs::create_dir(&valid).expect("valid root");
    let text = format!(
        r#"{{"app":{{"fs":{{"read":["{}/**","{}/**"]}}}}}}"#,
        spelling(&valid),
        spelling(&missing)
    );
    let verified = verified_from_text(&fixture, "unwind.jsonc", &text);
    let baseline = owner_handle_count();
    let deliberate = std::fs::File::open(&valid).expect("deliberate retained directory handle");
    assert_eq!(
        owner_handle_count(),
        baseline + 1,
        "census must detect the deliberate retained-directory-handle mutation"
    );
    drop(deliberate);
    assert_eq!(owner_handle_count(), baseline, "negative control cleanup");
    let error = FsBroker::prepare(&verified).expect_err("second scope open fails");
    assert_eq!(error.code(), "KELD-NATIVE-008");
    assert_eq!(
        owner_handle_count(),
        baseline,
        "partial preparation must release every provisional owner handle before returning"
    );
    std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
}

#[cfg(unix)]
#[test]
fn partial_prepare_failure_restores_isolated_owner_handle_census() {
    let output = std::process::Command::new(std::env::current_exe().expect("current test binary"))
        .args([
            "--exact",
            "partial_prepare_handle_census_child",
            "--nocapture",
        ])
        .env("KELD_KEL130_HANDLE_CENSUS_CHILD", "1")
        .output()
        .expect("run isolated handle census");
    assert!(
        output.status.success(),
        "isolated handle census failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[test]
fn partial_prepare_failure_releases_roots_for_windows_rename() {
    let fixture = owned_root("prepare-unwind-windows");
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
    std::fs::rename(&valid, &moved).expect("provisional Windows handle was released");
    std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
}

#[cfg(windows)]
#[test]
fn windows_broker_lifecycle_handle_census_child() {
    if std::env::var_os("KELD_KEL130_WINDOWS_CENSUS_CHILD").is_none() {
        return;
    }
    let root = owned_root("windows-broker-census");
    let nested = root.join("nested");
    std::fs::create_dir(&nested).expect("nested directory");
    let file = nested.join("file");
    std::fs::write(&file, b"inside").expect("seed file");
    let verified = manifest(&root);
    let baseline = owner_handle_count();

    let deliberate = std::fs::File::open(&file).expect("deliberate owner handle");
    assert!(
        owner_handle_count() > baseline,
        "census must detect a deliberate owner-process handle"
    );
    drop(deliberate);
    assert_eq!(owner_handle_count(), baseline, "negative control cleanup");

    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let prepared = owner_handle_count();
    assert!(prepared > baseline, "prepared broker must retain roots");
    assert_eq!(
        broker
            .read(
                &verified,
                Principal::AppProcess,
                &spelling(&file),
                &AtomicBool::new(false),
            )
            .expect("broker read"),
        b"inside"
    );
    assert_eq!(
        owner_handle_count(),
        prepared,
        "a returned call must release its per-call handle"
    );

    let first = std::rc::Rc::new(broker);
    let last = std::rc::Rc::clone(&first);
    drop(first);
    assert_eq!(
        owner_handle_count(),
        prepared,
        "dropping one wrapper is not broker destruction"
    );
    drop(last);
    assert_eq!(
        owner_handle_count(),
        baseline,
        "actual broker destruction restores the owner baseline"
    );

    let threaded = std::sync::Arc::new(FsBroker::prepare(&verified).expect("prepare Arc broker"));
    let clone = std::sync::Arc::clone(&threaded);
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
    let holder = std::thread::spawn(move || {
        ready_tx.send(()).expect("publish Arc readiness");
        release_rx.recv().expect("await Arc release");
        drop(clone);
    });
    ready_rx.recv().expect("Arc holder ready");
    let wrapped = owner_handle_count();
    drop(threaded);
    assert_eq!(
        owner_handle_count(),
        wrapped,
        "dropping one Arc wrapper must not release broker handles"
    );
    release_tx.send(()).expect("release Arc holder");
    holder.join().expect("join Arc holder");
    assert_eq!(
        owner_handle_count(),
        baseline,
        "last Arc owner restores the handle baseline"
    );

    let moved = root.with_extension("moved");
    std::fs::rename(&root, &moved).expect("destroyed broker releases rename");
    std::fs::remove_dir_all(&moved).expect("destroyed broker releases delete");
}

#[cfg(windows)]
#[test]
fn windows_broker_lifecycle_restores_isolated_owner_handle_census() {
    let output = std::process::Command::new(std::env::current_exe().expect("current test binary"))
        .args([
            "--exact",
            "windows_broker_lifecycle_handle_census_child",
            "--nocapture",
        ])
        .env("KELD_KEL130_WINDOWS_CENSUS_CHILD", "1")
        .output()
        .expect("run isolated Windows broker census");
    assert!(
        output.status.success(),
        "isolated Windows broker census failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[test]
fn windows_handle_inheritance_child_waits_for_parent_release() {
    use std::io::{Read as _, Write as _};
    use std::sync::mpsc;
    use std::time::Duration;

    if std::env::var_os("KELD_KEL130_INHERITANCE_CHILD").is_none() {
        return;
    }

    std::io::stdout()
        .write_all(b"KEL130_CHILD_READY\n")
        .expect("publish child readiness");
    std::io::stdout().flush().expect("flush child readiness");
    let (finished_tx, finished_rx) = mpsc::sync_channel(0);
    let watchdog = std::thread::spawn(move || {
        if finished_rx.recv_timeout(Duration::from_secs(5)).is_err() {
            std::process::exit(2);
        }
    });
    let mut input = Vec::new();
    std::io::stdin()
        .read_to_end(&mut input)
        .expect("wait for parent release");
    finished_tx.send(()).expect("disarm child watchdog");
    watchdog.join().expect("join child watchdog");
}

#[cfg(windows)]
#[test]
fn windows_broker_roots_are_not_inherited_by_a_live_child() {
    use std::io::BufRead as _;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Duration;

    let fixture = owned_root("windows-noninheritance");
    let root = fixture.join("granted");
    let moved = fixture.join("moved");
    std::fs::create_dir(&root).expect("granted root");
    let verified = manifest(&root);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");

    let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "windows_handle_inheritance_child_waits_for_parent_release",
            "--nocapture",
        ])
        .env("KELD_KEL130_INHERITANCE_CHILD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn inheritance child");
    let stdout = child.stdout.take().expect("child stdout");
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let lines = std::io::BufReader::new(stdout).lines();
        let mut ready = false;
        for line in lines {
            if !ready && matches!(line.as_deref(), Ok("KEL130_CHILD_READY")) {
                ready = true;
                let _ = ready_tx.send(true);
            }
        }
        if !ready {
            let _ = ready_tx.send(false);
        }
    });
    let ready = match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(ready) => ready,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            drop(broker);
            std::fs::remove_dir_all(&fixture).expect("cleanup timed-out fixture");
            panic!("child readiness timed out: {error}");
        }
    };
    drop(broker);
    let rename_result = std::fs::rename(&root, &moved);
    drop(child.stdin.take());
    let child_status = child.wait().expect("wait for bounded child");
    reader.join().expect("join child output reader");
    if rename_result.is_ok() {
        std::fs::remove_dir_all(&moved).expect("remove moved root");
    } else {
        std::fs::remove_dir_all(&root).expect("remove root after child exit");
    }
    std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
    assert!(ready, "child exited before readiness");
    rename_result.expect("live child did not inherit retained root handle");
    assert!(
        child_status.success(),
        "inheritance child failed: {child_status}"
    );
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

#[cfg(any(unix, windows))]
#[test]
fn subtree_internal_link_passes_while_exact_and_external_links_deny() {
    let fixture = owned_root("link-policy");
    let root = fixture.join("granted");
    let outside = fixture.join("outside");
    std::fs::create_dir(&root).expect("granted root");
    std::fs::create_dir(&outside).expect("outside root");
    std::fs::write(root.join("target"), b"inside").expect("inside target");
    std::fs::write(outside.join("sentinel"), b"outside").expect("outside sentinel");
    file_symlink("target", root.join("internal"));
    file_symlink(outside.join("sentinel"), root.join("external"));

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

#[cfg(any(unix, windows))]
#[test]
fn post_expansion_component_limit_passes_256_and_denies_257() {
    let root = owned_root("components");
    std::fs::write(root.join("file"), b"boundary").expect("boundary file");
    let pass_target = format!("{}file", "./".repeat(254));
    let deny_target = format!("{}file", "./".repeat(255));
    file_symlink(&pass_target, root.join("pass"));
    file_symlink(&deny_target, root.join("deny"));
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

#[cfg(any(unix, windows))]
#[test]
fn symlink_expansion_limit_passes_40_and_denies_41() {
    let root = owned_root("link-expansions");
    std::fs::write(root.join("file"), b"boundary").expect("boundary file");

    for (prefix, count) in [("pass", 40_usize), ("deny", 41_usize)] {
        for index in 0..count {
            let target = if index + 1 == count {
                PathBuf::from("file")
            } else {
                PathBuf::from(format!("{prefix}-{}", index + 1))
            };
            file_symlink(target, root.join(format!("{prefix}-{index}")));
        }
    }

    let verified = manifest(&root);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    assert_eq!(
        broker
            .read(
                &verified,
                Principal::AppProcess,
                &spelling(&root.join("pass-0")),
                &cancelled,
            )
            .expect("40 expanded links"),
        b"boundary"
    );
    let denied = broker
        .read(
            &verified,
            Principal::AppProcess,
            &spelling(&root.join("deny-0")),
            &cancelled,
        )
        .expect_err("41 expanded links");
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
    assert_eq!(
        result
            .expect_err("reserved device must be rejected before filesystem entry")
            .code(),
        "KELD-GUARD002"
    );
}

#[cfg(windows)]
#[test]
fn windows_request_path_bytes_accept_4096_and_reject_4097() {
    assert_eq!(keld_native::fs::MAX_FS_PATH_BYTES, 4096);
    let root = owned_root("path-byte-boundary");
    let drive_root = format!("{}/", &spelling(&root)[..2]);
    let scope = format!("{}/**", &spelling(&root)[..2]);
    let text = format!(r#"{{"app":{{"fs":{{"read":["{scope}"]}}}}}}"#);
    let verified = verified_from_text(&root, "path-boundary.jsonc", &text);
    let broker = FsBroker::prepare(&verified).expect("prepare volume broker");
    let cancelled = AtomicBool::new(false);
    let maximum = format!("{drive_root}{}", "x".repeat(4096 - drive_root.len()));
    let over = format!("{drive_root}{}", "x".repeat(4097 - drive_root.len()));
    assert_eq!(maximum.len(), 4096);
    assert_eq!(over.len(), 4097);

    let maximum_error = broker
        .read(&verified, Principal::AppProcess, &maximum, &cancelled)
        .expect_err("the synthetic maximum path does not name a fixture file");
    assert_ne!(
        maximum_error.code(),
        "KELD-NATIVE-004",
        "4096 request bytes must pass the broker's literal size boundary"
    );
    let over_error = broker
        .read(&verified, Principal::AppProcess, &over, &cancelled)
        .expect_err("4097 request bytes must reject");
    assert_eq!(over_error.code(), "KELD-NATIVE-004");

    let control_path = root.join("fresh");
    std::fs::write(&control_path, b"fresh").expect("fresh control file");
    assert_eq!(
        broker
            .read(
                &verified,
                Principal::AppProcess,
                &spelling(&control_path),
                &cancelled,
            )
            .expect("fresh allowed read"),
        b"fresh"
    );
    drop(broker);
    std::fs::remove_dir_all(&root).expect("cleanup owned root");
}

#[cfg(windows)]
#[test]
fn windows_retained_root_blocks_replacement_until_broker_destruction() {
    let fixture = owned_root("retained-root-destruction");
    let root = fixture.join("granted");
    let moved = fixture.join("moved");
    std::fs::create_dir(&root).expect("granted root");
    std::fs::write(root.join("fresh"), b"fresh").expect("fresh control file");
    let verified = manifest(&root);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);

    let rename_error = std::fs::rename(&root, &moved)
        .expect_err("retained Windows root handle must block ambient replacement");
    assert!(
        matches!(rename_error.kind(), std::io::ErrorKind::PermissionDenied)
            || matches!(rename_error.raw_os_error(), Some(5 | 32)),
        "unexpected retained-root rename failure: {rename_error}"
    );
    assert_eq!(
        broker
            .read(
                &verified,
                Principal::AppProcess,
                &spelling(&root.join("fresh")),
                &cancelled,
            )
            .expect("fresh read while root is retained"),
        b"fresh"
    );

    drop(broker);
    std::fs::rename(&root, &moved).expect("broker destruction releases root rename");
    std::fs::remove_dir_all(&moved).expect("broker destruction releases root deletion");
    std::fs::remove_dir_all(&fixture).expect("cleanup fixture");
}

/// OS facts about a file that a read, write, rename or chmod would move.
#[derive(Debug, PartialEq, Eq)]
struct OsFacts {
    len: u64,
    modified: std::time::SystemTime,
    /// `None` where the volume does not move atime on a read.
    accessed: Option<std::time::SystemTime>,
    /// `None` where the platform does not report a creation time.
    created: Option<std::time::SystemTime>,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    changed: (i64, i64),
}

impl OsFacts {
    fn of(path: &Path, atime_observable: bool) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt as _;
        let metadata = std::fs::metadata(path).expect("stat sentinel");
        Self {
            len: metadata.len(),
            modified: metadata.modified().expect("mtime"),
            accessed: atime_observable.then(|| metadata.accessed().expect("atime")),
            created: metadata.created().ok(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(unix)]
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }
}

/// A granted `<base>/scope` subtree (with `scope/sub`) and a backdated
/// `<base>/sentinel` beside it, outside the grant.
///
/// The sentinel's atime is set strictly older than its mtime. APFS and Linux
/// `relatime` then move atime on the next read, so a read becomes observable,
/// which a twin control file read directly must show. Only Windows may leave
/// atime unobservable. There the sentinel's no-read property rests on the
/// dispatch-boundary error code alone (the guard refuses before any
/// filesystem call), and length, mtime and creation time still bind writes.
struct OutsideFixture {
    base: PathBuf,
    scope: PathBuf,
    sentinel: PathBuf,
    atime_observable: bool,
    before: OsFacts,
}

impl OutsideFixture {
    /// `prepare` runs after the files exist and before they are backdated, so
    /// links and canonical-target checks it makes cannot move the baseline.
    fn new(case: &str, prepare: impl FnOnce(&Self)) -> Self {
        use std::time::{Duration, UNIX_EPOCH};

        let base = owned_root(case);
        let scope = base.join("scope");
        std::fs::create_dir_all(scope.join("sub")).expect("granted scope");
        let mut fixture = Self {
            sentinel: base.join("sentinel"),
            base,
            scope,
            atime_observable: false,
            before: OsFacts {
                len: 0,
                modified: UNIX_EPOCH,
                accessed: None,
                created: None,
                #[cfg(unix)]
                inode: 0,
                #[cfg(unix)]
                changed: (0, 0),
            },
        };
        let control = fixture.base.join("atime-control");
        for file in [&fixture.sentinel, &control] {
            std::fs::write(file, b"outside").expect("seed file outside the scope");
        }
        prepare(&fixture);
        let accessed = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        let modified = accessed + Duration::from_secs(100);
        for file in [&fixture.sentinel, &control] {
            std::fs::OpenOptions::new()
                .write(true)
                .open(file)
                .expect("open to backdate")
                .set_times(
                    std::fs::FileTimes::new()
                        .set_accessed(accessed)
                        .set_modified(modified),
                )
                .expect("backdate times");
        }
        std::fs::read(&control).expect("read the atime control directly");
        fixture.atime_observable = std::fs::metadata(&control)
            .expect("control metadata")
            .accessed()
            .expect("control atime")
            != accessed;
        assert!(
            fixture.atime_observable || cfg!(windows),
            "a direct read did not move atime on this Unix volume, so the no-read oracle would be vacuous"
        );
        fixture.before = OsFacts::of(&fixture.sentinel, fixture.atime_observable);
        assert_eq!(
            fixture.before.accessed,
            fixture.atime_observable.then_some(accessed),
            "fixture atime did not hold"
        );
        fixture
    }

    fn in_scope(&self, tail: &str) -> String {
        format!("{}{tail}", spelling(&self.scope))
    }

    /// Nothing outside the scope changed: the sentinel's OS facts and the set
    /// of names beside the scope.
    fn assert_outside_untouched(&self, case: &str) {
        let atime = if self.atime_observable {
            "compared"
        } else {
            "unobservable on this volume, so no-read rests on the error code"
        };
        assert_eq!(
            OsFacts::of(&self.sentinel, self.atime_observable),
            self.before,
            "{case}: the sentinel outside the scope was touched (atime {atime})"
        );
        let mut names: Vec<String> = std::fs::read_dir(&self.base)
            .expect("list base")
            .map(|entry| {
                entry
                    .expect("base entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["atime-control", "scope", "sentinel"],
            "{case}: a name appeared outside the scope"
        );
    }

    /// Checks the sentinel's bytes last (reading them moves atime) and cleans up.
    fn finish(self) {
        let bytes = std::fs::read(&self.sentinel).expect("observe sentinel bytes");
        std::fs::remove_dir_all(&self.base).expect("cleanup owned root");
        assert_eq!(bytes, b"outside");
    }
}

/// Where the guarded path refuses a request.
#[derive(Clone, Copy, Debug)]
enum Refusal {
    /// `dispatch_privileged`'s normalized-path gate, before the grant decision
    /// is used or the broker walks anything (`KELD-GUARD002`).
    Dispatch,
    /// `evaluate`'s grant scope: a normalized path outside every grant
    /// (`KELD-GUARD002`).
    GrantScope,
    /// A literal in-scope name that does not exist (`KELD-NATIVE-001`).
    MissingInside,
}

fn assert_refused(
    refusal: Refusal,
    operation: &str,
    result: Result<(), keld_native::fs::FsError>,
    requested: &str,
) {
    use keld_guard::DenyReason;
    use keld_native::fs::FsError;

    let error = result.expect_err("a request that climbs out must be refused");
    let refused = match refusal {
        Refusal::Dispatch | Refusal::GrantScope => {
            error.code() == "KELD-GUARD002"
                && matches!(
                    &error,
                    FsError::Denied(DenyReason::OutOfScope {
                        capability,
                        scope,
                        requested: denied,
                        ..
                    }) if capability == operation
                        && denied == requested
                        && (scope == "absolute normalized filesystem request")
                            == matches!(refusal, Refusal::Dispatch)
                )
        }
        Refusal::MissingInside => {
            error.code() == "KELD-NATIVE-001"
                && matches!(&error, FsError::Io(source) if source.kind() == std::io::ErrorKind::NotFound)
        }
    };
    assert!(
        refused,
        "{operation} {requested}: expected {refusal:?}, got {error:?}"
    );
}

fn canonical(path: &str) -> PathBuf {
    std::fs::canonicalize(path).expect("the OS resolves the attack path")
}

/// #673: the OS-level `..` traversal proof that hosted CI runs. A request that
/// climbs out of a granted subtree with a `..` segment, and demonstrably names
/// the sentinel beside it (its canonical path is the sentinel's), is refused
/// at the dispatch boundary (`KELD-GUARD002`) for read, overwrite and create.
/// Nothing outside the scope changes (see [`OutsideFixture`]), and the same
/// broker still serves in-scope I/O. The ignored macOS device acceptance
/// (`path-parent-component`) remains the zero-I/O counter proof.
#[test]
fn parent_component_inside_a_granted_scope_never_reaches_the_outside() {
    let fixture = OutsideFixture::new("parent-component", |fixture| {
        assert_eq!(
            canonical(&fixture.in_scope("/../sentinel")),
            canonical(&spelling(&fixture.sentinel)),
            "the attack must name the sentinel"
        );
    });
    let verified = manifest(&fixture.scope);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    let app = Principal::AppProcess;
    let attack = fixture.in_scope("/../sentinel");
    let create = fixture.in_scope("/../created");
    let read = broker.read(&verified, app, &attack, &cancelled);
    let overwrite = broker.write(&verified, app, &attack, b"overwritten", &cancelled);
    let created = broker.write(&verified, app, &create, b"created", &cancelled);
    fixture.assert_outside_untouched("parent-component");
    let inside = fixture.in_scope("/inside");
    let live = broker
        .write(&verified, app, &inside, b"inside", &cancelled)
        .and_then(|()| broker.read(&verified, app, &inside, &cancelled));
    drop(broker);

    assert_refused(Refusal::Dispatch, "fs.read", read.map(|_| ()), &attack);
    assert_refused(Refusal::Dispatch, "fs.write", overwrite, &attack);
    assert_refused(Refusal::Dispatch, "fs.write", created, &create);
    assert_eq!(live.expect("in-scope write then read"), b"inside");
    fixture.finish();
}

/// One attack form: name, request path, expected refusal, and (for forms the
/// OS resolves) the name beside the scope it reaches (`""` is the base itself).
type Climb = (&'static str, String, Refusal, Option<&'static str>);

/// Every spelling of a climb out of the scope that the #673 review tried.
fn climbing_spellings(fixture: &OutsideFixture) -> Vec<Climb> {
    use Refusal::{Dispatch, GrantScope, MissingInside};

    // Windows rejects a component ending in dot or space before dispatch; on
    // Unix those are literal in-scope names that do not exist.
    let trailing_dot_or_space = if cfg!(windows) {
        Dispatch
    } else {
        MissingInside
    };
    let scope = |tail: &str| fixture.in_scope(tail);
    let native = fixture.scope.display().to_string();
    let without_root = spelling(&fixture.scope).trim_start_matches('/').to_owned();
    vec![
        ("parent", scope("/../sentinel"), Dispatch, Some("sentinel")),
        (
            "depth-2",
            scope("/sub/../../sentinel"),
            Dispatch,
            Some("sentinel"),
        ),
        (
            "dot-then-parent",
            scope("/./../sentinel"),
            Dispatch,
            Some("sentinel"),
        ),
        (
            "double-slash-parent",
            scope("//../sentinel"),
            Dispatch,
            Some("sentinel"),
        ),
        ("trailing-parent", scope("/.."), Dispatch, Some("")),
        ("trailing-depth-2", scope("/sub/../.."), Dispatch, Some("")),
        (
            "re-enter-then-escape",
            scope("/../scope/../sentinel"),
            Dispatch,
            Some("sentinel"),
        ),
        ("backslash", scope("\\..\\sentinel"), Dispatch, None),
        (
            "slash-then-backslash",
            scope("/..\\sentinel"),
            Dispatch,
            None,
        ),
        (
            "verbatim-prefix",
            format!("\\\\?\\{native}\\..\\sentinel"),
            Dispatch,
            None,
        ),
        (
            "slash-verbatim-prefix",
            format!("//?/{without_root}/../sentinel"),
            Dispatch,
            None,
        ),
        (
            "percent-encoded",
            scope("/%2e%2e/sentinel"),
            MissingInside,
            None,
        ),
        (
            "absolute-outside",
            spelling(&fixture.sentinel),
            GrantScope,
            Some("sentinel"),
        ),
        (
            "trailing-space-parent",
            scope("/.. /sentinel"),
            trailing_dot_or_space,
            None,
        ),
        (
            "triple-dot",
            scope("/.../sentinel"),
            trailing_dot_or_space,
            None,
        ),
    ]
}

/// #673 review: every climbing spelling is refused, for read and overwrite,
/// with nothing outside the scope changed. On Unix each form the OS resolves
/// is first shown to reach the sentinel or the directory holding it.
#[test]
fn every_climbing_spelling_is_refused_and_never_reaches_the_outside() {
    let mut rows = Vec::new();
    let fixture = OutsideFixture::new("climbing-spellings", |fixture| {
        rows = climbing_spellings(fixture);
        #[cfg(unix)]
        for (case, path, _, target) in &rows {
            if let Some(leaf) = target {
                assert_eq!(
                    canonical(path),
                    canonical(&spelling(&fixture.base.join(leaf))),
                    "{case}: the OS does not resolve the attack to its target"
                );
            }
        }
    });
    assert_eq!(rows.len(), 15, "one row per reviewed attack form");
    let verified = manifest(&fixture.scope);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    let app = Principal::AppProcess;
    for (case, path, refusal, _) in &rows {
        let read = broker.read(&verified, app, path, &cancelled).map(|_| ());
        let write = broker.write(&verified, app, path, b"overwritten", &cancelled);
        fixture.assert_outside_untouched(case);
        assert_refused(*refusal, "fs.read", read, path);
        assert_refused(*refusal, "fs.write", write, path);
    }
    drop(broker);
    fixture.finish();
}

#[cfg(unix)]
fn dir_symlink(target: impl AsRef<Path>, link: impl AsRef<Path>) {
    std::os::unix::fs::symlink(target, link).expect("create directory symlink");
}

#[cfg(windows)]
fn dir_symlink(target: impl AsRef<Path>, link: impl AsRef<Path>) {
    std::os::windows::fs::symlink_dir(target, link).expect("create directory symlink");
}

/// #673 review: relative links inside the scope that climb out are never
/// followed. `up -> ../sentinel` and `updir -> ..` (each shown to resolve to
/// the sentinel) are refused with `KELD-NATIVE-002` for read and overwrite,
/// and nothing outside the scope changes. A permanent bypass fixture
/// (`crates/keld-guard/AGENTS.md`).
#[test]
fn relative_links_that_climb_out_of_the_scope_are_never_followed() {
    let up_target = if cfg!(windows) {
        "..\\sentinel"
    } else {
        "../sentinel"
    };
    let fixture = OutsideFixture::new("climbing-links", |fixture| {
        file_symlink(up_target, fixture.scope.join("up"));
        dir_symlink("..", fixture.scope.join("updir"));
        for tail in ["/up", "/updir/sentinel"] {
            assert_eq!(
                canonical(&fixture.in_scope(tail)),
                canonical(&spelling(&fixture.sentinel)),
                "{tail} must resolve to the sentinel"
            );
        }
    });
    let verified = manifest(&fixture.scope);
    let broker = FsBroker::prepare(&verified).expect("prepare broker");
    let cancelled = AtomicBool::new(false);
    let app = Principal::AppProcess;
    for tail in ["/up", "/updir/sentinel"] {
        let path = fixture.in_scope(tail);
        let read = broker.read(&verified, app, &path, &cancelled);
        let write = broker.write(&verified, app, &path, b"overwritten", &cancelled);
        fixture.assert_outside_untouched(tail);
        for (operation, result) in [("fs.read", read.map(|_| ())), ("fs.write", write)] {
            let error = result.expect_err("a climbing link must be refused");
            // The walker's own escape rule refuses the link; its later
            // empty-stack guard would also say KELD-NATIVE-002, so the
            // detail pins which rule fired.
            assert!(
                matches!(
                    &error,
                    keld_native::fs::FsError::ResolvedOutOfScope { detail, .. }
                        if detail == "relative link escaped above the retained root"
                ),
                "{operation} {tail}: expected the walker's escape refusal, got {error:?}"
            );
            assert_eq!(error.code(), "KELD-NATIVE-002", "{operation} {tail}");
        }
    }
    drop(broker);
    fixture.finish();
}
