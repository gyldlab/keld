//! `keld-attempt` endpoint locator (KEL-53 §4 "Candidate connect-back",
//! *Locator*; approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06).
//!
//! The name is the namespace prefix followed by the 32-byte BLAKE3 digest of
//! `UTF8("keld.attempt-endpoint/v1\0") || purpose || installation_id || a || b`
//! as 64 lowercase hexadecimal digits, each byte in digest order with its high
//! nibble first. `purpose` is one byte and every other input is exactly 32
//! bytes, so no input needs a length prefix. The name conveys no authority; it
//! only lets the claimant check that the IDs it is offered derive the name it
//! was launched with. Purpose `2` (bootstrap) lands with KEL-53 §6 S11.

use std::fmt;

/// The `keld-attempt` namespace prefix: the locator's and
/// `WindowsNamedPipeBootstrapStream::is_attempt_endpoint`'s one constant, so
/// the names the locator derives and the names the predicate admits cannot
/// drift apart.
pub(crate) const ATTEMPT_ENDPOINT_PREFIX: &str = r"\\.\pipe\keld-attempt-";

/// The NUL-terminated BLAKE3 domain, in the `keld.<name>/v1` style of the
/// landed domain-separated derivations.
const LOCATOR_DOMAIN: &[u8] = b"keld.attempt-endpoint/v1\0";

/// The closed set of attempt-endpoint purposes, numbered from `1`. It is its
/// own type: `WindowsLifecyclePurpose` is never reused for these (KEL-53
/// criterion 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum AttemptEndpointPurpose {
    /// Candidate connect-back: `a` is the attempt ID and `b` the
    /// health-channel ID.
    ConnectBack = 1,
}

/// One input of the `keld-attempt` locator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsAttemptLocatorInput {
    /// The provenance-derived installation ID.
    InstallationId,
    /// The attempt ID that `keld-update` minted.
    AttemptId,
    /// The health-channel ID that `keld-update` minted.
    HealthChannelId,
}

impl WindowsAttemptLocatorInput {
    const fn describe(self) -> &'static str {
        match self {
            Self::InstallationId => "installation ID",
            Self::AttemptId => "attempt ID",
            Self::HealthChannelId => "health-channel ID",
        }
    }
}

/// Typed refusal of the `keld-attempt` locator, before it hashes anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsAttemptLocatorError {
    /// `KELD-IPC-014`: an input is 32 zero bytes.
    ZeroInput {
        /// The all-zero input.
        input: WindowsAttemptLocatorInput,
    },
    /// `KELD-IPC-014`: two inputs are equal.
    EqualInputs {
        /// The earlier of the two equal inputs, in locator input order.
        first: WindowsAttemptLocatorInput,
        /// The later of the two equal inputs, in locator input order.
        second: WindowsAttemptLocatorInput,
    },
}

impl fmt::Display for WindowsAttemptLocatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KELD-IPC-014: keld-attempt locator input refused (")?;
        match self {
            Self::ZeroInput { input } => write!(f, "the {} is all zero", input.describe())?,
            Self::EqualInputs { first, second } => write!(
                f,
                "the {} equals the {}",
                first.describe(),
                second.describe()
            )?,
        }
        f.write_str(
            "). Pass the provenance-derived installation ID and the attempt and \
             health-channel IDs that keld-update minted: each nonzero and no two equal.",
        )
    }
}

impl std::error::Error for WindowsAttemptLocatorError {}

/// Derives the purpose-`1` (candidate connect-back) endpoint name from the
/// provenance-derived installation ID and the minted attempt and
/// health-channel IDs: `\\.\pipe\keld-attempt-<64 lowercase hex>`.
///
/// The owner names the endpoint it creates with it
/// ([`WindowsAttemptEndpoint::create_connect_back`](super::WindowsAttemptEndpoint::create_connect_back)),
/// and the claimant requires it to yield its own rendezvous name before it
/// sends `KELD-AA1` ([`WindowsAttemptClient::claim`](super::WindowsAttemptClient::claim)).
///
/// # Errors
///
/// [`WindowsAttemptLocatorError::ZeroInput`] for an all-zero input and
/// [`WindowsAttemptLocatorError::EqualInputs`] for two equal inputs, as the
/// lifecycle binding refuses them.
pub fn windows_attempt_connect_back_endpoint(
    installation_id: &[u8; 32],
    attempt_id: &[u8; 32],
    health_channel_id: &[u8; 32],
) -> Result<String, WindowsAttemptLocatorError> {
    use WindowsAttemptLocatorInput::{AttemptId, HealthChannelId, InstallationId};
    for (input, value) in [
        (InstallationId, installation_id),
        (AttemptId, attempt_id),
        (HealthChannelId, health_channel_id),
    ] {
        if *value == [0; 32] {
            return Err(WindowsAttemptLocatorError::ZeroInput { input });
        }
    }
    for (first, left, second, right) in [
        (InstallationId, installation_id, AttemptId, attempt_id),
        (
            InstallationId,
            installation_id,
            HealthChannelId,
            health_channel_id,
        ),
        (AttemptId, attempt_id, HealthChannelId, health_channel_id),
    ] {
        if left == right {
            return Err(WindowsAttemptLocatorError::EqualInputs { first, second });
        }
    }
    Ok(attempt_endpoint(
        AttemptEndpointPurpose::ConnectBack,
        installation_id,
        attempt_id,
        health_channel_id,
    ))
}

/// The one encoder: prefix, domain, purpose, input order, hash and rendering.
/// `blake3::Hash::to_hex` renders each digest byte in order, high nibble
/// first, in lowercase, as the landed lifecycle locator does.
fn attempt_endpoint(
    purpose: AttemptEndpointPurpose,
    installation_id: &[u8; 32],
    a: &[u8; 32],
    b: &[u8; 32],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(LOCATOR_DOMAIN);
    hasher.update(&[purpose as u8]);
    hasher.update(installation_id);
    hasher.update(a);
    hasher.update(b);
    let digest = hasher.finalize().to_hex();
    let mut endpoint = String::with_capacity(ATTEMPT_ENDPOINT_PREFIX.len() + digest.len());
    endpoint.push_str(ATTEMPT_ENDPOINT_PREFIX);
    endpoint.push_str(&digest);
    endpoint
}

#[cfg(test)]
mod tests;
