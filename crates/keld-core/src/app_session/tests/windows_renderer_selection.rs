use std::{fs, path::Path};

use super::{
    BOOT_FILE, PERMISSIONS_FILE, ValidatedInitialWindow, WindowsBootMode, valid_boot,
    validate_windows_boot_files,
};

// KEL-96 T1a AC3-5, sections 4.2/7, and KEL-254 AC9 require the declared
// renderer's owned bytes. This fixture isolates the existing boot-file loader;
// root ACL admission, package authenticity, and WebView navigation remain unqualified.
#[test]
fn windows_boot_files_keep_declared_renderer_bytes_after_selection() {
    const ENTRY: &str = "src/main.ts";
    const RENDERER: &str = "ui/start.html";
    const ORIGINAL: &[u8] = b"<p>declared-renderer-before-replacement</p>\n";
    const REPLACEMENT: &[u8] = b"<p>declared-renderer-after-replacement</p>\n";
    const INDEX_DECOY: &[u8] = b"<p>wrong-default-index-renderer</p>\n";
    const ENTRY_DECOY: &[u8] = b"export const source = 'wrong-primary-entry-renderer';\n";

    let temp = tempfile::tempdir().expect("renderer-selection fixture");
    let fixture = temp.path().to_path_buf();
    let root = fixture.join("tree");
    fs::create_dir_all(root.join("src")).expect("entry directory");
    fs::create_dir_all(root.join("ui")).expect("renderer directory");
    fs::write(root.join(ENTRY), ENTRY_DECOY).expect("distinct primary entry");
    fs::write(root.join(RENDERER), ORIGINAL).expect("declared nested renderer");
    fs::write(root.join("index.html"), INDEX_DECOY).expect("distinct default-path decoy");
    fs::write(root.join(PERMISSIONS_FILE), b"{}\n").expect("permissions fixture");
    let boot = valid_boot(ENTRY, RENDERER);
    fs::write(root.join(BOOT_FILE), &boot).expect("declared renderer descriptor");

    let selected = validate_windows_boot_files(root.clone(), WindowsBootMode::DevStage)
        .expect("declared renderer selection");
    let mut replacement_boot: serde_json::Value =
        serde_json::from_slice(&boot).expect("existing fixture descriptor");
    replacement_boot["name"] = serde_json::Value::String("ReplacedFixture".to_owned());
    let replacement_boot =
        serde_json::to_vec(&replacement_boot).expect("replacement fixture descriptor");
    fs::write(root.join(RENDERER), REPLACEMENT).expect("replace declared renderer");
    fs::write(root.join(BOOT_FILE), &replacement_boot).expect("replace descriptor name");
    let disk_renderer = fs::read(root.join(RENDERER)).expect("observe replaced renderer");
    let disk_boot = fs::read(root.join(BOOT_FILE)).expect("observe replaced descriptor");

    let reloaded = validate_windows_boot_files(root, WindowsBootMode::DevStage)
        .expect("fresh selection observes replacement");
    let selected_name = selected.app.name.clone();
    let selected_entry = selected.app.entry_path.clone();
    let selected_renderer = match &selected.app.initial_window {
        ValidatedInitialWindow::Renderer(bytes) => Some(bytes.clone()),
        ValidatedInitialWindow::None => None,
    };
    let reloaded_name = reloaded.app.name.clone();
    let reloaded_renderer = match &reloaded.app.initial_window {
        ValidatedInitialWindow::Renderer(bytes) => Some(bytes.clone()),
        ValidatedInitialWindow::None => None,
    };

    // Close both retained entry/permissions handles before deleting the fixture.
    // Semantic failures then retain their exact observations in libtest output.
    drop(reloaded);
    drop(selected);
    let cleanup = temp.close();
    let cleanup_observation = fs::symlink_metadata(&fixture).map_err(|error| error.kind());
    eprintln!(
        "renderer-selection observations: fixture={fixture:?}; cleanup={cleanup:?}; \
         selected_name={selected_name:?}; selected_renderer={selected_renderer:?}; \
         reloaded_name={reloaded_name:?}; reloaded_renderer={reloaded_renderer:?}"
    );
    cleanup.expect("release renderer-selection fixture");
    assert!(
        matches!(cleanup_observation, Err(std::io::ErrorKind::NotFound)),
        "fixture remains after explicit cleanup: {cleanup_observation:?}"
    );
    assert_eq!(
        disk_renderer, REPLACEMENT,
        "renderer replacement reached disk"
    );
    assert_eq!(
        disk_boot, replacement_boot,
        "descriptor replacement reached disk"
    );
    assert_eq!(selected_name, "Fixture", "accepted descriptor changed");
    assert_eq!(
        selected_entry,
        Path::new(ENTRY),
        "wrong primary entry selected"
    );
    assert_eq!(
        selected_renderer.as_deref(),
        Some(ORIGINAL),
        "accepted renderer must be the declared path's original bytes"
    );
    assert_eq!(
        reloaded_name, "ReplacedFixture",
        "fresh descriptor was stale"
    );
    assert_eq!(
        reloaded_renderer.as_deref(),
        Some(REPLACEMENT),
        "fresh loader must observe the replacement of the declared renderer"
    );
}
