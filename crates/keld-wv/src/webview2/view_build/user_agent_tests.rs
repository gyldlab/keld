use std::cell::RefCell;

use super::apply_application_name_for_user_agent;
use crate::{ApplicationNameForUserAgent, WvError};

#[test]
fn original_metadata_preserves_native_prefix_and_requires_exact_readback() {
    let metadata = ApplicationNameForUserAgent::new("KeldQualification/1 Test".to_owned())
        .expect("bounded authored metadata");
    let native = "Mozilla/5.0 Existing/1.2  ";
    let expected = "Mozilla/5.0 Existing/1.2   KeldQualification/1 Test";
    let mut readbacks = [native, expected].into_iter();
    let calls = RefCell::new(Vec::new());
    apply_application_name_for_user_agent(
        &metadata,
        |limit| {
            calls.borrow_mut().push(limit);
            Ok(readbacks.next().expect("exactly two reads").to_owned())
        },
        |configured| {
            calls.borrow_mut().push(0);
            assert_eq!(configured, expected);
            Ok(())
        },
    )
    .expect("readback retained the exact setting");
    assert_eq!(calls.into_inner(), [4096, 0, 4121]);

    let error = apply_application_name_for_user_agent(
        &metadata,
        |_| Ok(native.to_owned()),
        |configured| {
            assert_eq!(configured, expected);
            Ok(())
        },
    )
    .expect_err("successful setter with ignored UA must refuse the view");
    assert!(error.to_string().contains("did not retain"));
}

#[test]
fn original_metadata_rejects_empty_or_overbound_native_prefix_without_setter() {
    let metadata = ApplicationNameForUserAgent::new("KeldQualification/1".to_owned())
        .expect("bounded authored metadata");
    for invalid in [String::new(), "x".repeat(4097), "\u{1f680}".repeat(2049)] {
        let error = apply_application_name_for_user_agent(
            &metadata,
            |_| Ok(invalid.clone()),
            |_| panic!("invalid native prefix reached the setter"),
        )
        .expect_err("invalid capture must refuse the view");
        assert!(error.to_string().contains("empty or exceeds"));
    }
}

#[test]
fn original_metadata_preserves_maximum_utf16_prefix_and_literal_suffix() {
    let suffix = "L".repeat(256);
    let metadata = ApplicationNameForUserAgent::new(suffix.clone())
        .expect("maximum bounded authored metadata");
    let native = "\u{1f680}".repeat(2048);
    let expected = format!("{native} {suffix}");
    let mut readbacks = [(4096, native), (4353, expected.clone())].into_iter();
    apply_application_name_for_user_agent(
        &metadata,
        |limit| {
            let (observed_bound, value) = readbacks.next().expect("one read per bound");
            assert_eq!(limit, observed_bound);
            Ok(value)
        },
        |configured| {
            assert_eq!(configured, expected);
            assert_eq!(configured.encode_utf16().count(), 4353);
            Ok(())
        },
    )
    .expect("the full native prefix survives at the bound");
}

#[test]
fn original_metadata_propagates_native_read_and_setter_failures() {
    let metadata = ApplicationNameForUserAgent::new("KeldQualification/1".to_owned())
        .expect("bounded authored metadata");
    let error = apply_application_name_for_user_agent(
        &metadata,
        |_| Err(WvError::Webview(String::from("fixture native UA getter"))),
        |_| panic!("refused getter reached the setter"),
    )
    .expect_err("refused getter must refuse the view");
    assert!(error.to_string().contains("fixture native UA getter"));

    let error = apply_application_name_for_user_agent(
        &metadata,
        |limit| {
            assert_eq!(limit, 4096, "refused setter must stop before readback");
            Ok(String::from("Mozilla/5.0"))
        },
        |_| Err(WvError::Webview(String::from("fixture refused UA setter"))),
    )
    .expect_err("refused setter must refuse the view");
    assert!(error.to_string().contains("fixture refused UA setter"));
}
