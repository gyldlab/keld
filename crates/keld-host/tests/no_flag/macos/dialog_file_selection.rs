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

/// The OS's broad XML UTType can admit an SVG row. The same real AX selection
/// must succeed under All Files, but must never gain a successful Open reply
/// when the caller declared XML only. AX's selected-child setter can force
/// the otherwise disabled native row, so Host result rejection is essential.
pub(super) fn reject_widened_svg(dialog: &mut NativeDialog, fixture: &ProductFixture, svg: &Path) {
    let selector = compile_ax(fixture, "select_file");
    dialog.submit("OPEN_XML_SVG_ALLFILES");
    let positive = run_ax(
        Command::new(&selector)
            .arg(dialog.host_pid.to_string())
            .arg(svg),
    );
    assert!(
        positive.status.success(),
        "All Files SVG control: {positive:?}"
    );
    let actual = read_control_line(&mut dialog.reader);
    let prefix = "SVG_CONTROL_SELECTED false ";
    assert!(actual.starts_with(prefix), "real SVG selection: {actual}");
    let selected = Path::new(actual.trim_start_matches(prefix));
    assert_eq!(selected.file_name(), svg.file_name());
    assert_eq!(
        selected.parent().and_then(|p| p.canonicalize().ok()),
        svg.parent().and_then(|p| p.canonicalize().ok()),
        "All Files returned the actual fixture path"
    );
    dialog.await_detached("OPEN_XML_SVG_ALLFILES");

    dialog.submit("OPEN_XML_SVG");
    let forced = run_ax(
        Command::new(&selector)
            .arg(dialog.host_pid.to_string())
            .arg(svg)
            .arg("--allow-native-denial"),
    );
    assert!(forced.status.success(), "native SVG selection: {forced:?}");
    eprintln!(
        "GH478_NATIVE_XML_SVG_REFUSAL stdout={} stderr={}",
        String::from_utf8_lossy(&forced.stdout),
        String::from_utf8_lossy(&forced.stderr)
    );
    let observed = String::from_utf8_lossy(&forced.stdout);
    if observed.contains("NATIVE_OS_DENIED_FILE_ROW") {
        // Native exact-filter admission may make an SVG row entirely
        // unselectable. A healthy physical Cancel proves the original dialog
        // remains owned and no prohibited path was returned.
        let canceled = run_ax(
            Command::new(&dialog.button_helper)
                .arg(dialog.host_pid.to_string())
                .arg("Cancel"),
        );
        assert!(
            canceled.status.success(),
            "dismiss native refusal: {canceled:?}"
        );
        assert_eq!(read_control_line(&mut dialog.reader), "OPEN_RESULT true 0");
    } else {
        assert!(
            observed.contains("EXACT_HOST_NATIVE_FILE_SELECTED"),
            "unexpected native SVG oracle: {forced:?}"
        );
        // A programmatic AX selection may force a disabled row far enough
        // to attempt Open. AppKit's validateURL callback must reject that
        // action *before* returning a selected URL to Keld. Observe and
        // dismiss the real native error alert, then Cancel the same sheet.
        let rejected = run_ax(
            Command::new(&dialog.button_helper)
                .arg(dialog.host_pid.to_string())
                .arg("OK"),
        );
        assert_eq!(
            rejected.status.success(),
            true,
            "AppKit must show an OS validation refusal: {rejected:?}"
        );
        let canceled = run_ax(
            Command::new(&dialog.button_helper)
                .arg(dialog.host_pid.to_string())
                .arg("Cancel"),
        );
        assert!(
            canceled.status.success(),
            "Cancel after native validation refusal: {canceled:?}"
        );
        assert_eq!(read_control_line(&mut dialog.reader), "OPEN_RESULT true 0");
    }
    dialog.await_detached("OPEN_XML_SVG");
}

/// An independent, user-equivalent native-row oracle. Its helper posts real
/// mouse down/up only after binding one visible SVG row to the exact Host
/// process, current folder and screen hit target. A failed Accessibility read,
/// unsupported operation or timeout must fail this test, never imply denial.
///
/// The same mouse path must first Open the fixture under All Files. Under an
/// XML-only request it must leave native Open disabled *before* any OS commit.
/// Removing/forcing true the production shouldEnableURL delegate must make the
/// second phase fail; without that mutation run, this is a candidate oracle,
/// not complete negative-control evidence.
pub(super) fn verify_physical_svg_denial(
    dialog: &mut NativeDialog,
    fixture: &ProductFixture,
    svg: &Path,
) {
    let clicker = compile_ax(fixture, "user_click_file");
    let host_pid = dialog.host_pid;
    let run_click = |mode: &str| {
        run_ax(
            Command::new(&clicker)
                .arg(host_pid.to_string())
                .arg(svg)
                .arg(mode)
                .env("KELD_GH478_EXCLUSIVE_GUI", "1"),
        )
    };

    dialog.submit("OPEN_USER_SVG_ALLFILES");
    let permitted = run_click("allowed");
    assert!(
        permitted.status.success(),
        "physical All Files SVG control did not select: {permitted:?}"
    );
    eprintln!(
        "GH478_USER_CLICK_ALL_FILES stdout={} stderr={}",
        String::from_utf8_lossy(&permitted.stdout).trim(),
        String::from_utf8_lossy(&permitted.stderr).trim()
    );
    assert_eq!(
        String::from_utf8_lossy(&permitted.stdout).trim(),
        format!(
            "GH478_USER_CLICK_ALLOWED pid={} basename=gh478-widened.svg",
            dialog.host_pid
        ),
        "All Files must be selected by a real mouse event"
    );
    let reply = read_control_line(&mut dialog.reader);
    let prefix = "SVG_USER_SELECTED false ";
    assert!(reply.starts_with(prefix), "All Files result: {reply}");
    let returned = Path::new(reply.trim_start_matches(prefix));
    assert_eq!(returned.file_name(), svg.file_name());
    assert_eq!(
        returned.parent().and_then(|path| path.canonicalize().ok()),
        svg.parent().and_then(|path| path.canonicalize().ok()),
        "physical All Files click must return the exact owned SVG"
    );
    dialog.await_detached("OPEN_USER_SVG_ALLFILES");
    dialog.press("MESSAGE_NEXT", "MESSAGE_RESULT 0", "Save");

    dialog.submit("OPEN_USER_SVG_XML_ONLY");
    let denied = run_click("denied");
    assert!(
        denied.status.success(),
        "native XML-only SVG click not independently proven disabled: {denied:?}"
    );
    eprintln!(
        "GH478_USER_CLICK_XML_DENY stdout={} stderr={}",
        String::from_utf8_lossy(&denied.stdout).trim(),
        String::from_utf8_lossy(&denied.stderr).trim()
    );
    assert_eq!(
        String::from_utf8_lossy(&denied.stdout).trim(),
        format!(
            "GH478_USER_CLICK_DENIED pid={} basename=gh478-widened.svg open_enabled=false",
            dialog.host_pid
        ),
        "a missing AX attribute or failed HID action is never a native denial"
    );
    let canceled = run_ax(
        Command::new(&dialog.button_helper)
            .arg(dialog.host_pid.to_string())
            .arg("Cancel"),
    );
    assert!(
        canceled.status.success(),
        "dismiss denied native panel: {canceled:?}"
    );
    assert_eq!(
        read_control_line(&mut dialog.reader),
        "SVG_USER_DENIED_CANCEL 0",
        "a physically denied SVG must not commit a file path"
    );
    dialog.await_detached("OPEN_USER_SVG_XML_ONLY");
}

/// Independently select an actual XML file with the exact same OS AX helper.
/// The XML-positive and SVG-negative fixture directories each contain one
/// owned file, so the helper cannot select the wrong row or fabricate a path.
pub(super) fn verify_native_xml_positive(
    dialog: &mut NativeDialog,
    fixture: &ProductFixture,
    xml: &Path,
) {
    let selector = compile_ax(fixture, "select_file");
    dialog.submit("OPEN_XML_ONLY");
    let output = run_ax(
        Command::new(&selector)
            .arg(dialog.host_pid.to_string())
            .arg(xml),
    );
    assert!(
        output.status.success(),
        "native XML-only selection is not available: {output:?}"
    );
    let result = read_control_line(&mut dialog.reader);
    let prefix = "XML_SELECTED false ";
    assert!(result.starts_with(prefix), "native XML selection: {result}");
    let selected = Path::new(result.trim_start_matches(prefix));
    assert_eq!(selected.file_name(), xml.file_name());
    assert_eq!(
        selected.parent().and_then(|p| p.canonicalize().ok()),
        xml.parent().and_then(|p| p.canonicalize().ok()),
        "the OS must return the exact owned XML path"
    );
    dialog.await_detached("OPEN_XML_ONLY");
}

/// Exercise the true native accessory rather than changing Rust's filter model.
/// The selected paths are independently returned by the authenticated Host;
/// the fixture helper cannot synthesize a completion or alter the KIPC frame.
pub(super) fn verify_native_filter_switching(
    dialog: &mut NativeDialog,
    fixture: &ProductFixture,
    paths: &FilePaths,
) {
    let accessory = compile_ax(fixture, "switch_filter");
    let selector = compile_ax(fixture, "select_file");
    if std::env::var("KELD_GH478_EXCLUSIVE_GUI").ok().as_deref() == Some("1") {
        // AXSelectedChildren can force disabled rows; use the physical mouse
        // oracle only in an explicitly exclusive macOS GUI session.
        let clicker = compile_ax(fixture, "user_click_file");
        dialog.submit("OPEN_INITIAL_VSDX_DENIED");
        capture_open_sheet(dialog);
        let denied = run_ax(
            Command::new(&clicker)
                .arg(dialog.host_pid.to_string())
                .arg(&paths.vsdx)
                .arg("denied"),
        );
        eprintln!(
            "GH478_PHYSICAL_INITIAL_VSDX_DENIAL {}",
            String::from_utf8_lossy(&denied.stderr)
        );
        assert!(
            denied.status.success(),
            "native VSDX mouse denial: {denied:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&denied.stdout).trim(),
            format!(
                "GH478_USER_CLICK_DENIED pid={} basename=gh478-format.vsdx open_enabled=false",
                dialog.host_pid
            ),
        );
        let canceled = run_ax(
            Command::new(&dialog.button_helper)
                .arg(dialog.host_pid.to_string())
                .arg("Cancel"),
        );
        assert!(
            canceled.status.success(),
            "initial VSDX denial dismiss: {canceled:?}"
        );
        assert_eq!(
            read_control_line(&mut dialog.reader),
            "VSDX_DENIED_CANCEL 0"
        );
        dialog.await_detached("OPEN_INITIAL_VSDX_DENIED");
    }
    for (operation, group, expected_prefix, path) in [
        (
            "OPEN_SWITCH_VSDX",
            "VSDX",
            "VSDX_SELECTED false ",
            &paths.vsdx,
        ),
        (
            "OPEN_SWITCH_MERMAID",
            "Mermaid",
            "MERMAID_SELECTED false ",
            &paths.mermaid,
        ),
        (
            "OPEN_SWITCH_ALL",
            "All Files",
            "ALL_SELECTED false ",
            &paths.svg,
        ),
    ] {
        dialog.submit(operation);
        capture_open_sheet(dialog);
        let switched = run_ax(
            Command::new(&accessory)
                .arg(dialog.host_pid.to_string())
                .arg(path)
                .arg(group),
        );
        assert!(
            switched.status.success(),
            "actual native {group} accessory did not select: {switched:?}"
        );
        eprintln!(
            "GH478_NATIVE_ACCESSORY_PROOF {}",
            String::from_utf8_lossy(&switched.stderr)
        );
        let selected = run_ax(
            Command::new(&selector)
                .arg(dialog.host_pid.to_string())
                .arg(path),
        );
        assert!(
            selected.status.success(),
            "{group} OS row not selected: {selected:?}"
        );
        let reply = read_control_line(&mut dialog.reader);
        assert!(reply.starts_with(expected_prefix), "{group} reply: {reply}");
        let actual = Path::new(reply.trim_start_matches(expected_prefix));
        assert_eq!(actual.file_name(), path.file_name());
        assert_eq!(
            actual
                .parent()
                .and_then(|parent| parent.canonicalize().ok()),
            path.parent().and_then(|parent| parent.canonicalize().ok()),
            "switching the true native group must preserve the selected file"
        );
        dialog.await_detached(operation);
    }
    // Electron accepts duplicate labels with independently ordered groups.
    // The original source index 2 admits XML, while index 0 (same title)
    // admits drawio only; selecting the second duplicate must return XML.
    dialog.submit("OPEN_DUPLICATE_NAMES");
    let switched = run_ax(
        Command::new(&accessory)
            .arg(dialog.host_pid.to_string())
            .arg(&paths.xml)
            .arg("Duplicate"),
    );
    assert!(
        switched.status.success(),
        "same-title native filter index 2 unavailable: {switched:?}"
    );
    let selected = run_ax(
        Command::new(&selector)
            .arg(dialog.host_pid.to_string())
            .arg(&paths.xml),
    );
    assert!(
        selected.status.success(),
        "native menu's second Duplicate group failed to admit exact XML: {selected:?}"
    );
    let reply = read_control_line(&mut dialog.reader);
    assert_eq!(
        reply,
        format!("DUPLICATE_SELECTED false {}", paths.xml.display()),
        "the final OS-selected same-title group must preserve source index 2"
    );
    dialog.await_detached("OPEN_DUPLICATE_NAMES");
}

/// Select a real file, a real directory, and both simultaneously from one
/// Open panel configured with `openFile`, `openDirectory` and `multiSelections`.
/// The AX helper acts only on the two fixture-owned native rows.
pub(super) fn verify_native_mixed_selection(
    dialog: &mut NativeDialog,
    fixture: &ProductFixture,
    paths: &FilePaths,
) {
    let selector = compile_ax(fixture, "select_mixed");
    for (operation, selection, expected) in [
        ("OPEN_MIXED_FILE", "file", vec![&paths.mixed_file]),
        (
            "OPEN_MIXED_DIRECTORY",
            "directory",
            vec![&paths.mixed_folder],
        ),
        (
            "OPEN_MIXED_BOTH",
            "both",
            vec![&paths.mixed_file, &paths.mixed_folder],
        ),
    ] {
        dialog.submit(operation);
        let clicked = run_ax(
            Command::new(&selector)
                .arg(dialog.host_pid.to_string())
                .arg(&paths.mixed_file)
                .arg(&paths.mixed_folder)
                .arg(selection),
        );
        assert!(
            clicked.status.success(),
            "native mixed {selection} selection failed: {clicked:?}"
        );
        let result = read_control_line(&mut dialog.reader);
        let prefix = "MIXED_SELECTED false ";
        assert!(
            result.starts_with(prefix),
            "native mixed {selection}: {result}"
        );
        let actual = result
            .trim_start_matches(prefix)
            .split('|')
            .map(Path::new)
            .map(|path| path.canonicalize().expect("returned native path exists"))
            .collect::<Vec<_>>();
        let observer = String::from_utf8_lossy(&clicked.stdout);
        let native_order = observer
            .split_whitespace()
            .find_map(|token| token.strip_prefix("order="))
            .expect("independent OS-selected row order must be observed");
        let wanted_order = native_order
            .split(',')
            .map(|name| {
                let candidate = if paths.mixed_file.file_name().and_then(|s| s.to_str())
                    == Some(name)
                {
                    &paths.mixed_file
                } else if paths.mixed_folder.file_name().and_then(|s| s.to_str()) == Some(name) {
                    &paths.mixed_folder
                } else {
                    panic!("AX returned an unowned selected row: {name}");
                };
                candidate
                    .canonicalize()
                    .expect("observed native path exists")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual, wanted_order,
            "Host must preserve the exact native OS selection order"
        );
        let mut actual = actual;
        let mut wanted = expected
            .into_iter()
            .map(|path| {
                path.canonicalize()
                    .expect("owned expected native path exists")
            })
            .collect::<Vec<_>>();
        actual.sort_unstable();
        wanted.sort_unstable();
        assert_eq!(
            actual, wanted,
            "only the exact OS-selected paths may return"
        );
        dialog.await_detached(operation);
    }
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
