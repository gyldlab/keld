/// Authenticode FFI names that only the `keld-guard` owner may use. Split
/// literals keep these needles from matching this test's own source.
const AUTHENTICODE_FFI_NEEDLES: [&str; 8] = [
    concat!("WinTrust", "::"),
    concat!("Cryptography", "::"),
    concat!("WinVerify", "Trust"),
    concat!("WT", "Helper"),
    concat!("CryptDecode", "Object"),
    concat!("CryptEncode", "Object"),
    concat!("CryptQuery", "Object"),
    concat!("Crypt", "Msg"),
];

fn authenticode_ffi_needles_in(source: &str) -> Vec<&'static str> {
    AUTHENTICODE_FFI_NEEDLES
        .into_iter()
        .filter(|needle| source.contains(needle))
        .collect()
}

/// The KEL-135 Authenticode FFI has one owner in `keld-guard` (KEL-270 D4).
#[test]
fn keld_core_source_holds_no_authenticode_ffi() {
    let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut scanned = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("read a keld-core source directory") {
            let path = entry.expect("read a keld-core source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = fs::read_to_string(&path).expect("read a keld-core source file");
                scanned += 1;
                let found = authenticode_ffi_needles_in(&source);
                assert!(
                    found.is_empty(),
                    "{} names {found:?}; call the keld-guard Authenticode owner instead",
                    path.display()
                );
            }
        }
    }
    assert!(scanned > 1, "no keld-core source files were scanned");
}

#[test]
fn authenticode_ffi_scan_flags_each_needle() {
    assert_eq!(
        authenticode_ffi_needles_in("fn clean() {}"),
        [] as [&str; 0]
    );
    for needle in AUTHENTICODE_FFI_NEEDLES {
        let planted = format!("fn planted() {{ let _ = {needle}; }}");
        assert_eq!(authenticode_ffi_needles_in(&planted), [needle]);
    }
}

#[test]
#[cfg(windows)]
fn current_test_executable_is_rejected_as_a_release_carrier() {
    let executable = current_windows_executable().expect("test executable path");
    let error = verified_windows_image(&executable)
        .expect_err("the test executable is not an approved signed Keld package carrier");
    assert_eq!(error.code(), "KELD-WV-009");
    // The running image opened as a pinned trust image and was refused by its owner.
    assert!(
        error
            .to_string()
            .contains("rejected the current executable with status"),
        "{error}"
    );
}

/// Verifies the specified host carrier, or this signed libtest for older fixtures.
#[test]
#[ignore = "requires a trusted Authenticode-signed fixture executable"]
#[cfg(windows)]
fn kel135_signed_package_acceptance_fixture() {
    let carrier = std::env::var_os("KELD_KEL135_CARRIER_UNDER_TEST")
        .map_or_else(current_windows_executable, |path| Ok(PathBuf::from(path)))
        .expect("the signed fixture carrier path");
    let (_verified, identity) = verified_windows_image(&carrier)
        .expect("the signed fixture must produce a verified Windows identity");
    let app_id = identity.app_id.clone();
    let publisher_scope = identity.publisher_scope.iter().fold(
        String::with_capacity(identity.publisher_scope.len() * 2),
        |mut text, byte| {
            use std::fmt::Write as _;
            write!(&mut text, "{byte:02x}").expect("format publisher scope");
            text
        },
    );
    let profile_namespace = identity.profile_identity.namespace_segment();
    let mode = select_windows_profile_mode(Some(identity), false)
        .expect("the verified signed fixture must select a persistent profile");
    assert!(matches!(mode, WindowsProfileMode::Persistent(_)));
    println!(
        "KELD_KEL135_SIGNED_IDENTITY app_id={app_id} publisher_scope={publisher_scope} profile_namespace={profile_namespace}"
    );
}

/// Proves the signed acceptance fixture binary includes the isolated-profile-root build.
#[test]
#[ignore = "probes only the debug KEL-135 profile test-root feature"]
#[cfg(all(windows, feature = "kel135-profile-test-root"))]
fn kel135_profile_test_root_probe_fixture() {
    let root = std::env::var_os("KELD_PROFILE_TEST_ROOT")
        .expect("the profile test-root probe needs the requested path");
    println!("KELD_PROFILE_TEST_ROOT_FEATURE {}", root.to_string_lossy());
}

/// Runs only from a deliberately signed copy of this libtest binary.
#[test]
#[ignore = "requires a trusted Authenticode-signed fixture executable"]
#[cfg(windows)]
fn kel135_signed_package_purge_acceptance_fixture() {
    let executable = current_windows_executable().expect("signed fixture path");
    let (_verified, identity) = verified_windows_image(&executable)
        .expect("the signed fixture must produce a verified Windows identity");
    let profile_namespace = identity.profile_identity.namespace_segment();
    WebView2Engine::purge_persistent_profile(identity.profile_identity)
        .expect("the signed fixture must purge its authenticated idle profile");
    println!("KELD_KEL135_SIGNED_PURGE profile_namespace={profile_namespace}");
}

#[test]
#[cfg(windows)]
fn quit_peer_close_wait_is_bounded_for_a_non_closing_client() {
    let (mut server, client) = primary_test_stream_pair();
    server
        .set_app_link_read_deadline(Some(Duration::from_millis(10)))
        .expect("server poll deadline");

    let started = Instant::now();
    let error = await_windows_quit_peer_close_until(
        &mut server,
        Instant::now() + Duration::from_millis(50),
    )
    .expect_err("non-closing client must hit the peer-close deadline");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "peer-close deadline did not bound the wait"
    );
    assert!(error.to_string().contains("did not close"), "{error}");
    drop(client);
}

#[test]
#[cfg(windows)]
fn dev_stage_cleanup_retries_only_transient_final_directory_sharing() {
    let temp = tempfile::tempdir().expect("cleanup fixture root");
    let root = temp.path().join("stage");
    fs::create_dir(&root).expect("cleanup stage");
    fs::write(root.join("entry"), b"fixture").expect("cleanup stage entry");

    let mut blocker = ReapedTestChild(
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Write-Output (Get-Location).Path; $null = [Console]::In.ReadLine()",
            ])
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn final-directory holder"),
    );
    let mut blocker_stdin = blocker.0.stdin.take();
    let mut ready = String::new();
    BufReader::new(blocker.0.stdout.take().expect("holder readiness pipe"))
        .read_line(&mut ready)
        .expect("holder readiness");
    let observed_ready = Path::new(ready.trim_end())
        .canonicalize()
        .expect("canonical holder working directory");
    let expected_root = root.canonicalize().expect("canonical cleanup stage");
    assert_eq!(observed_ready, expected_root);

    let bare_error = fs::remove_dir_all(&root)
        .expect_err("bare Windows removal must expose the transient sharing violation");
    assert_eq!(
        bare_error.raw_os_error(),
        Some(WINDOWS_SHARING_VIOLATION),
        "{bare_error}"
    );
    assert!(root.is_dir(), "sharing failure removed the held directory");

    let mut exhausted_retry_count = 0;
    let exhausted = remove_windows_dev_stage_with(&root, || false, || exhausted_retry_count += 1)
        .expect_err("expired cleanup deadline must preserve the sharing violation");
    assert_eq!(
        exhausted.raw_os_error(),
        Some(WINDOWS_SHARING_VIOLATION),
        "{exhausted}"
    );
    assert_eq!(
        exhausted_retry_count, 0,
        "expired cleanup deadline entered the retry path"
    );

    let mut retry_count = 0;
    let retry_budget = Cell::new(10_000_u32);
    remove_windows_dev_stage_with(
        &root,
        || {
            let remaining = retry_budget.get();
            if remaining == 0 {
                false
            } else {
                retry_budget.set(remaining - 1);
                true
            }
        },
        || {
            retry_count += 1;
            if let Some(stdin) = blocker_stdin.take() {
                drop(stdin);
                blocker.0.wait().expect("reap final-directory holder");
            }
            thread::yield_now();
        },
    )
    .expect("bounded cleanup after holder exit");

    assert!(retry_count > 0, "sharing violation did not exercise retry");
    assert!(!root.exists(), "cleanup left the final directory");

    let file = temp.path().join("not-a-directory");
    fs::write(&file, b"fixture").expect("non-directory fixture");
    let mut unrelated_retry_count = 0;
    let unrelated = remove_windows_dev_stage_with(&file, || true, || unrelated_retry_count += 1)
        .expect_err("non-directory cleanup must fail");
    assert_ne!(
        unrelated.raw_os_error(),
        Some(WINDOWS_SHARING_VIOLATION),
        "{unrelated}"
    );
    assert_eq!(
        unrelated_retry_count, 0,
        "non-sharing error entered the retry path"
    );
}
