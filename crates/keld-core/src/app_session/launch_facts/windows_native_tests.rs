use super::{capture, first_language};
use crate::app_session::launch_facts::{paths, valid_text};

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

#[test]
fn first_actual_preference_is_preserved_without_normalization_or_fallback() {
    assert_eq!(
        first_language(&wide("fr-CA\0en-US\0\0"), 2).expect("ordered native list"),
        Some("fr-CA".to_owned()),
    );
    assert_eq!(
        first_language(&wide("zh-Hant-TW\0fr\0\0"), 2).expect("first exact bytes"),
        Some("zh-Hant-TW".to_owned()),
    );
    assert_eq!(first_language(&[0, 0], 0).expect("actual empty list"), None);
}

#[test]
fn malformed_native_multistrings_and_field_overflow_are_refused() {
    for (buffer, count) in [
        (Vec::new(), 0),
        (vec![0], 0),
        (vec![0, 0], 1),
        (wide("fr\0"), 1),
        (wide("fr\0\0"), 2),
        (wide("fr\0\0\0"), 1),
        (vec![0xd800, 0, 0], 1),
        (wide(&format!("{}\0\0", "λ".repeat(2049))), 1),
    ] {
        let error = first_language(&buffer, count).expect_err("bounded native-list refusal");
        assert_eq!(error.code(), "KELD-CORE-037");
    }
    assert_eq!(
        first_language(&wide(&format!("{}\0\0", "λ".repeat(2048))), 1)
            .expect("4096-byte first item")
            .expect("present item")
            .len(),
        4096,
    );
}

#[test]
fn native_folder_grammar_is_distinct_from_retained_stage_root() {
    for path in [
        r"D:\",
        r"d:\Users\λ\Roaming",
        r"\\?\D:\Users\Exact",
        r"\\server\share",
        r"\\server\share\Documents",
        r"\\?\UNC\server\share\Documents",
    ] {
        assert!(paths::observed_absolute(path), "{path}");
    }
    for path in [
        "",
        "/foreign",
        r"D:relative",
        r"\relative",
        r"\\server",
        r"\\.\device",
        r"\\?\GLOBALROOT\Device\Volume",
        r"D:\a\..\b",
        r"D:\a\\b",
        r"D:\a/b",
    ] {
        assert!(!paths::observed_absolute(path), "{path}");
    }
    assert!(!paths::retained_root(r"D:\Users\Exact"));
    assert!(!paths::retained_root(r"\\server\share"));
    assert!(paths::retained_root(r"\\?\D:\stage"));
}

#[test]
fn derived_user_data_reuses_guard_component_rules_and_keeps_other_facts() {
    for name in [
        "",
        ".",
        "..",
        "../escape",
        "a\\b",
        "trailing.",
        "trailing ",
        "NUL",
        "nul.txt",
        "CON",
        "COM1",
        "com¹.log",
        "LPT²",
        "AUX",
    ] {
        assert_eq!(
            paths::user_data(Some(r"D:\Actual\Roaming"), name),
            None,
            "{name}"
        );
    }
    assert_eq!(paths::user_data(None, "App"), None);
    assert_eq!(
        paths::user_data(Some(r"D:\Actual\Roaming"), "Exact λ"),
        Some("D:\\Actual\\Roaming\\Exact λ".to_owned()),
    );
    assert_eq!(
        paths::user_data(Some(r"D:\"), "Exact"),
        Some(r"D:\Exact".to_owned())
    );
}

#[test]
fn actual_windows_queries_return_bounded_observations_or_explicit_absence() {
    // Actual OS calls, not a locale/known-folder oracle for application acceptance.
    // No filesystem file/directory is opened or created by this test.
    let observed = capture().expect("native response follows bounded query contract");
    for path in [observed.app_data.as_deref(), observed.documents.as_deref()]
        .into_iter()
        .flatten()
    {
        assert!(valid_text(path));
        assert!(paths::observed_absolute(path));
    }
    if let Some(locale) = observed.locale.as_deref() {
        assert!(valid_text(locale));
    }
    println!(
        "KELD_WINDOWS_NATIVE_FACTS locale={} app_data={} documents={}",
        observed.locale.is_some(),
        observed.app_data.is_some(),
        observed.documents.is_some(),
    );
}
