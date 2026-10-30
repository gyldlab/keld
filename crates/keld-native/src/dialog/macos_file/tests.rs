//! Unit tests for the bounded native Open result: real NSURL objects, no mocked path
//! authority and no inference from `AppKit` filter visibility.

use super::{selected_open_paths, selected_save_path};
use crate::dialog::MAX_DIALOG_PATHS;
use crate::dialog::file_filter::MacFileFilterModel;
use keld_ipc::IpcError;
use objc2_foundation::{NSArray, NSString, NSURL};
use std::path::Path;

#[test]
fn bounded_open_selection_preserves_exact_paths() {
    let unfiltered = MacFileFilterModel::new(&[]).expect("unrestricted filter");
    let empty = NSArray::<NSURL>::new();
    assert_eq!(
        selected_open_paths(&empty, &unfiltered, true, false, false)
            .expect("empty count is bounded"),
        Vec::<String>::new()
    );

    let selected = NSURL::fileURLWithPath_isDirectory(
        &NSString::from_str("/tmp/gh478-selection.drawio"),
        false,
    );
    let entries = vec![selected; MAX_DIALOG_PATHS];
    let urls = NSArray::from_retained_slice(&entries);
    assert_eq!(
        selected_open_paths(&urls, &unfiltered, true, false, false)
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
    let error = selected_open_paths(&urls, &unfiltered, true, false, false)
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
        selected_open_paths(&urls, &xml, true, false, false),
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
        selected_open_paths(&valid, &documents, true, false, false)
            .expect("both paths are in the group"),
        vec!["/tmp/one.pdf", "/tmp/two.JSON"]
    );

    let mixed = NSArray::from_retained_slice(&[pdf, svg, json]);
    assert!(matches!(
        selected_open_paths(&mixed, &documents, true, false, false),
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
        selected_open_paths(&urls, &unrestricted, false, true, false),
        Err(IpcError::Protocol {
            detail: "native Open selected a file outside the active filter"
        })
    ));
    assert_eq!(
        selected_open_paths(&urls, &unrestricted, true, false, false)
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
    assert!(selected_open_paths(&dirs, &unrestricted, true, false, false).is_err());
    let selected = selected_open_paths(&dirs, &unrestricted, false, true, false)
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
        selected_save_path(Some(pdf), &documents, false).expect("declared PDF extension"),
        "/tmp/report.PDF"
    );
    let svg = NSURL::fileURLWithPath_isDirectory(&NSString::from_str("/tmp/diagram.svg"), false);
    assert!(matches!(
        selected_save_path(Some(svg.clone()), &documents, false),
        Err(IpcError::Protocol {
            detail: "native Save selected a file outside the active filter"
        })
    ));
    assert!(selected_save_path(None, &documents, false).is_err());
    let unrestricted = MacFileFilterModel::new(&[]).expect("unrestricted Save");
    assert_eq!(
        selected_save_path(Some(svg), &unrestricted, false).expect("All Files / no filter"),
        "/tmp/diagram.svg"
    );
}

fn assert_save_package_as_file(package: &Path, unrestricted: &MacFileFilterModel) {
    let url =
        NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&package.to_string_lossy()), true);
    assert_eq!(
        selected_save_path(Some(url.clone()), unrestricted, false)
            .expect("Save's default package-as-file classification is preserved"),
        package.to_string_lossy()
    );
    assert!(
        selected_save_path(Some(url), unrestricted, true).is_err(),
        "Save cannot accept a package after AppKit treats it as a directory"
    );
}

#[test]
fn macos_packages_are_selectable_files_under_the_native_default() {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove only test-owned directory");
        }
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("fixture clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "keld-gh478-package-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&root).expect("isolated directory");
    let _cleanup = Cleanup(root.clone());

    let packages = [root.join("Selected.app"), root.join("Notes.rtfd")];
    let ordinary = root.join("ordinary-folder");
    for dir in packages.iter().chain(std::iter::once(&ordinary)) {
        fs::create_dir(dir).expect("isolated selected item");
        assert!(
            dir.is_dir(),
            "all native fixtures are filesystem directories"
        );
    }

    let unrestricted = MacFileFilterModel::new(&[]).expect("All Files");
    let app_only = MacFileFilterModel::new(&[super::super::FileDialogFilter {
        name: "Apps".into(),
        extensions: vec!["app".into()],
    }])
    .expect("exact application-package extension");
    for package in &packages {
        let url = NSURL::fileURLWithPath_isDirectory(
            &NSString::from_str(&package.to_string_lossy()),
            true,
        );
        let selected_urls = NSArray::from_retained_slice(&[url]);
        let selected = selected_open_paths(&selected_urls, &unrestricted, true, false, false)
            .expect("native file package is a selectable file, not a chosen folder");
        assert_eq!(selected.len(), 1);
        assert_eq!(Path::new(&selected[0]), package);

        assert!(
            selected_open_paths(&selected_urls, &unrestricted, false, true, false).is_err(),
            "directory-only requests must not admit packages treated as files"
        );
        assert_eq!(
            selected_open_paths(&selected_urls, &app_only, true, false, false).is_ok(),
            package.extension().is_some_and(|ext| ext == "app"),
            "package-as-file must still obey the exact declared suffix"
        );

        assert!(
            selected_open_paths(&selected_urls, &unrestricted, true, false, true).is_err(),
            "when packages are treated as directories, OpenFile-only must refuse"
        );
        assert_eq!(
            selected_open_paths(&selected_urls, &unrestricted, false, true, true)
                .expect("package-as-directory follows native OpenDirectory setting"),
            selected
        );
        assert_save_package_as_file(package, &unrestricted);
    }

    let folder_url =
        NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&ordinary.to_string_lossy()), true);
    assert!(
        selected_open_paths(
            &NSArray::from_retained_slice(&[folder_url]),
            &unrestricted,
            true,
            false,
            false,
        )
        .is_err(),
        "ordinary directory is forbidden by an OpenFile-only request"
    );
    assert!(
        selected_save_path(
            Some(NSURL::fileURLWithPath_isDirectory(
                &NSString::from_str(&ordinary.to_string_lossy()),
                true,
            )),
            &unrestricted,
            false,
        )
        .is_err(),
        "Save must not confuse an ordinary folder with a package-as-file"
    );
}
