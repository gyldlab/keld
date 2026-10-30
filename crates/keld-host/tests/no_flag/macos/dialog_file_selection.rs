//! Real OS positive file selection and independent returned-path observations.
use super::{FilePaths, NativeDialog, compile_ax, run_ax};
use crate::support::control::read_control_line;
use crate::support::product::ProductFixture;
use std::path::Path;
use std::process::Command;

pub(super) fn selected_paths(
    dialog: &mut NativeDialog,
    fixture: &ProductFixture,
    paths: &FilePaths,
    directory_hint: bool,
) {
    let selector = compile_ax(fixture, "select_file");
    let save_observer = directory_hint.then(|| compile_ax(fixture, "save_directory"));
    for (operation, prefix, selected) in [
        ("SAVE_SELECT", "SAVE_SELECTED false ", &paths.destination),
        ("OPEN_SELECT", "OPEN_SELECTED false ", &paths.source),
    ] {
        eprintln!("GH478_POSITIVE_PHASE {operation} begin");
        dialog.submit(operation);
        eprintln!("GH478_POSITIVE_PHASE {operation} submitted");
        if operation == "OPEN_SELECT" {
            capture_open_sheet(dialog);
        }
        let activation = if operation == "OPEN_SELECT" {
            run_ax(
                Command::new(&selector)
                    .arg(dialog.host_pid.to_string())
                    .arg(selected),
            )
        } else if let Some(observer) = &save_observer {
            run_ax(
                Command::new(observer)
                    .arg(dialog.host_pid.to_string())
                    .arg(selected),
            )
        } else {
            run_ax(
                Command::new(&dialog.button_helper)
                    .arg(dialog.host_pid.to_string())
                    .arg("Save"),
            )
        };
        assert!(
            activation.status.success(),
            "{operation} UI selection unavailable: {activation:?}"
        );
        if directory_hint && operation == "SAVE_SELECT" {
            eprintln!("{}", String::from_utf8_lossy(&activation.stderr));
        }
        eprintln!("GH478_POSITIVE_PHASE {operation} button pressed");
        let result = read_control_line(&mut dialog.reader);
        eprintln!("GH478_POSITIVE_PHASE {operation} result {result}");
        assert!(result.starts_with(prefix), "native result: {result}");
        let value = Path::new(result.trim_start_matches(prefix));
        assert_eq!(
            value.file_name(),
            selected.file_name(),
            "real OS filename expected"
        );
        assert_eq!(
            value.parent().and_then(|p| p.canonicalize().ok()),
            selected.parent().and_then(|p| p.canonicalize().ok()),
            "real path must remain in dedicated test fixture, not a guessed directory",
        );
        assert!(
            !paths.destination.exists(),
            "native Save only returned a path; no file created"
        );
        dialog.await_detached(operation);
    }
}

/// A real OS selection can exceed Apple's `UTType`-conformance filter.
/// Keld must reject the filename before sending an Open success to the caller.
pub(super) fn reject_widened_svg(dialog: &mut NativeDialog, fixture: &ProductFixture, svg: &Path) {
    let selector = compile_ax(fixture, "select_file");
    dialog.submit("OPEN_XML_SVG");
    let activation = run_ax(
        Command::new(&selector)
            .arg(dialog.host_pid.to_string())
            .arg(svg),
    );
    assert!(
        activation.status.success(),
        "real SVG attempt: {activation:?}"
    );
    assert_eq!(
        read_control_line(&mut dialog.reader),
        "OPEN_ERROR KELD-NATIVE-016",
        "an XML-only request must never return SVG success"
    );
    dialog.await_detached("OPEN_XML_SVG");
}

fn capture_open_sheet(dialog: &NativeDialog) {
    let (Ok(helper), Ok(capture_path)) = (
        std::env::var("KELD_TEST_WINDOW_ID_HELPER"),
        std::env::var("KELD_TEST_CAPTURE_PATH"),
    ) else {
        return;
    };
    let observation = run_ax(
        Command::new(&dialog.button_helper)
            .arg(dialog.host_pid.to_string())
            .arg("Cancel")
            .arg("--observe"),
    );
    assert!(
        observation.status.success(),
        "Open sheet missing: {observation:?}"
    );
    let ids = Command::new(helper)
        .arg(dialog.host_pid.to_string())
        .output()
        .expect("query only this Host's current native window ID");
    let stdout = String::from_utf8(ids.stdout).expect("window ID output UTF-8");
    if let Some(number) = stdout
        .split_whitespace()
        .find_map(|token| token.strip_prefix("WINDOW_ID="))
    {
        let capture = Command::new("/usr/sbin/screencapture")
            .args(["-x", "-o", "-l", number, &capture_path])
            .output()
            .expect("capture exact Host-owned window for native acceptance review");
        eprintln!(
            "GH478_SCREENSHOT_EXIT={} PATH={capture_path}",
            capture.status
        );
    }
}
