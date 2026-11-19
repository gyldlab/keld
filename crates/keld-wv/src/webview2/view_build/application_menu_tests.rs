//! Independent modeled Win32 observations for the production candidate decision.

use super::check_managed_application_menu;
use crate::engine::BuildStage;

#[test]
fn invalid_managed_hwnd_refuses_without_querying_its_menu() {
    let Err(failure) = check_managed_application_menu(|| false, || panic!("invalid menu query"))
    else {
        panic!("invalid managed window was accepted");
    };
    assert_eq!(failure.stage, BuildStage::Window);
    assert!(failure.source.to_string().contains("invalid HWND"));
}

#[test]
fn a_present_managed_application_menu_refuses_the_owned_build() {
    let Err(failure) = check_managed_application_menu(|| true, || true) else {
        panic!("managed window with an application menu was accepted");
    };
    assert_eq!(failure.stage, BuildStage::Window);
    assert!(
        failure
            .source
            .to_string()
            .contains("native application menu")
    );
}

#[test]
fn valid_managed_hwnd_without_application_menu_proceeds() {
    let valid_checked = std::cell::Cell::new(false);
    assert!(
        check_managed_application_menu(
            || {
                valid_checked.set(true);
                true
            },
            || {
                assert!(valid_checked.get());
                false
            },
        )
        .is_ok()
    );
}
