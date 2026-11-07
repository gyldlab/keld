use super::ApplicationNameForUserAgent;

#[test]
fn application_name_for_user_agent_has_one_bounded_visible_ascii_contract() {
    for valid in [
        String::from("!"),
        String::from("FirstPartyA/1 FrameworkMetadata"),
        "~".repeat(256),
        String::from("internal  spaces"),
    ] {
        let value =
            ApplicationNameForUserAgent::new(valid.clone()).expect("visible bounded metadata");
        assert_eq!(value.as_str(), valid);
    }
    for invalid in [
        String::new(),
        String::from(" leading"),
        String::from("trailing "),
        String::from(" "),
        String::from("a\nb"),
        String::from("a\tb"),
        String::from("a\0b"),
        String::from("a\u{7f}b"),
        String::from("café"),
        "x".repeat(257),
    ] {
        assert!(ApplicationNameForUserAgent::new(invalid).is_err());
    }
    for byte in 0_u8..=255 {
        let text = format!("a{}b", char::from(byte));
        let result = ApplicationNameForUserAgent::new(text);
        assert_eq!(result.is_ok(), (b' '..=b'~').contains(&byte), "byte {byte}");
    }
}
