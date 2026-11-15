//! The purpose-`1` `keld-attempt` locator (KEL-53 §4 "Candidate connect-back",
//! *Locator*; §7 "8 (keld-attempt codec)").
//!
//! Oracle: the spec's golden vector, written here as the literal full name,
//! and literal ID fills. No expected name is derived by hashing in the test.

use super::{
    ATTEMPT_ENDPOINT_PREFIX, WindowsAttemptLocatorError, WindowsAttemptLocatorInput,
    windows_attempt_connect_back_endpoint,
};
use crate::bootstrap::WindowsNamedPipeBootstrapStream;

const INSTALLATION: [u8; 32] = [0x11; 32];
const ATTEMPT: [u8; 32] = [0x22; 32];
const CHANNEL: [u8; 32] = [0x33; 32];

/// KEL-53 §4 *Locator*: installation ID `11`x32, attempt ID `22`x32 and
/// health-channel ID `33`x32 with purpose `1`.
const PURPOSE_ONE_GOLDEN: &str =
    r"\\.\pipe\keld-attempt-a56a565b56c571bd19b06b8e62845fa5a14c28fecd5611c94e4c90e8a1641ba3";

#[test]
fn purpose_one_reproduces_the_golden_vector_byte_for_byte() {
    let name = windows_attempt_connect_back_endpoint(&INSTALLATION, &ATTEMPT, &CHANNEL)
        .expect("distinct nonzero IDs derive a name");
    assert_eq!(name.as_bytes(), PURPOSE_ONE_GOLDEN.as_bytes());
    assert!(WindowsNamedPipeBootstrapStream::is_attempt_endpoint(&name));
}

#[test]
fn the_locator_and_the_shape_predicate_share_one_prefix() {
    assert_eq!(ATTEMPT_ENDPOINT_PREFIX, r"\\.\pipe\keld-attempt-");
    assert!(PURPOSE_ONE_GOLDEN.starts_with(ATTEMPT_ENDPOINT_PREFIX));
    // Any 64 lowercase hex digits after the shared prefix pass the predicate,
    // so a drift of either side's prefix fails here or in the golden test.
    let other = format!("{ATTEMPT_ENDPOINT_PREFIX}{}", "0".repeat(64));
    assert!(WindowsNamedPipeBootstrapStream::is_attempt_endpoint(&other));
}

#[test]
fn exchanging_attempt_and_channel_changes_the_name() {
    let exchanged = windows_attempt_connect_back_endpoint(&INSTALLATION, &CHANNEL, &ATTEMPT)
        .expect("distinct nonzero IDs derive a name");
    assert_ne!(exchanged, PURPOSE_ONE_GOLDEN);
    assert!(WindowsNamedPipeBootstrapStream::is_attempt_endpoint(
        &exchanged
    ));
}

#[test]
fn each_all_zero_input_is_refused() {
    use WindowsAttemptLocatorInput::{AttemptId, HealthChannelId, InstallationId};
    let zero = [0; 32];
    for (installation, attempt, channel, input) in [
        (&zero, &ATTEMPT, &CHANNEL, InstallationId),
        (&INSTALLATION, &zero, &CHANNEL, AttemptId),
        (&INSTALLATION, &ATTEMPT, &zero, HealthChannelId),
    ] {
        assert_eq!(
            windows_attempt_connect_back_endpoint(installation, attempt, channel),
            Err(WindowsAttemptLocatorError::ZeroInput { input }),
        );
    }
}

#[test]
fn each_pair_of_equal_inputs_is_refused() {
    use WindowsAttemptLocatorInput::{AttemptId, HealthChannelId, InstallationId};
    for (installation, attempt, channel, first, second) in [
        (
            &INSTALLATION,
            &INSTALLATION,
            &CHANNEL,
            InstallationId,
            AttemptId,
        ),
        (
            &INSTALLATION,
            &ATTEMPT,
            &INSTALLATION,
            InstallationId,
            HealthChannelId,
        ),
        (
            &INSTALLATION,
            &ATTEMPT,
            &ATTEMPT,
            AttemptId,
            HealthChannelId,
        ),
    ] {
        assert_eq!(
            windows_attempt_connect_back_endpoint(installation, attempt, channel),
            Err(WindowsAttemptLocatorError::EqualInputs { first, second }),
        );
    }
}

#[test]
fn every_locator_error_names_its_code_and_fix() {
    for error in [
        WindowsAttemptLocatorError::ZeroInput {
            input: WindowsAttemptLocatorInput::AttemptId,
        },
        WindowsAttemptLocatorError::EqualInputs {
            first: WindowsAttemptLocatorInput::InstallationId,
            second: WindowsAttemptLocatorInput::HealthChannelId,
        },
    ] {
        let text = error.to_string();
        assert!(text.starts_with("KELD-IPC-014: "), "{text}");
        assert!(text.contains("nonzero and no two equal"), "{text}");
    }
}
