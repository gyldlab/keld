#[test]
#[cfg(target_os = "macos")]
fn fs_quiesce_refuses_new_admission_and_drains_existing_lease() {
    use sha2::{Digest as _, Sha256};

    let temp = tempfile::tempdir().expect("FS quiesce root");
    let manifest_text = "{}\n";
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, manifest_text).expect("write empty manifest");
    let digest: [u8; 32] = Sha256::digest(manifest_text.as_bytes()).into();
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open empty manifest"),
        manifest_path,
        digest,
    )
    .expect("verify empty manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare empty FS broker");

    let admitted = snapshot
        .fs
        .admit()
        .expect("first FS admission")
        .expect("session initially accepts");
    snapshot.fs.quiesce().expect("publish FS quiescing");
    assert!(
        snapshot
            .fs
            .admit()
            .expect("post-quiesce admission check")
            .is_none(),
        "new FS handler entered after quiescing"
    );
    assert_eq!(
        snapshot.fs.state.lock().expect("FS state").in_flight,
        1,
        "pre-admitted lease disappeared before its terminal outcome"
    );
    drop(admitted);
    snapshot.fs.drain().expect("drain admitted FS work");
    assert_eq!(
        snapshot.fs.state.lock().expect("FS state").in_flight,
        0,
        "drain returned before the admitted lease terminated"
    );
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn dequeued_fs_item_losing_quiesce_never_enters_handler() {
    let temp = tempfile::tempdir().expect("paused FS entry root");
    let manifest_text = "{}\n";
    let manifest_path = temp.path().join(PERMISSIONS_FILE);
    fs::write(&manifest_path, manifest_text).expect("write paused FS manifest");
    // SHA-256 of the exact static fixture bytes `{}\n`. Keep the Linux
    // lifecycle regression independent of the macOS-only sha2 dependency.
    let digest = [
        0xca, 0x3d, 0x16, 0x3b, 0xab, 0x05, 0x53, 0x81, 0x82, 0x72, 0x26, 0x14, 0x05, 0x68, 0xf3,
        0xbe, 0xf7, 0xea, 0xac, 0x18, 0x7c, 0xeb, 0xd7, 0x68, 0x78, 0xe0, 0xb6, 0x3e, 0x9e, 0x44,
        0x23, 0x56,
    ];
    let verified = load_verified_manifest(
        File::open(&manifest_path).expect("open paused FS manifest"),
        manifest_path,
        digest,
    )
    .expect("verify paused FS manifest");
    let snapshot = GuardSnapshot::prepare(verified).expect("prepare paused FS broker");
    let admitted = snapshot
        .fs
        .admit()
        .expect("paused FS admission")
        .expect("session initially accepts");
    let invoked = Arc::new(AtomicBool::new(false));
    let invoked_in_worker = Arc::clone(&invoked);
    let (dequeued_tx, dequeued_rx) = mpsc::sync_channel(1);
    let (enter_tx, enter_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        dequeued_tx.send(()).expect("report dequeued FS item");
        enter_rx.recv().expect("release paused handler entry");
        admitted
            .handle_with(|| invoked_in_worker.store(true, Ordering::Release))
            .expect("attempt paused handler entry")
            .is_none()
    });

    dequeued_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("worker dequeued admitted FS item");
    snapshot.fs.quiesce().expect("publish FS quiescence");
    enter_tx.send(()).expect("release handler-entry attempt");
    assert!(worker.join().expect("paused FS worker joins"));
    assert!(
        !invoked.load(Ordering::Acquire),
        "dequeued item invoked its handler after quiescence publication"
    );
    snapshot.fs.drain().expect("drain rejected FS item");
}

#[test]
#[cfg(target_os = "macos")]
fn guard_snapshot_drops_only_after_ordered_cleanup() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join("index.html"), "<p>guarded</p>\n").expect("renderer");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");
    let ValidatedBootSelection {
        schema: _,
        app,
        permissions_file,
        permissions_digest,
    } = validate_from_root(&root).expect("validated selection");
    let verified = load_verified_manifest(
        permissions_file,
        root.join(PERMISSIONS_FILE),
        permissions_digest,
    )
    .expect("verified manifest");
    drop(app);
    let dropped = Arc::new(AtomicBool::new(false));
    let mut snapshot = GuardSnapshot::prepare(verified).expect("prepare guarded FS broker");
    snapshot.drop_observer = Some(Arc::clone(&dropped));

    let digest = finish_guarded_session(Some(&snapshot), |live| {
        assert!(
            !dropped.load(Ordering::Acquire),
            "snapshot dropped before cleanup"
        );
        live.expect("guarded cleanup receives the snapshot")
            .verified
            .verified_sha256()
    });

    assert_eq!(digest, permissions_digest);
    assert!(
        !dropped.load(Ordering::Acquire),
        "borrowed cleanup helper destroyed the outer session owner"
    );
    drop(snapshot);
    assert!(
        dropped.load(Ordering::Acquire),
        "outer session owner did not destroy the snapshot after cleanup"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn boot_sidecar_symlink_is_rejected_before_target_resolution() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    let outside = temp.path().join("outside-boot.json");
    fs::write(&outside, valid_boot("src/main.ts", "index.html")).expect("outside boot");
    symlink(&outside, root.join("keld.boot.json")).expect("boot symlink");

    let error = must_err(validate_from_root(&root), "boot symlink must fail");
    assert!(error.to_string().contains("boot descriptor"), "{error}");
}

#[test]
#[cfg(target_os = "macos")]
fn descriptor_limit_does_not_narrow_renderer_size() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("mode");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    let renderer = vec![b'x'; MAX_BOOT_BYTES + 1];
    fs::write(root.join("index.html"), &renderer).expect("large renderer");
    fs::write(root.join("keld.permissions.jsonc"), b"{}\n").expect("permissions");
    fs::write(
        root.join("keld.boot.json"),
        valid_boot("main.ts", "index.html"),
    )
    .expect("boot");

    let selection = validate_from_root(&root).expect("large renderer is valid");
    assert!(matches!(
        &selection.app.initial_window,
        ValidatedInitialWindow::Renderer(bytes) if bytes == &renderer
    ));
}

#[test]
#[cfg(target_os = "macos")]
fn dev_lease_reader_is_nonblocking_cloexec_and_read_only() {
    let (reader, writer) = nix::unistd::pipe().expect("lease pipe");
    configure_dev_lease_fd(&reader).expect("configure lease reader");
    let descriptor = FdFlag::from_bits_truncate(
        fcntl(&reader, FcntlArg::F_GETFD).expect("lease descriptor flags"),
    );
    let status =
        OFlag::from_bits_truncate(fcntl(&reader, FcntlArg::F_GETFL).expect("lease status flags"));
    assert!(descriptor.contains(FdFlag::FD_CLOEXEC));
    assert!(status.contains(OFlag::O_NONBLOCK));
    assert!((status & OFlag::O_ACCMODE).is_empty());

    let error = configure_dev_lease_fd(&writer).expect_err("writer is not a lease reader");
    assert!(error.to_string().contains("read-only end"), "{error}");
}

#[test]
#[cfg(target_os = "macos")]
fn dev_lease_data_drain_yields_after_a_fixed_work_budget() {
    struct EndlessReader {
        reads: usize,
    }

    impl std::io::Read for EndlessReader {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            bytes.fill(b'x');
            Ok(bytes.len())
        }
    }

    let mut reader = EndlessReader { reads: 0 };
    assert!(!poll_dev_lease_reader(&mut reader).expect("bounded data drain"));
    assert_eq!(reader.reads, DEV_LEASE_DRAIN_READS);
}

#[test]
#[cfg(target_os = "macos")]
fn stage_directory_mode_is_mandatory() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("stage");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o1700)).expect("wrong mode");
    let mode_error = must_err(validate_from_root(&root), "mode must be exact 0700");
    assert!(mode_error.to_string().contains("0o700"), "{mode_error}");
}
