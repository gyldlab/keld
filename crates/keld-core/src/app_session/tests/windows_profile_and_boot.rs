#[test]
#[cfg(windows)]
fn windows_profile_follows_the_boot_route() {
    let development =
        select_windows_profile_mode(None, true).expect("a dev stage runs under its lease");
    assert!(matches!(development, WindowsProfileMode::EphemeralDev));

    let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
        .expect("synthetic verified identity");
    let verified_profile = identity.profile_identity;
    let installed = select_windows_profile_mode(Some(identity), false)
        .expect("an installed package runs without a lease");
    let WindowsProfileMode::Persistent(selection) = installed else {
        panic!("an installed package selected an ephemeral profile")
    };
    assert_eq!(
        selection,
        WebProfileSelection::Persistent(verified_profile),
        "the profile is the identity verified at boot"
    );

    let error = select_windows_profile_mode(None, false)
        .expect_err("a dev stage without a live lease must not run");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert!(error.to_string().contains("no live dev lease"), "{error}");
    let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
        .expect("synthetic verified identity");
    let error = select_windows_profile_mode(Some(identity), true)
        .expect_err("an installed package must not run under a dev lease");
    assert_eq!(error.code(), "KELD-CORE-037");
    assert!(
        error.to_string().contains("presented a dev lease"),
        "{error}"
    );
}

#[test]
#[cfg(windows)]
fn windows_dev_lease_routes_only_to_the_dev_stage_validator() {
    let error = select_windows_boot_route_with(
        true,
        || Err(target_error("app root", "synthetic dev-stage refusal")),
        || panic!("a dev lease must not reach installed-package boot"),
    )
    .expect_err("the synthetic dev-stage validator decides the dev route");
    assert_eq!(error.code(), "KELD-CORE-036");
    assert!(error.to_string().contains("synthetic dev-stage refusal"));
}

#[test]
#[cfg(windows)]
fn windows_lease_less_boot_reaches_only_installed_package_boot() {
    let unsigned = select_windows_boot_route_with(
        false,
        || panic!("a lease-less launch must not reach the dev-stage validator"),
        || {
            Err(windows_identity_error(
                "synthetic verifier rejection",
                "Use a real signed package fixture.",
            ))
        },
    )
    .expect_err("an unverified lease-less executable is refused");
    assert_eq!(unsigned.code(), "KELD-WV-009");

    let unselected = select_windows_boot_route_with(
        false,
        || panic!("a lease-less launch must not reach the dev-stage validator"),
        || {
            Err(installed_package_error(
                &keld_update::UpdateError::ExecutableBinding {
                    image: keld_update::WindowsLocatedImage::Host,
                    step: "locator",
                    detail: "synthetic staged layout".to_owned(),
                },
            ))
        },
    )
    .expect_err("a verified host outside an installation is refused");
    assert_eq!(unselected.code(), "KELD-UPDATE-018");
    assert!(
        unselected.to_string().contains("synthetic staged layout"),
        "{unselected}"
    );
}

#[test]
#[cfg(windows)]
fn installed_package_refusal_carries_its_code_once() {
    let error = installed_package_error(&keld_update::UpdateError::ExecutableBinding {
        image: keld_update::WindowsLocatedImage::Host,
        step: "locator",
        detail: "synthetic staged layout".to_owned(),
    });
    let rendered = error.to_string();
    println!("KELD_KEL254_INSTALLED_PACKAGE_REFUSAL {rendered}");
    // The resource snapshot reads process-wide counters other tests advance.
    let (message, _resources) = rendered
        .split_once(" [startup-resource-attempts ")
        .expect("a host error ends with its startup-resource snapshot");
    assert_eq!(
        message,
        "KELD-UPDATE-018: no-flag host failed during Windows installed package selection \
             — installed executable locator refused (synthetic staged layout). Launch \
             keld-host.exe from its installation's selected version tree; repair or reinstall \
             through the trusted installer if the layout is damaged. Then relaunch the \
             installed host."
    );
    assert_eq!(rendered.matches("KELD-UPDATE-").count(), 1, "{rendered}");
    assert!(!rendered.contains(".."), "{rendered}");
}

#[test]
#[cfg(windows)]
fn installed_boot_of_an_unsigned_host_refuses_before_resources() {
    let before = startup_resource_snapshot();
    let error = validate_installed_current_exe()
        .expect_err("the unsigned test executable is not an installed package");
    assert_eq!(error.code(), "KELD-WV-009");
    // The running image opened as a pinned trust image and was refused by its owner.
    assert!(
        error
            .to_string()
            .contains("rejected the current executable with status"),
        "{error}"
    );
    assert_eq!(
        error.resources, before,
        "installed boot advanced app resources"
    );
}

#[test]
#[cfg(windows)]
fn installed_boot_reads_the_expectation_from_the_handle_before_selection() {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let executable = std::env::current_exe().expect("test executable path");
    // The test executable carries no `.keldeai` container and lies in no installation.
    let image = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&executable)
        .expect("open the test executable as the verified-image owner does");
    let locator = executable.canonicalize().expect("canonical locator");
    let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
        .expect("synthetic verified identity");
    let before = startup_resource_snapshot();
    let error = validate_installed_from_verified(&locator, &image, identity)
        .expect_err("a host without an embedded expectation is refused");
    // KELD-UPDATE-019, not the locator's KELD-UPDATE-018: the expectation is read from
    // the handle before any installation is located.
    assert_eq!(error.code(), "KELD-UPDATE-019");
    assert!(error.to_string().contains("KELD-PACK-007"), "{error}");
    assert_eq!(
        error.resources, before,
        "installed boot advanced app resources"
    );
}

#[test]
#[cfg(windows)]
fn installed_boot_reads_the_verified_handle_never_the_locator_path() {
    use std::io::{Seek as _, SeekFrom, Write as _};

    // The locator names this test executable, a valid PE image without a container
    // (KELD-PACK-007 if anything reopened it). The handle is an anonymous file, with
    // no path to reopen and its cursor at end of file, holding no PE image at all.
    let locator = std::env::current_exe()
        .expect("test executable path")
        .canonicalize()
        .expect("canonical locator");
    let mut image = tempfile::tempfile().expect("anonymous image handle");
    image
        .write_all(b"MZ is not a host image")
        .expect("write the malformed image");
    image.seek(SeekFrom::End(0)).expect("cursor to end of file");
    let identity = ValidatedAppIdentity::from_verified_parts([9; 32], "com.example.synthetic")
        .expect("synthetic verified identity");
    let before = startup_resource_snapshot();
    let error = validate_installed_from_verified(&locator, &image, identity)
        .expect_err("a malformed verified image is refused");
    // KELD-PACK-006 is the handle's malformed image; a locator reopen would report the
    // test executable's missing container instead.
    assert_eq!(error.code(), "KELD-UPDATE-019");
    assert!(error.to_string().contains("KELD-PACK-006"), "{error}");
    assert_eq!(
        error.resources, before,
        "installed boot advanced app resources"
    );
}

#[test]
#[cfg(windows)]
fn windows_boot_files_validate_without_the_dev_stage_dacl() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("tree");
    fs::create_dir(&root).expect("tree");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join("index.html"), "<p>installed</p>\n").expect("renderer");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");

    // The inherited temporary-directory DACL is not owner-private: the dev route
    // still refuses this tree, so its predicate is unchanged.
    let error = must_err(validate_from_root(&root), "a non-private stage must fail");
    assert_eq!(error.code(), "KELD-CORE-036");
    assert!(error.to_string().contains("app root DACL"), "{error}");

    let selection = validate_windows_boot_files(root.clone(), WindowsBootMode::DevStage)
        .expect("the selected tree's boot files validate");
    assert_eq!(selection.app.root, root);
    assert_eq!(selection.app.name, "Fixture");
    assert_eq!(selection.app.entry_path, Path::new("main.ts"));
    assert!(matches!(
        &selection.app.initial_window,
        ValidatedInitialWindow::Renderer(bytes) if bytes == b"<p>installed</p>\n"
    ));
    assert_eq!(
        selection.permissions_digest,
        decode_digest(DIGEST).expect("fixture digest")
    );
}

#[test]
#[cfg(windows)]
fn windows_boot_files_keep_the_strict_descriptor_and_target_rules() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("tree");
    fs::create_dir(&root).expect("tree");
    fs::write(root.join("main.ts"), "await new Promise(() => {});\n").expect("entry");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions");
    fs::write(root.join(BOOT_FILE), br#"{"schema":1,"foreign":true}"#).expect("boot");
    let error = must_err(
        validate_windows_boot_files(root.clone(), WindowsBootMode::DevStage),
        "a malformed descriptor must fail",
    );
    assert_eq!(error.code(), "KELD-CORE-035");

    let outside = temp.path().join("outside.html");
    fs::write(&outside, "<p>outside</p>\n").expect("outside renderer");
    std::os::windows::fs::symlink_file(&outside, root.join("index.html"))
        .expect("renderer symlink");
    fs::write(root.join(BOOT_FILE), valid_boot("main.ts", "index.html")).expect("boot");
    let error = must_err(
        validate_windows_boot_files(root, WindowsBootMode::DevStage),
        "a reparse-point renderer must fail",
    );
    assert_eq!(error.code(), "KELD-CORE-036");
    assert!(error.to_string().contains("reparse point"), "{error}");
}

#[test]
#[cfg(windows)]
fn windows_trust_image_refusal_is_reported_as_wv009() {
    let directory = tempfile::tempdir().expect("trust-image fixture directory");
    let target = directory.path().join("target.exe");
    let link = directory.path().join("link.exe");
    fs::write(&target, b"trust image fixture").expect("write trust-image target");
    std::os::windows::fs::symlink_file(&target, &link).expect("create a leaf symlink");

    // keld-guard refuses the leaf reparse point; keld-core owns the code.
    let error = verified_windows_image(&link).expect_err("a leaf reparse point is refused");
    assert_eq!(error.code(), "KELD-WV-009");
    assert!(error.to_string().contains("reparse point"), "{error}");
}
