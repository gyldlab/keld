//! Unit tests for the bounded native Open result: real NSURL objects, no mocked path
//! authority and no inference from `AppKit` filter visibility.

use super::*;

#[test]
fn bounded_open_selection_preserves_exact_paths() {
    let unfiltered = MacFileFilterModel::new(&[]).expect("unrestricted filter");
    let empty = NSArray::<NSURL>::new();
    assert_eq!(
        selected_open_paths(&empty, &unfiltered, true, false).expect("empty count is bounded"),
        Vec::<String>::new()
    );

    let selected = NSURL::fileURLWithPath_isDirectory(
        &NSString::from_str("/tmp/gh478-selection.drawio"),
        false,
    );
    let entries = vec![selected; MAX_DIALOG_PATHS];
    let urls = NSArray::from_retained_slice(&entries);
    assert_eq!(
        selected_open_paths(&urls, &unfiltered, true, false)
            .expect("exactly 32 file URLs are bounded"),
        vec![String::from("/tmp/gh478-selection.drawio"); MAX_DIALOG_PATHS],
    );
}

#[test]
fn excessive_open_selection_is_rejected_before_url_conversion() {
    let unfiltered = MacFileFilterModel::new(&[]).expect("unrestricted filter");
    // An invalid URL first ensures that per-entry conversion cannot take
    // precedence over the independently specified native selection count.
    let non_file = NSURL::URLWithString(&NSString::from_str("https://example.invalid/file"))
        .expect("synthetic non-file URL");
    let entries = vec![non_file; MAX_DIALOG_PATHS + 1];
    let urls = NSArray::from_retained_slice(&entries);
    let error = selected_open_paths(&urls, &unfiltered, true, false)
        .expect_err("32+1 native paths must be refused");
    assert!(
        matches!(
            error,
            IpcError::Protocol {
                detail: "native Open selection exceeds path limit"
            }
        ),
        "native over-limit selection was processed before refusal: {error:?}"
    );
}

#[test]
fn os_xml_type_conformance_cannot_publish_a_selected_svg_path() {
    let xml = MacFileFilterModel::new(&[super::super::FileDialogFilter {
        name: "XML".into(),
        extensions: vec!["xml".into()],
    }])
    .expect("valid XML-only group");
    let svg =
        NSURL::fileURLWithPath_isDirectory(&NSString::from_str("/tmp/gh478-widened.svg"), false);
    let urls = NSArray::from_retained_slice(&[svg]);
    assert!(matches!(
        selected_open_paths(&urls, &xml, true, false),
        Err(IpcError::Protocol {
            detail: "native Open selected a file outside the active filter"
        })
    ));
}

#[test]
fn any_disallowed_member_refuses_the_whole_native_multi_selection() {
    let documents = MacFileFilterModel::new(&[super::super::FileDialogFilter {
        name: "Documents".into(),
        extensions: vec!["pdf".into(), "json".into()],
    }])
    .expect("valid unrelated group");
    let pdf = NSURL::fileURLWithPath_isDirectory(&NSString::from_str("/tmp/one.pdf"), false);
    let json = NSURL::fileURLWithPath_isDirectory(&NSString::from_str("/tmp/two.JSON"), false);
    let svg = NSURL::fileURLWithPath_isDirectory(&NSString::from_str("/tmp/three.svg"), false);

    let valid = NSArray::from_retained_slice(&[pdf.clone(), json.clone()]);
    assert_eq!(
        selected_open_paths(&valid, &documents, true, false).expect("both paths are in the group"),
        vec!["/tmp/one.pdf", "/tmp/two.JSON"]
    );

    let mixed = NSArray::from_retained_slice(&[pdf, svg, json]);
    assert!(matches!(
        selected_open_paths(&mixed, &documents, true, false),
        Err(IpcError::Protocol {
            detail: "native Open selected a file outside the active filter"
        })
    ));
}

#[test]
fn native_open_result_enforces_file_and_directory_selection_flags_independently() {
    let unrestricted = MacFileFilterModel::new(&[]).expect("unfiltered Open");
    let file = NSURL::fileURLWithPath_isDirectory(&NSString::from_str("/tmp/document.xml"), false);
    let urls = NSArray::from_retained_slice(&[file]);

    assert!(matches!(
        selected_open_paths(&urls, &unrestricted, false, true),
        Err(IpcError::Protocol {
            detail: "native Open selected a file outside the active filter"
        })
    ));
    assert_eq!(
        selected_open_paths(&urls, &unrestricted, true, false)
            .expect("normal OpenFile permits file URLs"),
        vec!["/tmp/document.xml"]
    );

    let existing_directory = std::env::temp_dir();
    assert!(existing_directory.is_dir());
    let url = NSURL::fileURLWithPath_isDirectory(
        &NSString::from_str(&existing_directory.to_string_lossy()),
        true,
    );
    let dirs = NSArray::from_retained_slice(&[url]);
    assert!(selected_open_paths(&dirs, &unrestricted, true, false).is_err());
    let selected = selected_open_paths(&dirs, &unrestricted, false, true)
        .expect("directory-only native selection should preserve its path");
    assert_eq!(selected.len(), 1);
    assert_eq!(Path::new(&selected[0]), existing_directory);
}

#[test]
fn native_save_result_uses_the_selected_generic_filter_not_type_conformance() {
    let documents = MacFileFilterModel::new(&[super::super::FileDialogFilter {
        name: "Documents".into(),
        extensions: vec!["pdf".into(), "json".into()],
    }])
    .expect("valid Save group");
    let pdf = NSURL::fileURLWithPath_isDirectory(&NSString::from_str("/tmp/report.PDF"), false);
    assert_eq!(
        selected_save_path(Some(pdf), &documents).expect("declared PDF extension"),
        "/tmp/report.PDF"
    );
    let svg = NSURL::fileURLWithPath_isDirectory(&NSString::from_str("/tmp/diagram.svg"), false);
    assert!(matches!(
        selected_save_path(Some(svg.clone()), &documents),
        Err(IpcError::Protocol {
            detail: "native Save selected a file outside the active filter"
        })
    ));
    assert!(selected_save_path(None, &documents).is_err());
    let unrestricted = MacFileFilterModel::new(&[]).expect("unrestricted Save");
    assert_eq!(
        selected_save_path(Some(svg), &unrestricted).expect("All Files / no filter"),
        "/tmp/diagram.svg"
    );
}
