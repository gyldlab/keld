//! Fixed error metadata; malformed attacker strings never become diagnostics.
use super::super::WvLinkRejectReason;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DecodeError {
    Malformed,
    Storage,
}
impl DecodeError {
    pub(super) const fn reason(self) -> WvLinkRejectReason {
        match self {
            Self::Malformed | Self::Storage => WvLinkRejectReason::MalformedEnvelope,
        }
    }
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "malformed renderer envelope",
            Self::Storage => "renderer storage admission refused",
        })
    }
}
impl std::error::Error for DecodeError {}

impl serde::de::Error for DecodeError {
    fn custom<T: fmt::Display>(_message: T) -> Self {
        // Diagnostics may contain attacker-controlled JSON fields. Overflow
        // comes only from the shared bounded payload's typed parse state.
        Self::Malformed
    }
}
