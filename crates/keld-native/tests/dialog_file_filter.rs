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
