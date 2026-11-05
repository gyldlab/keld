//! Closed, deduplicated GH478/GH480 system grants share one owner.
use keld_guard::{Decision, DenyReason, Principal, evaluate, json_pointer_for, parse_manifest};

#[test]
fn menu_and_dialog_system_grants_can_coexist_in_either_order() {
    for text in [
        r#"{"app":{"system":["dialog","menu"]}}"#,
        r#"{"app":{"system":["menu","dialog"]}}"#,
    ] {
        let manifest = parse_manifest(text).expect("valid first-proof system manifest");
        for (operation, resource) in [("system.dialog", "dialog"), ("system.menu", "menu")] {
            assert!(
                matches!(
                    evaluate(&manifest, Principal::AppProcess, operation, resource),
                    Decision::Allow(_)
                ),
                "{text} must independently grant {operation}"
            );
        }
    }
}

#[test]
fn one_literal_never_grants_the_other_system_operation() {
    for (literal, own_operation, other_operation, other_resource) in [
        ("dialog", "system.dialog", "system.menu", "menu"),
        ("menu", "system.menu", "system.dialog", "dialog"),
    ] {
        let manifest = parse_manifest(&format!(r#"{{"app":{{"system":["{literal}"]}}}}"#))
            .expect("valid first-proof system manifest");
        assert!(matches!(
            evaluate(&manifest, Principal::AppProcess, own_operation, literal),
            Decision::Allow(_)
        ));
        assert!(matches!(
            evaluate(
                &manifest,
                Principal::AppProcess,
                other_operation,
                other_resource
            ),
            Decision::Deny(_)
        ));
        assert!(matches!(
            evaluate(
                &manifest,
                Principal::AppProcess,
                own_operation,
                other_resource
            ),
            Decision::Deny(_)
        ));
    }
}

#[test]
fn malformed_or_unknown_system_entries_deny_both_operations() {
    for text in [
        r"{}",
        r#"{"app":{"system":[]}}"#,
        r#"{"app":{"system":"menu"}}"#,
        r#"{"app":{"system":["dialog","dialog"]}}"#,
        r#"{"app":{"system":["menu","menu"]}}"#,
        r#"{"app":{"system":["dialog","menu","menu"]}}"#,
        r#"{"app":{"system":["dialog","menu","other"]}}"#,
        r#"{"app":{"system":["dialog","menu","*"]}}"#,
        r#"{"app":{"system":["dialog","menu",1]}}"#,
        r#"{"app":{"system":["Dialog","menu"]}}"#,
        r#"{"app":{"system":["dialog","system.menu"]}}"#,
    ] {
        let manifest = parse_manifest(text).expect("valid JSON, not necessarily an admitted grant");
        for (operation, resource) in [("system.dialog", "dialog"), ("system.menu", "menu")] {
            assert!(
                matches!(
                    evaluate(&manifest, Principal::AppProcess, operation, resource),
                    Decision::Deny(_)
                ),
                "{text} must not salvage {operation}"
            );
        }
    }
}

#[test]
fn system_grants_stay_principal_first_and_use_one_fix_pointer() {
    let manifest = parse_manifest(r#"{"app":{"system":["dialog","menu"]}}"#).expect("manifest");
    for principal in [
        Principal::Webview {
            id: 7,
            generation: 1,
        },
        Principal::Plugin { id: 3 },
    ] {
        for (operation, resource) in [("system.dialog", "dialog"), ("system.menu", "menu")] {
            assert!(matches!(
                evaluate(&manifest, principal, operation, resource),
                Decision::Deny(DenyReason::NotAppProcess { .. })
            ));
        }
    }
    assert_eq!(json_pointer_for("system.dialog"), "/app/system");
    assert_eq!(json_pointer_for("system.menu"), "/app/system");
}
