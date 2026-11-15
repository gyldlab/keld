//! Exact Keld pipe-name shapes and their disjoint namespaces (KEL-53 §4
//! "Machine-UAC bootstrap" item 3; §7 "17 (argument shape)").
//!
//! The oracle is the literal name table below, never a name derived by the
//! production locator or predicate.

use std::io;
use std::time::{Duration, Instant};

use crate::windows_named_pipe::WindowsNamedPipeServer;

use super::{
    WindowsLifecycleBinding, WindowsLifecycleExpectation, WindowsLifecyclePurpose,
    WindowsNamedPipeBootstrapStream,
};

const LOCATOR: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

type NamespacePredicate = fn(&str) -> bool;

fn attempt_name(locator: &str) -> String {
    format!(r"\\.\pipe\keld-attempt-{locator}")
}

#[test]
fn attempt_endpoint_accepts_only_the_exact_local_shape() {
    assert!(WindowsNamedPipeBootstrapStream::is_attempt_endpoint(
        &attempt_name(LOCATOR)
    ));
    assert!(WindowsNamedPipeBootstrapStream::is_attempt_endpoint(
        &attempt_name(&"f".repeat(64))
    ));

    // Same-length locators with one wrong digit: each is exactly 64 bytes, so
    // the length rule admits it and only the digit rule can refuse it. U+FF10
    // (FULLWIDTH DIGIT ZERO) is three UTF-8 bytes.
    let upper_one = format!("{}A", &LOCATOR[..63]);
    let non_hex = format!("{}g", &LOCATOR[..63]);
    let full_width_digit = format!("{}\u{ff10}", &LOCATOR[..61]);
    for locator in [&upper_one, &non_hex, &full_width_digit] {
        assert_eq!(locator.len(), 64, "{locator:?}");
    }
    let mut refused = vec![
        // UNC, remote, device-path and slash variants of the same locator.
        format!(r"\\server\pipe\keld-attempt-{LOCATOR}"),
        format!(r"\\127.0.0.1\pipe\keld-attempt-{LOCATOR}"),
        format!(r"\\localhost\pipe\keld-attempt-{LOCATOR}"),
        format!(r"\\?\pipe\keld-attempt-{LOCATOR}"),
        format!(r"\\?\UNC\server\pipe\keld-attempt-{LOCATOR}"),
        format!(r"\??\pipe\keld-attempt-{LOCATOR}"),
        format!(r"//./pipe/keld-attempt-{LOCATOR}"),
        format!(r"\\.\PIPE\keld-attempt-{LOCATOR}"),
        format!(r"\\.\pipe\KELD-attempt-{LOCATOR}"),
        format!(r"\\.\pipe\..\pipe\keld-attempt-{LOCATOR}"),
        format!(r"keld-attempt-{LOCATOR}"),
        // Every other keld-* namespace, and near-miss namespaces.
        format!(r"\\.\pipe\keld-{LOCATOR}"),
        format!(r"\\.\pipe\keld-lifecycle-{LOCATOR}"),
        format!(r"\\.\pipe\keld-attempts-{LOCATOR}"),
        format!(r"\\.\pipe\keld-attempt{LOCATOR}"),
        format!(r"\\.\pipe\keld-attempt--{LOCATOR}"),
        format!(r"\\.\pipe\keld-bootstrap-{LOCATOR}"),
        // Uppercase hex, whole or one digit.
        attempt_name(&LOCATOR.to_ascii_uppercase()),
        attempt_name(&upper_one),
        // Wrong length.
        attempt_name(""),
        attempt_name(&LOCATOR[..63]),
        attempt_name(&format!("{LOCATOR}0")),
        attempt_name(&LOCATOR[..32]),
        // Any extra suffix, prefix or non-hex digit.
        format!(r"{}\", attempt_name(LOCATOR)),
        format!("{}.", attempt_name(LOCATOR)),
        format!("{}:stream", attempt_name(LOCATOR)),
        format!("{}\0", attempt_name(LOCATOR)),
        format!("{} ", attempt_name(LOCATOR)),
        format!(" {}", attempt_name(LOCATOR)),
        format!("{} --role recovery", attempt_name(LOCATOR)),
        attempt_name(&non_hex),
        attempt_name(&full_width_digit),
    ];
    refused.push(String::new());
    for name in &refused {
        assert!(
            !WindowsNamedPipeBootstrapStream::is_attempt_endpoint(name),
            "attempt endpoint predicate admitted {name:?}"
        );
    }
}

#[test]
fn keld_pipe_namespaces_are_pairwise_disjoint() {
    let app_link = format!(r"\\.\pipe\keld-{LOCATOR}");
    let lifecycle = format!(r"\\.\pipe\keld-lifecycle-{LOCATOR}");
    let attempt = attempt_name(LOCATOR);
    let predicates: [(&str, NamespacePredicate); 3] = [
        (
            "app-link",
            WindowsNamedPipeBootstrapStream::is_keld_endpoint,
        ),
        (
            "lifecycle",
            WindowsNamedPipeBootstrapStream::is_lifecycle_endpoint,
        ),
        (
            "attempt",
            WindowsNamedPipeBootstrapStream::is_attempt_endpoint,
        ),
    ];
    for (row, name) in [&app_link, &lifecycle, &attempt].into_iter().enumerate() {
        for (column, (namespace, predicate)) in predicates.iter().enumerate() {
            assert_eq!(
                predicate(name),
                row == column,
                "{namespace} predicate on {name:?}"
            );
        }
    }
}

/// A live pipe sits at the attempt name, so a client that opened it would
/// succeed: an `InvalidInput` refusal proves the namespace check ran first.
#[test]
fn app_link_and_lifecycle_clients_refuse_a_live_attempt_name_before_opening() -> io::Result<()> {
    let endpoint = attempt_name(&super::random_test_locator()?);
    let live = WindowsNamedPipeServer::bind(&endpoint)?;

    let app_link = WindowsNamedPipeBootstrapStream::connect(&endpoint)
        .expect_err("the app-link client must refuse the attempt namespace");
    assert_eq!(app_link.kind(), io::ErrorKind::InvalidInput);

    let binding = WindowsLifecycleBinding::new(
        [0x11; 32],
        [0x22; 32],
        [0x33; 32],
        WindowsLifecyclePurpose::CoordinatorToKeeper,
    )?;
    let lifecycle = crate::connect_windows_lifecycle_rendezvous_until(
        &endpoint,
        WindowsLifecycleExpectation::exact(binding),
        Instant::now() + Duration::from_secs(2),
        |_, _| None::<NeverPin>,
    )
    .expect_err("the lifecycle client must refuse the attempt namespace");
    assert_eq!(lifecycle.kind(), io::ErrorKind::InvalidInput);

    // Positive control: the same live instance is openable by a direct open.
    let opened = WindowsNamedPipeServer::connect_client_until(
        &endpoint,
        Instant::now() + Duration::from_secs(2),
    )?;
    drop(opened);
    drop(live);
    Ok(())
}

/// Server pin that no test path constructs: the lifecycle client must refuse
/// the namespace before it ever asks for one.
#[derive(Debug)]
struct NeverPin;

impl crate::WindowsLifecyclePeerPin for NeverPin {
    fn process_id(&self) -> u32 {
        0
    }

    fn session_id(&self) -> u32 {
        0
    }

    fn has_exited(&self) -> io::Result<bool> {
        Ok(true)
    }
}
