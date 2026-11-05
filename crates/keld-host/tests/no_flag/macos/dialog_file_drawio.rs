//! Source-exact draw.io picker options on the actual facade/Guard/native path.
//! Legacy is explicit: selected paths are data, not filesystem scope grants.

use super::{FilePaths, NativeDialog, compile_ax, run_ax, stage_role};
use crate::support::control::read_control_line;
use crate::support::product::ProductFixture;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

pub(super) fn stage(fixture: &ProductFixture, paths: &FilePaths) -> keld_cli::boot::DevBootStage {
    let role = include_str!("../../fixtures/drawio_file_dialog_role.ts")
        .replace(
            "__OPEN_PATH__",
            &serde_json::to_string(&paths.source).expect("fixture Open path"),
        )
        .replace(
            "__SAVE_PATH__",
            &serde_json::to_string(&paths.destination).expect("fixture Save path"),
        );
    stage_role(
        fixture,
        &role,
        r#"{"profile":"legacy","app":{"system":["dialog"]}}"#,
    )
}

pub(super) fn exercise(dialog: &mut NativeDialog, fixture: &ProductFixture, paths: &FilePaths) {
    // The shared launch first cancels sourceOpen/Save(undefined, ...) through
    // real native buttons. These requests separately prove absent-key parity.
    dialog.press("OPEN_ABSENT_CANCEL", "OPEN_RESULT true 0", "Cancel");
    dialog.press("SAVE_ABSENT_CANCEL", "SAVE_RESULT true true", "Cancel");

    let before = fs::read(&paths.source).expect("independent source sentinel");
    let selector = compile_ax(fixture, "select_file");
    dialog.submit("DRAWIO_OPEN_SELECT");
    let activation = run_ax(
        Command::new(&selector)
            .arg(dialog.host_pid.to_string())
            .arg(&paths.source),
    );
    assert!(
        activation.status.success(),
        "source-exact Open native file selection: {activation:?}"
    );
    let open = result(dialog, "DRAWIO_OPEN_SELECTED ");
    assert_eq!(open.as_object().expect("Open result object").len(), 2);
    assert_eq!(open["canceled"], false);
    let file_paths = open["filePaths"].as_array().expect("Open filePaths array");
    assert_eq!(file_paths.len(), 1, "one actually selected ordinary file");
    assert_path(
        file_paths[0].as_str().expect("Open path string"),
        &paths.source,
    );
    dialog.await_detached("DRAWIO_OPEN_SELECT");

    // Press Save without editing its name or directory. The returned actual
    // path must prove that the explicit defaultPath reached the native panel.
    assert!(!paths.destination.exists(), "owned absent Save destination");
    dialog.submit("DRAWIO_SAVE_SELECT");
    let activation = run_ax(
        Command::new(&dialog.button_helper)
            .arg(dialog.host_pid.to_string())
            .arg("Save"),
    );
    assert!(
        activation.status.success(),
        "source-exact Save native button: {activation:?}"
    );
    let save = result(dialog, "DRAWIO_SAVE_SELECTED ");
    assert_eq!(save.as_object().expect("Save result object").len(), 2);
    assert_eq!(save["canceled"], false);
    assert_path(
        save["filePath"].as_str().expect("Save path string"),
        &paths.destination,
    );
    assert!(
        !paths.destination.exists(),
        "native Save returns a path without creating a file"
    );
    assert_eq!(
        fs::read(&paths.source).expect("source after pickers"),
        before
    );
    dialog.await_detached("DRAWIO_SAVE_SELECT");
    // Shared scenario now requires a healthy real Message, Quit, clean Host
    // exit, the actual Legacy disclosure and disappearance of the Bun role.
}

fn result(dialog: &mut NativeDialog, prefix: &str) -> serde_json::Value {
    let line = read_control_line(&mut dialog.reader);
    let json = line
        .strip_prefix(prefix)
        .expect("original picker result correlation");
    serde_json::from_str(json).expect("actual public picker result JSON")
}

fn assert_path(actual: &str, expected: &Path) {
    let actual = Path::new(actual);
    assert!(actual.is_absolute(), "native path must be absolute POSIX");
    assert_eq!(actual.file_name(), expected.file_name());
    assert_eq!(
        actual.parent().and_then(|p| p.canonicalize().ok()),
        expected.parent().and_then(|p| p.canonicalize().ok()),
        "native path must resolve to the independently owned fixture directory"
    );
}

pub(super) fn assert_legacy_disclosure(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr
            .matches("legacy — zero-authority claim forfeited")
            .count(),
        1,
        "explicit Legacy forfeits ambient node:fs containment: {stderr}"
    );
}

#[test]
fn real_host_drawio_source_exact_open_save_options_cancel_and_select_paths() {
    super::run_real_host_files(super::FileScenario::DrawioExact, 2);
}
