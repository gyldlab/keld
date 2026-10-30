//! First-proof macOS filter grouping: state transitions, not a fake `NSOpenPanel`.
#![cfg(target_os = "macos")]

use keld_native::dialog::{FileDialogFilter, file_filter::MacFileFilterModel};

fn filter(name: &str, ext: &[&str]) -> FileDialogFilter {
    FileDialogFilter {
        name: name.to_owned(),
        extensions: ext.iter().map(|item| (*item).to_owned()).collect(),
    }
}

#[test]
fn grouped_drawio_filters_remain_ordered_and_switch_by_replacing_allowed_types() {
    let mut model = MacFileFilterModel::new(&[
        filter("draw.io", &["draw.io", "xml", "io"]),
        filter("VSDX", &["vsdx"]),
        filter("Mermaid", &["mmd", "mermaid"]),
        filter("All Files", &["*"]),
    ])
    .expect("admitted exact filter groups");
    assert_eq!(model.labels(), ["draw.io", "VSDX", "Mermaid", "All Files"]);
    assert_eq!(model.selected_index(), Some(0));
    assert_eq!(
        model.active_suffixes(),
        Some(&["io".to_owned(), "xml".to_owned()][..])
    );
    assert!(model.select(1).expect("VSDX group"));
    assert_eq!(model.active_suffixes(), Some(&["vsdx".to_owned()][..]));
    assert!(model.select(2).expect("Mermaid group"));
    assert_eq!(
        model.active_suffixes(),
        Some(&["mmd".to_owned(), "mermaid".to_owned()][..])
    );
    assert!(model.select(3).expect("unrestricted All Files group"));
    assert_eq!(model.active_suffixes(), None);
    assert!(model.select(0).expect("return to draw.io"));
    assert_eq!(
        model.active_suffixes(),
        Some(&["io".to_owned(), "xml".to_owned()][..])
    );
    assert!(!model.select(0).expect("idempotent selection"));
    assert!(model.select(4).is_err(), "out-of-range selection must fail");
    assert_eq!(
        model.selected_index(),
        Some(0),
        "invalid choice cannot mutate selection"
    );
}

#[test]
fn empty_groups_and_selected_wildcard_group_both_mean_unrestricted_only_now() {
    let mut empty = MacFileFilterModel::new(&[]).expect("unfiltered native case");
    assert_eq!(empty.labels(), [] as [&str; 0]);
    assert_eq!(empty.selected_index(), None);
    assert_eq!(empty.active_suffixes(), None);
    assert!(empty.select(0).is_err());

    let mut chosen =
        MacFileFilterModel::new(&[filter("All Files", &["*"]), filter("SVG", &["svg"])])
            .expect("grouped exact * token");
    assert_eq!(chosen.active_suffixes(), None);
    chosen.select(1).expect("switch to SVG");
    assert_eq!(chosen.active_suffixes(), Some(&["svg".to_owned()][..]));
    chosen.select(0).expect("switch back to unrestricted");
    assert_eq!(chosen.active_suffixes(), None);
}

#[test]
fn malformed_compounds_and_wildcard_mix_fail_before_an_nsopenpanel() {
    for ext in [vec!["bad."], vec!["*", "xml"], vec![".xml"], vec!["x/y"]] {
        assert!(MacFileFilterModel::new(&[filter("bad", &ext)]).is_err());
    }
    let model = MacFileFilterModel::new(&[filter("mixed-case", &["XML", "xml", "dRaW.Io"])])
        .expect("distinct source spellings are valid; effective suffixes dedup");
    assert_eq!(
        model.active_suffixes(),
        Some(&["xml".to_owned(), "io".to_owned()][..]),
    );
}

#[test]
fn selected_open_paths_are_exact_not_uttype_conformance_widened() {
    use std::path::Path;

    let mut model = MacFileFilterModel::new(&[
        filter("XML", &["xml"]),
        filter("Other", &["draw.io"]),
        filter("All Files", &["*"]),
    ])
    .expect("three ordered groups");

    assert!(model.accepts_selected_path(Path::new("/tmp/diagram.xml"), false, true, false));
    assert!(model.accepts_selected_path(Path::new("/tmp/DIAGRAM.XML"), false, true, false));
    assert!(!model.accepts_selected_path(Path::new("/tmp/diagram.svg"), false, true, false));
    assert!(!model.accepts_selected_path(Path::new("/tmp/.xml"), false, true, false));

    model.select(1).expect("select compound extension group");
    assert!(model.accepts_selected_path(Path::new("/tmp/diagram.draw.io"), false, true, false));
    assert!(!model.accepts_selected_path(Path::new("/tmp/diagram.xml"), false, true, false));
    model
        .select(2)
        .expect("select explicitly unrestricted group");
    assert!(model.accepts_selected_path(Path::new("/tmp/diagram.svg"), false, true, false));

    let directory = std::env::temp_dir();
    assert!(directory.is_dir(), "OS temp directory exists");
    model.select(0).expect("select XML group");
    assert!(!model.accepts_selected_path(&directory, true, false, false));
    assert!(model.accepts_selected_path(&directory, true, false, true));
    assert!(model.accepts_selected_path(&directory, true, true, true));
    assert!(
        !model.accepts_selected_path(Path::new("/tmp/diagram.xml"), false, false, true),
        "directory-only admission must reject ordinary files despite matching suffix"
    );
}

#[test]
fn arbitrary_filter_groups_use_the_same_exact_extension_rule() {
    use std::path::Path;

    // These are deliberately unrelated to the original XML/SVG reproducer.
    // One generic source-driven rule must apply when a new application supplies
    // new groups, not only to the pinned draw.io fixture.
    let mut model = MacFileFilterModel::new(&[
        filter("Images", &["png", "jpeg"]),
        filter("Documents", &["pdf", "json"]),
        filter("Archives", &["tar.gz"]),
        filter("Everything", &["*"]),
    ])
    .expect("arbitrary application filter groups");

    let candidates = [
        ("photo.PNG", [true, false, false, true]),
        ("portrait.jpeg", [true, false, false, true]),
        ("invoice.pdf", [false, true, false, true]),
        ("package.JSON", [false, true, false, true]),
        // macOS first proof intentionally uses only the final compound suffix.
        ("bundle.tar.gz", [false, false, true, true]),
        ("bundle.part.gz", [false, false, true, true]),
        ("bundle.tar", [false, false, false, true]),
        ("diagram.svg", [false, false, false, true]),
        ("extensionless", [false, false, false, true]),
        (".png", [false, false, false, true]),
    ];

    for group in 0..4 {
        model.select(group).expect("valid group switch");
        for (filename, expected_by_group) in candidates {
            let selected = Path::new("/tmp").join(filename);
            assert_eq!(
                model.accepts_selected_path(&selected, false, true, false),
                expected_by_group[group],
                "group={group} selected={filename} must not inherit another group's types"
            );
        }
    }
}
