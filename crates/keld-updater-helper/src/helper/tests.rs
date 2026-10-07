//! The argument check (KEL-53 §7 "17 (argument shape)") and the interim refusals
//! (KEL-53 §4 *Interim*), decided without an installation.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;

use keld_runtime::windows_job::WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR;
use keld_update::UpdaterHelperRole;

use super::{interim_refusal, parse_argument};
use crate::error::HelperError;

/// 64 lowercase hex digits, written out rather than derived from the endpoint owner.
const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The recovery-role selector as KEL-53 §4 spells it, written out rather than imported:
/// a cross-version argument contract between a host and an older tree's helper.
const SELECTOR: &str = "--recovery-role";

fn rendezvous() -> String {
    format!(r"\\.\pipe\keld-attempt-{HEX}")
}

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

/// The argument check's refusal detail; any other outcome fails the case.
fn refused(values: Vec<OsString>) -> String {
    match parse_argument(values) {
        Err(HelperError::Invocation { detail }) => detail,
        other => panic!("expected the KELD-HELPER-001 refusal, got {other:?}"),
    }
}

#[test]
fn the_exact_local_rendezvous_selects_the_activation_role() {
    let role = parse_argument(args(&[&rendezvous()])).expect("the exact rendezvous shape");
    assert_eq!(role, UpdaterHelperRole::Activation);
}

#[test]
fn the_exact_selector_selects_the_recovery_role() {
    assert_eq!(
        WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR, SELECTOR,
        "keld-runtime's selector is the KEL-53 literal"
    );
    let role = parse_argument(args(&[SELECTOR])).expect("the exact selector");
    assert_eq!(role, UpdaterHelperRole::Recovery);
}

#[test]
fn any_argument_count_but_one_is_refused() {
    let one = rendezvous();
    assert_eq!(refused(args(&[])), "it takes exactly one argument, not 0");
    for extra in [
        args(&[&one, &one]),
        args(&[&one, ""]),
        args(&[SELECTOR, SELECTOR]),
        args(&[SELECTOR, &one]),
        args(&[&one, SELECTOR]),
        args(&[SELECTOR, ""]),
    ] {
        assert_eq!(
            refused(extra.clone()),
            "it takes exactly one argument, not 2",
            "{extra:?}"
        );
    }
}

/// Every refusal of a wrong shape renders the same text, so none echoes its argument.
#[test]
fn every_other_shape_is_refused_without_echoing_it() {
    let shape = "its argument is neither an exact local keld-attempt rendezvous nor \
                 `--recovery-role`";
    let rendered = HelperError::Invocation {
        detail: shape.to_owned(),
    }
    .to_string();
    let upper = HEX.to_ascii_uppercase();
    let cases = [
        String::new(),
        format!(r"\\server\pipe\keld-attempt-{HEX}"),
        format!(r"\\?\pipe\keld-attempt-{HEX}"),
        format!(r"\\.\PIPE\keld-attempt-{HEX}"),
        format!(r"\\.\pipe\keld-lifecycle-{HEX}"),
        format!(r"\\.\pipe\keld-other-{HEX}"),
        format!(r"\\.\pipe\keld-attempt-{upper}"),
        format!(r"\\.\pipe\keld-attempt-{}", &HEX[..63]),
        format!(r"\\.\pipe\keld-attempt-{HEX}0"),
        format!(r"\\.\pipe\keld-attempt-{HEX} "),
        format!(r" \\.\pipe\keld-attempt-{HEX}"),
        format!(r"\\.\pipe\keld-attempt-{HEX}\x"),
        "--RECOVERY-ROLE".to_owned(),
        "--Recovery-Role".to_owned(),
        "--recovery-role ".to_owned(),
        " --recovery-role".to_owned(),
        "-recovery-role".to_owned(),
        "recovery-role".to_owned(),
        "--recovery_role".to_owned(),
        "--recovery-role=1".to_owned(),
        "--recovery-role\0".to_owned(),
        "/recovery-role".to_owned(),
    ];
    for case in cases {
        let detail = refused(args(&[&case]));
        assert_eq!(detail, shape, "case {case:?}");
        assert_eq!(
            HelperError::Invocation { detail }.to_string(),
            rendered,
            "case {case:?}"
        );
    }
}

#[test]
fn a_non_unicode_argument_is_refused() {
    // An unpaired UTF-16 surrogate has no Unicode form.
    let unpaired = OsString::from_wide(&[0x005C, 0xD800, 0x005C]);
    assert_eq!(refused(vec![unpaired]), "its argument is not valid Unicode");
}

#[test]
fn the_invocation_refusal_names_the_two_accepted_starts() {
    let rendered = HelperError::Invocation {
        detail: "it takes exactly one argument, not 0".to_owned(),
    }
    .to_string();
    assert_eq!(
        rendered,
        "KELD-HELPER-001: keld-updater-helper.exe refused how it was started (it takes exactly \
         one argument, not 0). It takes exactly one argument: keld-host.exe passes the \
         activation rendezvous `\\\\.\\pipe\\keld-attempt-<64 lowercase hex>`, and keld-host.exe \
         or an administrator passes `--recovery-role`."
    );
}

#[test]
fn each_role_refuses_with_its_own_pinned_interim_text() {
    assert_eq!(
        interim_refusal(UpdaterHelperRole::Activation).to_string(),
        "KELD-HELPER-003: the updater helper's activation role is not available in this \
         release: it anchored itself and then refused, before the writer lease and any write. \
         Nothing changed; keep running the current version."
    );
    assert_eq!(
        interim_refusal(UpdaterHelperRole::Recovery).to_string(),
        "KELD-HELPER-004: the updater helper's recovery-only role is disabled in this release \
         (RecoveryDisabled): it anchored itself and then refused, before the writer lease and \
         any write. No supported resolution exists other than administrator action; any \
         activation journal, the pointers and the versions are preserved."
    );
}
