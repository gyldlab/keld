//! The argument check (KEL-53 §7 "17 (argument shape)") and the interim refusals
//! (KEL-53 §4 *Interim*), decided without an installation.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;

use keld_update::UpdaterHelperRole;

use super::{interim_refusal, parse_argument};
use crate::error::HelperError;

/// 64 lowercase hex digits, written out rather than derived from the endpoint owner.
const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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
fn any_argument_count_but_one_is_refused() {
    let one = rendezvous();
    assert_eq!(refused(args(&[])), "it takes exactly one argument, not 0");
    assert_eq!(
        refused(args(&[&one, &one])),
        "it takes exactly one argument, not 2"
    );
    assert_eq!(
        refused(args(&[&one, ""])),
        "it takes exactly one argument, not 2"
    );
}

#[test]
fn every_other_shape_is_refused_without_echoing_it() {
    let shape = "its argument is not an exact local keld-attempt rendezvous";
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
        "--recovery-role".to_owned(),
    ];
    for case in cases {
        let detail = refused(args(&[&case]));
        assert_eq!(detail, shape, "case {case:?}");
        if !case.is_empty() {
            assert!(
                !HelperError::Invocation { detail }
                    .to_string()
                    .contains(&case),
                "the refusal echoes {case:?}"
            );
        }
    }
}

#[test]
fn a_non_unicode_argument_is_refused() {
    // An unpaired UTF-16 surrogate has no Unicode form.
    let unpaired = OsString::from_wide(&[0x005C, 0xD800, 0x005C]);
    assert_eq!(refused(vec![unpaired]), "its argument is not valid Unicode");
}

#[test]
fn the_invocation_refusal_names_the_one_accepted_start() {
    let rendered = HelperError::Invocation {
        detail: "it takes exactly one argument, not 0".to_owned(),
    }
    .to_string();
    assert_eq!(
        rendered,
        "KELD-HELPER-001: keld-updater-helper.exe refused how it was started (it takes exactly \
         one argument, not 0). Only keld-host.exe starts it, on Windows, with exactly one \
         argument: the activation rendezvous `\\\\.\\pipe\\keld-attempt-<64 lowercase hex>`."
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
